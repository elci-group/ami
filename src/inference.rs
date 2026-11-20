// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT

use crate::types::{Project, SearchPhrase};
use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const DEFAULT_MODEL: &str = "llama-3.3-70b-versatile";

/// Optional market-hypothesis inference backed by Groq.
///
/// Ami remains usable without a key: deterministic search generation is always
/// available, while this client adds model-generated hypotheses when configured.
pub struct GroqInference {
    client: Client,
    api_key: String,
    model: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    response_format: ResponseFormat,
}

#[derive(Debug, Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: &'static str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InferenceOutput {
    search_phrases: Vec<String>,
}

impl GroqInference {
    /// Build a client from GROQ_API_KEY and optional GROQ_MODEL.
    pub fn from_env() -> Result<Option<Self>> {
        let Some(api_key) = env::var("GROQ_API_KEY")
            .ok() // traci: allow: absent env var means Groq is disabled, not an error
            .filter(|key| !key.trim().is_empty())
        else {
            return Ok(None);
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .context("failed to create Groq HTTP client")?;

        Ok(Some(Self {
            client,
            api_key,
            model: env::var("GROQ_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
        }))
    }

    /// Ask Groq for additional market search hypotheses.
    pub async fn infer_search_phrases(
        &self,
        project: &Project,
        max_phrases: usize,
    ) -> Result<Vec<SearchPhrase>> {
        if max_phrases == 0 {
            return Ok(Vec::new());
        }

        let request = ChatRequest {
            model: self.model.clone(),
            temperature: 0.2,
            response_format: ResponseFormat { format_type: "json_object" },
            messages: vec![
                Message {
                    role: "system",
                    content: "You identify plausible adopter search phrases from project metadata. Return only JSON in the form {\"search_phrases\":[\"phrase\"]}. Do not invent named companies, users, or facts. Keep phrases concise and distinct.".to_string(),
                },
                Message {
                    role: "user",
                    content: format!(
                        "Project: {}\nDescription: {}\nKeywords: {}\nCapabilities: {}\nSuggest up to {} search phrases.",
                        project.name,
                        project.description,
                        project.keywords.join(", "),
                        project.capabilities.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "),
                        max_phrases,
                    ),
                },
            ],
        };

        let response = self
            .client
            .post(GROQ_API_URL)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .context("Groq inference request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            eprintln!("ami: Groq inference request failed with status {status}");
            return Err(anyhow!("Groq inference returned {}: {}", status, body));
        }

        let completion: ChatResponse = response
            .json()
            .await
            .context("invalid response from Groq inference")?;
        let content = completion
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .ok_or_else(|| anyhow!("Groq inference returned no message content"))?;
        let output: InferenceOutput = serde_json::from_str(content)
            .context("Groq inference did not return the expected JSON shape")?;

        Ok(output
            .search_phrases
            .into_iter()
            .map(|phrase| phrase.trim().to_string())
            .filter(|phrase| !phrase.is_empty())
            .take(max_phrases)
            .map(|phrase| SearchPhrase {
                phrase,
                source_capabilities: vec!["groq-inference".to_string()],
                weight: 0.75,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_key_disables_optional_inference() {
        let previous = env::var("GROQ_API_KEY").ok();
        env::remove_var("GROQ_API_KEY");
        assert!(GroqInference::from_env().unwrap().is_none());
        if let Some(value) = previous {
            env::set_var("GROQ_API_KEY", value);
        }
    }
}
