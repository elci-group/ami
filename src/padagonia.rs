use crate::types::{
    GateResult, GateStatus, Project, Prospect, SearchPhrase, TransformationCandidate,
    TransformationDistance,
};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct PadagoniaClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct Provenance {
    agent: &'static str,
    model: &'static str,
    confidence: f32,
    cost: f32,
    evidence: Vec<String>,
}

impl PadagoniaClient {
    pub fn from_env() -> Result<Self> {
        let base_url = env::var("PADAGONIA_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:7373".to_string())
            .trim_end_matches('/')
            .to_string();
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .context("failed to create Padagonia client")?,
            base_url,
            api_key: env::var("PADAGONIA_API_KEY").ok(),
        })
    }

    pub async fn record_analysis(
        &self,
        project: &Project,
        phrases: &[SearchPhrase],
        prospects: &[Prospect],
    ) -> Result<usize> {
        let project_id = self
            .create_node(
                "AmiProject",
                json!({
                    "name": project.name,
                    "description": project.description,
                    "repository_url": project.repository_url,
                    "capabilities": project.capabilities.iter().map(|c| &c.name).collect::<Vec<_>>(),
                }),
                1.0,
                vec!["ami:project-reader".to_string()],
            )
            .await?;
        let mut recorded = 1;
        for phrase in phrases {
            let phrase_id = self
                .create_node(
                    "AmiSearchPhrase",
                    json!({"phrase": phrase.phrase, "weight": phrase.weight}),
                    phrase.weight,
                    vec!["ami:search-generator".to_string()],
                )
                .await?;
            self.create_edge(project_id, phrase_id, "generated_search_phrase")
                .await?;
            recorded += 1;
        }
        for prospect in prospects {
            let prospect_id = self
                .create_node(
                    "AmiProspect",
                    json!({
                        "name": prospect.name,
                        "url": prospect.url,
                        "platform": prospect.platform.to_string(),
                        "confidence": prospect.confidence,
                        "audience_size": prospect.audience_size,
                    }),
                    prospect.confidence,
                    vec![format!("{}:{}", prospect.platform, prospect.url)],
                )
                .await?;
            self.create_edge(project_id, prospect_id, "potential_adopter")
                .await?;
            recorded += 1;
        }
        Ok(recorded)
    }

    /// Record a Transformation Proposal (directive sections 13-14): the
    /// candidate, its measured distance, and every gate outcome, linked
    /// together for later provenance queries.
    pub async fn record_transformation_proposal(
        &self,
        candidate: &TransformationCandidate,
        distance: &TransformationDistance,
        gates: &[GateResult],
    ) -> Result<usize> {
        let candidate_id = self
            .create_node(
                "AmiTransformationCandidate",
                json!({
                    "id": candidate.id,
                    "description": candidate.description,
                    "desired_outcome": candidate.desired_outcome,
                    "required_capability": candidate.required_capability,
                    "capability_status": candidate.capability_status.as_str(),
                    "evidence_confidence": format!("{:?}", candidate.evidence_confidence),
                    "market_evidence": candidate.market_evidence,
                    "component_distance": distance.component_distance,
                    "architecture_distance": distance.architecture_distance,
                    "capability_distance": distance.capability_distance,
                    "product_identity_distance": distance.product_identity_distance,
                    "overall_distance": distance.overall(),
                    "band": distance.band(),
                }),
                1.0,
                candidate.market_evidence.clone(),
            )
            .await?;
        let mut recorded = 1;
        for gate in gates {
            let gate_id = self
                .create_node(
                    "AmiGateResult",
                    json!({
                        "gate": gate.gate,
                        "status": match gate.status {
                            GateStatus::Pass => "pass",
                            GateStatus::Fail => "fail",
                            GateStatus::NotEvaluated => "not_evaluated",
                        },
                        "rationale": gate.rationale,
                    }),
                    match gate.status {
                        GateStatus::Pass => 1.0,
                        GateStatus::Fail => 1.0,
                        GateStatus::NotEvaluated => 0.0,
                    },
                    vec![format!("ami:gate:{}", unix_timestamp())],
                )
                .await?;
            self.create_edge(candidate_id, gate_id, "evaluated_by")
                .await?;
            recorded += 1;
        }
        Ok(recorded)
    }

    async fn create_node(
        &self,
        label: &str,
        properties: Value,
        confidence: f32,
        evidence: Vec<String>,
    ) -> Result<u64> {
        let response = self
            .request(self.client.post(format!("{}/api/v1/nodes", self.base_url)))
            .json(&json!({
                "label": label,
                "properties": properties,
                "provenance": Provenance {
                    agent: "ami",
                    model: "deterministic+groq",
                    confidence,
                    cost: 0.0,
                    evidence,
                }
            }))
            .send()
            .await
            .context("Padagonia node request failed")?;
        let response = response
            .error_for_status()
            .context("Padagonia rejected node")?;
        response
            .json::<Value>()
            .await?
            .get("id")
            .and_then(Value::as_u64)
            .context("Padagonia node response had no id")
    }

    async fn create_edge(&self, src: u64, dst: u64, label: &str) -> Result<()> {
        self.request(self.client.post(format!("{}/api/v1/edges", self.base_url)))
            .json(&json!({
                "src": src,
                "dst": dst,
                "label": label,
                "provenance": Provenance {
                    agent: "ami",
                    model: "deterministic+groq",
                    confidence: 1.0,
                    cost: 0.0,
                    evidence: vec![format!("ami:research:{}", unix_timestamp())],
                }
            }))
            .send()
            .await
            .context("Padagonia edge request failed")?
            .error_for_status()
            .context("Padagonia rejected edge")?;
        Ok(())
    }

    fn request(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
