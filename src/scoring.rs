// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT

use crate::types::{Project, Prospect, ScoringWeights};

pub struct ScoringModel {
    weights: ScoringWeights,
}

impl ScoringModel {
    pub fn new(weights: ScoringWeights) -> Self {
        Self { weights }
    }

    pub fn with_default_weights() -> Self {
        Self::new(ScoringWeights::default())
    }

    /// Score a prospect against a project
    pub fn score_prospect(&self, project: &Project, prospect: &mut Prospect) {
        let keyword_score = self.score_keyword_overlap(project, prospect);
        let technology_score = self.score_technology_overlap(project, prospect);
        let domain_score = self.score_domain_overlap(project, prospect);
        let activity_score = self.score_repository_activity(prospect);
        let audience_score = self.score_audience_relevance(prospect);
        let prior_score = self.score_prior_interactions(prospect);

        let total_score = (keyword_score * self.weights.keyword_overlap
            + technology_score * self.weights.technology_overlap
            + domain_score * self.weights.domain_overlap
            + activity_score * self.weights.repository_activity
            + audience_score * self.weights.audience_relevance
            + prior_score * self.weights.prior_interactions)
            / (self.weights.keyword_overlap
                + self.weights.technology_overlap
                + self.weights.domain_overlap
                + self.weights.repository_activity
                + self.weights.audience_relevance
                + self.weights.prior_interactions);

        prospect.confidence = total_score;
        prospect.last_updated = chrono::Utc::now();
    }

    /// Score based on keyword overlap
    fn score_keyword_overlap(&self, project: &Project, prospect: &Prospect) -> f32 {
        let project_keywords: Vec<String> =
            project.keywords.iter().map(|k| k.to_lowercase()).collect();

        let prospect_text =
            format!("{} {}", prospect.name, prospect.reason_matched.join(" ")).to_lowercase();

        let matches = project_keywords
            .iter()
            .filter(|k| prospect_text.contains(k.as_str()))
            .count();

        if project_keywords.is_empty() {
            0.0
        } else {
            (matches as f32) / (project_keywords.len() as f32)
        }
    }

    /// Score based on technology overlap
    fn score_technology_overlap(&self, project: &Project, prospect: &Prospect) -> f32 {
        let project_tech: Vec<String> = project
            .capabilities
            .iter()
            .filter(|c| {
                matches!(
                    c.category,
                    crate::types::CapabilityCategory::Technology
                        | crate::types::CapabilityCategory::Language
                        | crate::types::CapabilityCategory::Framework
                )
            })
            .map(|c| c.name.to_lowercase())
            .collect();

        let prospect_text =
            format!("{} {}", prospect.name, prospect.reason_matched.join(" ")).to_lowercase();

        let matches = project_tech
            .iter()
            .filter(|t| prospect_text.contains(t.as_str()))
            .count();

        if project_tech.is_empty() {
            0.0
        } else {
            (matches as f32) / (project_tech.len() as f32)
        }
    }

    /// Score based on domain overlap
    fn score_domain_overlap(&self, project: &Project, prospect: &Prospect) -> f32 {
        let project_domains: Vec<String> = project
            .capabilities
            .iter()
            .filter(|c| matches!(c.category, crate::types::CapabilityCategory::Domain))
            .map(|c| c.name.to_lowercase())
            .collect();

        let prospect_text =
            format!("{} {}", prospect.name, prospect.reason_matched.join(" ")).to_lowercase();

        let matches = project_domains
            .iter()
            .filter(|d| prospect_text.contains(d.as_str()))
            .count();

        if project_domains.is_empty() {
            0.0
        } else {
            (matches as f32) / (project_domains.len() as f32)
        }
    }

    /// Score based on repository activity (stars, forks, etc.)
    fn score_repository_activity(&self, prospect: &Prospect) -> f32 {
        if let Some(audience_size) = prospect.audience_size {
            // Logarithmic scaling to avoid dominance by very large projects
            if audience_size == 0 {
                0.0
            } else {
                (audience_size as f32).log10() / 5.0 // Max score at 100k stars
            }
        } else {
            0.5 // Neutral score if no activity data
        }
    }

    /// Score based on audience relevance
    fn score_audience_relevance(&self, prospect: &Prospect) -> f32 {
        // Higher score if contact information is available
        if prospect.contact_available && !prospect.contact_info.is_empty() {
            1.0
        } else {
            0.3
        }
    }

    /// Score based on prior interactions
    fn score_prior_interactions(&self, prospect: &Prospect) -> f32 {
        if prospect.previous_contact.is_some() {
            0.5 // Neutral score - could be positive or negative
        } else {
            1.0 // Fresh prospect
        }
    }

    /// Calculate addressable market score from all prospects
    pub fn calculate_market_score(
        &self,
        prospects: &[Prospect],
    ) -> crate::types::AddressableMarketScore {
        let total_prospects = prospects.len() as u64;

        let reachable_users = prospects.iter().filter(|p| p.contact_available).count() as u64;

        let high_confidence = prospects.iter().filter(|p| p.confidence >= 0.7).count() as u64;

        let immediate_contacts = prospects
            .iter()
            .filter(|p| p.confidence >= 0.8 && p.contact_available)
            .count() as u64;

        // Estimate potential users based on audience sizes
        let potential_users = prospects
            .iter()
            .filter_map(|p| p.audience_size)
            .sum::<u64>()
            .max(total_prospects * 10); // Minimum estimate

        crate::types::AddressableMarketScore {
            potential_users,
            reachable_users,
            high_confidence,
            immediate_contacts,
            calculated_at: chrono::Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Capability, CapabilityCategory, CapabilitySource, DevelopmentStage, Platform, ProspectType,
    };

    #[test]
    fn test_scoring() {
        let project = crate::types::Project {
            name: "test".to_string(),
            description: "test".to_string(),
            repository_url: None,
            website_url: None,
            capabilities: vec![
                Capability {
                    name: "rust".to_string(),
                    category: CapabilityCategory::Language,
                    source: CapabilitySource::CargoToml,
                    confidence: 0.9,
                },
                Capability {
                    name: "terminal".to_string(),
                    category: CapabilityCategory::Domain,
                    source: CapabilitySource::Readme,
                    confidence: 0.8,
                },
            ],
            keywords: vec!["cli".to_string(), "productivity".to_string()],
            development_stage: DevelopmentStage::Prototype,
            last_analyzed: chrono::Utc::now(),
            commit_hash: None,
        };

        let mut prospect = Prospect {
            name: "terminal-rust-app".to_string(),
            prospect_type: ProspectType::Repository,
            platform: Platform::GitHub,
            url: "https://github.com/test/repo".to_string(),
            reason_matched: vec!["A rust terminal application".to_string()],
            confidence: 0.0,
            audience_size: Some(1000),
            contact_available: true,
            contact_info: vec![],
            previous_contact: None,
            discovered_at: chrono::Utc::now(),
            last_updated: chrono::Utc::now(),
        };

        let model = ScoringModel::with_default_weights();
        model.score_prospect(&project, &mut prospect);

        assert!(prospect.confidence > 0.0);
    }
}
