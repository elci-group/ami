//! Black-box coverage for the `ami evolve` subcommands: runs the actual
//! compiled binary against a real temp git repo, since the gate/frontier/
//! transformation logic was previously only verified by hand against this
//! repo and a live Padagonia server.

use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git command failed to start");
    assert!(status.success(), "git {args:?} failed");
}

/// A minimal but real Rust project with one commit, so `evolve frontier`
/// and `evolve propose` have a git history and a Cargo.toml to read.
fn fixture_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);

    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n\n[dependencies]\ncsv = \"1\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.path().join("README.md"), "# fixture\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    dir
}

fn ami(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ami"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run the ami binary")
}

#[test]
fn evolve_frontier_runs_against_a_real_project() {
    let repo = fixture_repo();
    let output = ami(repo.path(), &["evolve", "frontier", "--path", "."]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Capability frontier for"));
    assert!(stdout.contains("Existing"));
    // csv is a real dependency in the fixture, so the reachable-extension
    // rule should fire.
    assert!(stdout.contains("Reachable"));
}

#[test]
fn evolve_propose_reports_gate_failures_without_erroring() {
    let repo = fixture_repo();
    let output = ami(
        repo.path(),
        &[
            "evolve",
            "propose",
            "--path",
            ".",
            "--description",
            "a speculative feature",
            "--outcome",
            "test coverage for the gate pipeline",
            "--capability-status",
            "speculative",
        ],
    );
    // A gate failure is a reported finding, not a process error.
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Decision gates"));
    assert!(stdout.contains("3: ontologically valid"));
    assert!(stdout.contains("gates failed"));
}

#[test]
fn evolve_propose_rejects_an_unknown_capability_status() {
    let repo = fixture_repo();
    let output = ami(
        repo.path(),
        &[
            "evolve",
            "propose",
            "--path",
            ".",
            "--description",
            "x",
            "--outcome",
            "y",
            "--capability-status",
            "not-a-real-status",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown --capability-status"));
}

#[test]
fn evolve_trajectory_walks_real_commit_history() {
    let repo = fixture_repo();
    std::fs::write(repo.path().join("src/extra.rs"), "pub fn f() {}\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-q", "-m", "add extra module"]);

    let output = ami(
        repo.path(),
        &["evolve", "trajectory", "--path", ".", "--limit", "5"],
    );
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Transformation trajectory for"));
    assert!(stdout.contains("add extra module"));
}

#[test]
fn evolve_history_warns_without_erroring_when_padagonia_is_unreachable() {
    let repo = fixture_repo();
    let output = Command::new(env!("CARGO_BIN_EXE_ami"))
        .args(["evolve", "history"])
        .current_dir(repo.path())
        .env("PADAGONIA_URL", "http://127.0.0.1:1")
        .output()
        .expect("failed to run the ami binary");
    assert!(output.status.success(), "{output:?}");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Padagonia query failed"));
}
