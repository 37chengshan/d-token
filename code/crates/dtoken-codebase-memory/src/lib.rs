mod proxy;
mod ui_host;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use thiserror::Error;

pub use proxy::{run_stdio_proxy, scope_ref, AgentSessionReceipt, AGENT_SESSION_RECEIPT_FILE};
pub use ui_host::{run_index_controlled, run_ui_host, IndexReceipt, UiHostReady};

pub const COMPONENT: &str = "dtoken-codebase-memory-launcher";
pub const POLICY_VERSION: u32 = 1;
pub const PROTOCOL_VERSION: u32 = 1;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Deterministic `lipo -create` result from the two verified v0.9.0 macOS
/// release binaries. It is a d-token-derived universal artifact, not an
/// independently upstream-signed release asset.
pub const MACOS_UNIVERSAL_BINARY_SHA256: &str =
    "dbd7473051e8459f0b16e83eb91ed6d2ef90e31ed238c10067eb952ab985759f";
pub const MAX_MCP_REQUEST_BYTES: usize = 512 * 1024;
pub const MAX_MCP_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_CACHE_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_CACHE_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const MAX_CACHE_ENTRIES: u64 = 1_000_000;
const MAX_HOOK_INPUT_BYTES: u64 = 64 * 1024;
const MAX_HOOK_OUTPUT_CHARS: usize = 700;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSpec {
    pub os: &'static str,
    pub arch: &'static str,
    pub upstream_version: &'static str,
    pub upstream_commit: &'static str,
    pub archive_name: &'static str,
    pub archive_sha256: &'static str,
    pub binary_sha256: &'static str,
}

pub const ARTIFACT_SPECS: &[ArtifactSpec] = &[
    ArtifactSpec {
        os: "macos",
        arch: "aarch64",
        upstream_version: "0.9.0",
        upstream_commit: "b637e3330c96cfe452da623db068c241aaa3ec01",
        archive_name: "codebase-memory-mcp-darwin-arm64.tar.gz",
        archive_sha256: "faa02f0404230c451a9812230394481948f80183801fa5bf67044b41c2f25ed4",
        binary_sha256: "d9fbdd7d8570a77b2fb32453e00bd52a02627281309cd56003a4eccfcfe878d6",
    },
    ArtifactSpec {
        os: "macos",
        arch: "x86_64",
        upstream_version: "0.9.0",
        upstream_commit: "b637e3330c96cfe452da623db068c241aaa3ec01",
        archive_name: "codebase-memory-mcp-darwin-amd64.tar.gz",
        archive_sha256: "6af3d02a27f589901fa763d3971089337bc8c9838bbed5d0cf543ca9f1a9e543",
        binary_sha256: "04ee3048810c19099502adc8bb83039423f02f2553d17677892a7f03b924e01f",
    },
    ArtifactSpec {
        os: "windows",
        arch: "x86_64",
        upstream_version: "0.9.0",
        upstream_commit: "b637e3330c96cfe452da623db068c241aaa3ec01",
        archive_name: "codebase-memory-mcp-windows-amd64.zip",
        archive_sha256: "92f96896f952e539f0d6cb34d7892a25064b677ccbf808b8f8310ad897e86f2c",
        binary_sha256: "9a205fa5ae759fbc866bfe1554f0c05a303be9ae6e0a00f94d875dc0c25e0680",
    },
    ArtifactSpec {
        os: "windows",
        arch: "aarch64",
        upstream_version: "0.9.0",
        upstream_commit: "b637e3330c96cfe452da623db068c241aaa3ec01",
        archive_name: "codebase-memory-mcp-windows-arm64.zip",
        archive_sha256: "63994fcfd15bf5e3f03cbf368cce86261713c7d7802e31469ae81a3939e4fae6",
        binary_sha256: "a299150de19bb4da9a880a6139757076bd4139b2f6a7e7cf31bdd431d53aed9c",
    },
];

pub fn artifact_spec_for_current_platform() -> Result<&'static ArtifactSpec> {
    ARTIFACT_SPECS
        .iter()
        .find(|spec| spec.os == std::env::consts::OS && spec.arch == std::env::consts::ARCH)
        .context("no verified codebase-memory artifact is registered for this platform")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedUpstreamArtifact {
    pub path: PathBuf,
    pub sha256: String,
}

/// Verifies the packaged upstream executable without creating cache state or
/// inspecting a repository. Desktop status checks use this pure boundary.
pub fn verify_upstream_artifact(path: impl AsRef<Path>) -> Result<PathBuf> {
    Ok(verify_upstream_artifact_with_digest(path)?.path)
}

/// Verifies the upstream executable and returns the digest produced by that
/// same read. Callers must not hash the large artifact a second time merely to
/// retain verification evidence.
pub fn verify_upstream_artifact_with_digest(
    path: impl AsRef<Path>,
) -> Result<VerifiedUpstreamArtifact> {
    let upstream_path = canonical_file(path.as_ref(), "upstream executable")?;
    let expected = artifact_spec_for_current_platform()?;
    let actual_sha256 = sha256_file(&upstream_path)?;
    let verified_universal =
        cfg!(target_os = "macos") && actual_sha256 == MACOS_UNIVERSAL_BINARY_SHA256;
    if actual_sha256 != expected.binary_sha256 && !verified_universal {
        bail!("upstream executable SHA-256 does not match the pinned artifact manifest");
    }
    Ok(VerifiedUpstreamArtifact {
        path: upstream_path,
        sha256: actual_sha256,
    })
}

pub fn run_hook_session_start(mut input: impl Read, mut output: impl Write) -> Result<()> {
    std::io::copy(
        &mut input.by_ref().take(MAX_HOOK_INPUT_BYTES),
        &mut std::io::sink(),
    )
    .context("failed to discard bounded codebase-memory Hook input")?;
    let mut message = "d-token Codebase Memory is an optional, repository-bound local code graph. Prefer its read-only search_graph, trace_path, get_code_snippet, query_graph, and get_architecture tools for code discovery when they are available. index_repository is constrained to the approved canonical root with persistence disabled. Never infer that the package is installed, indexed, or active from this Hook alone; inspect the five independent d-token status dimensions. Mutation, ADR, export, cross-repository, updater, and native upstream UI capabilities are not authorized.".to_string();
    if message.chars().count() > MAX_HOOK_OUTPUT_CHARS {
        message = message.chars().take(MAX_HOOK_OUTPUT_CHARS).collect();
    }
    output.write_all(message.as_bytes())?;
    output.write_all(b"\n")?;
    Ok(())
}

const PROTECTED_REPOSITORY_ARTIFACTS: &[&str] = &[
    ".codebase-memory/graph.db.zst",
    ".codebase-memory/artifact.json",
];

const SAFE_READ_TOOLS: &[&str] = &[
    "search_graph",
    "query_graph",
    "trace_path",
    "get_code_snippet",
    "get_graph_schema",
    "get_architecture",
    "search_code",
    "list_projects",
    "index_status",
    "detect_changes",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DimensionState {
    Unavailable,
    Untrusted,
    NotConfigured,
    Stopped,
    Starting,
    Ready,
    Busy,
    Partial,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatusDimension {
    pub state: DimensionState,
    pub reason_code: String,
    pub detail: String,
    pub checked_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodebaseMemoryStatus {
    pub schema_version: u32,
    pub package: StatusDimension,
    pub integration: StatusDimension,
    pub agent_session: StatusDimension,
    pub ui_session: StatusDimension,
    pub index: StatusDimension,
}

impl CodebaseMemoryStatus {
    pub fn unavailable(checked_at_ms: u64) -> Self {
        Self {
            schema_version: 1,
            package: dimension(
                DimensionState::Unavailable,
                "package_unavailable",
                "No verified bundled codebase-memory package is available.",
                checked_at_ms,
            ),
            integration: dimension(
                DimensionState::NotConfigured,
                "integration_not_configured",
                "Agent MCP integration has not been configured.",
                checked_at_ms,
            ),
            agent_session: dimension(
                DimensionState::Unknown,
                "agent_session_unobserved",
                "A new Agent session is required after configuration.",
                checked_at_ms,
            ),
            ui_session: dimension(
                DimensionState::Stopped,
                "ui_session_stopped",
                "The d-token graph page is not running.",
                checked_at_ms,
            ),
            index: dimension(
                DimensionState::Unavailable,
                "index_unavailable",
                "Index state is unavailable until the package is verified.",
                checked_at_ms,
            ),
        }
    }
}

fn dimension(
    state: DimensionState,
    reason_code: &str,
    detail: &str,
    checked_at_ms: u64,
) -> StatusDimension {
    StatusDimension {
        state,
        reason_code: reason_code.to_string(),
        detail: detail.to_string(),
        checked_at_ms,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetadataReceipt {
    pub schema_version: u32,
    pub operation: String,
    pub outcome: String,
    pub reason_code: String,
    pub policy_version: u32,
    pub occurred_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CacheInspection {
    pub bytes: u64,
    pub entries: u64,
    pub newest_modified_at_ms: Option<u64>,
    pub stale: bool,
}

pub fn inspect_cache(cache_dir: impl AsRef<Path>, now_ms: u64) -> Result<CacheInspection> {
    let cache_dir = fs::canonicalize(cache_dir.as_ref()).with_context(|| {
        format!(
            "failed to resolve isolated cache {}",
            cache_dir.as_ref().display()
        )
    })?;
    if !cache_dir.is_dir() {
        bail!("isolated cache is not a directory");
    }
    let mut stack = vec![(cache_dir, 0usize)];
    let mut bytes = 0u64;
    let mut entries = 0u64;
    let mut newest_modified_at_ms = None;
    while let Some((directory, depth)) = stack.pop() {
        if depth > 64 {
            bail!("isolated cache exceeds the bounded directory depth");
        }
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                bail!("isolated cache contains an unsupported symbolic link");
            }
            entries = entries.saturating_add(1);
            if entries > MAX_CACHE_ENTRIES {
                bail!("isolated cache exceeds the bounded entry limit");
            }
            if metadata.is_dir() {
                stack.push((entry.path(), depth + 1));
            } else if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let modified_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                        newest_modified_at_ms = Some(
                            newest_modified_at_ms
                                .map_or(modified_ms, |current: u64| current.max(modified_ms)),
                        );
                    }
                }
            }
        }
    }
    Ok(CacheInspection {
        bytes,
        entries,
        newest_modified_at_ms,
        stale: newest_modified_at_ms
            .is_some_and(|modified| now_ms.saturating_sub(modified) > DEFAULT_CACHE_TTL_MS),
    })
}

pub fn enforce_cache_quota(cache_dir: impl AsRef<Path>) -> Result<CacheInspection> {
    let inspection = inspect_cache(cache_dir, current_time_ms())?;
    if inspection.bytes > DEFAULT_CACHE_QUOTA_BYTES {
        bail!("isolated cache exceeds the d-token quota; explicit index deletion is required");
    }
    Ok(inspection)
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchConfig {
    pub upstream_path: PathBuf,
    pub repository_root: PathBuf,
    pub cache_dir: PathBuf,
}

impl LaunchConfig {
    pub fn validate(
        upstream_path: impl AsRef<Path>,
        repository_root: impl AsRef<Path>,
        cache_dir: impl AsRef<Path>,
    ) -> Result<Self> {
        let verified = verify_upstream_artifact_with_digest(upstream_path)?;
        Self::validate_verified(&verified, repository_root, cache_dir)
    }

    /// Validates repository and cache policy using an artifact proof produced
    /// by `verify_upstream_artifact_with_digest`. This avoids rereading a large
    /// immutable binary several times inside one guarded desktop transaction.
    pub fn validate_verified(
        upstream: &VerifiedUpstreamArtifact,
        repository_root: impl AsRef<Path>,
        cache_dir: impl AsRef<Path>,
    ) -> Result<Self> {
        let upstream_path = canonical_file(&upstream.path, "upstream executable")?;
        if upstream_path != upstream.path {
            bail!("verified upstream executable path changed before launch validation");
        }
        let repository_root = fs::canonicalize(repository_root.as_ref()).with_context(|| {
            format!(
                "failed to resolve approved repository root {}",
                repository_root.as_ref().display()
            )
        })?;
        if !repository_root.is_dir() {
            bail!("approved repository root is not a directory");
        }
        if repository_root.parent().is_none() {
            bail!("filesystem roots cannot be approved for codebase-memory indexing");
        }
        for home_name in ["HOME", "USERPROFILE"] {
            if let Some(home) =
                std::env::var_os(home_name).and_then(|value| fs::canonicalize(value).ok())
            {
                if repository_root == home {
                    bail!("the user home directory cannot be approved as a project root");
                }
            }
        }

        fs::create_dir_all(cache_dir.as_ref()).with_context(|| {
            format!(
                "failed to create isolated cache {}",
                cache_dir.as_ref().display()
            )
        })?;
        let cache_dir = fs::canonicalize(cache_dir.as_ref()).with_context(|| {
            format!(
                "failed to resolve isolated cache {}",
                cache_dir.as_ref().display()
            )
        })?;
        secure_cache_permissions(&cache_dir)?;
        if cache_dir.starts_with(&repository_root) || repository_root.starts_with(&cache_dir) {
            bail!("isolated cache must not overlap the approved repository root");
        }
        enforce_cache_quota(&cache_dir)?;

        let config = Self {
            upstream_path,
            repository_root,
            cache_dir,
        };
        config.validate_repository_inputs()?;
        config.ensure_repository_zero_write_precondition()?;
        Ok(config)
    }

    pub fn ensure_repository_zero_write_precondition(&self) -> Result<()> {
        for relative in PROTECTED_REPOSITORY_ARTIFACTS {
            if self.repository_root.join(relative).exists() {
                bail!(
                    "repository contains a codebase-memory persistence artifact; remove or relocate it before d-token indexing"
                );
            }
        }
        Ok(())
    }

    pub fn validate_repository_inputs(&self) -> Result<()> {
        if self.repository_root.join(".codebase-memory.json").exists() {
            bail!(
                "project .codebase-memory.json is untrusted and must be reviewed before indexing"
            );
        }
        let sensitive = find_unignored_sensitive_paths(&self.repository_root)?;
        if !sensitive.is_empty() {
            bail!(
                "repository contains sensitive paths that are not excluded from indexing: {}",
                sensitive.join(", ")
            );
        }
        Ok(())
    }
}

#[cfg(unix)]
fn secure_cache_permissions(cache_dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(cache_dir, fs::Permissions::from_mode(0o700)).with_context(|| {
        format!(
            "failed to enforce private cache permissions on {}",
            cache_dir.display()
        )
    })
}

#[cfg(target_os = "windows")]
fn secure_cache_permissions(cache_dir: &Path) -> Result<()> {
    let username = std::env::var("USERNAME").context("USERNAME is unavailable for cache ACL")?;
    let identity = std::env::var("USERDOMAIN")
        .ok()
        .filter(|domain| !domain.is_empty())
        .map_or(username.clone(), |domain| format!("{domain}\\{username}"));
    let user_grant = format!("{identity}:(OI)(CI)F");
    let status = Command::new("icacls")
        .arg(cache_dir)
        .args(["/inheritance:r", "/grant:r"])
        .arg(user_grant)
        .arg("*S-1-5-18:(OI)(CI)F")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to invoke icacls for the private cache")?;
    if !status.success() {
        bail!("failed to enforce a private Windows cache ACL");
    }
    Ok(())
}

#[cfg(not(any(unix, target_os = "windows")))]
fn secure_cache_permissions(_cache_dir: &Path) -> Result<()> {
    Ok(())
}

const SENSITIVE_FILE_NAMES: &[&str] = &[
    "auth.json",
    "credentials.json",
    "service_account.json",
    "id_rsa",
    "id_ed25519",
];
const SENSITIVE_DIRECTORY_NAMES: &[&str] = &[
    ".ai-bridge",
    ".d-token",
    ".ssh",
    ".aws",
    ".gnupg",
    "private-recovery",
];
const DISCOVERY_SKIP_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    ".cache",
];
const MAX_PREFLIGHT_ENTRIES: usize = 1_000_000;

fn find_unignored_sensitive_paths(root: &Path) -> Result<Vec<String>> {
    let mut candidates = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("failed to inspect repository path {}", directory.display()))?
        {
            visited += 1;
            if visited > MAX_PREFLIGHT_ENTRIES {
                bail!("repository preflight exceeds the bounded entry limit");
            }
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let path = entry.path();
            if file_type.is_dir() {
                if DISCOVERY_SKIP_DIRECTORIES.contains(&name.as_str()) {
                    continue;
                }
                if SENSITIVE_DIRECTORY_NAMES.contains(&name.as_str()) {
                    candidates.push(path);
                    continue;
                }
                stack.push(path);
            } else if file_type.is_file()
                && (name == ".env"
                    || name.starts_with(".env.")
                    || name.ends_with(".pem")
                    || name.ends_with(".key")
                    || SENSITIVE_FILE_NAMES.contains(&name.as_str()))
            {
                candidates.push(path);
            }
        }
    }
    let mut unignored = Vec::new();
    for path in candidates {
        if !is_git_ignored(root, &path) {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            unignored.push(relative.to_string_lossy().to_string());
        }
    }
    unignored.sort();
    unignored.dedup();
    Ok(unignored)
}

fn is_git_ignored(root: &Path, path: &Path) -> bool {
    if !root.join(".git").exists() {
        return false;
    }
    let relative = path.strip_prefix(root).unwrap_or(path);
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "-q", "--"])
        .arg(relative)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryWriteSentinel {
    root: PathBuf,
    fingerprint: BTreeMap<String, String>,
}

impl RepositoryWriteSentinel {
    pub fn capture(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.to_path_buf(),
            fingerprint: repository_write_fingerprint(root)?,
        })
    }

    pub fn verify_unchanged(&self) -> Result<()> {
        let after = repository_write_fingerprint(&self.root)?;
        if after == self.fingerprint {
            return Ok(());
        }
        let changed = self
            .fingerprint
            .keys()
            .chain(after.keys())
            .filter(|path| self.fingerprint.get(*path) != after.get(*path))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        bail!(
            "repository zero-write sentinel detected unexpected changes: {}",
            changed.join(", ")
        )
    }
}

fn repository_write_fingerprint(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut fingerprint = BTreeMap::new();
    fingerprint_path(root, &root.join(".gitattributes"), &mut fingerprint, 0)?;
    fingerprint_path(root, &root.join("artifact.json"), &mut fingerprint, 0)?;
    fingerprint_path(root, &root.join(".codebase-memory"), &mut fingerprint, 0)?;
    Ok(fingerprint)
}

fn fingerprint_path(
    root: &Path,
    path: &Path,
    output: &mut BTreeMap<String, String>,
    depth: usize,
) -> Result<()> {
    if depth > 8 || !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
    if metadata.file_type().is_symlink() {
        output.insert(relative.to_string(), "symlink".to_string());
    } else if metadata.is_file() {
        output.insert(relative.to_string(), sha256_file(path)?);
    } else if metadata.is_dir() {
        output.insert(format!("{relative}/"), "directory".to_string());
        let mut children = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        children.sort_by_key(|entry| entry.file_name());
        if children.len() > 10_000 {
            bail!("repository write sentinel directory exceeds the bounded entry limit");
        }
        for child in children {
            fingerprint_path(root, &child.path(), output, depth + 1)?;
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read artifact {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("failed to resolve {label} {}", path.display()))?;
    if !canonical.is_file() {
        bail!("{label} is not a file");
    }
    Ok(canonical)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("invalid JSON-RPC request")]
    InvalidRequest,
    #[error("method is not permitted by d-token codebase-memory policy")]
    MethodDenied,
    #[error("tool is not permitted by d-token codebase-memory policy")]
    ToolDenied,
    #[error("index arguments violate d-token codebase-memory policy")]
    InvalidIndexArguments,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RequestDisposition {
    Respond(Value),
    Forward(Value),
    Ignore,
}

pub fn evaluate_request(
    request: &Value,
    repository_root: &Path,
) -> std::result::Result<RequestDisposition, PolicyError> {
    let object = request.as_object().ok_or(PolicyError::InvalidRequest)?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(PolicyError::InvalidRequest);
    }
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or(PolicyError::InvalidRequest)?;
    let id = object.get("id").cloned();

    match method {
        "initialize" => id
            .map(|_| RequestDisposition::Forward(request.clone()))
            .ok_or(PolicyError::InvalidRequest),
        "notifications/initialized" | "notifications/cancelled" => {
            Ok(RequestDisposition::Forward(request.clone()))
        }
        "ping" => Ok(id.map_or(RequestDisposition::Ignore, |id| {
            RequestDisposition::Respond(json!({"jsonrpc":"2.0", "id":id, "result":{}}))
        })),
        "tools/list" => Ok(id.map_or(RequestDisposition::Ignore, |id| {
            RequestDisposition::Respond(json!({
                "jsonrpc":"2.0", "id":id, "result":{"tools": public_tool_definitions()}
            }))
        })),
        "tools/call" => {
            let id = id.ok_or(PolicyError::InvalidRequest)?;
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .ok_or(PolicyError::InvalidRequest)?;
            if SAFE_READ_TOOLS.contains(&name) {
                return Ok(RequestDisposition::Forward(request.clone()));
            }
            if name != "index_repository" {
                return Err(PolicyError::ToolDenied);
            }
            let arguments = constrained_index_arguments(request, repository_root)?;
            Ok(RequestDisposition::Forward(json!({
                "jsonrpc":"2.0", "id":id, "method":"tools/call",
                "params":{"name":"index_repository", "arguments":arguments}
            })))
        }
        _ => Err(PolicyError::MethodDenied),
    }
}

fn constrained_index_arguments(
    request: &Value,
    repository_root: &Path,
) -> std::result::Result<Value, PolicyError> {
    let arguments = request
        .pointer("/params/arguments")
        .and_then(Value::as_object)
        .ok_or(PolicyError::InvalidIndexArguments)?;
    let allowed_keys = BTreeSet::from(["repo_path", "mode"]);
    if arguments
        .keys()
        .any(|key| !allowed_keys.contains(key.as_str()))
    {
        return Err(PolicyError::InvalidIndexArguments);
    }
    let supplied_root = arguments
        .get("repo_path")
        .and_then(Value::as_str)
        .ok_or(PolicyError::InvalidIndexArguments)?;
    let supplied_root =
        fs::canonicalize(supplied_root).map_err(|_| PolicyError::InvalidIndexArguments)?;
    if supplied_root != repository_root {
        return Err(PolicyError::InvalidIndexArguments);
    }
    let mode = arguments
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("moderate");
    if !matches!(mode, "full" | "moderate" | "fast") {
        return Err(PolicyError::InvalidIndexArguments);
    }
    Ok(json!({
        "repo_path": repository_root.to_string_lossy(),
        "mode": mode,
        "persistence": false
    }))
}

pub fn public_tool_definitions() -> Vec<Value> {
    let mut tools = Vec::with_capacity(SAFE_READ_TOOLS.len() + 1);
    tools.push(json!({
        "name": "index_repository",
        "description": "Index only the repository approved by d-token. Persistence, custom names, and cross-repository modes are disabled.",
        "inputSchema": {
            "type":"object",
            "properties": {
                "repo_path":{"type":"string", "description":"Exact canonical repository root approved by d-token."},
                "mode":{"type":"string", "enum":["full","moderate","fast"], "default":"moderate"}
            },
            "required":["repo_path"], "additionalProperties":false
        }
    }));
    for name in SAFE_READ_TOOLS {
        tools.push(json!({
            "name": name,
            "description": format!("Read-only code graph tool `{name}` constrained to the d-token-approved index."),
            "inputSchema":{"type":"object", "additionalProperties":true}
        }));
    }
    tools
}

pub fn jsonrpc_policy_error(id: Option<Value>, error: &PolicyError) -> Option<Value> {
    id.map(|id| json!({
        "jsonrpc":"2.0", "id":id,
        "error":{"code":-32602, "message":error.to_string(), "data":{"policy_version":POLICY_VERSION}}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        let cache = temp.path().join("cache");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&cache).unwrap();
        (
            temp,
            repo.canonicalize().unwrap(),
            cache.canonicalize().unwrap(),
        )
    }

    #[test]
    fn cache_inspection_is_bounded_and_reports_usage() {
        let (_temp, _repo, cache) = fixture();
        fs::create_dir_all(cache.join("nested")).unwrap();
        fs::write(cache.join("nested/graph.cache"), b"12345").unwrap();
        let inspection = inspect_cache(&cache, current_time_ms()).unwrap();
        assert_eq!(inspection.bytes, 5);
        assert_eq!(inspection.entries, 2);
        assert!(!inspection.stale);
        assert!(enforce_cache_quota(&cache).is_ok());
    }

    #[test]
    fn exposes_only_versioned_safe_tool_set() {
        let names = public_tool_definitions()
            .into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        assert_eq!(names.len(), SAFE_READ_TOOLS.len() + 1);
        assert!(names.contains(&"index_repository".to_string()));
        assert!(!names.iter().any(|name| matches!(
            name.as_str(),
            "delete_project" | "manage_adr" | "ingest_traces"
        )));
    }

    #[test]
    fn rewrites_index_to_exact_root_and_forces_no_persistence() {
        let (_temp, repo, _cache) = fixture();
        let request = json!({
            "jsonrpc":"2.0", "id":7, "method":"tools/call",
            "params":{"name":"index_repository", "arguments":{"repo_path":repo, "mode":"fast"}}
        });
        let RequestDisposition::Forward(value) = evaluate_request(&request, &repo).unwrap() else {
            panic!("request was not forwarded")
        };
        assert_eq!(
            value
                .pointer("/params/arguments/repo_path")
                .and_then(Value::as_str),
            repo.to_str()
        );
        assert_eq!(
            value.pointer("/params/arguments/mode"),
            Some(&json!("fast"))
        );
        assert_eq!(
            value.pointer("/params/arguments/persistence"),
            Some(&json!(false))
        );
    }

    #[test]
    fn rejects_cross_repo_custom_name_persistence_and_unknown_arguments() {
        let (_temp, repo, _cache) = fixture();
        for extra in [
            json!({"persistence":true}),
            json!({"target_projects":["*"]}),
            json!({"name":"foreign"}),
            json!({"unexpected":true}),
        ] {
            let mut arguments = Map::from_iter([("repo_path".to_string(), json!(repo))]);
            arguments.extend(extra.as_object().unwrap().clone());
            let request = json!({
                "jsonrpc":"2.0", "id":1, "method":"tools/call",
                "params":{"name":"index_repository", "arguments":arguments}
            });
            assert_eq!(
                evaluate_request(&request, &repo),
                Err(PolicyError::InvalidIndexArguments)
            );
        }
    }

    #[test]
    fn rejects_other_roots_mutating_tools_and_unlisted_methods() {
        let (temp, repo, _cache) = fixture();
        let other = temp.path().join("other");
        fs::create_dir_all(&other).unwrap();
        let request = json!({
            "jsonrpc":"2.0", "id":1, "method":"tools/call",
            "params":{"name":"index_repository", "arguments":{"repo_path":other}}
        });
        assert_eq!(
            evaluate_request(&request, &repo),
            Err(PolicyError::InvalidIndexArguments)
        );
        for tool in [
            "delete_project",
            "manage_adr",
            "ingest_traces",
            "future_tool",
        ] {
            let request = json!({
                "jsonrpc":"2.0", "id":1, "method":"tools/call",
                "params":{"name":tool, "arguments":{}}
            });
            assert_eq!(
                evaluate_request(&request, &repo),
                Err(PolicyError::ToolDenied)
            );
        }
        assert_eq!(
            evaluate_request(
                &json!({"jsonrpc":"2.0", "id":1, "method":"resources/list"}),
                &repo
            ),
            Err(PolicyError::MethodDenied)
        );
    }

    #[test]
    fn refuses_cache_overlap_and_preexisting_persistence_artifacts() {
        let (temp, repo, _cache) = fixture();
        let upstream = temp.path().join("upstream");
        fs::write(&upstream, b"binary").unwrap();
        assert!(LaunchConfig::validate(&upstream, &repo, repo.join("cache")).is_err());
        let config = LaunchConfig {
            upstream_path: upstream,
            repository_root: repo.clone(),
            cache_dir: temp.path().join("external-cache"),
        };
        fs::create_dir_all(repo.join(".codebase-memory")).unwrap();
        fs::write(repo.join(".codebase-memory/graph.db.zst"), b"existing").unwrap();
        assert!(config.ensure_repository_zero_write_precondition().is_err());
    }

    #[test]
    fn artifact_manifest_is_complete_and_unique_for_supported_desktop_targets() {
        assert_eq!(ARTIFACT_SPECS.len(), 4);
        let keys = ARTIFACT_SPECS
            .iter()
            .map(|spec| (spec.os, spec.arch))
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), ARTIFACT_SPECS.len());
        assert!(ARTIFACT_SPECS.iter().all(|spec| {
            spec.upstream_version == "0.9.0"
                && spec.upstream_commit.len() == 40
                && spec.archive_sha256.len() == 64
                && spec.binary_sha256.len() == 64
        }));
    }

    #[test]
    fn runtime_rejects_wrong_hash_even_when_a_file_looks_executable() {
        let root = tempfile::tempdir().expect("temporary artifact root");
        let upstream = root.path().join(if cfg!(target_os = "windows") {
            "codebase-memory-mcp.exe"
        } else {
            "codebase-memory-mcp"
        });
        fs::write(&upstream, b"not-the-pinned-v0.9.0-binary").expect("write wrong artifact");
        let error = verify_upstream_artifact(&upstream).expect_err("wrong hash must fail closed");
        assert!(error
            .to_string()
            .contains("SHA-256 does not match the pinned artifact manifest"));
    }

    #[test]
    fn sensitive_repository_inputs_fail_closed() {
        let (_temp, repo, _cache) = fixture();
        fs::write(repo.join(".env.production"), b"TOKEN=do-not-index").unwrap();
        let sensitive = find_unignored_sensitive_paths(&repo).unwrap();
        assert_eq!(sensitive, vec![".env.production"]);
        fs::write(repo.join(".codebase-memory.json"), b"{}").unwrap();
        let config = LaunchConfig {
            upstream_path: repo.join("upstream"),
            repository_root: repo,
            cache_dir: PathBuf::from("/tmp/cache"),
        };
        assert!(config.validate_repository_inputs().is_err());
    }

    #[test]
    fn repository_write_sentinel_detects_only_protected_surface_changes() {
        let (_temp, repo, _cache) = fixture();
        let sentinel = RepositoryWriteSentinel::capture(&repo).unwrap();
        fs::write(repo.join("ordinary-source.rs"), b"fn main() {}").unwrap();
        sentinel.verify_unchanged().unwrap();

        let sentinel = RepositoryWriteSentinel::capture(&repo).unwrap();
        fs::write(repo.join(".gitattributes"), b"unexpected").unwrap();
        let error = sentinel.verify_unchanged().unwrap_err();
        assert!(error.to_string().contains(".gitattributes"));
    }

    #[test]
    fn status_keeps_five_dimensions_independent() {
        let status = CodebaseMemoryStatus::unavailable(42);
        assert_eq!(status.package.state, DimensionState::Unavailable);
        assert_eq!(status.integration.state, DimensionState::NotConfigured);
        assert_eq!(status.agent_session.state, DimensionState::Unknown);
        assert_eq!(status.ui_session.state, DimensionState::Stopped);
        assert_eq!(status.index.state, DimensionState::Unavailable);
    }

    #[test]
    fn hook_is_bounded_truthful_and_never_echoes_input() {
        let input = b"DO_NOT_ECHO secret prompt content";
        let mut output = Vec::new();
        run_hook_session_start(&input[..], &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(!text.contains("DO_NOT_ECHO"));
        assert!(text.contains("Never infer"));
        assert!(text.contains("persistence disabled"));
        assert!(text.chars().count() <= MAX_HOOK_OUTPUT_CHARS + 1);
    }
}
