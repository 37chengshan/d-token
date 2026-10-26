pub mod package_import;
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
pub const MAX_MCP_REQUEST_BYTES: usize = 512 * 1024;
pub const MAX_MCP_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_CACHE_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_CACHE_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const MAX_CACHE_ENTRIES: u64 = 1_000_000;
const MAX_HOOK_INPUT_BYTES: u64 = 64 * 1024;
const MAX_HOOK_OUTPUT_CHARS: usize = 700;
const RUNTIME_NAMESPACE_PREFIX: &str = "dtoken-cbm-runtime";

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
        upstream_version: "0.10.8",
        upstream_commit: "46ae198fc11cda80e817acbc5f5908d7c2de7032",
        archive_name: "codebase-memory-mcp-darwin-arm64.tar.gz",
        archive_sha256: "9bd840dfb3ec7eaef4f310382057adaa5b0e904df883104d03ffcf39836afd07",
        binary_sha256: "2412e017268bef8f847f38d1b0f79f63185b38c27fe6fba637067bfc87c0eedf",
    },
    ArtifactSpec {
        os: "macos",
        arch: "x86_64",
        upstream_version: "0.10.8",
        upstream_commit: "46ae198fc11cda80e817acbc5f5908d7c2de7032",
        archive_name: "codebase-memory-mcp-darwin-amd64.tar.gz",
        archive_sha256: "2b193085410af3801634a522f4b17dcd6699695e015a068393c87817c1d260d4",
        binary_sha256: "1d5b79257d91973809ee70482781db3d1a78f62fc1cd5d18a5ab0608451d16d4",
    },
    ArtifactSpec {
        os: "windows",
        arch: "x86_64",
        upstream_version: "0.10.8",
        upstream_commit: "46ae198fc11cda80e817acbc5f5908d7c2de7032",
        archive_name: "codebase-memory-mcp-windows-amd64.zip",
        archive_sha256: "b43ad982994c4d829670749e08d3b622a74bb20041fc0a7d02bef6113f81c34d",
        binary_sha256: "b4b403b1d7c4def3785f148b93f345ce8427858f4f5489ce28580c4387a336a6",
    },
    ArtifactSpec {
        os: "windows",
        arch: "aarch64",
        upstream_version: "0.10.8",
        upstream_commit: "46ae198fc11cda80e817acbc5f5908d7c2de7032",
        archive_name: "codebase-memory-mcp-windows-arm64.zip",
        archive_sha256: "254b26e819f00bab7f430c5f809d37d22b07bb3eb6427e290e5a27ba5b8e983e",
        binary_sha256: "67b0341ee62f07f850d3954e4f387855f90ea8c6c4b7ed41b8a62d61344373a4",
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
    if actual_sha256 != expected.binary_sha256 {
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
    pub runtime_dir: PathBuf,
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
        let runtime_dir = prepare_runtime_dir(&cache_dir)?;
        enforce_cache_quota(&cache_dir)?;

        let config = Self {
            upstream_path,
            repository_root,
            cache_dir,
            runtime_dir,
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

pub fn runtime_dir_for_cache(cache_dir: impl AsRef<Path>) -> Result<PathBuf> {
    let cache_path = cache_dir.as_ref();
    let cache_identity = fs::canonicalize(cache_path)
        .or_else(|error| {
            if cache_path.is_absolute() {
                Ok(cache_path.to_path_buf())
            } else {
                Err(error)
            }
        })
        .with_context(|| format!("failed to resolve isolated cache {}", cache_path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(b"dtoken-codebase-memory-runtime-v1 ");
    hasher.update(cache_identity.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let digest = digest
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(runtime_base_dir().join(digest))
}

pub fn cleanup_runtime_dir_for_cache(cache_dir: impl AsRef<Path>) -> Result<bool> {
    let runtime_dir = runtime_dir_for_cache(cache_dir)?;
    if !runtime_dir.exists() {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(&runtime_dir).with_context(|| {
        format!(
            "failed to inspect isolated runtime directory {}",
            runtime_dir.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("isolated runtime path is not a d-token-owned directory");
    }
    let runtime_dir = fs::canonicalize(&runtime_dir).with_context(|| {
        format!(
            "failed to resolve isolated runtime directory {}",
            runtime_dir.display()
        )
    })?;
    let runtime_base = fs::canonicalize(runtime_base_dir())
        .context("failed to resolve d-token runtime namespace")?;
    if !runtime_dir.starts_with(&runtime_base) {
        bail!("isolated runtime directory escaped the d-token runtime namespace");
    }
    fs::remove_dir_all(&runtime_dir).with_context(|| {
        format!(
            "failed to remove isolated runtime directory {}",
            runtime_dir.display()
        )
    })?;
    Ok(true)
}

fn prepare_runtime_dir(cache_dir: &Path) -> Result<PathBuf> {
    let runtime_base = runtime_base_dir();
    fs::create_dir_all(&runtime_base).with_context(|| {
        format!(
            "failed to create d-token runtime namespace {}",
            runtime_base.display()
        )
    })?;
    secure_cache_permissions(&runtime_base)?;
    let runtime_base = fs::canonicalize(&runtime_base).with_context(|| {
        format!(
            "failed to resolve d-token runtime namespace {}",
            runtime_base.display()
        )
    })?;
    let runtime_dir = runtime_dir_for_cache(cache_dir)?;
    fs::create_dir_all(&runtime_dir).with_context(|| {
        format!(
            "failed to create isolated runtime directory {}",
            runtime_dir.display()
        )
    })?;
    secure_cache_permissions(&runtime_dir)?;
    let runtime_dir = fs::canonicalize(&runtime_dir).with_context(|| {
        format!(
            "failed to resolve isolated runtime directory {}",
            runtime_dir.display()
        )
    })?;
    if !runtime_dir.is_dir() || !runtime_dir.starts_with(&runtime_base) {
        bail!("isolated runtime directory must stay within the d-token runtime namespace");
    }
    Ok(runtime_dir)
}

fn runtime_base_dir() -> PathBuf {
    runtime_parent_dir().join(runtime_namespace_name())
}

#[cfg(unix)]
fn runtime_parent_dir() -> PathBuf {
    PathBuf::from("/tmp")
}

#[cfg(not(unix))]
fn runtime_parent_dir() -> PathBuf {
    std::env::temp_dir()
}

#[cfg(unix)]
fn runtime_namespace_name() -> String {
    format!("{}-{}", RUNTIME_NAMESPACE_PREFIX, unsafe {
        libc::geteuid()
    })
}

#[cfg(not(unix))]
fn runtime_namespace_name() -> String {
    RUNTIME_NAMESPACE_PREFIX.to_string()
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

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
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
                let forwarded = constrained_read_arguments(request, name, repository_root)?;
                return Ok(RequestDisposition::Forward(forwarded));
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

fn constrained_read_arguments(
    request: &Value,
    name: &str,
    repository_root: &Path,
) -> std::result::Result<Value, PolicyError> {
    let id = request
        .get("id")
        .cloned()
        .ok_or(PolicyError::InvalidRequest)?;
    let arguments = request
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let arguments = arguments
        .as_object()
        .ok_or(PolicyError::InvalidRequest)?
        .clone();
    if arguments_reference_external_project(&Value::Object(arguments.clone()), repository_root) {
        return Err(PolicyError::InvalidIndexArguments);
    }
    Ok(json!({
        "jsonrpc":"2.0",
        "id":id,
        "method":"tools/call",
        "params":{"name":name, "arguments":arguments}
    }))
}

fn arguments_reference_external_project(value: &Value, repository_root: &Path) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            if project_selector_key(key) {
                return !selector_value_matches_root(value, repository_root);
            }
            arguments_reference_external_project(value, repository_root)
        }),
        Value::Array(values) => values
            .iter()
            .any(|value| arguments_reference_external_project(value, repository_root)),
        _ => false,
    }
}

pub(crate) fn project_selector_key(key: &str) -> bool {
    matches!(
        key,
        "project"
            | "project_name"
            | "project_id"
            | "projects"
            | "repo"
            | "repo_path"
            | "repository"
            | "repository_root"
            | "root"
            | "root_path"
            | "target_project"
            | "target_projects"
    )
}

fn selector_value_matches_root(value: &Value, repository_root: &Path) -> bool {
    match value {
        Value::String(raw) => fs::canonicalize(raw).ok().as_deref() == Some(repository_root),
        Value::Array(values) => values
            .iter()
            .all(|value| selector_value_matches_root(value, repository_root)),
        _ => false,
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

pub fn filter_list_projects_response(mut response: Value, repository_root: &Path) -> Result<Value> {
    let mut filtered_any = false;
    if let Some(container) = response.pointer_mut("/result") {
        filtered_any |= filter_projects_container(container, repository_root);
    }
    if let Some(container) = response.pointer_mut("/result/structuredContent") {
        filtered_any |= filter_projects_container(container, repository_root);
    }

    if let Some(content) = response
        .pointer_mut("/result/content")
        .and_then(Value::as_array_mut)
    {
        for item in content {
            let Some(text) = item.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Ok(mut value) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            if filter_projects_container(&mut value, repository_root) {
                *item.get_mut("text").expect("text was already present") =
                    Value::String(serde_json::to_string(&value)?);
                filtered_any = true;
            }
        }
    }

    if filtered_any {
        return Ok(response);
    }
    bail!("list_projects response is missing its project list")
}

fn filter_projects_container(container: &mut Value, repository_root: &Path) -> bool {
    let Some(projects) = container.get_mut("projects").and_then(Value::as_array_mut) else {
        return false;
    };
    filter_projects_array(projects, repository_root);
    let visible = Value::from(projects.len());
    if let Some(object) = container.as_object_mut() {
        object.insert("total".to_string(), visible.clone());
        object.insert("returned".to_string(), visible);
        object.insert("offset".to_string(), Value::from(0));
        object.insert("has_more".to_string(), Value::Bool(false));
    }
    true
}

fn filter_projects_array(projects: &mut Vec<Value>, repository_root: &Path) {
    projects.retain(|project| {
        project
            .get("root_path")
            .and_then(Value::as_str)
            .and_then(|root| fs::canonicalize(root).ok())
            .as_deref()
            == Some(repository_root)
    });
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
    fn runtime_directory_is_private_and_derived_from_cache_namespace() {
        let (_temp, _repo, cache) = fixture();
        let runtime = prepare_runtime_dir(&cache).unwrap();
        assert_eq!(
            runtime,
            runtime_dir_for_cache(&cache)
                .unwrap()
                .canonicalize()
                .unwrap()
        );
        assert!(runtime.starts_with(runtime_base_dir().canonicalize().unwrap()));
        assert!(!runtime.starts_with(&cache));
        let sibling_cache = cache.parent().unwrap().join("sibling-role");
        fs::create_dir_all(&sibling_cache).unwrap();
        assert_ne!(runtime, runtime_dir_for_cache(&sibling_cache).unwrap());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::symlink_metadata(&runtime).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
            let socket_path = runtime
                .join(format!("cbm-daemon-{}", unsafe { libc::geteuid() }))
                .join("sock");
            assert!(
                socket_path.to_string_lossy().len() < 100,
                "runtime socket path is too long: {}",
                socket_path.display()
            );
        }
        assert!(cleanup_runtime_dir_for_cache(&cache).unwrap());
        assert!(!runtime.exists());

        let absent_cache = cache.parent().unwrap().join("absent-cache");
        let absent_runtime = runtime_dir_for_cache(&absent_cache).unwrap();
        fs::create_dir_all(&absent_runtime).unwrap();
        assert!(cleanup_runtime_dir_for_cache(&absent_cache).unwrap());
        assert!(!absent_runtime.exists());
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
    fn read_tools_reject_caller_selected_project_scope() {
        let (_temp, repo, _cache) = fixture();
        let request = json!({
            "jsonrpc":"2.0", "id":7, "method":"tools/call",
            "params":{"name":"search_graph", "arguments":{"query":"main", "project":"foreign"}}
        });
        assert_eq!(
            evaluate_request(&request, &repo),
            Err(PolicyError::InvalidIndexArguments)
        );
        let request = json!({
            "jsonrpc":"2.0", "id":7, "method":"tools/call",
            "params":{"name":"search_graph", "arguments":{"query":"main", "repo_path":repo}}
        });
        assert!(matches!(
            evaluate_request(&request, &repo),
            Ok(RequestDisposition::Forward(_))
        ));
    }

    #[test]
    fn list_projects_response_is_filtered_to_approved_root() {
        let (temp, repo, _cache) = fixture();
        let other = temp.path().join("other");
        fs::create_dir_all(&other).unwrap();
        let other = other.canonicalize().unwrap();
        let response = json!({
            "jsonrpc":"2.0",
            "id":1,
            "result":{"projects":[
                {"name":"approved","root_path":repo},
                {"name":"other","root_path":other}
            ]}
        });
        let filtered = filter_list_projects_response(response, &repo).unwrap();
        let projects = filtered
            .pointer("/result/projects")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0].get("name").and_then(Value::as_str),
            Some("approved")
        );

        let response = json!({
            "jsonrpc":"2.0",
            "id":1,
            "result":{
                "content":[{"type":"text","text":serde_json::to_string(&json!({
                    "projects":[
                        {"name":"approved","root_path":repo},
                        {"name":"other","root_path":other}
                    ],
                    "total":2,
                    "returned":2,
                    "has_more":true,
                    "offset":0
                })).unwrap()}],
                "structuredContent":{
                    "projects":[
                        {"name":"approved","root_path":repo},
                        {"name":"other","root_path":other}
                    ],
                    "total":2,
                    "returned":2,
                    "has_more":true,
                    "offset":0
                }
            }
        });
        let filtered = filter_list_projects_response(response, &repo).unwrap();
        let text = filtered
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .unwrap();
        let value: Value = serde_json::from_str(text).unwrap();
        let projects = value.get("projects").and_then(Value::as_array).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(value.get("total"), Some(&json!(1)));
        assert_eq!(value.get("returned"), Some(&json!(1)));
        assert_eq!(value.get("has_more"), Some(&json!(false)));
        assert_eq!(
            projects[0].get("name").and_then(Value::as_str),
            Some("approved")
        );
        let structured = filtered
            .pointer("/result/structuredContent/projects")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(structured.len(), 1);
        assert_eq!(
            filtered.pointer("/result/structuredContent/total"),
            Some(&json!(1))
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
            runtime_dir: temp.path().join("external-cache/runtime"),
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
            spec.upstream_version == "0.10.8"
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
        fs::write(&upstream, b"not-the-pinned-v0.10.8-binary").expect("write wrong artifact");
        let error = verify_upstream_artifact(&upstream).expect_err("wrong hash must fail closed");
        // ARTIFACT_SPECS only pins upstream binaries for supported desktop
        // targets (macOS/Windows). On other hosts verification fails closed
        // earlier with the platform-registration error, which is the same
        // fail-closed contract; only platforms with a registered spec can
        // reach the hash comparison itself.
        if artifact_spec_for_current_platform().is_ok() {
            assert!(error
                .to_string()
                .contains("SHA-256 does not match the pinned artifact manifest"));
        }
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
            runtime_dir: PathBuf::from("/tmp/cache/runtime"),
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
