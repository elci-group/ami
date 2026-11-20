// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT

use crate::types::{Platform, Prospect, SearchPhrase};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;

fn discovery_client() -> Client {
    Client::builder()
        .user_agent("ami-market-intelligence/0.1 (+https://github.com/elci-group/ami)")
        .build()
        .expect("discovery HTTP client configuration is valid") // traci: allow: fails only on TLS backend init, never from a bare user agent
}

#[async_trait]
pub trait DiscoverySourceTrait: Send + Sync {
    /// Search for prospects using a search phrase
    async fn search(&self, phrase: &SearchPhrase) -> Result<Vec<Prospect>>;

    /// Get the platform this source targets
    fn platform(&self) -> Platform;

    /// Check if this source is enabled
    fn is_enabled(&self) -> bool;

    /// Get rate limit in milliseconds
    fn rate_limit_ms(&self) -> Option<u64>;
}

pub struct DiscoveryEngine {
    sources: Vec<Box<dyn DiscoverySourceTrait>>,
}

impl DiscoveryEngine {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    pub fn add_source(mut self, source: Box<dyn DiscoverySourceTrait>) -> Self {
        self.sources.push(source);
        self
    }

    pub async fn discover(&self, phrases: &[SearchPhrase]) -> Result<Vec<Prospect>> {
        let mut all_prospects = Vec::new();

        for source in &self.sources {
            if !source.is_enabled() {
                continue;
            }

            for phrase in phrases {
                match source.search(phrase).await {
                    Ok(prospects) => {
                        all_prospects.extend(prospects);
                    }
                    Err(e) => {
                        eprintln!("Error searching with {}: {}", source.platform(), e);
                    }
                }

                // Respect rate limits
                if let Some(delay) = source.rate_limit_ms() {
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                }
            }
        }

        Ok(all_prospects)
    }
}

impl Default for DiscoveryEngine {
    fn default() -> Self {
        Self::new()
    }
}

// GitHub discovery source
pub struct GitHubDiscovery {
    enabled: bool,
    rate_limit_ms: u64,
    client: Client,
}

impl GitHubDiscovery {
    pub fn new(enabled: bool, rate_limit_ms: u64) -> Self {
        Self {
            enabled,
            rate_limit_ms,
            client: discovery_client(),
        }
    }

    async fn search_repositories(&self, phrase: &str) -> Result<Vec<Prospect>> {
        let url = format!(
            "https://api.github.com/search/repositories?q={}&sort=stars&order=desc&per_page=10",
            urlencoding::encode(phrase)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "ami-market-intelligence")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let json: serde_json::Value = response.json().await?;
        let items = json.get("items").and_then(|v| v.as_array());

        let mut prospects = Vec::new();

        if let Some(items) = items {
            for item in items {
                if let Some(prospect) = self.parse_repository(item) {
                    prospects.push(prospect);
                }
            }
        }

        Ok(prospects)
    }

    fn parse_repository(&self, item: &serde_json::Value) -> Option<Prospect> {
        let name = item.get("name")?.as_str()?.to_string();
        let url = item.get("html_url")?.as_str()?.to_string();
        let description = item
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        let stars = item.get("stargazers_count").and_then(|c| c.as_u64());

        let owner = item.get("owner")?;
        let owner_url = owner.get("html_url")?.as_str()?.to_string();

        Some(Prospect {
            name,
            prospect_type: crate::types::ProspectType::Repository,
            platform: Platform::GitHub,
            url,
            reason_matched: vec![description],
            confidence: 0.7,
            audience_size: stars,
            contact_available: true,
            contact_info: vec![crate::types::ContactChannel {
                channel_type: crate::types::ContactType::GitHub,
                value: owner_url,
                verified: true,
            }],
            previous_contact: None,
            discovered_at: Utc::now(),
            last_updated: Utc::now(),
        })
    }
}

#[async_trait]
impl DiscoverySourceTrait for GitHubDiscovery {
    async fn search(&self, phrase: &SearchPhrase) -> Result<Vec<Prospect>> {
        self.search_repositories(&phrase.phrase).await
    }

    fn platform(&self) -> Platform {
        Platform::GitHub
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn rate_limit_ms(&self) -> Option<u64> {
        Some(self.rate_limit_ms)
    }
}

// Reddit public JSON discovery source
pub struct RedditDiscovery {
    enabled: bool,
    rate_limit_ms: u64,
    client: Client,
}

impl RedditDiscovery {
    pub fn new(enabled: bool, rate_limit_ms: u64) -> Self {
        Self {
            enabled,
            rate_limit_ms,
            client: discovery_client(),
        }
    }
}

#[async_trait]
impl DiscoverySourceTrait for RedditDiscovery {
    async fn search(&self, phrase: &SearchPhrase) -> Result<Vec<Prospect>> {
        let url = format!(
            "https://www.reddit.com/search.json?q={}&limit=10&sort=relevance",
            urlencoding::encode(&phrase.phrase)
        );
        let response: serde_json::Value = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response
            .pointer("/data/children")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|child| {
                let data = child.get("data")?;
                let name = data.get("title")?.as_str()?.to_string();
                let permalink = data.get("permalink")?.as_str()?;
                let score = data.get("score").and_then(|value| value.as_u64());
                Some(Prospect {
                    name,
                    prospect_type: crate::types::ProspectType::Community,
                    platform: Platform::Reddit,
                    url: format!("https://www.reddit.com{}", permalink),
                    reason_matched: vec![phrase.phrase.clone()],
                    confidence: 0.55,
                    audience_size: score,
                    contact_available: true,
                    contact_info: vec![],
                    previous_contact: None,
                    discovered_at: Utc::now(),
                    last_updated: Utc::now(),
                })
            })
            .collect())
    }

    fn platform(&self) -> Platform {
        Platform::Reddit
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn rate_limit_ms(&self) -> Option<u64> {
        Some(self.rate_limit_ms)
    }
}

// Hacker News Algolia discovery source
pub struct HackerNewsDiscovery {
    enabled: bool,
    rate_limit_ms: u64,
    client: Client,
}

impl HackerNewsDiscovery {
    pub fn new(enabled: bool, rate_limit_ms: u64) -> Self {
        Self {
            enabled,
            rate_limit_ms,
            client: discovery_client(),
        }
    }
}

#[async_trait]
impl DiscoverySourceTrait for HackerNewsDiscovery {
    async fn search(&self, phrase: &SearchPhrase) -> Result<Vec<Prospect>> {
        let url = format!(
            "https://hn.algolia.com/api/v1/search?query={}&tags=story&hitsPerPage=10",
            urlencoding::encode(&phrase.phrase)
        );
        let response: serde_json::Value = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response
            .get("hits")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|hit| {
                let name = hit.get("title")?.as_str()?.to_string();
                let object_id = hit.get("objectID")?.as_str()?;
                let url = hit
                    .get("url")
                    .and_then(|value| value.as_str())
                    .unwrap_or("https://news.ycombinator.com/item?id=");
                let url = if url.ends_with("id=") {
                    format!("{url}{object_id}")
                } else {
                    url.to_string()
                };
                Some(Prospect {
                    name,
                    prospect_type: crate::types::ProspectType::Publication,
                    platform: Platform::HackerNews,
                    url,
                    reason_matched: vec![phrase.phrase.clone()],
                    confidence: 0.55,
                    audience_size: hit.get("points").and_then(|value| value.as_u64()),
                    contact_available: true,
                    contact_info: vec![],
                    previous_contact: None,
                    discovered_at: Utc::now(),
                    last_updated: Utc::now(),
                })
            })
            .collect())
    }

    fn platform(&self) -> Platform {
        Platform::HackerNews
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn rate_limit_ms(&self) -> Option<u64> {
        Some(self.rate_limit_ms)
    }
}

/// Dev.to article discovery source.
pub struct DevToDiscovery {
    enabled: bool,
    rate_limit_ms: u64,
    client: Client,
}

impl DevToDiscovery {
    pub fn new(enabled: bool, rate_limit_ms: u64) -> Self {
        Self {
            enabled,
            rate_limit_ms,
            client: discovery_client(),
        }
    }
}

#[async_trait]
impl DiscoverySourceTrait for DevToDiscovery {
    async fn search(&self, phrase: &SearchPhrase) -> Result<Vec<Prospect>> {
        let url = format!(
            "https://dev.to/api/articles?per_page=10&search={}",
            urlencoding::encode(&phrase.phrase)
        );
        let articles: Vec<serde_json::Value> = self
            .client
            .get(url)
            .header("Accept", "application/vnd.forem.api-v1+json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(articles
            .into_iter()
            .filter_map(|article| {
                Some(Prospect {
                    name: article.get("title")?.as_str()?.to_string(),
                    prospect_type: crate::types::ProspectType::Publication,
                    platform: Platform::DevTo,
                    url: article.get("url")?.as_str()?.to_string(),
                    reason_matched: vec![phrase.phrase.clone()],
                    confidence: 0.5,
                    audience_size: article
                        .get("positive_reactions_count")
                        .and_then(|value| value.as_u64()),
                    contact_available: true,
                    contact_info: vec![],
                    previous_contact: None,
                    discovered_at: Utc::now(),
                    last_updated: Utc::now(),
                })
            })
            .collect())
    }
    fn platform(&self) -> Platform {
        Platform::DevTo
    }
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn rate_limit_ms(&self) -> Option<u64> {
        Some(self.rate_limit_ms)
    }
}

/// Stack Exchange API discovery source, focused on Stack Overflow questions.
pub struct StackOverflowDiscovery {
    enabled: bool,
    rate_limit_ms: u64,
    client: Client,
}

impl StackOverflowDiscovery {
    pub fn new(enabled: bool, rate_limit_ms: u64) -> Self {
        Self {
            enabled,
            rate_limit_ms,
            client: discovery_client(),
        }
    }
}

#[async_trait]
impl DiscoverySourceTrait for StackOverflowDiscovery {
    async fn search(&self, phrase: &SearchPhrase) -> Result<Vec<Prospect>> {
        let url = format!("https://api.stackexchange.com/2.3/search/advanced?order=desc&sort=relevance&q={}&site=stackoverflow&pagesize=10", urlencoding::encode(&phrase.phrase));
        let response: serde_json::Value = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response
            .get("items")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| {
                Some(Prospect {
                    name: item.get("title")?.as_str()?.to_string(),
                    prospect_type: crate::types::ProspectType::Community,
                    platform: Platform::StackOverflow,
                    url: item.get("link")?.as_str()?.to_string(),
                    reason_matched: vec![phrase.phrase.clone()],
                    confidence: 0.55,
                    audience_size: item.get("score").and_then(|value| value.as_u64()),
                    contact_available: true,
                    contact_info: vec![],
                    previous_contact: None,
                    discovered_at: Utc::now(),
                    last_updated: Utc::now(),
                })
            })
            .collect())
    }
    fn platform(&self) -> Platform {
        Platform::StackOverflow
    }
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn rate_limit_ms(&self) -> Option<u64> {
        Some(self.rate_limit_ms)
    }
}
