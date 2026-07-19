use crate::{
    evaluate_request, jsonrpc_policy_error, LaunchConfig, RepositoryWriteSentinel,
    RequestDisposition, MAX_MCP_REQUEST_BYTES, MAX_MCP_RESPONSE_BYTES,
};
use anyhow::{bail, Context, Result};
use dtoken_process::{file_sha256, process_birth_token};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const AGENT_SESSION_RECEIPT_FILE: &str = ".dtoken-agent-session.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentSessionReceipt {
    pub schema_version: u32,
    pub instance_id: String,
    pub owner: String,
    pub state: String,
    pub pid: u32,
    pub parent_pid: u32,
    pub process_birth_token: String,
    pub launcher_sha256: String,
    pub upstream_sha256: String,
    pub project_ref: String,
    pub cache_ref: String,
    pub initialize_count: u64,
    pub tools_list_count: u64,
    pub tool_call_count: u64,
    pub denied_count: u64,
    pub started_at_ms: u64,
    pub last_seen_at_ms: u64,
    pub exited_at_ms: Option<u64>,
}

impl AgentSessionReceipt {
    fn new(config: &LaunchConfig) -> Result<Self> {
        let now = now_ms();
        let pid = std::process::id();
        let launcher = std::env::current_exe().context("launcher executable is unavailable")?;
        Ok(Self {
            schema_version: 1,
            instance_id: Uuid::new_v4().to_string(),
            owner: "agent".to_string(),
            state: "starting".to_string(),
            pid,
            parent_pid: parent_pid(),
            process_birth_token: process_birth_token(pid).unwrap_or_default(),
            launcher_sha256: file_sha256(&launcher)
                .map_err(anyhow::Error::msg)
                .context("failed to hash the launcher session executable")?,
            upstream_sha256: file_sha256(&config.upstream_path)
                .map_err(anyhow::Error::msg)
                .context("failed to hash the upstream session executable")?,
            project_ref: scope_ref(&config.repository_root),
            cache_ref: scope_ref(&config.cache_dir),
            initialize_count: 0,
            tools_list_count: 0,
            tool_call_count: 0,
            denied_count: 0,
            started_at_ms: now,
            last_seen_at_ms: now,
            exited_at_ms: None,
        })
    }

    fn observe_request(&mut self, request: &Value, denied: bool) {
        self.last_seen_at_ms = now_ms();
        match request.get("method").and_then(Value::as_str) {
            Some("initialize") => self.initialize_count = self.initialize_count.saturating_add(1),
            Some("tools/list") => self.tools_list_count = self.tools_list_count.saturating_add(1),
            Some("tools/call") => self.tool_call_count = self.tool_call_count.saturating_add(1),
            _ => {}
        }
        if denied {
            self.denied_count = self.denied_count.saturating_add(1);
        }
        self.state = if self.initialize_count > 0 {
            "observed".to_string()
        } else {
            "starting".to_string()
        };
    }

    fn finish(&mut self, failed: bool) {
        let now = now_ms();
        self.last_seen_at_ms = now;
        self.exited_at_ms = Some(now);
        self.state = if failed { "failed" } else { "exited" }.to_string();
    }

    fn persist(&self, cache_dir: &Path) -> Result<()> {
        let destination = cache_dir.join(AGENT_SESSION_RECEIPT_FILE);
        let temporary = cache_dir.join(format!(".{AGENT_SESSION_RECEIPT_FILE}.{}.tmp", self.pid));
        let bytes = serde_json::to_vec(self)?;
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &destination)?;
        Ok(())
    }
}

pub fn scope_ref(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(unix)]
fn parent_pid() -> u32 {
    // SAFETY: getppid has no preconditions and does not dereference memory.
    u32::try_from(unsafe { libc::getppid() }).unwrap_or(0)
}

#[cfg(not(unix))]
fn parent_pid() -> u32 {
    0
}

pub fn run_stdio_proxy(
    config: &LaunchConfig,
    input: impl Read,
    mut output: impl Write,
) -> Result<()> {
    config.ensure_repository_zero_write_precondition()?;
    configure_upstream(config)?;
    let mut receipt = AgentSessionReceipt::new(config)?;
    receipt.persist(&config.cache_dir)?;
    let mut child = spawn_upstream(config)?;
    let child_input = child.stdin.take().context("upstream stdin unavailable")?;
    let child_output = child.stdout.take().context("upstream stdout unavailable")?;
    let result = proxy_loop(
        config,
        input,
        &mut output,
        child_input,
        child_output,
        &mut receipt,
    );
    terminate_child(&mut child);
    receipt.finish(result.is_err());
    let _ = receipt.persist(&config.cache_dir);
    result
}

pub(crate) fn configure_upstream(config: &LaunchConfig) -> Result<()> {
    for key in ["auto_watch", "auto_index"] {
        let mut command = base_command(config);
        command
            .args(["config", "set", key, "false"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let status = run_bounded_status(&mut command, Duration::from_secs(30))
            .context("failed to configure isolated upstream cache")?;
        if !status.success() {
            bail!("failed to disable upstream automatic indexing or watching");
        }
    }
    Ok(())
}

fn run_bounded_status(
    command: &mut Command,
    timeout: Duration,
) -> Result<std::process::ExitStatus> {
    let mut child = command.spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("upstream configuration command timed out");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn spawn_upstream(config: &LaunchConfig) -> Result<Child> {
    base_command(config)
        .arg("--ui=false")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start verified codebase-memory upstream")
}

pub(crate) fn base_command(config: &LaunchConfig) -> Command {
    let mut command = Command::new(&config.upstream_path);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("CBM_ALLOWED_ROOT", &config.repository_root)
        .env("CBM_CACHE_DIR", &config.cache_dir)
        .env("CBM_DIAGNOSTICS", "0")
        .env("CBM_LOG_LEVEL", "none")
        .env("CBM_WORKERS", "2");
    for name in ["HOME", "USERPROFILE", "TMPDIR", "TEMP", "TMP", "LANG"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    if cfg!(target_os = "windows") {
        for name in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
    }
    command
}

fn proxy_loop(
    config: &LaunchConfig,
    input: impl Read,
    output: &mut impl Write,
    mut child_input: ChildStdin,
    child_output: ChildStdout,
    receipt: &mut AgentSessionReceipt,
) -> Result<()> {
    let mut client = BufReader::new(input);
    let mut upstream = BufReader::new(child_output);
    let mut pending_initialize_id: Option<Value> = None;
    loop {
        let Some(request) = read_bounded_json_line(&mut client, MAX_MCP_REQUEST_BYTES, "request")?
        else {
            break;
        };
        match evaluate_request(&request, &config.repository_root) {
            Ok(RequestDisposition::Respond(response)) => {
                receipt.observe_request(&request, false);
                receipt.persist(&config.cache_dir)?;
                write_json_line(output, &response)?;
            }
            Ok(RequestDisposition::Ignore) => {
                receipt.observe_request(&request, false);
                receipt.persist(&config.cache_dir)?;
            }
            Ok(RequestDisposition::Forward(forwarded)) => {
                receipt.observe_request(&request, false);
                receipt.persist(&config.cache_dir)?;
                let is_index = forwarded.pointer("/params/name").and_then(Value::as_str)
                    == Some("index_repository");
                let sentinel = if is_index {
                    config.ensure_repository_zero_write_precondition()?;
                    Some(RepositoryWriteSentinel::capture(&config.repository_root)?)
                } else {
                    None
                };
                let method = forwarded.get("method").and_then(Value::as_str);
                write_json_line(&mut child_input, &forwarded)?;
                if method == Some("initialize") {
                    if pending_initialize_id.is_some() {
                        bail!("duplicate MCP initialize request");
                    }
                    pending_initialize_id = forwarded.get("id").cloned();
                    write_json_line(output, &local_initialize_response(&forwarded)?)?;
                    continue;
                }
                if forwarded.get("id").is_none() {
                    continue;
                }
                if let Some(initialize_id) = pending_initialize_id.take() {
                    let initialize_response =
                        read_expected_response(&mut upstream, output, &initialize_id)?;
                    if initialize_response.get("error").is_some() {
                        bail!("upstream rejected the MCP initialize request");
                    }
                }
                let response = read_expected_response(
                    &mut upstream,
                    output,
                    forwarded
                        .get("id")
                        .context("forwarded request id is missing")?,
                )?;
                if response.get("id") != forwarded.get("id") {
                    bail!("upstream response identity mismatch");
                }
                if let Some(sentinel) = sentinel {
                    sentinel.verify_unchanged()?;
                }
                write_json_line(output, &response)?;
            }
            Err(error) => {
                receipt.observe_request(&request, true);
                receipt.persist(&config.cache_dir)?;
                if let Some(response) = jsonrpc_policy_error(request.get("id").cloned(), &error) {
                    write_json_line(output, &response)?;
                }
            }
        }
    }
    Ok(())
}

fn local_initialize_response(request: &Value) -> Result<Value> {
    let id = request
        .get("id")
        .cloned()
        .context("initialize id is missing")?;
    Ok(serde_json::json!({
        "jsonrpc":"2.0",
        "id":id,
        "result":{
            "protocolVersion":request.pointer("/params/protocolVersion")
                .and_then(Value::as_str).unwrap_or("2024-11-05"),
            "capabilities":{"tools":{"listChanged":false}},
            "serverInfo":{"name":crate::COMPONENT,"version":crate::VERSION},
            "instructions":"d-token constrained code graph: approved repository only; persistence and mutation tools are disabled."
        }
    }))
}

fn read_expected_response(
    upstream: &mut impl BufRead,
    output: &mut impl Write,
    expected_id: &Value,
) -> Result<Value> {
    loop {
        let candidate = read_bounded_json_line(upstream, MAX_MCP_RESPONSE_BYTES, "response")?
            .context("upstream closed before returning a response")?;
        match candidate.get("id") {
            None => write_json_line(output, &candidate)?,
            Some(id) if id == expected_id => return Ok(candidate),
            Some(_) => bail!("upstream response identity mismatch"),
        }
    }
}

fn read_bounded_json_line(
    reader: &mut impl BufRead,
    limit: usize,
    label: &str,
) -> Result<Option<Value>> {
    let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
    let read = reader
        .take(u64::try_from(limit).unwrap_or(u64::MAX) + 2)
        .read_until(b'\n', &mut bytes)
        .with_context(|| format!("failed to read MCP {label}"))?;
    if read == 0 {
        return Ok(None);
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    if bytes.len() > limit {
        bail!("MCP {label} exceeds the size limit");
    }
    let value =
        serde_json::from_slice(&bytes).with_context(|| format!("invalid MCP {label} JSON"))?;
    Ok(Some(value))
}

fn write_json_line(output: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bounded_reader_rejects_oversized_unterminated_frames() {
        let bytes = vec![b'x'; 128];
        let error = read_bounded_json_line(&mut Cursor::new(bytes), 32, "request")
            .expect_err("oversized frame must fail");
        assert!(error.to_string().contains("size limit"));
    }

    #[test]
    fn bounded_reader_accepts_one_json_line() {
        let mut input = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":1}\n".to_vec());
        let value = read_bounded_json_line(&mut input, 1024, "request")
            .unwrap()
            .unwrap();
        assert_eq!(value["id"], 1);
    }
}
