//! Ulysses transformation-distance measurement (directive section 7).
//!
//! These are heuristic proxies derived from a git diff's shape, not a
//! precise architectural measurement — illustrative rather than
//! prescriptive, matching the directive's own framing of the decision
//! function in section 10. They exist to surface distance for a human (or
//! Ami) to weigh, not to serve as an automated gate on their own.

use crate::types::TransformationDistance;
use anyhow::{Context, Result};
use git2::{DiffOptions, Repository};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Measure the transformation distance of a diff.
///
/// `base` defaults to `HEAD`. `head` defaults to the working tree (including
/// staged changes), so with no arguments this measures uncommitted work.
pub fn measure(
    repo_path: &Path,
    base: Option<&str>,
    head: Option<&str>,
) -> Result<TransformationDistance> {
    let repo = Repository::open(repo_path).context("failed to open git repository")?;

    let base_tree = match base {
        Some(reference) => repo
            .revparse_single(reference)
            .with_context(|| format!("failed to resolve base revision '{reference}'"))?
            .peel_to_tree()
            .context("base revision has no tree")?,
        None => repo
            .head()
            .context("repository has no HEAD to diff against")?
            .peel_to_tree()
            .context("HEAD has no tree")?,
    };

    let mut diff_opts = DiffOptions::new();
    diff_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true);
    let diff = match head {
        Some(reference) => {
            let head_tree = repo
                .revparse_single(reference)
                .with_context(|| format!("failed to resolve head revision '{reference}'"))?
                .peel_to_tree()
                .context("head revision has no tree")?;
            repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut diff_opts))
        }
        None => repo.diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut diff_opts)),
    }
    .context("failed to compute diff")?;

    let stats = diff.stats().context("failed to compute diff stats")?;
    let files_changed = stats.files_changed() as f32;
    let total_lines = (stats.insertions() + stats.deletions()) as f32;

    let mut touched_dirs: HashSet<PathBuf> = HashSet::new();
    let mut touches_cargo_toml = false;
    let mut touches_entry_point = false;

    for delta in diff.deltas() {
        let path = delta.new_file().path().or_else(|| delta.old_file().path());
        let Some(path) = path else { continue };
        let path_str = path.to_string_lossy();
        if path_str == "Cargo.toml" {
            touches_cargo_toml = true;
        }
        if path_str.ends_with("src/main.rs") || path_str.ends_with("src/lib.rs") {
            touches_entry_point = true;
        }
        if let Some(parent) = path.parent() {
            touched_dirs.insert(parent.to_path_buf());
        }
    }

    let total_source_files = count_source_files(repo_path).max(1) as f32;

    let component_distance = (files_changed / total_source_files).clamp(0.0, 1.0);

    let architecture_distance = (if touches_cargo_toml { 0.3 } else { 0.0 }
        + (touched_dirs.len() as f32 * 0.05))
        .clamp(0.0, 1.0);

    let capability_distance =
        ((total_lines / 500.0) * 0.6 + if touches_entry_point { 0.2 } else { 0.0 }).clamp(0.0, 1.0);

    // Identity distance only rises when a large fraction of the codebase and
    // its dependency manifest move together — a proxy for "this is no
    // longer the same product", not a general size penalty.
    let product_identity_distance = if touches_cargo_toml && component_distance > 0.5 {
        0.5
    } else {
        0.0
    };

    Ok(TransformationDistance {
        component_distance,
        architecture_distance,
        capability_distance,
        product_identity_distance,
    })
}

fn count_source_files(repo_path: &Path) -> usize {
    let src = repo_path.join("src");
    let Ok(read) = std::fs::read_dir(&src) else {
        return 0;
    };
    read.flatten()
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git command failed to start");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo(dir: &Path) {
        run(dir, &["init", "-q"]);
        run(dir, &["config", "user.email", "test@example.com"]);
        run(dir, &["config", "user.name", "Test"]);
    }

    #[test]
    fn small_local_change_scores_low() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        run(dir.path(), &["add", "."]);
        run(dir.path(), &["commit", "-q", "-m", "init"]);

        std::fs::write(dir.path().join("src/util.rs"), "pub fn f() {}\n").unwrap();

        let distance = measure(dir.path(), None, None).unwrap();
        assert!(distance.overall() < 0.4, "distance was {distance:?}");
    }

    #[test]
    fn dependency_and_wide_change_raises_identity_distance() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        run(dir.path(), &["add", "."]);
        run(dir.path(), &["commit", "-q", "-m", "init"]);

        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\n[dependencies]\naxum=\"1\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/main.rs"),
            "fn main() { println!(\"rewritten\"); }\n",
        )
        .unwrap();

        let distance = measure(dir.path(), None, None).unwrap();
        assert!(
            distance.product_identity_distance > 0.0,
            "distance was {distance:?}"
        );
    }
}
