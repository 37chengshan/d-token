use anyhow::{bail, Context, Result};
use dtoken_preprocessor::{
    bounded_truncate_marker, filters, filters::CommandFilterConfig, managed_filter_config,
    verify_managed_hook_root, ManagedHookManifest,
    MAX_MANAGED_RTK_OUTPUT_BYTES, MAX_MANAGED_RTK_USAGE_ENTRIES,
    MAX_MANAGED_RTK_USAGE_RECORD_BYTES, MAX_MANAGED_RTK_USAGE_TOTAL_BYTES,
    MIN_MANAGED_RTK_OUTPUT_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const BRIDGE_COMPONENT: &str = "dtoken-agent-bridge";
pub const BRIDGE_PROTOCOL_VERSION: u32 = 1;
pub const BRIDGE_VERSION: &str = env!("CARGO_PKG_VERSION");

const LOOPBACK: &str = "127.0.0.1";
const PORT: u16 = 18_001;
const MAX_HOOK_INPUT: u64 = 64 * 1024;
const MAX_HTTP_RESPONSE: u64 = 256 * 1024;
const MAX_MCP_LINE: usize = 256 * 1024;
const USAGE_RECORD_SCHEMA_VERSION: u8 = 1;
const DEFAULT_RTK_OUTPUT_BYTES: usize = 64 * 1024;
const RTK_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const RTK_TIMEOUT_EXIT_CODE: i32 = 124;
/// Optional override for the wrapped-command timeout. Honored only in
/// debug/test builds, so an untrusted Agent environment can never silently
/// weaken the reviewed production timeout policy: release bridge binaries
/// always use the reviewed default. Bounded to 1..=3600 seconds; an invalid
/// value fails closed to the reviewed default.
const RTK_TIMEOUT_ENV: &str = "D_TOKEN_RTK_WRAP_TIMEOUT_SECONDS";

fn rtk_command_timeout() -> Duration {
    #[cfg(debug_assertions)]
    {
        if let Ok(value) = std::env::var(RTK_TIMEOUT_ENV) {
            if let Ok(seconds) = value.parse::<u64>() {
                if (1..=3600).contains(&seconds) {
                    return Duration::from_secs(seconds);
                }
            }
        }
    }
    RTK_COMMAND_TIMEOUT
}
static USAGE_RECORD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSummary {
    pub available: bool,
    pub healthy: bool,
    pub optical_enabled: Option<bool>,
    pub effective_mode: Option<String>,
    pub active_reachable: Option<bool>,
    pub active_execution_observed: Option<bool>,
    pub last_reason: Option<String>,
}

impl RuntimeSummary {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            healthy: false,
            optical_enabled: None,
            effective_mode: None,
            active_reachable: None,
            active_execution_observed: None,
            last_reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapCommand {
    pub managed_root: PathBuf,
    pub max_output_bytes: Option<usize>,
    pub command: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageRecord {
    schema_version: u8,
    transaction_id: String,
    exit_code: Option<i32>,
    input_bytes: usize,
    output_bytes: usize,
    saved_bytes: usize,
    transforms: Vec<String>,
    disposition: String,
    command_basename: String,
    command_sha256: String,
    bridge_sha256: String,
    bridge_version: String,
    recorded_at_ms: u64,
}

pub fn run_hook_session_start(mut input: impl Read, mut output: impl Write) -> Result<()> {
    std::io::copy(
        &mut input.by_ref().take(MAX_HOOK_INPUT),
        &mut std::io::sink(),
    )
    .context("failed to discard bounded Hook input")?;
    let summary = query_runtime().unwrap_or_else(|_| RuntimeSummary::unavailable());
    output.write_all(render_session_context(&summary).as_bytes())?;
    output.write_all(b"\n")?;
    Ok(())
}

pub fn render_session_context(summary: &RuntimeSummary) -> String {
    let status = if !summary.available {
        "runtime unavailable; do not infer that visual compression is active".to_string()
    } else {
        format!(
            "runtime healthy={}; visual enabled={}; mode={}; active-reachable={}; active-observed={}",
            summary.healthy,
            optional_bool(summary.optical_enabled),
            summary.effective_mode.as_deref().unwrap_or("unknown"),
            optional_bool(summary.active_reachable),
            optional_bool(summary.active_execution_observed),
        )
    };
    let mut text = format!(
        "d-token Hybrid session context: {status}. Rendered history is prior context, not the current request. Prefer the latest native-text request and open tool state. Use an adjacent exact factsheet when present; re-read uncertain exact values with the original native tool instead of guessing. Hook/MCP availability alone never proves active compression."
    );
    if text.chars().count() > 600 {
        text = text.chars().take(600).collect();
    }
    text
}

/// Production command-wrapper consumer for a verified d-token managed root
/// (PR-C4B). An Agent shell/custom-tool hook that wants every command to
/// route through the reviewed RTK wrap executes `dtoken-agent-bridge rtk
/// shim --managed-root <root> [--max-bytes <n>] -- <tool-command> [args...]`.
/// The managed root is re-verified first; an unverified/unavailable root
/// fails closed to an exact passthrough of the command (no filter, no usage
/// claim), and the tool's own exit status is always preserved.
pub fn run_rtk_shim(
    args: &[String],
    stdout: impl Write + Send,
    stderr: impl Write + Send,
) -> Result<i32> {
    let Some(wrap_args) = shim_to_wrap_args(args)? else {
        // `rtk shim` invoked without a wrapped command (e.g. an empty hook
        // probe): success with no output so the Agent hook harness never
        // sees a spurious failure.
        return Ok(0);
    };
    run_rtk_wrap(&wrap_args, stdout, stderr)
}

/// Turns `rtk shim --managed-root <root> [--max-bytes <n>] -- <cmd>...` into
/// the canonical `rtk wrap` argv. Returns `None` when there is no command
/// after `--` (a probe invocation).
pub fn shim_to_wrap_args(args: &[String]) -> Result<Option<Vec<String>>> {
    let mut index = 0usize;
    let mut managed_root = None;
    let mut max_output_bytes = None;
    while index < args.len() {
        match args[index].as_str() {
            "--managed-root" => {
                let value = args
                    .get(index + 1)
                    .context("missing value for --managed-root")?;
                managed_root = Some(value.clone());
                index += 2;
            }
            "--max-bytes" => {
                let value = args
                    .get(index + 1)
                    .context("missing value for --max-bytes")?;
                max_output_bytes = Some(value.clone());
                index += 2;
            }
            "--" => {
                let command = args[index + 1..].to_vec();
                if command.is_empty() {
                    return Ok(None);
                }
                let mut wrap = vec!["rtk".to_string(), "wrap".to_string()];
                if let Some(root) = managed_root {
                    wrap.push("--managed-root".to_string());
                    wrap.push(root);
                }
                if let Some(bytes) = max_output_bytes {
                    wrap.push("--max-bytes".to_string());
                    wrap.push(bytes);
                }
                wrap.push("--".to_string());
                wrap.extend(command);
                return Ok(Some(wrap));
            }
            flag => bail!("unsupported rtk shim argument: {flag}"),
        }
    }
    Ok(None)
}

pub fn run_rtk_wrap(
    args: &[String],
    mut stdout: impl Write + Send,
    mut stderr: impl Write + Send,
) -> Result<i32> {
    let command = parse_wrap_command(args)?;
    let verification = verify_managed_hook_root(&command.managed_root, true).ok();
    let Some(hook) = verification.as_ref() else {
        return run_wrapped_command_passthrough(&command.command, &mut stdout, &mut stderr);
    };
    let output_limit = reviewed_output_limit(&command, &hook.manifest)?;
    let output = run_wrapped_command_bounded(&command.command, output_limit, rtk_command_timeout())?;
    let exit_code = normalized_exit_code(&output.status, output.timed_out);
    // The d-token-owned structured command filter (PR-C4A) applies only when
    // the reviewed filter binding is intact. A tampered or unreadable binding
    // fails closed: the captured output is delivered verbatim and no usage
    // claim is recorded.
    let filter_config = match managed_filter_config(&hook.manifest) {
        Ok(config) => config,
        Err(error) => {
            let stdout_text = render_verbatim_capture(&output.stdout, output_limit);
            let stderr_text = render_verbatim_capture(&output.stderr, output_limit);
            stdout.write_all(stdout_text.as_bytes())?;
            stderr.write_all(stderr_text.as_bytes())?;
            stderr.write_all(
                format!("\n~ d-token filter binding unavailable: {error:#} ~\n").as_bytes(),
            )?;
            stdout.flush()?;
            stderr.flush()?;
            return Ok(exit_code);
        }
    };
    let stdout_filtered = filter_bounded_capture(
        &output.stdout,
        output_limit,
        &command.command,
        exit_code,
        &filter_config,
    );
    let stderr_filtered = filter_bounded_capture(
        &output.stderr,
        output_limit,
        &command.command,
        exit_code,
        &filter_config,
    );
    let mut transforms = stdout_filtered
        .transforms
        .iter()
        .map(|transform| format!("stdout:{transform}"))
        .collect::<Vec<_>>();
    transforms.extend(
        stderr_filtered
            .transforms
            .iter()
            .map(|transform| format!("stderr:{transform}")),
    );
    if output.stdout.truncated {
        transforms.push("stdout:bounded_capture".to_string());
    }
    if output.stderr.truncated {
        transforms.push("stderr:bounded_capture".to_string());
    }
    if output.timed_out {
        transforms.push("process:timeout".to_string());
    }
    transforms.sort();
    transforms.dedup();
    let input_bytes = output
        .stdout
        .total_bytes
        .saturating_add(output.stderr.total_bytes);
    let output_bytes = stdout_filtered
        .output_bytes
        .saturating_add(stderr_filtered.output_bytes);
    let evidence = UsageRecord {
        schema_version: USAGE_RECORD_SCHEMA_VERSION,
        transaction_id: hook.manifest.transaction_id.clone(),
        exit_code: Some(exit_code),
        input_bytes,
        output_bytes,
        saved_bytes: input_bytes.saturating_sub(output_bytes),
        transforms,
        disposition: "filtered_verified_hook".to_string(),
        command_basename: command_basename(&command.command),
        command_sha256: command_digest(&command.command),
        bridge_sha256: hook.manifest.bridge.sha256.clone(),
        bridge_version: hook.manifest.bridge.version.clone(),
        recorded_at_ms: now_ms(),
    };

    stdout.write_all(stdout_filtered.output.as_bytes())?;
    stderr.write_all(stderr_filtered.output.as_bytes())?;
    if write_usage_record(&hook.usage_dir, &hook.manifest, &evidence).is_err() {
        stderr.write_all(b"\n~ d-token usage receipt unavailable ~\n")?;
    }
    stdout.flush()?;
    stderr.flush()?;

    Ok(exit_code)
}

pub fn run_mcp(input: impl Read, mut output: impl Write) -> Result<()> {
    let mut reader = BufReader::new(input);
    loop {
        let mut line = Vec::with_capacity(MAX_MCP_LINE.min(8 * 1024));
        let read = reader
            .by_ref()
            .take(u64::try_from(MAX_MCP_LINE).unwrap_or(u64::MAX) + 2)
            .read_until(b'\n', &mut line)
            .context("failed to read MCP request")?;
        if read == 0 {
            break;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        if line.len() > MAX_MCP_LINE {
            bail!("MCP request exceeds the size limit");
        }
        let line = std::str::from_utf8(&line).context("MCP request is not valid UTF-8")?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(line).context("invalid MCP JSON-RPC request")?;
        if let Some(response) = handle_mcp_request(&request) {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

pub fn handle_mcp_request(request: &Value) -> Option<Value> {
    let id = request.get("id").cloned()?;
    let method = request.get("method").and_then(Value::as_str)?;
    let result = match method {
        "initialize" => json!({
            "protocolVersion": request.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "dtoken-hybrid", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Metadata-only diagnostics for d-token Hybrid. This server never returns raw prompts, source code, PNG bytes, credentials, or exact-context recovery data."
        }),
        "tools/list" => json!({"tools": [
            {
                "name": "hybrid_status",
                "description": "Return metadata-only d-token runtime and visual-compression status.",
                "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
            },
            {
                "name": "explain_last_decision",
                "description": "Return the latest metadata-only Hybrid decision reason when available.",
                "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        ]}),
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let summary = query_runtime().unwrap_or_else(|_| RuntimeSummary::unavailable());
            match name {
                "hybrid_status" => tool_text(json!({
                    "available": summary.available,
                    "healthy": summary.healthy,
                    "visual_enabled": summary.optical_enabled,
                    "effective_mode": summary.effective_mode,
                    "active_reachable": summary.active_reachable,
                    "active_execution_observed": summary.active_execution_observed,
                    "data_policy": "metadata_only"
                })),
                "explain_last_decision" => tool_text(json!({
                    "available": summary.available,
                    "reason": summary.last_reason,
                    "detail": if summary.last_reason.is_some() { "latest public decision reason" } else { "no public decision evidence available" },
                    "data_policy": "metadata_only"
                })),
                _ => {
                    return Some(
                        json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32602, "message": "unknown d-token Hybrid tool"}}),
                    )
                }
            }
        }
        _ => {
            return Some(
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}}),
            )
        }
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn tool_text(value: Value) -> Value {
    json!({"content": [{"type": "text", "text": serde_json::to_string(&value).unwrap_or_else(|_| "{\"available\":false}".to_string())}]})
}

fn parse_wrap_command(args: &[String]) -> Result<WrapCommand> {
    let mut managed_root = None;
    let mut max_output_bytes = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--managed-root" => {
                let value = args
                    .get(index + 1)
                    .context("missing value for --managed-root")?;
                managed_root = Some(PathBuf::from(value));
                index += 2;
            }
            "--max-bytes" => {
                let value = args
                    .get(index + 1)
                    .context("missing value for --max-bytes")?;
                max_output_bytes = Some(
                    value
                        .parse::<usize>()
                        .context("--max-bytes must be a positive integer")?,
                );
                if max_output_bytes.is_some_and(|limit| {
                    !(MIN_MANAGED_RTK_OUTPUT_BYTES..=MAX_MANAGED_RTK_OUTPUT_BYTES).contains(&limit)
                }) {
                    bail!("--max-bytes is outside the reviewed 1 KiB to 1 MiB range");
                }
                index += 2;
            }
            "--" => {
                let command = args[index + 1..].to_vec();
                if command.is_empty() {
                    bail!("rtk wrap requires a command after --");
                }
                return Ok(WrapCommand {
                    managed_root: managed_root.context("rtk wrap requires --managed-root")?,
                    max_output_bytes,
                    command,
                });
            }
            flag => bail!("unsupported rtk wrap argument: {flag}"),
        }
    }
    bail!("rtk wrap requires -- followed by a command")
}

fn reviewed_output_limit(command: &WrapCommand, manifest: &ManagedHookManifest) -> Result<usize> {
    let reviewed = manifest
        .max_output_bytes
        .unwrap_or(DEFAULT_RTK_OUTPUT_BYTES);
    if let Some(requested) = command.max_output_bytes {
        if requested != reviewed {
            bail!("--max-bytes does not match the reviewed managed RTK manifest");
        }
    }
    Ok(reviewed)
}

#[derive(Debug)]
struct BoundedCapture {
    bytes: Vec<u8>,
    total_bytes: usize,
    truncated: bool,
}

impl BoundedCapture {
    fn render(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

fn filter_bounded_capture(
    capture: &BoundedCapture,
    output_limit: usize,
    argv: &[String],
    exit_code: i32,
    config: &CommandFilterConfig,
) -> filters::CommandFilterResult {
    const CAPTURE_MARKER: &str = "~ d-token bounded capture truncated ~";
    let text = capture.render();
    if !capture.truncated {
        return filters::filter_command_output(argv, &text, Some(exit_code), config);
    }

    let separator = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let suffix_bytes = separator.len() + CAPTURE_MARKER.len();
    let body_cap = output_limit.saturating_sub(suffix_bytes);
    let mut filtered = filters::filter_command_output(argv, &text, Some(exit_code), config);
    if filtered.output.len() > body_cap {
        let (bounded, _) = bounded_truncate_marker(&filtered.output, body_cap);
        filtered.output = bounded;
    }
    filtered.output.push_str(separator);
    filtered.output.push_str(CAPTURE_MARKER);
    filtered.output_bytes = filtered.output.len();
    filtered.truncated = true;
    filtered
}

/// Renders a captured stream verbatim (with its truncation marker) when the
/// reviewed filter binding is unusable. Never claims savings.
fn render_verbatim_capture(capture: &BoundedCapture, output_limit: usize) -> String {
    const CAPTURE_MARKER: &str = "~ d-token bounded capture truncated ~";
    let text = capture.render();
    if !capture.truncated {
        return text;
    }
    let separator = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let suffix_bytes = separator.len() + CAPTURE_MARKER.len();
    let body_limit = output_limit.saturating_sub(suffix_bytes);
    let mut output = text;
    if output.len() > body_limit {
        let (bounded, _) = bounded_truncate_marker(&output, body_limit);
        output = bounded;
    }
    output.push_str(separator);
    output.push_str(CAPTURE_MARKER);
    output
}

#[derive(Debug)]
struct BoundedCommandOutput {
    status: ExitStatus,
    stdout: BoundedCapture,
    stderr: BoundedCapture,
    timed_out: bool,
}

fn spawn_wrapped_command(command: &[String]) -> Result<Child> {
    let program = command.first().context("wrapped command is empty")?;
    let mut builder = Command::new(program);
    builder
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // The wrapped command runs in its own process group so a timeout can
    // terminate the whole tree (e.g. a shell whose children still hold the
    // output pipes) instead of leaving the capture threads blocked.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        builder.process_group(0);
    }
    builder
        .spawn()
        .with_context(|| format!("failed to spawn wrapped command {}", program))
}

/// Terminates the wrapped command and its process group/tree. On Unix the
/// child was placed in its own process group, so killing the group reaps
/// grandchildren that would otherwise keep the capture pipes open. On
/// Windows the process tree is terminated with `taskkill /T`.
fn terminate_wrapped_command(child: &mut Child) -> Result<()> {
    #[cfg(unix)]
    {
        let pid = i32::try_from(child.id()).context("wrapped command pid overflow")?;
        // SIGKILL the whole group; fall back to the direct child so a
        // missing group never leaks the process.
        let group_result = unsafe { libc::kill(-pid, libc::SIGKILL) };
        if group_result != 0 {
            child.kill().context("terminate timed-out wrapped command")?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .status();
        child.kill().context("terminate timed-out wrapped command")?;
    }
    Ok(())
}

fn wait_for_wrapped_command(child: &mut Child, timeout: Duration) -> Result<(ExitStatus, bool)> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("inspect wrapped command state")? {
            return Ok((status, false));
        }
        if started.elapsed() >= timeout {
            terminate_wrapped_command(child)?;
            return child
                .wait()
                .map(|status| (status, true))
                .context("reap timed-out wrapped command");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn normalized_exit_code(status: &ExitStatus, timed_out: bool) -> i32 {
    if timed_out {
        return RTK_TIMEOUT_EXIT_CODE;
    }
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128_i32.saturating_add(signal);
        }
    }
    1
}

fn run_wrapped_command_passthrough(
    command: &[String],
    stdout: &mut (impl Write + Send),
    stderr: &mut (impl Write + Send),
) -> Result<i32> {
    let mut child = spawn_wrapped_command(command)?;
    let mut child_stdout = child
        .stdout
        .take()
        .context("wrapped command stdout pipe is unavailable")?;
    let mut child_stderr = child
        .stderr
        .take()
        .context("wrapped command stderr pipe is unavailable")?;
    let (status_result, stdout_result, stderr_result) = thread::scope(|scope| {
        let stdout_copy = scope.spawn(|| std::io::copy(&mut child_stdout, stdout));
        let stderr_copy = scope.spawn(|| std::io::copy(&mut child_stderr, stderr));
        let status = wait_for_wrapped_command(&mut child, rtk_command_timeout());
        (status, stdout_copy.join(), stderr_copy.join())
    });
    let (status, timed_out) = status_result?;
    stdout_result
        .map_err(|_| anyhow::anyhow!("wrapped command stdout worker panicked"))?
        .context("stream wrapped command stdout")?;
    stderr_result
        .map_err(|_| anyhow::anyhow!("wrapped command stderr worker panicked"))?
        .context("stream wrapped command stderr")?;
    stdout.flush()?;
    stderr.flush()?;
    Ok(normalized_exit_code(&status, timed_out))
}

fn run_wrapped_command_bounded(
    command: &[String],
    capture_limit: usize,
    timeout: Duration,
) -> Result<BoundedCommandOutput> {
    let mut child = spawn_wrapped_command(command)?;
    let mut child_stdout = child
        .stdout
        .take()
        .context("wrapped command stdout pipe is unavailable")?;
    let mut child_stderr = child
        .stderr
        .take()
        .context("wrapped command stderr pipe is unavailable")?;
    let (status_result, stdout_result, stderr_result) = thread::scope(|scope| {
        let stdout_capture = scope.spawn(|| read_bounded_capture(&mut child_stdout, capture_limit));
        let stderr_capture = scope.spawn(|| read_bounded_capture(&mut child_stderr, capture_limit));
        let status = wait_for_wrapped_command(&mut child, timeout);
        (status, stdout_capture.join(), stderr_capture.join())
    });
    let (status, timed_out) = status_result?;
    let stdout =
        stdout_result.map_err(|_| anyhow::anyhow!("wrapped command stdout worker panicked"))??;
    let stderr =
        stderr_result.map_err(|_| anyhow::anyhow!("wrapped command stderr worker panicked"))??;
    Ok(BoundedCommandOutput {
        status,
        stdout,
        stderr,
        timed_out,
    })
}

fn read_bounded_capture(reader: &mut impl Read, limit: usize) -> std::io::Result<BoundedCapture> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut total_bytes = 0usize;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read);
        let remaining = limit.saturating_sub(bytes.len());
        if remaining > 0 {
            bytes.extend_from_slice(&buffer[..read.min(remaining)]);
        }
    }
    Ok(BoundedCapture {
        truncated: total_bytes > bytes.len(),
        bytes,
        total_bytes,
    })
}

fn write_usage_record(
    dir: &Path,
    manifest: &ManagedHookManifest,
    record: &UsageRecord,
) -> Result<()> {
    let metadata = fs::symlink_metadata(dir)
        .with_context(|| format!("managed usage directory is unavailable: {}", dir.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("managed usage directory must be a real directory");
    }
    validate_private_directory(dir, "managed usage directory")?;
    let bytes = serde_json::to_vec_pretty(record).context("serialize usage record")?;
    if bytes.len() > MAX_MANAGED_RTK_USAGE_RECORD_BYTES {
        bail!("managed usage record exceeds its size limit");
    }
    prune_usage_records(dir, manifest, bytes.len())?;
    let sequence = USAGE_RECORD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stem = format!(
        "{}-{}-{sequence}",
        record.recorded_at_ms,
        std::process::id()
    );
    let temp = dir.join(format!("{stem}.tmp"));
    let path = dir.join(format!("{stem}.json"));
    fs::write(&temp, bytes).with_context(|| format!("failed to write {}", temp.display()))?;
    if let Err(error) = fs::rename(&temp, &path) {
        let _ = fs::remove_file(&temp);
        return Err(error).with_context(|| format!("failed to replace {}", path.display()));
    }
    Ok(())
}

#[derive(Debug)]
struct StoredUsageRecord {
    path: PathBuf,
    size: usize,
    record: UsageRecord,
}

fn prune_usage_records(
    dir: &Path,
    manifest: &ManagedHookManifest,
    incoming_bytes: usize,
) -> Result<()> {
    if incoming_bytes > MAX_MANAGED_RTK_USAGE_TOTAL_BYTES {
        bail!("managed usage record exceeds the aggregate size limit");
    }
    let mut stored = fs::read_dir(dir)
        .context("managed usage directory cannot be read")?
        .map(|entry| entry.context("managed usage entry cannot be read"))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .map(|entry| load_owned_usage_record(entry.path(), manifest))
        .collect::<Result<Vec<_>>>()?;
    stored.sort_by(|left, right| {
        left.record
            .recorded_at_ms
            .cmp(&right.record.recorded_at_ms)
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut total_bytes = stored.iter().try_fold(0usize, |total, entry| {
        total
            .checked_add(entry.size)
            .context("managed usage aggregate size overflow")
    })?;
    while stored.len() >= MAX_MANAGED_RTK_USAGE_ENTRIES
        || total_bytes
            .checked_add(incoming_bytes)
            .is_none_or(|total| total > MAX_MANAGED_RTK_USAGE_TOTAL_BYTES)
    {
        let oldest = stored
            .first()
            .context("managed usage capacity cannot be reclaimed")?;
        match fs::remove_file(&oldest.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to prune managed usage record {}",
                        oldest.path.display()
                    )
                })
            }
        }
        total_bytes = total_bytes.saturating_sub(oldest.size);
        stored.remove(0);
    }
    Ok(())
}

fn load_owned_usage_record(
    path: PathBuf,
    manifest: &ManagedHookManifest,
) -> Result<StoredUsageRecord> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("managed usage filename is not valid UTF-8")?;
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("managed usage metadata is unavailable: {}", path.display()))?;
    if name.len() > 128
        || !name.ends_with(".json")
        || metadata.file_type().is_symlink()
        || !metadata.is_file()
    {
        bail!("managed usage directory contains an unowned entry");
    }
    let size =
        usize::try_from(metadata.len()).context("managed usage record size is not supported")?;
    if size > MAX_MANAGED_RTK_USAGE_RECORD_BYTES {
        bail!("managed usage record exceeds its size limit");
    }
    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read managed usage record {}", path.display()))?;
    let record: UsageRecord =
        serde_json::from_slice(&bytes).context("managed usage record is invalid")?;
    validate_owned_usage_record(&record, manifest)?;
    Ok(StoredUsageRecord { path, size, record })
}

fn validate_owned_usage_record(record: &UsageRecord, manifest: &ManagedHookManifest) -> Result<()> {
    let expected_saved = record
        .input_bytes
        .checked_sub(record.output_bytes)
        .context("managed usage byte accounting is invalid")?;
    if record.schema_version != USAGE_RECORD_SCHEMA_VERSION
        || record.transaction_id != manifest.transaction_id
        || record.saved_bytes != expected_saved
        || record.disposition != "filtered_verified_hook"
        || record.bridge_sha256 != manifest.bridge.sha256
        || record.bridge_version != manifest.bridge.version
        || !is_sha256_hex(&record.command_sha256)
        || !is_safe_metadata_text(&record.command_basename, 128)
        || record.transforms.len() > 16
        || !record
            .transforms
            .iter()
            .all(|transform| is_allowed_usage_transform(transform))
    {
        bail!("managed usage record does not match reviewed ownership evidence");
    }
    Ok(())
}

fn command_basename(command: &[String]) -> String {
    let raw = command
        .first()
        .and_then(|program| Path::new(program).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let safe = raw
        .bytes()
        .take(128)
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
                char::from(byte)
            } else {
                '_'
            }
        })
        .collect::<String>();
    if safe.is_empty() {
        "unknown".to_string()
    } else {
        safe
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_safe_metadata_text(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

/// Rejects a managed usage directory exposed to group/other users. Usage
/// receipts are metadata-only (no argv, output, or credentials), but they
/// still describe the caller's command workload and must stay private.
/// No-op on platforms without POSIX permission bits.
fn validate_private_directory(path: &Path, label: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .with_context(|| format!("{label} metadata is unavailable: {}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            bail!(
                "{label} is not private (mode {mode:o}); d-token usage receipts must not be exposed to group or other users"
            );
        }
    }
    Ok(())
}

fn is_allowed_usage_transform(value: &str) -> bool {
    if matches!(value, "stdout:bounded_capture" | "stderr:bounded_capture" | "process:timeout") {
        return true;
    }
    let Some((stream, transform)) = value.split_once(':') else {
        return false;
    };
    if !matches!(stream, "stdout" | "stderr") {
        return false;
    }
    is_known_filter_transform(transform)
}

/// Accepts every transform the d-token-owned structured filters (PR-C4A) and
/// the reviewed builtin can emit.
fn is_known_filter_transform(transform: &str) -> bool {
    if matches!(transform, "unknown-family" | "ansi_strip" | "blank_and_repeat_fold" | "bounded_truncate") {
        return true;
    }
    let Some((family, action)) = transform.split_once(':') else {
        return false;
    };
    matches!(family, "git" | "cargo" | "npm" | "pytest" | "json")
        && matches!(
            action,
            "failure_preserve"
                | "secret_preserve"
                | "error_preserve"
                | "disabled"
                | "ansi_strip"
                | "fold_repeated_lines"
                | "bounded_truncate"
                | "negative_benefit"
                | "no_change"
                | "unparsed_preserve"
                | "secret_keys_preserve"
                | "oversize_preserve"
                | "minify"
        )
}

fn command_digest(command: &[String]) -> String {
    let mut digest = Sha256::new();
    for part in command {
        digest.update(part.as_bytes());
        digest.update([0u8]);
    }
    format!("{:x}", digest.finalize())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

pub fn query_runtime() -> Result<RuntimeSummary> {
    let health = http_get_json("/health").ok();
    let status = http_get_json("/compression/optical/status").ok();
    let events = http_get_json("/compression/optical/events/recent").ok();
    if health.is_none() && status.is_none() {
        bail!("d-token runtime unavailable");
    }
    let health_value = health.as_ref().context("d-token health unavailable")?;
    if health_value.get("component").and_then(Value::as_str) != Some("dtoken-core-api")
        || health_value.get("runtime_identity").and_then(Value::as_str)
            != Some("dtoken-runtime-local")
        || health_value
            .get("version")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    {
        bail!("d-token loopback identity mismatch");
    }
    Ok(RuntimeSummary {
        available: true,
        healthy: health
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            .is_some_and(|state| matches!(state, "healthy" | "ready")),
        optical_enabled: status
            .as_ref()
            .and_then(|value| value.get("enabled"))
            .and_then(Value::as_bool),
        effective_mode: status
            .as_ref()
            .and_then(|value| {
                value
                    .get("effective_mode")
                    .or_else(|| value.get("default_mode"))
            })
            .and_then(Value::as_str)
            .map(str::to_string),
        active_reachable: status
            .as_ref()
            .and_then(|value| {
                value
                    .get("active_reachable")
                    .or_else(|| value.get("allow_active"))
            })
            .and_then(Value::as_bool),
        active_execution_observed: status
            .as_ref()
            .and_then(|value| value.get("active_execution_observed"))
            .and_then(Value::as_bool),
        last_reason: latest_public_reason(events.as_ref()),
    })
}

fn latest_public_reason(events: Option<&Value>) -> Option<String> {
    let array = events?
        .as_array()
        .or_else(|| events?.get("events")?.as_array())?;
    array
        .first()?
        .pointer("/metadata/reason_code")
        .and_then(Value::as_str)
        .map(|value| value.chars().take(120).collect())
}

fn http_get_json(path: &str) -> Result<Value> {
    if !path.starts_with('/') || path.contains("..") {
        bail!("invalid loopback path");
    }
    let address = SocketAddr::from(([127, 0, 0, 1], PORT));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(120))?;
    stream.set_read_timeout(Some(Duration::from_millis(300)))?;
    stream.set_write_timeout(Some(Duration::from_millis(120)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {LOOPBACK}:{PORT}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    )?;
    let mut response = Vec::new();
    stream.take(MAX_HTTP_RESPONSE).read_to_end(&mut response)?;
    let marker = b"\r\n\r\n";
    let split = response
        .windows(marker.len())
        .position(|window| window == marker)
        .context("invalid loopback HTTP response")?;
    let head = std::str::from_utf8(&response[..split])?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        bail!("loopback HTTP request failed");
    }
    Ok(serde_json::from_slice(&response[split + marker.len()..])?)
}

fn optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_context_is_bounded_and_truthful_when_runtime_is_missing() {
        let text = render_session_context(&RuntimeSummary::unavailable());
        assert!(text.contains("runtime unavailable"));
        assert!(text.contains("never proves active compression"));
        assert!(text.chars().count() <= 600);
        assert!(!text.contains("prompt"));
    }

    #[test]
    fn mcp_lists_only_two_metadata_tools() {
        let response =
            handle_mcp_request(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
                .unwrap();
        let tools = response
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "hybrid_status");
        assert_eq!(tools[1]["name"], "explain_last_decision");
        assert!(!response.to_string().contains("recover"));
    }

    #[test]
    fn notification_produces_no_response() {
        assert!(handle_mcp_request(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none());
    }

    #[test]
    fn normalized_exit_code_reports_timeout_as_124() {
        let success = std::process::Command::new("true")
            .status()
            .expect("true status");
        assert_eq!(normalized_exit_code(&success, false), 0);
        assert_eq!(normalized_exit_code(&success, true), RTK_TIMEOUT_EXIT_CODE);
        assert_eq!(RTK_TIMEOUT_EXIT_CODE, 124);
    }

    #[test]
    fn usage_records_cannot_carry_argv_or_raw_output_fields() {
        let manifest = ManagedHookManifest {
            schema_version: 1,
            transaction_id: "rtk-argv-reject".to_string(),
            platform: dtoken_types::agent::HostPlatform::Macos,
            host_config_fingerprint: "a".repeat(64),
            managed_root: "/tmp/managed-rtk".to_string(),
            bridge_path: "/tmp/bridge".to_string(),
            bridge: dtoken_preprocessor::ManagedHookBridgeIdentity {
                component: BRIDGE_COMPONENT.to_string(),
                protocol_version: 1,
                version: "0.2.0-beta.1".to_string(),
                arch: "fixture".to_string(),
                os: "fixture".to_string(),
                sha256: "b".repeat(64),
            },
            max_output_bytes: Some(MIN_MANAGED_RTK_OUTPUT_BYTES),
            filter_dsl_sha256: None,            usage_dir: "usage".to_string(),
        };
        let secret_argv = serde_json::json!({
            "schema_version": USAGE_RECORD_SCHEMA_VERSION,
            "transaction_id": "rtk-argv-reject",
            "exit_code": 0,
            "input_bytes": 10,
            "output_bytes": 5,
            "saved_bytes": 5,
            "transforms": [],
            "disposition": "filtered_verified_hook",
            "command_basename": "cargo",
            "command_sha256": "c".repeat(64),
            "bridge_sha256": manifest.bridge.sha256,
            "bridge_version": manifest.bridge.version,
            "recorded_at_ms": 1,
            "argv": ["cargo", "build", "--release"],
            "raw_output": "SECRET=must-not-persist"
        });
        let parsed: Result<UsageRecord, _> = serde_json::from_value(secret_argv);
        assert!(
            parsed.is_err(),
            "an argv/raw-output carrying record must never deserialize"
        );
    }

    #[cfg(unix)]
    #[test]
    fn usage_writer_rejects_an_exposed_usage_directory() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("usage tempdir");
        let usage_dir = root.path().join("usage");
        std::fs::create_dir_all(&usage_dir).expect("usage directory");
        std::fs::set_permissions(&usage_dir, std::fs::Permissions::from_mode(0o755))
            .expect("expose usage directory");
        let manifest = ManagedHookManifest {
            schema_version: 1,
            transaction_id: "rtk-acl".to_string(),
            platform: dtoken_types::agent::HostPlatform::Macos,
            host_config_fingerprint: "a".repeat(64),
            managed_root: root.path().display().to_string(),
            bridge_path: root.path().join("bridge").display().to_string(),
            bridge: dtoken_preprocessor::ManagedHookBridgeIdentity {
                component: BRIDGE_COMPONENT.to_string(),
                protocol_version: 1,
                version: "0.2.0-beta.1".to_string(),
                arch: "fixture".to_string(),
                os: "fixture".to_string(),
                sha256: "b".repeat(64),
            },
            max_output_bytes: Some(MIN_MANAGED_RTK_OUTPUT_BYTES),
            filter_dsl_sha256: None,            usage_dir: "usage".to_string(),
        };
        let record = UsageRecord {
            schema_version: USAGE_RECORD_SCHEMA_VERSION,
            transaction_id: manifest.transaction_id.clone(),
            exit_code: Some(0),
            input_bytes: 10,
            output_bytes: 5,
            saved_bytes: 5,
            transforms: vec![],
            disposition: "filtered_verified_hook".to_string(),
            command_basename: "cargo".to_string(),
            command_sha256: "c".repeat(64),
            bridge_sha256: manifest.bridge.sha256.clone(),
            bridge_version: manifest.bridge.version.clone(),
            recorded_at_ms: 1,
        };
        let error =
            write_usage_record(&usage_dir, &manifest, &record).expect_err("exposed usage dir");
        assert!(error.to_string().contains("not private"));
        assert!(std::fs::read_dir(&usage_dir)
            .expect("usage dir")
            .next()
            .is_none());
    }

    #[test]
    fn hook_and_mcp_never_echo_untrusted_input() {
        let secret = b"{\"transcript\":\"DO_NOT_ECHO_secret_value\"}";
        let mut hook_output = Vec::new();
        run_hook_session_start(&secret[..], &mut hook_output).unwrap();
        assert!(!String::from_utf8(hook_output)
            .unwrap()
            .contains("DO_NOT_ECHO"));

        let response = handle_mcp_request(&json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {"name": "hybrid_status", "arguments": {"raw": "DO_NOT_ECHO"}}
        }))
        .unwrap();
        assert!(!response.to_string().contains("DO_NOT_ECHO"));
    }

    #[test]
    fn mcp_rejects_an_oversized_unterminated_line_with_a_bounded_read() {
        let input = vec![b'x'; MAX_MCP_LINE * 4];
        let error = run_mcp(input.as_slice(), Vec::new()).expect_err("oversized MCP line");
        assert!(error.to_string().contains("size limit"));
    }

    #[test]
    fn wrap_parser_rejects_limits_outside_the_reviewed_range() {
        let below = parse_wrap_command(&[
            "--max-bytes".to_string(),
            (MIN_MANAGED_RTK_OUTPUT_BYTES - 1).to_string(),
            "--".to_string(),
            "echo".to_string(),
        ])
        .expect_err("sub-1KiB limit");
        assert!(below.to_string().contains("reviewed 1 KiB to 1 MiB range"));

        let above = parse_wrap_command(&[
            "--max-bytes".to_string(),
            (MAX_MANAGED_RTK_OUTPUT_BYTES + 1).to_string(),
            "--".to_string(),
            "echo".to_string(),
        ])
        .expect_err("over-1MiB limit");
        assert!(above.to_string().contains("reviewed 1 KiB to 1 MiB range"));
    }

    #[test]
    fn command_basename_is_safe_for_usage_metadata() {
        assert_eq!(
            command_basename(&["/tmp/agent tool/cli".to_string()]),
            "cli"
        );
        assert_eq!(
            command_basename(&["/tmp/agent tool/cli name".to_string()]),
            "cli_name"
        );
        assert_eq!(command_basename(&[]), "unknown");
    }

    #[test]
    fn usage_writer_prunes_only_owned_records_before_capacity_is_exceeded() {
        let root = tempfile::tempdir().expect("usage tempdir");
        let usage_dir = root.path().join("usage");
        std::fs::create_dir_all(&usage_dir).expect("usage directory");
        // The preprocessor creates the managed usage directory private; the
        // writer now rejects exposed directories, so mirror that here.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&usage_dir, std::fs::Permissions::from_mode(0o700))
                .expect("private usage directory");
        }
        let manifest = ManagedHookManifest {
            schema_version: 1,
            transaction_id: "rtk-usage-prune".to_string(),
            platform: dtoken_types::agent::HostPlatform::Macos,
            host_config_fingerprint: "a".repeat(64),
            managed_root: root.path().display().to_string(),
            bridge_path: root.path().join("bridge").display().to_string(),
            bridge: dtoken_preprocessor::ManagedHookBridgeIdentity {
                component: BRIDGE_COMPONENT.to_string(),
                protocol_version: 1,
                version: "0.2.0-beta.1".to_string(),
                arch: "fixture".to_string(),
                os: "fixture".to_string(),
                sha256: "b".repeat(64),
            },
            max_output_bytes: Some(MIN_MANAGED_RTK_OUTPUT_BYTES),
            filter_dsl_sha256: None,            usage_dir: "usage".to_string(),
        };
        for index in 0..MAX_MANAGED_RTK_USAGE_ENTRIES {
            let record = UsageRecord {
                schema_version: USAGE_RECORD_SCHEMA_VERSION,
                transaction_id: manifest.transaction_id.clone(),
                exit_code: Some(0),
                input_bytes: 10,
                output_bytes: 5,
                saved_bytes: 5,
                transforms: vec![],
                disposition: "filtered_verified_hook".to_string(),
                command_basename: "cargo".to_string(),
                command_sha256: "c".repeat(64),
                bridge_sha256: manifest.bridge.sha256.clone(),
                bridge_version: manifest.bridge.version.clone(),
                recorded_at_ms: index as u64,
            };
            std::fs::write(
                usage_dir.join(format!("existing-{index}.json")),
                serde_json::to_vec(&record).expect("record JSON"),
            )
            .expect("existing usage record");
        }

        let incoming = UsageRecord {
            schema_version: USAGE_RECORD_SCHEMA_VERSION,
            transaction_id: manifest.transaction_id.clone(),
            exit_code: Some(0),
            input_bytes: 20,
            output_bytes: 10,
            saved_bytes: 10,
            transforms: vec!["stdout:bounded_capture".to_string()],
            disposition: "filtered_verified_hook".to_string(),
            command_basename: "cargo".to_string(),
            command_sha256: "d".repeat(64),
            bridge_sha256: manifest.bridge.sha256.clone(),
            bridge_version: manifest.bridge.version.clone(),
            recorded_at_ms: 10_000,
        };
        write_usage_record(&usage_dir, &manifest, &incoming).expect("prune and write");

        let entries = std::fs::read_dir(&usage_dir)
            .expect("read usage directory")
            .count();
        assert_eq!(entries, MAX_MANAGED_RTK_USAGE_ENTRIES);
        assert!(!usage_dir.join("existing-0.json").exists());
    }
}
