use crate::types::{Project, SearchPhrase};
use anyhow::{anyhow, Result};
use reqwest::{redirect::Policy, Client};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    time::{Duration, Instant},
};

const DEFAULT_MODEL: &str = "openai/gpt-oss-20b";
const MAX_RESPONSE_BYTES: usize = 1_048_576;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Openrouter,
    Groq,
    Platform,
}

impl Provider {
    fn key_env(self) -> &'static str {
        match self {
            Self::Openrouter => "OPENROUTER_API_KEY",
            Self::Groq => "GROQ_API_KEY",
            Self::Platform => "REPO_ANALYSIS_WORKER_TOKEN",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Openrouter => "OpenRouter",
            Self::Groq => "Groq",
            Self::Platform => "Dreamsequence",
        }
    }
    fn endpoint(self) -> Result<String> {
        match self {
            Self::Openrouter => Ok("https://openrouter.ai/api/v1/chat/completions".into()),
            Self::Groq => Ok("https://api.groq.com/openai/v1/chat/completions".into()),
            Self::Platform => {
                let base = env::var("AMI_PLATFORM_API_URL")
                    .map_err(|_| anyhow!("AMI_PLATFORM_API_URL is required"))?;
                let url = reqwest::Url::parse(&base)
                    .map_err(|_| anyhow!("invalid platform API origin"))?;
                if url.scheme() != "https"
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    return Err(anyhow!(
                        "platform API origin must be HTTPS without embedded credentials"
                    ));
                }
                Ok(format!(
                    "{}/internal/tools/inference",
                    base.trim_end_matches('/')
                ))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteConfig {
    pub provider: Provider,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "yes")]
    pub supports_temperature: bool,
}
fn default_model() -> String {
    DEFAULT_MODEL.into()
}
fn yes() -> bool {
    true
}
fn default_timeout() -> u64 {
    45
}
fn default_max_tokens() -> u32 {
    1000
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingConfig {
    pub routes: Vec<RouteConfig>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}
impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            routes: vec![
                RouteConfig {
                    provider: Provider::Openrouter,
                    model: DEFAULT_MODEL.into(),
                    supports_temperature: true,
                },
                RouteConfig {
                    provider: Provider::Groq,
                    model: DEFAULT_MODEL.into(),
                    supports_temperature: true,
                },
            ],
            timeout_seconds: 45,
            max_tokens: 1000,
        }
    }
}
impl RoutingConfig {
    pub fn parse(text: &str) -> Result<Self> {
        // TOML parse errors can include the source line: never echo configuration values.
        let config: Self =
            toml::from_str(text).map_err(|_| anyhow!("invalid AMI inference configuration"))?;
        config.validate()?;
        Ok(config)
    }
    fn validate(&self) -> Result<()> {
        if self.routes.is_empty()
            || self.routes.len() > 3
            || !(1..=60).contains(&self.timeout_seconds)
            || !(64..=4000).contains(&self.max_tokens)
        {
            return Err(anyhow!(
                "AMI routing requires 1–3 routes, a 1–60 second timeout, and 64–4000 output tokens"
            ));
        }
        for route in &self.routes {
            if route.model.is_empty()
                || route.model.len() > 200
                || !route
                    .model
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/-_.:".contains(c))
            {
                return Err(anyhow!("invalid inference model identifier"));
            }
        }
        Ok(())
    }
}

struct Route {
    config: RouteConfig,
    api_key: String,
    endpoint: String,
}

/// Operator-defined routing; credentials exist only in the process environment.
/// Project repositories cannot implicitly load routing or choose credential recipients.
pub struct RoutedInference {
    client: Client,
    routes: Vec<Route>,
    max_tokens: u32,
}
/// Backward-compatible name. Default routing now prefers OpenRouter.
pub type GroqInference = RoutedInference;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferenceAttribution {
    pub requested_model: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub gateway: Option<String>,
    pub attribution_status: String,
    pub cached: bool,
    pub total_tokens: u64,
}
#[derive(Serialize)]
struct Event<'a> {
    status: &'a str,
    latency_ms: u128,
    #[serde(flatten)]
    attribution: &'a InferenceAttribution,
}

#[derive(Debug, Serialize)]
struct Message {
    role: &'static str,
    content: String,
}
#[derive(Deserialize)]
struct InferenceOutput {
    search_phrases: Vec<String>,
}
#[derive(Default, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    model: Option<String>,
    provider: Option<String>,
    gateway: Option<String>,
    id: Option<String>,
    content: Option<String>,
    cached: Option<bool>,
    attribution_status: Option<String>,
    total_tokens: Option<u64>,
    #[serde(default)]
    usage: Usage,
}
#[derive(Default, Deserialize)]
struct Usage {
    #[serde(default)]
    total_tokens: u64,
}
#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}
#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

fn safe_label(value: Option<String>, limit: usize) -> Option<String> {
    value
        .map(|s| {
            s.chars()
                .filter(|c| !c.is_control())
                .take(limit)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
}

impl RoutedInference {
    pub fn from_env() -> Result<Option<Self>> {
        let mut config = match env::var("AMI_INFERENCE_CONFIG") {
            Ok(path) => RoutingConfig::parse(
                &fs::read_to_string(path)
                    .map_err(|_| anyhow!("could not read AMI inference configuration"))?,
            )?,
            Err(_) => {
                let mut config = RoutingConfig::default();
                if let Ok(model) = env::var("OPENROUTER_MODEL") {
                    config.routes[0].model = model;
                }
                if let Ok(model) = env::var("GROQ_MODEL") {
                    config.routes[1].model = model;
                }
                config
            }
        };
        config.validate()?;
        let mut routes = Vec::new();
        for route in config.routes.drain(..) {
            if let Ok(api_key) = env::var(route.provider.key_env()) {
                if !api_key.trim().is_empty() {
                    routes.push(Route {
                        endpoint: route.provider.endpoint()?,
                        config: route,
                        api_key,
                    });
                }
            }
        }
        if routes.is_empty() {
            return Ok(None);
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| anyhow!("could not create inference transport"))?;
        Ok(Some(Self {
            client,
            routes,
            max_tokens: config.max_tokens,
        }))
    }

    pub async fn infer_search_phrases(
        &self,
        project: &Project,
        max_phrases: usize,
    ) -> Result<Vec<SearchPhrase>> {
        if max_phrases == 0 {
            return Ok(Vec::new());
        }
        let max_phrases = max_phrases.min(50);
        let messages=vec![
            Message { role:"system", content:"You identify plausible adopter search phrases from project metadata. Return only JSON in the form {\"search_phrases\":[\"phrase\"]}. Do not invent named companies, users, or facts. Keep phrases concise and distinct.".into() },
            Message { role:"user", content:format!("Project: {}\nDescription: {}\nKeywords: {}\nCapabilities: {}\nSuggest up to {} search phrases.", project.name,project.description,project.keywords.join(", "),project.capabilities.iter().map(|c|c.name.as_str()).collect::<Vec<_>>().join(", "),max_phrases) },
        ];
        for route in &self.routes {
            let start = Instant::now();
            match self.call(route, &messages).await {
                Ok(completion) => {
                    let attribution = self.attribution(route, &completion).await;
                    let content = completion.content.as_deref().or_else(|| {
                        completion
                            .choices
                            .first()
                            .and_then(|choice| choice.message.content.as_deref())
                    });
                    let parsed = content
                        .and_then(|content| serde_json::from_str::<InferenceOutput>(content).ok());
                    let status = if parsed.is_some() {
                        if attribution.cached {
                            "cached"
                        } else {
                            "completed"
                        }
                    } else {
                        "invalid_output"
                    };
                    println!(
                        "AMI_INFERENCE_EVENT {}",
                        serde_json::to_string(&Event {
                            status,
                            latency_ms: start.elapsed().as_millis(),
                            attribution: &attribution
                        })?
                    );
                    if let Some(output) = parsed {
                        let mut seen = std::collections::HashSet::new();
                        let phrases = output
                            .search_phrases
                            .into_iter()
                            .map(|p| p.trim().to_string())
                            .filter(|p| {
                                !p.is_empty() && p.len() <= 200 && seen.insert(p.to_lowercase())
                            })
                            .take(max_phrases)
                            .map(|phrase| SearchPhrase {
                                phrase,
                                source_capabilities: vec![format!(
                                    "{}-inference",
                                    route.config.provider.label().to_lowercase()
                                )],
                                weight: 0.75,
                            })
                            .collect::<Vec<_>>();
                        if !phrases.is_empty() {
                            return Ok(phrases);
                        }
                    }
                }
                Err(_) => {
                    let attribution = InferenceAttribution {
                        requested_model: route.config.model.clone(),
                        model: None,
                        provider: None,
                        gateway: match route.config.provider {
                            Provider::Openrouter => Some("OpenRouter".into()),
                            Provider::Platform => Some("Dreamsequence".into()),
                            Provider::Groq => None,
                        },
                        attribution_status: "unavailable".into(),
                        cached: false,
                        total_tokens: 0,
                    };
                    // No provider body, credentials or project metadata enter errors or events.
                    println!(
                        "AMI_INFERENCE_EVENT {}",
                        serde_json::to_string(&Event {
                            status: "failed",
                            latency_ms: start.elapsed().as_millis(),
                            attribution: &attribution
                        })?
                    );
                }
            }
        }
        Err(anyhow!("all configured inference routes are unavailable"))
    }

    async fn call(&self, route: &Route, messages: &[Message]) -> Result<ChatResponse> {
        let request = if route.config.provider == Provider::Platform {
            let id = env::var("AMI_PLATFORM_JOB_ID")
                .ok()
                .and_then(|id| id.parse::<u64>().ok())
                .filter(|id| *id > 0)
                .ok_or_else(|| anyhow!("a live platform job is required"))?;
            let claim = env::var("AMI_PLATFORM_CLAIM")
                .map_err(|_| anyhow!("a platform worker claim is required"))?;
            let kind =
                env::var("AMI_PLATFORM_JOB_KIND").unwrap_or_else(|_| "concept_analysis".into());
            serde_json::json!({"job_id":id,"claimed_by":claim,"job_kind":kind,"contract":"ami-search-phrases-v1","messages":messages})
        } else {
            let mut request = serde_json::json!({"model":route.config.model,"messages":messages,"max_tokens":self.max_tokens,"response_format":{"type":"json_object"}});
            if route.config.supports_temperature {
                request["temperature"] = serde_json::json!(0.2);
            }
            if route.config.provider == Provider::Openrouter {
                request["provider"] = serde_json::json!({"allow_fallbacks":true,"require_parameters":true,"sort":"price"});
            }
            request
        };
        let response = self
            .client
            .post(&route.endpoint)
            .bearer_auth(&route.api_key)
            .header(
                "User-Agent",
                "ami-market-intelligence/0.2 (+https://github.com/elci-group/ami)",
            )
            .json(&request)
            .send()
            .await
            .map_err(|_| anyhow!("inference transport unavailable"))?;
        if !response.status().is_success() {
            return Err(anyhow!("inference provider rejected the request"));
        }
        bounded_json(response).await
    }

    async fn attribution(&self, route: &Route, completion: &ChatResponse) -> InferenceAttribution {
        let mut model = safe_label(completion.model.clone(), 200);
        let mut provider = match route.config.provider {
            Provider::Groq => Some("Groq".into()),
            _ => safe_label(completion.provider.clone(), 128),
        };
        let gateway = match route.config.provider {
            Provider::Openrouter => Some("OpenRouter".into()),
            Provider::Platform => safe_label(completion.gateway.clone(), 128),
            Provider::Groq => None,
        };
        if route.config.provider == Provider::Openrouter && provider.is_none() {
            if let Some(id) = completion.id.as_deref().filter(|id| {
                id.len() <= 200
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
            }) {
                if let Ok(response) = self
                    .client
                    .get("https://openrouter.ai/api/v1/generation")
                    .query(&[("id", id)])
                    .bearer_auth(&route.api_key)
                    .timeout(Duration::from_secs(3))
                    .send()
                    .await
                {
                    if response.status().is_success() {
                        if let Ok(data) = bounded_json::<serde_json::Value>(response).await {
                            provider = safe_label(
                                data["data"]["provider_name"].as_str().map(str::to_string),
                                128,
                            );
                            model =
                                safe_label(data["data"]["model"].as_str().map(str::to_string), 200)
                                    .or(model);
                        }
                    }
                }
            }
        }
        let status = if model.is_some() && provider.is_some() {
            "resolved"
        } else {
            "pending"
        };
        InferenceAttribution {
            requested_model: route.config.model.clone(),
            model,
            provider,
            gateway,
            attribution_status: completion
                .attribution_status
                .clone()
                .unwrap_or_else(|| status.into()),
            cached: completion.cached.unwrap_or(false),
            total_tokens: completion
                .total_tokens
                .unwrap_or(completion.usage.total_tokens),
        }
    }
}

async fn bounded_json<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow!("incomplete inference response"))?
    {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(anyhow!("inference response exceeds size limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow!("invalid inference response"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_prefer_openrouter_and_keep_groq() {
        let config = RoutingConfig::default();
        assert_eq!(config.routes[0].provider, Provider::Openrouter);
        assert_eq!(config.routes[1].provider, Provider::Groq);
        config.validate().unwrap();
    }
    #[test]
    fn configuration_controls_order_and_model() {
        let config=RoutingConfig::parse("[[routes]]\nprovider='groq'\nmodel='llama-3.3-70b-versatile'\n[[routes]]\nprovider='openrouter'\nmodel='moonshotai/kimi-k3'\nsupports_temperature=false").unwrap();
        assert_eq!(config.routes[0].provider, Provider::Groq);
        assert_eq!(config.routes[1].model, "moonshotai/kimi-k3");
        assert!(!config.routes[1].supports_temperature);
    }
    #[test]
    fn invalid_config_does_not_echo_secrets() {
        let error = RoutingConfig::parse(
            "[[routes]]\nprovider='openrouter'\napi_key='synthetic-sensitive-value'",
        )
        .unwrap_err()
        .to_string();
        assert!(!error.contains("synthetic-sensitive-value"));
        assert!(RoutingConfig::parse("routes=[]").is_err());
        assert!(RoutingConfig::parse("[[routes]]\nprovider='unknown'").is_err());
    }
    #[test]
    fn platform_config_uses_only_the_bridge() {
        let config =
            RoutingConfig::parse(include_str!("../examples/platform-inference.toml")).unwrap();
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.routes[0].provider, Provider::Platform);
        assert_eq!(config.timeout_seconds, 60);
    }
    #[tokio::test]
    async fn failed_openrouter_falls_back_to_groq_and_retains_actual_model() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/chat/completions", listener.local_addr().unwrap());
        let thread = thread::spawn(move || {
            let mut bodies = Vec::new();
            for i in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let n = stream.read(&mut buffer).unwrap();
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..pos]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|s| s.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= pos + 4 + length {
                            bodies.push(
                                serde_json::from_slice::<serde_json::Value>(
                                    &bytes[pos + 4..pos + 4 + length],
                                )
                                .unwrap(),
                            );
                            break;
                        }
                    }
                }
                let body = if i == 0 {
                    "{\"error\":{\"message\":\"must-not-be-logged\"}}".to_string()
                } else {
                    serde_json::json!({"model":"actual-groq-model","choices":[{"message":{"content":"{\"search_phrases\":[\"developer tools\",\"Developer tools\"]}"}}],"usage":{"total_tokens":42}}).to_string()
                };
                let status = if i == 0 {
                    "402 Payment Required"
                } else {
                    "200 OK"
                };
                write!(stream,"HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            bodies
        });
        let config = RoutingConfig::default();
        let routes = config
            .routes
            .into_iter()
            .map(|mut config| {
                config.supports_temperature = false;
                Route {
                    config,
                    api_key: "synthetic".into(),
                    endpoint: endpoint.clone(),
                }
            })
            .collect();
        let inference = RoutedInference {
            client: Client::builder().no_proxy().build().unwrap(),
            routes,
            max_tokens: 1000,
        };
        let project = Project {
            name: "fixture".into(),
            description: "Fixture".into(),
            repository_url: None,
            website_url: None,
            capabilities: vec![],
            keywords: vec![],
            development_stage: crate::types::DevelopmentStage::Prototype,
            last_analyzed: chrono::Utc::now(),
            commit_hash: None,
        };
        let phrases = inference.infer_search_phrases(&project, 20).await.unwrap();
        assert_eq!(phrases.len(), 1);
        let bodies = thread.join().unwrap();
        assert!(bodies.iter().all(|body| body.get("temperature").is_none()));
        assert!(bodies[0].get("provider").is_some());
        assert!(bodies[1].get("provider").is_none());
    }
}
