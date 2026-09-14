use crate::types::{Capability, CapabilityCategory, CapabilitySource, DevelopmentStage, Project};
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::Path;

type ProjectMetadata = (
    Option<String>,
    String,
    Option<String>,
    Vec<String>,
    Vec<Capability>,
);

pub struct ProjectReader;

impl ProjectReader {
    /// Read a project from a directory path
    pub fn read_from_path(path: &Path) -> Result<Project> {
        let readme_path = path.join("README.md");
        let cargo_path = path.join("Cargo.toml");
        let package_path = path.join("package.json");

        let mut name = String::new();
        let mut description = String::new();
        let mut repository_url = None;
        let website_url = None;
        let mut capabilities = Vec::new();
        let mut keywords = Vec::new();
        let mut commit_hash = None;

        // Try to read Cargo.toml first
        if cargo_path.exists() {
            let (cargo_name, cargo_desc, cargo_repo, cargo_keywords, cargo_caps) =
                Self::read_cargo_toml(&cargo_path)?;
            if name.is_empty() {
                if let Some(cargo_name) = cargo_name {
                    name = cargo_name;
                }
            }
            if description.is_empty() {
                description = cargo_desc;
            }
            if repository_url.is_none() {
                repository_url = cargo_repo;
            }
            keywords.extend(cargo_keywords);
            capabilities.extend(cargo_caps);
        }

        // Try to read package.json
        if package_path.exists() {
            let (pkg_name, pkg_desc, pkg_repo, pkg_keywords, pkg_caps) =
                Self::read_package_json(&package_path)?;
            if name.is_empty() {
                if let Some(pkg_name) = pkg_name {
                    name = pkg_name;
                }
            }
            if description.is_empty() {
                description = pkg_desc;
            }
            if repository_url.is_none() {
                repository_url = pkg_repo;
            }
            keywords.extend(pkg_keywords);
            capabilities.extend(pkg_caps);
        }

        // Read README.md for additional info
        if readme_path.exists() {
            let (readme_desc, readme_caps) = Self::read_readme(&readme_path)?;
            if description.is_empty() {
                description = readme_desc;
            }
            capabilities.extend(readme_caps);
        }

        // Try to get git commit hash
        if let Ok(hash) = Self::get_git_commit_hash(path) {
            commit_hash = Some(hash);
        }

        // Infer development stage from capabilities and project state
        let development_stage = Self::infer_development_stage(&capabilities, path)?;

        // Deduplicate keywords and capabilities
        keywords.sort();
        keywords.dedup();
        capabilities.sort_by(|a, b| a.name.cmp(&b.name));
        capabilities.dedup_by(|a, b| a.name == b.name);

        let name = if name.is_empty() {
            path.canonicalize()
                .ok()
                .and_then(|value| value.file_name().map(|name| name.to_owned()))
                .and_then(|value| value.into_string().ok())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            name
        };
        let description = if description.is_empty() {
            format!("{} project", name)
        } else {
            description
        };

        Ok(Project {
            name,
            description,
            repository_url,
            website_url,
            capabilities,
            keywords,
            development_stage,
            last_analyzed: Utc::now(),
            commit_hash,
        })
    }

    /// Read Cargo.toml
    fn read_cargo_toml(path: &Path) -> Result<ProjectMetadata> {
        let content = std::fs::read_to_string(path).context("Failed to read Cargo.toml")?;

        let value: toml::Value = content.parse().context("Failed to parse Cargo.toml")?;

        let package = value.get("package").and_then(|p| p.as_table());

        let name = package
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());

        let description = package
            .and_then(|p| p.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        let repository_url = package
            .and_then(|p| p.get("repository"))
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());

        let keywords = package
            .and_then(|p| p.get("keywords"))
            .and_then(|k| k.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let mut capabilities = Vec::new();

        // Extract dependencies as capabilities
        if let Some(deps) = value.get("dependencies").and_then(|d| d.as_table()) {
            for dep_name in deps.keys() {
                capabilities.push(Capability {
                    name: dep_name.to_string(),
                    category: CapabilityCategory::Framework,
                    source: CapabilitySource::CargoToml,
                    confidence: 0.8,
                });
            }
        }

        Ok((name, description, repository_url, keywords, capabilities))
    }

    /// Read package.json
    fn read_package_json(path: &Path) -> Result<ProjectMetadata> {
        let content = std::fs::read_to_string(path).context("Failed to read package.json")?;

        let value: serde_json::Value =
            serde_json::from_str(&content).context("Failed to parse package.json")?;

        let name = value
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());

        let description = value
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        let repository_url = value.get("repository").and_then(|r| {
            if r.is_string() {
                r.as_str().map(|s| s.to_string())
            } else if r.is_object() {
                r.get("url").and_then(|u| u.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        });

        let keywords = value
            .get("keywords")
            .and_then(|k| k.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let mut capabilities = Vec::new();

        // Extract dependencies as capabilities
        if let Some(deps) = value.get("dependencies").and_then(|d| d.as_object()) {
            for dep_name in deps.keys() {
                capabilities.push(Capability {
                    name: dep_name.to_string(),
                    category: CapabilityCategory::Framework,
                    source: CapabilitySource::PackageJson,
                    confidence: 0.8,
                });
            }
        }

        Ok((name, description, repository_url, keywords, capabilities))
    }

    /// Read README.md
    fn read_readme(path: &Path) -> Result<(String, Vec<Capability>)> {
        let content = std::fs::read_to_string(path).context("Failed to read README.md")?;

        // Extract the first human-readable paragraph, skipping badges,
        // HTML, images, and fenced examples commonly found at the top of a README.
        let mut description_lines = Vec::new();
        let mut started = false;
        let mut in_code_block = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }
            if !started {
                if trimmed.is_empty()
                    || trimmed.starts_with('#')
                    || trimmed.starts_with('<')
                    || trimmed.starts_with("![")
                    || trimmed.starts_with("[![")
                {
                    continue;
                }
                started = true;
            } else if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('<') {
                break;
            }
            description_lines.push(trimmed);
        }
        let description = description_lines.join(" ");

        // Extract capabilities from README
        let capabilities = Self::extract_capabilities_from_text(&content, CapabilitySource::Readme);

        Ok((description, capabilities))
    }

    /// Extract capabilities from text
    fn extract_capabilities_from_text(text: &str, source: CapabilitySource) -> Vec<Capability> {
        let mut capabilities = Vec::new();

        // Look for feature lists, bullet points, etc.
        let lines: Vec<&str> = text.lines().collect();

        let mut in_code_block = false;
        for (i, line) in lines.iter().enumerate() {
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block || line.trim().starts_with('<') || line.trim().starts_with("![") {
                continue;
            }
            // Check for bullet points with checkmarks
            if line.trim().starts_with("- [x]") || line.trim().starts_with("* [x]") {
                if let Some(cap) = Self::parse_capability_line(line) {
                    capabilities.push(Capability {
                        name: cap,
                        category: CapabilityCategory::Feature,
                        source: source.clone(),
                        confidence: 0.9,
                    });
                }
            }

            // Check for regular bullet points
            if line.trim().starts_with("- ") || line.trim().starts_with("* ") {
                if let Some(cap) = Self::parse_capability_line(line) {
                    capabilities.push(Capability {
                        name: cap,
                        category: CapabilityCategory::Feature,
                        source: source.clone(),
                        confidence: 0.7,
                    });
                }
            }

            // Check for headings that might indicate features
            if line.starts_with("##") {
                let heading = line.trim_start_matches('#').trim();
                if heading.eq_ignore_ascii_case("features")
                    || heading.eq_ignore_ascii_case("capabilities")
                    || heading.eq_ignore_ascii_case("what it does")
                {
                    // Extract next few lines as capabilities
                    for next_line in lines.iter().skip(i + 1).take(10) {
                        if next_line.trim().is_empty() || next_line.starts_with('#') {
                            break;
                        }
                        if let Some(cap) = Self::parse_capability_line(next_line) {
                            capabilities.push(Capability {
                                name: cap,
                                category: CapabilityCategory::Feature,
                                source: source.clone(),
                                confidence: 0.85,
                            });
                        }
                    }
                }
            }
        }

        capabilities
    }

    /// Parse a capability from a line
    fn parse_capability_line(line: &str) -> Option<String> {
        let cleaned = line
            .trim()
            .trim_start_matches("- [x]")
            .trim_start_matches("- [ ]")
            .trim_start_matches("* [x]")
            .trim_start_matches("* [ ]")
            .trim_start_matches("-")
            .trim_start_matches("*")
            .trim()
            .trim_start_matches("✓")
            .trim();

        let cleaned = cleaned
            .trim_matches('*')
            .replace('`', "")
            .trim()
            .to_string();
        let cleaned = if let Some(end_bold) = cleaned.find("**:") {
            cleaned[..end_bold].trim_matches('*').trim().to_string()
        } else if let Some(colon) = cleaned.find(':') {
            let prefix = cleaned[..colon].trim();
            if prefix.len() >= 2 && prefix.len() <= 80 {
                prefix.to_string()
            } else {
                cleaned
            }
        } else {
            cleaned
        };

        let normalized = cleaned.to_ascii_lowercase();
        let metadata_heading = matches!(
            normalized.as_str(),
            "core"
                | "extended"
                | "major"
                | "minor"
                | "patch"
                | "features"
                | "capabilities"
                | "table of contents"
        );
        let implementation_artifact = normalized.starts_with('.')
            || normalized.contains(".json")
            || normalized.contains("<uuid>")
            || normalized.contains("<id>");

        if cleaned.is_empty()
            || cleaned.len() < 2
            || cleaned.len() > 120
            || metadata_heading
            || implementation_artifact
        {
            None
        } else {
            Some(cleaned)
        }
    }

    /// Get current git commit hash
    fn get_git_commit_hash(path: &Path) -> Result<String> {
        let repo = git2::Repository::discover(path).context("Failed to discover git repository")?;

        let head = repo.head().context("Failed to get HEAD")?;

        let commit = head.peel_to_commit().context("Failed to peel to commit")?;

        Ok(commit.id().to_string()[..8].to_string())
    }

    /// Infer development stage from capabilities and project state
    fn infer_development_stage(
        capabilities: &[Capability],
        path: &Path,
    ) -> Result<DevelopmentStage> {
        // Check for tags to determine stability
        if let Ok(repo) = git2::Repository::discover(path) {
            let tag_count = repo.tag_names(None)?.len();

            if tag_count > 10 {
                return Ok(DevelopmentStage::Stable);
            }
            if tag_count > 5 {
                return Ok(DevelopmentStage::ReleaseCandidate);
            }
            if tag_count > 2 {
                return Ok(DevelopmentStage::Beta);
            }
        }

        // Use capability count as a heuristic
        let cap_count = capabilities.len();

        if cap_count > 20 {
            Ok(DevelopmentStage::Beta)
        } else if cap_count > 10 {
            Ok(DevelopmentStage::Alpha)
        } else if cap_count > 5 {
            Ok(DevelopmentStage::MVP)
        } else {
            Ok(DevelopmentStage::Prototype)
        }
    }
}
