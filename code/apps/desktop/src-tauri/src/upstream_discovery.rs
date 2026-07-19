#[cfg(test)]
use crate::agent_registry::AGENT_REGISTRY;
use crate::agent_registry::{
    agent_descriptor, scan_current_process_agents, AgentDescriptor, AgentEvidenceKind,
    AgentInstallCapability,
};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
    pub agent_ids: Vec<String>,
    pub confidence: u8,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentUpstreamBinding {
    pub agent_id: String,
    pub agent_name: String,
    pub installed: bool,
    pub binary_path: Option<String>,
    pub config_path: Option<String>,
    pub config_paths: Vec<String>,
    pub current_config_source: String,
    pub candidates: Vec<UpstreamCandidate>,
    pub resolved_candidate: Option<UpstreamCandidate>,
    pub resolution: String,
    pub confidence: u8,
    pub reasons: Vec<String>,
    pub capability: String,
    pub already_connected: bool,
}

pub fn discover() -> Value {
    let mut candidates = Vec::new();
    let mut sources = Vec::new();
    let agent_discovery = scan_current_process_agents();
    discover_opencode_catalog(&mut candidates, &mut sources);
    if let Ok(snapshot) = &agent_discovery {
        discover_agent_configs(&snapshot.agents, &mut candidates, &mut sources);
    }
    discover_cc_switch(&mut candidates, &mut sources);
    normalize_cc_switch_proxy_candidates(&mut candidates);
    candidates.sort_by(|left, right| left.source.cmp(&right.source).then(left.id.cmp(&right.id)));
    candidates.dedup_by(|left, right| {
        left.source == right.source && left.id == right.id && left.models == right.models
    });
    let (agent_bindings, agent_scan) = match agent_discovery {
        Ok(snapshot) => {
            let status = json!({
                "state": "loaded",
                "freshness": format!("unix-ms:{}", snapshot.observed_at_ms),
                "evidence_revision": snapshot.evidence_revision,
                "detected_count": snapshot.agents.len(),
            });
            (
                resolve_agent_bindings(&candidates, &snapshot.agents),
                status,
            )
        }
        Err(error) => (
            Vec::new(),
            json!({
                "state": "unavailable",
                "freshness": now_timestamp(),
                "reason": error.to_string(),
            }),
        ),
    };
    let selection_required = agent_bindings.iter().any(|binding| {
        binding.installed
            && matches!(
                binding.resolution.as_str(),
                "needs-choice" | "needs-credential"
            )
    });
    let cc_switch_sync = cc_switch_sync_state(&sources, &candidates);
    let revision = discovery_revision(&agent_bindings, &cc_switch_sync);
    json!({
        "source": "dtoken-desktop-read-only-discovery",
        "freshness": now_timestamp(),
        "revision": revision,
        "state": if candidates.is_empty() && agent_bindings.is_empty() { "unavailable" } else { "loaded" },
        "candidates": candidates,
        "agent_bindings": agent_bindings,
        "agent_scan": agent_scan,
        "sources": sources,
        "selection_required": selection_required,
        "cc_switch_sync": cc_switch_sync,
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
    if let Some(path) = opencode_models_cache_path() {
        if let Ok(raw) = fs::read(&path) {
            let models = parse_opencode_free_models(&raw);
            if !models.is_empty() {
                candidates.push(UpstreamCandidate {
                    id: "opencode".to_string(),
                    name: "OpenCode official free catalog".to_string(),
                    protocol: "open_ai".to_string(),
                    base_url: Some("https://opencode.ai/zen/v1".to_string()),
                    models,
                    source: "opencode-provider-catalog-cache".to_string(),
                    freshness: now_timestamp(),
                    auth_mode: "none".to_string(),
                    active: false,
                    port: Some(443),
                    state: "available".to_string(),
                    agent_ids: vec!["opencode".to_string()],
                    confidence: 85,
                    reasons: vec![
                        "OpenCode's local official catalog reports active zero-cost models"
                            .to_string(),
                    ],
                });
                sources.push(json!({
                    "id":"opencode-provider-catalog",
                    "state":"loaded",
                    "freshness":now_timestamp(),
                    "source":"opencode-local-cache",
                    "path":display_path(&path)
                }));
                return;
            }
        }
    }
    let output =
        run_capture_with_timeout("opencode", &["models", "opencode"], Duration::from_secs(6));
    match output {
        Ok((true, stdout)) => {
            let models = stdout
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
                    active: false,
                    port: Some(443),
                    state: "available".to_string(),
                    agent_ids: vec!["opencode".to_string()],
                    confidence: 70,
                    reasons: vec!["Official OpenCode free catalog reported this model".to_string()],
                });
            }
            sources.push(json!({"id":"opencode-provider-catalog","state":"loaded","freshness":now_timestamp()}));
        }
        Ok((false, _)) => sources.push(json!({"id":"opencode-provider-catalog","state":"unavailable","freshness":now_timestamp()})),
        Err(error) => sources.push(json!({"id":"opencode-provider-catalog","state":if error == "timeout" { "timeout" } else { "missing" },"freshness":now_timestamp()})),
    }
}

fn opencode_models_cache_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("DTOKEN_OPENCODE_MODELS_CACHE") {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("opencode").join("models.json"))
    }
    #[cfg(not(windows))]
    {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".cache"))
            })
            .map(|root| root.join("opencode").join("models.json"))
    }
}

fn parse_opencode_free_models(raw: &[u8]) -> Vec<String> {
    if raw.len() > 8 * 1024 * 1024 {
        return vec![];
    }
    let Ok(value) = serde_json::from_slice::<Value>(raw) else {
        return vec![];
    };
    let Some(models) = value
        .get("opencode")
        .and_then(|provider| provider.get("models"))
        .and_then(Value::as_object)
    else {
        return vec![];
    };
    let mut free = models
        .iter()
        .filter_map(|(id, model)| {
            let active = model.get("status").and_then(Value::as_str) != Some("deprecated");
            let input_free = model
                .pointer("/cost/input")
                .and_then(Value::as_f64)
                .is_some_and(|cost| cost == 0.0);
            let output_free = model
                .pointer("/cost/output")
                .and_then(Value::as_f64)
                .is_some_and(|cost| cost == 0.0);
            (active && input_free && output_free).then(|| id.clone())
        })
        .collect::<Vec<_>>();
    free.sort();
    free
}

fn candidates_from_opencode_config(path: &Path) -> Result<Vec<UpstreamCandidate>, &'static str> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(_) => return Err("unavailable"),
    };
    let value: Value = match serde_json::from_slice(&raw) {
        Ok(value) => value,
        Err(_) => return Err("invalid"),
    };
    let selected_model = value
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut candidates = Vec::new();
    if let Some(providers) = value.get("provider").and_then(Value::as_object) {
        for (id, provider) in providers {
            let active = selected_model
                .as_deref()
                .is_some_and(|model| model == id || model.starts_with(&format!("{id}/")));
            if let Some(mut candidate) =
                candidate_from_json(id, provider, "opencode-normal-config", active, None)
            {
                candidate.agent_ids = vec!["opencode".to_string()];
                candidate.confidence = if active { 98 } else { 80 };
                candidate.reasons = vec![if active {
                    "OpenCode currently selects this provider/model".to_string()
                } else {
                    "Provider is present in the normal OpenCode configuration".to_string()
                }];
                if let Some(model) = selected_model
                    .as_deref()
                    .and_then(|model| model.strip_prefix(&format!("{id}/")))
                {
                    candidate.models = vec![model.to_string()];
                }
                candidates.push(candidate);
            }
        }
    }
    Ok(candidates)
}

fn discover_agent_configs(
    detected_agents: &[dtoken_config_patcher::agent::DetectedAgent],
    candidates: &mut Vec<UpstreamCandidate>,
    sources: &mut Vec<Value>,
) {
    for detected in detected_agents {
        let Some(spec) = agent_descriptor(&detected.id).copied() else {
            continue;
        };
        let config_paths = detected
            .evidence
            .iter()
            .filter(|evidence| evidence.kind == AgentEvidenceKind::Config)
            .map(|evidence| evidence.path.as_path())
            .collect::<Vec<_>>();
        if config_paths.is_empty() {
            sources.push(json!({
                "id": format!("agent-config:{}", spec.id),
                "agent_id": spec.id,
                "state": "missing",
                "freshness": now_timestamp()
            }));
            continue;
        }
        for (index, path) in config_paths.iter().enumerate() {
            let extension = path.extension().and_then(|item| item.to_str());
            let result = if spec.id == "opencode" && extension == Some("json") {
                candidates_from_opencode_config(path)
            } else if spec.id == "codex" && extension == Some("toml") {
                candidate_from_codex_toml(path)
                    .map(|candidate| candidate.into_iter().collect::<Vec<_>>())
            } else if extension == Some("json") {
                candidate_from_agent_json(spec, path)
                    .map(|candidate| candidate.into_iter().collect::<Vec<_>>())
            } else {
                Err("unsupported_format")
            };
            let source_id = format!("agent-config:{}:{}", spec.id, index + 1);
            match result {
                Ok(mut discovered) => {
                    if config_paths.len() > 1 {
                        for candidate in &mut discovered {
                            candidate.id = format!("{}-config-{}", candidate.id, index + 1);
                        }
                    }
                    let state = if discovered.is_empty() {
                        "loaded_no_safe_upstream"
                    } else {
                        "loaded"
                    };
                    candidates.extend(discovered);
                    sources.push(json!({
                        "id": source_id,
                        "agent_id": spec.id,
                        "state": state,
                        "freshness": now_timestamp(),
                        "path": display_path(path)
                    }));
                }
                Err(state) => sources.push(json!({
                    "id": source_id,
                    "agent_id": spec.id,
                    "state": state,
                    "freshness": now_timestamp(),
                    "path": display_path(path)
                })),
            }
        }
    }
}

fn candidate_from_agent_json(
    spec: AgentDescriptor,
    path: &Path,
) -> Result<Option<UpstreamCandidate>, &'static str> {
    let raw = fs::read(path).map_err(|_| "unavailable")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let value: Value = serde_json::from_slice(&raw).map_err(|_| "invalid")?;
    let source = format!("agent-config:{}", spec.id);
    let mut candidate =
        match candidate_from_json(&format!("{}-current", spec.id), &value, &source, true, None) {
            Some(candidate) => candidate,
            None => return Ok(None),
        };
    candidate.name = format!("{} current upstream", spec.name);
    candidate.agent_ids = vec![spec.id.to_string()];
    candidate.confidence = 96;
    candidate.reasons = vec![format!("{} currently references this upstream", spec.name)];
    Ok(Some(candidate))
}

fn candidate_from_codex_toml(path: &Path) -> Result<Option<UpstreamCandidate>, &'static str> {
    let raw = fs::read_to_string(path).map_err(|_| "unavailable")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let value: toml::Value = toml::from_str(&raw).map_err(|_| "invalid")?;
    let provider_id = value
        .get("model_provider")
        .and_then(toml::Value::as_str)
        .unwrap_or("openai");
    let model = value
        .get("model")
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    let provider = value
        .get("model_providers")
        .and_then(toml::Value::as_table)
        .and_then(|providers| providers.get(provider_id));
    let base_url = provider
        .and_then(|provider| provider.get("base_url"))
        .and_then(toml::Value::as_str)
        .and_then(redact_base_url);
    let protocol = provider
        .and_then(|provider| provider.get("wire_api"))
        .and_then(toml::Value::as_str)
        .unwrap_or("responses")
        .to_string();
    let has_env_key = provider
        .and_then(|provider| provider.get("env_key"))
        .and_then(toml::Value::as_str)
        .is_some();
    if base_url.is_none() && model.is_none() {
        return Ok(None);
    }
    Ok(Some(UpstreamCandidate {
        id: format!("codex-{provider_id}"),
        name: format!("Codex {provider_id}"),
        protocol,
        base_url,
        models: model.into_iter().collect(),
        source: "agent-config:codex".to_string(),
        freshness: now_timestamp(),
        auth_mode: if has_env_key { "unavailable" } else { "none" }.to_string(),
        active: true,
        port: None,
        state: "discovered".to_string(),
        agent_ids: vec!["codex".to_string()],
        confidence: 96,
        reasons: vec!["Codex currently selects this model provider".to_string()],
    }))
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
                agent_ids: agent_ids_for_cc_switch_app(&app_type),
                confidence: if active { 96 } else { 72 },
                reasons: vec![if active {
                    "cc-switch marks this provider as current".to_string()
                } else {
                    "Provider exists in the cc-switch database".to_string()
                }],
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
                    agent_ids: agent_ids_for_cc_switch_app(&app_type),
                    confidence: if enabled && proxy_enabled { 99 } else { 65 },
                    reasons: vec![if enabled && proxy_enabled {
                        "cc-switch proxy is enabled for this Agent".to_string()
                    } else {
                        "cc-switch proxy is configured but not enabled".to_string()
                    }],
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

fn normalize_cc_switch_proxy_candidates(candidates: &mut [UpstreamCandidate]) {
    let active_models = candidates
        .iter()
        .filter(|candidate| {
            candidate.source.starts_with("cc-switch:")
                && candidate.active
                && !candidate.source.ends_with("proxy_config")
        })
        .flat_map(|candidate| {
            candidate
                .agent_ids
                .iter()
                .map(move |agent_id| (agent_id.clone(), candidate.models.clone()))
        })
        .collect::<Vec<_>>();
    for proxy in candidates
        .iter_mut()
        .filter(|candidate| candidate.source == "cc-switch:proxy_config" && candidate.active)
    {
        if let Some((_, models)) = active_models
            .iter()
            .find(|(agent_id, _)| proxy.agent_ids.contains(agent_id))
        {
            proxy.models = models.clone();
        }
    }
    let proxy_agents = candidates
        .iter()
        .filter(|candidate| candidate.source == "cc-switch:proxy_config" && candidate.active)
        .flat_map(|candidate| candidate.agent_ids.clone())
        .collect::<Vec<_>>();
    for direct in candidates.iter_mut().filter(|candidate| {
        candidate.source.starts_with("cc-switch:") && candidate.source != "cc-switch:proxy_config"
    }) {
        if direct
            .agent_ids
            .iter()
            .any(|agent_id| proxy_agents.contains(agent_id))
        {
            direct.active = false;
            direct.reasons.push(
                "The enabled cc-switch proxy is the safe connection target for this Agent"
                    .to_string(),
            );
        }
    }
}

fn resolve_agent_bindings(
    candidates: &[UpstreamCandidate],
    detected_agents: &[dtoken_config_patcher::agent::DetectedAgent],
) -> Vec<AgentUpstreamBinding> {
    detected_agents
        .iter()
        .filter_map(|detected| {
            let spec = agent_descriptor(&detected.id)?;
            let config_paths = detected
                .evidence
                .iter()
                .filter(|evidence| evidence.kind == AgentEvidenceKind::Config)
                .map(|evidence| evidence.path.clone())
                .collect::<Vec<_>>();
            let config_path = config_paths.first().cloned();
            let binary_path = detected
                .evidence
                .iter()
                .find(|evidence| evidence.kind == AgentEvidenceKind::Executable)
                .or_else(|| {
                    detected.evidence.iter().find(|evidence| {
                        matches!(
                            evidence.kind,
                            AgentEvidenceKind::AppBundle | AgentEvidenceKind::Extension
                        )
                    })
                })
                .map(|evidence| display_path(&evidence.path));
            let all_candidates = candidates
                .iter()
                .filter(|candidate| candidate.agent_ids.iter().any(|id| id == spec.id))
                .cloned()
                .collect::<Vec<_>>();
            let already_connected = all_candidates.iter().any(|candidate| {
                candidate
                    .base_url
                    .as_deref()
                    .is_some_and(is_dtoken_listener)
            });
            let eligible = all_candidates
                .iter()
                .filter(|candidate| {
                    !candidate
                        .base_url
                        .as_deref()
                        .is_some_and(is_dtoken_listener)
                })
                .cloned()
                .collect::<Vec<_>>();
            let active = eligible
                .iter()
                .filter(|candidate| candidate.active)
                .cloned()
                .collect::<Vec<_>>();
            let equivalent_active = collapse_equivalent_active_candidates(spec.id, &active);
            let selected = if let Some(current) =
                preferred_current_agent_candidate(spec.id, &equivalent_active, &active)
            {
                Some(current)
            } else if equivalent_active.len() == 1 {
                equivalent_active.first().cloned()
            } else if equivalent_active.is_empty() && eligible.len() == 1 {
                eligible.first().cloned()
            } else if equivalent_active.is_empty() && spec.id == "opencode" {
                preferred_opencode_free_candidate(&eligible)
            } else {
                None
            };
            let (resolution, confidence, reasons) = resolve_binding_state(
                spec,
                true,
                already_connected,
                &eligible,
                &equivalent_active,
                selected.as_ref(),
                config_path.is_some(),
            );
            let current_config_source = selected
                .as_ref()
                .map(|candidate| candidate.source.clone())
                .or_else(|| {
                    config_path
                        .as_ref()
                        .map(|_| format!("agent-config:{}", spec.id))
                })
                .unwrap_or_else(|| "unknown".to_string());
            Some(AgentUpstreamBinding {
                agent_id: spec.id.to_string(),
                agent_name: spec.name.to_string(),
                installed: true,
                binary_path,
                config_path: config_path.as_deref().map(display_path),
                config_paths: config_paths.iter().map(|path| display_path(path)).collect(),
                current_config_source,
                candidates: eligible,
                resolved_candidate: if matches!(
                    resolution.as_str(),
                    "auto-resolved" | "needs-credential"
                ) {
                    selected
                } else {
                    None
                },
                resolution,
                confidence,
                reasons,
                capability: spec.install_capability.as_str().to_string(),
                already_connected,
            })
        })
        .collect()
}

fn collapse_equivalent_active_candidates(
    agent_id: &str,
    active: &[UpstreamCandidate],
) -> Vec<UpstreamCandidate> {
    let mut collapsed: Vec<UpstreamCandidate> = Vec::new();
    for candidate in active {
        let route_key = candidate_route_key(candidate);
        if let Some(current) = collapsed
            .iter_mut()
            .find(|current| candidate_route_key(current) == route_key)
        {
            let candidate_is_agent_config = candidate.source == format!("agent-config:{agent_id}")
                || (agent_id == "opencode" && candidate.source == "opencode-normal-config");
            if candidate_is_agent_config && candidate.models.len() == 1 {
                current.models = candidate.models.clone();
                current.source = candidate.source.clone();
                current.name = candidate.name.clone();
            }
            if current.auth_mode == "unavailable" && candidate.auth_mode == "none" {
                current.auth_mode = "none".to_string();
            }
            current.confidence = current.confidence.max(candidate.confidence);
            current.reasons.extend(candidate.reasons.clone());
            current.reasons.sort();
            current.reasons.dedup();
        } else {
            collapsed.push(candidate.clone());
        }
    }
    collapsed
}

fn candidate_route_key(candidate: &UpstreamCandidate) -> String {
    candidate
        .base_url
        .as_deref()
        .map(|url| {
            format!(
                "{}|{}",
                url.trim_end_matches('/').to_ascii_lowercase(),
                candidate.protocol
            )
        })
        .unwrap_or_else(|| {
            format!(
                "{}|{}|{}",
                candidate.source, candidate.id, candidate.protocol
            )
        })
}

fn preferred_current_agent_candidate(
    agent_id: &str,
    collapsed_active: &[UpstreamCandidate],
    raw_active: &[UpstreamCandidate],
) -> Option<UpstreamCandidate> {
    let source = format!("agent-config:{agent_id}");
    let mut matches = collapsed_active.iter().filter(|candidate| {
        candidate.source == source
            || (agent_id == "opencode" && candidate.source == "opencode-normal-config")
    });
    let mut current = matches.next()?.clone();
    if matches.next().is_some() {
        return None;
    }

    // An Agent's current config is the strongest evidence of what it will use.
    // A cc-switch proxy may describe the same loopback endpoint with a different
    // protocol label; in that case the proxy also proves d-token does not need
    // to reuse the Agent's private credential.
    if current.auth_mode == "unavailable"
        && current.base_url.as_deref().is_some_and(|base_url| {
            let endpoint = normalized_candidate_endpoint(base_url);
            raw_active.iter().any(|candidate| {
                candidate.source == "cc-switch:proxy_config"
                    && candidate.auth_mode == "none"
                    && candidate.base_url.as_deref().is_some_and(|proxy_url| {
                        normalized_candidate_endpoint(proxy_url) == endpoint
                    })
            })
        })
    {
        current.auth_mode = "none".to_string();
        current.reasons.push(
            "The active cc-switch loopback proxy can be used without reusing the Agent credential"
                .to_string(),
        );
    }
    Some(current)
}

fn normalized_candidate_endpoint(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn preferred_opencode_free_candidate(eligible: &[UpstreamCandidate]) -> Option<UpstreamCandidate> {
    let mut candidate = eligible
        .iter()
        .find(|candidate| candidate.source.starts_with("opencode-provider-catalog"))?
        .clone();
    let preferred = [
        "deepseek-v4-flash-free",
        "north-mini-code-free",
        "mimo-v2.5-free",
        "hy3-free",
        "nemotron-3-ultra-free",
        "big-pickle",
    ]
    .into_iter()
    .find(|model| candidate.models.iter().any(|item| item == model))
    .map(str::to_string)
    .or_else(|| candidate.models.first().cloned())?;
    candidate.models = vec![preferred];
    candidate.confidence = 90;
    candidate.reasons = vec![
        "No active OpenCode model was configured; the review plan defaults to an official active zero-cost model"
            .to_string(),
    ];
    Some(candidate)
}

fn resolve_binding_state(
    spec: &AgentDescriptor,
    installed: bool,
    already_connected: bool,
    eligible: &[UpstreamCandidate],
    active: &[UpstreamCandidate],
    selected: Option<&UpstreamCandidate>,
    has_config: bool,
) -> (String, u8, Vec<String>) {
    if !installed {
        return (
            "unavailable".to_string(),
            0,
            vec![format!(
                "{} was not detected on this Mac or PATH",
                spec.name
            )],
        );
    }
    if already_connected {
        return (
            "already-connected".to_string(),
            100,
            vec![format!(
                "{} already points to the d-token listener",
                spec.name
            )],
        );
    }
    if matches!(
        spec.install_capability,
        AgentInstallCapability::ManualOnly | AgentInstallCapability::PromptOnly
    ) {
        return (
            "unsupported".to_string(),
            0,
            vec![format!(
                "{} requires a reviewed manual or prompt-based setup",
                spec.name
            )],
        );
    }
    if eligible.is_empty() {
        return (
            if has_config {
                "unsupported"
            } else {
                "unavailable"
            }
            .to_string(),
            0,
            vec![if has_config {
                "A config file exists, but its upstream format could not be parsed safely"
                    .to_string()
            } else {
                "No safe upstream metadata was found".to_string()
            }],
        );
    }
    if selected.is_none() && (active.len() > 1 || (active.is_empty() && eligible.len() > 1)) {
        return (
            "needs-choice".to_string(),
            0,
            vec!["Multiple equally plausible upstreams were found".to_string()],
        );
    }
    let Some(candidate) = selected else {
        return (
            "needs-choice".to_string(),
            0,
            vec!["Select one discovered upstream".to_string()],
        );
    };
    if candidate.auth_mode == "unavailable" {
        return (
            "needs-credential".to_string(),
            candidate.confidence,
            vec![
                "The upstream is known, but its credential cannot be reused by d-token".to_string(),
            ],
        );
    }
    if candidate.models.len() != 1 {
        return (
            "needs-choice".to_string(),
            candidate.confidence,
            vec!["Select the model to keep for this Agent".to_string()],
        );
    }
    (
        "auto-resolved".to_string(),
        candidate.confidence,
        candidate.reasons.clone(),
    )
}

fn cc_switch_sync_state(sources: &[Value], candidates: &[UpstreamCandidate]) -> Value {
    let source = sources
        .iter()
        .find(|source| source.get("id").and_then(Value::as_str) == Some("cc-switch-sqlite"));
    let detected = source
        .and_then(|source| source.get("state"))
        .and_then(Value::as_str)
        == Some("loaded");
    let current = candidates
        .iter()
        .find(|candidate| candidate.source.starts_with("cc-switch:") && candidate.active);
    json!({
        "detected": detected,
        "status": if !detected { "unavailable" } else if current.is_some() { "needs-confirmation" } else { "detected" },
        "profile": current.map(|candidate| candidate.name.clone()),
        "upstream": current.and_then(|candidate| candidate.base_url.clone()),
        "connection_path": current.map(|candidate| format!("Agent -> d-token -> {}", candidate.name)),
        "source": "cc-switch-read-only",
        "writes_performed": false
    })
}

fn discovery_revision(bindings: &[AgentUpstreamBinding], cc_switch_sync: &Value) -> String {
    let stable = json!({
        "bindings": bindings.iter().map(|binding| json!({
            "agent_id": binding.agent_id,
            "installed": binding.installed,
            "config_path": binding.config_path,
            "resolution": binding.resolution,
            "resolved": binding.resolved_candidate.as_ref().map(|candidate| json!({
                "id": candidate.id,
                "source": candidate.source,
                "models": candidate.models,
                "base_url": candidate.base_url,
                "auth_mode": candidate.auth_mode
            })),
            "candidate_count": binding.candidates.len(),
            "already_connected": binding.already_connected
        })).collect::<Vec<_>>(),
        "cc_switch": cc_switch_sync
    })
    .to_string();
    let hash = stable.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("setup-{hash:016x}")
}

fn candidate_from_json(
    id: &str,
    value: &Value,
    source: &str,
    active: bool,
    port: Option<u16>,
) -> Option<UpstreamCandidate> {
    let base_url = find_safe_string(value, &["baseurl", "base_url", "apiurl", "api_url"])
        .and_then(|url| redact_base_url(&url));
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
        agent_ids: vec![],
        confidence: if active { 90 } else { 65 },
        reasons: vec!["Safe provider metadata was read from a local configuration".to_string()],
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

fn run_capture_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<(bool, String), String> {
    let mut child = Command::new(program)
        .args(args)
        .env_remove("OPENCODE_CONFIG_CONTENT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => {
                let mut stdout = Vec::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.by_ref().take(256 * 1024).read_to_end(&mut stdout);
                }
                return Ok((
                    status.success(),
                    String::from_utf8_lossy(&stdout).to_string(),
                ));
            }
            None if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timeout".to_string());
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn redact_base_url(value: &str) -> Option<String> {
    let mut parsed = url::Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    Some(parsed.to_string().trim_end_matches('/').to_string())
}

fn is_dtoken_listener(value: &str) -> bool {
    url::Url::parse(value).ok().is_some_and(|url| {
        matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
            && url.port_or_known_default() == Some(18001)
    })
}

fn agent_ids_for_cc_switch_app(app: &str) -> Vec<String> {
    match app {
        "claude" => vec!["claude-code".to_string()],
        "codex" => vec!["codex".to_string()],
        "gemini" => vec!["gemini-cli".to_string()],
        "opencode" => vec!["opencode".to_string()],
        _ => vec![],
    }
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
        assert!(value["selection_required"].is_boolean());
        assert!(value["agent_bindings"].is_array());
        assert!(value["revision"]
            .as_str()
            .is_some_and(|revision| revision.starts_with("setup-")));
        assert_eq!(value["writes_performed"], false);
    }

    #[test]
    fn opencode_cache_keeps_only_active_zero_cost_models() {
        let models = parse_opencode_free_models(
            br#"{
              "opencode": {"models": {
                "free-active": {"cost": {"input": 0, "output": 0}},
                "free-deprecated": {"status": "deprecated", "cost": {"input": 0, "output": 0}},
                "paid": {"cost": {"input": 1, "output": 2}},
                "unknown": {}
              }}
            }"#,
        );

        assert_eq!(models, vec!["free-active"]);
    }

    #[test]
    fn bindings_include_only_backend_detected_agents() {
        use dtoken_config_patcher::agent::{
            AgentDetectionEvidence, AgentInstallCapability, AgentSessionCapability, DetectedAgent,
        };

        let detected = vec![DetectedAgent {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            install_capability: AgentInstallCapability::ConfirmRequired,
            session_capability: AgentSessionCapability::Metadata,
            evidence: vec![AgentDetectionEvidence {
                kind: AgentEvidenceKind::Config,
                path: PathBuf::from("/tmp/codex-config.toml"),
            }],
        }];

        let bindings = resolve_agent_bindings(&[], &detected);

        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].agent_id, "codex");
        assert!(bindings[0].installed);
    }

    #[test]
    fn discovery_records_every_detected_config_and_requires_choice_for_distinct_routes() {
        use dtoken_config_patcher::agent::{
            AgentDetectionEvidence, AgentInstallCapability, AgentSessionCapability, DetectedAgent,
        };

        let temporary = tempfile::tempdir().expect("temporary directory");
        let first = temporary.path().join("settings.json");
        let second = temporary.path().join("profile.json");
        fs::write(
            &first,
            r#"{"baseURL":"https://first.example/v1","model":"model-a"}"#,
        )
        .expect("first config");
        fs::write(
            &second,
            r#"{"baseURL":"https://second.example/v1","model":"model-b"}"#,
        )
        .expect("second config");
        let detected = vec![DetectedAgent {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            install_capability: AgentInstallCapability::ConfirmRequired,
            session_capability: AgentSessionCapability::Metadata,
            evidence: vec![
                AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Config,
                    path: first,
                },
                AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Config,
                    path: second,
                },
            ],
        }];
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_agent_configs(&detected, &mut candidates, &mut sources);

        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|source| source["state"] == "loaded"));
        assert_eq!(candidates.len(), 2);
        assert_ne!(candidates[0].id, candidates[1].id);
        let bindings = resolve_agent_bindings(&candidates, &detected);
        assert_eq!(bindings[0].config_paths.len(), 2);
        assert_eq!(bindings[0].resolution, "needs-choice");
        assert!(bindings[0].resolved_candidate.is_none());
    }

    #[test]
    fn unique_current_candidate_is_auto_resolved_but_multiple_candidates_require_choice() {
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "opencode")
            .unwrap();
        let current = candidate_from_json(
            "current",
            &json!({"baseURL":"https://example.test/v1","model":"model-a"}),
            "agent-config:opencode",
            true,
            None,
        )
        .unwrap();
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            std::slice::from_ref(&current),
            std::slice::from_ref(&current),
            Some(&current),
            true,
        );
        assert_eq!(resolution, "auto-resolved");

        let second = UpstreamCandidate {
            id: "second".to_string(),
            active: false,
            ..current.clone()
        };
        let (resolution, _, _) =
            resolve_binding_state(spec, true, false, &[current, second], &[], None, true);
        assert_eq!(resolution, "needs-choice");
    }

    #[test]
    fn opencode_without_an_active_model_uses_a_reviewed_official_free_default() {
        let catalog = UpstreamCandidate {
            id: "opencode".to_string(),
            name: "OpenCode official free catalog".to_string(),
            protocol: "open_ai".to_string(),
            base_url: Some("https://opencode.ai/zen/v1".to_string()),
            models: vec![
                "big-pickle".to_string(),
                "deepseek-v4-flash-free".to_string(),
            ],
            source: "opencode-provider-catalog-cache".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "none".to_string(),
            active: false,
            port: Some(443),
            state: "available".to_string(),
            agent_ids: vec!["opencode".to_string()],
            confidence: 85,
            reasons: vec![],
        };

        let selected = preferred_opencode_free_candidate(&[catalog]).expect("official default");

        assert_eq!(selected.models, vec!["deepseek-v4-flash-free"]);
        assert_eq!(selected.confidence, 90);
        let custom = UpstreamCandidate {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: Some("https://custom.example/v1".to_string()),
            models: vec!["custom-model".to_string()],
            source: "opencode-normal-config".to_string(),
            auth_mode: "unavailable".to_string(),
            confidence: 80,
            ..selected.clone()
        };
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "opencode")
            .expect("OpenCode adapter");
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            &[selected.clone(), custom],
            &[],
            Some(&selected),
            true,
        );
        assert_eq!(resolution, "auto-resolved");
    }

    #[test]
    fn agent_config_and_cc_switch_proxy_for_the_same_endpoint_are_not_ambiguous() {
        let config = UpstreamCandidate {
            id: "claude-code-current".to_string(),
            name: "Claude Code current upstream".to_string(),
            protocol: "anthropic".to_string(),
            base_url: Some("http://127.0.0.1:15721".to_string()),
            models: vec!["opus".to_string()],
            source: "agent-config:claude-code".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "unavailable".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["claude-code".to_string()],
            confidence: 96,
            reasons: vec![],
        };
        let proxy = UpstreamCandidate {
            id: "cc-switch-proxy-claude".to_string(),
            name: "cc-switch claude proxy".to_string(),
            source: "cc-switch:proxy_config".to_string(),
            auth_mode: "none".to_string(),
            confidence: 99,
            ..config.clone()
        };

        let collapsed = collapse_equivalent_active_candidates("claude-code", &[config, proxy]);

        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].models, vec!["opus"]);
        assert_eq!(collapsed[0].auth_mode, "none");
    }

    #[test]
    fn current_agent_config_wins_over_saved_profiles_and_proxy_protocol_labels() {
        let current = UpstreamCandidate {
            id: "claude-code-current".to_string(),
            name: "Claude Code current upstream".to_string(),
            protocol: "anthropic".to_string(),
            base_url: Some("http://127.0.0.1:15721".to_string()),
            models: vec!["opus".to_string()],
            source: "agent-config:claude-code".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "unavailable".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["claude-code".to_string()],
            confidence: 96,
            reasons: vec![],
        };
        let proxy = UpstreamCandidate {
            id: "cc-switch-proxy-claude".to_string(),
            name: "cc-switch claude proxy".to_string(),
            protocol: "open_ai".to_string(),
            source: "cc-switch:proxy_config".to_string(),
            auth_mode: "none".to_string(),
            confidence: 99,
            ..current.clone()
        };
        let saved_profile = UpstreamCandidate {
            id: "saved-profile".to_string(),
            name: "Saved profile".to_string(),
            base_url: Some("https://saved.example/v1".to_string()),
            source: "cc-switch:claude".to_string(),
            active: false,
            ..current.clone()
        };
        let raw_active = vec![current.clone(), proxy];
        let collapsed = collapse_equivalent_active_candidates("claude-code", &raw_active);
        let selected = preferred_current_agent_candidate("claude-code", &collapsed, &raw_active)
            .expect("current Agent config");

        assert_eq!(selected.source, "agent-config:claude-code");
        assert_eq!(selected.models, ["opus"]);
        assert_eq!(selected.auth_mode, "none");
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "claude-code")
            .expect("Claude Code adapter");
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            &[current, saved_profile],
            &collapsed,
            Some(&selected),
            true,
        );
        assert_eq!(resolution, "auto-resolved");
    }

    #[test]
    fn credential_gap_and_existing_dtoken_listener_are_distinct_states() {
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "opencode")
            .unwrap();
        let credential = candidate_from_json(
            "current",
            &json!({"baseURL":"https://example.test/v1","model":"model-a","apiKey":"must-not-escape"}),
            "agent-config:opencode",
            true,
            None,
        )
        .unwrap();
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            std::slice::from_ref(&credential),
            std::slice::from_ref(&credential),
            Some(&credential),
            true,
        );
        assert_eq!(resolution, "needs-credential");

        let (resolution, _, _) = resolve_binding_state(spec, true, true, &[], &[], None, true);
        assert_eq!(resolution, "already-connected");
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
    fn cc_switch_gemini_candidates_bind_to_the_backend_gemini_adapter() {
        assert_eq!(agent_ids_for_cc_switch_app("gemini"), ["gemini-cli"]);
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
