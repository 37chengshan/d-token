//! d-token-owned structured command-output filters (PR-C4A).
//!
//! The historical RTK binary filtered command output with its own rules. This
//! module provides the d-token-owned replacement: a small classifier for the
//! git/cargo/npm/pytest/json command families, a TOML DSL that configures each
//! family filter, and structured filters that are deliberately conservative.
//!
//! Every filter fails closed to the original input:
//! - unknown command families are never guessed from output shape,
//! - secret-like, error-like, failed (`exit_code != 0`), and unknown-exit
//!   output is passed through verbatim,
//! - a filter that would not save bytes preserves the input,
//! - JSON is only ever minified losslessly and is preserved when it contains
//!   secret-shaped keys or does not parse,
//! - text truncation is always explicit (`~<bytes>` suffix), never silent.
//!
//! Filters record an evidence-bearing transform list so callers can persist
//! byte-level accounting without persisting command bodies.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{bounded_truncate_marker, strip_ansi};

/// TOML DSL schema version. Receipts and configs are versioned so a future
/// rule change cannot be misread as the reviewed behavior.
pub const FILTER_SCHEMA_VERSION: u8 = 1;

/// The command families with d-token-owned structured filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandFamily {
    Git,
    Cargo,
    Npm,
    Pytest,
    Json,
}

impl CommandFamily {
    /// Classify from the command argv only. Unknown executables are `None`:
    /// the filters never guess a family from output shape alone, because
    /// misclassification could drop bytes the caller expected verbatim.
    pub fn from_argv(argv: &[String]) -> Option<CommandFamily> {
        let program = argv.first()?;
        let base = Path::new(program).file_name()?.to_string_lossy();
        let base = base.strip_suffix(".exe").unwrap_or(&base);
        let base = base.to_ascii_lowercase();
        match base.as_str() {
            "git" => Some(CommandFamily::Git),
            "cargo" | "rustc" => Some(CommandFamily::Cargo),
            "npm" | "npx" | "pnpm" | "yarn" | "corepack" => Some(CommandFamily::Npm),
            "pytest" | "py.test" | "py" => Some(CommandFamily::Pytest),
            "jq" => Some(CommandFamily::Json),
            _ => None,
        }
    }

    /// Classify from the output shape when the argv gave no family. Only a
    /// complete, parseable JSON container is classified as `Json`; every
    /// other shape stays unknown so the input is preserved.
    pub fn from_output(text: &str) -> Option<CommandFamily> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let head = trimmed.as_bytes()[0];
        if head == b'{' || head == b'[' {
            let _ = parse_json_value(trimmed)?;
            return Some(CommandFamily::Json);
        }
        None
    }

    pub fn label(self) -> &'static str {
        match self {
            CommandFamily::Git => "git",
            CommandFamily::Cargo => "cargo",
            CommandFamily::Npm => "npm",
            CommandFamily::Pytest => "pytest",
            CommandFamily::Json => "json",
        }
    }
}

/// Per-family settings in the TOML DSL. `None` fields keep the documented
/// default (enabled, lossless folds, 256 KiB output cap).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FamilyFilterSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub fold_repeated_lines: bool,
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: Option<usize>,
}

impl Default for FamilyFilterSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            fold_repeated_lines: true,
            max_output_bytes: default_max_output_bytes(),
        }
    }
}

/// JSON-family settings. JSON is only minified losslessly; when the input
/// does not parse, contains secret-shaped keys, or minification would exceed
/// the cap, the original text is preserved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JsonFilterSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub minify: bool,
    #[serde(default = "default_true")]
    pub preserve_secret_keys: bool,
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: Option<usize>,
}

impl Default for JsonFilterSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            minify: true,
            preserve_secret_keys: true,
            max_output_bytes: default_max_output_bytes(),
        }
    }
}

/// The TOML DSL document. Unknown top-level keys or fields fail parsing so a
/// typo cannot silently disable a guard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommandFilterConfig {
    pub schema_version: u8,
    #[serde(default)]
    pub git: Option<FamilyFilterSettings>,
    #[serde(default)]
    pub cargo: Option<FamilyFilterSettings>,
    #[serde(default)]
    pub npm: Option<FamilyFilterSettings>,
    #[serde(default)]
    pub pytest: Option<FamilyFilterSettings>,
    #[serde(default)]
    pub json: Option<JsonFilterSettings>,
}

impl CommandFilterConfig {
    pub fn defaults() -> Self {
        Self {
            schema_version: FILTER_SCHEMA_VERSION,
            git: None,
            cargo: None,
            npm: None,
            pytest: None,
            json: None,
        }
    }

    pub fn from_toml(text: &str) -> Result<Self> {
        let config: CommandFilterConfig =
            toml::from_str(text).context("d-token command filter DSL is invalid")?;
        if config.schema_version != FILTER_SCHEMA_VERSION {
            bail!(
                "d-token command filter DSL schema version {} is unsupported",
                config.schema_version
            );
        }
        Ok(config)
    }

    pub fn settings_for(&self, family: CommandFamily) -> Option<&FamilyFilterSettings> {
        match family {
            CommandFamily::Git => self.git.as_ref(),
            CommandFamily::Cargo => self.cargo.as_ref(),
            CommandFamily::Npm => self.npm.as_ref(),
            CommandFamily::Pytest => self.pytest.as_ref(),
            // JSON has its own settings surface (`json_settings`).
            CommandFamily::Json => None,
        }
    }

    /// Effective JSON settings (defaults when the table is absent).
    pub fn json_settings(&self) -> JsonFilterSettings {
        self.json.clone().unwrap_or_default()
    }
}

/// The bounded result of a structured filter. `output` is the filtered text;
/// `preserved` is true exactly when the input passed through byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandFilterResult {
    pub output: String,
    pub family: Option<CommandFamily>,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub transforms: Vec<String>,
    pub preserved: bool,
    pub truncated: bool,
}

/// Filter one completed command output. The command family comes from the
/// argv; when the argv is unavailable or unknown, only a complete parseable
/// JSON document is treated as `Json` and everything else is preserved.
pub fn filter_command_output(
    argv: &[String],
    input: &str,
    exit_code: Option<i32>,
    config: &CommandFilterConfig,
) -> CommandFilterResult {
    if input.is_empty() {
        return CommandFilterResult {
            output: input.to_string(),
            family: None,
            input_bytes: 0,
            output_bytes: 0,
            transforms: vec![],
            preserved: true,
            truncated: false,
        };
    }
    let family = CommandFamily::from_argv(argv).or_else(|| CommandFamily::from_output(input));
    let Some(family) = family else {
        return preserved_with(None, input, "unknown-family");
    };
    // A failed command's output is never rewritten: the caller must see the
    // original failure output (and the wrapper's exit status is authoritative).
    if exit_code.is_some_and(|code| code != 0) {
        return preserved_with(
            Some(family),
            input,
            format!("{}:failure_preserve", family.label()),
        );
    }
    if looks_secret_like(input) {
        return preserved_with(
            Some(family),
            input,
            format!("{}:secret_preserve", family.label()),
        );
    }
    if looks_error_like(input) {
        return preserved_with(
            Some(family),
            input,
            format!("{}:error_preserve", family.label()),
        );
    }
    if family == CommandFamily::Json {
        return filter_json(input, config);
    }
    filter_text_family(family, input, config)
}

fn preserved_with(
    family: Option<CommandFamily>,
    input: &str,
    transform: impl Into<String>,
) -> CommandFilterResult {
    CommandFilterResult {
        output: input.to_string(),
        family,
        input_bytes: input.len(),
        output_bytes: input.len(),
        transforms: vec![transform.into()],
        preserved: true,
        truncated: false,
    }
}

fn filter_text_family(
    family: CommandFamily,
    input: &str,
    config: &CommandFilterConfig,
) -> CommandFilterResult {
    let settings = config
        .settings_for(family)
        .cloned()
        .unwrap_or_default();
    if !settings.enabled {
        return preserved_with(
            Some(family),
            input,
            format!("{}:disabled", family.label()),
        );
    }
    let mut transforms = Vec::new();
    let mut output = input.to_string();

    let without_ansi = strip_ansi(input);
    if without_ansi != input {
        transforms.push(format!("{}:ansi_strip", family.label()));
        output = without_ansi;
    }

    if settings.fold_repeated_lines {
        let folded = fold_exact_repeated_lines(&output);
        if folded != output {
            transforms.push(format!("{}:fold_repeated_lines", family.label()));
            output = folded;
        }
    }

    let limit = settings.max_output_bytes;
    let mut truncated = false;
    if let Some(limit) = limit {
        if output.len() > limit {
            let (bounded, did_truncate) = bounded_truncate_marker(&output, limit);
            output = bounded;
            truncated = did_truncate;
            if did_truncate {
                transforms.push(format!("{}:bounded_truncate", family.label()));
            }
        }
    }

    if output.len() >= input.len() && !truncated {
        return preserved_with(
            Some(family),
            input,
            format!("{}:negative_benefit", family.label()),
        );
    }
    if output == input {
        return preserved_with(
            Some(family),
            input,
            format!("{}:no_change", family.label()),
        );
    }
    let output_bytes = output.len();
    CommandFilterResult {
        output,
        family: Some(family),
        input_bytes: input.len(),
        output_bytes,
        transforms,
        preserved: false,
        truncated,
    }
}

fn filter_json(input: &str, config: &CommandFilterConfig) -> CommandFilterResult {
    let settings = config.json_settings();
    if !settings.enabled || !settings.minify {
        return preserved_with(Some(CommandFamily::Json), input, "json:disabled");
    }
    let Some(value) = parse_json_value(input.trim()) else {
        return preserved_with(Some(CommandFamily::Json), input, "json:unparsed_preserve");
    };
    if settings.preserve_secret_keys && json_has_secret_keys(&value) {
        return preserved_with(Some(CommandFamily::Json), input, "json:secret_keys_preserve");
    }
    let minified = serde_json::to_string(&value).unwrap_or_default();
    if let Some(limit) = settings.max_output_bytes {
        if minified.len() > limit {
            // Truncating minified JSON would break the document; preserve.
            return preserved_with(Some(CommandFamily::Json), input, "json:oversize_preserve");
        }
    }
    if minified.len() >= input.len() {
        return preserved_with(
            Some(CommandFamily::Json),
            input,
            "json:negative_benefit",
        );
    }
    let minified_bytes = minified.len();
    CommandFilterResult {
        output: minified,
        family: Some(CommandFamily::Json),
        input_bytes: input.len(),
        output_bytes: minified_bytes,
        transforms: vec!["json:minify".to_string()],
        preserved: false,
        truncated: false,
    }
}

/// Fold runs of exactly repeated lines into an explicit marker. Unlike the
/// builtin filter this does not touch blank lines or whitespace, so the
/// surviving text is byte-identical to the source minus the folded repeats.
fn fold_exact_repeated_lines(text: &str) -> String {
    let mut lines = Vec::new();
    let mut previous: Option<&str> = None;
    let mut repeats = 0usize;
    for line in text.lines() {
        if previous == Some(line) {
            repeats += 1;
            continue;
        }
        if repeats > 0 {
            lines.push(format!(
                "[d-token: previous line repeated {repeats} times]"
            ));
            repeats = 0;
        }
        lines.push(line.to_string());
        previous = Some(line);
    }
    if repeats > 0 {
        lines.push(format!("[d-token: previous line repeated {repeats} times]"));
    }
    let mut output = lines.join("\n");
    if text.ends_with('\n') && !output.is_empty() {
        output.push('\n');
    }
    output
}

fn parse_json_value(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(text).ok()
}

/// Recursively checks JSON keys for secret-shaped names. Conservative: any
/// key containing a credential marker preserves the whole document.
fn json_has_secret_keys(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            let key_lower = key.to_ascii_lowercase();
            JSON_SECRET_KEY_MARKERS
                .iter()
                .any(|marker| key_lower.contains(marker))
                || json_has_secret_keys(value)
        }),
        serde_json::Value::Array(items) => items.iter().any(json_has_secret_keys),
        _ => false,
    }
}

const JSON_SECRET_KEY_MARKERS: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "secret",
    "password",
    "passwd",
    "token",
    "authorization",
    "private_key",
    "private-key",
    "client_secret",
    "credential",
];

/// Conservative secret detection for text output. Any hit preserves the
/// whole output; false positives are safe (pass-through), false negatives
/// are not. Env-style assignments (`GITHUB_TOKEN=…`, `AWS_SECRET_KEY=…`)
/// are matched structurally, not just by literal markers.
fn looks_secret_like(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if TEXT_SECRET_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return true;
    }
    text.lines().any(|line| {
        let trimmed = line.trim();
        let Some(eq) = trimmed.find('=') else {
            return false;
        };
        let key = &trimmed[..eq];
        if key.is_empty() || key.len() > 64 {
            return false;
        }
        let uppercase = key.to_ascii_uppercase();
        uppercase.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
        }) && (uppercase.ends_with("_TOKEN")
            || uppercase.ends_with("_KEY")
            || uppercase.ends_with("_SECRET")
            || uppercase.ends_with("_PASSWORD")
            || uppercase.ends_with("_PASSWD"))
    })
}

const TEXT_SECRET_MARKERS: &[&str] = &[
    "api_key",
    "api-key",
    "apikey",
    "secret",
    "password",
    "passwd",
    "client_secret",
    "private_key",
    "private-key",
    "x-api-key",
    "authorization:",
    "bearer ",
    "begin rsa private",
    "begin openssh private",
    "begin private",
    "aws_access_key",
    "aws_secret_access_key",
];

/// Conservative error detection. Error-shaped output is preserved so the
/// caller sees the original failure output.
fn looks_error_like(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ERROR_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

const ERROR_MARKERS: &[&str] = &[
    "error:",
    "error[",
    "fatal:",
    "traceback (most recent call last):",
    "panicked at ",
    "thread 'main' panicked",
    "assertionerror",
    "command not found",
    "no such file or directory",
    "permission denied",
    "exit status",
    "npm err!",
    "failed to ",
    "cannot find ",
    "unrecognized ",
    "invalid option",
    "e   assert",
    "1 failed",
    "test session fails",
];

fn default_true() -> bool {
    true
}

fn default_max_output_bytes() -> Option<usize> {
    Some(256 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config() -> CommandFilterConfig {
        CommandFilterConfig::defaults()
    }

    fn git_config() -> CommandFilterConfig {
        CommandFilterConfig::defaults()
    }

    #[test]
    fn family_classification_uses_argv_basename_and_exact_json_shape() {
        assert_eq!(
            CommandFamily::from_argv(&["git".to_string(), "status".to_string()]),
            Some(CommandFamily::Git)
        );
        assert_eq!(
            CommandFamily::from_argv(&["/usr/bin/cargo".to_string()]),
            Some(CommandFamily::Cargo)
        );
        assert_eq!(
            CommandFamily::from_argv(&["npm".to_string()]),
            Some(CommandFamily::Npm)
        );
        assert_eq!(
            CommandFamily::from_argv(&["pytest".to_string()]),
            Some(CommandFamily::Pytest)
        );
        assert_eq!(
            CommandFamily::from_argv(&["jq".to_string()]),
            Some(CommandFamily::Json)
        );
        assert_eq!(CommandFamily::from_argv(&["curl".to_string()]), None);
        assert_eq!(CommandFamily::from_argv(&[]), None);

        assert_eq!(
            CommandFamily::from_output(r#"{"a": 1}"#),
            Some(CommandFamily::Json)
        );
        assert_eq!(
            CommandFamily::from_output("not json at all"),
            None
        );
        assert_eq!(CommandFamily::from_output(""), None);
    }

    #[test]
    fn git_filter_folds_repeats_and_strips_ansi() {
        let input = "\u{1b}[31mReceiving objects:  10%\u{1b}[0m\nReceiving objects:  10%\nReceiving objects:  10%\n";
        let result = filter_command_output(
            &["git".to_string(), "fetch".to_string()],
            input,
            Some(0),
            &git_config(),
        );
        assert!(!result.preserved);
        assert!(result.output.contains("previous line repeated"));
        assert!(!result.output.contains("\u{1b}[31m"));
        assert!(result.transforms.contains(&"git:ansi_strip".to_string()));
        assert!(result
            .transforms
            .contains(&"git:fold_repeated_lines".to_string()));
        assert!(result.output_bytes < result.input_bytes);
    }

    #[test]
    fn cargo_filter_folds_repeated_compile_lines() {
        let input = "Compiling serde v1.0.0\nCompiling serde v1.0.0\nCompiling serde v1.0.0\nFinished `dev` profile\n";
        let result = filter_command_output(
            &["cargo".to_string(), "build".to_string()],
            input,
            Some(0),
            &config(),
        );
        assert!(!result.preserved);
        assert!(result.output.contains("previous line repeated 2 times"));
        assert!(result.output.contains("Finished `dev` profile"));
        assert_eq!(result.output.lines().count(), 3);
    }

    #[test]
    fn npm_filter_folds_repeated_warning_lines() {
        let input = "npm warn deprecated a@1.0.0\n".repeat(5) + "added 1 package\n";
        let result = filter_command_output(
            &["npm".to_string(), "install".to_string()],
            &input,
            Some(0),
            &config(),
        );
        assert!(!result.preserved);
        assert!(result.output.contains("previous line repeated 4 times"));
        assert!(result.output.contains("added 1 package"));
    }

    #[test]
    fn pytest_filter_keeps_summary_and_folds_repeats() {
        let input = "test_a PASSED\n".repeat(6) + "1 passed in 0.01s\n";
        let result = filter_command_output(
            &["pytest".to_string()],
            &input,
            Some(0),
            &config(),
        );
        assert!(!result.preserved);
        assert!(result.output.contains("1 passed in 0.01s"));
        assert!(result.output.contains("previous line repeated 5 times"));
    }

    #[test]
    fn json_filter_minifies_losslessly_and_round_trips() {
        let pretty = serde_json::to_string_pretty(&json!({"key": [1, 2, {"nested": "value"}]}))
            .expect("pretty JSON");
        let result = filter_command_output(
            &["jq".to_string(), ".".to_string()],
            &pretty,
            Some(0),
            &config(),
        );
        assert!(!result.preserved);
        assert_eq!(result.transforms, vec!["json:minify".to_string()]);
        assert_eq!(result.output.find('\n'), None);
        let parsed: serde_json::Value =
            serde_json::from_str(&result.output).expect("minified JSON still parses");
        assert_eq!(parsed, json!({"key": [1, 2, {"nested": "value"}]}));
    }

    #[test]
    fn json_filter_preserves_secret_keyed_documents() {
        let secret = r#"{"credential_file": "/etc/x", "ok": true}"#;
        let result = filter_command_output(
            &["jq".to_string(), ".".to_string()],
            secret,
            Some(0),
            &config(),
        );
        assert!(result.preserved);
        assert_eq!(result.output, secret);
        assert!(result.transforms.contains(&"json:secret_keys_preserve".to_string()));
    }

    #[test]
    fn json_filter_preserves_unparsed_and_oversize_output() {
        let unparsed = "{ not json";
        let result = filter_command_output(
            &["jq".to_string(), ".".to_string()],
            unparsed,
            Some(0),
            &config(),
        );
        assert!(result.preserved);
        assert_eq!(result.output, unparsed);

        let mut config = CommandFilterConfig::defaults();
        config.json = Some(JsonFilterSettings {
            enabled: true,
            minify: true,
            preserve_secret_keys: true,
            max_output_bytes: Some(16),
        });
        let big = serde_json::to_string(&json!({"a": "x".repeat(64)})).expect("JSON");
        let result = filter_command_output(
            &["jq".to_string(), ".".to_string()],
            &big,
            Some(0),
            &config,
        );
        assert!(result.preserved);
        assert_eq!(result.output, big);
    }

    #[test]
    fn failed_and_error_shaped_output_is_never_rewritten() {
        let input = "error: could not compile `demo`\nCompiling demo v0.1.0\nCompiling demo v0.1.0\n";
        let failed = filter_command_output(
            &["cargo".to_string(), "build".to_string()],
            input,
            Some(101),
            &config(),
        );
        assert!(failed.preserved);
        assert_eq!(failed.output, input);
        assert!(failed
            .transforms
            .contains(&"cargo:failure_preserve".to_string()));

        let error_shaped = filter_command_output(
            &["cargo".to_string(), "build".to_string()],
            input,
            Some(0),
            &config(),
        );
        assert!(error_shaped.preserved);
        assert_eq!(error_shaped.output, input);
        assert!(error_shaped
            .transforms
            .contains(&"cargo:error_preserve".to_string()));
    }

    #[test]
    fn secret_shaped_output_is_preserved_for_every_family() {
        let input = "GITHUB_TOKEN=ghp_abcdef\nSome line\nSome line\n";
        for family in [
            CommandFamily::Git,
            CommandFamily::Cargo,
            CommandFamily::Npm,
            CommandFamily::Pytest,
        ] {
            let argv = vec![family.label().to_string()];
            let result =
                filter_command_output(&argv, input, Some(0), &config());
            assert!(result.preserved, "{family:?}");
            assert_eq!(result.output, input);
            assert!(result
                .transforms
                .contains(&format!("{}:secret_preserve", family.label())));
        }
    }

    #[test]
    fn unknown_family_and_unknown_exit_are_preserved() {
        let input = "Some line\n".repeat(10);
        let unknown = filter_command_output(
            &["curl".to_string(), "-s".to_string()],
            &input,
            Some(0),
            &config(),
        );
        assert!(unknown.preserved);
        assert_eq!(unknown.output, input);
        assert_eq!(unknown.transforms, vec!["unknown-family".to_string()]);

        let unknown_exit = filter_command_output(
            &["git".to_string(), "fetch".to_string()],
            &input,
            None,
            &config(),
        );
        assert!(!unknown_exit.preserved);
        assert!(unknown_exit
            .transforms
            .contains(&"git:fold_repeated_lines".to_string()));
    }

    #[test]
    fn negative_benefit_preserves_input() {
        let input = "a\nb\n";
        let result = filter_command_output(
            &["git".to_string(), "status".to_string()],
            input,
            Some(0),
            &config(),
        );
        assert!(result.preserved);
        assert_eq!(result.output, input);
        assert!(result
            .transforms
            .contains(&"git:negative_benefit".to_string()));
    }

    #[test]
    fn text_truncation_is_explicit_and_bounded() {
        let mut config = CommandFilterConfig::defaults();
        config.git = Some(FamilyFilterSettings {
            enabled: true,
            fold_repeated_lines: true,
            max_output_bytes: Some(64),
        });
        let input = "line one\nline two\nline three\n".repeat(20);
        let result = filter_command_output(
            &["git".to_string(), "log".to_string()],
            &input,
            Some(0),
            &config,
        );
        assert!(!result.preserved);
        assert!(result.truncated);
        assert!(result.output.len() <= 64);
        assert!(result.output.contains('~'));
        assert!(result
            .transforms
            .contains(&"git:bounded_truncate".to_string()));
    }

    #[test]
    fn disabled_family_preserves_input() {
        let mut config = CommandFilterConfig::defaults();
        config.git = Some(FamilyFilterSettings {
            enabled: false,
            fold_repeated_lines: true,
            max_output_bytes: None,
        });
        let input = "same\nsame\n";
        let result = filter_command_output(
            &["git".to_string(), "status".to_string()],
            input,
            Some(0),
            &config,
        );
        assert!(result.preserved);
        assert_eq!(result.output, input);
        assert_eq!(result.transforms, vec!["git:disabled".to_string()]);
    }

    #[test]
    fn toml_dsl_parses_and_rejects_unknown_fields() {
        let dsl = r#"
schema_version = 1

[git]
enabled = true
fold_repeated_lines = false
max_output_bytes = 131072

[cargo]
enabled = true

[pytest]
enabled = true
"#;
        let parsed = CommandFilterConfig::from_toml(dsl).expect("valid DSL");
        let git = parsed.git.expect("git table");
        assert!(!git.fold_repeated_lines);
        assert_eq!(git.max_output_bytes, Some(131_072));
        assert_eq!(parsed.cargo.expect("cargo table").enabled, true);

        let invalid = "schema_version = 1\n[git]\nunknown_setting = true\n";
        assert!(CommandFilterConfig::from_toml(invalid).is_err());

        let wrong_version = "schema_version = 9\n";
        let error = CommandFilterConfig::from_toml(wrong_version).expect_err("version");
        assert!(error.to_string().contains("schema version 9"));
    }

    #[test]
    fn structured_filters_never_panic_on_arbitrary_unicode_and_bytes() {
        let repeated = "x".repeat(300_000);
        let corpus = [
            "",
            " ",
            "\n\n\n",
            "\u{1b}[1;31m\u{1b}[0m",
            "日本語の行\n日本語の行\n日本語の行\n",
            "a\r\nb\r\nb\r\n",
            "{\"a\": [1, 2, {\"b\": \"c\"}]}",
            "error: boom\nerror: boom\n",
            repeated.as_str(),
            "\u{0}\u{1b}[38;5;196mx\u{0}",
        ];
        for input in corpus {
            for family in [
                CommandFamily::Git,
                CommandFamily::Cargo,
                CommandFamily::Npm,
                CommandFamily::Pytest,
                CommandFamily::Json,
            ] {
                let argv = vec![family.label().to_string()];
                let result =
                    filter_command_output(&argv, input, Some(0), &config());
                if result.preserved {
                    assert_eq!(result.output, input);
                } else {
                    assert!(result.output.len() <= input.len().max(1) * 2 + 256);
                }
                assert!(result.output_bytes == result.output.len());
            }
        }
    }
}
