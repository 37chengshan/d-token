use crate::{
    enforce_cache_quota,
    proxy::{base_command, configure_upstream},
    LaunchConfig, RepositoryWriteSentinel, MAX_MCP_RESPONSE_BYTES, POLICY_VERSION,
};
use anyhow::{bail, Context, Result};
use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    process::Stdio,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{net::TcpListener, sync::oneshot};
use uuid::Uuid;

const MAX_HOST_SECRET_BYTES: usize = 4096;
const MIN_TOKEN_BYTES: usize = 32;
const MAX_QUERY_CHARS: usize = 240;
const DEFAULT_HOST_TTL_SECS: u64 = 15 * 60;
const MAX_HOST_TTL_SECS: u64 = 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexReceipt {
    pub schema_version: u32,
    pub operation: String,
    pub outcome: String,
    pub reason_code: String,
    pub policy_version: u32,
    pub indexed_files: Option<u64>,
    pub node_count: Option<u64>,
    pub edge_count: Option<u64>,
    pub occurred_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiHostReady {
    pub schema_version: u32,
    pub component: String,
    pub instance_id: String,
    pub owner: String,
    pub host: String,
    pub port: u16,
    pub parent_pid: u32,
    pub expires_at_ms: u64,
    pub policy_version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostSecret {
    token: String,
}

#[derive(Clone)]
struct HostState {
    config: LaunchConfig,
    token_digest: [u8; 32],
    instance_id: String,
    expires_at_ms: u64,
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchQuery {
    q: String,
}

pub fn run_index_controlled(
    config: &LaunchConfig,
    mode: &str,
    input: impl Read + Send + 'static,
) -> Result<IndexReceipt> {
    if !matches!(mode, "full" | "moderate" | "fast") {
        bail!("unsupported index mode");
    }
    config.validate_repository_inputs()?;
    config.ensure_repository_zero_write_precondition()?;
    configure_upstream(config)?;
    let sentinel = RepositoryWriteSentinel::capture(&config.repository_root)?;
    let arguments = serde_json::to_string(&json!({
        "repo_path": config.repository_root,
        "mode": mode,
        "persistence": false
    }))?;
    let mut command = base_command(config);
    command
        .args(["cli", "index_repository", &arguments])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .context("failed to start controlled codebase-memory index")?;

    let (cancel_tx, cancel_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(input);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) if line.trim().eq_ignore_ascii_case("cancel") => {
                    let _ = cancel_tx.send(());
                    return;
                }
                Ok(_) => {}
            }
        }
    });

    let status = loop {
        if cancel_rx.try_recv().is_ok() {
            terminate_index_child(&mut child);
            sentinel.verify_unchanged()?;
            return Ok(IndexReceipt {
                schema_version: 1,
                operation: "index_repository".to_string(),
                outcome: "cancelled".to_string(),
                reason_code: "user_cancelled".to_string(),
                policy_version: POLICY_VERSION,
                indexed_files: None,
                node_count: None,
                edge_count: None,
                occurred_at_ms: now_ms(),
            });
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(50));
    };

    let mut stdout = child.stdout.take().context("index stdout unavailable")?;
    let mut bytes = Vec::new();
    stdout
        .by_ref()
        .take(u64::try_from(MAX_MCP_RESPONSE_BYTES).unwrap_or(u64::MAX) + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_MCP_RESPONSE_BYTES {
        bail!("index response exceeds the size limit");
    }
    sentinel.verify_unchanged()?;
    if !status.success() {
        bail!("controlled codebase-memory index failed");
    }
    let value: Value = serde_json::from_slice(&bytes).context("invalid index response JSON")?;
    enforce_cache_quota(&config.cache_dir)?;
    Ok(IndexReceipt {
        schema_version: 1,
        operation: "index_repository".to_string(),
        outcome: "ready".to_string(),
        reason_code: "index_completed".to_string(),
        policy_version: POLICY_VERSION,
        indexed_files: first_u64(&value, &["files_indexed", "indexed_files", "file_count"]),
        node_count: first_u64(&value, &["nodes", "node_count", "total_nodes"]),
        edge_count: first_u64(&value, &["edges", "edge_count", "total_edges"]),
        occurred_at_ms: now_ms(),
    })
}

fn terminate_index_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let process_group = i32::try_from(child.id()).unwrap_or(i32::MAX);
        // SAFETY: the child was placed in a new process group with its PID as PGID.
        unsafe {
            libc::kill(-process_group, libc::SIGTERM);
        }
        for _ in 0..20 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        // SAFETY: same verified process group as above; SIGKILL is the bounded fallback.
        unsafe {
            libc::kill(-process_group, libc::SIGKILL);
        }
        let _ = child.wait();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = child.wait();
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn run_ui_host(
    config: LaunchConfig,
    input: impl Read + Send + 'static,
    mut ready_output: impl Write,
    parent_pid: u32,
    ttl_secs: Option<u64>,
) -> Result<()> {
    config.validate_repository_inputs()?;
    config.ensure_repository_zero_write_precondition()?;
    configure_upstream(&config)?;
    let mut reader = BufReader::new(input);
    let secret = read_host_secret(&mut reader)?;
    let ttl_secs = ttl_secs
        .unwrap_or(DEFAULT_HOST_TTL_SECS)
        .clamp(60, MAX_HOST_TTL_SECS);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()?;
    runtime.block_on(async move {
        let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .context("failed to bind codebase-memory UI host to loopback")?;
        let port = listener.local_addr()?.port();
        let instance_id = Uuid::new_v4().to_string();
        let expires_at_ms = now_ms().saturating_add(ttl_secs.saturating_mul(1000));
        let state = HostState {
            config,
            token_digest: Sha256::digest(secret.token.as_bytes()).into(),
            instance_id: instance_id.clone(),
            expires_at_ms,
            port,
        };
        let router = Router::new()
            .route("/s/{token}/", get(graph_page))
            .route("/s/{token}/api/health", get(host_health))
            .route("/s/{token}/api/architecture", get(architecture))
            .route("/s/{token}/api/search", get(search))
            .with_state(state);
        let ready = UiHostReady {
            schema_version: 1,
            component: "dtoken-codebase-memory-ui-host".to_string(),
            instance_id,
            owner: "d-token".to_string(),
            host: "127.0.0.1".to_string(),
            port,
            parent_pid,
            expires_at_ms,
            policy_version: POLICY_VERSION,
        };
        serde_json::to_writer(&mut ready_output, &ready)?;
        ready_output.write_all(b"\n")?;
        ready_output.flush()?;

        let (closed_tx, closed_rx) = oneshot::channel();
        thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => {
                        let _ = closed_tx.send(());
                        return;
                    }
                    Ok(_) => {}
                }
            }
        });
        let shutdown = async move {
            tokio::select! {
                _ = closed_rx => {},
                _ = tokio::time::sleep(Duration::from_secs(ttl_secs)) => {},
            }
        };
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await
            .context("codebase-memory UI host failed")?;
        Ok::<_, anyhow::Error>(())
    })
}

fn read_host_secret(reader: &mut impl BufRead) -> Result<HostSecret> {
    let mut line = Vec::new();
    reader
        .take(u64::try_from(MAX_HOST_SECRET_BYTES).unwrap_or(u64::MAX) + 2)
        .read_until(b'\n', &mut line)?;
    if line.len() > MAX_HOST_SECRET_BYTES {
        bail!("UI host secret exceeds the size limit");
    }
    let secret: HostSecret = serde_json::from_slice(&line).context("invalid UI host secret")?;
    if secret.token.len() < MIN_TOKEN_BYTES || secret.token.len() > 512 {
        bail!("UI host token length is invalid");
    }
    Ok(secret)
}

async fn graph_page(
    AxumPath(token): AxumPath<String>,
    State(state): State<HostState>,
    headers: HeaderMap,
) -> Response {
    if !authorized_request(&token, &state, &headers) {
        return StatusCode::NOT_FOUND.into_response();
    }
    secured(Html(GRAPH_PAGE).into_response(), "text/html; charset=utf-8")
}

async fn host_health(
    AxumPath(token): AxumPath<String>,
    State(state): State<HostState>,
    headers: HeaderMap,
) -> Response {
    if !authorized_request(&token, &state, &headers) {
        return StatusCode::NOT_FOUND.into_response();
    }
    secured(
        Json(json!({
            "component":"dtoken-codebase-memory-ui-host",
            "instance_id":state.instance_id,
            "owner":"d-token",
            "expires_at_ms":state.expires_at_ms,
            "policy_version":POLICY_VERSION
        }))
        .into_response(),
        "application/json",
    )
}

async fn architecture(
    AxumPath(token): AxumPath<String>,
    State(state): State<HostState>,
    headers: HeaderMap,
) -> Response {
    if !authorized_request(&token, &state, &headers) {
        return StatusCode::NOT_FOUND.into_response();
    }
    cli_response(state, "get_architecture", json!({})).await
}

async fn search(
    AxumPath(token): AxumPath<String>,
    State(state): State<HostState>,
    Query(query): Query<SearchQuery>,
    headers: HeaderMap,
) -> Response {
    if !authorized_request(&token, &state, &headers) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let query = query.q.trim();
    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
        return secured(
            Json(json!({"error":"invalid_query"})).into_response(),
            "application/json",
        );
    }
    cli_response(
        state,
        "search_graph",
        json!({"query":query, "limit":50, "offset":0}),
    )
    .await
}

async fn cli_response(state: HostState, tool: &'static str, arguments: Value) -> Response {
    match tokio::task::spawn_blocking(move || run_cli_tool(&state.config, tool, arguments)).await {
        Ok(Ok(value)) => secured(Json(value).into_response(), "application/json"),
        _ => secured(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":"graph_unavailable"})),
            )
                .into_response(),
            "application/json",
        ),
    }
}

fn run_cli_tool(config: &LaunchConfig, tool: &str, arguments: Value) -> Result<Value> {
    if !matches!(tool, "get_architecture" | "search_graph" | "index_status") {
        bail!("UI host tool is not allowed");
    }
    let mut arguments = arguments
        .as_object()
        .cloned()
        .context("read-only graph arguments must be an object")?;
    arguments.insert(
        "project".to_string(),
        Value::String(resolve_project(config)?),
    );
    let input = serde_json::to_vec(&arguments)?;
    let mut child = base_command(config)
        .args(["cli", tool])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .context("upstream CLI stdin unavailable")?
        .write_all(&input)?;
    drop(child.stdin.take());
    let output = child.wait_with_output()?;
    if !output.status.success() || output.stdout.len() > MAX_MCP_RESPONSE_BYTES {
        bail!("read-only graph query failed");
    }
    serde_json::from_slice(&output.stdout).context("invalid read-only graph response")
}

fn resolve_project(config: &LaunchConfig) -> Result<String> {
    let mut child = base_command(config)
        .args(["cli", "list_projects"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .context("upstream CLI stdin unavailable")?
        .write_all(b"{}")?;
    drop(child.stdin.take());
    let output = child.wait_with_output()?;
    if !output.status.success() || output.stdout.len() > MAX_MCP_RESPONSE_BYTES {
        bail!("project discovery failed");
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    resolve_project_name(&value, &config.repository_root)
}

fn resolve_project_name(value: &Value, approved_root: &std::path::Path) -> Result<String> {
    let projects = value
        .get("projects")
        .and_then(Value::as_array)
        .context("project discovery response is invalid")?;
    let approved_root = approved_root.to_string_lossy();
    projects
        .iter()
        .find(|project| project.get("root_path").and_then(Value::as_str) == Some(&approved_root))
        .and_then(|project| project.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .context("approved project is not indexed")
}

fn authorized(token: &str, state: &HostState) -> bool {
    let candidate: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    candidate
        .iter()
        .zip(state.token_digest.iter())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
        && now_ms() < state.expires_at_ms
}

fn authorized_request(token: &str, state: &HostState, headers: &HeaderMap) -> bool {
    let expected_host = format!("127.0.0.1:{}", state.port);
    if headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        != Some(&expected_host)
    {
        return false;
    }
    let expected_origin = format!("http://{expected_host}");
    if headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| origin != expected_origin)
    {
        return false;
    }
    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|site| !matches!(site, "same-origin" | "none"))
    {
        return false;
    }
    authorized(token, state)
}

fn secured(mut response: Response, content_type: &'static str) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; connect-src 'self'; img-src 'self' data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        ),
    );
    response
}

fn first_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(value) = object.get(*key).and_then(Value::as_u64) {
                    return Some(value);
                }
            }
            object.values().find_map(|value| first_u64(value, keys))
        }
        Value::Array(values) => values.iter().find_map(|value| first_u64(value, keys)),
        _ => None,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

const GRAPH_PAGE: &str = r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>d-token · Codebase Memory</title><style>
:root{color-scheme:light dark;font-family:Inter,ui-sans-serif,system-ui;background:#0b0d12;color:#eef2ff}*{box-sizing:border-box}
body{margin:0;min-height:100vh;background:radial-gradient(circle at 15% 0,#1c2944 0,transparent 38%),#0b0d12}.shell{max-width:1180px;margin:auto;padding:28px}
header{display:flex;justify-content:space-between;gap:18px;align-items:end;margin-bottom:18px}h1{font-size:25px;margin:0 0 6px}.muted{color:#9aa7bd;font-size:13px}
.surface{background:rgba(18,22,31,.92);border:1px solid #293247;border-radius:14px;padding:18px;margin-bottom:14px;box-shadow:0 16px 40px rgba(0,0,0,.2)}
.toolbar{display:flex;gap:10px}input{flex:1;background:#0d111a;border:1px solid #344058;border-radius:9px;padding:10px 12px;color:inherit}button{background:#5a7dff;color:#fff;border:0;border-radius:9px;padding:10px 15px;font-weight:650;cursor:pointer}
#graph{display:grid;grid-template-columns:repeat(auto-fill,minmax(230px,1fr));gap:10px;margin-top:14px}.node{border:1px solid #303b51;border-radius:11px;padding:12px;background:#111722}.node b{display:block;overflow-wrap:anywhere}.node small{color:#94a3b8}
pre{white-space:pre-wrap;overflow-wrap:anywhere;max-height:360px;overflow:auto;color:#c8d4e8;font-size:12px;line-height:1.55}.status{font-size:12px;color:#91a3c1}
@media(max-width:700px){.shell{padding:16px}header{align-items:start;flex-direction:column}.toolbar{flex-direction:column}}
</style></head><body><div class="shell"><header><div><h1>代码图谱</h1><div class="muted">d-token 授权的只读本地图谱视图 · 不暴露上游管理端点</div></div><div class="status" id="status">正在连接…</div></header>
<section class="surface"><div class="toolbar"><input id="query" maxlength="240" placeholder="搜索函数、类、路由或概念"><button id="search">搜索图谱</button></div><div id="graph"></div></section>
<section class="surface"><strong>架构摘要</strong><pre id="architecture">正在读取…</pre></section></div><script>
const base=location.pathname;const statusEl=document.querySelector('#status'),graph=document.querySelector('#graph'),arch=document.querySelector('#architecture');
const api=(path)=>fetch(base+'api/'+path,{cache:'no-store',headers:{accept:'application/json'}}).then(r=>{if(!r.ok)throw new Error('unavailable');return r.json()});
const text=v=>typeof v==='string'?v:JSON.stringify(v,null,2);function items(v){if(Array.isArray(v))return v;if(v&&typeof v==='object'){for(const k of ['results','nodes','matches','items'])if(Array.isArray(v[k]))return v[k];for(const x of Object.values(v)){const r=items(x);if(r.length)return r}}return []}
api('health').then(v=>statusEl.textContent='本地会话已验证 · '+v.instance_id.slice(0,8)).catch(()=>statusEl.textContent='会话不可用');api('architecture').then(v=>arch.textContent=text(v)).catch(()=>arch.textContent='架构摘要暂不可用');
async function search(){const q=document.querySelector('#query').value.trim();if(!q)return;graph.innerHTML='<div class="muted">正在查询…</div>';try{const v=await api('search?q='+encodeURIComponent(q));const rows=items(v);graph.innerHTML=rows.length?rows.slice(0,50).map((x,i)=>{const name=x.name||x.qualified_name||x.path||('节点 '+(i+1));const kind=x.type||x.label||x.kind||'graph';return '<article class="node"><b>'+escapeHtml(String(name))+'</b><small>'+escapeHtml(String(kind))+'</small></article>'}).join(''):'<div class="muted">没有匹配节点</div>'}catch{graph.innerHTML='<div class="muted">查询暂不可用</div>'}}
function escapeHtml(s){return s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}document.querySelector('#search').onclick=search;document.querySelector('#query').onkeydown=e=>{if(e.key==='Enter')search()};
</script></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_check_is_exact_and_expiry_aware() {
        let token = "a".repeat(MIN_TOKEN_BYTES);
        let state = HostState {
            config: LaunchConfig {
                upstream_path: "upstream".into(),
                repository_root: "repo".into(),
                cache_dir: "cache".into(),
            },
            token_digest: Sha256::digest(token.as_bytes()).into(),
            instance_id: "instance".to_string(),
            expires_at_ms: now_ms() + 10_000,
            port: 41234,
        };
        assert!(authorized(&token, &state));
        assert!(!authorized(&format!("{token}x"), &state));
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:41234"));
        assert!(authorized_request(&token, &state, &headers));
        headers.insert(header::HOST, HeaderValue::from_static("evil.example"));
        assert!(!authorized_request(&token, &state, &headers));
    }

    #[test]
    fn graph_page_has_no_native_upstream_admin_routes_or_external_assets() {
        for denied in [
            "/api/browse",
            "/api/logs",
            "/api/processes",
            "/rpc",
            "http://",
            "https://",
        ] {
            assert!(!GRAPH_PAGE.contains(denied));
        }
        assert!(GRAPH_PAGE.contains("代码图谱"));
        assert!(GRAPH_PAGE.contains("api('search"));
    }

    #[test]
    fn count_extraction_is_metadata_only() {
        let value = json!({"summary":{"files_indexed":12,"nodes":34,"edges":56},"source":"secret"});
        assert_eq!(first_u64(&value, &["files_indexed"]), Some(12));
        assert_eq!(first_u64(&value, &["nodes"]), Some(34));
        assert_eq!(first_u64(&value, &["edges"]), Some(56));
    }

    #[test]
    fn project_resolution_requires_an_exact_approved_root_match() {
        let projects = json!({"projects":[{"name":"other","root_path":"/other"}]});
        assert!(resolve_project_name(&projects, std::path::Path::new("/approved")).is_err());
        assert_eq!(
            resolve_project_name(&projects, std::path::Path::new("/other")).unwrap(),
            "other"
        );
    }
}
