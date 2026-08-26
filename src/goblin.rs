use anyhow::{Context, Result};
use serde_json::json;
use std::env;
use std::process::{Command, Stdio};

/// Use Goblin only for its bounded, deterministic classification capability.
/// Missing Goblin or an escalation is a normal fallback, never a hard failure.
pub fn classify(text: &str, labels: &[&str]) -> Result<Option<String>> {
    let binary = env::var("GOBLIN_BIN").unwrap_or_else(|_| "goblin".to_string());
    let input = serde_json::to_string(&json!({ "text": text, "labels": labels }))?;
    let mut child = match Command::new(binary)
        .args(["run", "classification", "--threshold", "0.55"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Ok(None),
    };
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("Goblin returned invalid classification JSON")?;
    if response.get("status").and_then(|value| value.as_str()) != Some("success") {
        return Ok(None);
    }
    Ok(response
        .get("result")
        .and_then(|result| result.get("label"))
        .and_then(|label| label.as_str())
        .map(str::to_string))
}
