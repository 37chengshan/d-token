pub mod filters;

use anyhow::{bail, Context, Result};
use dtoken_types::{
    agent::HostPlatform, AccountingBucket, AccountingEntry, HookStatus, IntegrityState,
    ManagedRtkOperation, ManagedRtkPlatformCapability, ManagedRtkTransactionPlan,
    ManagedRtkTransactionReceipt, ManagedRtkTransactionState, ManagedRtkUsageEvidence,
    PreprocessorImplementation, PreprocessorProfile, RtkDiscoverReport, RtkGainReport,
    RtkSessionReport, RtkStatus, RuntimeMode, TelemetryStatus, ToolStatus,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command as StdCommand,
};
use tokio::process::Command;

/// The bounded, d-token-owned fallback used when the historical RTK binary is
/// not available.  It intentionally operates on one completed command output
/// at a time: callers retain ownership of process execution and of any raw
/// recovery data.  This keeps ordinary runtime telemetry free of command
/// bodies while still giving the route pipeline a deterministic, fail-safe
/// cleanup primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinPreprocessResult {
    pub output: String,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub transforms: Vec<&'static str>,
    pub truncated: bool,
}

/// Applies only lossless whitespace/ANSI cleanup and an explicitly visible
/// repeat-line fold. The cap never silently discards output: the compact
/// `~<bytes>` suffix states exactly how many bytes were omitted. Agent hooks
/// can decide to bypass this for protected/error output.
pub fn preprocess_command_output(input: &str, max_bytes: Option<usize>) -> BuiltinPreprocessResult {
    let input_bytes = input.len();
    let without_ansi = strip_ansi(input);
    let mut transforms = Vec::new();
    if without_ansi != input {
        transforms.push("ansi_strip");
    }

    let mut lines = Vec::new();
    let mut blank_pending = false;
    let mut previous: Option<&str> = None;
    let mut repeats = 0usize;
    for line in without_ansi.lines() {
        if line.trim().is_empty() {
            if !blank_pending {
                lines.push(String::new());
                blank_pending = true;
            }
            continue;
        }
        blank_pending = false;
        if previous == Some(line) {
            repeats += 1;
            continue;
        }
        if repeats > 0 {
            lines.push(format!("[d-token: previous line repeated {repeats} times]"));
            repeats = 0;
        }
        lines.push(line.to_string());
        previous = Some(line);
    }
    if repeats > 0 {
        lines.push(format!("[d-token: previous line repeated {repeats} times]"));
    }
    let mut output = lines.join("\n");
    if input.ends_with('\n') && !output.is_empty() {
        output.push('\n');
    }
    if output != without_ansi {
        transforms.push("blank_and_repeat_fold");
    }

    let mut truncated = false;
    if let Some(limit) = max_bytes {
        if output.len() > limit {
            // UTF-8 boundary and a deterministic marker make the loss
            // explicit; callers must record this as a non-lossless transform.
            let (bounded, did_truncate) = bounded_truncate_marker(&output, limit);
            output = bounded;
            truncated = did_truncate;
            if did_truncate {
                transforms.push("bounded_truncate");
            }
        }
    }
    BuiltinPreprocessResult {
        output_bytes: output.len(),
        output,
        input_bytes,
        transforms,
        truncated,
    }
}

/// Truncates `output` at `limit` bytes on a UTF-8 boundary, appending a
/// compact `~<omitted>` marker so the loss is explicit and deterministic.
/// Returns `(text, true)` when bytes were actually omitted.
pub fn bounded_truncate_marker(output: &str, limit: usize) -> (String, bool) {
    if output.len() <= limit {
        return (output.to_string(), false);
    }
    let mut boundary = limit.min(output.len());
    while boundary > 0 && !output.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let mut body_end = boundary;
    loop {
        let omitted = output.len().saturating_sub(body_end);
        let marker = format!("~{omitted}");
        let next_end = boundary.saturating_sub(marker.len());
        if next_end >= body_end {
            return (format!("{}{}", &output[..body_end], marker), true);
        }
        body_end = next_end;
        while body_end > 0 && !output.is_char_boundary(body_end) {
            body_end -= 1;
        }
    }
}

/// Metadata describing how a host would invoke a managed RTK wrapper. It does
/// not contain argv or output, so it is safe to store with a lifecycle plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedCommandRewrite {
    pub use_managed_wrapper: bool,
    pub propagates_exit_status: bool,
    pub executable: Option<PathBuf>,
    pub args_prefix: Vec<String>,
    pub managed_root: Option<PathBuf>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedHookPaths {
    pub managed_root: PathBuf,
    pub bridge_path: PathBuf,
    pub max_output_bytes: Option<usize>,
    /// Optional d-token-owned structured command filter DSL (TOML, PR-C4A).
    /// When present it is parsed and hash-bound into the manifest as
    /// `filters.toml` under the managed root; when absent the reviewed
    /// builtin defaults apply. The bridge re-verifies the hash before every
    /// wrapped command so a tampered filter can never run.
    pub filter_dsl: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedHookBridgeIdentity {
    pub component: String,
    pub protocol_version: u32,
    pub version: String,
    pub arch: String,
    pub os: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedHookManifest {
    pub schema_version: u8,
    pub transaction_id: String,
    pub platform: HostPlatform,
    pub host_config_fingerprint: String,
    pub managed_root: String,
    pub bridge_path: String,
    pub bridge: ManagedHookBridgeIdentity,
    pub max_output_bytes: Option<usize>,
    /// sha256 of the d-token-owned `filters.toml` DSL, when one was reviewed
    /// at install time. `None` means the bridge must use the builtin
    /// defaults. Missing on older manifests; validated when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_dsl_sha256: Option<String>,
    pub usage_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedHookVerification {
    pub manifest: ManagedHookManifest,
    pub manifest_path: PathBuf,
    pub usage_dir: PathBuf,
}

/// The output is returned directly to the caller for delivery to the Agent;
/// only `evidence` is suitable for persistence. No raw output is retained by
/// `ManagedRtkLifecycle`.
#[derive(Clone, PartialEq, Eq)]
pub struct ManagedCommandOutput {
    pub output: String,
    pub evidence: ManagedRtkUsageEvidence,
}

/// Fail-closed lifecycle contract for the Agent-specific managed RTK
/// adapters. This crate owns command-result filtering, but it does not own an
/// Agent configuration schema. The d-token-owned wrapper manifest lifecycle
/// (install/verify/rollback/uninstall) is available on every supported host
/// platform; an unconfigured managed runtime is reported as `Unavailable`,
/// never hard-`Blocked`. Agent configuration bytes remain externally owned
/// and are transacted by the owning adapter, not by this crate.
#[derive(Debug, Clone)]
pub struct ManagedRtkLifecycle {
    plan: ManagedRtkTransactionPlan,
    state: ManagedRtkTransactionState,
    hook: Option<ManagedHookVerification>,
}

impl ManagedRtkLifecycle {
    pub fn plan(
        transaction_id: &str,
        platform: HostPlatform,
        host_config_fingerprint: &str,
    ) -> Self {
        let valid_id = is_safe_transaction_id(transaction_id);
        let valid_fingerprint = is_sha256_hex(host_config_fingerprint);
        let capability = managed_rtk_platform_capability(platform);
        let state = if valid_id && valid_fingerprint {
            ManagedRtkTransactionState::Planned
        } else {
            ManagedRtkTransactionState::Failed
        };
        Self {
            plan: ManagedRtkTransactionPlan {
                schema_version: 1,
                transaction_id: if valid_id {
                    transaction_id.to_string()
                } else {
                    "invalid-transaction-id".to_string()
                },
                platform,
                host_config_fingerprint: if valid_fingerprint {
                    host_config_fingerprint.to_ascii_lowercase()
                } else {
                    "invalid-fingerprint".to_string()
                },
                capability,
                operations: vec![
                    ManagedRtkOperation::Install,
                    ManagedRtkOperation::Configure,
                    ManagedRtkOperation::Verify,
                    ManagedRtkOperation::Rollback,
                    ManagedRtkOperation::Uninstall,
                ],
                requires_user_confirmation: true,
            },
            state,
            hook: None,
        }
    }

    pub fn transaction_plan(&self) -> &ManagedRtkTransactionPlan {
        &self.plan
    }

    pub fn state(&self) -> ManagedRtkTransactionState {
        self.state
    }

    /// Returns a preview receipt rather than performing a write. The d-token
    /// managed wrapper only owns its own manifest/provenance; Agent-specific
    /// config backup/apply/verify/rollback remains external to this crate.
    ///
    /// The managed runtime is no longer hard-blocked on any host platform: a
    /// valid plan may always be installed and the wrapper lifecycle can be
    /// driven to `Verified`. An operation that cannot proceed from the current
    /// transaction state (e.g. verifying before the wrapper is installed) is
    /// reported as `Unavailable`, not `Blocked`.
    pub fn request_operation(
        &self,
        operation: ManagedRtkOperation,
    ) -> ManagedRtkTransactionReceipt {
        let (state, summary) = if self.state == ManagedRtkTransactionState::Failed {
            (
                ManagedRtkTransactionState::Failed,
                "managed RTK plan rejected: transaction id or host configuration fingerprint is invalid"
                    .to_string(),
            )
        } else {
            match self.preview_transition(operation) {
                Some(target) => (
                    target,
                    format!(
                        "managed RTK {operation:?} is available from {:?} and would transition to {target:?}; {}",
                        self.state, self.plan.capability.reason
                    ),
                ),
                None => (
                    ManagedRtkTransactionState::Unavailable,
                    format!(
                        "managed RTK {operation:?} unavailable from {:?}: the d-token-owned wrapper is not configured for this operation ({})",
                        self.state, self.plan.capability.reason
                    ),
                ),
            }
        };
        ManagedRtkTransactionReceipt {
            schema_version: 1,
            transaction_id: self.plan.transaction_id.clone(),
            operation,
            state,
            host_config_fingerprint: self.plan.host_config_fingerprint.clone(),
            summary,
        }
    }

    /// The state a requested operation would reach from the current
    /// transaction state, or `None` when the operation cannot proceed (the
    /// managed runtime is unconfigured for it).
    fn preview_transition(
        &self,
        operation: ManagedRtkOperation,
    ) -> Option<ManagedRtkTransactionState> {
        Some(match (self.state, operation) {
            (ManagedRtkTransactionState::Planned, ManagedRtkOperation::Install) => {
                ManagedRtkTransactionState::Installed
            }
            (ManagedRtkTransactionState::Installed, ManagedRtkOperation::Configure) => {
                ManagedRtkTransactionState::Configured
            }
            (
                ManagedRtkTransactionState::Installed | ManagedRtkTransactionState::Configured,
                ManagedRtkOperation::Verify,
            ) => ManagedRtkTransactionState::Verified,
            (
                ManagedRtkTransactionState::Installed
                | ManagedRtkTransactionState::Configured
                | ManagedRtkTransactionState::Verified,
                ManagedRtkOperation::Rollback,
            ) => ManagedRtkTransactionState::RolledBack,
            (
                ManagedRtkTransactionState::Installed
                | ManagedRtkTransactionState::Configured
                | ManagedRtkTransactionState::Verified,
                ManagedRtkOperation::Uninstall,
            ) => ManagedRtkTransactionState::Uninstalled,
            _ => return None,
        })
    }

    pub fn install_hook(
        &mut self,
        paths: ManagedHookPaths,
    ) -> Result<ManagedRtkTransactionReceipt> {
        self.ensure_planned()?;
        let verified = write_and_verify_managed_hook_manifest(&self.plan, &paths)?;
        self.state = ManagedRtkTransactionState::Installed;
        self.hook = Some(verified);
        Ok(self.receipt(
            ManagedRtkOperation::Install,
            ManagedRtkTransactionState::Installed,
            "managed RTK wrapper manifest installed; Agent config remains externally owned"
                .to_string(),
        ))
    }

    pub fn verify_hook(&mut self) -> Result<ManagedRtkTransactionReceipt> {
        self.ensure_not_failed()?;
        let hook = self.hook.as_ref().context(
            "managed RTK hook cannot be verified before install_hook has created a manifest",
        )?;
        let verified = verify_managed_hook_root(Path::new(&hook.manifest.managed_root), false)?;
        if verified.manifest.transaction_id != self.plan.transaction_id
            || verified.manifest.host_config_fingerprint != self.plan.host_config_fingerprint
            || verified.manifest.platform != self.plan.platform
        {
            bail!("managed RTK hook manifest does not match the reviewed transaction");
        }
        self.state = ManagedRtkTransactionState::Verified;
        self.hook = Some(verified);
        Ok(self.receipt(
            ManagedRtkOperation::Verify,
            ManagedRtkTransactionState::Verified,
            "managed RTK wrapper verified; command rewrite may use d-token-owned wrap mode"
                .to_string(),
        ))
    }

    pub fn uninstall_hook(&mut self) -> Result<ManagedRtkTransactionReceipt> {
        self.ensure_not_failed()?;
        let hook = self
            .hook
            .take()
            .context("managed RTK hook uninstall requires an installed manifest")?;
        remove_managed_hook_manifest(&hook.manifest_path)?;
        self.state = ManagedRtkTransactionState::Uninstalled;
        Ok(self.receipt(
            ManagedRtkOperation::Uninstall,
            ManagedRtkTransactionState::Uninstalled,
            "managed RTK wrapper uninstalled; command rewrite has returned to passthrough"
                .to_string(),
        ))
    }

    /// A rewrite may run only after the d-token-owned wrapper manifest has been
    /// installed and re-verified. Agent configuration ownership is separate:
    /// this rewrite contract is safe to use whenever a reviewed host has
    /// already pointed at the wrapper.
    pub fn command_rewrite(&self) -> ManagedCommandRewrite {
        if self.state == ManagedRtkTransactionState::Verified {
            if let Some(hook) = &self.hook {
                return managed_command_rewrite(hook);
            }
        }
        ManagedCommandRewrite {
            use_managed_wrapper: false,
            propagates_exit_status: true,
            executable: None,
            args_prefix: vec![],
            managed_root: None,
            reason: "managed hook is not verified; command must run without a d-token wrapper"
                .to_string(),
        }
    }

    pub fn verified_hook(&self) -> Option<&ManagedHookVerification> {
        self.hook.as_ref()
    }

    fn ensure_planned(&self) -> Result<()> {
        self.ensure_not_failed()?;
        if self.state != ManagedRtkTransactionState::Planned {
            bail!(
                "managed RTK install requires a planned transaction, got {:?}",
                self.state
            );
        }
        Ok(())
    }

    fn ensure_not_failed(&self) -> Result<()> {
        if self.state == ManagedRtkTransactionState::Failed {
            bail!(
                "managed RTK plan rejected: transaction id or host configuration fingerprint is invalid"
            );
        }
        Ok(())
    }

    fn receipt(
        &self,
        operation: ManagedRtkOperation,
        state: ManagedRtkTransactionState,
        summary: String,
    ) -> ManagedRtkTransactionReceipt {
        ManagedRtkTransactionReceipt {
            schema_version: 1,
            transaction_id: self.plan.transaction_id.clone(),
            operation,
            state,
            host_config_fingerprint: self.plan.host_config_fingerprint.clone(),
            summary,
        }
    }

    /// Filters an already-produced command result only when a managed hook was
    /// verified. In the current fail-closed state it preserves output and exit
    /// status exactly, while emitting redacted byte-level evidence. The
    /// structured d-token-owned family filters (PR-C4A) apply when the hook
    /// manifest binds a reviewed filter DSL; otherwise the builtin defaults
    /// run. Secret-, error-, and failure-shaped output is always preserved.
    /// Output caps are governed by the reviewed manifest/DSL; the historical
    /// caller-supplied `max_bytes` override is no longer applied.
    pub fn filter_command_output(
        &self,
        output: &str,
        exit_code: Option<i32>,
        _max_bytes: Option<usize>,
    ) -> ManagedCommandOutput {
        let rewrite = self.command_rewrite();
        if rewrite.use_managed_wrapper {
            let config = self
                .hook
                .as_ref()
                .and_then(|hook| managed_filter_config(&hook.manifest).ok())
                .unwrap_or_default();
            let filtered =
                filters::filter_command_output(&[], output, exit_code, &config);
            let saved_bytes = filtered.input_bytes.saturating_sub(filtered.output_bytes);
            ManagedCommandOutput {
                output: filtered.output,
                evidence: ManagedRtkUsageEvidence {
                    schema_version: 1,
                    transaction_id: self.plan.transaction_id.clone(),
                    exit_code,
                    input_bytes: filtered.input_bytes,
                    output_bytes: filtered.output_bytes,
                    saved_bytes,
                    transforms: filtered.transforms,
                    disposition: "filtered_verified_hook".to_string(),
                },
            }
        } else {
            ManagedCommandOutput {
                output: output.to_string(),
                evidence: ManagedRtkUsageEvidence {
                    schema_version: 1,
                    transaction_id: self.plan.transaction_id.clone(),
                    exit_code,
                    input_bytes: output.len(),
                    output_bytes: output.len(),
                    saved_bytes: 0,
                    transforms: vec![],
                    disposition: "passthrough_unverified_hook".to_string(),
                },
            }
        }
    }
}

/// Build the exclusive command rewrite for a fully verified managed hook.
/// Production adapters (Agent hook writers, desktop flows) consume this so
/// the wrap argv, managed root, and exit-status contract stay canonical.
pub fn managed_command_rewrite(verification: &ManagedHookVerification) -> ManagedCommandRewrite {
    let mut args_prefix = vec![
        "rtk".to_string(),
        "wrap".to_string(),
        "--managed-root".to_string(),
        verification.manifest.managed_root.clone(),
    ];
    if let Some(limit) = verification.manifest.max_output_bytes {
        args_prefix.push("--max-bytes".to_string());
        args_prefix.push(limit.to_string());
    }
    args_prefix.push("--".to_string());
    ManagedCommandRewrite {
        use_managed_wrapper: true,
        propagates_exit_status: true,
        executable: Some(PathBuf::from(&verification.manifest.bridge_path)),
        args_prefix,
        managed_root: Some(PathBuf::from(&verification.manifest.managed_root)),
        reason: "verified d-token-owned RTK bridge manifest".to_string(),
    }
}

/// There is intentionally no built-in Agent hook writer in this crate. The
/// capability matrix makes the platform boundary explicit and prevents the
/// builtin fallback from being counted as an installed integrated RTK hook.
pub fn managed_rtk_platform_capability(platform: HostPlatform) -> ManagedRtkPlatformCapability {
    let reason = match platform {
        HostPlatform::Macos => {
            "macOS may use a d-token-owned RTK wrapper manifest, but Agent config remains externally transacted"
        }
        HostPlatform::Windows => {
            "Windows may use a d-token-owned RTK wrapper manifest, but Agent config remains externally transacted"
        }
        HostPlatform::Linux => {
            "Linux/WSL may use a d-token-owned RTK wrapper manifest, but desktop Agent config remains externally transacted"
        }
    };
    ManagedRtkPlatformCapability {
        platform,
        hook_status: HookStatus::Partial,
        can_modify_agent_config: false,
        reason: reason.to_string(),
    }
}

const MANAGED_HOOK_MANIFEST: &str = "managed-rtk-hook.json";
const MANAGED_HOOK_USAGE_DIR: &str = "usage";
const MANAGED_HOOK_SCHEMA_VERSION: u8 = 1;
/// d-token-owned structured command filter DSL artifact (PR-C4A), bound by
/// sha256 in the managed manifest when a reviewed DSL was installed.
pub const MANAGED_HOOK_FILTER_FILE: &str = "filters.toml";
pub const MAX_MANAGED_RTK_FILTER_BYTES: usize = 16 * 1024;
const BRIDGE_COMPONENT: &str = "dtoken-agent-bridge";
const MANAGED_RTK_USAGE_SCHEMA_VERSION: u8 = 1;
const MAX_MANAGED_RTK_MANIFEST_BYTES: usize = 16 * 1024;
pub const MAX_MANAGED_RTK_USAGE_RECORD_BYTES: usize = 8 * 1024;
pub const MAX_MANAGED_RTK_USAGE_TOTAL_BYTES: usize = 256 * 1024;
pub const MAX_MANAGED_RTK_USAGE_ENTRIES: usize = 128;
const MAX_MANAGED_RTK_TRANSFORMS: usize = 16;
/// Bounded retries for the transient Linux `Text file busy` (ETXTBSY) that can
/// surface when a freshly staged bridge copy is executed concurrently.
const MAX_BRIDGE_EXEC_ATTEMPTS: u32 = 3;
pub const MIN_MANAGED_RTK_OUTPUT_BYTES: usize = 1024;
pub const MAX_MANAGED_RTK_OUTPUT_BYTES: usize = 1024 * 1024;

/// A redacted receipt summary. It deliberately excludes command argv, output,
/// digests, and bridge paths; callers may expose it as runtime evidence.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManagedRtkUsageReceipt {
    pub recorded_at_ms: u64,
    pub exit_code: Option<i32>,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub saved_bytes: u64,
}

/// Read-only status and aggregate usage evidence for one caller-supplied
/// managed RTK root. Invalid, unknown, oversized, or symlinked artifacts are
/// never partially accepted: `verified` stays false and no usage is returned.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManagedRtkRuntimeEvidence {
    pub installed: bool,
    pub verified: bool,
    pub usage_count: usize,
    pub saved_bytes: u64,
    pub last_receipt: Option<ManagedRtkUsageReceipt>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedRtkUsageRecord {
    schema_version: u8,
    transaction_id: String,
    exit_code: Option<i32>,
    input_bytes: u64,
    output_bytes: u64,
    saved_bytes: u64,
    transforms: Vec<String>,
    disposition: String,
    command_basename: String,
    command_sha256: String,
    bridge_sha256: String,
    bridge_version: String,
    recorded_at_ms: u64,
}

impl ManagedRtkUsageRecord {
    fn receipt(&self) -> ManagedRtkUsageReceipt {
        ManagedRtkUsageReceipt {
            recorded_at_ms: self.recorded_at_ms,
            exit_code: self.exit_code,
            input_bytes: self.input_bytes,
            output_bytes: self.output_bytes,
            saved_bytes: self.saved_bytes,
        }
    }
}

/// Maps a bounded RTK usage evidence to the canonical command-output
/// accounting bucket (PR-C4C). Fails closed when the source digest is not a
/// sha256 hex value or the byte accounting is inconsistent, so an unknown or
/// negative claim can never enter the ledger as a positive saving.
pub fn command_output_accounting_entry(
    evidence: &ManagedRtkUsageEvidence,
    source_digest: &str,
    recorded_at_ms: u64,
) -> Result<AccountingEntry> {
    if !is_sha256_hex(source_digest) {
        bail!("command-output accounting requires a sha256 source digest");
    }
    let input = u64::try_from(evidence.input_bytes).context("input bytes overflow")?;
    let output = u64::try_from(evidence.output_bytes).context("output bytes overflow")?;
    let saved = u64::try_from(evidence.saved_bytes).context("saved bytes overflow")?;
    if input < output || saved != input - output {
        bail!("command-output accounting bytes are inconsistent");
    }
    Ok(AccountingEntry {
        schema_version: 1,
        bucket: AccountingBucket::CommandOutput,
        source_digest: source_digest.to_string(),
        input_bytes: input,
        output_bytes: output,
        saved_bytes: saved,
        disposition: evidence.disposition.clone(),
        recorded_at_ms,
    })
}

pub fn managed_hook_manifest_path(root: &Path) -> PathBuf {
    root.join(MANAGED_HOOK_MANIFEST)
}

/// Loads the reviewed structured filter DSL (PR-C4A) bound by a verified
/// managed manifest. Re-verifies the sha256 binding so a tampered or missing
/// artifact fails closed; a manifest without a bound DSL yields the reviewed
/// builtin defaults. Callers must have already verified the root.
pub fn managed_filter_config(manifest: &ManagedHookManifest) -> Result<filters::CommandFilterConfig> {
    match &manifest.filter_dsl_sha256 {
        Some(expected) => {
            let root = Path::new(&manifest.managed_root);
            let filter_path = root.join(MANAGED_HOOK_FILTER_FILE);
            let bytes = read_regular_file_bounded(&filter_path, MAX_MANAGED_RTK_FILTER_BYTES)
                .with_context(|| {
                    format!(
                        "managed RTK filter DSL is unavailable: {}",
                        filter_path.display()
                    )
                })?;
            let actual = format!("{:x}", Sha256::digest(&bytes));
            if &actual != expected {
                bail!("managed RTK filter DSL no longer matches the reviewed manifest");
            }
            let text =
                std::str::from_utf8(&bytes).context("managed RTK filter DSL is not UTF-8")?;
            filters::CommandFilterConfig::from_toml(text)
                .context("managed RTK filter DSL cannot be parsed")
        }
        None => Ok(filters::CommandFilterConfig::defaults()),
    }
}

/// Inspects an explicit d-token-managed root without creating directories or
/// reading command output. The manifest and bridge must re-verify before the
/// bounded usage JSON metadata is considered usable.
pub fn inspect_managed_rtk_runtime(root: &Path) -> ManagedRtkRuntimeEvidence {
    let root = match canonical_existing_directory(root, "managed RTK root") {
        Ok(root) => root,
        Err(_) => return unavailable_managed_rtk_evidence("managed_root_unavailable"),
    };
    let manifest_path = managed_hook_manifest_path(&root);
    let manifest = match read_managed_hook_manifest(&manifest_path) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => return unavailable_managed_rtk_evidence("managed_manifest_missing"),
        Err(_) => return unverified_managed_rtk_evidence("managed_root_unverified"),
    };
    if !manifest_matches_managed_root(&manifest, &root)
        || verify_managed_bridge_identity(&manifest).is_err()
    {
        return unverified_managed_rtk_evidence("managed_root_unverified");
    }
    match read_managed_usage_evidence(&root, &manifest) {
        Ok((usage_count, saved_bytes, last_receipt)) => ManagedRtkRuntimeEvidence {
            installed: true,
            verified: true,
            usage_count,
            saved_bytes,
            last_receipt,
            reason: if usage_count == 0 {
                "verified_no_usage_evidence".to_string()
            } else {
                "verified_usage_evidence".to_string()
            },
        },
        Err(_) => unverified_managed_rtk_evidence("managed_usage_unverified"),
    }
}

/// Removes only a fully verified d-token-owned managed RTK manifest and its
/// bounded usage receipts. Unknown entries or invalid evidence fail closed so
/// uninstall cannot sweep user files.
pub fn uninstall_managed_rtk_runtime(root: &Path) -> Result<ManagedRtkTransactionReceipt> {
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ManagedRtkTransactionReceipt {
                schema_version: 1,
                transaction_id: "already-uninstalled".to_string(),
                operation: ManagedRtkOperation::Uninstall,
                state: ManagedRtkTransactionState::Uninstalled,
                host_config_fingerprint: "0".repeat(64),
                summary: "managed RTK wrapper was already absent".to_string(),
            });
        }
        Err(error) => return Err(error).context("managed RTK root metadata is unavailable"),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!("managed RTK root must be a real directory")
        }
        Ok(_) => {}
    }

    let evidence = inspect_managed_rtk_runtime(root);
    if !evidence.installed || !evidence.verified {
        bail!("managed RTK uninstall requires verified ownership evidence");
    }
    let verification = verify_managed_hook_root(root, false)?;
    let mut usage_entries = fs::read_dir(&verification.usage_dir)
        .context("managed RTK usage directory cannot be read")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("managed RTK usage directory contains an unreadable entry")?;
    if usage_entries.len() > MAX_MANAGED_RTK_USAGE_ENTRIES {
        bail!("managed RTK usage entry limit exceeded");
    }
    usage_entries.sort_by_key(|entry| entry.file_name());
    for entry in usage_entries {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("managed RTK usage filename is not UTF-8"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if name.len() > 128
            || !name.ends_with(".json")
            || metadata.file_type().is_symlink()
            || !metadata.is_file()
        {
            bail!("managed RTK usage directory contains an unowned entry");
        }
        fs::remove_file(&path).with_context(|| {
            format!(
                "failed to remove managed RTK usage receipt {}",
                path.display()
            )
        })?;
    }
    fs::remove_dir(&verification.usage_dir).with_context(|| {
        format!(
            "failed to remove managed RTK usage directory {}",
            verification.usage_dir.display()
        )
    })?;
    if verification.manifest.filter_dsl_sha256.is_some() {
        remove_managed_hook_filter(&root.join(MANAGED_HOOK_FILTER_FILE))?;
    }
    remove_managed_hook_manifest(&verification.manifest_path)?;
    match fs::remove_dir(root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to remove managed RTK root {}", root.display()))
        }
    }
    Ok(ManagedRtkTransactionReceipt {
        schema_version: 1,
        transaction_id: verification.manifest.transaction_id,
        operation: ManagedRtkOperation::Uninstall,
        state: ManagedRtkTransactionState::Uninstalled,
        host_config_fingerprint: verification.manifest.host_config_fingerprint,
        summary: "managed RTK wrapper and d-token-owned usage receipts were removed".to_string(),
    })
}

fn unavailable_managed_rtk_evidence(reason: &str) -> ManagedRtkRuntimeEvidence {
    ManagedRtkRuntimeEvidence {
        installed: false,
        verified: false,
        usage_count: 0,
        saved_bytes: 0,
        last_receipt: None,
        reason: reason.to_string(),
    }
}

fn unverified_managed_rtk_evidence(reason: &str) -> ManagedRtkRuntimeEvidence {
    ManagedRtkRuntimeEvidence {
        installed: true,
        verified: false,
        usage_count: 0,
        saved_bytes: 0,
        last_receipt: None,
        reason: reason.to_string(),
    }
}

fn read_managed_hook_manifest(path: &Path) -> Result<Option<ManagedHookManifest>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("managed RTK manifest metadata is unavailable"),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("managed RTK manifest must be a real regular file");
    }
    let bytes = read_regular_file_bounded(path, MAX_MANAGED_RTK_MANIFEST_BYTES)?;
    let manifest = serde_json::from_slice(&bytes).context("managed RTK manifest is invalid")?;
    validate_managed_hook_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn validate_managed_hook_manifest(manifest: &ManagedHookManifest) -> Result<()> {
    if manifest.schema_version != MANAGED_HOOK_SCHEMA_VERSION
        || !is_safe_transaction_id(&manifest.transaction_id)
        || !is_sha256_hex(&manifest.host_config_fingerprint)
        || !is_sha256_hex(&manifest.bridge.sha256)
        || manifest.usage_dir != MANAGED_HOOK_USAGE_DIR
        || manifest.max_output_bytes.is_some_and(|limit| {
            !(MIN_MANAGED_RTK_OUTPUT_BYTES..=MAX_MANAGED_RTK_OUTPUT_BYTES).contains(&limit)
        })
        || manifest
            .filter_dsl_sha256
            .as_deref()
            .is_some_and(|digest| !is_sha256_hex(digest))
        || !is_safe_bridge_identity(&manifest.bridge)
    {
        bail!("managed RTK manifest contains invalid metadata");
    }
    Ok(())
}

fn is_safe_bridge_identity(identity: &ManagedHookBridgeIdentity) -> bool {
    identity.component == BRIDGE_COMPONENT
        && identity.protocol_version == 1
        && is_safe_identity_text(&identity.version, 128)
        && is_safe_identity_text(&identity.arch, 64)
        && is_safe_identity_text(&identity.os, 32)
}

fn is_safe_identity_text(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn manifest_matches_managed_root(manifest: &ManagedHookManifest, root: &Path) -> bool {
    manifest.managed_root == root.to_string_lossy()
}

fn verify_managed_bridge_identity(manifest: &ManagedHookManifest) -> Result<()> {
    let bridge_path = canonical_existing_file(Path::new(&manifest.bridge_path), "RTK bridge")?;
    if bridge_identity(&bridge_path)? != manifest.bridge {
        bail!("managed RTK bridge identity no longer matches its manifest");
    }
    Ok(())
}

fn read_managed_usage_evidence(
    root: &Path,
    manifest: &ManagedHookManifest,
) -> Result<(usize, u64, Option<ManagedRtkUsageReceipt>)> {
    let records = read_managed_usage_records(root, manifest)?;
    let mut saved_bytes = 0u64;
    let mut last_receipt = None;
    for record in &records {
        saved_bytes = saved_bytes
            .checked_add(record.saved_bytes)
            .context("managed RTK saved-byte total overflow")?;
        let receipt = record.receipt();
        if last_receipt
            .as_ref()
            .is_none_or(|last: &ManagedRtkUsageReceipt| {
                receipt.recorded_at_ms > last.recorded_at_ms
            })
        {
            last_receipt = Some(receipt);
        }
    }
    Ok((
        records.len(),
        saved_bytes,
        last_receipt,
    ))
}

/// One verified d-token-owned usage record, bound to its command digest.
/// Suitable for exclusive command-output accounting (PR-C4C): the digest is
/// the claim's `source_digest` and the byte counts are the claim's values.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VerifiedRtkUsageRecord {
    pub evidence: ManagedRtkUsageEvidence,
    pub command_sha256: String,
    pub recorded_at_ms: u64,
}

/// Canonical digest binding one accounting claim to a single verified usage
/// receipt. Includes the command digest, the receipt timestamp, the
/// transaction id, and the exact byte counts, so a legitimate repeated
/// invocation is its own distinct claim while a replayed (copied) receipt —
/// identical content — collapses to the same digest and is rejected by the
/// exclusive ledger's duplicate guard.
pub fn verified_usage_claim_digest(record: &VerifiedRtkUsageRecord) -> String {
    let mut digest = Sha256::new();
    digest.update(record.command_sha256.as_bytes());
    digest.update([0]);
    digest.update(record.recorded_at_ms.to_string().as_bytes());
    digest.update([0]);
    digest.update(record.evidence.transaction_id.as_bytes());
    digest.update([0]);
    digest.update(
        record
            .evidence
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_default()
            .as_bytes(),
    );
    digest.update([0]);
    digest.update(record.evidence.input_bytes.to_string().as_bytes());
    digest.update([0]);
    digest.update(record.evidence.output_bytes.to_string().as_bytes());
    digest.update([0]);
    digest.update(record.evidence.saved_bytes.to_string().as_bytes());
    format!("{:x}", digest.finalize())
}

/// Reads every verified usage record under a fully verified managed RTK root,
/// fail closed. Invalid, unknown, oversized, or symlinked artifacts abort the
/// whole read so no partial evidence ever reaches accounting.
pub fn verified_managed_rtk_usage(root: &Path) -> Result<Vec<VerifiedRtkUsageRecord>> {
    let verification = verify_managed_hook_root(root, false)?;
    let records = read_managed_usage_records(root, &verification.manifest)?;
    records
        .into_iter()
        .map(|record| {
            let input_bytes = usize::try_from(record.input_bytes)
                .context("managed RTK usage input bytes are not supported")?;
            let output_bytes = usize::try_from(record.output_bytes)
                .context("managed RTK usage output bytes are not supported")?;
            let saved_bytes = usize::try_from(record.saved_bytes)
                .context("managed RTK usage saved bytes are not supported")?;
            Ok(VerifiedRtkUsageRecord {
                evidence: ManagedRtkUsageEvidence {
                    schema_version: record.schema_version,
                    transaction_id: record.transaction_id,
                    exit_code: record.exit_code,
                    input_bytes,
                    output_bytes,
                    saved_bytes,
                    transforms: record.transforms,
                    disposition: record.disposition,
                },
                command_sha256: record.command_sha256,
                recorded_at_ms: record.recorded_at_ms,
            })
        })
        .collect()
}

fn read_managed_usage_records(
    root: &Path,
    manifest: &ManagedHookManifest,
) -> Result<Vec<ManagedRtkUsageRecord>> {
    let usage_dir = canonical_existing_directory(
        &root.join(MANAGED_HOOK_USAGE_DIR),
        "managed RTK usage directory",
    )?;
    let mut entries = fs::read_dir(&usage_dir)
        .context("managed RTK usage directory cannot be read")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("managed RTK usage directory contains an unreadable entry")?;
    if entries.len() > MAX_MANAGED_RTK_USAGE_ENTRIES {
        bail!("managed RTK usage entry limit exceeded");
    }
    entries.sort_by_key(|entry| entry.file_name());

    let mut total_bytes = 0usize;
    let mut records = Vec::with_capacity(entries.len());
    for entry in entries {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("managed RTK usage filename is not UTF-8"))?;
        if name.len() > 128 || !name.ends_with(".json") {
            bail!("managed RTK usage directory contains an unknown entry");
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).with_context(|| {
            format!(
                "managed RTK usage metadata is unavailable: {}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("managed RTK usage entries must be real regular files");
        }
        let size =
            usize::try_from(metadata.len()).context("managed RTK usage record is too large")?;
        if size > MAX_MANAGED_RTK_USAGE_RECORD_BYTES {
            bail!("managed RTK usage record exceeds its size limit");
        }
        total_bytes = total_bytes
            .checked_add(size)
            .context("managed RTK usage total size overflow")?;
        if total_bytes > MAX_MANAGED_RTK_USAGE_TOTAL_BYTES {
            bail!("managed RTK usage total size limit exceeded");
        }
        let bytes = read_regular_file_bounded(&path, MAX_MANAGED_RTK_USAGE_RECORD_BYTES)?;
        let record: ManagedRtkUsageRecord =
            serde_json::from_slice(&bytes).context("managed RTK usage record is invalid")?;
        validate_managed_usage_record(&record, manifest)?;
        records.push(record);
    }
    Ok(records)
}

fn validate_managed_usage_record(
    record: &ManagedRtkUsageRecord,
    manifest: &ManagedHookManifest,
) -> Result<()> {
    let expected_saved = record
        .input_bytes
        .checked_sub(record.output_bytes)
        .context("managed RTK usage bytes are inconsistent")?;
    if record.schema_version != MANAGED_RTK_USAGE_SCHEMA_VERSION
        || record.transaction_id != manifest.transaction_id
        || record.saved_bytes != expected_saved
        || record.disposition != "filtered_verified_hook"
        || !is_safe_metadata_text(&record.command_basename, 128)
        || !is_sha256_hex(&record.command_sha256)
        || record.bridge_sha256 != manifest.bridge.sha256
        || record.bridge_version != manifest.bridge.version
        || record.transforms.len() > MAX_MANAGED_RTK_TRANSFORMS
        || !record
            .transforms
            .iter()
            .all(|transform| is_allowed_usage_transform(transform))
    {
        bail!("managed RTK usage record contains invalid metadata");
    }
    Ok(())
}

fn is_safe_metadata_text(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
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
/// the reviewed builtin can emit, so a wrapped command's usage record can
/// never be invalidated by a legitimately reviewed filter action.
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

fn read_regular_file_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("managed RTK metadata is unavailable: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("managed RTK artifact must be a real regular file");
    }
    if metadata.len() > u64::try_from(max_bytes).unwrap_or(u64::MAX) {
        bail!("managed RTK artifact exceeds its size limit");
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(max_bytes));
    fs::File::open(path)
        .with_context(|| format!("failed to read managed RTK artifact {}", path.display()))?
        .take(
            u64::try_from(max_bytes)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
        .read_to_end(&mut bytes)
        .context("failed to read managed RTK artifact")?;
    if bytes.len() > max_bytes {
        bail!("managed RTK artifact exceeds its size limit");
    }
    Ok(bytes)
}

pub fn verify_managed_hook_root(
    root: &Path,
    require_current_exe_match: bool,
) -> Result<ManagedHookVerification> {
    let root = canonical_existing_directory(root, "managed RTK root")?;
    let manifest_path = managed_hook_manifest_path(&root);
    let manifest = read_managed_hook_manifest(&manifest_path)?
        .context("managed RTK hook manifest is unavailable")?;
    if !manifest_matches_managed_root(&manifest, &root) {
        bail!("managed RTK hook manifest does not own the requested root");
    }
    validate_private_directory(&root, "managed RTK root")?;
    let bridge_path = canonical_existing_file(Path::new(&manifest.bridge_path), "RTK bridge")?;
    let actual_identity = bridge_identity(&bridge_path)?;
    if actual_identity != manifest.bridge {
        bail!("managed RTK hook bridge identity no longer matches the reviewed manifest");
    }
    if require_current_exe_match {
        let current_exe = std::env::current_exe().context("resolve current bridge executable")?;
        let current_exe = fs::canonicalize(&current_exe)
            .with_context(|| format!("canonicalize {}", current_exe.display()))?;
        if current_exe != bridge_path {
            bail!("managed RTK hook is being used by an unexpected bridge executable");
        }
    }
    let filter_path = root.join(MANAGED_HOOK_FILTER_FILE);
    match &manifest.filter_dsl_sha256 {
        Some(expected) => {
            let actual = hash_file(&filter_path).with_context(|| {
                format!(
                    "managed RTK filter DSL is unavailable: {}",
                    filter_path.display()
                )
            })?;
            if &actual != expected {
                bail!("managed RTK filter DSL no longer matches the reviewed manifest");
            }
        }
        None => match fs::symlink_metadata(&filter_path) {
            Ok(_) => bail!("managed RTK root contains an unexpected filter DSL artifact"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "managed RTK filter DSL metadata is unavailable: {}",
                        filter_path.display()
                    )
                })
            }
        },
    }
    let usage_dir = canonical_existing_directory(
        &root.join(MANAGED_HOOK_USAGE_DIR),
        "managed RTK usage directory",
    )?;
    validate_private_directory(&usage_dir, "managed RTK usage directory")?;
    Ok(ManagedHookVerification {
        manifest,
        manifest_path,
        usage_dir,
    })
}

fn write_and_verify_managed_hook_manifest(
    plan: &ManagedRtkTransactionPlan,
    paths: &ManagedHookPaths,
) -> Result<ManagedHookVerification> {
    let managed_root = validate_managed_root(&paths.managed_root)?;
    let bridge_path = canonical_existing_file(&paths.bridge_path, "RTK bridge")?;
    let usage_dir = managed_root.join(MANAGED_HOOK_USAGE_DIR);
    fs::create_dir_all(&usage_dir).with_context(|| {
        format!(
            "failed to create managed RTK usage directory {}",
            usage_dir.display()
        )
    })?;
    ensure_private_directory_permissions(&usage_dir)?;
    let filter_dsl_sha256 = match paths.filter_dsl.as_ref() {
        Some(text) => {
            let config = filters::CommandFilterConfig::from_toml(text)
                .context("managed RTK filter DSL is invalid")?;
            if config.schema_version != filters::FILTER_SCHEMA_VERSION {
                bail!("managed RTK filter DSL schema version is unsupported");
            }
            if text.len() > MAX_MANAGED_RTK_FILTER_BYTES {
                bail!("managed RTK filter DSL exceeds its size limit");
            }
            let filter_path = managed_root.join(MANAGED_HOOK_FILTER_FILE);
            fs::write(&filter_path, text)
                .with_context(|| format!("failed to write {}", filter_path.display()))?;
            ensure_private_file_permissions(&filter_path)?;
            Some(hash_file(&filter_path)?)
        }
        None => None,
    };
    let manifest = ManagedHookManifest {
        schema_version: MANAGED_HOOK_SCHEMA_VERSION,
        transaction_id: plan.transaction_id.clone(),
        platform: plan.platform,
        host_config_fingerprint: plan.host_config_fingerprint.clone(),
        managed_root: managed_root.display().to_string(),
        bridge_path: bridge_path.display().to_string(),
        bridge: bridge_identity(&bridge_path)?,
        max_output_bytes: paths.max_output_bytes,
        filter_dsl_sha256,
        usage_dir: MANAGED_HOOK_USAGE_DIR.to_string(),
    };
    validate_managed_hook_manifest(&manifest)?;
    let manifest_path = managed_hook_manifest_path(&managed_root);
    write_json_atomic(&manifest_path, &manifest)?;
    verify_managed_hook_root(&managed_root, false)
}

fn validate_managed_root(root: &Path) -> Result<PathBuf> {
    if !root.is_absolute() {
        bail!("managed RTK root must be absolute");
    }
    reject_parent_components(root)?;
    fs::create_dir_all(root)
        .with_context(|| format!("failed to create managed RTK root {}", root.display()))?;
    ensure_private_directory_permissions(root)?;
    canonical_existing_directory(root, "managed RTK root")
}

/// Makes a d-token-owned file private (0600 on Unix) so the filter DSL is
/// never readable or writable by group/other users. No-op on platforms
/// without POSIX permission bits.
fn ensure_private_file_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to secure permissions on {}", path.display()))?;
    }
    Ok(())
}

/// Makes a d-token-owned directory private (0700 on Unix) so usage receipts
/// and the wrapper manifest are never readable or writable by group/other
/// users. No-op on platforms without POSIX permission bits.
fn ensure_private_directory_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to secure permissions on {}", path.display()))?;
    }
    Ok(())
}

/// Rejects a managed RTK directory whose POSIX ACL exposes it to group/other
/// users (any group or other permission bit). This is the ACL half of the
/// private tee contract: usage receipts and the manifest must stay private
/// even when the caller re-verifies an existing root. No-op on platforms
/// without POSIX permission bits.
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
                "{label} is not private (mode {mode:o}); managed RTK artifacts must not be exposed to group or other users"
            );
        }
    }
    Ok(())
}

/// Executes the RTK bridge `identity` command, retrying the transient Linux
/// `Text file busy` (ETXTBSY) failure that can occur when a bridge copy has
/// just been written and is executed concurrently (parallel workspace tests).
/// Other spawn errors surface immediately; the retry is bounded so genuine
/// failures still fail closed.
fn execute_bridge_identity_with_etxtbsy_retry(path: &Path) -> Result<std::process::Output> {
    use std::io::ErrorKind;
    use std::time::Duration;

    let mut attempt = 0u32;
    loop {
        match StdCommand::new(path).arg("identity").output() {
            Ok(output) => return Ok(output),
            Err(error)
                if error.kind() == ErrorKind::ExecutableFileBusy
                    && attempt < MAX_BRIDGE_EXEC_ATTEMPTS =>
            {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(50 * u64::from(attempt)));
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to execute RTK bridge {}", path.display()))
            }
        }
    }
}

fn bridge_identity(path: &Path) -> Result<ManagedHookBridgeIdentity> {
    let output = execute_bridge_identity_with_etxtbsy_retry(path)?;
    if !output.status.success() {
        bail!("RTK bridge identity command failed");
    }
    #[derive(Deserialize)]
    struct IdentityResponse {
        component: String,
        protocol_version: u32,
        version: String,
        arch: String,
        os: String,
    }
    let response: IdentityResponse = serde_json::from_slice(&output.stdout)
        .context("RTK bridge identity output is not valid JSON")?;
    if response.component != BRIDGE_COMPONENT {
        bail!("unexpected RTK bridge component identity");
    }
    Ok(ManagedHookBridgeIdentity {
        component: response.component,
        protocol_version: response.protocol_version,
        version: response.version,
        arch: response.arch,
        os: response.os,
        sha256: hash_file(path)?,
    })
}

fn remove_managed_hook_manifest(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn remove_managed_hook_filter(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .context("managed RTK manifest path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value).context("serialize managed RTK manifest")?;
    fs::write(&temp, bytes).with_context(|| format!("failed to write {}", temp.display()))?;
    fs::rename(&temp, path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn canonical_existing_file(path: &Path, label: &str) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("{label} path must be absolute");
    }
    reject_parent_components(path)?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("{label} is unavailable: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("{label} must be a real regular file");
    }
    fs::canonicalize(path).with_context(|| format!("failed to canonicalize {label}"))
}

fn canonical_existing_directory(path: &Path, label: &str) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("{label} path must be absolute");
    }
    reject_parent_components(path)?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("{label} is unavailable: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("{label} must be a real directory");
    }
    fs::canonicalize(path).with_context(|| format!("failed to canonicalize {label}"))
}

fn reject_parent_components(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("managed RTK paths may not contain parent-directory traversal");
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_safe_transaction_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub(crate) fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        } else {
            let rest = &input[index..];
            let character = rest.chars().next().expect("valid UTF-8 boundary");
            output.push(character);
            index += character.len_utf8();
        }
    }
    output
}

#[derive(Debug, Clone, Default)]
pub struct RtkAdapter {
    pub binary_path: Option<String>,
}

impl RtkAdapter {
    pub fn new(binary_path: Option<String>) -> Self {
        Self { binary_path }
    }

    pub async fn detect() -> RtkStatus {
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return fixture_status("fixture");
        }
        if mode == RuntimeMode::Native {
            return native_builtin_status();
        }

        let adapter = Self::default();
        match adapter.run_version().await {
            Some(version) => RtkStatus {
                status: ToolStatus::Found,
                binary_path: Some(adapter.binary_name()),
                version: Some(version),
                install_method: Some("external_path".to_string()),
                hook_status: HookStatus::Unknown,
                integrity_state: IntegrityState::Unknown,
                telemetry_status: TelemetryStatus::Unknown,
                warnings: vec![
                    "M1 only detects RTK binary/version; hook and analytics import are skeletons"
                        .to_string(),
                ],
            },
            None => RtkStatus {
                status: ToolStatus::Missing,
                binary_path: None,
                version: None,
                install_method: None,
                hook_status: HookStatus::Missing,
                integrity_state: IntegrityState::NotInstalled,
                telemetry_status: TelemetryStatus::Unknown,
                warnings: vec!["rtk binary was not found in PATH".to_string()],
            },
        }
    }

    pub async fn gain_report(&self) -> RtkGainReport {
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return fixture_gain_report();
        }
        if mode == RuntimeMode::Native {
            return RtkGainReport {
                available: false,
                total_commands: 0,
                input_tokens: 0,
                output_tokens: 0,
                saved_tokens: 0,
                savings_percent: 0.0,
                source: "native_unavailable".to_string(),
            };
        }

        match self
            .run_json_command(&["gain", "--all", "--format", "json"])
            .await
        {
            Some(raw) => {
                serde_json::from_str::<RtkGainReport>(&raw).unwrap_or_else(|_| RtkGainReport {
                    available: true,
                    total_commands: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    saved_tokens: 0,
                    savings_percent: 0.0,
                    source: format!("{:?}_unparsed_gain", mode).to_lowercase(),
                })
            }
            None => RtkGainReport {
                available: false,
                total_commands: 0,
                input_tokens: 0,
                output_tokens: 0,
                saved_tokens: 0,
                savings_percent: 0.0,
                source: format!("{:?}_missing_rtk", mode).to_lowercase(),
            },
        }
    }

    pub async fn discover_report(&self) -> RtkDiscoverReport {
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return fixture_discover_report();
        }
        if mode == RuntimeMode::Native {
            return RtkDiscoverReport {
                available: false,
                source: "native_unavailable".to_string(),
                missed_opportunities: vec![],
                raw_summary: None,
            };
        }

        match self.run_text_command(&["discover", "--all"]).await {
            Some(raw) => parse_discover_text("external", &raw),
            None => RtkDiscoverReport {
                available: false,
                source: "external_missing_rtk".to_string(),
                missed_opportunities: vec![],
                raw_summary: None,
            },
        }
    }

    pub async fn session_report(&self) -> RtkSessionReport {
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return fixture_session_report();
        }
        if mode == RuntimeMode::Native {
            return RtkSessionReport {
                available: false,
                source: "native_unavailable".to_string(),
                sessions: vec![],
                raw_summary: None,
            };
        }

        match self.run_text_command(&["session"]).await {
            Some(raw) => parse_session_text("external", &raw),
            None => RtkSessionReport {
                available: false,
                source: "external_missing_rtk".to_string(),
                sessions: vec![],
                raw_summary: None,
            },
        }
    }

    pub fn profile_from_status(status: &RtkStatus) -> PreprocessorProfile {
        PreprocessorProfile {
            id: "rtk-default".to_string(),
            name: "External RTK diagnostic".to_string(),
            implementation: PreprocessorImplementation::RtkExternal,
            binary_path: status.binary_path.clone(),
            version: status.version.clone(),
            agent_hook_status: status.hook_status.clone(),
            telemetry_status: status.telemetry_status.clone(),
            config_path: None,
            tracking_db_path: None,
            tee_dir: None,
            supported_agents: vec![
                "claude-code".to_string(),
                "codex".to_string(),
                "cursor".to_string(),
                "windsurf".to_string(),
                "cline".to_string(),
            ],
        }
    }

    async fn run_version(&self) -> Option<String> {
        let output = Command::new(self.binary_name())
            .arg("--version")
            .output()
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            Some("unknown".to_string())
        } else {
            Some(stdout)
        }
    }

    async fn run_json_command(&self, args: &[&str]) -> Option<String> {
        self.run_text_command(args).await
    }

    async fn run_text_command(&self, args: &[&str]) -> Option<String> {
        let output = Command::new(self.binary_name())
            .args(args)
            .output()
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn binary_name(&self) -> String {
        self.binary_path
            .clone()
            .unwrap_or_else(|| "rtk".to_string())
    }
}

pub async fn detect_rtk_status() -> RtkStatus {
    RtkAdapter::detect().await
}

pub async fn rtk_gain_mock() -> RtkGainReport {
    RtkAdapter::default().gain_report().await
}

pub async fn rtk_discover_report() -> RtkDiscoverReport {
    RtkAdapter::default().discover_report().await
}

pub async fn rtk_session_report() -> RtkSessionReport {
    RtkAdapter::default().session_report().await
}

pub async fn list_preprocessors() -> Vec<PreprocessorProfile> {
    let status = detect_rtk_status().await;
    let mut profiles = vec![builtin_preprocessor_profile()];
    // Never convert the d-token builtin fallback into an external or managed
    // RTK profile. The latter would let callers infer an installed hook from a
    // `Found` status that only means the local output filter exists.
    if should_expose_external_diagnostic(&status) {
        profiles.push(RtkAdapter::profile_from_status(&status));
    }
    profiles
}

fn builtin_preprocessor_profile() -> PreprocessorProfile {
    PreprocessorProfile {
        id: "dtoken-builtin-command-filter".to_string(),
        name: "d-token command output filter".to_string(),
        implementation: PreprocessorImplementation::DtokenBuiltin,
        binary_path: None,
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        agent_hook_status: HookStatus::Unsupported,
        telemetry_status: TelemetryStatus::Disabled,
        config_path: None,
        tracking_db_path: None,
        tee_dir: None,
        supported_agents: vec![],
    }
}

fn should_expose_external_diagnostic(status: &RtkStatus) -> bool {
    status.status == ToolStatus::Found && status.install_method.as_deref() == Some("external_path")
}

fn runtime_mode_from_env() -> RuntimeMode {
    let profile = env::var("DTOKEN_PROFILE").ok();
    RuntimeMode::from_profile(profile.as_deref(), cfg!(debug_assertions))
}

fn native_builtin_status() -> RtkStatus {
    RtkStatus {
        status: ToolStatus::Found,
        binary_path: None,
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        install_method: Some("dtoken_builtin".to_string()),
        hook_status: HookStatus::Unsupported,
        integrity_state: IntegrityState::Verified,
        telemetry_status: TelemetryStatus::Unknown,
        warnings: vec![
            "d-token built-in command filtering is available; managed Agent hook/wrap and raw-output recovery remain unavailable".to_string(),
        ],
    }
}

fn fixture_status(source: &str) -> RtkStatus {
    let version =
        read_fixture_text("rtk/version.txt").unwrap_or_else(|| "rtk 0.0.0-fixture".to_string());
    RtkStatus {
        status: ToolStatus::Found,
        binary_path: Some("support/fake-bin/rtk".to_string()),
        version: Some(version.trim().to_string()),
        install_method: Some(source.to_string()),
        hook_status: HookStatus::Installed,
        integrity_state: IntegrityState::Verified,
        telemetry_status: TelemetryStatus::Disabled,
        warnings: vec!["fixture mode: real RTK was not called".to_string()],
    }
}

fn fixture_gain_report() -> RtkGainReport {
    read_fixture_text("rtk/gain-all.json")
        .and_then(|raw| serde_json::from_str::<RtkGainReport>(&raw).ok())
        .unwrap_or(RtkGainReport {
            available: true,
            total_commands: 0,
            input_tokens: 0,
            output_tokens: 0,
            saved_tokens: 0,
            savings_percent: 0.0,
            source: "fixture_fallback".to_string(),
        })
}

fn fixture_discover_report() -> RtkDiscoverReport {
    let raw = read_fixture_text("rtk/discover.txt").unwrap_or_default();
    parse_discover_text("fixture", &raw)
}

fn fixture_session_report() -> RtkSessionReport {
    let raw = read_fixture_text("rtk/session.txt").unwrap_or_default();
    parse_session_text("fixture", &raw)
}

fn parse_discover_text(source: &str, raw: &str) -> RtkDiscoverReport {
    let missed_opportunities = raw
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    RtkDiscoverReport {
        available: true,
        source: source.to_string(),
        missed_opportunities,
        raw_summary: Some(raw.to_string()),
    }
}

fn parse_session_text(source: &str, raw: &str) -> RtkSessionReport {
    let sessions = raw
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    RtkSessionReport {
        available: true,
        source: source.to_string(),
        sessions,
        raw_summary: Some(raw.to_string()),
    }
}

fn read_fixture_text(relative: &str) -> Option<String> {
    fixture_candidates(relative)
        .into_iter()
        .find_map(|path| fs::read_to_string(path).ok())
}

fn fixture_candidates(relative: &str) -> Vec<PathBuf> {
    let root = env::var("DTOKEN_FIXTURE_ROOT").ok();
    let mut candidates = Vec::new();
    if let Some(root) = root {
        candidates.push(PathBuf::from(root).join(relative));
    }
    candidates.push(PathBuf::from("../support/fixtures").join(relative));
    candidates.push(PathBuf::from("support/fixtures").join(relative));
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fake_bridge_path() -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .expect("repo root")
            .to_path_buf();
        if cfg!(windows) {
            root.join("support/fake-bin/dtoken-agent-bridge.cmd")
        } else {
            root.join("support/fake-bin/dtoken-agent-bridge")
        }
    }

    #[test]
    fn discover_fixture_text_extracts_missed_opportunities() {
        let report = parse_discover_text("fixture", "- cargo test --workspace\nsource=fixture");
        assert_eq!(report.source, "fixture");
        assert_eq!(report.missed_opportunities, vec!["cargo test --workspace"]);
    }

    #[test]
    fn missing_or_unknown_profile_defaults_to_native_and_fixture_is_explicit() {
        env::remove_var("DTOKEN_PROFILE");
        assert_eq!(runtime_mode_from_env(), RuntimeMode::Native);

        env::set_var("DTOKEN_PROFILE", "fixture");
        assert_eq!(
            runtime_mode_from_env(),
            if cfg!(debug_assertions) {
                RuntimeMode::Fixture
            } else {
                RuntimeMode::Native
            }
        );

        env::set_var("DTOKEN_PROFILE", "unexpected-profile");
        assert_eq!(runtime_mode_from_env(), RuntimeMode::Native);
        env::remove_var("DTOKEN_PROFILE");
    }

    #[test]
    fn builtin_filter_is_bounded_and_never_silently_truncates() {
        let result =
            preprocess_command_output("\u{1b}[31merror\u{1b}[0m\n\nerror\nerror\n", Some(80));
        assert!(result.output.contains("error"));
        assert!(result.output.contains("previous line repeated"));
        assert!(!result.output.contains("\u{1b}[31m"));
        assert!(result.transforms.contains(&"ansi_strip"));
        assert!(result.transforms.contains(&"blank_and_repeat_fold"));

        let capped = preprocess_command_output("0123456789abcdef", Some(8));
        assert!(capped.truncated);
        assert!(capped.output.contains('~'));
        assert!(capped.output.len() <= 8);
        assert!(capped.transforms.contains(&"bounded_truncate"));
    }

    #[test]
    fn native_status_exposes_only_builtin_capability() {
        let status = native_builtin_status();
        assert_eq!(status.status, ToolStatus::Found);
        assert_eq!(status.install_method.as_deref(), Some("dtoken_builtin"));
        assert_eq!(status.hook_status, HookStatus::Unsupported);
        assert!(status.warnings.join(" ").contains("hook/wrap"));

        let builtin = builtin_preprocessor_profile();
        assert_eq!(
            builtin.implementation,
            PreprocessorImplementation::DtokenBuiltin
        );
        assert_eq!(builtin.agent_hook_status, HookStatus::Unsupported);
        assert!(!should_expose_external_diagnostic(&status));
    }

    #[test]
    fn only_real_external_detection_may_appear_as_external_diagnostic() {
        let external = RtkStatus {
            status: ToolStatus::Found,
            binary_path: Some("rtk".to_string()),
            version: Some("external-version".to_string()),
            install_method: Some("external_path".to_string()),
            hook_status: HookStatus::Unknown,
            integrity_state: IntegrityState::Unknown,
            telemetry_status: TelemetryStatus::Unknown,
            warnings: vec![],
        };
        assert!(should_expose_external_diagnostic(&external));
        assert_eq!(
            RtkAdapter::profile_from_status(&external).name,
            "External RTK diagnostic"
        );
    }

    #[test]
    fn managed_rtk_unconfigured_is_unavailable_not_blocked_on_all_platforms() {
        let fingerprint = "a".repeat(64);
        for platform in [
            HostPlatform::Macos,
            HostPlatform::Windows,
            HostPlatform::Linux,
        ] {
            let lifecycle = ManagedRtkLifecycle::plan("rtk-transaction.1", platform, &fingerprint);
            assert_eq!(lifecycle.state(), ManagedRtkTransactionState::Planned);
            assert_eq!(
                lifecycle.transaction_plan().capability.hook_status,
                HookStatus::Partial
            );
            assert!(
                !lifecycle
                    .transaction_plan()
                    .capability
                    .can_modify_agent_config
            );
            assert!(!lifecycle.command_rewrite().use_managed_wrapper);

            // The managed runtime is unconfigured: no operation other than
            // Install is available, and none is ever hard-Blocked.
            let install = lifecycle.request_operation(ManagedRtkOperation::Install);
            assert_eq!(install.state, ManagedRtkTransactionState::Installed);
            assert_eq!(install.host_config_fingerprint, fingerprint);
            assert!(!install.summary.contains("blocked"));

            for operation in [
                ManagedRtkOperation::Configure,
                ManagedRtkOperation::Verify,
                ManagedRtkOperation::Rollback,
                ManagedRtkOperation::Uninstall,
            ] {
                let receipt = lifecycle.request_operation(operation);
                assert_eq!(
                    receipt.state,
                    ManagedRtkTransactionState::Unavailable,
                    "{operation:?} on an unconfigured runtime must be unavailable"
                );
                assert_eq!(receipt.host_config_fingerprint, fingerprint);
                assert!(!receipt.summary.contains("blocked"));
                assert!(receipt.summary.contains("unavailable"));
            }
        }
    }

    #[test]
    fn managed_rtk_preview_transitions_track_the_lifecycle_state_machine() {
        let root = tempdir().expect("managed RTK tempdir");
        let managed_root = root.path().join("managed-rtk");
        let mut lifecycle = ManagedRtkLifecycle::plan(
            "rtk-transaction.preview",
            HostPlatform::Linux,
            &"9".repeat(64),
        );
        // Unconfigured: only Install is available.
        assert_eq!(
            lifecycle
                .request_operation(ManagedRtkOperation::Install)
                .state,
            ManagedRtkTransactionState::Installed
        );
        assert_eq!(
            lifecycle
                .request_operation(ManagedRtkOperation::Verify)
                .state,
            ManagedRtkTransactionState::Unavailable
        );
        lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: Some(1_024),
                filter_dsl: None,
            })
            .expect("install managed RTK hook");
        // Installed: verify/rollback/uninstall available; install is not.
        assert_eq!(
            lifecycle
                .request_operation(ManagedRtkOperation::Verify)
                .state,
            ManagedRtkTransactionState::Verified
        );
        assert_eq!(
            lifecycle
                .request_operation(ManagedRtkOperation::Install)
                .state,
            ManagedRtkTransactionState::Unavailable
        );
        lifecycle.verify_hook().expect("verify managed RTK hook");
        assert_eq!(
            lifecycle
                .request_operation(ManagedRtkOperation::Rollback)
                .state,
            ManagedRtkTransactionState::RolledBack
        );
        assert_eq!(
            lifecycle
                .request_operation(ManagedRtkOperation::Uninstall)
                .state,
            ManagedRtkTransactionState::Uninstalled
        );
    }

    #[cfg(unix)]
    #[test]
    fn managed_rtk_artifacts_are_private_and_exposed_roots_fail_closed() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempdir().expect("managed RTK tempdir");
        let managed_root = root.path().join("managed-rtk");
        let mut lifecycle =
            ManagedRtkLifecycle::plan("rtk-transaction.acl", HostPlatform::Macos, &"8".repeat(64));
        lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: None,
                filter_dsl: None,
            })
            .expect("install managed RTK hook");

        let root_mode = fs::metadata(&managed_root)
            .expect("managed root metadata")
            .permissions()
            .mode();
        assert_eq!(root_mode & 0o077, 0, "managed root must be private");
        let usage_mode = fs::metadata(managed_root.join("usage"))
            .expect("usage metadata")
            .permissions()
            .mode();
        assert_eq!(usage_mode & 0o077, 0, "usage dir must be private");

        // An exposed (group/other-readable) root fails closed.
        fs::set_permissions(&managed_root, fs::Permissions::from_mode(0o755))
            .expect("expose managed root");
        let error = verify_managed_hook_root(&managed_root, false).expect_err("exposed root");
        assert!(error.to_string().contains("not private"));
    }

    #[test]
    fn command_output_accounting_entry_is_bucket_bound_and_fails_closed() {
        let evidence = ManagedRtkUsageEvidence {
            schema_version: 1,
            transaction_id: "rtk-tx".to_string(),
            exit_code: Some(0),
            input_bytes: 100,
            output_bytes: 40,
            saved_bytes: 60,
            transforms: vec!["stdout:ansi_strip".to_string()],
            disposition: "filtered_verified_hook".to_string(),
        };
        let digest = "a".repeat(64);
        let entry = command_output_accounting_entry(&evidence, &digest, 1_000)
            .expect("valid evidence maps to an entry");
        assert_eq!(entry.bucket, AccountingBucket::CommandOutput);
        assert_eq!(entry.source_digest, digest);
        assert_eq!(entry.saved_bytes, 60);

        let inconsistent = ManagedRtkUsageEvidence {
            saved_bytes: 50,
            ..evidence.clone()
        };
        assert!(
            command_output_accounting_entry(&inconsistent, &digest, 1_000).is_err(),
            "inconsistent bytes must fail closed"
        );
        assert!(
            command_output_accounting_entry(&evidence, "not-a-digest", 1_000).is_err(),
            "non-sha256 digest must fail closed"
        );
    }

    #[test]
    fn managed_rtk_manifest_install_verify_rewrite_and_uninstall_are_dtoken_owned() {
        let root = tempdir().expect("managed RTK tempdir");
        let mut lifecycle =
            ManagedRtkLifecycle::plan("rtk-transaction.2", HostPlatform::Macos, &"c".repeat(64));
        let install = lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: root.path().join("managed-rtk"),
                bridge_path: fake_bridge_path(),
                max_output_bytes: Some(1_024),
                filter_dsl: None,
            })
            .expect("install managed RTK hook");
        assert_eq!(install.state, ManagedRtkTransactionState::Installed);
        let verify = lifecycle.verify_hook().expect("verify managed RTK hook");
        assert_eq!(verify.state, ManagedRtkTransactionState::Verified);

        let rewrite = lifecycle.command_rewrite();
        assert!(rewrite.use_managed_wrapper);
        // install_hook persists the canonical bridge path in the manifest, so
        // compare against the canonicalized fixture path (on Windows
        // fs::canonicalize returns a \\?\ extended-length path).
        let canonical_fake_bridge =
            fs::canonicalize(fake_bridge_path()).expect("canonical fake bridge");
        assert_eq!(
            rewrite.executable.as_deref(),
            Some(canonical_fake_bridge.as_path())
        );
        let managed_root =
            fs::canonicalize(root.path().join("managed-rtk")).expect("canonical root");
        let managed_root_text = managed_root.display().to_string();
        let flag_index = rewrite
            .args_prefix
            .iter()
            .position(|value| value == "--managed-root")
            .expect("managed-root flag");
        assert_eq!(
            rewrite.args_prefix.get(flag_index + 1).map(String::as_str),
            Some(managed_root_text.as_str())
        );
        assert!(managed_hook_manifest_path(&managed_root).exists());

        let uninstall = lifecycle
            .uninstall_hook()
            .expect("uninstall managed RTK hook");
        assert_eq!(uninstall.state, ManagedRtkTransactionState::Uninstalled);
        assert!(!managed_hook_manifest_path(&managed_root).exists());
        assert!(!lifecycle.command_rewrite().use_managed_wrapper);
    }

    #[test]
    fn managed_rtk_manifest_rejects_unreviewed_output_limits_before_writing() {
        let root = tempdir().expect("managed RTK tempdir");
        let managed_root = root.path().join("managed-rtk");
        let mut lifecycle = ManagedRtkLifecycle::plan(
            "rtk-transaction.limit",
            HostPlatform::Macos,
            &"c".repeat(64),
        );

        let error = lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: Some(1_023),
                filter_dsl: None,
            })
            .expect_err("sub-1KiB output limit must fail closed");

        assert!(error.to_string().contains("invalid metadata"));
        assert!(!managed_hook_manifest_path(&managed_root).exists());
    }

    #[test]
    fn verified_claim_digest_is_per_invocation_and_replay_guarded() {
        fn record(recorded_at_ms: u64) -> VerifiedRtkUsageRecord {
            VerifiedRtkUsageRecord {
                evidence: ManagedRtkUsageEvidence {
                    schema_version: 1,
                    transaction_id: "rtk-claim.1".to_string(),
                    exit_code: Some(0),
                    input_bytes: 100,
                    output_bytes: 40,
                    saved_bytes: 60,
                    transforms: vec!["stdout:json:minify".to_string()],
                    disposition: "filtered_verified_hook".to_string(),
                },
                command_sha256: "3".repeat(64),
                recorded_at_ms,
            }
        }
        // Two genuinely distinct invocations (different timestamps) are two
        // distinct claims: both can be counted.
        let invocation_a = record(1_000);
        let invocation_b = record(2_000);
        let digest_a = verified_usage_claim_digest(&invocation_a);
        let digest_b = verified_usage_claim_digest(&invocation_b);
        assert_ne!(digest_a, digest_b);
        assert!(is_sha256_hex(&digest_a));
        // An exact duplicate (replayed) receipt has an identical digest: the
        // exclusive ledger's duplicate guard rejects it.
        let replay = record(1_000);
        assert_eq!(verified_usage_claim_digest(&replay), digest_a);
    }

    #[test]
    fn managed_rtk_root_uninstall_is_verified_bounded_and_idempotent() {
        let root = tempdir().expect("managed RTK tempdir");
        let managed_root = root.path().join("managed-rtk");
        let mut lifecycle = ManagedRtkLifecycle::plan(
            "rtk-transaction.uninstall",
            HostPlatform::Macos,
            &"e".repeat(64),
        );
        lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: Some(1_024),
                filter_dsl: None,
            })
            .expect("install managed RTK hook");
        lifecycle.verify_hook().expect("verify managed RTK hook");

        let receipt =
            uninstall_managed_rtk_runtime(&managed_root).expect("uninstall verified RTK root");
        assert_eq!(receipt.state, ManagedRtkTransactionState::Uninstalled);
        assert_eq!(receipt.transaction_id, "rtk-transaction.uninstall");
        assert!(!managed_hook_manifest_path(&managed_root).exists());

        let idempotent =
            uninstall_managed_rtk_runtime(&managed_root).expect("repeat RTK uninstall");
        assert_eq!(idempotent.state, ManagedRtkTransactionState::Uninstalled);
        assert_eq!(idempotent.transaction_id, "already-uninstalled");
    }

    #[test]
    fn unverified_managed_hook_passthrough_preserves_exit_and_does_not_store_output() {
        const RAW_OUTPUT: &str = "API_KEY=must-not-enter-usage-evidence\n\u{1b}[31merror\u{1b}[0m";
        let lifecycle = ManagedRtkLifecycle::plan("rtk-tx-2", HostPlatform::Macos, &"b".repeat(64));

        let result = lifecycle.filter_command_output(RAW_OUTPUT, Some(17), Some(1));
        assert_eq!(result.output, RAW_OUTPUT);
        assert_eq!(result.evidence.exit_code, Some(17));
        assert_eq!(result.evidence.input_bytes, RAW_OUTPUT.len());
        assert_eq!(result.evidence.output_bytes, RAW_OUTPUT.len());
        assert_eq!(result.evidence.saved_bytes, 0);
        assert_eq!(result.evidence.disposition, "passthrough_unverified_hook");
        let evidence_json = serde_json::to_string(&result.evidence).expect("serialize evidence");
        assert!(!evidence_json.contains(RAW_OUTPUT));
        assert!(!evidence_json.contains("API_KEY"));
    }

    #[test]
    fn unsafe_transaction_metadata_fails_closed_without_echoing_input() {
        let lifecycle = ManagedRtkLifecycle::plan(
            "unsafe transaction\nsecret",
            HostPlatform::Windows,
            "not-a-fingerprint",
        );
        assert_eq!(lifecycle.state(), ManagedRtkTransactionState::Failed);
        let receipt = lifecycle.request_operation(ManagedRtkOperation::Install);
        assert_eq!(receipt.state, ManagedRtkTransactionState::Failed);
        assert_eq!(receipt.transaction_id, "invalid-transaction-id");
        assert_eq!(receipt.host_config_fingerprint, "invalid-fingerprint");
        assert!(!receipt.summary.contains("secret"));
    }

    #[test]
    fn managed_hook_verification_rejects_bridge_tampering() {
        let root = tempdir().expect("managed RTK tempdir");
        let managed_root = root.path().join("managed-rtk");
        let mut lifecycle =
            ManagedRtkLifecycle::plan("rtk-transaction.3", HostPlatform::Windows, &"d".repeat(64));
        lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: Some(1_024),
                filter_dsl: None,
            })
            .expect("install manifest");

        let manifest_path = managed_hook_manifest_path(&managed_root);
        let mut manifest: ManagedHookManifest =
            serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read manifest"))
                .expect("parse manifest");
        manifest.bridge.sha256 = "0".repeat(64);
        write_json_atomic(&manifest_path, &manifest).expect("rewrite manifest");

        let error = lifecycle
            .verify_hook()
            .expect_err("tampered bridge manifest");
        assert!(error.to_string().contains("identity"));
    }

    #[test]
    fn managed_runtime_evidence_reports_only_verified_bounded_usage_metadata() {
        let root = tempdir().expect("managed RTK tempdir");
        let managed_root = root.path().join("managed root with spaces");
        let mut lifecycle = ManagedRtkLifecycle::plan(
            "rtk-transaction.usage",
            HostPlatform::Windows,
            &"e".repeat(64),
        );
        lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: Some(1_024),
                filter_dsl: None,
            })
            .expect("install managed RTK hook");
        lifecycle.verify_hook().expect("verify managed RTK hook");
        let hook = lifecycle.verified_hook().expect("verified hook");

        let record = |recorded_at_ms, input_bytes, output_bytes| {
            serde_json::json!({
                "schema_version": 1,
                "transaction_id": "rtk-transaction.usage",
                "exit_code": 0,
                "input_bytes": input_bytes,
                "output_bytes": output_bytes,
                "saved_bytes": input_bytes - output_bytes,
                "transforms": ["stdout:ansi_strip"],
                "disposition": "filtered_verified_hook",
                "command_basename": "cargo",
                "command_sha256": "a".repeat(64),
                "bridge_sha256": hook.manifest.bridge.sha256,
                "bridge_version": hook.manifest.bridge.version,
                "recorded_at_ms": recorded_at_ms,
            })
        };
        fs::write(
            hook.usage_dir.join("100.json"),
            serde_json::to_vec(&record(100, 20u64, 12u64)).expect("record JSON"),
        )
        .expect("write record");
        fs::write(
            hook.usage_dir.join("200.json"),
            serde_json::to_vec(&record(200, 30u64, 15u64)).expect("record JSON"),
        )
        .expect("write record");

        let evidence = inspect_managed_rtk_runtime(&managed_root);
        assert!(evidence.installed);
        assert!(evidence.verified);
        assert_eq!(evidence.usage_count, 2);
        assert_eq!(evidence.saved_bytes, 23);
        assert_eq!(
            evidence
                .last_receipt
                .as_ref()
                .map(|receipt| receipt.recorded_at_ms),
            Some(200)
        );
        assert_eq!(evidence.reason, "verified_usage_evidence");
        let rendered = serde_json::to_string(&evidence).expect("serialize evidence");
        assert!(!rendered.contains("command_sha256"));
        assert!(!rendered.contains("api_key"));
    }

    #[test]
    fn managed_runtime_evidence_fails_closed_for_missing_or_tampered_roots() {
        let root = tempdir().expect("managed RTK tempdir");
        let missing = inspect_managed_rtk_runtime(&root.path().join("missing-managed-root"));
        assert!(!missing.installed);
        assert!(!missing.verified);
        assert_eq!(missing.reason, "managed_root_unavailable");

        let managed_root = root.path().join("managed-rtk");
        let mut lifecycle = ManagedRtkLifecycle::plan(
            "rtk-transaction.tamper",
            HostPlatform::Macos,
            &"f".repeat(64),
        );
        lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: None,
                filter_dsl: None,
            })
            .expect("install managed RTK hook");
        let manifest_path = managed_hook_manifest_path(&managed_root);
        let mut manifest: ManagedHookManifest =
            serde_json::from_slice(&fs::read(&manifest_path).expect("read manifest"))
                .expect("parse manifest");
        manifest.bridge.sha256 = "0".repeat(64);
        write_json_atomic(&manifest_path, &manifest).expect("rewrite manifest");

        let tampered = inspect_managed_rtk_runtime(&managed_root);
        assert!(tampered.installed);
        assert!(!tampered.verified);
        assert_eq!(tampered.reason, "managed_root_unverified");
    }

    #[test]
    fn managed_runtime_evidence_rejects_oversize_or_secret_shaped_usage_records() {
        let root = tempdir().expect("managed RTK tempdir");
        let managed_root = root.path().join("managed-rtk");
        let mut lifecycle = ManagedRtkLifecycle::plan(
            "rtk-transaction.usage-invalid",
            HostPlatform::Linux,
            &"1".repeat(64),
        );
        lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: None,
                filter_dsl: None,
            })
            .expect("install managed RTK hook");
        lifecycle.verify_hook().expect("verify managed RTK hook");
        let hook = lifecycle.verified_hook().expect("verified hook");

        fs::write(
            hook.usage_dir.join("oversize.json"),
            vec![b' '; MAX_MANAGED_RTK_USAGE_RECORD_BYTES + 1],
        )
        .expect("write oversize record");
        let oversize = inspect_managed_rtk_runtime(&managed_root);
        assert!(oversize.installed);
        assert!(!oversize.verified);
        assert_eq!(oversize.reason, "managed_usage_unverified");
        fs::remove_file(hook.usage_dir.join("oversize.json")).expect("remove oversize record");

        let record = serde_json::json!({
            "schema_version": 1,
            "transaction_id": "rtk-transaction.usage-invalid",
            "exit_code": 0,
            "input_bytes": 10,
            "output_bytes": 6,
            "saved_bytes": 4,
            "transforms": [],
            "disposition": "filtered_verified_hook",
            "command_basename": "cargo",
            "command_sha256": "a".repeat(64),
            "bridge_sha256": hook.manifest.bridge.sha256,
            "bridge_version": hook.manifest.bridge.version,
            "recorded_at_ms": 1,
            "api_key": "must-not-be-accepted",
        });
        fs::write(
            hook.usage_dir.join("secret-shaped.json"),
            serde_json::to_vec(&record).expect("record JSON"),
        )
        .expect("write secret-shaped record");
        let secret_shaped = inspect_managed_rtk_runtime(&managed_root);
        assert!(secret_shaped.installed);
        assert!(!secret_shaped.verified);
        assert_eq!(secret_shaped.reason, "managed_usage_unverified");
        assert!(!serde_json::to_string(&secret_shaped)
            .expect("serialize evidence")
            .contains("must-not-be-accepted"));
    }

    #[cfg(unix)]
    #[test]
    fn managed_runtime_evidence_rejects_symlinked_usage_records() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("managed RTK tempdir");
        let managed_root = root.path().join("managed-rtk");
        let mut lifecycle = ManagedRtkLifecycle::plan(
            "rtk-transaction.symlink",
            HostPlatform::Macos,
            &"2".repeat(64),
        );
        lifecycle
            .install_hook(ManagedHookPaths {
                managed_root: managed_root.clone(),
                bridge_path: fake_bridge_path(),
                max_output_bytes: None,
                filter_dsl: None,
            })
            .expect("install managed RTK hook");
        lifecycle.verify_hook().expect("verify managed RTK hook");
        let hook = lifecycle.verified_hook().expect("verified hook");
        let target = root.path().join("outside.json");
        fs::write(&target, b"{}" as &[u8]).expect("write target");
        symlink(&target, hook.usage_dir.join("linked.json")).expect("link usage record");

        let evidence = inspect_managed_rtk_runtime(&managed_root);
        assert!(evidence.installed);
        assert!(!evidence.verified);
        assert_eq!(evidence.reason, "managed_usage_unverified");
    }
}
