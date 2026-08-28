//! Gate 5 evidence gathering (directive section 8): observed/stated market
//! demand for a capability, sourced from the project's own GitHub issues —
//! currently the only demand channel Ami can verify. An empty result means
//! no evidence was found, never "assume interest" (directive section 17's
//! "market hallucination" anti-pattern).

use crate::types::EvidenceConfidence;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct DemandEvidence {
    pub confidence: EvidenceConfidence,
    /// "issue title (url)" for every open issue whose text matched a
    /// keyword — the citation trail a reader can go verify themselves.
    pub citations: Vec<String>,
}

#[derive(Deserialize)]
struct Issue {
    title: String,
    body: Option<String>,
    html_url: String,
    /// Present (non-null) when GitHub's issues endpoint is actually
    /// returning a pull request; those aren't demand signals.
    #[serde(default)]
    pull_request: Option<Value>,
}

/// Search the project's own open GitHub issues for keyword overlap with a
/// candidate's required capability. Requires `repository_url` to be a
/// github.com URL; anything else is treated as "no evidence source
/// available", not an error.
pub async fn from_github_issues(
    repository_url: Option<&str>,
    keywords: &[String],
) -> Result<DemandEvidence> {
    let none = DemandEvidence {
        confidence: EvidenceConfidence::None,
        citations: Vec::new(),
    };

    let Some(repository_url) = repository_url else {
        return Ok(none);
    };
    let Some((owner, repo)) = parse_github_owner_repo(repository_url) else {
        return Ok(none);
    };

    let keywords: Vec<String> = keywords
        .iter()
        .map(|keyword| keyword.to_lowercase())
        .filter(|keyword| keyword.len() >= 3)
        .collect();
    if keywords.is_empty() {
        return Ok(none);
    }

    let client = Client::builder()
        .user_agent("ami-market-intelligence/0.1 (+https://github.com/elci-group/ami)")
        .build()
        .context("failed to build GitHub client")?;

    let url = format!("https://api.github.com/repos/{owner}/{repo}/issues?state=open&per_page=100");
    let response = client
        .get(&url)
        .send()
        .await
        .context("GitHub issues request failed")?
        .error_for_status()
        .context("GitHub rejected the issues request")?;
    let issues: Vec<Issue> = response
        .json()
        .await
        .context("failed to parse GitHub issues response")?;

    let citations: Vec<String> = issues
        .into_iter()
        .filter(|issue| issue.pull_request.is_none())
        .filter_map(|issue| {
            let haystack = format!(
                "{} {}",
                issue.title,
                issue.body.as_deref().unwrap_or_default()
            )
            .to_lowercase();
            keywords
                .iter()
                .any(|keyword| haystack.contains(keyword.as_str()))
                .then(|| format!("{} ({})", issue.title, issue.html_url))
        })
        .collect();

    if citations.is_empty() {
        Ok(none)
    } else {
        Ok(DemandEvidence {
            confidence: EvidenceConfidence::Stated,
            citations,
        })
    }
}

fn parse_github_owner_repo(repository_url: &str) -> Option<(String, String)> {
    let trimmed = repository_url
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let after_host = trimmed.split("github.com").nth(1)?;
    let after_host = after_host.trim_start_matches(['/', ':']);
    let mut parts = after_host.splitn(2, '/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() {
        None
    } else {
        Some((owner, repo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_owner_and_repo_from_https_url() {
        assert_eq!(
            parse_github_owner_repo("https://github.com/sal/ami"),
            Some(("sal".to_string(), "ami".to_string()))
        );
        assert_eq!(
            parse_github_owner_repo("https://github.com/sal/ami.git"),
            Some(("sal".to_string(), "ami".to_string()))
        );
        assert_eq!(
            parse_github_owner_repo("https://github.com/sal/ami/"),
            Some(("sal".to_string(), "ami".to_string()))
        );
    }

    #[test]
    fn rejects_non_github_urls() {
        assert_eq!(parse_github_owner_repo("https://gitlab.com/sal/ami"), None);
        assert_eq!(parse_github_owner_repo("not a url"), None);
    }

    #[tokio::test]
    async fn no_repository_url_yields_no_evidence_not_an_error() {
        let evidence = from_github_issues(None, &["export".to_string()])
            .await
            .unwrap();
        assert_eq!(evidence.confidence, EvidenceConfidence::None);
        assert!(evidence.citations.is_empty());
    }

    #[tokio::test]
    async fn empty_keywords_yield_no_evidence() {
        let evidence = from_github_issues(Some("https://github.com/sal/ami"), &[])
            .await
            .unwrap();
        assert_eq!(evidence.confidence, EvidenceConfidence::None);
    }
}
