// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT

use std::fs;
use tempfile::TempDir;

#[test]
fn test_project_reader_basic() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a basic Cargo.toml
    let cargo_toml = r#"
[package]
name = "test-project"
description = "A test project for Ami"
version = "0.1.0"
keywords = ["cli", "terminal", "productivity"]

[dependencies]
serde = "1.0"
tokio = "1.0"
"#;
    fs::write(project_path.join("Cargo.toml"), cargo_toml).unwrap();

    // Create a basic README.md
    let readme = r#"
# Test Project

A test project for Ami.

## Features

- Terminal interface
- ANSI support
- Unicode handling
"#;
    fs::write(project_path.join("README.md"), readme).unwrap();

    // Read the project
    let project = ami::project_reader::ProjectReader::read_from_path(project_path).unwrap();

    assert_eq!(project.name, "test-project");
    assert_eq!(project.description, "A test project for Ami");
    assert!(project.keywords.contains(&"cli".to_string()));
    assert!(project.keywords.contains(&"terminal".to_string()));

    // Check capabilities were extracted
    assert!(!project.capabilities.is_empty());

    // Check README capabilities
    let readme_caps: Vec<_> = project
        .capabilities
        .iter()
        .filter(|c| c.source == ami::types::CapabilitySource::Readme)
        .collect();
    assert!(!readme_caps.is_empty());
}

#[test]
fn test_search_generator() {
    let project = ami::types::Project {
        name: "test".to_string(),
        description: "test".to_string(),
        repository_url: None,
        website_url: None,
        capabilities: vec![ami::types::Capability {
            name: "terminal".to_string(),
            category: ami::types::CapabilityCategory::Platform,
            source: ami::types::CapabilitySource::Readme,
            confidence: 0.9,
        }],
        keywords: vec!["cli".to_string()],
        development_stage: ami::types::DevelopmentStage::Prototype,
        last_analyzed: chrono::Utc::now(),
        commit_hash: None,
    };

    let phrases = ami::search_generator::SearchGenerator::generate(&project);

    assert!(!phrases.is_empty());
    assert!(phrases.iter().any(|p| p.phrase.contains("terminal")));
    assert!(phrases.iter().any(|p| p.phrase.contains("cli")));
}

#[test]
fn test_database_operations() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path();

    let db = ami::database::ContactDatabase::new(db_path);
    db.initialize().unwrap();

    // Test database initialization
    assert!(db_path.exists());
    assert!(db_path.join("lists").exists());

    // Test stats on empty database
    let stats = db.get_stats().unwrap();
    assert_eq!(stats.total_prospects, 0);

    let prospect = ami::types::Prospect {
        name: "test-repository".to_string(),
        prospect_type: ami::types::ProspectType::Repository,
        platform: ami::types::Platform::GitHub,
        url: "https://github.com/test/repository".to_string(),
        reason_matched: vec!["rust tooling".to_string(), "CLI workflow".to_string()],
        confidence: 0.85,
        audience_size: Some(250),
        contact_available: true,
        contact_info: vec![ami::types::ContactChannel {
            channel_type: ami::types::ContactType::GitHub,
            value: "https://github.com/test".to_string(),
            verified: true,
        }],
        previous_contact: None,
        discovered_at: chrono::Utc::now(),
        last_updated: chrono::Utc::now(),
    };

    db.save_prospects(std::slice::from_ref(&prospect)).unwrap();
    let loaded = db.load_prospects().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].reason_matched, prospect.reason_matched);
    assert_eq!(loaded[0].contact_info.len(), 1);

    db.generate_all_lists(&loaded).unwrap();
    assert!(
        db.load_mailing_list(&ami::types::MailingListType::Alpha)
            .unwrap()
            .len()
            == 1
    );
}

#[test]
fn test_scoring_model() {
    let project = ami::types::Project {
        name: "test".to_string(),
        description: "test".to_string(),
        repository_url: None,
        website_url: None,
        capabilities: vec![ami::types::Capability {
            name: "rust".to_string(),
            category: ami::types::CapabilityCategory::Language,
            source: ami::types::CapabilitySource::CargoToml,
            confidence: 0.9,
        }],
        keywords: vec!["cli".to_string()],
        development_stage: ami::types::DevelopmentStage::Prototype,
        last_analyzed: chrono::Utc::now(),
        commit_hash: None,
    };

    let mut prospect = ami::types::Prospect {
        name: "rust-cli-tool".to_string(),
        prospect_type: ami::types::ProspectType::Repository,
        platform: ami::types::Platform::GitHub,
        url: "https://github.com/test/repo".to_string(),
        reason_matched: vec!["A rust CLI tool".to_string()],
        confidence: 0.0,
        audience_size: Some(1000),
        contact_available: true,
        contact_info: vec![],
        previous_contact: None,
        discovered_at: chrono::Utc::now(),
        last_updated: chrono::Utc::now(),
    };

    let model = ami::scoring::ScoringModel::with_default_weights();
    model.score_prospect(&project, &mut prospect);

    assert!(prospect.confidence > 0.0);
    assert!(prospect.confidence <= 1.0);
}
