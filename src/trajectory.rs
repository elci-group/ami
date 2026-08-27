//! Cumulative transformation tracking (directive section 18): individually
//! small commits can collectively constitute a much larger evolutionary
//! movement than any one of them shows on its own. This walks a range of
//! commits and re-uses [`crate::transformation::measure`] — the same
//! distance function `ami evolve propose` uses for a single candidate — on
//! each commit against its first parent, so the cumulative figure is
//! directly comparable to a single proposal's distance rather than mixing
//! in an unrelated scoring system.

use crate::transformation;
use crate::types::TransformationDistance;
use anyhow::{Context, Result};
use git2::Repository;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TrajectoryStep {
    pub commit: String,
    pub summary: String,
    pub distance: TransformationDistance,
}

#[derive(Debug, Clone, Default)]
pub struct Trajectory {
    /// Oldest first.
    pub steps: Vec<TrajectoryStep>,
}

impl Trajectory {
    /// Walk up to `limit` commits back from HEAD, stopping early at `since`
    /// (exclusive) if given, and measure each commit's distance against its
    /// first parent. Root/merge-boundary commits with no first parent stop
    /// the walk rather than being silently skipped, so callers always know
    /// the trajectory covers a contiguous range.
    pub fn walk(repo_path: &Path, limit: usize, since: Option<&str>) -> Result<Self> {
        let repo = Repository::open(repo_path).context("failed to open git repository")?;

        let since_oid = since
            .map(|reference| -> Result<_> {
                Ok(repo
                    .revparse_single(reference)
                    .with_context(|| format!("failed to resolve '{reference}'"))?
                    .id())
            })
            .transpose()?;

        let mut revwalk = repo.revwalk().context("failed to start revwalk")?;
        revwalk.push_head().context("repository has no HEAD")?;

        let mut steps = Vec::new();
        for oid in revwalk {
            let oid = oid.context("failed to read commit during revwalk")?;
            if Some(oid) == since_oid || steps.len() >= limit {
                break;
            }
            let commit = repo.find_commit(oid).context("failed to read commit")?;
            let Ok(parent) = commit.parent(0) else {
                break; // reached the root commit
            };

            let distance = transformation::measure(
                repo_path,
                Some(&parent.id().to_string()),
                Some(&oid.to_string()),
            )?;
            steps.push(TrajectoryStep {
                commit: oid.to_string()[..7.min(oid.to_string().len())].to_string(),
                summary: commit.summary().unwrap_or("").to_string(),
                distance,
            });
        }
        steps.reverse();
        Ok(Self { steps })
    }

    pub fn cumulative(&self) -> f32 {
        self.steps.iter().map(|step| step.distance.overall()).sum()
    }

    fn max_step(&self) -> f32 {
        self.steps
            .iter()
            .map(|step| step.distance.overall())
            .fold(0.0, f32::max)
    }

    /// True when the *sum* of steps crosses a "significant architectural
    /// change" threshold (0.60, per the section 7 table) that no individual
    /// step reached on its own — the case the directive explicitly requires
    /// Ami to detect: ten innocuous PRs adding up to one large one.
    pub fn hidden_by_decomposition(&self) -> bool {
        self.cumulative().min(1.0) >= 0.60 && self.max_step() < 0.60
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
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

    fn commit_all(dir: &Path, message: &str) {
        run(dir, &["add", "."]);
        run(dir, &["commit", "-q", "-m", message]);
    }

    #[test]
    fn walks_commits_and_sums_distance() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        commit_all(dir.path(), "init");

        std::fs::write(dir.path().join("src/a.rs"), "pub fn a() {}\n").unwrap();
        commit_all(dir.path(), "add a");

        std::fs::write(dir.path().join("src/b.rs"), "pub fn b() {}\n").unwrap();
        commit_all(dir.path(), "add b");

        let trajectory = Trajectory::walk(dir.path(), 10, None).unwrap();
        assert_eq!(trajectory.steps.len(), 2);
        assert_eq!(trajectory.steps[0].summary, "add a");
        assert_eq!(trajectory.steps[1].summary, "add b");
        assert!(trajectory.cumulative() > 0.0);
    }

    #[test]
    fn since_stops_the_walk_early() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        commit_all(dir.path(), "init");
        let init_oid = {
            let repo = Repository::open(dir.path()).unwrap();
            let head = repo.head().unwrap();
            let commit = head.peel_to_commit().unwrap();
            commit.id().to_string()
        };

        std::fs::write(dir.path().join("src/a.rs"), "pub fn a() {}\n").unwrap();
        commit_all(dir.path(), "add a");

        let trajectory = Trajectory::walk(dir.path(), 10, Some(&init_oid)).unwrap();
        assert_eq!(trajectory.steps.len(), 1);
        assert_eq!(trajectory.steps[0].summary, "add a");
    }

    #[test]
    fn detects_cumulative_drift_hidden_by_decomposition() {
        let mut steps = Vec::new();
        for _ in 0..10 {
            steps.push(TrajectoryStep {
                commit: "abc1234".to_string(),
                summary: "small change".to_string(),
                distance: TransformationDistance {
                    component_distance: 0.2,
                    architecture_distance: 0.2,
                    capability_distance: 0.2,
                    product_identity_distance: 0.2,
                },
            });
        }
        let trajectory = Trajectory { steps };
        assert!(trajectory.max_step() < 0.60);
        assert!(trajectory.hidden_by_decomposition());
    }
}
