use crate::types::{Capability, Project, Prospect};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const REQUIRED_GENOME_FILES: &[&str] = &[
    "identity.yaml",
    "voice.yaml",
    "visual.yaml",
    "audience.yaml",
    "prohibited.yaml",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AudienceFile {
    pub segments: Vec<BrandSegment>,
    pub narratives: Vec<BrandNarrative>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BrandSegment {
    pub name: String,
    pub description: String,
    pub pains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BrandNarrative {
    pub capability: String,
    pub narrative: String,
    pub audiences: Vec<String>,
    pub formats: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrandStatus {
    pub root: PathBuf,
    pub configured: bool,
    pub missing_files: Vec<String>,
    pub segments: usize,
    pub narratives: usize,
    pub audience_file_modified: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrandMaintenancePlan {
    pub path: PathBuf,
    pub additions: Vec<BrandNarrative>,
    pub new_segments: Vec<BrandSegment>,
    pub applied: bool,
}

pub fn status(root: &Path) -> Result<BrandStatus> {
    let dir = root.join(".brandi");
    let missing_files = REQUIRED_GENOME_FILES
        .iter()
        .filter(|name| !dir.join(name).is_file())
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    let audience_path = dir.join("audience.yaml");
    let audience = if audience_path.is_file() {
        load_audience(&audience_path)?
    } else {
        AudienceFile::default()
    };
    let modified = audience_path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(|time| format!("{time:?}"));
    Ok(BrandStatus {
        root: root.to_path_buf(),
        configured: missing_files.is_empty(),
        missing_files,
        segments: audience.segments.len(),
        narratives: audience.narratives.len(),
        audience_file_modified: modified,
    })
}

pub fn plan_maintenance(
    root: &Path,
    project: &Project,
    prospects: &[Prospect],
    apply: bool,
) -> Result<BrandMaintenancePlan> {
    let path = root.join(".brandi/audience.yaml");
    if !path.is_file() {
        anyhow::bail!("Brandi audience file is missing: {}", path.display());
    }
    let mut audience = load_audience(&path)?;
    let existing_segments = audience
        .segments
        .iter()
        .map(|segment| segment.name.to_lowercase())
        .collect::<BTreeSet<_>>();
    let mut new_segments = Vec::new();
    for segment in inferred_segments(project, prospects) {
        if !existing_segments.contains(&segment.name.to_lowercase()) {
            new_segments.push(segment);
        }
    }

    let existing_narratives = audience
        .narratives
        .iter()
        .map(|narrative| narrative.capability.to_lowercase())
        .collect::<BTreeSet<_>>();
    let mut additions = Vec::new();
    for narrative in inferred_narratives(project, &new_segments) {
        if !existing_narratives.contains(&narrative.capability.to_lowercase()) {
            additions.push(narrative);
        }
    }

    if apply && (!new_segments.is_empty() || !additions.is_empty()) {
        audience.segments.extend(new_segments.clone());
        audience.narratives.extend(additions.clone());
        let yaml =
            serde_yaml::to_string(&audience).context("failed to render Brandi audience YAML")?;
        fs::write(&path, yaml).context("failed to write Brandi audience.yaml")?;
    }

    Ok(BrandMaintenancePlan {
        path,
        additions,
        new_segments,
        applied: apply,
    })
}

fn load_audience(path: &Path) -> Result<AudienceFile> {
    let content = fs::read_to_string(path).context("failed to read Brandi audience.yaml")?;
    serde_yaml::from_str(&content).context("failed to parse Brandi audience.yaml")
}

fn inferred_segments(project: &Project, prospects: &[Prospect]) -> Vec<BrandSegment> {
    let mut names = BTreeSet::new();
    let labels = [
        "developer_tools_adopters",
        "technical_early_adopters",
        "developer_educators",
        "adjacent_technology_teams",
    ];
    for prospect in prospects {
        let fallback = match prospect.platform {
            crate::types::Platform::GitHub | crate::types::Platform::StackOverflow => {
                "developer_tools_adopters"
            }
            crate::types::Platform::Reddit | crate::types::Platform::HackerNews => {
                "technical_early_adopters"
            }
            crate::types::Platform::DevTo => "developer_educators",
            _ => "adjacent_technology_teams",
        };
        let text = format!("{} {}", prospect.name, prospect.reason_matched.join(" "));
        let label = crate::goblin::classify(&text, &labels)
            .ok()
            .flatten()
            .unwrap_or_else(|| fallback.to_string());
        names.insert(label);
    }
    if names.is_empty() {
        names.insert("developer_tools_adopters".to_string());
    }
    names
        .into_iter()
        .map(|name| BrandSegment {
            name,
            description: format!(
                "People and teams evaluating {} for their workflow",
                project.name
            ),
            pains: vec![
                "finding trustworthy tools that fit existing workflows".to_string(),
                "keeping technical decisions aligned with project identity".to_string(),
            ],
        })
        .collect()
}

fn inferred_narratives(project: &Project, segments: &[BrandSegment]) -> Vec<BrandNarrative> {
    let audience: Vec<String> = segments
        .iter()
        .map(|segment| segment.name.clone())
        .collect();
    project
        .capabilities
        .iter()
        .filter(|capability| capability.confidence >= 0.7)
        .take(8)
        .map(|capability: &Capability| BrandNarrative {
            capability: capability.name.clone(),
            narrative: format!(
                "Use {} to help teams adopt {} with evidence from real project workflows",
                capability.name, project.name
            ),
            audiences: audience.clone(),
            formats: vec![
                "docs".to_string(),
                "blog".to_string(),
                "demo_video".to_string(),
            ],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        CapabilityCategory, CapabilitySource, DevelopmentStage, Platform, ProspectType,
    };
    use chrono::Utc;

    fn project() -> Project {
        Project {
            name: "ami".to_string(),
            description: "market intelligence".to_string(),
            repository_url: None,
            website_url: None,
            capabilities: vec![Capability {
                name: "deterministic discovery".to_string(),
                category: CapabilityCategory::Feature,
                source: CapabilitySource::Readme,
                confidence: 0.9,
            }],
            keywords: vec![],
            development_stage: DevelopmentStage::Beta,
            last_analyzed: Utc::now(),
            commit_hash: None,
        }
    }

    fn prospect() -> Prospect {
        Prospect {
            name: "example".to_string(),
            prospect_type: ProspectType::Repository,
            platform: Platform::GitHub,
            url: "https://github.com/example/example".to_string(),
            reason_matched: vec!["developer tooling".to_string()],
            confidence: 0.8,
            audience_size: Some(10),
            contact_available: false,
            contact_info: vec![],
            previous_contact: None,
            discovered_at: Utc::now(),
            last_updated: Utc::now(),
        }
    }

    #[test]
    fn maintenance_is_plan_first_and_applies_only_when_requested() {
        let root = std::env::temp_dir().join(format!(
            "ami-brandi-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let brandi_dir = root.join(".brandi");
        fs::create_dir_all(&brandi_dir).unwrap();
        for file in REQUIRED_GENOME_FILES {
            fs::write(
                brandi_dir.join(file),
                if *file == "audience.yaml" {
                    "segments: []\nnarratives: []\n"
                } else {
                    "{}\n"
                },
            )
            .unwrap();
        }

        let plan = plan_maintenance(&root, &project(), &[prospect()], false).unwrap();
        assert!(!plan.additions.is_empty());
        assert!(!plan.new_segments.is_empty());
        assert_eq!(
            load_audience(&brandi_dir.join("audience.yaml"))
                .unwrap()
                .narratives
                .len(),
            0
        );

        let applied = plan_maintenance(&root, &project(), &[prospect()], true).unwrap();
        assert!(applied.applied);
        let audience = load_audience(&brandi_dir.join("audience.yaml")).unwrap();
        assert_eq!(audience.narratives.len(), 1);
        assert_eq!(audience.segments[0].name, "developer_tools_adopters");
        fs::remove_dir_all(root).unwrap();
    }
}
