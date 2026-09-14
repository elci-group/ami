use crate::types::{Capability, CapabilityCategory, Project, SearchPhrase};
use std::collections::HashMap;

pub struct SearchGenerator;

impl SearchGenerator {
    /// Generate search phrases from project capabilities
    pub fn generate(project: &Project) -> Vec<SearchPhrase> {
        let mut phrases = Vec::new();
        let mut capability_map: HashMap<String, Vec<String>> = HashMap::new();

        // Group capabilities by name for reference
        for cap in &project.capabilities {
            capability_map
                .entry(cap.name.clone())
                .or_default()
                .push(cap.name.clone());
        }

        // Generate phrases based on capability categories
        for cap in &project.capabilities {
            phrases.extend(Self::generate_from_capability(cap, &capability_map));
        }

        // Generate phrases from keywords
        for keyword in &project.keywords {
            phrases.push(SearchPhrase {
                phrase: keyword.clone(),
                source_capabilities: vec![keyword.clone()],
                weight: 0.8,
            });
        }

        // Generate compound phrases
        phrases.extend(Self::generate_compound_phrases(&project.capabilities));

        // Generate domain-specific phrases
        phrases.extend(Self::generate_domain_phrases(project));

        // Sort by weight and deduplicate
        phrases.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        phrases.dedup_by(|a, b| a.phrase == b.phrase);

        phrases
    }

    /// Generate search phrases from a single capability
    fn generate_from_capability(
        capability: &Capability,
        _all_caps: &HashMap<String, Vec<String>>,
    ) -> Vec<SearchPhrase> {
        let mut phrases = Vec::new();
        let name = &capability.name;

        match capability.category {
            CapabilityCategory::Technology => {
                phrases.push(SearchPhrase {
                    phrase: format!("{} technology", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.9,
                });
                phrases.push(SearchPhrase {
                    phrase: format!("{} projects", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.85,
                });
            }
            CapabilityCategory::Domain => {
                phrases.push(SearchPhrase {
                    phrase: format!("{} software", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.9,
                });
                phrases.push(SearchPhrase {
                    phrase: format!("{} tools", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.85,
                });
            }
            CapabilityCategory::Platform => {
                phrases.push(SearchPhrase {
                    phrase: format!("{} development", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.9,
                });
                phrases.push(SearchPhrase {
                    phrase: format!("{} apps", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.8,
                });
            }
            CapabilityCategory::Language => {
                phrases.push(SearchPhrase {
                    phrase: format!("{} ecosystem", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.9,
                });
                phrases.push(SearchPhrase {
                    phrase: format!("{} programming", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.85,
                });
            }
            CapabilityCategory::Framework => {
                phrases.push(SearchPhrase {
                    phrase: format!("{} framework", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.9,
                });
                phrases.push(SearchPhrase {
                    phrase: format!("{} library", name),
                    source_capabilities: vec![name.clone()],
                    weight: 0.85,
                });
            }
            CapabilityCategory::Feature => {
                phrases.push(SearchPhrase {
                    phrase: name.clone(),
                    source_capabilities: vec![name.clone()],
                    weight: capability.confidence,
                });
                phrases.push(SearchPhrase {
                    phrase: format!("{} software", name),
                    source_capabilities: vec![name.clone()],
                    weight: capability.confidence * 0.9,
                });
            }
        }

        phrases
    }

    /// Generate compound search phrases from multiple capabilities
    fn generate_compound_phrases(capabilities: &[Capability]) -> Vec<SearchPhrase> {
        let mut phrases = Vec::new();

        // Get capability names
        let names: Vec<&str> = capabilities.iter().map(|c| c.name.as_str()).collect();

        // Generate pairs (limited to avoid explosion)
        for i in 0..names.len().min(10) {
            for j in (i + 1)..names.len().min(10) {
                phrases.push(SearchPhrase {
                    phrase: format!("{} {}", names[i], names[j]),
                    source_capabilities: vec![names[i].to_string(), names[j].to_string()],
                    weight: 0.95,
                });
            }
        }

        phrases
    }

    /// Generate domain-specific search phrases
    fn generate_domain_phrases(project: &Project) -> Vec<SearchPhrase> {
        let mut phrases = Vec::new();

        // Terminal-specific phrases
        if project.capabilities.iter().any(|c| {
            c.name.to_lowercase().contains("terminal")
                || c.name.to_lowercase().contains("ansi")
                || c.name.to_lowercase().contains("tui")
        }) {
            phrases.push(SearchPhrase {
                phrase: "terminal emulator".to_string(),
                source_capabilities: vec!["terminal".to_string()],
                weight: 0.9,
            });
            phrases.push(SearchPhrase {
                phrase: "terminal productivity".to_string(),
                source_capabilities: vec!["terminal".to_string()],
                weight: 0.85,
            });
            phrases.push(SearchPhrase {
                phrase: "TUI applications".to_string(),
                source_capabilities: vec!["TUI".to_string()],
                weight: 0.9,
            });
        }

        // Video/streaming-specific phrases
        if project.capabilities.iter().any(|c| {
            c.name.to_lowercase().contains("video")
                || c.name.to_lowercase().contains("stream")
                || c.name.to_lowercase().contains("sixel")
        }) {
            phrases.push(SearchPhrase {
                phrase: "video streaming".to_string(),
                source_capabilities: vec!["video".to_string()],
                weight: 0.9,
            });
            phrases.push(SearchPhrase {
                phrase: "terminal streaming".to_string(),
                source_capabilities: vec!["terminal".to_string(), "streaming".to_string()],
                weight: 0.95,
            });
        }

        // SSH/remote-specific phrases
        if project.capabilities.iter().any(|c| {
            c.name.to_lowercase().contains("ssh") || c.name.to_lowercase().contains("remote")
        }) {
            phrases.push(SearchPhrase {
                phrase: "SSH management".to_string(),
                source_capabilities: vec!["SSH".to_string()],
                weight: 0.9,
            });
            phrases.push(SearchPhrase {
                phrase: "remote terminal".to_string(),
                source_capabilities: vec!["remote".to_string(), "terminal".to_string()],
                weight: 0.95,
            });
        }

        // Rust ecosystem phrases
        if project.capabilities.iter().any(|c| {
            c.name.to_lowercase().contains("rust")
                || c.source == crate::types::CapabilitySource::CargoToml
        }) {
            phrases.push(SearchPhrase {
                phrase: "Rust ecosystem".to_string(),
                source_capabilities: vec!["Rust".to_string()],
                weight: 0.9,
            });
            phrases.push(SearchPhrase {
                phrase: "Rust projects".to_string(),
                source_capabilities: vec!["Rust".to_string()],
                weight: 0.85,
            });
        }

        phrases
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Capability, CapabilityCategory, CapabilitySource, DevelopmentStage};

    #[test]
    fn test_search_generation() {
        let project = Project {
            name: "test".to_string(),
            description: "test".to_string(),
            repository_url: None,
            website_url: None,
            capabilities: vec![
                Capability {
                    name: "terminal".to_string(),
                    category: CapabilityCategory::Platform,
                    source: CapabilitySource::Readme,
                    confidence: 0.9,
                },
                Capability {
                    name: "video".to_string(),
                    category: CapabilityCategory::Feature,
                    source: CapabilitySource::Readme,
                    confidence: 0.8,
                },
            ],
            keywords: vec!["cli".to_string(), "productivity".to_string()],
            development_stage: DevelopmentStage::Prototype,
            last_analyzed: chrono::Utc::now(),
            commit_hash: None,
        };

        let phrases = SearchGenerator::generate(&project);

        assert!(!phrases.is_empty());
        assert!(phrases.iter().any(|p| p.phrase.contains("terminal")));
        assert!(phrases.iter().any(|p| p.phrase.contains("video")));
    }
}
