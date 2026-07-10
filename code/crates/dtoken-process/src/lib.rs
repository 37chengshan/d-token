use anyhow::Result;
use dtoken_types::{InstanceProfile, ProcessStatus};
use serde::Serialize;
use std::{
    collections::HashMap,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::process::Child;

pub const OPENCODE_TEST_PROMPT: &str = "Reply with exactly: d-token-ok";
const OPENCODE_EXCLUDED_AGENTS: &str =
    "claude-code,codex-cli,cursor,vscode,aider,cline,roo,continue,windsurf";
const MAX_OUTPUT_SUMMARY_LEN: usize = 900;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpencodeCommandPlan {
    pub program: String,
    pub args: Vec<String>,
    pub envs: HashMap<String, String>,
    pub remove_envs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpencodeTestOutcome {
    pub ok: bool,
    pub run_id: String,
    pub model: String,
    pub category: String,
    pub output_summary: String,
    pub first_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpencodeLedgerEvent {
    pub event_id: String,
    pub run_id: String,
    pub stage: String,
    pub source: String,
    pub agent: String,
    pub route: String,
    pub provider: String,
    pub model: String,
    pub fallback: String,
    pub ok: bool,
    pub category: String,
    pub output_summary: String,
}

pub fn opencode_command_plan(model: &str) -> OpencodeCommandPlan {
    let mut envs = HashMap::new();
    envs.insert("DTOKEN_TEST_SCOPE".to_string(), "opencode-only".to_string());
    envs.insert(
        "DTOKEN_TEST_EXCLUDE".to_string(),
        OPENCODE_EXCLUDED_AGENTS.to_string(),
    );
    envs.insert("NO_PROXY".to_string(), "127.0.0.1,localhost".to_string());

    OpencodeCommandPlan {
        program: "opencode".to_string(),
        args: vec![
            "run".to_string(),
            "-m".to_string(),
            model.to_string(),
            OPENCODE_TEST_PROMPT.to_string(),
        ],
        envs,
        remove_envs: vec![
            "ANTHROPIC_BASE_URL".to_string(),
            "ANTHROPIC_API_URL".to_string(),
            "CLAUDE_CODE_PROXY".to_string(),
        ],
    }
}

pub fn make_opencode_run_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("opencode-{millis}")
}

pub fn sanitize_command_output(raw: &str) -> String {
    let mut result = Vec::new();
    for line in raw.lines().take(20) {
        let redacted = line
            .split_whitespace()
            .map(|part| {
                if looks_sensitive(part) {
                    "[redacted-secret]".to_string()
                } else {
                    part.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        if !redacted.trim().is_empty() {
            result.push(redacted);
        }
    }

    let mut summary = result.join("\n");
    if raw.lines().count() > 20 {
        summary.push_str("\n[truncated]");
    }
    if summary.len() > MAX_OUTPUT_SUMMARY_LEN {
        summary.truncate(MAX_OUTPUT_SUMMARY_LEN.saturating_sub(12));
        summary.push_str("[truncated]");
    }
    summary
}

fn looks_sensitive(part: &str) -> bool {
    let trimmed = part.trim_matches(|ch: char| {
        ch == '"' || ch == '\'' || ch == ',' || ch == ';' || ch == ')' || ch == '('
    });
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("sk-")
        || lower.starts_with("sk_")
        || lower.starts_with("xox")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("authorization:")
        || lower.contains("bearer")
        || lower.contains("token=")
}

pub fn classify_opencode_failure(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    if lower.contains("model") && (lower.contains("not found") || lower.contains("invalid")) {
        "model_error"
    } else if lower.contains("not found")
        || lower.contains("not recognized")
        || lower.contains("no such file")
        || lower.contains("program not found")
    {
        "missing_tool"
    } else if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("auth")
    {
        "provider_auth"
    } else if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("network")
        || lower.contains("econnreset")
    {
        "network_timeout"
    } else {
        "command_failed"
    }
}

pub fn run_opencode_model_test(model: &str) -> Result<OpencodeTestOutcome, String> {
    let run_id = make_opencode_run_id();
    let plan = opencode_command_plan(model);
    let mut command = Command::new(&plan.program);
    command.args(&plan.args);
    for (key, value) in &plan.envs {
        command.env(key, value);
    }
    for key in &plan.remove_envs {
        command.env_remove(key);
    }

    let output = command
        .output()
        .map_err(|err| format!("{}: {err}", plan.program))?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let output_summary = sanitize_command_output(&text);
    let first_line = output_summary
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let ok = output.status.success() && text.contains("d-token-ok");
    let category = if ok {
        "success"
    } else if output.status.success() {
        "unexpected_response"
    } else {
        classify_opencode_failure(&text)
    }
    .to_string();

    Ok(OpencodeTestOutcome {
        ok,
        run_id,
        model: model.to_string(),
        category,
        output_summary,
        first_line,
    })
}

pub fn opencode_ledger_event(outcome: &OpencodeTestOutcome) -> OpencodeLedgerEvent {
    OpencodeLedgerEvent {
        event_id: format!("evt-{}", outcome.run_id),
        run_id: outcome.run_id.clone(),
        stage: "agent_wrap".to_string(),
        source: "dtoken".to_string(),
        agent: "OpenCode".to_string(),
        route: "none/direct-command".to_string(),
        provider: "OpenCode configured provider".to_string(),
        model: outcome.model.clone(),
        fallback: "disabled".to_string(),
        ok: outcome.ok,
        category: outcome.category.clone(),
        output_summary: sanitize_command_output(&outcome.output_summary),
    }
}

#[derive(Default)]
pub struct ProcessManager {
    children: HashMap<String, Child>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self, instance: &InstanceProfile) -> ProcessStatus {
        if self.children.contains_key(&instance.id) {
            ProcessStatus::Running
        } else {
            instance.process_status.clone()
        }
    }

    pub async fn start_placeholder(&mut self, instance: &InstanceProfile) -> Result<ProcessStatus> {
        tracing::info!(instance_id = %instance.id, "M1 placeholder start requested");
        Ok(ProcessStatus::Starting)
    }

    pub async fn stop(&mut self, instance_id: &str) -> Result<ProcessStatus> {
        if let Some(mut child) = self.children.remove(instance_id) {
            let _ = child.kill().await;
        }
        Ok(ProcessStatus::Stopped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_command_plan_is_isolated() {
        let plan = opencode_command_plan("opencode/deepseek-v4-flash-free");

        assert_eq!(plan.program, "opencode");
        assert_eq!(
            plan.args,
            vec![
                "run",
                "-m",
                "opencode/deepseek-v4-flash-free",
                "Reply with exactly: d-token-ok"
            ]
        );
        assert_eq!(
            plan.envs.get("DTOKEN_TEST_SCOPE").map(String::as_str),
            Some("opencode-only")
        );
        assert!(plan
            .envs
            .get("DTOKEN_TEST_EXCLUDE")
            .is_some_and(|value| value.contains("claude-code") && value.contains("codex-cli")));
        assert_eq!(
            plan.envs.get("NO_PROXY").map(String::as_str),
            Some("127.0.0.1,localhost")
        );
        assert!(plan.remove_envs.contains(&"ANTHROPIC_BASE_URL".to_string()));
        assert!(plan.remove_envs.contains(&"ANTHROPIC_API_URL".to_string()));
        assert!(plan.remove_envs.contains(&"CLAUDE_CODE_PROXY".to_string()));
    }

    #[test]
    fn opencode_output_summary_redacts_secrets_and_limits_size() {
        let raw = "token sk-ant-abc123456789\nAuthorization: Bearer sk-test-secret\n".repeat(40);
        let summary = sanitize_command_output(&raw);

        assert!(!summary.contains("sk-ant-abc123456789"));
        assert!(!summary.contains("sk-test-secret"));
        assert!(summary.contains("[redacted-secret]"));
        assert!(summary.len() <= 900);
    }

    #[test]
    fn opencode_failure_classifier_uses_human_categories() {
        assert_eq!(
            classify_opencode_failure("program not found"),
            "missing_tool"
        );
        assert_eq!(
            classify_opencode_failure("401 unauthorized"),
            "provider_auth"
        );
        assert_eq!(classify_opencode_failure("model not found"), "model_error");
        assert_eq!(
            classify_opencode_failure("request timed out"),
            "network_timeout"
        );
        assert_eq!(classify_opencode_failure("panic"), "command_failed");
    }

    #[test]
    fn opencode_ledger_event_is_truthful_and_redacted() {
        let outcome = OpencodeTestOutcome {
            ok: true,
            run_id: "opencode-test-1".to_string(),
            model: "opencode/big-pickle".to_string(),
            category: "success".to_string(),
            output_summary: sanitize_command_output("d-token-ok\nsecret sk-test-123456"),
            first_line: "d-token-ok".to_string(),
        };

        let event = opencode_ledger_event(&outcome);
        let serialized = serde_json::to_string(&event).expect("event serializes");

        assert_eq!(event.route, "none/direct-command");
        assert_eq!(event.fallback, "disabled");
        assert_eq!(event.stage, "agent_wrap");
        assert!(!serialized.contains("Reply with exactly"));
        assert!(!serialized.contains("sk-test-123456"));
        assert!(serialized.contains("[redacted-secret]"));
    }
}
