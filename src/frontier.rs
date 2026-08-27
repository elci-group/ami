//! Capability frontier mapping (directive sections 3 and 6): a bounded,
//! evidence-grounded read of what the project can already do, what it
//! implies but does not yet expose, and what small set of extensions are
//! reachable from its present dependencies. Nothing here is speculative —
//! every entry cites the file or dependency it was derived from.

use crate::types::{CapabilityStatus, Project};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct FrontierEntry {
    pub capability: String,
    pub status: CapabilityStatus,
    pub rationale: String,
}

#[derive(Debug, Clone, Default)]
pub struct CapabilityFrontier {
    pub existing: Vec<FrontierEntry>,
    pub latent: Vec<FrontierEntry>,
    pub reachable: Vec<FrontierEntry>,
}

/// Extension-suggestion rules: (dependency/capability name substring,
/// suggested reachable capability, rationale). Only fires when the named
/// capability is actually present in the project.
const REACHABLE_RULES: &[(&str, &str, &str)] = &[
    (
        "csv",
        "structured export in additional formats (JSON/HTML/Markdown)",
        "CSV output is already implemented; an additional serializer is a bounded, same-shape extension",
    ),
    (
        "reqwest",
        "an additional public discovery source",
        "the HTTP client and DiscoverySourceTrait pattern already exist; a new source is a new trait impl, not new architecture",
    ),
    (
        "clap",
        "an additional CLI subcommand",
        "argument parsing is already centralised; a new subcommand extends the existing Commands enum",
    ),
];

const LATENT_MARKERS: &[&str] = &["todo!", "unimplemented!", "#[allow(dead_code)]"];

impl CapabilityFrontier {
    pub fn analyze(project: &Project, path: &Path) -> Self {
        let existing = project
            .capabilities
            .iter()
            .map(|cap| FrontierEntry {
                capability: cap.name.clone(),
                status: CapabilityStatus::Existing,
                rationale: format!("{:?} detected via {:?}", cap.category, cap.source),
            })
            .collect();

        Self {
            existing,
            latent: Self::find_latent(path),
            reachable: Self::suggest_reachable(project),
        }
    }

    fn find_latent(path: &Path) -> Vec<FrontierEntry> {
        let src = path.join("src");
        let Ok(read) = fs::read_dir(&src) else {
            return Vec::new();
        };

        let mut entries = Vec::new();
        for file in read.flatten() {
            let file_path = file.path();
            if file_path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&file_path) else {
                continue;
            };
            let file_name = file_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?");
            for marker in LATENT_MARKERS {
                // Skip lines where the marker only appears as a quoted string
                // literal (e.g. this module's own `LATENT_MARKERS` table) —
                // otherwise the detector flags itself.
                let quoted = format!("\"{marker}\"");
                let is_real_usage = content
                    .lines()
                    .any(|line| line.contains(marker) && !line.contains(&quoted));
                if is_real_usage {
                    entries.push(FrontierEntry {
                        capability: format!("incomplete implementation in src/{file_name}"),
                        status: CapabilityStatus::Latent,
                        rationale: format!("`{marker}` present in src/{file_name}"),
                    });
                }
            }
        }
        entries
    }

    fn suggest_reachable(project: &Project) -> Vec<FrontierEntry> {
        let names: Vec<String> = project
            .capabilities
            .iter()
            .map(|cap| cap.name.to_lowercase())
            .collect();

        REACHABLE_RULES
            .iter()
            .filter(|(needle, _, _)| names.iter().any(|name| name.contains(needle)))
            .map(|(_, capability, rationale)| FrontierEntry {
                capability: capability.to_string(),
                status: CapabilityStatus::Reachable,
                rationale: rationale.to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Capability, CapabilityCategory, CapabilitySource, DevelopmentStage};
    use chrono::Utc;
    use tempfile::tempdir;

    fn project_with(capabilities: Vec<Capability>) -> Project {
        Project {
            name: "test".to_string(),
            description: "test project".to_string(),
            repository_url: None,
            website_url: None,
            capabilities,
            keywords: Vec::new(),
            development_stage: DevelopmentStage::Alpha,
            last_analyzed: Utc::now(),
            commit_hash: None,
        }
    }

    #[test]
    fn reachable_rules_only_fire_on_present_dependencies() {
        let project = project_with(vec![Capability {
            name: "csv".to_string(),
            category: CapabilityCategory::Framework,
            source: CapabilitySource::CargoToml,
            confidence: 0.8,
        }]);
        let dir = tempdir().unwrap();
        let frontier = CapabilityFrontier::analyze(&project, dir.path());
        assert_eq!(frontier.reachable.len(), 1);
        assert!(frontier.reachable[0].capability.contains("export"));

        let empty = project_with(Vec::new());
        let frontier = CapabilityFrontier::analyze(&empty, dir.path());
        assert!(frontier.reachable.is_empty());
    }

    #[test]
    fn latent_capabilities_cite_the_source_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("stub.rs"), "fn f() { todo!() }").unwrap();

        let project = project_with(Vec::new());
        let frontier = CapabilityFrontier::analyze(&project, dir.path());
        assert_eq!(frontier.latent.len(), 1);
        assert!(frontier.latent[0].rationale.contains("stub.rs"));
    }
}
