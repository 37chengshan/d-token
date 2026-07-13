use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const CC_SWITCH_SCHEMA_VERSION: i64 = 12;
const CC_SWITCH_COMMIT: &str = "f39d463c442e705727531b85f2db98e00ccaf11e";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpstreamCandidate {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub models: Vec<String>,
    pub source: String,
    pub freshness: String,
    pub auth_mode: String,
    pub active: bool,
    pub port: Option<u16>,
    pub state: String,
}

pub fn discover() -> Value {
    let mut candidates = Vec::new();
    let mut sources = Vec::new();
    discover_opencode_catalog(&mut candidates, &mut sources);
    discover_normal_opencode_config(&mut candidates, &mut sources);
    discover_cc_switch(&mut candidates, &mut sources);
    candidates.sort_by(|left, right| left.source.cmp(&right.source).then(left.id.cmp(&right.id)));
    candidates.dedup_by(|left, right| {
        left.source == right.source && left.id == right.id && left.models == right.models
    });
    json!({
        "source": "dtoken-desktop-read-only-discovery",
        "freshness": now_timestamp(),
        "state": if candidates.is_empty() { "unavailable" } else { "loaded" },
        "candidates": candidates,
        "sources": sources,
        "selection_required": true,
        "writes_performed": false,
        "cc_switch_reference": {
            "repository": "https://github.com/farion1231/cc-switch",
            "commit": CC_SWITCH_COMMIT,
            "license": "MIT",
            "schema_version": CC_SWITCH_SCHEMA_VERSION
        },
        "privacy": "Reads configuration metadata only; auth.json and credential/api_key/token/auth/cookie values are never read or returned"
    })
}

fn discover_opencode_catalog(candidates: &mut Vec<UpstreamCandidate>, sources: &mut Vec<Value>) {
    let output = Command::new("opencode")
        .args(["models", "opencode"])
        .env_remove("OPENCODE_CONFIG_CONTENT")
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let models = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter_map(|line| line.strip_prefix("opencode/"))
                .filter(|model| !model.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if !models.is_empty() {
                candidates.push(UpstreamCandidate {
                    id: "opencode".to_string(),
                    name: "OpenCode official free catalog".to_string(),
                    protocol: "open_ai".to_string(),
                    base_url: Some("https://opencode.ai/zen/v1".to_string()),
                    models,
                    source: "opencode-provider-catalog".to_string(),
                    freshness: now_timestamp(),
                    auth_mode: "none".to_string(),
                    active: true,
                    port: Some(443),
                    state: "available".to_string(),
                });
            }
            sources.push(json!({"id":"opencode-provider-catalog","state":"loaded","freshness":now_timestamp()}));
        }
        Ok(_) => sources.push(json!({"id":"opencode-provider-catalog","state":"unavailable","freshness":now_timestamp()})),
        Err(_) => sources.push(json!({"id":"opencode-provider-catalog","state":"missing","freshness":now_timestamp()})),
    }
}

fn discover_normal_opencode_config(
    candidates: &mut Vec<UpstreamCandidate>,
    sources: &mut Vec<Value>,
) {
    let Some(path) = normal_opencode_config_path() else {
        sources.push(json!({"id":"opencode-normal-config","state":"unavailable","freshness":now_timestamp()}));
        return;
    };
    if path.file_name().and_then(|name| name.to_str()) == Some("auth.json") || !path.is_file() {
        sources.push(
            json!({"id":"opencode-normal-config","state":"missing","freshness":now_timestamp()}),
        );
        return;
    }
    let raw = match fs::read(&path) {
        Ok(raw) => raw,
        Err(_) => {
            sources.push(json!({"id":"opencode-normal-config","state":"unavailable","freshness":now_timestamp()}));
            return;
        }
    };
    let value: Value = match serde_json::from_slice(&raw) {
        Ok(value) => value,
        Err(_) => {
            sources.push(json!({"id":"opencode-normal-config","state":"invalid","freshness":now_timestamp()}));
            return;
        }
    };
    if let Some(providers) = value.get("provider").and_then(Value::as_object) {
        for (id, provider) in providers {
            if let Some(candidate) =
                candidate_from_json(id, provider, "opencode-normal-config", false, None)
            {
                candidates.push(candidate);
            }
        }
    }
    sources.push(json!({"id":"opencode-normal-config","state":"loaded","freshness":now_timestamp(),"path":display_path(&path)}));
}

fn discover_cc_switch(candidates: &mut Vec<UpstreamCandidate>, sources: &mut Vec<Value>) {
    let Some(path) = cc_switch_db_path() else {
        sources.push(cc_switch_source("missing", None, None));
        return;
    };
    discover_cc_switch_at(&path, candidates, sources);
}

fn discover_cc_switch_at(
    path: &Path,
    candidates: &mut Vec<UpstreamCandidate>,
    sources: &mut Vec<Value>,
) {
    if !path.is_file() {
        sources.push(cc_switch_source("missing", Some(path), None));
        return;
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = match Connection::open_with_flags(path, flags) {
        Ok(connection) => connection,
        Err(_) => {
            sources.push(cc_switch_source("locked_or_corrupt", Some(path), None));
            return;
        }
    };
    let version = match connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
    {
        Ok(version) => version,
        Err(_) => {
            sources.push(cc_switch_source("locked_or_corrupt", Some(path), None));
            return;
        }
    };
    if version > CC_SWITCH_SCHEMA_VERSION {
        sources.push(cc_switch_source(
            "unsupported_schema",
            Some(path),
            Some(version),
        ));
        return;
    }
    let mut statement = match connection.prepare(
        "SELECT id, app_type, name, is_current,
                COALESCE(json_extract(settings_config,'$.base_url'), json_extract(settings_config,'$.baseUrl'), json_extract(settings_config,'$.options.baseURL'), json_extract(settings_config,'$.env.ANTHROPIC_BASE_URL'), json_extract(settings_config,'$.env.OPENAI_BASE_URL'), json_extract(settings_config,'$.env.GEMINI_BASE_URL')),
                COALESCE(json_extract(settings_config,'$.protocol'), json_extract(settings_config,'$.apiFormat'), json_extract(settings_config,'$.api_format')),
                COALESCE(json_extract(settings_config,'$.model'), json_extract(settings_config,'$.model_name'), json_extract(settings_config,'$.env.ANTHROPIC_MODEL'), json_extract(settings_config,'$.env.OPENAI_MODEL'), json_extract(settings_config,'$.env.GEMINI_MODEL')),
                CASE WHEN json_type(settings_config,'$.api_key') IS NOT NULL OR json_type(settings_config,'$.apiKey') IS NOT NULL OR json_type(settings_config,'$.token') IS NOT NULL OR json_type(settings_config,'$.auth') IS NOT NULL OR json_type(settings_config,'$.env.ANTHROPIC_AUTH_TOKEN') IS NOT NULL OR json_type(settings_config,'$.env.OPENAI_API_KEY') IS NOT NULL OR json_type(settings_config,'$.env.GEMINI_API_KEY') IS NOT NULL THEN 1 ELSE 0 END
         FROM providers WHERE app_type IN ('claude','codex','gemini','opencode') ORDER BY app_type, sort_index, id"
    ) {
        Ok(statement) => statement,
        Err(_) => {
            sources.push(cc_switch_source("invalid_schema", Some(path), Some(version)));
            return;
        }
    };
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, bool>(7)?,
        ))
    });
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            let (id, app_type, name, active, base_url, protocol, model, has_auth_field) = row;
            if base_url.is_none() && model.is_none() {
                continue;
            }
            candidates.push(UpstreamCandidate {
                id,
                name,
                protocol: protocol.unwrap_or_else(|| protocol_for_app(&app_type).to_string()),
                base_url,
                models: model.into_iter().collect(),
                source: format!("cc-switch:{app_type}"),
                freshness: now_timestamp(),
                auth_mode: if has_auth_field {
                    "unavailable"
                } else {
                    "none"
                }
                .to_string(),
                active,
                port: None,
                state: "discovered".to_string(),
            });
        }
    }
    if let Ok(mut proxy) =
        connection.prepare("SELECT app_type, listen_port, enabled, proxy_enabled FROM proxy_config")
    {
        if let Ok(rows) = proxy.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u16>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, bool>(3)?,
            ))
        }) {
            for row in rows.flatten() {
                let (app_type, port, enabled, proxy_enabled) = row;
                candidates.push(UpstreamCandidate {
                    id: format!("cc-switch-proxy-{app_type}"),
                    name: format!("cc-switch {app_type} proxy"),
                    protocol: protocol_for_app(&app_type).to_string(),
                    base_url: Some(format!("http://127.0.0.1:{port}")),
                    models: vec![],
                    source: "cc-switch:proxy_config".to_string(),
                    freshness: now_timestamp(),
                    auth_mode: "none".to_string(),
                    active: enabled && proxy_enabled,
                    port: Some(port),
                    state: if enabled && proxy_enabled {
                        "running_configured"
                    } else {
                        "stopped"
                    }
                    .to_string(),
                });
            }
        }
    }
    let profile_count = connection
        .query_row("SELECT COUNT(*) FROM profiles", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0);
    sources.push(json!({"id":"cc-switch-sqlite","state":"loaded","freshness":now_timestamp(),"path":display_path(path),"schema_version":version,"profile_count":profile_count,"mode":"read_only"}));
}

fn candidate_from_json(
    id: &str,
    value: &Value,
    source: &str,
    active: bool,
    port: Option<u16>,
) -> Option<UpstreamCandidate> {
    let base_url = find_safe_string(value, &["baseurl", "base_url", "apiurl", "api_url"]);
    let models = find_models(value);
    if base_url.is_none() && models.is_empty() {
        return None;
    }
    let protocol = find_safe_string(value, &["protocol", "apiformat", "api_format"])
        .unwrap_or_else(|| {
            if source.contains("claude") {
                "anthropic".to_string()
            } else {
                "open_ai".to_string()
            }
        });
    Some(UpstreamCandidate {
        id: id.to_string(),
        name: id.to_string(),
        protocol,
        base_url,
        models,
        source: source.to_string(),
        freshness: now_timestamp(),
        auth_mode: auth_mode(value),
        active,
        port,
        state: "discovered".to_string(),
    })
}

fn find_safe_string(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    for (key, nested) in object {
        let normalized = key.to_ascii_lowercase();
        if keys.contains(&normalized.as_str()) {
            if let Some(text) = nested.as_str() {
                return Some(text.to_string());
            }
        }
        if is_sensitive_key(key) {
            continue;
        }
        if let Some(found) = find_safe_string(nested, keys) {
            return Some(found);
        }
    }
    None
}

fn find_models(value: &Value) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return vec![];
    };
    for (key, nested) in object {
        if is_sensitive_key(key) {
            continue;
        }
        let normalized = key.to_ascii_lowercase();
        if normalized == "model" {
            if let Some(model) = nested.as_str() {
                return vec![model.to_string()];
            }
        }
        if normalized == "models" {
            if let Some(models) = nested.as_object() {
                return models.keys().cloned().collect();
            }
            if let Some(models) = nested.as_array() {
                return models
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
            }
        }
        let found = find_models(nested);
        if !found.is_empty() {
            return found;
        }
    }
    vec![]
}

fn auth_mode(value: &Value) -> String {
    let Some(object) = value.as_object() else {
        return "none".to_string();
    };
    for (key, nested) in object {
        if is_sensitive_key(key) {
            return nested
                .as_str()
                .filter(|text| {
                    text.starts_with("keychain://") || text.starts_with("os-credential://")
                })
                .map(|_| "os-ref".to_string())
                .unwrap_or_else(|| "unavailable".to_string());
        }
        let nested_mode = auth_mode(nested);
        if nested_mode != "none" {
            return nested_mode;
        }
    }
    "none".to_string()
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    [
        "credential",
        "apikey",
        "token",
        "authorization",
        "auth",
        "cookie",
        "secret",
        "password",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn normal_opencode_config_path() -> Option<PathBuf> {
    env::var_os("DTOKEN_OPENCODE_CONFIG_PATH")
        .or_else(|| env::var_os("OPENCODE_CONFIG"))
        .map(PathBuf::from)
        .or_else(|| {
            let home = env::var_os("HOME")?;
            let root = env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(home).join(".config"));
            Some(root.join("opencode").join("opencode.json"))
        })
}

fn cc_switch_db_path() -> Option<PathBuf> {
    if let Some(home) = env::var_os("CC_SWITCH_TEST_HOME") {
        return Some(PathBuf::from(home).join(".cc-switch").join("cc-switch.db"));
    }
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|home| home.join(".cc-switch").join("cc-switch.db"))
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".cc-switch").join("cc-switch.db"))
    }
}

fn cc_switch_source(state: &str, path: Option<&Path>, version: Option<i64>) -> Value {
    json!({"id":"cc-switch-sqlite","state":state,"freshness":now_timestamp(),"path":path.map(display_path),"schema_version":version,"mode":"read_only"})
}
fn protocol_for_app(app: &str) -> &'static str {
    if app == "claude" {
        "anthropic"
    } else {
        "open_ai"
    }
}
fn display_path(path: &Path) -> String {
    env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|home| {
            path.strip_prefix(home)
                .ok()
                .map(|relative| format!("~/{}", relative.display()))
        })
        .unwrap_or_else(|| {
            format!(
                "<local>/{}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("config")
            )
        })
}
fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| format!("unix:{}", duration.as_secs()))
        .unwrap_or_else(|_| "unix:0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn source_state(sources: &[Value]) -> &str {
        sources
            .first()
            .and_then(|source| source.get("state"))
            .and_then(Value::as_str)
            .expect("cc-switch source state")
    }

    fn create_schema_12(path: &Path) -> Connection {
        let connection = Connection::open(path).expect("create cc-switch fixture database");
        connection
            .execute_batch(
                "PRAGMA user_version = 12;
                 CREATE TABLE providers (
                   id TEXT PRIMARY KEY,
                   app_type TEXT NOT NULL,
                   name TEXT NOT NULL,
                   settings_config TEXT NOT NULL,
                   is_current INTEGER NOT NULL DEFAULT 0,
                   sort_index INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE provider_endpoints (
                   id TEXT PRIMARY KEY,
                   provider_id TEXT NOT NULL,
                   endpoint TEXT NOT NULL
                 );
                 CREATE TABLE proxy_config (
                   app_type TEXT PRIMARY KEY,
                   listen_port INTEGER NOT NULL,
                   enabled INTEGER NOT NULL DEFAULT 0,
                   proxy_enabled INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE profiles (
                   id TEXT PRIMARY KEY,
                   name TEXT NOT NULL
                 );",
            )
            .expect("create schema 12 tables");
        connection
    }

    #[test]
    fn official_catalog_shape_is_no_auth_and_explicit() {
        let value = discover();
        assert_eq!(value["selection_required"], true);
        assert_eq!(value["writes_performed"], false);
    }
    #[test]
    fn sensitive_values_are_never_returned() {
        let value = json!({"baseURL":"https://example.test/v1","apiKey":"literal-secret","models":{"m":{}}});
        let candidate = candidate_from_json("x", &value, "fixture", false, None).unwrap();
        assert_eq!(candidate.auth_mode, "unavailable");
        assert!(!serde_json::to_string(&candidate)
            .unwrap()
            .contains("literal-secret"));
    }

    #[test]
    fn cc_switch_missing_database_is_reported_without_candidates() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("missing.db");
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(source_state(&sources), "missing");
        assert_eq!(sources[0]["mode"], "read_only");
    }

    #[test]
    fn cc_switch_corrupt_database_fails_closed() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        fs::write(&path, b"not a sqlite database").expect("write corrupt fixture");
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(source_state(&sources), "locked_or_corrupt");
        assert!(sources[0]["schema_version"].is_null());
    }

    #[test]
    fn cc_switch_locked_database_fails_closed() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let owner = create_schema_12(&path);
        owner
            .execute_batch("BEGIN EXCLUSIVE; UPDATE profiles SET name = name;")
            .expect("hold exclusive database lock");
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(source_state(&sources), "locked_or_corrupt");
        owner
            .execute_batch("ROLLBACK")
            .expect("release fixture lock");
    }

    #[test]
    fn cc_switch_newer_schema_is_rejected_before_table_reads() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let connection = Connection::open(&path).expect("create future schema fixture");
        connection
            .execute_batch("PRAGMA user_version = 13;")
            .expect("set future schema version");
        drop(connection);
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(source_state(&sources), "unsupported_schema");
        assert_eq!(sources[0]["schema_version"], 13);
    }

    #[test]
    fn cc_switch_schema_12_returns_multiple_safe_candidates_without_secret_payloads() {
        const SECRET_MARKER: &str = "CC_SWITCH_SECRET_MUST_NOT_ESCAPE_8f31";
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let connection = create_schema_12(&path);
        connection
            .execute(
                "INSERT INTO providers
                 (id, app_type, name, settings_config, is_current, sort_index)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    "claude-primary",
                    "claude",
                    "Claude primary",
                    json!({
                        "env": {
                            "ANTHROPIC_BASE_URL": "https://claude.example.test/v1",
                            "ANTHROPIC_MODEL": "claude-test",
                            "ANTHROPIC_AUTH_TOKEN": SECRET_MARKER
                        },
                        "private_metadata": SECRET_MARKER
                    })
                    .to_string(),
                    true,
                    1
                ],
            )
            .expect("insert claude provider");
        connection
            .execute(
                "INSERT INTO providers
                 (id, app_type, name, settings_config, is_current, sort_index)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    "opencode-secondary",
                    "opencode",
                    "OpenCode secondary",
                    json!({
                        "baseUrl": "https://opencode.example.test/v1",
                        "protocol": "open_ai",
                        "model": "safe-model"
                    })
                    .to_string(),
                    false,
                    2
                ],
            )
            .expect("insert opencode provider");
        connection
            .execute(
                "INSERT INTO proxy_config (app_type, listen_port, enabled, proxy_enabled)
                 VALUES ('codex', 15721, 1, 1)",
                [],
            )
            .expect("insert proxy config");
        connection
            .execute(
                "INSERT INTO profiles (id, name) VALUES ('one', 'One'), ('two', 'Two')",
                [],
            )
            .expect("insert profiles");
        drop(connection);
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert_eq!(source_state(&sources), "loaded");
        assert_eq!(sources[0]["schema_version"], 12);
        assert_eq!(sources[0]["profile_count"], 2);
        assert_eq!(candidates.len(), 3);
        let claude = candidates
            .iter()
            .find(|candidate| candidate.id == "claude-primary")
            .expect("claude candidate");
        assert_eq!(claude.protocol, "anthropic");
        assert_eq!(claude.models, ["claude-test"]);
        assert_eq!(claude.auth_mode, "unavailable");
        assert!(claude.active);
        let opencode = candidates
            .iter()
            .find(|candidate| candidate.id == "opencode-secondary")
            .expect("opencode candidate");
        assert_eq!(opencode.models, ["safe-model"]);
        assert_eq!(opencode.auth_mode, "none");
        let proxy = candidates
            .iter()
            .find(|candidate| candidate.id == "cc-switch-proxy-codex")
            .expect("proxy candidate");
        assert_eq!(proxy.base_url.as_deref(), Some("http://127.0.0.1:15721"));
        assert!(proxy.active);

        let returned = serde_json::to_string(&(candidates, sources)).expect("serialize discovery");
        assert!(!returned.contains(SECRET_MARKER));
        assert!(!returned.contains("private_metadata"));
    }
}
