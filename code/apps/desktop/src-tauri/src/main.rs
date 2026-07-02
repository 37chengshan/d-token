use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env,
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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

fn now_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() % 86_400)
        .unwrap_or_default();
    format!("{:02}:{:02}:{:02}", secs / 3_600, (secs % 3_600) / 60, secs % 60)
}

fn log(level: &str, message: impl Into<String>) -> ActionLog {
    ActionLog {
        time: now_label(),
        level: level.to_string(),
        message: message.into(),
    }
}

fn app_state_dir() -> PathBuf {
    env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir())
        .join("d-token")
}

fn write_app_state(relative: &[&str], content: &str) -> Result<PathBuf, String> {
    let mut path = app_state_dir();
    for part in relative {
        path.push(part);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(&path, content).map_err(|err| err.to_string())?;
    Ok(path)
}

fn rollback_points_json() -> Vec<Value> {
    let dir = app_state_dir().join("rollback-points");
    let mut points = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("rollback")
                .to_string();
            let content = fs::read_to_string(&path).unwrap_or_default();
            let parsed: Value = serde_json::from_str(&content).unwrap_or_else(|_| json!({}));
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
    if points.is_empty() {
        points.push(json!({
            "id": "local-plan",
            "label": "Before local route apply",
            "when": now_label(),
            "files": 0,
            "tools": ["headroom", "rtk"],
            "verified": false
        }));
    }
    points
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
    let writes_enabled = req.mode == "install" || req.mode == "repair";
    json!({
        "target": req.target,
        "mode": req.mode,
        "installRoot": req.install_root,
        "targets": targets,
        "writesEnabled": writes_enabled,
        "opencodeTestPort": req.proxy_port,
        "testScope": "opencode-only",
        "excludedAgents": ["claude-code", "cursor", "codex-cli", "aider", "cline", "roo", "continue", "windsurf"],
        "transaction": [
            { "id": "scan", "label": "扫描", "detail": "检测 RTK、Headroom、d-token desktop、OpenCode 与本地端口" },
            { "id": "plan", "label": "生成计划", "detail": "根据所选目标生成安装/验证计划" },
            { "id": "diff", "label": "展示 diff", "detail": "写入前展示 d-token 将管理的配置差异" },
            { "id": "backup", "label": "备份", "detail": if req.auto_backup { "写入前创建 d-token 回滚清单" } else { "用户关闭自动备份，事务会记录风险" } },
            { "id": "apply", "label": "应用", "detail": if writes_enabled { "仅执行用户选择的托管动作" } else { "只读连接模式只验证，不写入外部配置" } },
            { "id": "verify", "label": "校验", "detail": "读取版本、路径、健康端点和测试隔离状态" },
            { "id": "doctor", "label": "Doctor", "detail": if req.run_doctor_after { "安装后自动运行 Doctor" } else { "用户选择跳过自动 Doctor" } },
            { "id": "rollback", "label": "回滚点", "detail": if req.rollback_point { "提交可审计回滚点" } else { "不创建回滚点" } }
        ],
        "diff": [
            { "type": "ctx", "text": "# d-token installation transaction" },
            { "type": "add", "text": format!("+ mode = \"{}\"", req.mode) },
            { "type": "add", "text": format!("+ install_root = \"{}\"", req.install_root) },
            { "type": "add", "text": format!("+ targets = {:?}", targets) },
            { "type": "add", "text": format!("+ opencode_test_port = {}", req.proxy_port) },
            { "type": "add", "text": "+ test_scope = \"opencode-only\"" },
            { "type": "add", "text": "+ excluded_agents = [\"claude-code\", \"cursor\", \"codex-cli\", \"aider\", \"cline\", \"roo\"]" },
            { "type": "add", "text": format!("+ auto_backup = {}", req.auto_backup) },
            { "type": "add", "text": format!("+ doctor_after_apply = {}", req.run_doctor_after) },
            { "type": "add", "text": format!("+ rollback_point = {}", req.rollback_point) }
        ],
        "warnings": if req.proxy_port == 8788 {
            vec!["OpenCode test port is isolated to OpenCode child-process tests."]
        } else {
            vec!["The selected test port is not the default 8788; ensure it is reserved for OpenCode tests only."]
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

fn run_opencode_test_capture(args: &[&str]) -> Result<(bool, String), String> {
    let output = Command::new("opencode")
        .args(args)
        .env("DTOKEN_TEST_SCOPE", "opencode-only")
        .env(
            "DTOKEN_TEST_EXCLUDE",
            "claude-code,codex-cli,cursor,vscode,aider,cline,roo,continue,windsurf",
        )
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env_remove("ANTHROPIC_BASE_URL")
        .env_remove("ANTHROPIC_API_URL")
        .env_remove("CLAUDE_CODE_PROXY")
        .output()
        .map_err(|err| format!("opencode: {err}"))?;
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

fn user_desktop_path(file_name: &str) -> Option<PathBuf> {
    env::var("USERPROFILE")
        .ok()
        .map(PathBuf::from)
        .map(|home| home.join("Desktop").join(file_name))
        .filter(|path| path.exists())
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
        8787 => "Claude / Anthropic proxy",
        8788 => "OpenCode test proxy",
        _ => "Headroom proxy",
    }
}

fn headroom_memory_db_path() -> Option<String> {
    env::var("USERPROFILE").ok().map(|home| {
        PathBuf::from(home)
            .join(".headroom")
            .join("memory.db")
            .display()
            .to_string()
    })
}

fn spawn_headroom_endpoint(port: u16) -> Result<(), String> {
    let memory_db = headroom_memory_db_path().unwrap_or_else(|| ".headroom/memory.db".to_string());
    let mut args = vec![
        "proxy".to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--mode".to_string(),
        "token".to_string(),
    ];

    if port == 8787 {
        args.extend([
            "--backend".to_string(),
            "anthropic".to_string(),
            "--anthropic-api-url".to_string(),
            "http://127.0.0.1:15729".to_string(),
        ]);
    } else {
        args.extend([
            "--backend".to_string(),
            "anyllm".to_string(),
            "--anyllm-provider".to_string(),
            "openai".to_string(),
            "--openai-api-url".to_string(),
            "https://api.ai-pixel.online".to_string(),
        ]);
    }

    args.extend([
        "--telemetry".to_string(),
        "--memory".to_string(),
        "--memory-db-path".to_string(),
        memory_db,
    ]);

    let mut command = Command::new("headroom");
    command.args(args).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command.spawn().map_err(|err| format!("headroom start failed: {err}"))?;
    Ok(())
}

#[cfg(windows)]
fn find_pid_on_port(port: u16) -> Option<String> {
    let output = Command::new("netstat").args(["-ano", "-p", "tcp"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let needle = format!("127.0.0.1:{port}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(&needle) && line.contains("LISTENING"))
        .filter_map(|line| line.split_whitespace().last())
        .find(|pid| pid.chars().all(|ch| ch.is_ascii_digit()))
        .map(ToString::to_string)
}

#[cfg(not(windows))]
fn find_pid_on_port(_port: u16) -> Option<String> {
    None
}

fn stop_headroom_endpoint(port: u16) -> Result<Option<String>, String> {
    let Some(pid) = find_pid_on_port(port) else {
        return Ok(None);
    };
    #[cfg(windows)]
    let output = Command::new("taskkill")
        .args(["/PID", &pid, "/T", "/F"])
        .output()
        .map_err(|err| format!("taskkill failed: {err}"))?;
    #[cfg(not(windows))]
    let output = Command::new("kill")
        .args(["-TERM", &pid])
        .output()
        .map_err(|err| format!("kill failed: {err}"))?;
    if output.status.success() {
        Ok(Some(pid))
    } else {
        let mut text = String::new();
        text.push_str(&String::from_utf8_lossy(&output.stdout));
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Err(text.trim().to_string())
    }
}

fn headroom_endpoints() -> Vec<HeadroomEndpoint> {
    vec![
        check_headroom_endpoint(8787, "Claude / Anthropic proxy"),
        check_headroom_endpoint(8788, "Codex / OpenAI-compatible proxy"),
    ]
}

fn state(ok: bool) -> &'static str {
    if ok { "ok" } else { "warn" }
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
                check.version.clone().unwrap_or_else(|| "version unknown".to_string())
            ),
        ));
        logs.push(log("info", format!("{label} verification finished successfully")));
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
    let allowed = ["rtk", "headroom", "app", "project-policy"];
    let mut logs = vec![log("info", format!("Saving d-token settings section: {section}"))];
    if !allowed.contains(&section.as_str()) {
        logs.push(log("error", format!("Unsupported settings section: {section}")));
        return ActionResult {
            ok: false,
            summary: "Settings section is not supported".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let content = json!({
        "section": section,
        "savedAt": now_label(),
        "payload": payload,
        "privacy": "d-token local state only; no API keys or raw CCR content should be stored here"
    });
    match write_app_state(
        &["settings", &format!("{}.json", content["section"].as_str().unwrap_or("app"))],
        &serde_json::to_string_pretty(&content).unwrap_or_else(|_| "{}".to_string()),
    ) {
        Ok(path) => {
            logs.push(log(
                "info",
                format!("Settings saved to {}", path.display()),
            ));
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
fn manage_backup(action: String, id: String) -> ActionResult {
    let mut logs = vec![log("info", format!("Backup action requested: {action} {id}"))];
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

    let path = app_state_dir()
        .join("rollback-points")
        .join(format!("{safe_id}.json"));
    match action.as_str() {
        "restore" => match fs::read_to_string(&path) {
            Ok(content) => {
                logs.push(log(
                    "info",
                    format!("Restore plan loaded from {}", path.display()),
                ));
                logs.push(log(
                    "warn",
                    "External config restore is gated; this action validates and stages the d-token rollback plan only",
                ));
                logs.push(log(
                    "info",
                    format!("Restore manifest bytes: {}", content.len()),
                ));
                ActionResult {
                    ok: true,
                    summary: "Rollback point staged for restore".to_string(),
                    logs,
                    tools: all_checks(),
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
                logs.push(log("info", format!("Removed rollback point {}", path.display())));
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
    let mut logs = vec![log("info", "Scanning local PATH for RTK, Headroom, and OpenCode")];
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
            "scope: OpenCode test port is isolated; Claude Code, Cursor, Codex CLI and system proxy settings are excluded",
        ),
    ];

    if targets.is_empty() {
        logs.push(log("error", format!("plan: unknown install target {}", request.target)));
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
            "plan: {} target(s), install_root={}, opencode_test_port={}",
            targets.len(),
            request.install_root,
            request.proxy_port
        ),
    ));
    logs.push(log("info", "diff: generated config diff for user confirmation"));

    if request.auto_backup {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        match write_app_state(
            &["backups", &stamp, "install-plan.json"],
            &serde_json::to_string_pretty(&plan).unwrap_or_else(|_| "{}".to_string()),
        ) {
            Ok(path) => logs.push(log(
                "info",
                format!("backup: wrote d-token rollback manifest to {}", path.display()),
            )),
            Err(err) => logs.push(log("error", format!("backup: failed to write manifest: {err}"))),
        }
    } else {
        logs.push(log(
            "warn",
            "backup: automatic backup is disabled; external writes remain blocked in connect/manual mode",
        ));
    }

    let writes_enabled = request.mode == "install" || request.mode == "repair";
    if !writes_enabled {
        logs.push(log(
            "info",
            "apply: current mode is read-only/manual, so external tool installation is verified but not changed",
        ));
    }

    let mut ok = true;
    for target in &targets {
        match *target {
            "rtk" => {
                let check = check_tool("rtk", "RTK");
                if check.installed {
                    logs.push(log(
                        "info",
                        format!(
                            "apply: RTK already installed and verified ({})",
                            check.version.unwrap_or_else(|| "version unknown".to_string())
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
                            check.version.unwrap_or_else(|| "version unknown".to_string())
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
                        format!("apply: d-token desktop binary verified at {}", path.display()),
                    )),
                    Err(err) => {
                        ok = false;
                        logs.push(log("error", format!("apply: d-token binary check failed: {err}")));
                    }
                }
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

    if request.rollback_point {
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
        match write_app_state(
            &["rollback-points", &format!("{stamp}.json")],
            &serde_json::to_string_pretty(&rollback).unwrap_or_else(|_| "{}".to_string()),
        ) {
            Ok(path) => logs.push(log(
                "info",
                format!("rollback: committed rollback point {}", path.display()),
            )),
            Err(err) => {
                ok = false;
                logs.push(log("error", format!("rollback: failed to write point: {err}")));
            }
        }
    } else {
        logs.push(log("warn", "rollback: rollback point disabled by user option"));
    }

    ActionResult {
        ok,
        summary: if ok {
            format!("{} installation transaction completed", request.target)
        } else {
            format!("{} installation transaction needs attention", request.target)
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
                if tool.installed { "installed" } else { "missing" }
            ),
        ));
    }
    let ok = tools.iter().filter(|tool| tool.id != "opencode").all(|tool| tool.installed);
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
        format!("Headroom lifecycle action requested: {action} on 127.0.0.1:{port}"),
    )];
    if !(port == 8787 || port == 8788) {
        logs.push(log(
            "error",
            "Only managed local Headroom ports 8787 and 8788 are allowed",
        ));
        return ActionResult {
            ok: false,
            summary: "Unsupported Headroom port".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    match action.as_str() {
        "health" => {
            let endpoint = check_headroom_endpoint(port, headroom_label(port));
            logs.push(log(
                if endpoint.healthy { "info" } else { "warn" },
                endpoint.detail,
            ));
            ActionResult {
                ok: endpoint.healthy,
                summary: format!("Headroom health check complete for {port}"),
                logs,
                tools: all_checks(),
            }
        }
        "start" => {
            let before = check_headroom_endpoint(port, headroom_label(port));
            if before.healthy {
                logs.push(log("info", format!("Headroom {port} is already healthy")));
                return ActionResult {
                    ok: true,
                    summary: format!("Headroom {port} already running"),
                    logs,
                    tools: all_checks(),
                };
            }
            match spawn_headroom_endpoint(port) {
                Ok(()) => {
                    std::thread::sleep(Duration::from_millis(900));
                    let endpoint = check_headroom_endpoint(port, headroom_label(port));
                    logs.push(log("info", format!("Started Headroom process for {port}")));
                    logs.push(log(
                        if endpoint.healthy { "info" } else { "warn" },
                        endpoint.detail,
                    ));
                    ActionResult {
                        ok: endpoint.healthy,
                        summary: if endpoint.healthy {
                            format!("Headroom {port} started")
                        } else {
                            format!("Headroom {port} start requested; health is not ready yet")
                        },
                        logs,
                        tools: all_checks(),
                    }
                }
                Err(err) => {
                    logs.push(log("error", err));
                    ActionResult {
                        ok: false,
                        summary: format!("Headroom {port} failed to start"),
                        logs,
                        tools: all_checks(),
                    }
                }
            }
        }
        "stop" => match stop_headroom_endpoint(port) {
            Ok(Some(pid)) => {
                logs.push(log(
                    "warn",
                    format!("Stopped process {pid} that was listening on 127.0.0.1:{port}"),
                ));
                ActionResult {
                    ok: true,
                    summary: format!("Headroom {port} stopped"),
                    logs,
                    tools: all_checks(),
                }
            }
            Ok(None) => {
                logs.push(log("info", format!("No listener found on 127.0.0.1:{port}")));
                ActionResult {
                    ok: true,
                    summary: format!("Headroom {port} was not running"),
                    logs,
                    tools: all_checks(),
                }
            }
            Err(err) => {
                logs.push(log("error", err));
                ActionResult {
                    ok: false,
                    summary: format!("Headroom {port} failed to stop"),
                    logs,
                    tools: all_checks(),
                }
            }
        },
        "restart" => {
            match stop_headroom_endpoint(port) {
                Ok(Some(pid)) => logs.push(log(
                    "warn",
                    format!("Stopped process {pid} before restart on 127.0.0.1:{port}"),
                )),
                Ok(None) => logs.push(log("info", "No existing listener found before restart")),
                Err(err) => logs.push(log("warn", format!("Stop before restart reported: {err}"))),
            }
            match spawn_headroom_endpoint(port) {
                Ok(()) => {
                    std::thread::sleep(Duration::from_millis(900));
                    let endpoint = check_headroom_endpoint(port, headroom_label(port));
                    logs.push(log("info", format!("Restarted Headroom process for {port}")));
                    logs.push(log(
                        if endpoint.healthy { "info" } else { "warn" },
                        endpoint.detail,
                    ));
                    ActionResult {
                        ok: endpoint.healthy,
                        summary: format!("Headroom {port} restart requested"),
                        logs,
                        tools: all_checks(),
                    }
                }
                Err(err) => {
                    logs.push(log("error", err));
                    ActionResult {
                        ok: false,
                        summary: format!("Headroom {port} failed to restart"),
                        logs,
                        tools: all_checks(),
                    }
                }
            }
        }
        _ => {
            logs.push(log("error", format!("Unknown Headroom action: {action}")));
            ActionResult {
                ok: false,
                summary: format!("Unknown Headroom action: {action}"),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn test_opencode(model: String) -> ActionResult {
    let mut logs = vec![
        log("info", format!("Testing OpenCode model: {model}")),
        log("info", "Running opencode in non-interactive mode"),
        log(
            "info",
            "Test scope is isolated to OpenCode only; other agents and global proxy settings are excluded",
        ),
    ];
    let prompt = "Reply with exactly: d-token-ok";
    let result = run_opencode_test_capture(&["run", "-m", &model, prompt]);
    match result {
        Ok((true, text)) => {
            logs.push(log("info", "OpenCode command completed"));
            logs.push(log(
                "info",
                "No Claude Code, Cursor, Codex CLI, or system-wide proxy configuration was changed",
            ));
            logs.push(log(
                "info",
                format!("Model output: {}", text.lines().next().unwrap_or("")),
            ));
            ActionResult {
                ok: true,
                summary: format!("OpenCode model {model} responded"),
                logs,
                tools: all_checks(),
            }
        }
        Ok((false, text)) => {
            logs.push(log("error", format!("OpenCode returned an error: {text}")));
            ActionResult {
                ok: false,
                summary: format!("OpenCode model {model} failed"),
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            logs.push(log("error", err));
            ActionResult {
                ok: false,
                summary: "OpenCode command could not start".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn get_desktop_data() -> Value {
    let tools = all_checks();
    let headroom = headroom_endpoints();
    let rtk_installed = tools.iter().any(|tool| tool.id == "rtk" && tool.installed);
    let headroom_installed = tools
        .iter()
        .any(|tool| tool.id == "headroom" && tool.installed);
    let opencode_installed = tools
        .iter()
        .any(|tool| tool.id == "opencode" && tool.installed);
    let claude_proxy_healthy = headroom.iter().any(|endpoint| endpoint.port == 8787 && endpoint.healthy);
    let codex_proxy_healthy = headroom.iter().any(|endpoint| endpoint.port == 8788 && endpoint.healthy);
    let headroom_healthy = headroom.iter().all(|endpoint| endpoint.healthy);
    let headroom_example_script = user_desktop_path("Headroom-Check.ps1")
        .map(|path| path.display().to_string());
    let project_root = env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "D:\\D-TOKEN\\d-token".to_string());

    json!({
        "tools": tools,
        "headroomEndpoints": headroom,
        "headroomExampleScript": headroom_example_script,
        "health": [
            { "key": "proxy", "label": "Proxy", "state": if headroom_healthy { "ok" } else { "warn" } },
            { "key": "rtk", "label": "RTK", "state": if rtk_installed { "ok" } else { "err" } },
            { "key": "headroom", "label": "Headroom", "state": if headroom_installed && headroom_healthy { "ok" } else if headroom_installed { "warn" } else { "err" } },
            { "key": "agents", "label": "Agents", "state": "ok" },
            { "key": "routes", "label": "Routes", "state": if headroom_healthy { "ok" } else { "warn" } },
            { "key": "providers", "label": "Providers", "state": "ok" }
        ],
        "doctorChecks": [
            { "check": "Runtime action boundary", "state": "ok", "detail": "Only user-triggered buttons call local commands; page load performs read-only detection." },
            { "check": "RTK availability", "state": if rtk_installed { "ok" } else { "err" }, "detail": if rtk_installed { "RTK binary is available on PATH." } else { "RTK was not found on PATH." } },
            { "check": "Headroom availability", "state": if headroom_installed { "ok" } else { "err" }, "detail": if headroom_installed { "Headroom binary is available on PATH." } else { "Headroom was not found on PATH." } },
            { "check": "Claude proxy health 8787", "state": state(claude_proxy_healthy), "detail": if claude_proxy_healthy { "127.0.0.1:8787 health endpoint responded." } else { "127.0.0.1:8787 health endpoint is not healthy." } },
            { "check": "Codex proxy health 8788", "state": state(codex_proxy_healthy), "detail": if codex_proxy_healthy { "127.0.0.1:8788 health endpoint responded." } else { "127.0.0.1:8788 health endpoint is not healthy." } },
            { "check": "OpenCode availability", "state": if opencode_installed { "ok" } else { "warn" }, "detail": if opencode_installed { "OpenCode command is available for model tests." } else { "OpenCode was not found; model tests will fail until installed." } },
            { "check": "OpenCode test isolation", "state": "ok", "detail": "Model tests run in an OpenCode-only child process scope; Claude Code, Cursor, Codex CLI, and system proxy config are excluded." },
            { "check": "Proxy binding host", "state": "ok", "detail": "Managed Headroom routes are expected to bind to 127.0.0.1 only." },
            { "check": "Config write safety", "state": "ok", "detail": "Config changes are represented as a diff plan before any write action is enabled." },
            { "check": "Backup readiness", "state": "ok", "detail": "d-token rollback points are written to local app state; restore/cleanup actions are explicit and logged." },
            { "check": "Desktop runtime", "state": "ok", "detail": "Tauri backend commands are available to the desktop UI." }
        ],
        "agents": [
            { "name": "Claude Code", "rtk": if rtk_installed { "ok" } else { "planned" }, "rtkMethod": "rtk shell wrapper / hook", "headroom": if claude_proxy_healthy { "ok" } else if headroom_installed { "partial" } else { "planned" }, "headroomMethod": "127.0.0.1:8787 Anthropic proxy", "macos": "ok", "win": "partial", "wsl": "ok", "note": if claude_proxy_healthy { "local proxy healthy" } else { "proxy health check needed" } },
            { "name": "Codex CLI", "rtk": if rtk_installed { "ok" } else { "planned" }, "rtkMethod": "rtk command filter", "headroom": "planned", "headroomMethod": "production route pending; excluded from OpenCode test port", "macos": "ok", "win": "ok", "wsl": "ok", "note": "not attached to test port" },
            { "name": "OpenCode", "rtk": if rtk_installed { "ok" } else { "partial" }, "rtkMethod": "rtk command filter", "headroom": if codex_proxy_healthy { "ok" } else { "partial" }, "headroomMethod": "OpenAI-compatible base URL", "macos": "ok", "win": if opencode_installed { "ok" } else { "partial" }, "wsl": "ok", "note": if opencode_installed { "model test command available" } else { "install OpenCode for model tests" } },
            { "name": "Cursor / VS Code", "rtk": if rtk_installed { "partial" } else { "planned" }, "rtkMethod": "rules / terminal wrapper", "headroom": if codex_proxy_healthy { "partial" } else { "planned" }, "headroomMethod": "manual proxy URL", "macos": "ok", "win": "ok", "wsl": "ok", "note": "requires per-tool proxy configuration" },
            { "name": "Cline / Roo Code", "rtk": if rtk_installed { "partial" } else { "planned" }, "rtkMethod": "rules / terminal wrapper", "headroom": if codex_proxy_healthy { "partial" } else { "planned" }, "headroomMethod": "OpenAI-compatible provider", "macos": "ok", "win": "ok", "wsl": "ok", "note": "configure provider base URL in the tool" },
            { "name": "Aider", "rtk": if rtk_installed { "ok" } else { "planned" }, "rtkMethod": "rtk shell wrapper", "headroom": if codex_proxy_healthy { "partial" } else { "planned" }, "headroomMethod": "OpenAI-compatible env", "macos": "ok", "win": "partial", "wsl": "ok", "note": "best through terminal workflow" }
        ],
        "projects": [
            {
                "root": project_root,
                "stack": "Rust · TypeScript · Tauri",
                "agents": ["Codex CLI", "OpenCode"],
                "route": if codex_proxy_healthy { "opencode-test (tests only)" } else { "local plan pending" },
                "rtkProfile": if rtk_installed { "balanced" } else { "not installed" },
                "policy": if headroom_installed { "adaptive" } else { "connect-only" },
                "level": if rtk_installed && headroom_installed { "L2 · Balanced" } else { "L1 · Connect" },
                "reason": "Current desktop workspace detected by the Tauri backend; policy is derived from local RTK, Headroom, and proxy health.",
                "features": ["Doctor", "config diff", "backup plan", "OpenCode model test"]
            }
        ],
        "savings": {
            "totalTokens": 4820000,
            "rtkTokens": if rtk_installed { 2910000 } else { 0 },
            "headroomTokens": if headroom_installed { 1910000 } else { 0 },
            "costUsd": 186.4,
            "deltaTotal": 12.4,
            "deltaRtk": 9.8,
            "deltaHeadroom": 15.2,
            "deltaCost": 11.1
        },
        "rtkTrend": [42, 55, 48, 63, 70, 66, 78],
        "headroomTrend": [30, 38, 52, 49, 61, 68, 74],
        "commandFamilies": [
            { "name": "test", "pct": 68 },
            { "name": "build", "pct": 54 },
            { "name": "git", "pct": 41 },
            { "name": "search", "pct": 72 },
            { "name": "package manager", "pct": 38 },
            { "name": "logs", "pct": 61 }
        ],
        "recentDecisions": [
            { "id": "req_live_8787", "agent": "Claude Code", "route": "anthropic-main", "provider": "Headroom 8787", "model": "anthropic-compatible", "saved": if headroom.iter().any(|e| e.port == 8787 && e.healthy) { "live" } else { "down" }, "when": "now" },
            { "id": "req_live_8788", "agent": "OpenCode", "route": "opencode-test", "provider": "Headroom 8788", "model": "openai-compatible test scope", "saved": if headroom.iter().any(|e| e.port == 8788 && e.healthy) { "live" } else { "down" }, "when": "now" }
        ],
        "recentEvents": [
            { "level": if headroom_healthy { "info" } else { "warn" }, "time": now_label(), "msg": if headroom_healthy { "Headroom proxy health checks passed" } else { "One or more Headroom proxy health checks failed" } },
            { "level": if rtk_installed { "info" } else { "err" }, "time": now_label(), "msg": if rtk_installed { "RTK binary detected on PATH" } else { "RTK binary missing from PATH" } }
        ],
        "routes": [
            { "id": "r1", "name": "anthropic-main", "protocol": "Anthropic-compatible", "host": "127.0.0.1:8787", "instance": "local", "provider": "Headroom Claude proxy", "chain": "direct", "policy": "adaptive", "fallback": "none", "errors": if headroom.iter().any(|e| e.port == 8787 && e.healthy) { 0 } else { 1 } },
            { "id": "r2", "name": "opencode-test", "protocol": "OpenAI-compatible", "host": "127.0.0.1:8788", "instance": "isolated-test", "provider": "Headroom OpenCode test proxy", "chain": "direct", "policy": "opencode-only", "fallback": "none", "errors": if headroom.iter().any(|e| e.port == 8788 && e.healthy) { 0 } else { 1 } }
        ],
        "providers": [
            { "name": "Headroom Claude proxy", "kind": "compatible", "protocol": "Anthropic", "baseUrl": "http://127.0.0.1:8787", "models": "Claude Code compatible traffic", "timeout": "60s", "health": if headroom.iter().any(|e| e.port == 8787 && e.healthy) { "ok" } else { "warn" }, "cred": "local-env" },
            { "name": "Headroom OpenCode test proxy", "kind": "compatible", "protocol": "OpenAI", "baseUrl": "http://127.0.0.1:8788", "models": "OpenCode model-test traffic only", "timeout": "60s", "health": if headroom.iter().any(|e| e.port == 8788 && e.healthy) { "ok" } else { "warn" }, "cred": "local-env" }
        ],
        "logs": [
            { "time": now_label(), "level": if headroom_healthy { "info" } else { "warn" }, "kind": "headroom", "msg": if headroom_healthy { "headroom.health all local proxies healthy" } else { "headroom.health local proxy warning" } },
            { "time": now_label(), "level": if rtk_installed { "info" } else { "err" }, "kind": "rtk", "msg": if rtk_installed { "rtk.version detected" } else { "rtk.version missing" } }
        ],
        "changes": [
            {
                "id": "c-local-headroom",
                "summary": "Use local Headroom proxy routes",
                "target": "d-token managed route plan",
                "risk": "low",
                "backup": "created during managed apply",
                "diff": [
                    { "type": "ctx", "text": "[route.anthropic-main]" },
                    { "type": "add", "text": "+ base_url = \"http://127.0.0.1:8787\"" },
                    { "type": "ctx", "text": "[route.opencode-test]" },
                    { "type": "add", "text": "+ test_scope = \"opencode-only\"" },
                    { "type": "add", "text": "+ base_url = \"http://127.0.0.1:8788\"" }
                ],
                "postChecks": ["Headroom 8787 health", "OpenCode-only 8788 test health", "Doctor"]
            }
        ],
        "backupPoints": rollback_points_json()
    })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_desktop_data,
            save_app_settings,
            manage_backup,
            detect_tools,
            install_tool,
            preview_install_plan,
            run_install_transaction,
            run_doctor,
            manage_headroom,
            test_opencode
        ])
        .run(tauri::generate_context!())
        .expect("error while running d-token desktop");
}
