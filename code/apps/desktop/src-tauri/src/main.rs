use dtoken_config_patcher::{ConfigPatcher, ExpectedFileState};
use dtoken_process::sanitize_command_output;
#[cfg(debug_assertions)]
use dtoken_process::{opencode_command_plan, run_opencode_model_test};
use dtoken_types::{ConfigToolKind, RiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DTOKEN_SERVICE_PORT: u16 = 18001;
const EXTERNAL_HEADROOM_REFERENCE_PORT: u16 = 8787;
const LEGACY_OPENCODE_REFERENCE_PORT: u16 = 8788;

#[derive(Serialize)]
struct ToolCheck {
    id: String,
    label: String,
    installed: bool,
    version: Option<String>,
    path: Option<String>,
    status: String,
    detail: String,
}

#[derive(Serialize)]
struct ActionLog {
    time: String,
    level: String,
    message: String,
}

#[derive(Serialize)]
struct ActionResult {
    ok: bool,
    summary: String,
    logs: Vec<ActionLog>,
    tools: Vec<ToolCheck>,
}

#[derive(Serialize)]
struct HeadroomEndpoint {
    port: u16,
    label: String,
    healthy: bool,
    detail: String,
}

#[derive(Deserialize)]
struct InstallPlanRequest {
    target: String,
    install_root: String,
    proxy_port: u16,
    auto_backup: bool,
    run_doctor_after: bool,
    rollback_point: bool,
    mode: String,
}

#[derive(Deserialize)]
struct ProjectPolicyRecommendRequest {
    project_id: Option<String>,
    language_stack: Option<Vec<String>>,
    avg_request_tokens_7d: Option<u64>,
    avg_cli_noise_ratio_7d: Option<f32>,
    retrieve_rate_7d: Option<f32>,
    failure_rate_7d: Option<f32>,
    high_risk_task: Option<bool>,
    manual_override: Option<bool>,
}

#[derive(Deserialize)]
struct RouteProfileDraftRequest {
    route_id: String,
    route_name: String,
    provider_id: String,
    provider_name: String,
    provider_kind: String,
    protocol: String,
    base_url: String,
    api_key_ref: Option<String>,
    models: Vec<String>,
    chain_mode: String,
    compression_policy: String,
    local_port: u16,
    agent: String,
}

#[derive(Deserialize)]
struct ApplyRouteDraftRequest {
    route_id: String,
    reviewed_before_sha256: Option<String>,
    reviewed_after_sha256: Option<String>,
}

#[derive(Deserialize)]
struct RouteDraftReviewRequest {
    route_id: String,
}

#[derive(Deserialize)]
struct StoreProviderCredentialRequest {
    provider_id: String,
    credential: String,
}

#[derive(Deserialize)]
struct DeleteProviderCredentialRequest {
    provider_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct ManagedServiceManifest {
    managed_by: String,
    identity: String,
    pid: u32,
    port: u16,
    exe_path: String,
    config_path: String,
    data_dir: String,
    started_at: String,
}

fn now_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() % 86_400)
        .unwrap_or_default();
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3_600,
        (secs % 3_600) / 60,
        secs % 60
    )
}

fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| format!("unix:{}", duration.as_secs()))
        .unwrap_or_else(|_| "unix:0".to_string())
}

fn log(level: &str, message: impl Into<String>) -> ActionLog {
    ActionLog {
        time: now_label(),
        level: level.to_string(),
        message: message.into(),
    }
}

fn app_state_dir() -> PathBuf {
    if let Some(path) = env::var_os("DTOKEN_TEST_STATE_DIR") {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "windows")]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA is required for persistent d-token state")
            .join("d-token")
    }
    #[cfg(target_os = "macos")]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME is required for persistent d-token state")
            .join("Library")
            .join("Application Support")
            .join("d-token")
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(path).join("d-token");
        }
        env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME or XDG_DATA_HOME is required for persistent d-token state")
            .join(".local")
            .join("share")
            .join("d-token")
    }
}

fn app_state_path(relative: &[&str]) -> PathBuf {
    let mut path = app_state_dir();
    for part in relative {
        path.push(part);
    }
    path
}

fn sync_parent_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let parent = path
            .parent()
            .ok_or_else(|| "state file has no parent directory".to_string())?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|err| format!("sync {}: {err}", parent.display()))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|err| {
        format!(
            "replace {} with {}: {err}",
            destination.display(),
            source.display()
        )
    })
}

#[cfg(windows)]
fn atomic_replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(format!(
            "atomic state replace failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

fn durable_atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "state path has no valid file name".to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temporary =
        path.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|err| format!("create {}: {err}", temporary.display()))?;
        file.write_all(content)
            .map_err(|err| format!("write {}: {err}", temporary.display()))?;
        file.sync_all()
            .map_err(|err| format!("sync {}: {err}", temporary.display()))?;
        atomic_replace_file(&temporary, path)?;
        sync_parent_directory(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn durable_remove_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent_directory(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("remove {}: {err}", path.display())),
    }
}

fn write_app_state(relative: &[&str], content: &str) -> Result<PathBuf, String> {
    let path = app_state_path(relative);
    durable_atomic_write(&path, content.as_bytes())?;
    Ok(path)
}

fn write_json_app_state(relative: &[&str], value: &Value) -> Result<PathBuf, String> {
    let content = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
    let path = app_state_path(relative);
    durable_atomic_write(&path, &content)?;
    Ok(path)
}

fn read_app_state(relative: &[&str]) -> Result<Value, String> {
    let path = app_state_path(relative);
    let content = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    serde_json::from_str(&content).map_err(|err| err.to_string())
}

fn merge_json_objects(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                match (base_map.get_mut(&key), value) {
                    (Some(existing @ Value::Object(_)), next @ Value::Object(_)) => {
                        merge_json_objects(existing, next);
                    }
                    (_, next) => {
                        base_map.insert(key, next);
                    }
                }
            }
        }
        (base_slot, next) => {
            *base_slot = next;
        }
    }
}

fn active_config_path() -> PathBuf {
    app_state_dir()
        .join("active-config")
        .join("dtoken-config.json")
}

fn config_backup_root() -> PathBuf {
    fs::canonicalize(app_state_dir())
        .unwrap_or_else(|_| app_state_dir())
        .join("config-backups")
}

fn config_patcher() -> ConfigPatcher {
    ConfigPatcher::new(config_backup_root())
}

fn patchable_active_config_path() -> Result<PathBuf, String> {
    let path = active_config_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Active config parent path is unavailable".to_string())?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|err| format!("Active config parent normalization failed: {err}"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| "Active config file name is unavailable".to_string())?;
    Ok(canonical_parent.join(file_name))
}

fn active_route_config_status() -> Value {
    let path = active_config_path();
    let exists = path.exists();
    let mut status = json!({
        "source": "d-token-desktop",
        "exists": exists,
        "path": path.display().to_string(),
        "runtime_load_order": [
            "--config",
            "DTOKEN_CONFIG",
            "%LOCALAPPDATA%\\d-token\\active-config\\dtoken-config.json",
            "~/.dtoken/active-config/dtoken-config.json"
        ],
        "privacy": "d-token active config only; external OpenCode, Headroom, RTK, cc-switch, shell profile, and system proxy configs are not read."
    });
    if !exists {
        if let Some(object) = status.as_object_mut() {
            object.insert("state".to_string(), json!("missing"));
            object.insert("provider_count".to_string(), json!(0));
            object.insert("route_count".to_string(), json!(0));
            object.insert("instance_count".to_string(), json!(0));
        }
        return status;
    }

    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            if let Some(object) = status.as_object_mut() {
                object.insert("state".to_string(), json!("unreadable"));
                object.insert("error".to_string(), json!(err.to_string()));
            }
            return status;
        }
    };
    let config: Value = match serde_json::from_str(&content) {
        Ok(config) => config,
        Err(err) => {
            if let Some(object) = status.as_object_mut() {
                object.insert("state".to_string(), json!("invalid_json"));
                object.insert("error".to_string(), json!(err.to_string()));
            }
            return status;
        }
    };

    let providers = config
        .get("providers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let routes = config
        .get("routes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let instances = config
        .get("instances")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let first_provider = providers.first().cloned().unwrap_or_else(|| json!({}));
    let first_route = routes.first().cloned().unwrap_or_else(|| json!({}));
    let first_instance = instances.first().cloned().unwrap_or_else(|| json!({}));
    let fallback_disabled = routes.iter().all(|route| {
        route
            .get("fallback_provider_id")
            .map(Value::is_null)
            .unwrap_or(true)
    });
    if let Some(object) = status.as_object_mut() {
        object.insert("state".to_string(), json!("loaded"));
        object.insert("provider_count".to_string(), json!(providers.len()));
        object.insert("route_count".to_string(), json!(routes.len()));
        object.insert("instance_count".to_string(), json!(instances.len()));
        object.insert("fallback_disabled".to_string(), json!(fallback_disabled));
        object.insert(
            "provider".to_string(),
            json!({
                "id": first_provider.get("id").cloned().unwrap_or(Value::Null),
                "name": first_provider.get("name").cloned().unwrap_or(Value::Null),
                "kind": first_provider.get("kind").cloned().unwrap_or(Value::Null),
                "protocol": first_provider.get("protocol").cloned().unwrap_or(Value::Null),
                "base_url": first_provider.get("base_url").cloned().unwrap_or(Value::Null),
                "has_api_key_ref": first_provider.get("api_key_ref").map(|value| !value.is_null()).unwrap_or(false),
            }),
        );
        object.insert(
            "route".to_string(),
            json!({
                "id": first_route.get("id").cloned().unwrap_or(Value::Null),
                "name": first_route.get("name").cloned().unwrap_or(Value::Null),
                "chain_mode": first_route.get("chain_mode").cloned().unwrap_or(Value::Null),
                "compression_policy": first_route.get("compression_policy").cloned().unwrap_or(Value::Null),
                "local_port": first_route.get("local_port").cloned().unwrap_or(Value::Null),
                "target_provider_id": first_route.get("target_provider_id").cloned().unwrap_or(Value::Null),
            }),
        );
        object.insert(
            "instance".to_string(),
            json!({
                "id": first_instance.get("id").cloned().unwrap_or(Value::Null),
                "agent": first_instance.get("agent").cloned().unwrap_or(Value::Null),
                "process_status": first_instance.get("process_status").cloned().unwrap_or(Value::Null),
            }),
        );
    }
    status
}

fn rollback_points_result() -> Result<Vec<Value>, String> {
    let dir = app_state_dir().join("rollback-points");
    let mut points = Vec::new();
    match fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|err| err.to_string())?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let id = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("rollback")
                    .to_string();
                let content = fs::read_to_string(&path)
                    .map_err(|err| format!("read {}: {err}", path.display()))?;
                let parsed: Value = serde_json::from_str(&content)
                    .map_err(|err| format!("parse {}: {err}", path.display()))?;
                points.push(json!({
                "id": id,
                "label": format!("Rollback point {}", parsed.get("target").and_then(Value::as_str).unwrap_or("install")),
                "when": parsed.get("createdAt").and_then(Value::as_str).unwrap_or("local"),
                "files": 1,
                "tools": parsed.get("target").and_then(Value::as_str).map(|target| vec![target]).unwrap_or_else(|| vec!["d-token"]),
                "verified": parsed.get("ok").and_then(Value::as_bool).unwrap_or(false)
            }));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("read {}: {err}", dir.display())),
    }

    let patcher = config_patcher();
    let backup_root = config_backup_root();
    match fs::read_dir(&backup_root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|err| err.to_string())?;
                let file_type = entry.file_type().map_err(|err| err.to_string())?;
                if !file_type.is_dir() {
                    continue;
                }
                let Some(id) = entry.file_name().to_str().map(str::to_string) else {
                    return Err("Config backup id is not valid UTF-8".to_string());
                };
                let backup = patcher
                    .load_backup(&id)
                    .map_err(|err| format!("verify config backup {id}: {err:#}"))?;
                points.push(json!({
                    "id": backup.rollback_id,
                    "label": format!("Rollback point {}", backup.display_target),
                    "when": backup.created_at,
                    "files": 1,
                    "tools": ["d-token"],
                    "verified": true,
                    "source": "dtoken-config-patcher",
                    "target": backup.target_path.display().to_string(),
                    "expectedCurrentSha256": backup.planned_after_sha256,
                }));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("read {}: {err}", backup_root.display())),
    }
    points.sort_by(|a, b| {
        b.get("when")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(a.get("when").and_then(Value::as_str).unwrap_or_default())
    });
    Ok(points)
}

fn route_profile_drafts_result() -> Result<Vec<Value>, String> {
    let dir = app_state_dir().join("routing");
    let mut drafts = Vec::new();
    match fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|err| err.to_string())?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let content = fs::read_to_string(&path)
                    .map_err(|err| format!("read {}: {err}", path.display()))?;
                let mut value = serde_json::from_str::<Value>(&content)
                    .map_err(|err| format!("parse {}: {err}", path.display()))?;
                if let Some(object) = value.as_object_mut() {
                    object.insert("path".to_string(), json!(path.display().to_string()));
                }
                drafts.push(value);
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("read {}: {err}", dir.display())),
    }
    drafts.sort_by(|a, b| {
        b.get("savedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(a.get("savedAt").and_then(Value::as_str).unwrap_or_default())
    });
    Ok(drafts)
}

fn route_profile_drafts_json() -> Vec<Value> {
    route_profile_drafts_result().unwrap_or_default()
}

fn install_targets(target: &str) -> Vec<&'static str> {
    match target {
        "rtk" => vec!["rtk"],
        "headroom" => vec!["headroom"],
        "dtoken" => vec!["dtoken"],
        "all" => vec!["rtk", "headroom", "dtoken"],
        _ => vec![],
    }
}

fn install_plan_json(req: &InstallPlanRequest) -> Value {
    let targets = install_targets(&req.target);
    let writes_enabled = matches!(req.mode.as_str(), "install" | "repair" | "connect");
    json!({
        "target": req.target,
        "mode": req.mode,
        "installRoot": req.install_root,
        "targets": targets,
        "writesEnabled": writes_enabled,
        "opencodeRoutePort": DTOKEN_SERVICE_PORT,
        "routeScope": "agent-routed-connection",
        "diagnosticScope": "agent-connection-verification",
        "excludedAgents": ["claude-code", "cursor", "codex-cli", "aider", "cline", "roo", "continue", "windsurf"],
        "transaction": [
            { "id": "scan", "label": "扫描", "detail": "检测 RTK、Headroom、d-token desktop、OpenCode 与本地端口" },
            { "id": "plan", "label": "生成计划", "detail": "根据所选目标生成安装/验证计划" },
            { "id": "diff", "label": "展示 diff", "detail": "写入前展示 d-token 将管理的配置差异" },
            { "id": "backup", "label": "备份", "detail": if req.auto_backup { "写入前创建 d-token 回滚清单" } else { "用户关闭自动备份，事务会记录风险" } },
            { "id": "apply", "label": "应用", "detail": if writes_enabled { "仅执行用户选择的托管动作" } else { "只读连接模式只验证，不写入外部配置" } },
            { "id": "verify", "label": "校验", "detail": "读取版本、路径、d-token 18001 健康端点和 OpenCode route 状态" },
            { "id": "doctor", "label": "Doctor", "detail": if req.run_doctor_after { "安装后自动运行 Doctor" } else { "用户选择跳过自动 Doctor" } },
            { "id": "rollback", "label": "回滚点", "detail": if req.rollback_point { "提交可审计回滚点" } else { "不创建回滚点" } }
        ],
        "diff": [
            { "type": "ctx", "text": "# d-token installation transaction" },
            { "type": "add", "text": format!("+ mode = \"{}\"", req.mode) },
            { "type": "add", "text": format!("+ install_root = \"{}\"", req.install_root) },
            { "type": "add", "text": format!("+ targets = {:?}", targets) },
            { "type": "add", "text": format!("+ opencode_route_base_url = \"http://127.0.0.1:{DTOKEN_SERVICE_PORT}/v1\"") },
            { "type": "add", "text": "+ route_scope = \"agent-routed-connection\"" },
            { "type": "add", "text": "+ excluded_agents = [\"claude-code\", \"cursor\", \"codex-cli\", \"aider\", \"cline\", \"roo\"]" },
            { "type": "add", "text": format!("+ auto_backup = {}", req.auto_backup) },
            { "type": "add", "text": format!("+ doctor_after_apply = {}", req.run_doctor_after) },
            { "type": "add", "text": format!("+ rollback_point = {}", req.rollback_point) }
        ],
        "warnings": if req.proxy_port == DTOKEN_SERVICE_PORT {
            vec!["OpenCode route verification should use the d-token local service port."]
        } else {
            vec!["The selected port is not the d-token service default 18001; verify it belongs to a d-token-managed local service."]
        }
    })
}

fn run_capture(program: &str, args: &[&str]) -> Result<(bool, String), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("{program}: {err}"))?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok((output.status.success(), text.trim().to_string()))
}

#[cfg(windows)]
fn where_tool(name: &str) -> Option<String> {
    let output = Command::new("where").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

#[cfg(not(windows))]
fn where_tool(name: &str) -> Option<String> {
    let output = Command::new("which").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn version_for(name: &str) -> Option<String> {
    run_capture(name, &["--version"])
        .ok()
        .filter(|(ok, _)| *ok)
        .map(|(_, text)| {
            if text.is_empty() {
                "installed".to_string()
            } else {
                text.lines().next().unwrap_or("installed").to_string()
            }
        })
}

fn check_tool(id: &str, label: &str) -> ToolCheck {
    let path = where_tool(id);
    let version = path.as_ref().and_then(|_| version_for(id));
    let installed = path.is_some();
    ToolCheck {
        id: id.to_string(),
        label: label.to_string(),
        installed,
        version,
        path,
        status: if installed { "ok" } else { "missing" }.to_string(),
        detail: if installed {
            format!("{label} is available on PATH")
        } else {
            format!("{label} was not found on PATH")
        },
    }
}

fn simple_http_get(host: &str, port: u16, path: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect((host, port)).map_err(|err| err.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| err.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| err.to_string())?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|err| err.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| err.to_string())?;
    Ok(response)
}

fn http_body(raw: &str) -> &str {
    raw.split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or(raw)
}

fn service_json(port: u16, path: &str) -> Result<Value, String> {
    let raw = simple_http_get("127.0.0.1", port, path)?;
    serde_json::from_str(http_body(&raw)).map_err(|err| err.to_string())
}

fn service_text(port: u16, path: &str) -> Result<String, String> {
    let raw = simple_http_get("127.0.0.1", port, path)?;
    Ok(http_body(&raw).to_string())
}

fn service_manifest_path() -> PathBuf {
    app_state_dir().join("service").join("managed-service.json")
}

fn service_log_path() -> PathBuf {
    app_state_dir().join("service").join("dtoken-proxy.log")
}

fn service_data_dir() -> PathBuf {
    app_state_dir().join("runtime")
}

fn read_service_manifest() -> Option<ManagedServiceManifest> {
    let path = service_manifest_path();
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_service_manifest(manifest: &ManagedServiceManifest) -> Result<PathBuf, String> {
    let content = serde_json::to_string_pretty(manifest).map_err(|err| err.to_string())?;
    write_app_state(&["service", "managed-service.json"], &content)
}

fn remove_service_manifest() -> Result<(), String> {
    durable_remove_file(&service_manifest_path())
}

fn is_process_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        let output = Command::new("tasklist")
            .args(["/FI", &filter, "/FO", "CSV", "/NH"])
            .output();
        let Ok(output) = output else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        text.contains(&pid.to_string()) && !text.contains("INFO:")
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
fn process_executable_path(pid: u32) -> Option<PathBuf> {
    let command =
        format!("(Get-CimInstance Win32_Process -Filter \"ProcessId = {pid}\").ExecutablePath");
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn process_executable_path(pid: u32) -> Option<PathBuf> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn process_executable_path(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).ok()
}

fn executable_identity_matches(pid: u32, expected: &Path) -> bool {
    let Some(actual) = process_executable_path(pid) else {
        return false;
    };
    let expected = fs::canonicalize(expected).ok();
    let actual = fs::canonicalize(actual).ok();
    expected.is_some() && expected == actual
}

fn listener_output_contains_pid(output: &str, pid: u32) -> bool {
    let expected = pid.to_string();
    output.lines().any(|line| {
        line.split(|character: char| !character.is_ascii_digit())
            .any(|token| token == expected)
    })
}

#[cfg(not(windows))]
fn listener_owned_by_pid(pid: u32, port: u16) -> bool {
    let pid_arg = pid.to_string();
    let port_arg = format!("-iTCP:{port}");
    let output = Command::new("lsof")
        .args(["-nP", "-a", "-p", &pid_arg, &port_arg, "-sTCP:LISTEN", "-t"])
        .output();
    let Ok(output) = output else {
        return false;
    };
    output.status.success()
        && listener_output_contains_pid(&String::from_utf8_lossy(&output.stdout), pid)
}

#[cfg(windows)]
fn listener_owned_by_pid(pid: u32, port: u16) -> bool {
    let output = Command::new("netstat").args(["-ano", "-p", "tcp"]).output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let port_marker = format!(":{port}");
    String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        line.contains(&port_marker)
            && line.to_ascii_uppercase().contains("LISTENING")
            && listener_output_contains_pid(line, pid)
    })
}

fn terminate_process(pid: u32) -> Result<(), String> {
    if pid == 0 {
        return Err("invalid pid".to_string());
    }
    #[cfg(windows)]
    {
        let output = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(sanitize_command_output(&String::from_utf8_lossy(
                &output.stderr,
            )))
        }
    }
    #[cfg(not(windows))]
    {
        let output = Command::new("kill")
            .arg(pid.to_string())
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(sanitize_command_output(&String::from_utf8_lossy(
                &output.stderr,
            )))
        }
    }
}

fn candidate_proxy_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(explicit) = env::var("DTOKEN_PROXY_EXE") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            candidates.push(dir.join("dtoken-proxy-x86_64-pc-windows-gnu.exe"));
            candidates.push(dir.join("dtoken-proxy-x86_64-pc-windows-gnu"));
            candidates.push(dir.join("dtoken-proxy-x86_64-pc-windows-msvc.exe"));
            candidates.push(dir.join("dtoken-proxy-x86_64-pc-windows-msvc"));
            candidates.push(dir.join("dtoken-proxy.exe"));
            candidates.push(dir.join("dtoken-proxy"));
            candidates.push(
                dir.join("binaries")
                    .join("dtoken-proxy-x86_64-pc-windows-gnu.exe"),
            );
            candidates.push(
                dir.join("binaries")
                    .join("dtoken-proxy-x86_64-pc-windows-gnu"),
            );
            candidates.push(
                dir.join("binaries")
                    .join("dtoken-proxy-x86_64-pc-windows-msvc.exe"),
            );
            candidates.push(
                dir.join("binaries")
                    .join("dtoken-proxy-x86_64-pc-windows-msvc"),
            );
            candidates.push(dir.join("..").join("dtoken-proxy.exe"));
            candidates.push(dir.join("..").join("dtoken-proxy"));
        }
    }
    candidates.push(
        PathBuf::from("code")
            .join("apps")
            .join("desktop")
            .join("src-tauri")
            .join("binaries")
            .join("dtoken-proxy-x86_64-pc-windows-gnu.exe"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("apps")
            .join("desktop")
            .join("src-tauri")
            .join("binaries")
            .join("dtoken-proxy-x86_64-pc-windows-gnu"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("apps")
            .join("desktop")
            .join("src-tauri")
            .join("binaries")
            .join("dtoken-proxy-x86_64-pc-windows-msvc.exe"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("apps")
            .join("desktop")
            .join("src-tauri")
            .join("binaries")
            .join("dtoken-proxy-x86_64-pc-windows-msvc"),
    );
    candidates.push(
        PathBuf::from("target")
            .join("release")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("target")
            .join("debug")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("..")
            .join("..")
            .join("target")
            .join("release")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("..")
            .join("..")
            .join("target")
            .join("debug")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("target")
            .join("release")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("target")
            .join("debug")
            .join("dtoken-proxy.exe"),
    );
    candidates
}

fn resolve_dtoken_proxy_exe() -> Result<PathBuf, String> {
    for candidate in candidate_proxy_paths() {
        let path = if candidate.is_absolute() {
            candidate
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(candidate)
        };
        if path.exists() {
            return fs::canonicalize(&path).map_err(|err| err.to_string());
        }
    }
    Err("dtoken-proxy executable was not found; build or bundle d-token runtime first".to_string())
}

fn service_health_payload() -> Option<Value> {
    service_json(DTOKEN_SERVICE_PORT, "/health").ok()
}

fn dtoken_service_status_value() -> Value {
    let manifest = read_service_manifest();
    let health = service_health_payload();
    let healthy = health
        .as_ref()
        .and_then(|payload| payload.get("status").and_then(Value::as_str))
        .map(|status| status == "healthy" || status == "ready")
        .unwrap_or(false);
    let component_ok = health
        .as_ref()
        .and_then(|payload| payload.get("component").and_then(Value::as_str))
        .map(|component| component == "dtoken-core-api")
        .unwrap_or(false);
    let pid_running = manifest
        .as_ref()
        .map(|item| is_process_running(item.pid))
        .unwrap_or(false);
    let manifest_identity_ok = manifest
        .as_ref()
        .map(|item| {
            item.managed_by == "d-token-desktop"
                && item.identity == "dtoken-core-api"
                && item.port == DTOKEN_SERVICE_PORT
        })
        .unwrap_or(false);
    let executable_owned = manifest
        .as_ref()
        .filter(|_| pid_running)
        .map(|item| executable_identity_matches(item.pid, Path::new(&item.exe_path)))
        .unwrap_or(false);
    let port_owned = manifest
        .as_ref()
        .filter(|_| pid_running)
        .map(|item| listener_owned_by_pid(item.pid, item.port))
        .unwrap_or(false);
    let manifest_stale = manifest.is_some() && !pid_running && !healthy;
    let managed_running = healthy
        && component_ok
        && pid_running
        && manifest_identity_ok
        && executable_owned
        && port_owned;
    let external_or_unmanaged = healthy && component_ok && !managed_running;

    json!({
        "ok": managed_running || external_or_unmanaged,
        "mode": "dtoken-service-lifecycle",
        "state": if managed_running {
            "running"
        } else if external_or_unmanaged {
            "unmanaged_running"
        } else {
            "stopped"
        },
        "managed": managed_running,
        "pid": manifest.as_ref().map(|item| item.pid),
        "pid_running": pid_running,
        "manifest_identity_ok": manifest_identity_ok,
        "executable_owned": executable_owned,
        "port_owned": port_owned,
        "manifest_stale": manifest_stale,
        "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
        "manifest_path": service_manifest_path().display().to_string(),
        "log_path": service_log_path().display().to_string(),
        "health": health,
        "summary": if managed_running {
            "d-token local service is running and managed by this app"
        } else if external_or_unmanaged {
            "A d-token-compatible service is responding, but it was not started by this app"
        } else {
            "d-token local service is not running"
        },
        "privacy": "Only d-token-owned service manifest and 127.0.0.1 health endpoint are inspected. External Headroom, RTK, cc-switch, shell profiles, and agent configs are not touched."
    })
}

fn validate_active_config_for_start() -> Result<Value, String> {
    let config = read_app_state(&["active-config", "dtoken-config.json"])
        .map_err(|err| format!("No applied d-token route configuration is available: {err}"))?;
    let providers = config
        .get("providers")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config providers must be an array".to_string())?;
    let routes = config
        .get("routes")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config routes must be an array".to_string())?;
    let instances = config
        .get("instances")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config instances must be an array".to_string())?;
    if providers.is_empty() || routes.is_empty() || instances.is_empty() {
        return Err(
            "Active config is empty; apply a real provider, route, and instance before starting"
                .to_string(),
        );
    }
    if !routes.iter().any(|route| {
        route.get("local_port").and_then(Value::as_u64) == Some(u64::from(DTOKEN_SERVICE_PORT))
            && route.get("bind_host").and_then(Value::as_str) == Some("127.0.0.1")
    }) {
        return Err(format!(
            "Active config has no d-token-owned 127.0.0.1:{DTOKEN_SERVICE_PORT} route"
        ));
    }
    for provider in providers {
        let Some(base_url) = provider.get("base_url").and_then(Value::as_str) else {
            return Err("Active config provider is missing base_url".to_string());
        };
        if provider_targets_local_listener(base_url, DTOKEN_SERVICE_PORT) {
            return Err("Active config contains a provider self-loop into d-token".to_string());
        }
    }
    Ok(config)
}

fn open_log_file(path: &Path) -> Result<fs::File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| err.to_string())
}

fn check_headroom_endpoint(port: u16, label: &str) -> HeadroomEndpoint {
    match simple_http_get("127.0.0.1", port, "/health") {
        Ok(raw) => {
            let healthy = raw.contains("\"healthy\"")
                || raw.contains("\"ready\":true")
                || raw.contains("healthy")
                || raw.contains("ready");
            HeadroomEndpoint {
                port,
                label: label.to_string(),
                healthy,
                detail: if healthy {
                    format!("127.0.0.1:{port}/health responded")
                } else {
                    format!("127.0.0.1:{port}/health responded but was not clearly healthy")
                },
            }
        }
        Err(err) => HeadroomEndpoint {
            port,
            label: label.to_string(),
            healthy: false,
            detail: format!("127.0.0.1:{port}/health failed: {err}"),
        },
    }
}

fn headroom_label(port: u16) -> &'static str {
    match port {
        EXTERNAL_HEADROOM_REFERENCE_PORT => "External Headroom reference",
        LEGACY_OPENCODE_REFERENCE_PORT => "Legacy OpenCode reference",
        _ => "External Headroom reference",
    }
}

fn headroom_endpoints() -> Vec<HeadroomEndpoint> {
    vec![
        check_headroom_endpoint(
            EXTERNAL_HEADROOM_REFERENCE_PORT,
            "External Headroom reference",
        ),
        check_headroom_endpoint(LEGACY_OPENCODE_REFERENCE_PORT, "Legacy OpenCode reference"),
    ]
}

fn all_checks() -> Vec<ToolCheck> {
    vec![
        check_tool("rtk", "RTK"),
        check_tool("headroom", "Headroom"),
        check_tool("opencode", "OpenCode"),
    ]
}

fn install_or_verify(id: &str) -> ActionResult {
    let label = match id {
        "rtk" => "RTK",
        "headroom" => "Headroom",
        _ => "Unknown tool",
    };
    let mut logs = vec![
        log("info", format!("Starting {label} install check")),
        log("info", format!("Looking for {id} on PATH")),
    ];

    let check = check_tool(id, label);
    if check.installed {
        logs.push(log(
            "info",
            format!(
                "{label} already installed: {}",
                check
                    .version
                    .clone()
                    .unwrap_or_else(|| "version unknown".to_string())
            ),
        ));
        logs.push(log(
            "info",
            format!("{label} verification finished successfully"),
        ));
        return ActionResult {
            ok: true,
            summary: format!("{label} already installed and verified"),
            logs,
            tools: all_checks(),
        };
    }

    logs.push(log(
        "error",
        format!("{label} is missing; automatic installer is not configured for this machine yet"),
    ));
    logs.push(log(
        "warn",
        "No files were changed. Add a signed installer source before enabling write actions.",
    ));
    ActionResult {
        ok: false,
        summary: format!("{label} is missing"),
        logs,
        tools: all_checks(),
    }
}

#[tauri::command]
fn save_app_settings(section: String, payload: Value) -> ActionResult {
    let allowed = ["rtk", "headroom", "app", "project-policy", "routing"];
    let mut logs = vec![log(
        "info",
        format!("Saving d-token settings section: {section}"),
    )];
    if !allowed.contains(&section.as_str()) {
        logs.push(log(
            "error",
            format!("Unsupported settings section: {section}"),
        ));
        return ActionResult {
            ok: false,
            summary: "Settings section is not supported".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let existing = read_app_state(&["settings", &format!("{section}.json")])
        .ok()
        .and_then(|value| value.get("payload").cloned())
        .unwrap_or_else(|| json!({}));
    let mut merged_payload = existing;
    merge_json_objects(&mut merged_payload, payload);
    let content = json!({
        "section": section,
        "savedAt": now_label(),
        "payload": merged_payload,
        "privacy": "d-token local state only; no API keys or raw CCR content should be stored here"
    });
    match write_json_app_state(
        &[
            "settings",
            &format!("{}.json", content["section"].as_str().unwrap_or("app")),
        ],
        &content,
    ) {
        Ok(path) => {
            logs.push(log("info", format!("Settings saved to {}", path.display())));
            ActionResult {
                ok: true,
                summary: "Settings saved".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            logs.push(log("error", format!("Settings save failed: {err}")));
            ActionResult {
                ok: false,
                summary: "Settings save failed".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn get_app_settings(section: String) -> Value {
    let allowed = ["rtk", "headroom", "app", "project-policy", "routing"];
    if !allowed.contains(&section.as_str()) {
        return json!({
            "ok": false,
            "section": section,
            "source": "d-token-local-settings",
            "state": "unsupported_section",
            "payload": {}
        });
    }
    match read_app_state(&["settings", &format!("{section}.json")]) {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.insert("ok".to_string(), json!(true));
                object.insert("source".to_string(), json!("d-token-local-settings"));
                object.insert("state".to_string(), json!("loaded"));
            }
            value
        }
        Err(err) => json!({
            "ok": false,
            "section": section,
            "source": "d-token-local-settings",
            "state": "missing",
            "error": err,
            "payload": {}
        }),
    }
}

fn safe_slug(value: &str, fallback: &str) -> String {
    let slug = value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch == '-' || ch == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        fallback.to_string()
    } else {
        slug
    }
}

fn parse_protocol(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "anthropic" => "anthropic",
        _ => "open_ai",
    }
}

fn parse_provider_kind(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "official" => "official",
        "local_chain" | "cc_switch" | "cc-switch" => "local_chain",
        _ => "custom",
    }
}

fn parse_chain_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "chain_bridge" | "compress_then_cc_switch" | "cc_switch" | "cc-switch" => "chain_bridge",
        _ => "direct",
    }
}

fn parse_compression_policy(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => "off",
        "safe" => "safe",
        "aggressive" => "aggressive",
        "max" => "max",
        _ => "balanced",
    }
}

fn provider_targets_local_listener(base_url: &str, local_port: u16) -> bool {
    url::Url::parse(base_url).ok().is_some_and(|parsed| {
        let host_is_local = match parsed.host() {
            Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(address)) => address.is_loopback(),
            Some(url::Host::Ipv6(address)) => address.is_loopback(),
            None => false,
        };
        host_is_local && parsed.port_or_known_default() == Some(local_port)
    })
}

fn normalize_credential_reference(value: Option<String>) -> Result<Option<String>, ()> {
    let Some(value) = value.map(|value| value.trim().to_string()) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    let parsed = url::Url::parse(&value).map_err(|_| ())?;
    let allowed_scheme = matches!(parsed.scheme(), "os-credential" | "wincred" | "keychain");
    let has_target = parsed
        .host_str()
        .is_some_and(|host| !host.trim().is_empty())
        || !parsed.path().trim_matches('/').is_empty();
    if !allowed_scheme
        || !has_target
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(());
    }
    Ok(Some(value))
}

fn credential_reference_for_provider(provider_id: &str) -> (String, String) {
    let mut provider_slug = String::new();
    let mut last_was_separator = true;
    for character in provider_id.trim().chars() {
        if character.is_ascii_alphanumeric() {
            provider_slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if matches!(character, '-' | '_' | ' ') && !last_was_separator {
            provider_slug.push('-');
            last_was_separator = true;
        }
    }
    let provider_id = provider_slug.trim_matches('-');
    let provider_id = if provider_id.is_empty() {
        "provider"
    } else {
        provider_id
    };
    let account = format!("providers/{provider_id}");
    (format!("os-credential://d-token/{account}"), account)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn platform_store_credential(account: &str, credential: &str) -> Result<(), ()> {
    let entry = keyring::Entry::new("d-token", account).map_err(|_| ())?;
    entry.set_password(credential).map_err(|_| ())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_store_credential(_account: &str, _credential: &str) -> Result<(), ()> {
    Err(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn platform_delete_credential(account: &str) -> Result<(), ()> {
    let entry = keyring::Entry::new("d-token", account).map_err(|_| ())?;
    entry.delete_credential().map_err(|_| ())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_delete_credential(_account: &str) -> Result<(), ()> {
    Err(())
}

#[tauri::command]
fn store_provider_credential(request: StoreProviderCredentialRequest) -> ActionResult {
    let credential = request.credential.trim();
    if credential.is_empty() || credential.len() > 65_536 {
        return ActionResult {
            ok: false,
            summary: "Provider credential is empty or too large".to_string(),
            logs: vec![log(
                "error",
                "Credential was not written to the OS credential store",
            )],
            tools: all_checks(),
        };
    }
    let (_, account) = credential_reference_for_provider(&request.provider_id);
    match platform_store_credential(&account, credential) {
        Ok(()) => ActionResult {
            ok: true,
            summary: "Provider credential saved in the operating-system credential store"
                .to_string(),
            logs: vec![log(
                "info",
                "Credential stored by reference; plaintext was not written to d-token config or logs",
            )],
            tools: all_checks(),
        },
        Err(()) => ActionResult {
            ok: false,
            summary: "Operating-system credential store is unavailable".to_string(),
            logs: vec![log(
                "error",
                "Credential was not written; no plaintext fallback is allowed",
            )],
            tools: all_checks(),
        },
    }
}

#[tauri::command]
fn delete_provider_credential(request: DeleteProviderCredentialRequest) -> ActionResult {
    let (_, account) = credential_reference_for_provider(&request.provider_id);
    match platform_delete_credential(&account) {
        Ok(()) => ActionResult {
            ok: true,
            summary: "Provider credential removed from the operating-system credential store"
                .to_string(),
            logs: vec![log("info", "Credential-store entry removed")],
            tools: all_checks(),
        },
        Err(()) => ActionResult {
            ok: false,
            summary: "Provider credential could not be removed".to_string(),
            logs: vec![log(
                "error",
                "Credential-store removal failed; d-token config was not modified",
            )],
            tools: all_checks(),
        },
    }
}

#[tauri::command]
fn save_route_profile_draft(request: RouteProfileDraftRequest) -> ActionResult {
    let mut logs = vec![log("info", "Saving d-token route/provider draft")];
    let provider_id = safe_slug(&request.provider_id, "provider");
    let route_id = safe_slug(&request.route_id, "route");
    let instance_id = format!("{route_id}-instance");
    let base_url = request.base_url.trim().trim_end_matches('/').to_string();
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        logs.push(log(
            "error",
            "Provider base URL must start with http:// or https://",
        ));
        return ActionResult {
            ok: false,
            summary: "Invalid provider base URL".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if request.local_port == 0 {
        logs.push(log("error", "Local port must be a positive TCP port"));
        return ActionResult {
            ok: false,
            summary: "Invalid local route port".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if provider_targets_local_listener(&base_url, request.local_port) {
        logs.push(log(
            "error",
            "Provider upstream cannot point back to the same d-token listener",
        ));
        return ActionResult {
            ok: false,
            summary: "Provider route would loop back into d-token".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let models = request
        .models
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let api_key_ref = match normalize_credential_reference(request.api_key_ref) {
        Ok(reference) => reference,
        Err(()) => {
            logs.push(log(
                "error",
                "Provider credential reference must be an opaque OS credential-store reference",
            ));
            return ActionResult {
                ok: false,
                summary: "Invalid provider credential reference".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let protocol = parse_protocol(&request.protocol);
    let content = json!({
        "source": "d-token-desktop",
        "savedAt": now_label(),
        "privacy": "d-token-owned routing draft only; api_key_ref is a reference and plaintext secrets are rejected.",
        "config": {
            "providers": [{
                "id": provider_id,
                "name": request.provider_name,
                "kind": parse_provider_kind(&request.provider_kind),
                "protocol": protocol,
                "base_url": base_url,
                "api_key_ref": api_key_ref,
                "models": models,
                "timeout_ms": 120000,
                "healthcheck_path": "/health"
            }],
            "routes": [{
                "id": route_id,
                "name": request.route_name,
                "input_protocol": protocol,
                "instance_id": instance_id,
                "target_provider_id": provider_id,
                "fallback_provider_id": Value::Null,
                "chain_mode": parse_chain_mode(&request.chain_mode),
                "compression_policy": parse_compression_policy(&request.compression_policy),
                "bind_host": "127.0.0.1",
                "local_port": request.local_port
            }],
            "instances": [{
                "id": instance_id,
                "name": format!("{} instance", request.agent),
                "agent": request.agent,
                "route_id": route_id,
                "local_port": request.local_port,
                "process_status": "stopped",
                "log_path": format!("./logs/{route_id}.jsonl")
            }]
        },
        "diff": [
            { "type": "ctx", "text": format!("[provider.{provider_id}]") },
            { "type": "add", "text": format!("+ base_url = \"{}\"", base_url) },
            { "type": "ctx", "text": format!("[route.{route_id}]") },
            { "type": "add", "text": "+ bind_host = \"127.0.0.1\"" },
            { "type": "add", "text": format!("+ local_port = {}", request.local_port) },
            { "type": "add", "text": "+ fallback = \"disabled\"" }
        ]
    });
    let Some(active_parent) = active_config_path().parent().map(Path::to_path_buf) else {
        logs.push(log("error", "Active config parent path is unavailable"));
        return ActionResult {
            ok: false,
            summary: "Routing draft could not provision local control-plane state".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    if let Err(err) = fs::create_dir_all(&active_parent) {
        logs.push(log(
            "error",
            format!("Active config directory create failed: {err}"),
        ));
        return ActionResult {
            ok: false,
            summary: "Routing draft could not provision local control-plane state".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    match write_json_app_state(&["routing", &format!("{route_id}.json")], &content) {
        Ok(path) => {
            logs.push(log(
                "info",
                format!("Routing draft saved to {}", path.display()),
            ));
            logs.push(log("info", "No OpenCode, Headroom, RTK, cc-switch, shell profile, or system proxy config was changed"));
            ActionResult {
                ok: true,
                summary: "Route/provider draft saved".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            logs.push(log("error", format!("Routing draft save failed: {err}")));
            ActionResult {
                ok: false,
                summary: "Routing draft save failed".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn list_route_profile_drafts() -> Value {
    let drafts = route_profile_drafts_json();
    json!({
        "source": "d-token-desktop",
        "drafts": drafts,
        "count": drafts.len(),
        "privacy": "d-token local routing drafts only; external agent configs are not read."
    })
}

fn prepare_route_profile_draft_apply(
    requested_route_id: &str,
) -> Result<(String, dtoken_config_patcher::PreparedChange), String> {
    let requested_route_id = requested_route_id.trim();
    let route_id = safe_slug(requested_route_id, "");
    if route_id.is_empty() || route_id != requested_route_id {
        return Err("Route draft id is invalid or not normalized".to_string());
    }
    let draft_path = app_state_dir()
        .join("routing")
        .join(format!("{route_id}.json"));
    let draft_content = match fs::read_to_string(&draft_path) {
        Ok(content) => content,
        Err(err) => return Err(format!("Route draft not found: {err}")),
    };
    let draft: Value = match serde_json::from_str(&draft_content) {
        Ok(value) => value,
        Err(err) => return Err(format!("Route draft parse failed: {err}")),
    };
    let Some(config) = draft.get("config").cloned() else {
        return Err("Route draft has no config section".to_string());
    };
    if config.to_string().contains("sk-") {
        return Err("Route draft contains a plaintext-looking secret".to_string());
    }
    let serialized_config = serde_json::to_string_pretty(&config)
        .map_err(|err| format!("Active config serialize failed: {err}"))?;
    let target_path = patchable_active_config_path()?;
    let prepared = config_patcher()
        .prepare_change(
            target_path,
            &serialized_config,
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Medium,
        )
        .map_err(|err| format!("Active config plan failed: {err:#}"))?;
    Ok((route_id, prepared))
}

#[tauri::command]
fn preview_route_profile_draft_apply(request: RouteDraftReviewRequest) -> Value {
    match prepare_route_profile_draft_apply(&request.route_id) {
        Ok((route_id, prepared)) => json!({
            "ok": true,
            "source": "dtoken-desktop-control-plane",
            "mode": "route-apply-review",
            "route_id": route_id,
            "target": prepared.plan.target_file,
            "before_sha256": prepared.plan.before_hash,
            "after_sha256": prepared.plan.after_hash,
            "diff": prepared.plan.diff,
            "risk_level": prepared.plan.risk_level,
            "requires_explicit_confirmation": true,
            "runtime_required": false,
            "privacy": "Read-only review. No config, backup, runtime, external agent, or credential state was changed."
        }),
        Err(err) => json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "mode": "route-apply-review",
            "error": sanitize_command_output(&err),
            "requires_explicit_confirmation": true,
            "runtime_required": false,
        }),
    }
}

#[tauri::command]
fn apply_route_profile_draft(request: ApplyRouteDraftRequest) -> ActionResult {
    let mut logs = vec![log(
        "info",
        format!(
            "Applying reviewed d-token route draft: {}",
            request.route_id
        ),
    )];
    let (Some(reviewed_before), Some(reviewed_after)) = (
        request.reviewed_before_sha256.as_deref(),
        request.reviewed_after_sha256.as_deref(),
    ) else {
        logs.push(log(
            "error",
            "Apply rejected because no reviewed before/after hashes were supplied",
        ));
        return ActionResult {
            ok: false,
            summary: "Review the route diff before applying it".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    let (route_id, prepared) = match prepare_route_profile_draft_apply(&request.route_id) {
        Ok(prepared) => prepared,
        Err(err) => {
            logs.push(log("error", sanitize_command_output(&err)));
            return ActionResult {
                ok: false,
                summary: "Route draft could not be prepared for apply".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    if prepared.plan.before_hash != reviewed_before || prepared.plan.after_hash != reviewed_after {
        logs.push(log(
            "error",
            "Apply rejected because the active config or route draft changed after review",
        ));
        return ActionResult {
            ok: false,
            summary: "Route review is stale; review the current diff again".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let transaction = match config_patcher().apply_transaction(&[prepared]) {
        Ok(transaction) => transaction,
        Err(err) => {
            logs.push(log(
                "error",
                format!("Active config transaction failed: {err:#}"),
            ));
            return ActionResult {
                ok: false,
                summary: "Active config transaction failed and was not committed".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let Some(receipt) = transaction.changes.first() else {
        logs.push(log(
            "error",
            "Active config transaction returned no receipt",
        ));
        return ActionResult {
            ok: false,
            summary: "Active config transaction did not return verification evidence".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    logs.push(log(
        "info",
        format!(
            "Verified active config transaction {} for route {route_id}; rollback point {}",
            transaction.transaction_id, receipt.backup.rollback_id
        ),
    ));
    logs.push(log(
        "info",
        "No OpenCode, Headroom, RTK, cc-switch, shell profile, or system proxy config was changed",
    ));
    ActionResult {
        ok: true,
        summary: "Reviewed route draft applied and verified with a durable rollback point"
            .to_string(),
        logs,
        tools: all_checks(),
    }
}

#[tauri::command]
fn recommend_project_policy(request: ProjectPolicyRecommendRequest) -> Value {
    let input = dtoken_project_policy::RecommendationInput {
        project_id: request
            .project_id
            .unwrap_or_else(|| "desktop-project".to_string()),
        language_stack: request
            .language_stack
            .unwrap_or_else(|| vec!["rust".to_string(), "typescript".to_string()]),
        avg_request_tokens_7d: request.avg_request_tokens_7d.unwrap_or(47_307),
        avg_cli_noise_ratio_7d: request.avg_cli_noise_ratio_7d.unwrap_or(0.42),
        retrieve_rate_7d: request.retrieve_rate_7d.unwrap_or(0.02),
        failure_rate_7d: request.failure_rate_7d.unwrap_or(0.0),
        high_risk_task: request.high_risk_task.unwrap_or(false),
        manual_override: request.manual_override.unwrap_or(false),
    };
    let recommendation = dtoken_project_policy::RuleBasedRecommendationEngine.recommend(input);
    json!({
        "source": "dtoken-project-policy",
        "runtime": "d-token-owned",
        "recommendation": recommendation,
        "mapping": {
            "enable_ccr": true,
            "enable_memory": true,
            "enable_learn": true,
            "enable_output_shaper": true,
            "holdout_ratio": 0.1
        },
        "privacy": "Recommendation uses aggregate project/runtime metrics only; raw prompt/output is not read."
    })
}

#[tauri::command]
fn manage_backup(action: String, id: String) -> ActionResult {
    let mut logs = vec![log(
        "info",
        format!("Backup action requested: {action} {id}"),
    )];
    let safe_id = id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    if safe_id.is_empty() || safe_id != id {
        logs.push(log("error", "Invalid rollback point id"));
        return ActionResult {
            ok: false,
            summary: "Invalid rollback point id".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let config_backup_path = config_backup_root().join(&safe_id);
    if fs::symlink_metadata(&config_backup_path).is_ok() {
        return match action.as_str() {
            "restore" => {
                let patcher = config_patcher();
                let backup = match patcher.load_backup(&safe_id) {
                    Ok(backup) => backup,
                    Err(err) => {
                        logs.push(log(
                            "error",
                            format!("Config backup verification failed: {err:#}"),
                        ));
                        return ActionResult {
                            ok: false,
                            summary: "Config rollback point failed integrity verification"
                                .to_string(),
                            logs,
                            tools: all_checks(),
                        };
                    }
                };
                let expected = ExpectedFileState::Hash(backup.planned_after_sha256.clone());
                match patcher.restore_backup(&backup, &expected) {
                    Ok(receipt) => {
                        logs.push(log(
                            "info",
                            format!(
                                "Verified config rollback {} to {}",
                                receipt.rollback_id,
                                receipt.target_path.display()
                            ),
                        ));
                        ActionResult {
                            ok: true,
                            summary: "d-token active config restored from verified backup"
                                .to_string(),
                            logs,
                            tools: all_checks(),
                        }
                    }
                    Err(err) => {
                        logs.push(log("error", format!("Config rollback refused: {err:#}")));
                        ActionResult {
                            ok: false,
                            summary: "Config rollback refused because current state or backup evidence changed"
                                .to_string(),
                            logs,
                            tools: all_checks(),
                        }
                    }
                }
            }
            "cleanup" => {
                logs.push(log(
                    "warn",
                    "Verified config backups are retained; cleanup is not available until an audited retention API is implemented",
                ));
                ActionResult {
                    ok: false,
                    summary: "Verified config backup retention is enforced".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
            _ => {
                logs.push(log("error", format!("Unsupported backup action: {action}")));
                ActionResult {
                    ok: false,
                    summary: "Unsupported backup action".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
        };
    }

    let path = app_state_dir()
        .join("rollback-points")
        .join(format!("{safe_id}.json"));
    match action.as_str() {
        "restore" => match fs::read_to_string(&path) {
            Ok(content) => {
                let parsed: Value = serde_json::from_str(&content).unwrap_or_else(|_| json!({}));
                logs.push(log(
                    "info",
                    format!("Restore plan loaded from {}", path.display()),
                ));
                logs.push(log(
                    "info",
                    format!("Restore manifest bytes: {}", content.len()),
                ));
                let active_path = active_config_path();
                let is_dtoken_active_config = parsed
                    .get("target")
                    .and_then(Value::as_str)
                    .map(|target| target == "d-token active route config")
                    .unwrap_or(false);
                if is_dtoken_active_config {
                    match parsed.get("previous_config") {
                        Some(Value::String(previous)) => {
                            if let Err(err) = serde_json::from_str::<Value>(previous) {
                                logs.push(log(
                                    "error",
                                    format!("Previous active config is not valid JSON: {err}"),
                                ));
                                return ActionResult {
                                    ok: false,
                                    summary: "Rollback manifest invalid".to_string(),
                                    logs,
                                    tools: all_checks(),
                                };
                            }
                            if let Err(err) =
                                durable_atomic_write(&active_path, previous.as_bytes())
                            {
                                logs.push(log(
                                    "error",
                                    format!("Active config restore failed: {err}"),
                                ));
                                return ActionResult {
                                    ok: false,
                                    summary: "Rollback restore failed".to_string(),
                                    logs,
                                    tools: all_checks(),
                                };
                            }
                            logs.push(log(
                                "info",
                                format!(
                                    "Restored previous d-token active config to {}",
                                    active_path.display()
                                ),
                            ));
                            logs.push(log("info", "No external agent, Headroom, RTK, cc-switch, shell profile, or system proxy config was changed"));
                            ActionResult {
                                ok: true,
                                summary: "d-token active config restored".to_string(),
                                logs,
                                tools: all_checks(),
                            }
                        }
                        Some(Value::Null) | None => {
                            if active_path.exists() {
                                if let Err(err) = durable_remove_file(&active_path) {
                                    logs.push(log(
                                        "error",
                                        format!("Active config remove failed: {err}"),
                                    ));
                                    return ActionResult {
                                        ok: false,
                                        summary: "Rollback restore failed".to_string(),
                                        logs,
                                        tools: all_checks(),
                                    };
                                }
                            }
                            logs.push(log("info", "Removed d-token active config because the rollback point had no previous config"));
                            logs.push(log("info", "No external agent, Headroom, RTK, cc-switch, shell profile, or system proxy config was changed"));
                            ActionResult {
                                ok: true,
                                summary: "d-token active config restored to empty state"
                                    .to_string(),
                                logs,
                                tools: all_checks(),
                            }
                        }
                        _ => {
                            logs.push(log("error", "Rollback manifest previous_config is invalid"));
                            ActionResult {
                                ok: false,
                                summary: "Rollback manifest invalid".to_string(),
                                logs,
                                tools: all_checks(),
                            }
                        }
                    }
                } else {
                    logs.push(log(
                        "warn",
                        "External config restore is gated; this action validates and stages the rollback plan only",
                    ));
                    ActionResult {
                        ok: true,
                        summary: "Rollback point staged for restore".to_string(),
                        logs,
                        tools: all_checks(),
                    }
                }
            }
            Err(err) => {
                logs.push(log("error", format!("Restore plan not found: {err}")));
                ActionResult {
                    ok: false,
                    summary: "Rollback point not found".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
        },
        "cleanup" => match fs::remove_file(&path) {
            Ok(()) => {
                logs.push(log(
                    "info",
                    format!("Removed rollback point {}", path.display()),
                ));
                ActionResult {
                    ok: true,
                    summary: "Rollback point cleaned up".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
            Err(err) => {
                logs.push(log("error", format!("Cleanup failed: {err}")));
                ActionResult {
                    ok: false,
                    summary: "Rollback cleanup failed".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
        },
        _ => {
            logs.push(log("error", format!("Unsupported backup action: {action}")));
            ActionResult {
                ok: false,
                summary: "Unsupported backup action".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn detect_tools() -> ActionResult {
    let tools = all_checks();
    let mut logs = vec![log(
        "info",
        "Scanning local PATH for RTK, Headroom, and OpenCode",
    )];
    for tool in &tools {
        let level = if tool.installed { "info" } else { "warn" };
        logs.push(log(
            level,
            format!(
                "{}: {}",
                tool.label,
                tool.version.clone().unwrap_or_else(|| tool.detail.clone())
            ),
        ));
    }
    ActionResult {
        ok: tools.iter().all(|tool| tool.installed),
        summary: "Local tool scan complete".to_string(),
        logs,
        tools,
    }
}

#[tauri::command]
fn install_tool(target: String) -> ActionResult {
    match target.as_str() {
        "rtk" | "headroom" => install_or_verify(&target),
        "all" => {
            let mut logs = vec![log("info", "Starting one-click install workflow")];
            let rtk = install_or_verify("rtk");
            logs.extend(rtk.logs);
            let headroom = install_or_verify("headroom");
            logs.extend(headroom.logs);
            logs.push(log("info", "Running final local verification"));
            let tools = all_checks();
            let ok = tools
                .iter()
                .filter(|tool| tool.id == "rtk" || tool.id == "headroom")
                .all(|tool| tool.installed);
            logs.push(log(
                if ok { "info" } else { "error" },
                if ok {
                    "One-click workflow completed successfully"
                } else {
                    "One-click workflow finished with missing tools"
                },
            ));
            ActionResult {
                ok,
                summary: if ok {
                    "RTK and Headroom verified".to_string()
                } else {
                    "Some tools are still missing".to_string()
                },
                logs,
                tools,
            }
        }
        _ => ActionResult {
            ok: false,
            summary: format!("Unknown install target: {target}"),
            logs: vec![log("error", format!("Unknown install target: {target}"))],
            tools: all_checks(),
        },
    }
}

#[tauri::command]
fn preview_install_plan(request: InstallPlanRequest) -> Value {
    install_plan_json(&request)
}

#[tauri::command]
fn get_dtoken_service_status() -> Value {
    dtoken_service_status_value()
}

#[tauri::command]
fn start_dtoken_service() -> ActionResult {
    let mut logs = vec![log(
        "info",
        "Starting d-token-owned local service lifecycle action",
    )];
    let current = dtoken_service_status_value();
    if current.get("state").and_then(Value::as_str) == Some("running") {
        logs.push(log(
            "info",
            "d-token local service is already running and managed by this app",
        ));
        return ActionResult {
            ok: true,
            summary: "d-token local service is already running".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if current.get("state").and_then(Value::as_str) == Some("unmanaged_running") {
        logs.push(log(
            "warn",
            "127.0.0.1:18001 is responding as d-token but was not started by this app; leaving it untouched",
        ));
        return ActionResult {
            ok: false,
            summary: "Unmanaged d-token-compatible service is already using the port".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    if let Err(err) = validate_active_config_for_start() {
        logs.push(log(
            "error",
            format!("Active config validation failed: {err}"),
        ));
        return ActionResult {
            ok: false,
            summary: "Configure and apply a real d-token route before starting the service"
                .to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let exe = match resolve_dtoken_proxy_exe() {
        Ok(path) => path,
        Err(err) => {
            logs.push(log("error", err));
            return ActionResult {
                ok: false,
                summary: "d-token runtime executable is missing".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let config_path = active_config_path();
    let data_dir = service_data_dir();
    if let Err(err) = fs::create_dir_all(&data_dir) {
        logs.push(log(
            "error",
            format!("Runtime data dir create failed: {err}"),
        ));
        return ActionResult {
            ok: false,
            summary: "d-token service start failed before launch".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    let log_path = service_log_path();
    let stdout = match open_log_file(&log_path) {
        Ok(file) => file,
        Err(err) => {
            logs.push(log("error", format!("Service log open failed: {err}")));
            return ActionResult {
                ok: false,
                summary: "d-token service start failed before launch".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let stderr = match stdout.try_clone() {
        Ok(file) => file,
        Err(err) => {
            logs.push(log("error", format!("Service log clone failed: {err}")));
            return ActionResult {
                ok: false,
                summary: "d-token service start failed before launch".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };

    let child = Command::new(&exe)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(DTOKEN_SERVICE_PORT.to_string())
        .arg("--config")
        .arg(&config_path)
        .arg("--data-dir")
        .arg(&data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(err) => {
            logs.push(log("error", format!("Service process spawn failed: {err}")));
            return ActionResult {
                ok: false,
                summary: "d-token service start failed".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let pid = child.id();
    logs.push(log(
        "info",
        format!(
            "spawn: d-token runtime pid={} exe={} port={}",
            pid,
            exe.display(),
            DTOKEN_SERVICE_PORT
        ),
    ));

    let mut healthy = false;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(100));
        let health = service_health_payload();
        healthy = health
            .as_ref()
            .and_then(|payload| payload.get("component").and_then(Value::as_str))
            .map(|component| component == "dtoken-core-api")
            .unwrap_or(false);
        if healthy {
            break;
        }
    }

    if !healthy {
        logs.push(log(
            "error",
            "verify: d-token service did not become healthy on 127.0.0.1:18001",
        ));
        let _ = terminate_process(pid);
        let _ = remove_service_manifest();
        return ActionResult {
            ok: false,
            summary: "d-token service did not become healthy".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    if !executable_identity_matches(pid, &exe) || !listener_owned_by_pid(pid, DTOKEN_SERVICE_PORT) {
        let _ = terminate_process(pid);
        let _ = remove_service_manifest();
        logs.push(log(
            "error",
            "verify: spawned PID, executable, and listening port ownership could not be proven",
        ));
        return ActionResult {
            ok: false,
            summary: "d-token service ownership verification failed".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let manifest = ManagedServiceManifest {
        managed_by: "d-token-desktop".to_string(),
        identity: "dtoken-core-api".to_string(),
        pid,
        port: DTOKEN_SERVICE_PORT,
        exe_path: exe.display().to_string(),
        config_path: config_path.display().to_string(),
        data_dir: data_dir.display().to_string(),
        started_at: now_label(),
    };
    let manifest_path = match write_service_manifest(&manifest) {
        Ok(path) => path,
        Err(err) => {
            let _ = terminate_process(pid);
            let _ = remove_service_manifest();
            logs.push(log(
                "error",
                format!("Durable service manifest write failed; process was stopped: {err}"),
            ));
            return ActionResult {
                ok: false,
                summary:
                    "d-token service start rolled back because lifecycle state was not durable"
                        .to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    if read_service_manifest().as_ref() != Some(&manifest) {
        let _ = terminate_process(pid);
        let _ = remove_service_manifest();
        logs.push(log(
            "error",
            "Service manifest verification failed; process was stopped",
        ));
        return ActionResult {
            ok: false,
            summary:
                "d-token service start rolled back because lifecycle state verification failed"
                    .to_string(),
            logs,
            tools: all_checks(),
        };
    }
    logs.push(log(
        "info",
        format!(
            "manifest: d-token service manifest durably written to {}",
            manifest_path.display()
        ),
    ));
    logs.push(log(
        "info",
        "verify: d-token local service is healthy and managed by this app",
    ));
    ActionResult {
        ok: true,
        summary: "d-token local service started".to_string(),
        logs,
        tools: all_checks(),
    }
}

#[tauri::command]
fn stop_dtoken_service() -> ActionResult {
    let mut logs = vec![log(
        "info",
        "Stopping d-token-owned local service lifecycle action",
    )];
    let Some(manifest) = read_service_manifest() else {
        logs.push(log(
            "warn",
            "No d-token service manifest exists; no process will be stopped",
        ));
        return ActionResult {
            ok: false,
            summary: "No managed d-token service is registered".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    if manifest.managed_by != "d-token-desktop"
        || manifest.identity != "dtoken-core-api"
        || manifest.port != DTOKEN_SERVICE_PORT
    {
        logs.push(log(
            "error",
            "Service manifest identity did not match d-token desktop; refusing to stop process",
        ));
        return ActionResult {
            ok: false,
            summary: "Service identity check failed".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if !is_process_running(manifest.pid) {
        logs.push(log(
            "warn",
            format!(
                "Manifest pid {} is not running; clearing stale manifest",
                manifest.pid
            ),
        ));
        let _ = remove_service_manifest();
        return ActionResult {
            ok: true,
            summary: "Stale d-token service manifest cleared".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if !executable_identity_matches(manifest.pid, Path::new(&manifest.exe_path)) {
        logs.push(log(
            "error",
            "Manifest PID is running but its executable does not match the managed d-token runtime; refusing to stop it",
        ));
        return ActionResult {
            ok: false,
            summary: "Service executable identity check failed".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if !listener_owned_by_pid(manifest.pid, manifest.port) {
        logs.push(log(
            "error",
            "Manifest PID does not own the d-token listening port; refusing to stop it",
        ));
        return ActionResult {
            ok: false,
            summary: "Service port ownership check failed".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    let health = service_health_payload();
    let health_identity_ok = health
        .as_ref()
        .and_then(|payload| payload.get("component").and_then(Value::as_str))
        .map(|component| component == "dtoken-core-api")
        .unwrap_or(false);
    if !health_identity_ok {
        logs.push(log(
            "error",
            "Service health identity could not be verified on 127.0.0.1:18001; refusing to stop process",
        ));
        return ActionResult {
            ok: false,
            summary: "Service health identity check failed".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    match terminate_process(manifest.pid) {
        Ok(()) => {
            if let Err(err) = remove_service_manifest() {
                logs.push(log(
                    "error",
                    format!("Managed service stopped but manifest cleanup failed: {err}"),
                ));
                return ActionResult {
                    ok: false,
                    summary: "d-token service stopped but lifecycle state cleanup failed"
                        .to_string(),
                    logs,
                    tools: all_checks(),
                };
            }
            logs.push(log(
                "info",
                format!("Stopped managed d-token service pid {}", manifest.pid),
            ));
            ActionResult {
                ok: true,
                summary: "d-token local service stopped".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            logs.push(log("error", format!("Process stop failed: {err}")));
            ActionResult {
                ok: false,
                summary: "d-token local service stop failed".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn run_install_transaction(request: InstallPlanRequest) -> ActionResult {
    let targets = install_targets(&request.target);
    let mut logs = vec![
        log(
            "info",
            format!(
                "scan: starting installation transaction target={} mode={}",
                request.target, request.mode
            ),
        ),
        log(
            "info",
            "scope: OpenCode connection route is scoped to d-token; Claude Code, Cursor, Codex CLI and system proxy settings are excluded",
        ),
    ];

    if targets.is_empty() {
        logs.push(log(
            "error",
            format!("plan: unknown install target {}", request.target),
        ));
        return ActionResult {
            ok: false,
            summary: "Installation target is unknown".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let plan = install_plan_json(&request);
    logs.push(log(
        "info",
        format!(
            "plan: {} target(s), install_root={}, dtoken_route_port={}",
            targets.len(),
            request.install_root,
            request.proxy_port
        ),
    ));
    logs.push(log(
        "info",
        "diff: generated config diff for user confirmation",
    ));

    let writes_enabled = matches!(request.mode.as_str(), "install" | "repair" | "connect");
    let backup_persisted = if writes_enabled && request.auto_backup {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        match write_json_app_state(&["backups", &stamp, "install-plan.json"], &plan) {
            Ok(path) => {
                logs.push(log(
                    "info",
                    format!(
                        "backup: wrote d-token rollback manifest to {}",
                        path.display()
                    ),
                ));
                true
            }
            Err(err) => {
                logs.push(log(
                    "error",
                    format!("backup: failed to write manifest: {err}"),
                ));
                false
            }
        }
    } else if writes_enabled {
        logs.push(log(
            "warn",
            "backup: automatic backup is disabled for this explicit state-changing action",
        ));
        true
    } else {
        logs.push(log(
            "info",
            "backup: read-only mode does not persist a backup or rollback artifact",
        ));
        true
    };

    if !writes_enabled {
        logs.push(log(
            "info",
            "apply: current mode is read-only/manual, so external tool installation is verified but not changed",
        ));
    }

    let mut ok = backup_persisted;
    if !backup_persisted && writes_enabled {
        logs.push(log(
            "error",
            "apply: aborted because the requested recovery manifest was not durable",
        ));
        return ActionResult {
            ok: false,
            summary: "Installation transaction aborted before apply".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    for target in &targets {
        match *target {
            "rtk" => {
                let check = check_tool("rtk", "RTK");
                if check.installed {
                    logs.push(log(
                        "info",
                        format!(
                            "apply: RTK already installed and verified ({})",
                            check
                                .version
                                .unwrap_or_else(|| "version unknown".to_string())
                        ),
                    ));
                } else if writes_enabled {
                    ok = false;
                    logs.push(log(
                        "error",
                        "apply: RTK is missing and no signed RTK installer source is configured yet",
                    ));
                } else {
                    ok = false;
                    logs.push(log("warn", "verify: RTK is missing in read-only mode"));
                }
            }
            "headroom" => {
                let check = check_tool("headroom", "Headroom");
                if check.installed {
                    logs.push(log(
                        "info",
                        format!(
                            "apply: Headroom already installed and verified ({})",
                            check
                                .version
                                .unwrap_or_else(|| "version unknown".to_string())
                        ),
                    ));
                } else if writes_enabled {
                    ok = false;
                    logs.push(log(
                        "error",
                        "apply: Headroom is missing and no signed Headroom installer source is configured yet",
                    ));
                } else {
                    ok = false;
                    logs.push(log("warn", "verify: Headroom is missing in read-only mode"));
                }
            }
            "dtoken" => {
                match env::current_exe() {
                    Ok(path) => logs.push(log(
                        "info",
                        format!(
                            "apply: d-token desktop binary verified at {}",
                            path.display()
                        ),
                    )),
                    Err(err) => {
                        ok = false;
                        logs.push(log(
                            "error",
                            format!("apply: d-token binary check failed: {err}"),
                        ));
                    }
                }
                if writes_enabled || request.mode == "connect" {
                    let service = start_dtoken_service();
                    ok &= service.ok;
                    logs.extend(service.logs);
                }
                let service_status = dtoken_service_status_value();
                logs.push(log(
                    if service_status.get("state").and_then(Value::as_str) == Some("running") {
                        "info"
                    } else {
                        "warn"
                    },
                    format!(
                        "verify: d-token service lifecycle state={} managed={} url={}",
                        service_status
                            .get("state")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown"),
                        service_status
                            .get("managed")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        service_status
                            .get("service_url")
                            .and_then(Value::as_str)
                            .unwrap_or("http://127.0.0.1:18001")
                    ),
                ));
            }
            _ => {}
        }
    }

    let tools = all_checks();
    logs.push(log("info", "verify: refreshed local tool scan"));
    for endpoint in headroom_endpoints() {
        logs.push(log(
            if endpoint.healthy { "info" } else { "warn" },
            format!("verify: {} -> {}", endpoint.label, endpoint.detail),
        ));
    }

    if request.run_doctor_after {
        let doctor_ok = tools
            .iter()
            .filter(|tool| tool.id == "rtk" || tool.id == "headroom")
            .all(|tool| tool.installed);
        ok = ok && doctor_ok;
        logs.push(log(
            if doctor_ok { "info" } else { "error" },
            if doctor_ok {
                "doctor: install prerequisites passed"
            } else {
                "doctor: install prerequisites failed"
            },
        ));
    } else {
        logs.push(log("warn", "doctor: skipped by user option"));
    }

    if request.rollback_point && writes_enabled {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let rollback = json!({
            "createdAt": now_label(),
            "target": request.target,
            "mode": request.mode,
            "ok": ok,
            "tools": tools,
            "note": "d-token-owned rollback point; external restore remains gated behind explicit restore action"
        });
        match write_json_app_state(&["rollback-points", &format!("{stamp}.json")], &rollback) {
            Ok(path) => logs.push(log(
                "info",
                format!("rollback: committed rollback point {}", path.display()),
            )),
            Err(err) => {
                ok = false;
                logs.push(log(
                    "error",
                    format!("rollback: failed to write point: {err}"),
                ));
            }
        }
    } else if writes_enabled {
        logs.push(log(
            "warn",
            "rollback: rollback point disabled by user option",
        ));
    } else {
        logs.push(log(
            "info",
            "rollback: read-only mode does not create a rollback point",
        ));
    }

    ActionResult {
        ok,
        summary: if ok {
            format!("{} installation transaction completed", request.target)
        } else {
            format!(
                "{} installation transaction needs attention",
                request.target
            )
        },
        logs,
        tools: all_checks(),
    }
}

#[tauri::command]
fn run_doctor() -> ActionResult {
    let tools = all_checks();
    let mut logs = vec![log("info", "Running desktop Doctor checks")];
    for tool in &tools {
        logs.push(log(
            if tool.installed { "info" } else { "warn" },
            format!(
                "{} status: {}",
                tool.label,
                if tool.installed {
                    "installed"
                } else {
                    "missing"
                }
            ),
        ));
    }
    let ok = tools
        .iter()
        .filter(|tool| tool.id != "opencode")
        .all(|tool| tool.installed);
    logs.push(log(
        if ok { "info" } else { "error" },
        if ok {
            "Doctor passed for install prerequisites"
        } else {
            "Doctor found missing install prerequisites"
        },
    ));
    ActionResult {
        ok,
        summary: "Doctor check complete".to_string(),
        logs,
        tools,
    }
}

#[tauri::command]
fn manage_headroom(action: String, port: u16) -> ActionResult {
    let mut logs = vec![log(
        "info",
        format!("External Headroom reference action requested: {action} on 127.0.0.1:{port}"),
    )];
    if !(port == EXTERNAL_HEADROOM_REFERENCE_PORT || port == LEGACY_OPENCODE_REFERENCE_PORT) {
        logs.push(log(
            "error",
            "Only external reference ports 8787 and 8788 are recognized for read-only health checks",
        ));
        return ActionResult {
            ok: false,
            summary: "Unsupported external reference port".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    match action.as_str() {
        "health" => {
            let endpoint = check_headroom_endpoint(port, headroom_label(port));
            logs.push(log(
                "info",
                "No external process was started, stopped, restarted, or reconfigured",
            ));
            logs.push(log(
                if endpoint.healthy { "info" } else { "warn" },
                endpoint.detail,
            ));
            ActionResult {
                ok: endpoint.healthy,
                summary: format!("External Headroom reference health check complete for {port}"),
                logs,
                tools: all_checks(),
            }
        }
        "start" | "stop" | "restart" => {
            logs.push(log(
                "error",
                "Blocked by PRD: external Headroom is read-only reference only; d-token-owned runtime must own lifecycle",
            ));
            logs.push(log(
                "info",
                format!("Use d-token local service on 127.0.0.1:{DTOKEN_SERVICE_PORT} for managed runtime work"),
            ));
            ActionResult {
                ok: false,
                summary: "External Headroom lifecycle actions are disabled".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        _ => {
            logs.push(log(
                "error",
                format!("Unknown external reference action: {action}"),
            ));
            ActionResult {
                ok: false,
                summary: format!("Unknown external reference action: {action}"),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
fn test_opencode(model: String) -> ActionResult {
    let plan = opencode_command_plan(&model);
    let mut logs = vec![
        log("info", format!("Testing OpenCode model: {model}")),
        log(
            "info",
            format!("Command: {} {}", plan.program, plan.args.join(" ")),
        ),
        log(
            "info",
            format!(
                "scope={} · excluded={}",
                plan.envs
                    .get("DTOKEN_TEST_SCOPE")
                    .map(String::as_str)
                    .unwrap_or("opencode-only"),
                plan.envs
                    .get("DTOKEN_TEST_EXCLUDE")
                    .map(String::as_str)
                    .unwrap_or("")
            ),
        ),
        log(
            "info",
            "direct-model-connectivity=true · routed_dtoken_18001=false · not final route evidence",
        ),
    ];
    match run_opencode_model_test(&model) {
        Ok(outcome) => {
            logs.push(log(
                "info",
                format!("run_id={} category={}", outcome.run_id, outcome.category),
            ));
            logs.push(log(
                "info",
                "No Claude Code, Cursor, Codex CLI, OpenCode config, or system-wide proxy configuration was changed",
            ));
            if outcome.output_summary.is_empty() {
                logs.push(log("warn", "OpenCode returned no visible output"));
            } else {
                logs.push(log(
                    if outcome.ok { "info" } else { "warn" },
                    format!("Output summary: {}", outcome.output_summary),
                ));
            }
            let summary = if outcome.ok {
                format!("OpenCode model {model} responded with d-token-ok")
            } else {
                format!("OpenCode model {model} result: {}", outcome.category)
            };
            ActionResult {
                ok: outcome.ok,
                summary,
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            let summary = sanitize_command_output(&err);
            logs.push(log(
                "error",
                format!("OpenCode command could not start: {summary}"),
            ));
            ActionResult {
                ok: false,
                summary: "OpenCode command could not start".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
fn test_opencode_models(models: Vec<String>) -> Value {
    let mut results = Vec::new();
    for model in models {
        let action = test_opencode(model.clone());
        results.push(json!({
            "model": model,
            "ok": action.ok,
            "summary": action.summary,
            "logs": action.logs,
        }));
    }
    json!({
        "mode": "opencode-test",
        "route": "direct-model-connectivity",
        "routedDtoken18001": false,
        "fallback": "disabled",
        "results": results,
    })
}

#[cfg(debug_assertions)]
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(debug_assertions)]
#[tauri::command]
fn run_opencode_workload_benchmark() -> Result<Value, String> {
    let root = workspace_root();
    let script = root
        .join("support")
        .join("scripts")
        .join("run_opencode_workload_benchmark.ps1");
    if !script.exists() {
        return Err(format!(
            "OpenCode workload script not found: {}",
            script.display()
        ));
    }

    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script)
        .current_dir(&root)
        .output()
        .map_err(|err| format!("failed to launch OpenCode workload benchmark: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = sanitize_command_output(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Ok(json!({
            "ok": false,
            "summary": "OpenCode workload benchmark failed",
            "exit_code": output.status.code(),
            "stderr": stderr,
            "stdout": sanitize_command_output(&stdout),
            "tools": all_checks(),
        }));
    }

    let mut parsed: Value = serde_json::from_str(&stdout)
        .map_err(|err| format!("OpenCode workload benchmark returned invalid JSON: {err}"))?;
    if let Some(object) = parsed.as_object_mut() {
        object.insert("ok".to_string(), json!(true));
        object.insert(
            "summary".to_string(),
            json!(format!(
                "OpenCode real workload routed: {} workloads, {} saved tokens, {}% savings",
                object
                    .get("workload_count")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                object
                    .get("total_saved_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                object
                    .get("savings_percent")
                    .and_then(Value::as_f64)
                    .unwrap_or_default()
            )),
        );
        object.insert("tools".to_string(), json!(all_checks()));
        object.insert(
            "privacy".to_string(),
            json!("Uses user's existing OpenCode config; secrets are not read, printed, persisted, or committed."),
        );
    }
    Ok(parsed)
}

#[cfg(debug_assertions)]
#[tauri::command]
fn run_headroom_capability_smoke() -> Value {
    let install = run_install_transaction(InstallPlanRequest {
        target: "dtoken".to_string(),
        install_root: "%LOCALAPPDATA%\\d-token".to_string(),
        proxy_port: DTOKEN_SERVICE_PORT,
        auto_backup: true,
        run_doctor_after: true,
        rollback_point: true,
        mode: "connect".to_string(),
    });
    let detect = detect_tools();
    let doctor = run_doctor();
    let policy = recommend_project_policy(ProjectPolicyRecommendRequest {
        project_id: Some("desktop-auto-scan".to_string()),
        language_stack: Some(vec!["rust".to_string(), "typescript".to_string()]),
        avg_request_tokens_7d: Some(49_224),
        avg_cli_noise_ratio_7d: Some(0.42),
        retrieve_rate_7d: Some(0.02),
        failure_rate_7d: Some(0.0),
        high_risk_task: Some(false),
        manual_override: Some(false),
    });
    let settings = save_app_settings(
        "project-policy".to_string(),
        json!({
            "source": "headroom-capability-smoke",
            "project_id": "desktop-auto-scan",
            "recommended_level": policy["recommendation"]["recommended_level"].clone(),
            "enable_ccr": true,
            "enable_memory": true,
            "enable_learn": true,
            "enable_output_shaper": true,
            "privacy": "local d-token setting only; no raw prompt/output"
        }),
    );
    let dashboard = match service_json(DTOKEN_SERVICE_PORT, "/dashboard/payload") {
        Ok(payload) => json!({
            "ok": true,
            "source": payload.get("source").cloned().unwrap_or(Value::Null),
            "runtime": payload.get("runtime").cloned().unwrap_or(Value::Null),
            "stats": payload.get("stats").cloned().unwrap_or(Value::Null),
            "metrics": payload.get("metrics").cloned().unwrap_or(Value::Null),
            "sections": payload.get("dashboard_sections").cloned().unwrap_or(Value::Null),
        }),
        Err(err) => json!({
            "ok": false,
            "error": sanitize_command_output(&err),
            "source": "dtoken-local-service-unavailable"
        }),
    };
    let export = match service_json(DTOKEN_SERVICE_PORT, "/export/json") {
        Ok(payload) => json!({
            "ok": true,
            "redacted": payload.get("redacted").and_then(Value::as_bool).unwrap_or(false),
            "raw_available": payload.get("raw_available").and_then(Value::as_bool).unwrap_or(true),
            "active_config_source": payload.pointer("/active_config/config/source").cloned().unwrap_or(Value::Null),
        }),
        Err(err) => json!({
            "ok": false,
            "error": sanitize_command_output(&err),
        }),
    };
    let hook_effective = active_route_config_status();
    let checks = vec![
        json!({
            "id": "install_transaction",
            "label": "d-token install transaction",
            "ok": install.ok,
            "summary": install.summary,
        }),
        json!({
            "id": "tool_and_hook_scan",
            "label": "OpenCode / RTK / Headroom source scan",
            "ok": detect.ok,
            "summary": detect.summary,
            "tools": detect.tools,
        }),
        json!({
            "id": "doctor",
            "label": "Desktop Doctor",
            "ok": doctor.ok,
            "summary": doctor.summary,
        }),
        json!({
            "id": "auto_policy",
            "label": "Auto scan compression level",
            "ok": policy["recommendation"]["recommended_level"].is_string(),
            "summary": policy["recommendation"]["recommended_level"].clone(),
            "detail": policy,
        }),
        json!({
            "id": "config_write",
            "label": "Policy settings write",
            "ok": settings.ok,
            "summary": settings.summary,
        }),
        json!({
            "id": "runtime_data",
            "label": "d-token runtime dashboard payload",
            "ok": dashboard["ok"].as_bool().unwrap_or(false),
            "summary": dashboard["source"].clone(),
            "detail": dashboard,
        }),
        json!({
            "id": "export_privacy",
            "label": "Redacted runtime export",
            "ok": export["ok"].as_bool().unwrap_or(false)
                && export["redacted"].as_bool().unwrap_or(false)
                && !export["raw_available"].as_bool().unwrap_or(true),
            "summary": export.clone(),
        }),
        json!({
            "id": "hook_effective",
            "label": "d-token active config / hook effectiveness",
            "ok": hook_effective.pointer("/config/loaded").and_then(Value::as_bool).unwrap_or(false),
            "summary": hook_effective.pointer("/config/source").cloned().unwrap_or(Value::Null),
            "detail": hook_effective,
        }),
    ];
    let passed = checks
        .iter()
        .filter(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    json!({
        "ok": passed == checks.len(),
        "passed": passed,
        "total": checks.len(),
        "summary": format!("Headroom-derived capability smoke: {passed}/{} checks passed", checks.len()),
        "checks": checks,
        "logs": install.logs,
        "privacy": "No external Headroom/RTK/cc-switch service is started, stopped, reconfigured, or used as product runtime."
    })
}

#[tauri::command]
fn run_product_readiness_check() -> Value {
    let install = preview_install_plan(InstallPlanRequest {
        target: "dtoken".to_string(),
        install_root: app_state_dir().display().to_string(),
        proxy_port: DTOKEN_SERVICE_PORT,
        auto_backup: false,
        run_doctor_after: false,
        rollback_point: false,
        mode: "manual".to_string(),
    });
    let detect = detect_tools();
    let doctor = run_doctor();
    let route = verify_agent_route_config();
    let local_state = match (route_profile_drafts_result(), rollback_points_result()) {
        (Ok(drafts), Ok(points)) => json!({
            "ok": true,
            "source": "dtoken-desktop-control-plane",
            "route_draft_count": drafts.len(),
            "rollback_point_count": points.len(),
            "runtime_required": false,
        }),
        (drafts, points) => json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "route_state_error": drafts.err(),
            "rollback_state_error": points.err(),
            "runtime_required": false,
        }),
    };
    let active = active_route_config_status();
    let dashboard = runtime_probe("/dashboard/payload");
    let export = runtime_probe("/export/json");
    let capability = headroom_derived_runtime_capability_status(&dashboard);

    let compression_ok = dashboard
        .pointer("/payload/source")
        .and_then(Value::as_str)
        .map(|source| source == "dtoken-owned-runtime")
        .unwrap_or(false);
    let export_redacted = export
        .pointer("/payload/redacted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !export
            .pointer("/payload/raw_available")
            .and_then(Value::as_bool)
            .unwrap_or(true);
    let route_ok = route.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let capability_ok = capability
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let active_ok = active.get("state").and_then(Value::as_str) == Some("loaded");
    let detected_tools = detect.tools.iter().filter(|tool| tool.installed).count();
    let doctor_ran = !doctor.logs.is_empty();
    let install_ok = install
        .get("targets")
        .and_then(Value::as_array)
        .is_some_and(|targets| !targets.is_empty())
        && install.get("writesEnabled").and_then(Value::as_bool) == Some(false);

    let checks = vec![
        json!({
            "id": "install_connect",
            "label": "Install / connect plan",
            "ok": install_ok,
            "summary": if install_ok { "Read-only install/connect plan is available" } else { "Install/connect plan is invalid" },
            "detail": install,
        }),
        json!({
            "id": "agent_detection",
            "label": "Agent and tool discovery",
            "ok": !detect.tools.is_empty(),
            "summary": format!("{detected_tools}/{} known local tools available", detect.tools.len()),
            "detail": { "tools": detect.tools },
        }),
        json!({
            "id": "route_config",
            "label": "Agent route configuration",
            "ok": route_ok,
            "summary": route.get("summary").cloned().unwrap_or(Value::Null),
            "detail": route,
        }),
        json!({
            "id": "compression_data",
            "label": "Compression and dashboard data",
            "ok": compression_ok,
            "summary": dashboard.pointer("/payload/source").cloned().unwrap_or(Value::Null),
            "detail": dashboard,
        }),
        json!({
            "id": "hook_or_config_effect",
            "label": "Hook / config effect",
            "ok": active_ok,
            "summary": active.get("state").cloned().unwrap_or(Value::Null),
            "detail": active,
        }),
        json!({
            "id": "local_transaction_state",
            "label": "Local transaction state",
            "ok": local_state.get("ok").and_then(Value::as_bool).unwrap_or(false),
            "summary": if local_state.get("ok").and_then(Value::as_bool).unwrap_or(false) { "Desktop-owned drafts and rollback state are readable" } else { "Desktop-owned transaction state is unreadable" },
            "detail": local_state,
        }),
        json!({
            "id": "headroom_derived_capabilities",
            "label": "Headroom-derived capabilities",
            "ok": capability_ok,
            "summary": capability.get("summary").cloned().unwrap_or(Value::Null),
            "detail": capability,
        }),
        json!({
            "id": "export_redaction",
            "label": "Logs and export redaction",
            "ok": export_redacted,
            "summary": export.pointer("/payload/privacy").cloned().unwrap_or_else(|| json!("redacted runtime export")),
            "detail": export,
        }),
        json!({
            "id": "doctor",
            "label": "Doctor diagnostics",
            "ok": doctor_ran,
            "summary": doctor.summary,
            "detail": { "logs": doctor.logs, "tools": doctor.tools },
        }),
    ];
    let passed = checks
        .iter()
        .filter(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();

    json!({
        "ok": passed == checks.len(),
        "mode": "product-readiness-check",
        "passed": passed,
        "total": checks.len(),
        "summary": format!("Product readiness check: {passed}/{} checks passed", checks.len()),
        "checks": checks,
        "privacy": "Product action only: install/connect/verify/diagnose/status/log/export evidence. No development validation surface and no raw prompt/response/secrets are returned.",
        "external_services": "Does not start, stop, reconfigure, or depend on external Headroom, RTK, or cc-switch services.",
    })
}

fn headroom_derived_runtime_capability_status(dashboard: &Value) -> Value {
    let payload = dashboard.get("payload").unwrap_or(&Value::Null);
    let source_ok = payload
        .get("source")
        .and_then(Value::as_str)
        .map(|source| source == "dtoken-owned-runtime")
        .unwrap_or(false);
    let compression = payload.get("stats").is_some() || payload.get("metrics").is_some();
    let ccr_raw_blocked = payload
        .pointer("/ccr/raw_available")
        .and_then(Value::as_bool)
        .map(|raw| !raw)
        .unwrap_or(false);
    let memory_raw_blocked = payload
        .pointer("/memory/raw_available")
        .and_then(Value::as_bool)
        .map(|raw| !raw)
        .unwrap_or(false);
    let output_shape_raw_blocked = payload
        .pointer("/output_shape/raw_available")
        .and_then(Value::as_bool)
        .map(|raw| !raw)
        .unwrap_or(false);
    let route_adapter = payload.get("route").is_some()
        || payload.get("route_decisions").is_some()
        || payload.pointer("/stats/route_overhead").is_some();

    let checks = vec![
        json!({
            "id": "runtime_source",
            "label": "d-token owned runtime",
            "ok": source_ok,
        }),
        json!({
            "id": "compression_pipeline",
            "label": "Compression / metrics",
            "ok": source_ok && compression,
        }),
        json!({
            "id": "ccr_metadata",
            "label": "CCR metadata boundary",
            "ok": source_ok && ccr_raw_blocked,
        }),
        json!({
            "id": "memory_learn",
            "label": "Memory / learn boundary",
            "ok": source_ok && memory_raw_blocked,
        }),
        json!({
            "id": "output_shaping",
            "label": "Output shaping boundary",
            "ok": source_ok && output_shape_raw_blocked,
        }),
        json!({
            "id": "route_adapter",
            "label": "Route adapter visibility",
            "ok": source_ok && route_adapter,
        }),
    ];
    let passed = checks
        .iter()
        .filter(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();

    json!({
        "ok": passed == checks.len(),
        "passed": passed,
        "total": checks.len(),
        "summary": format!("Headroom-derived d-token runtime capabilities: {passed}/{} checks passed", checks.len()),
        "checks": checks,
        "privacy": "Uses d-token runtime metadata only; no external Headroom service, raw prompt, raw response, or secrets are required.",
    })
}

#[tauri::command]
fn get_active_route_config_status() -> Value {
    active_route_config_status()
}

fn runtime_probe(path: &str) -> Value {
    match service_json(DTOKEN_SERVICE_PORT, path) {
        Ok(payload) => json!({
            "ok": true,
            "path": path,
            "payload": payload,
        }),
        Err(err) => json!({
            "ok": false,
            "path": path,
            "error": sanitize_command_output(&err),
        }),
    }
}

#[tauri::command]
fn verify_agent_route_config() -> Value {
    let active = active_route_config_status();
    let active_loaded = active.get("state").and_then(Value::as_str) == Some("loaded");
    let fallback_disabled = active
        .get("fallback_disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let active_route = active.pointer("/route/id").cloned().unwrap_or(Value::Null);
    let active_provider = active
        .pointer("/provider/id")
        .cloned()
        .unwrap_or(Value::Null);

    let health = runtime_probe("/health");
    let health_ok = health
        .pointer("/payload/status")
        .and_then(Value::as_str)
        .map(|status| status == "healthy" || status == "ready")
        .unwrap_or_else(|| health.get("ok").and_then(Value::as_bool).unwrap_or(false));

    let runtime_active = runtime_probe("/config/active");
    let runtime_loaded = runtime_active
        .pointer("/payload/config/loaded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let runtime_route_count = runtime_active
        .pointer("/payload/config/route_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let dashboard = runtime_probe("/dashboard/payload");
    let dashboard_source = dashboard
        .pointer("/payload/source")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let dashboard_ok = dashboard_source == "dtoken-owned-runtime";
    let request_count = dashboard
        .pointer("/payload/stats/requests/total")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let route_decisions = dashboard
        .pointer("/payload/route_decisions")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    let export = runtime_probe("/export/json");
    let export_ok = export.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let export_redacted = export
        .pointer("/payload/redacted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let export_raw_available = export
        .pointer("/payload/raw_available")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let checks = vec![
        json!({
            "id": "active_config",
            "label": "Active d-token config",
            "ok": active_loaded,
            "summary": if active_loaded { "d-token active route config is loaded" } else { "No applied d-token active route config yet" },
            "detail": active,
        }),
        json!({
            "id": "fallback",
            "label": "Fallback boundary",
            "ok": fallback_disabled,
            "summary": if fallback_disabled { "Silent fallback is disabled" } else { "Fallback has not been proven disabled" },
            "detail": { "route": active_route, "provider": active_provider },
        }),
        json!({
            "id": "runtime_health",
            "label": "d-token local service",
            "ok": health_ok,
            "summary": if health_ok { "127.0.0.1:18001 responded healthy" } else { "d-token local service is not responding as healthy" },
            "detail": health,
        }),
        json!({
            "id": "runtime_active_config",
            "label": "Runtime route loader",
            "ok": runtime_loaded && runtime_route_count > 0,
            "summary": if runtime_loaded && runtime_route_count > 0 { "Runtime loaded the active route config" } else { "Runtime has not loaded the active route config yet" },
            "detail": runtime_active,
        }),
        json!({
            "id": "dashboard",
            "label": "Dashboard data source",
            "ok": dashboard_ok,
            "summary": format!("{dashboard_source} · {request_count} requests · {route_decisions} route decisions"),
            "detail": dashboard,
        }),
        json!({
            "id": "export_redaction",
            "label": "Redacted export",
            "ok": export_ok && export_redacted && !export_raw_available,
            "summary": if export_ok && export_redacted && !export_raw_available { "Runtime export is redacted" } else { "Runtime export redaction is not proven yet" },
            "detail": export,
        }),
    ];
    let passed = checks
        .iter()
        .filter(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let ok = passed == checks.len();
    json!({
        "ok": ok,
        "mode": "agent-route-verification",
        "passed": passed,
        "total": checks.len(),
        "summary": if ok {
            "Agent route configuration is active and observable through d-token runtime"
        } else {
            "Agent route configuration still needs attention"
        },
        "checks": checks,
        "route": active_route,
        "provider": active_provider,
        "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
        "privacy": "Read-only d-token local verification; no external Headroom/RTK/cc-switch service is touched and no secrets are read."
    })
}

#[tauri::command]
async fn get_dtoken_dashboard_payload() -> Value {
    match service_json(DTOKEN_SERVICE_PORT, "/dashboard/payload") {
        Ok(mut payload) => {
            if let Some(object) = payload.as_object_mut() {
                object.insert(
                    "desktop_data_source".to_string(),
                    json!("dtoken-local-service"),
                );
                object.insert(
                    "desktop_service_url".to_string(),
                    json!(format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}")),
                );
                object.insert(
                    "desktop_active_config".to_string(),
                    active_route_config_status(),
                );
                if let Ok(active) = service_json(DTOKEN_SERVICE_PORT, "/config/active") {
                    object.insert("runtime_active_config".to_string(), active);
                }
            }
            payload
        }
        Err(err) => {
            json!({
                "source": "unavailable",
                "desktop_data_source": "unavailable",
                "evidence_state": "unavailable",
                "freshness": "unavailable",
                "complete": false,
                "partial": true,
                "desktop_service_error": err,
                "desktop_service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
                "desktop_active_config": active_route_config_status(),
                "providers": [],
                "routes": [],
                "instances": [],
                "request_logs": [],
                "logs": [],
                "recentEvents": []
            })
        }
    }
}

#[tauri::command]
async fn export_dtoken_runtime(format: String) -> Result<Value, String> {
    let normalized = format.trim().to_ascii_lowercase();
    let exported_at = now_label();
    match normalized.as_str() {
        "json" => {
            let payload = service_json(DTOKEN_SERVICE_PORT, "/export/json")?;
            let content = serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())?;
            Ok(json!({
                "format": "json",
                "mime": "application/json",
                "filename": format!("dtoken-runtime-redacted-{exported_at}.json").replace(':', "-"),
                "content": content,
                "source": "dtoken-local-service",
                "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}/export/json"),
                "redacted": payload.get("redacted").and_then(Value::as_bool).unwrap_or(true)
            }))
        }
        "csv" => {
            let content = service_text(DTOKEN_SERVICE_PORT, "/export/csv")?;
            Ok(json!({
                "format": "csv",
                "mime": "text/csv",
                "filename": format!("dtoken-runtime-redacted-{exported_at}.csv").replace(':', "-"),
                "content": content,
                "source": "dtoken-local-service",
                "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}/export/csv"),
                "redacted": true
            }))
        }
        _ => Err("Unsupported export format; use json or csv".to_string()),
    }
}

fn offline_control_plane_data(
    reason: &str,
    route_drafts: Vec<Value>,
    backup_points: Vec<Value>,
) -> Value {
    let changes = changes_from_route_drafts(&route_drafts);
    json!({
        "source": "dtoken-desktop-control-plane",
        "desktop_data_source": "dtoken-desktop-control-plane",
        "evidence_state": "partial",
        "freshness": "local",
        "complete": false,
        "partial": true,
        "runtime_available": false,
        "runtime_evidence_state": "unavailable",
        "error": reason,
        "transaction_provenance": {
            "source": "dtoken-desktop-control-plane",
            "command": "get_desktop_data",
            "schema_version": 1,
            "generated_at": now_timestamp(),
            "state_root": app_state_dir().display().to_string(),
            "runtime_required": false
        },
        "active_config": active_route_config_status(),
        "service": dtoken_service_status_value(),
        "tools": all_checks(),
        "health": [],
        "doctorChecks": [],
        "agents": [],
        "projects": [],
        "savings": [],
        "rtkTrend": [],
        "headroomTrend": [],
        "commandFamilies": [],
        "recentDecisions": [],
        "recentEvents": [],
        "routes": [],
        "providers": [],
        "instances": [],
        "request_logs": [],
        "logs": [],
        "changes": changes,
        "routeDrafts": route_drafts,
        "backupPoints": backup_points,
        "runtimeCompatibility": {
            "source": "unavailable",
            "agents": [],
            "protocolInjection": [],
            "chainStatus": null
        }
    })
}

fn changes_from_route_drafts(route_drafts: &[Value]) -> Vec<Value> {
    route_drafts
        .iter()
        .filter_map(|draft| {
            let route = draft
                .get("config")?
                .get("routes")?
                .as_array()?
                .first()?;
            let route_id = route.get("id")?.as_str()?;
            Some(json!({
                "id": format!("draft-{route_id}"),
                "summary": format!("Apply d-token route draft {route_id}"),
                "target": draft.get("path").cloned().unwrap_or(Value::Null),
                "risk": "medium",
                "backup": "created durably before active config replacement",
                "diff": draft.get("diff").cloned().unwrap_or_else(|| json!([])),
                "postChecks": ["d-token active config", "local service health", "agent route verification"]
            }))
        })
        .collect()
}

fn complete_desktop_data(
    mut runtime: Value,
    route_drafts: Vec<Value>,
    backup_points: Vec<Value>,
    compatibility: Option<Value>,
    protocol_injection: Option<Value>,
    chain_status: Option<Value>,
) -> Value {
    let changes = changes_from_route_drafts(&route_drafts);
    let object = runtime.as_object_mut().expect("validated runtime object");
    object.insert("source".to_string(), json!("dtoken-owned-runtime"));
    object.insert(
        "desktop_data_source".to_string(),
        json!("dtoken-local-service"),
    );
    object.insert("evidence_state".to_string(), json!("verified"));
    object.insert("freshness".to_string(), json!("fresh"));
    object.insert("complete".to_string(), json!(true));
    object.insert("partial".to_string(), json!(false));
    object.insert("runtime_available".to_string(), json!(true));
    object.insert("runtime_evidence_state".to_string(), json!("verified"));
    object.insert(
        "transaction_provenance".to_string(),
        json!({
            "source": "dtoken-owned-runtime",
            "command": "get_desktop_data",
            "schema_version": 1,
            "generated_at": now_timestamp(),
            "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
            "state_root": app_state_dir().display().to_string()
        }),
    );
    object.insert("routeDrafts".to_string(), json!(route_drafts));
    object.insert("backupPoints".to_string(), json!(backup_points));
    object.insert("changes".to_string(), json!(changes));
    object.insert(
        "runtimeCompatibility".to_string(),
        json!({
            "source": if compatibility.is_some() || protocol_injection.is_some() || chain_status.is_some() {
                "dtoken-local-service"
            } else {
                "unavailable"
            },
            "agents": compatibility.unwrap_or_else(|| json!([])),
            "protocolInjection": protocol_injection.unwrap_or_else(|| json!([])),
            "chainStatus": chain_status.unwrap_or(Value::Null),
            "privacy": "Compatibility evidence is read only from the d-token-owned local service."
        }),
    );
    runtime
}

#[tauri::command]
fn get_desktop_data() -> Value {
    let route_drafts = match route_profile_drafts_result() {
        Ok(drafts) => drafts,
        Err(err) => {
            return offline_control_plane_data(
                &format!("Routing state is unreadable: {err}"),
                vec![],
                vec![],
            )
        }
    };
    let backup_points = match rollback_points_result() {
        Ok(points) => points,
        Err(err) => {
            return offline_control_plane_data(
                &format!("Rollback state is unreadable: {err}"),
                route_drafts,
                vec![],
            )
        }
    };
    let health = match service_health_payload() {
        Some(health)
            if health.get("component").and_then(Value::as_str) == Some("dtoken-core-api")
                && matches!(
                    health.get("status").and_then(Value::as_str),
                    Some("healthy" | "ready")
                ) =>
        {
            health
        }
        Some(_) => {
            return offline_control_plane_data(
                "The local listener did not prove dtoken-core-api health",
                route_drafts,
                backup_points,
            )
        }
        None => {
            return offline_control_plane_data(
                "d-token local service is unavailable",
                route_drafts,
                backup_points,
            )
        }
    };
    let runtime = match service_json(DTOKEN_SERVICE_PORT, "/dashboard/payload") {
        Ok(payload)
            if payload.is_object()
                && payload.get("source").and_then(Value::as_str)
                    == Some("dtoken-owned-runtime") =>
        {
            payload
        }
        Ok(_) => {
            return offline_control_plane_data(
                "Dashboard payload did not prove d-token runtime provenance",
                route_drafts,
                backup_points,
            )
        }
        Err(err) => {
            return offline_control_plane_data(
                &format!("Dashboard payload is unavailable: {err}"),
                route_drafts,
                backup_points,
            )
        }
    };
    let mut payload = complete_desktop_data(
        runtime,
        route_drafts,
        backup_points,
        service_json(DTOKEN_SERVICE_PORT, "/compatibility/agents").ok(),
        service_json(DTOKEN_SERVICE_PORT, "/protocol-injection").ok(),
        service_json(DTOKEN_SERVICE_PORT, "/chain/status").ok(),
    );
    if let Some(object) = payload.as_object_mut() {
        object.insert("runtime_health".to_string(), health);
        object.insert("tools".to_string(), json!(all_checks()));
        object.insert("active_config".to_string(), active_route_config_status());
    }
    payload
}

#[cfg(debug_assertions)]
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_desktop_data,
            get_app_settings,
            save_app_settings,
            save_route_profile_draft,
            store_provider_credential,
            delete_provider_credential,
            list_route_profile_drafts,
            preview_route_profile_draft_apply,
            apply_route_profile_draft,
            get_active_route_config_status,
            verify_agent_route_config,
            recommend_project_policy,
            manage_backup,
            detect_tools,
            install_tool,
            preview_install_plan,
            get_dtoken_service_status,
            start_dtoken_service,
            stop_dtoken_service,
            run_install_transaction,
            run_doctor,
            manage_headroom,
            test_opencode,
            test_opencode_models,
            run_opencode_workload_benchmark,
            run_headroom_capability_smoke,
            run_product_readiness_check,
            get_dtoken_dashboard_payload,
            export_dtoken_runtime
        ])
        .run(tauri::generate_context!())
        .expect("error while running d-token desktop");
}

#[cfg(not(debug_assertions))]
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_desktop_data,
            get_app_settings,
            save_app_settings,
            save_route_profile_draft,
            store_provider_credential,
            delete_provider_credential,
            list_route_profile_drafts,
            preview_route_profile_draft_apply,
            apply_route_profile_draft,
            get_active_route_config_status,
            verify_agent_route_config,
            recommend_project_policy,
            manage_backup,
            detect_tools,
            install_tool,
            preview_install_plan,
            get_dtoken_service_status,
            start_dtoken_service,
            stop_dtoken_service,
            run_install_transaction,
            run_doctor,
            manage_headroom,
            run_product_readiness_check,
            get_dtoken_dashboard_payload,
            export_dtoken_runtime
        ])
        .run(tauri::generate_context!())
        .expect("error while running d-token desktop");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    };
    use std::thread;

    fn test_state_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_isolated_app_state<T>(name: &str, run: impl FnOnce() -> T) -> T {
        let _guard = test_state_lock()
            .lock()
            .expect("test app state lock poisoned");
        let previous = env::var("DTOKEN_TEST_STATE_DIR").ok();
        let root = env::temp_dir().join(format!(
            "dtoken-desktop-test-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&root).expect("create isolated app state");
        env::set_var("DTOKEN_TEST_STATE_DIR", &root);
        let result = run();
        match previous {
            Some(value) => env::set_var("DTOKEN_TEST_STATE_DIR", value),
            None => env::remove_var("DTOKEN_TEST_STATE_DIR"),
        }
        let _ = fs::remove_dir_all(root);
        result
    }

    fn review_and_apply_route(route_id: &str) -> ActionResult {
        let review = preview_route_profile_draft_apply(RouteDraftReviewRequest {
            route_id: route_id.to_string(),
        });
        assert_eq!(review["ok"], true, "route review failed: {review}");
        apply_route_profile_draft(ApplyRouteDraftRequest {
            route_id: route_id.to_string(),
            reviewed_before_sha256: review["before_sha256"].as_str().map(str::to_string),
            reviewed_after_sha256: review["after_sha256"].as_str().map(str::to_string),
        })
    }

    fn save_test_route(route_id: &str, base_url: &str) -> ActionResult {
        save_route_profile_draft(RouteProfileDraftRequest {
            route_id: route_id.to_string(),
            route_name: format!("{route_id} route"),
            provider_id: format!("{route_id}-provider"),
            provider_name: format!("{route_id} provider"),
            provider_kind: "custom".to_string(),
            protocol: "openai".to_string(),
            base_url: base_url.to_string(),
            api_key_ref: None,
            models: vec!["model/test".to_string()],
            chain_mode: "direct".to_string(),
            compression_policy: "balanced".to_string(),
            local_port: DTOKEN_SERVICE_PORT,
            agent: "OpenCode".to_string(),
        })
    }

    #[test]
    fn app_state_uses_only_the_explicit_test_override_for_temporary_storage() {
        with_isolated_app_state("state-location", || {
            let root = PathBuf::from(env::var("DTOKEN_TEST_STATE_DIR").expect("test override"));
            assert_eq!(app_state_dir(), root);
            assert!(!app_state_dir().starts_with(env::temp_dir().join("d-token")));
        });
    }

    #[test]
    fn durable_json_write_replaces_content_without_leaving_a_partial_file() {
        with_isolated_app_state("atomic-json", || {
            write_json_app_state(&["settings", "app.json"], &json!({"version": 1}))
                .expect("write first version");
            write_json_app_state(&["settings", "app.json"], &json!({"version": 2}))
                .expect("replace second version");

            let saved = read_app_state(&["settings", "app.json"]).expect("read saved JSON");
            assert_eq!(saved["version"], 2);
            let entries = fs::read_dir(app_state_dir().join("settings"))
                .expect("read settings directory")
                .flatten()
                .collect::<Vec<_>>();
            assert_eq!(
                entries.len(),
                1,
                "temporary transaction files must be removed"
            );
        });
    }

    #[test]
    fn missing_active_config_is_empty_and_never_materializes_a_placeholder_route() {
        with_isolated_app_state("empty-active-config", || {
            assert!(!active_config_path().exists());
            let result = validate_active_config_for_start();
            assert!(result.is_err());
            assert!(!active_config_path().exists());
            assert_eq!(active_route_config_status()["provider_count"], 0);
            assert_eq!(active_route_config_status()["route_count"], 0);
        });
    }

    #[test]
    fn offline_desktop_data_preserves_local_control_plane_without_runtime_claims() {
        with_isolated_app_state("desktop-unavailable", || {
            let drafts = vec![json!({
                "path": "/local/draft.json",
                "config": { "routes": [{ "id": "offline-route" }] },
                "diff": [{ "type": "add", "text": "+ route" }]
            })];
            let points = vec![json!({ "id": "offline-backup", "verified": true })];
            let payload = offline_control_plane_data("runtime unavailable", drafts, points);
            assert_eq!(payload["complete"], false);
            assert_eq!(payload["source"], "dtoken-desktop-control-plane");
            assert_eq!(payload["freshness"], "local");
            assert_eq!(payload["runtime_available"], false);
            assert_eq!(payload["transaction_provenance"]["runtime_required"], false);
            assert_eq!(payload["routeDrafts"].as_array().map(Vec::len), Some(1));
            assert_eq!(payload["backupPoints"].as_array().map(Vec::len), Some(1));
            assert_eq!(payload["changes"].as_array().map(Vec::len), Some(1));
            for field in [
                "health",
                "savings",
                "routes",
                "providers",
                "logs",
                "recentEvents",
            ] {
                assert!(
                    payload
                        .get(field)
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty),
                    "{field} must be explicitly empty"
                );
            }
        });
    }

    #[test]
    fn complete_desktop_data_has_the_required_transaction_provenance() {
        let runtime = json!({
            "source": "dtoken-owned-runtime",
            "providers": [],
            "routes": [],
            "instances": [],
            "request_logs": []
        });
        let payload = complete_desktop_data(runtime, vec![], vec![], None, None, None);
        assert_eq!(payload["source"], "dtoken-owned-runtime");
        assert_eq!(payload["desktop_data_source"], "dtoken-local-service");
        assert_eq!(payload["freshness"], "fresh");
        assert_eq!(payload["complete"], true);
        assert_eq!(
            payload["transaction_provenance"]["source"],
            "dtoken-owned-runtime"
        );
        assert_eq!(
            payload["transaction_provenance"]["command"],
            "get_desktop_data"
        );
        assert_eq!(payload["transaction_provenance"]["schema_version"], 1);
        assert!(payload["transaction_provenance"]["generated_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }

    #[test]
    fn listener_pid_parser_requires_the_manifest_pid() {
        assert!(listener_output_contains_pid("723\n", 723));
        assert!(listener_output_contains_pid(
            "TCP    127.0.0.1:18001    0.0.0.0:0    LISTENING    723\n",
            723
        ));
        assert!(!listener_output_contains_pid("1723\n", 723));
        assert!(!listener_output_contains_pid(
            "TCP    127.0.0.1:18001    0.0.0.0:0    LISTENING    999\n",
            723
        ));
    }

    #[test]
    fn http_body_extracts_json_payload() {
        let raw = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"source\":\"dtoken-owned-runtime\"}";
        assert_eq!(http_body(raw), "{\"source\":\"dtoken-owned-runtime\"}");
    }

    #[test]
    fn provider_target_cannot_point_back_to_the_same_dtoken_listener() {
        assert!(provider_targets_local_listener(
            "http://127.0.0.1:18001/v1",
            18001
        ));
        assert!(provider_targets_local_listener(
            "http://localhost:18001/v1",
            18001
        ));
        assert!(provider_targets_local_listener(
            "http://[::1]:18001/v1",
            18001
        ));
        assert!(!provider_targets_local_listener(
            "http://127.0.0.1:19001/v1",
            18001
        ));
        assert!(!provider_targets_local_listener(
            "https://api.example.com/v1",
            18001
        ));
    }

    #[test]
    fn provider_credential_reference_uses_a_stable_os_store_target() {
        assert_eq!(
            credential_reference_for_provider("OpenCode Zen"),
            (
                "os-credential://d-token/providers/opencode-zen".to_string(),
                "providers/opencode-zen".to_string()
            )
        );
    }

    #[test]
    fn project_policy_recommendation_uses_runtime_engine() {
        let result = recommend_project_policy(ProjectPolicyRecommendRequest {
            project_id: Some("desktop-project".to_string()),
            language_stack: Some(vec!["rust".to_string(), "typescript".to_string()]),
            avg_request_tokens_7d: Some(47_307),
            avg_cli_noise_ratio_7d: Some(0.42),
            retrieve_rate_7d: Some(0.02),
            failure_rate_7d: Some(0.0),
            high_risk_task: Some(false),
            manual_override: Some(false),
        });

        assert_eq!(result["source"], "dtoken-project-policy");
        assert_eq!(result["runtime"], "d-token-owned");
        assert_eq!(result["recommendation"]["project_id"], "desktop-project");
        assert!(result["recommendation"]["recommended_level"].is_string());
        assert_eq!(result["mapping"]["enable_output_shaper"], true);
    }

    #[test]
    fn install_transaction_for_dtoken_returns_auditable_logs() {
        with_isolated_app_state("manual-install-read-only", || {
            let result = run_install_transaction(InstallPlanRequest {
                target: "dtoken".to_string(),
                install_root: app_state_dir().display().to_string(),
                proxy_port: DTOKEN_SERVICE_PORT,
                auto_backup: true,
                run_doctor_after: false,
                rollback_point: true,
                mode: "manual".to_string(),
            });

            assert!(result.summary.contains("dtoken installation transaction"));
            assert!(result.logs.iter().any(|entry| entry
                .message
                .contains("scan: starting installation transaction")));
            assert!(result
                .logs
                .iter()
                .any(|entry| entry.message.contains("diff: generated config diff")));
            assert!(result
                .logs
                .iter()
                .any(|entry| entry.message.contains("verify: refreshed local tool scan")));
            assert!(result
                .logs
                .iter()
                .any(|entry| entry.message.contains("read-only mode does not persist")));
            assert!(!app_state_dir().join("backups").exists());
            assert!(!app_state_dir().join("rollback-points").exists());
        });
    }

    #[test]
    fn service_lifecycle_refuses_manifest_identity_mismatch() {
        with_isolated_app_state("service-identity-mismatch", || {
            let manifest = ManagedServiceManifest {
                managed_by: "another-app".to_string(),
                identity: "not-dtoken".to_string(),
                pid: 999_999,
                port: DTOKEN_SERVICE_PORT,
                exe_path: "unknown".to_string(),
                config_path: "unknown".to_string(),
                data_dir: "unknown".to_string(),
                started_at: now_label(),
            };
            write_service_manifest(&manifest).expect("write manifest");

            let result = stop_dtoken_service();
            assert!(!result.ok);
            assert!(result.summary.contains("identity"));
            assert!(service_manifest_path().exists());
        });
    }

    #[test]
    fn service_lifecycle_clears_stale_owned_manifest() {
        with_isolated_app_state("service-stale-manifest", || {
            let manifest = ManagedServiceManifest {
                managed_by: "d-token-desktop".to_string(),
                identity: "dtoken-core-api".to_string(),
                pid: 999_999,
                port: DTOKEN_SERVICE_PORT,
                exe_path: "unknown".to_string(),
                config_path: "unknown".to_string(),
                data_dir: "unknown".to_string(),
                started_at: now_label(),
            };
            write_service_manifest(&manifest).expect("write manifest");

            let result = stop_dtoken_service();
            assert!(result.ok);
            assert!(result.summary.contains("Stale"));
            assert!(!service_manifest_path().exists());
        });
    }

    #[test]
    fn service_lifecycle_resolves_explicit_proxy_exe() {
        with_isolated_app_state("service-resolve-proxy", || {
            let proxy = app_state_dir().join("fake-dtoken-proxy.exe");
            fs::create_dir_all(proxy.parent().expect("fake proxy parent"))
                .expect("create fake proxy parent");
            fs::write(&proxy, "placeholder").expect("write fake proxy");
            let previous = env::var("DTOKEN_PROXY_EXE").ok();
            env::set_var("DTOKEN_PROXY_EXE", &proxy);
            let resolved = resolve_dtoken_proxy_exe();
            match previous {
                Some(value) => env::set_var("DTOKEN_PROXY_EXE", value),
                None => env::remove_var("DTOKEN_PROXY_EXE"),
            }
            assert_eq!(
                resolved.expect("resolve proxy"),
                fs::canonicalize(proxy).unwrap()
            );
        });
    }

    #[test]
    fn service_lifecycle_start_stop_real_proxy_when_enabled() {
        if env::var("DTOKEN_LIFECYCLE_TEST").ok().as_deref() != Some("1") {
            return;
        }
        with_isolated_app_state("service-lifecycle-real", || {
            assert!(
                service_health_payload().is_none(),
                "127.0.0.1:{DTOKEN_SERVICE_PORT} is already occupied; lifecycle test refuses to touch unmanaged services"
            );
            write_json_app_state(
                &["active-config", "dtoken-config.json"],
                &json!({
                    "providers": [{
                        "id": "lifecycle-provider",
                        "name": "Lifecycle test provider",
                        "kind": "custom",
                        "protocol": "open_ai",
                        "base_url": "http://127.0.0.1:39199",
                        "api_key_ref": null,
                        "models": ["lifecycle/test"]
                    }],
                    "routes": [{
                        "id": "lifecycle-route",
                        "name": "Lifecycle test route",
                        "input_protocol": "open_ai",
                        "instance_id": "lifecycle-instance",
                        "target_provider_id": "lifecycle-provider",
                        "fallback_provider_id": null,
                        "chain_mode": "direct",
                        "compression_policy": "off",
                        "bind_host": "127.0.0.1",
                        "local_port": DTOKEN_SERVICE_PORT
                    }],
                    "instances": [{
                        "id": "lifecycle-instance",
                        "name": "Lifecycle test instance",
                        "agent": "OpenCode",
                        "route_id": "lifecycle-route",
                        "local_port": DTOKEN_SERVICE_PORT,
                        "process_status": "stopped"
                    }]
                }),
            )
            .expect("write explicit lifecycle config");

            let start = start_dtoken_service();
            assert!(
                start.ok,
                "start failed: {}",
                start
                    .logs
                    .iter()
                    .map(|entry| entry.message.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            );

            let status = dtoken_service_status_value();
            let status_ok = status["state"] == "running" && status["managed"] == true;
            let stop = stop_dtoken_service();
            assert!(
                stop.ok,
                "stop failed: {}",
                stop.logs
                    .iter()
                    .map(|entry| entry.message.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            );

            for _ in 0..30 {
                if service_health_payload().is_none() {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }

            assert!(
                status_ok,
                "unexpected lifecycle status after start: {status}"
            );
            assert_eq!(dtoken_service_status_value()["state"], "stopped");
        });
    }

    #[test]
    fn product_readiness_check_is_product_scoped_without_external_headroom_gate() {
        with_isolated_app_state("product-readiness", || {
            let result = run_product_readiness_check();
            assert_eq!(result["mode"], "product-readiness-check");
            assert_eq!(result["total"], 9);
            assert_eq!(result["external_services"], "Does not start, stop, reconfigure, or depend on external Headroom, RTK, or cc-switch services.");
            let checks = result["checks"].as_array().expect("readiness checks");
            assert!(checks.iter().any(|check| {
                check["id"] == "install_connect" && check["ok"].as_bool().unwrap_or(false)
            }));
            assert!(checks.iter().any(|check| {
                check["id"] == "agent_detection" && check["ok"].as_bool().unwrap_or(false)
            }));
            assert!(checks.iter().any(|check| {
                check["id"] == "doctor" && check["ok"].as_bool().unwrap_or(false)
            }));
            let text = result.to_string();
            assert!(!text.contains("headroom-capability-smoke"));
            assert!(!text.contains("workload UI"));
            assert!(!app_state_dir().join("settings").exists());
            assert!(!app_state_dir().join("backups").exists());
            assert!(!app_state_dir().join("rollback-points").exists());
            assert!(!app_state_dir().join("config-backups").exists());
        });
    }

    #[test]
    fn product_readiness_check_passes_with_dtoken_runtime_metadata() {
        with_isolated_app_state("product-readiness-runtime", || {
            let config = json!({
                "providers": [{
                    "id": "readiness-provider",
                    "name": "Readiness Provider",
                    "kind": "custom",
                    "protocol": "open_ai",
                    "base_url": "http://127.0.0.1:39199",
                    "api_key_ref": null,
                    "models": ["mock/model"]
                }],
                "routes": [{
                    "id": "readiness-route",
                    "name": "Readiness Route",
                    "input_protocol": "open_ai",
                    "instance_id": "readiness-instance",
                    "target_provider_id": "readiness-provider",
                    "fallback_provider_id": null,
                    "chain_mode": "direct",
                    "compression_policy": "balanced",
                    "bind_host": "127.0.0.1",
                    "local_port": DTOKEN_SERVICE_PORT
                }],
                "instances": [{
                    "id": "readiness-instance",
                    "name": "Readiness Instance",
                    "agent": "OpenCode",
                    "route_id": "readiness-route",
                    "local_port": DTOKEN_SERVICE_PORT,
                    "process_status": "running"
                }]
            });
            write_app_state(
                &["active-config", "dtoken-config.json"],
                &serde_json::to_string_pretty(&config).expect("serialize active config"),
            )
            .expect("write active config");

            let shutdown = start_fake_dtoken_runtime();
            let result = run_product_readiness_check();
            shutdown.store(true, Ordering::SeqCst);

            assert_eq!(result["mode"], "product-readiness-check");
            assert_eq!(result["ok"], true);
            assert_eq!(result["passed"], 9);
            assert_eq!(result["total"], 9);
            assert!(result["privacy"]
                .as_str()
                .unwrap_or_default()
                .contains("Product action only"));
            let checks = result["checks"].as_array().expect("readiness checks");
            for expected in [
                "install_connect",
                "agent_detection",
                "route_config",
                "compression_data",
                "hook_or_config_effect",
                "local_transaction_state",
                "headroom_derived_capabilities",
                "export_redaction",
                "doctor",
            ] {
                assert!(
                    checks.iter().any(|check| {
                        check["id"] == expected && check["ok"].as_bool().unwrap_or(false)
                    }),
                    "missing or failed readiness check: {expected}"
                );
            }
            assert!(!app_state_dir()
                .join("settings")
                .join("project-policy.json")
                .exists());
            assert!(!app_state_dir().join("backups").exists());
        });
    }

    fn start_fake_dtoken_runtime() -> Arc<AtomicBool> {
        let listener = TcpListener::bind(("127.0.0.1", DTOKEN_SERVICE_PORT))
            .expect("bind fake d-token runtime on product port");
        listener
            .set_nonblocking(true)
            .expect("set fake runtime nonblocking");
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        thread::spawn(move || {
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0_u8; 2048];
                        let read = stream.read(&mut buffer).unwrap_or_default();
                        let request = String::from_utf8_lossy(&buffer[..read]);
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("/");
                        let body = fake_runtime_body(path);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        shutdown
    }

    fn fake_runtime_body(path: &str) -> String {
        match path {
            "/health" => json!({
                "status": "healthy",
                "component": "dtoken-core-api",
                "version": "0.1.0"
            }),
            "/config/active" => json!({
                "config": {
                    "loaded": true,
                    "source": "desktop_active_config",
                    "fallback_disabled": true,
                    "route_count": 1,
                    "provider_count": 1,
                    "instance_count": 1
                }
            }),
            "/dashboard/payload" => json!({
                "source": "dtoken-owned-runtime",
                "desktop_data_source": "dtoken-local-service",
                "stats": {
                    "requests": { "total": 1 },
                    "route_overhead": { "tokens": 0, "percent": 0.0 }
                },
                "metrics": {
                    "saved_tokens": 12,
                    "latency_breakdown": {
                        "compression_ms": 1,
                        "route_adapter_ms": 1,
                        "provider_ms": 2
                    }
                },
                "route": { "id": "readiness-route" },
                "route_decisions": [{ "route": "readiness-route" }],
                "ccr": { "entries": 1, "raw_available": false },
                "memory": { "entries": 1, "raw_available": false },
                "output_shape": { "verbosity": "terse", "raw_available": false }
            }),
            "/export/json" => json!({
                "redacted": true,
                "raw_available": false,
                "privacy": "redacted runtime export"
            }),
            _ => json!({ "ok": true }),
        }
        .to_string()
    }

    #[test]
    fn headroom_derived_runtime_capability_status_uses_dtoken_dashboard_metadata() {
        let dashboard = json!({
            "ok": true,
            "payload": {
                "source": "dtoken-owned-runtime",
                "stats": { "requests": 2 },
                "metrics": { "saved_tokens": 12 },
                "ccr": { "raw_available": false },
                "memory": { "raw_available": false },
                "output_shape": { "raw_available": false },
                "route": { "id": "agent-route" }
            }
        });
        let result = headroom_derived_runtime_capability_status(&dashboard);
        assert_eq!(result["ok"], true);
        assert_eq!(result["passed"], 6);
        assert_eq!(result["total"], 6);
        assert!(result["privacy"]
            .as_str()
            .unwrap_or_default()
            .contains("no external Headroom service"));
    }

    #[test]
    fn route_profile_draft_rejects_plaintext_secret_refs() {
        with_isolated_app_state("route-profile", || {
            let result = save_route_profile_draft(RouteProfileDraftRequest {
                route_id: "opencode-real".to_string(),
                route_name: "OpenCode Real".to_string(),
                provider_id: "opencode-zen".to_string(),
                provider_name: "OpenCode Zen".to_string(),
                provider_kind: "custom".to_string(),
                protocol: "openai".to_string(),
                base_url: "https://opencode.ai/zen/v1".to_string(),
                api_key_ref: Some("plaintext-secret-value".to_string()),
                models: vec!["deepseek-v4-flash-free".to_string()],
                chain_mode: "direct".to_string(),
                compression_policy: "balanced".to_string(),
                local_port: DTOKEN_SERVICE_PORT,
                agent: "OpenCode".to_string(),
            });

            assert!(!result.ok);
            assert!(result.summary.contains("credential reference"));
            assert_eq!(list_route_profile_drafts()["count"], 0);
        });
    }

    #[test]
    fn route_profile_draft_rejects_non_opaque_credential_references() {
        with_isolated_app_state("route-profile-invalid-ref", || {
            let result = save_route_profile_draft(RouteProfileDraftRequest {
                route_id: "secured-route".to_string(),
                route_name: "Secured route".to_string(),
                provider_id: "secured-provider".to_string(),
                provider_name: "Secured provider".to_string(),
                provider_kind: "official".to_string(),
                protocol: "openai".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                api_key_ref: Some("plain-token-value-without-sk-prefix".to_string()),
                models: vec!["model".to_string()],
                chain_mode: "direct".to_string(),
                compression_policy: "balanced".to_string(),
                local_port: DTOKEN_SERVICE_PORT,
                agent: "OpenCode".to_string(),
            });

            assert!(!result.ok);
            assert!(result.summary.contains("credential reference"));
            assert_eq!(list_route_profile_drafts()["count"], 0);
        });
    }

    #[test]
    fn route_apply_requires_a_current_explicit_review() {
        with_isolated_app_state("route-review", || {
            assert!(save_test_route("reviewed-route", "https://one.example.test/v1").ok);

            let unreviewed = apply_route_profile_draft(ApplyRouteDraftRequest {
                route_id: "reviewed-route".to_string(),
                reviewed_before_sha256: None,
                reviewed_after_sha256: None,
            });
            assert!(!unreviewed.ok);
            assert!(!active_config_path().exists());
            assert!(!config_backup_root().exists());

            let stale_review = preview_route_profile_draft_apply(RouteDraftReviewRequest {
                route_id: "reviewed-route".to_string(),
            });
            assert_eq!(stale_review["ok"], true);
            assert!(!active_config_path().exists());
            assert!(!config_backup_root().exists());

            assert!(save_test_route("reviewed-route", "https://two.example.test/v1").ok);
            let stale_apply = apply_route_profile_draft(ApplyRouteDraftRequest {
                route_id: "reviewed-route".to_string(),
                reviewed_before_sha256: stale_review["before_sha256"].as_str().map(str::to_string),
                reviewed_after_sha256: stale_review["after_sha256"].as_str().map(str::to_string),
            });
            assert!(!stale_apply.ok);
            assert!(stale_apply.summary.contains("stale"));
            assert!(!active_config_path().exists());

            let applied = review_and_apply_route("reviewed-route");
            assert!(applied.ok);
            assert_eq!(
                active_route_config_status()["route"]["id"],
                "reviewed-route"
            );
            assert_eq!(rollback_points_result().expect("rollback points").len(), 1);
        });
    }

    #[test]
    fn rollback_restore_reinstates_previous_dtoken_active_config() {
        with_isolated_app_state("rollback", || {
            let first = save_route_profile_draft(RouteProfileDraftRequest {
                route_id: "first-route".to_string(),
                route_name: "First Route".to_string(),
                provider_id: "first-provider".to_string(),
                provider_name: "First Provider".to_string(),
                provider_kind: "custom".to_string(),
                protocol: "openai".to_string(),
                base_url: "https://first.example.test/v1".to_string(),
                api_key_ref: None,
                models: vec!["mock/model".to_string()],
                chain_mode: "direct".to_string(),
                compression_policy: "balanced".to_string(),
                local_port: DTOKEN_SERVICE_PORT,
                agent: "OpenCode".to_string(),
            });
            assert!(first.ok);
            assert!(review_and_apply_route("first-route").ok);

            let second = save_route_profile_draft(RouteProfileDraftRequest {
                route_id: "second-route".to_string(),
                route_name: "Second Route".to_string(),
                provider_id: "second-provider".to_string(),
                provider_name: "Second Provider".to_string(),
                provider_kind: "custom".to_string(),
                protocol: "openai".to_string(),
                base_url: "https://second.example.test/v1".to_string(),
                api_key_ref: None,
                models: vec!["mock/model".to_string()],
                chain_mode: "direct".to_string(),
                compression_policy: "balanced".to_string(),
                local_port: DTOKEN_SERVICE_PORT,
                agent: "OpenCode".to_string(),
            });
            assert!(second.ok);
            let second_review = preview_route_profile_draft_apply(RouteDraftReviewRequest {
                route_id: "second-route".to_string(),
            });
            assert_eq!(second_review["ok"], true);
            let second_after = second_review["after_sha256"]
                .as_str()
                .expect("second route after hash")
                .to_string();
            assert!(
                apply_route_profile_draft(ApplyRouteDraftRequest {
                    route_id: "second-route".to_string(),
                    reviewed_before_sha256: second_review["before_sha256"]
                        .as_str()
                        .map(str::to_string),
                    reviewed_after_sha256: Some(second_after.clone()),
                })
                .ok
            );
            assert_eq!(active_route_config_status()["route"]["id"], "second-route");

            let points = rollback_points_result().expect("read rollback points");
            let rollback_id = points
                .iter()
                .find(|point| {
                    point.get("expectedCurrentSha256").and_then(Value::as_str)
                        == Some(second_after.as_str())
                })
                .and_then(|point| point.get("id").and_then(Value::as_str))
                .expect("second route rollback point should exist")
                .to_string();
            let restored = manage_backup("restore".to_string(), rollback_id);
            assert!(restored.ok);
            assert!(restored.summary.contains("active config restored"));
            assert_eq!(active_route_config_status()["route"]["id"], "first-route");
        });
    }

    #[test]
    fn project_policy_settings_round_trip_merges_payloads() {
        with_isolated_app_state("project-policy", || {
            let first = save_app_settings(
                "project-policy".to_string(),
                json!({
                    "policy": {
                        "project_id": "desktop-project",
                        "level": "balanced",
                        "enable_ccr": true,
                        "enable_memory": true
                    },
                    "privacy": "aggregate runtime metrics only"
                }),
            );
            assert!(first.ok);

            let second = save_app_settings(
                "project-policy".to_string(),
                json!({
                    "policy": {
                        "enable_output_shaper": true,
                        "holdout_ratio": 0.1
                    },
                    "source": "dtoken-project-policy"
                }),
            );
            assert!(second.ok);

            let saved = get_app_settings("project-policy".to_string());
            assert_eq!(saved["state"], "loaded");
            assert_eq!(saved["payload"]["policy"]["project_id"], "desktop-project");
            assert_eq!(saved["payload"]["policy"]["level"], "balanced");
            assert_eq!(saved["payload"]["policy"]["enable_ccr"], true);
            assert_eq!(saved["payload"]["policy"]["enable_output_shaper"], true);
            assert_eq!(saved["payload"]["policy"]["holdout_ratio"], 0.1);
            assert_eq!(saved["payload"]["source"], "dtoken-project-policy");
        });
    }
}
