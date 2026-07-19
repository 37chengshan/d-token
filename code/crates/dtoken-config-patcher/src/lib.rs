use anyhow::{bail, Context, Result};
use dtoken_compress::optical::contains_secret_like;
use dtoken_types::{ConfigPatchPlan, ConfigToolKind, RiskLevel, RollbackPoint};
use serde::{
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use sha2::{Digest, Sha256};
use similar::TextDiff;
use std::{
    collections::HashSet,
    fmt,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

#[cfg(target_os = "windows")]
use std::process::{Command, Stdio};

pub mod agent;
pub mod agent_integration;
pub mod codebase_memory_integration;

const BACKUP_SCHEMA_VERSION: u32 = 1;
const BACKUP_CONTENT_FILE: &str = "original.bin";
const BACKUP_MANIFEST_FILE: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationKind {
    Utf8,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PermissionSnapshot {
    pub readonly: bool,
    pub unix_mode: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileSnapshot {
    pub exists: bool,
    pub sha256: Option<String>,
    pub byte_len: u64,
    pub permissions: Option<PermissionSnapshot>,
}

impl FileSnapshot {
    fn missing() -> Self {
        Self {
            exists: false,
            sha256: None,
            byte_len: 0,
            permissions: None,
        }
    }

    fn expected_state(&self) -> Result<ExpectedFileState> {
        if !self.exists {
            return Ok(ExpectedFileState::Missing);
        }
        let hash = self
            .sha256
            .clone()
            .context("existing file snapshot is missing its content hash")?;
        Ok(ExpectedFileState::Hash(hash))
    }
}

#[derive(Clone)]
pub struct PreparedChange {
    pub plan: ConfigPatchPlan,
    pub target_path: PathBuf,
    pub validation: ValidationKind,
    pub before: FileSnapshot,
    next_content: Vec<u8>,
}

impl PreparedChange {
    pub fn next_content_len(&self) -> usize {
        self.next_content.len()
    }

    pub fn next_content_hash(&self) -> String {
        hash_bytes(&self.next_content)
    }
}

impl fmt::Debug for PreparedChange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedChange")
            .field("plan", &self.plan)
            .field("target_path", &self.target_path)
            .field("validation", &self.validation)
            .field("before", &self.before)
            .field("next_content_len", &self.next_content.len())
            .field("next_content_hash", &hash_bytes(&self.next_content))
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackupRecord {
    pub schema_version: u32,
    pub rollback_id: String,
    pub plan_id: String,
    pub plan_before_sha256: String,
    pub planned_after_sha256: String,
    pub created_at: String,
    pub target_path: PathBuf,
    pub display_target: String,
    pub validation: ValidationKind,
    pub original: FileSnapshot,
    pub content_file: Option<String>,
    #[serde(skip)]
    pub backup_dir: PathBuf,
    #[serde(skip)]
    pub manifest_sha256: String,
}

impl BackupRecord {
    pub fn rollback_point(&self) -> RollbackPoint {
        RollbackPoint {
            id: self.rollback_id.clone(),
            created_at: self.created_at.clone(),
            backup_dir: self.backup_dir.display().to_string(),
            target_files: vec![self.display_target.clone()],
            patch_ids: vec![self.plan_id.clone()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "sha256")]
pub enum ExpectedFileState {
    Missing,
    Hash(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyReceipt {
    pub plan_id: String,
    pub target_path: PathBuf,
    pub backup: BackupRecord,
    pub before: FileSnapshot,
    pub after: FileSnapshot,
    pub applied_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreReceipt {
    pub rollback_id: String,
    pub target_path: PathBuf,
    pub restored: FileSnapshot,
    pub restored_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionReceipt {
    pub transaction_id: String,
    pub applied_at: String,
    pub changes: Vec<ApplyReceipt>,
}

#[derive(Debug, Clone)]
pub struct ConfigPatcher {
    backup_root: PathBuf,
}

impl ConfigPatcher {
    pub fn new(backup_root: impl Into<PathBuf>) -> Self {
        Self {
            backup_root: backup_root.into(),
        }
    }

    pub fn plan_replace(
        &self,
        target_file: impl AsRef<Path>,
        next_content: &str,
        tool: ConfigToolKind,
        agent: Option<String>,
        risk_level: RiskLevel,
    ) -> Result<ConfigPatchPlan> {
        Ok(self
            .prepare_change(target_file, next_content, tool, agent, risk_level)?
            .plan)
    }

    pub fn prepare_change(
        &self,
        target_file: impl AsRef<Path>,
        next_content: &str,
        tool: ConfigToolKind,
        agent: Option<String>,
        risk_level: RiskLevel,
    ) -> Result<PreparedChange> {
        let display_target = target_file.as_ref().display().to_string();
        let target_path = normalize_target_path(target_file.as_ref())?;
        let validation = validation_for_path(&target_path);
        let (before, before_bytes) = inspect_target(&target_path)?;
        if let Some(bytes) = before_bytes.as_deref() {
            validate_content(validation, bytes).context("existing configuration is invalid")?;
        }
        validate_content(validation, next_content.as_bytes())
            .context("replacement configuration is invalid")?;

        let before_text = before_bytes
            .as_deref()
            .map(bytes_to_utf8)
            .transpose()?
            .unwrap_or_default();
        let before_hash = before.sha256.clone().unwrap_or_else(|| hash_bytes(&[]));
        let after_hash = hash_bytes(next_content.as_bytes());
        let diff = simple_diff(before_text, next_content);
        let plan = ConfigPatchPlan {
            id: Uuid::new_v4().to_string(),
            target_file: display_target,
            tool,
            agent,
            before_hash,
            after_hash,
            diff,
            risk_level,
            requires_restart: true,
        };

        Ok(PreparedChange {
            plan,
            target_path,
            validation,
            before,
            next_content: next_content.as_bytes().to_vec(),
        })
    }

    pub fn backup(&self, plan: &ConfigPatchPlan) -> Result<RollbackPoint> {
        let prepared = self.prepared_from_existing_plan(plan, &[])?;
        Ok(self.backup_prepared(&prepared)?.rollback_point())
    }

    pub fn backup_prepared(&self, prepared: &PreparedChange) -> Result<BackupRecord> {
        self.validate_prepared_identity(prepared)?;
        let (current, current_bytes) = inspect_target(&prepared.target_path)?;
        if current != prepared.before {
            bail!("configuration changed after planning; refusing to create a stale backup");
        }

        let backup_root = self.ensure_backup_root()?;
        let rollback_id = Uuid::new_v4().to_string();
        let backup_dir = backup_root.join(&rollback_id);
        create_private_dir(&backup_dir).with_context(|| {
            format!("failed to create backup directory {}", backup_dir.display())
        })?;

        let result = (|| -> Result<BackupRecord> {
            let content_file = if let Some(bytes) = current_bytes.as_deref() {
                let content_path = backup_dir.join(BACKUP_CONTENT_FILE);
                write_new_private_file(&content_path, bytes).with_context(|| {
                    format!(
                        "failed to persist backup content in {}",
                        backup_dir.display()
                    )
                })?;
                Some(BACKUP_CONTENT_FILE.to_string())
            } else {
                None
            };

            let mut record = BackupRecord {
                schema_version: BACKUP_SCHEMA_VERSION,
                rollback_id: rollback_id.clone(),
                plan_id: prepared.plan.id.clone(),
                plan_before_sha256: prepared.plan.before_hash.clone(),
                planned_after_sha256: prepared.plan.after_hash.clone(),
                created_at: now_timestamp()?,
                target_path: prepared.target_path.clone(),
                display_target: prepared.plan.target_file.clone(),
                validation: prepared.validation,
                original: current,
                content_file,
                backup_dir: backup_dir.clone(),
                manifest_sha256: String::new(),
            };
            let manifest = serde_json::to_vec_pretty(&record)
                .context("failed to serialize backup manifest")?;
            record.manifest_sha256 = hash_bytes(&manifest);
            write_new_private_file(&backup_dir.join(BACKUP_MANIFEST_FILE), &manifest)
                .context("failed to persist backup manifest")?;
            sync_directory(&backup_dir)?;
            sync_directory(&backup_root)?;
            Ok(record)
        })();

        if result.is_err() {
            let _ = fs::remove_dir_all(&backup_dir);
        }
        result
    }

    pub fn load_backup(&self, rollback_id: &str) -> Result<BackupRecord> {
        validate_rollback_id(rollback_id)?;
        let backup_root = self.existing_backup_root()?;
        let backup_dir = backup_root.join(rollback_id);
        ensure_path_has_no_symlinks(&backup_dir)?;
        ensure_private_directory(&backup_dir, "backup directory")?;
        let manifest_path = backup_dir.join(BACKUP_MANIFEST_FILE);
        ensure_private_file(&manifest_path, "backup manifest")?;
        let manifest = read_limited(&manifest_path, MAX_MANIFEST_BYTES)?;
        validate_strict_json(&manifest).context("backup manifest is invalid")?;
        let manifest_sha256 = hash_bytes(&manifest);
        let mut record: BackupRecord =
            serde_json::from_slice(&manifest).context("failed to decode backup manifest")?;

        if record.schema_version != BACKUP_SCHEMA_VERSION {
            bail!("unsupported backup manifest schema version");
        }
        if record.rollback_id != rollback_id {
            bail!("backup manifest identity mismatch");
        }
        validate_sha256(&record.plan_before_sha256)?;
        validate_sha256(&record.planned_after_sha256)?;
        if record.target_path != normalize_target_path(&record.target_path)? {
            bail!("backup manifest target path is not normalized");
        }
        validate_backup_content_reference(&record)?;
        validate_manifest_snapshot_binding(&record)?;
        record.backup_dir = backup_dir;
        record.manifest_sha256 = manifest_sha256;
        self.verify_backup_content(&record)?;
        Ok(record)
    }

    pub fn apply_replace(
        &self,
        plan: &ConfigPatchPlan,
        next_content: &str,
    ) -> Result<RollbackPoint> {
        let prepared = self.prepared_from_existing_plan(plan, next_content.as_bytes())?;
        let backup = self.backup_prepared(&prepared)?;
        match self.apply_prepared(&prepared, &backup) {
            Ok(_) => Ok(backup.rollback_point()),
            Err(apply_error) => {
                let rollback_result = self.restore_current_if_owned(&prepared, &backup);
                match rollback_result {
                    Ok(()) => Err(apply_error),
                    Err(rollback_error) => Err(anyhow::anyhow!(
                        "configuration apply failed and automatic restore was incomplete: {rollback_error:#}; original apply error: {apply_error:#}"
                    )),
                }
            }
        }
    }

    pub fn apply_prepared(
        &self,
        prepared: &PreparedChange,
        backup: &BackupRecord,
    ) -> Result<ApplyReceipt> {
        self.validate_prepared_identity(prepared)?;
        validate_content(prepared.validation, &prepared.next_content)
            .context("replacement configuration is invalid")?;
        if hash_bytes(&prepared.next_content) != prepared.plan.after_hash {
            bail!("replacement content does not match the planned content hash");
        }

        let persisted = self.reload_and_match_backup(backup)?;
        if persisted.plan_id != prepared.plan.id
            || persisted.target_path != prepared.target_path
            || persisted.original != prepared.before
            || persisted.plan_before_sha256 != prepared.plan.before_hash
            || persisted.planned_after_sha256 != prepared.plan.after_hash
        {
            bail!("backup does not belong to the prepared configuration change");
        }

        let permissions = permissions_for_replacement(&prepared.before);
        let expected = prepared.before.expected_state()?;
        check_snapshot_unchanged(&prepared.target_path, &prepared.before)?;
        atomic_replace_checked(
            &prepared.target_path,
            &prepared.next_content,
            &permissions,
            &expected,
        )?;
        let (after, after_bytes) = inspect_target(&prepared.target_path)?;
        let after_bytes = after_bytes.context("replacement target disappeared after apply")?;
        if after.sha256.as_deref() != Some(prepared.plan.after_hash.as_str())
            || after_bytes != prepared.next_content
        {
            bail!("post-write content verification failed");
        }
        validate_content(prepared.validation, &after_bytes)
            .context("post-write configuration validation failed")?;
        verify_permissions(after.permissions.as_ref(), &permissions)?;

        Ok(ApplyReceipt {
            plan_id: prepared.plan.id.clone(),
            target_path: prepared.target_path.clone(),
            backup: persisted,
            before: prepared.before.clone(),
            after,
            applied_at: now_timestamp()?,
        })
    }

    pub fn restore_backup(
        &self,
        backup: &BackupRecord,
        expected_current: &ExpectedFileState,
    ) -> Result<RestoreReceipt> {
        let persisted = self.reload_and_match_backup(backup)?;
        check_expected_state(&persisted.target_path, expected_current)?;

        if persisted.original.exists {
            let content_path = persisted.backup_dir.join(BACKUP_CONTENT_FILE);
            ensure_private_file(&content_path, "backup content")?;
            let bytes = fs::read(&content_path).context("failed to read backup content")?;
            self.verify_original_bytes(&persisted, &bytes)?;
            validate_content(persisted.validation, &bytes)
                .context("backup configuration is invalid")?;
            let permissions = persisted
                .original
                .permissions
                .as_ref()
                .context("backup manifest is missing original permissions")?;
            atomic_replace_checked(
                &persisted.target_path,
                &bytes,
                permissions,
                expected_current,
            )?;
            validate_content(persisted.validation, &bytes)
                .context("restored configuration validation failed")?;
        } else {
            durable_remove_checked(&persisted.target_path, expected_current)?;
        }

        let (restored, restored_bytes) = inspect_target(&persisted.target_path)?;
        if restored != persisted.original {
            bail!("restored file metadata or content hash does not match the backup");
        }
        if let Some(bytes) = restored_bytes.as_deref() {
            self.verify_original_bytes(&persisted, bytes)?;
        }

        Ok(RestoreReceipt {
            rollback_id: persisted.rollback_id,
            target_path: persisted.target_path,
            restored,
            restored_at: now_timestamp()?,
        })
    }

    pub fn apply_transaction(&self, changes: &[PreparedChange]) -> Result<TransactionReceipt> {
        self.apply_transaction_with_observer(changes, |_, _| Ok(()))
    }

    fn apply_transaction_with_observer<F>(
        &self,
        changes: &[PreparedChange],
        mut after_apply: F,
    ) -> Result<TransactionReceipt>
    where
        F: FnMut(usize, &ApplyReceipt) -> Result<()>,
    {
        ensure_unique_targets(changes)?;
        let mut backups = Vec::with_capacity(changes.len());
        for change in changes {
            backups.push(self.backup_prepared(change)?);
        }

        let mut receipts = Vec::with_capacity(changes.len());
        for (index, (change, backup)) in changes.iter().zip(backups.iter()).enumerate() {
            let receipt = match self.apply_prepared(change, backup) {
                Ok(receipt) => receipt,
                Err(apply_error) => {
                    return self.transaction_failure(change, backup, &receipts, apply_error);
                }
            };
            if let Err(observer_error) = after_apply(index, &receipt) {
                return self.transaction_failure(change, backup, &receipts, observer_error);
            }
            receipts.push(receipt);
        }

        Ok(TransactionReceipt {
            transaction_id: Uuid::new_v4().to_string(),
            applied_at: now_timestamp()?,
            changes: receipts,
        })
    }

    fn rollback_applied(&self, receipts: &[ApplyReceipt]) -> Result<()> {
        let mut failures = Vec::new();
        for receipt in receipts.iter().rev() {
            let expected = receipt.after.expected_state()?;
            if let Err(error) = self.restore_backup(&receipt.backup, &expected) {
                failures.push(format!("{}: {error:#}", receipt.target_path.display()));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!("{}", failures.join("; "))
        }
    }

    fn transaction_failure(
        &self,
        current_change: &PreparedChange,
        current_backup: &BackupRecord,
        prior_receipts: &[ApplyReceipt],
        apply_error: anyhow::Error,
    ) -> Result<TransactionReceipt> {
        let mut rollback_failures = Vec::new();
        if let Err(error) = self.restore_current_if_owned(current_change, current_backup) {
            rollback_failures.push(format!(
                "{}: failed to inspect rollback candidate: {error:#}",
                current_change.target_path.display()
            ));
        }
        if let Err(error) = self.rollback_applied(prior_receipts) {
            rollback_failures.push(format!("prior targets: {error:#}"));
        }

        if rollback_failures.is_empty() {
            Err(apply_error.context(
                "configuration transaction failed; transaction-owned changes were restored",
            ))
        } else {
            Err(anyhow::anyhow!(
                "configuration transaction failed and rollback was incomplete: {}; original apply error: {apply_error:#}",
                rollback_failures.join("; ")
            ))
        }
    }

    fn restore_current_if_owned(
        &self,
        change: &PreparedChange,
        backup: &BackupRecord,
    ) -> Result<()> {
        let (snapshot, _) = inspect_target(&change.target_path)?;
        if snapshot.sha256.as_deref() == Some(change.plan.after_hash.as_str()) {
            let expected = ExpectedFileState::Hash(change.plan.after_hash.clone());
            self.restore_backup(backup, &expected)?;
        }
        Ok(())
    }

    fn prepared_from_existing_plan(
        &self,
        plan: &ConfigPatchPlan,
        next_content: &[u8],
    ) -> Result<PreparedChange> {
        let target_path = normalize_target_path(Path::new(&plan.target_file))?;
        let validation = validation_for_path(&target_path);
        let (before, before_bytes) = inspect_target(&target_path)?;
        if let Some(bytes) = before_bytes.as_deref() {
            validate_content(validation, bytes).context("existing configuration is invalid")?;
        }
        let current_hash = before.sha256.clone().unwrap_or_else(empty_hash);
        if current_hash != plan.before_hash {
            bail!("config hash mismatch; refusing to apply stale patch plan");
        }
        if !next_content.is_empty() || plan.after_hash == hash_bytes(&[]) {
            validate_content(validation, next_content)
                .context("replacement configuration is invalid")?;
            if hash_bytes(next_content) != plan.after_hash {
                bail!("replacement content does not match the planned content hash");
            }
        }
        Ok(PreparedChange {
            plan: plan.clone(),
            target_path,
            validation,
            before,
            next_content: next_content.to_vec(),
        })
    }

    fn validate_prepared_identity(&self, prepared: &PreparedChange) -> Result<()> {
        let normalized = normalize_target_path(Path::new(&prepared.plan.target_file))?;
        if normalized != prepared.target_path {
            bail!("prepared target identity does not match its patch plan");
        }
        if prepared.before.exists
            && prepared.before.sha256.as_deref() != Some(&prepared.plan.before_hash)
        {
            bail!("prepared snapshot does not match its patch plan");
        }
        if !prepared.before.exists && prepared.plan.before_hash != hash_bytes(&[]) {
            bail!("missing target snapshot does not match its patch plan");
        }
        Ok(())
    }

    fn ensure_backup_root(&self) -> Result<PathBuf> {
        reject_parent_components(&self.backup_root)?;
        ensure_existing_ancestors_have_no_symlinks(&self.backup_root)?;
        match fs::symlink_metadata(&self.backup_root) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    bail!("backup root is not a directory");
                }
                ensure_private_directory(&self.backup_root, "backup root")?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                fs::create_dir_all(&self.backup_root).with_context(|| {
                    format!(
                        "failed to create backup root {}",
                        self.backup_root.display()
                    )
                })?;
                set_private_dir_permissions(&self.backup_root)?;
            }
            Err(error) => return Err(error).context("failed to inspect backup root"),
        }
        ensure_path_has_no_symlinks(&self.backup_root)?;
        fs::canonicalize(&self.backup_root).with_context(|| {
            format!(
                "failed to normalize backup root {}",
                self.backup_root.display()
            )
        })
    }

    fn existing_backup_root(&self) -> Result<PathBuf> {
        reject_parent_components(&self.backup_root)?;
        ensure_path_has_no_symlinks(&self.backup_root)?;
        let metadata =
            fs::symlink_metadata(&self.backup_root).context("backup root is unavailable")?;
        if !metadata.is_dir() {
            bail!("backup root is not a directory");
        }
        ensure_private_directory(&self.backup_root, "backup root")?;
        fs::canonicalize(&self.backup_root).context("failed to normalize backup root")
    }

    fn reload_and_match_backup(&self, supplied: &BackupRecord) -> Result<BackupRecord> {
        if supplied.manifest_sha256.is_empty() {
            bail!("backup record has no manifest integrity anchor");
        }
        let persisted = self.load_backup(&supplied.rollback_id)?;
        if persisted.manifest_sha256 != supplied.manifest_sha256 {
            bail!("backup manifest changed after it was created or loaded");
        }
        Ok(persisted)
    }

    fn verify_backup_content(&self, record: &BackupRecord) -> Result<()> {
        if !record.original.exists {
            return Ok(());
        }
        let path = record.backup_dir.join(BACKUP_CONTENT_FILE);
        ensure_private_file(&path, "backup content")?;
        let bytes = fs::read(&path).context("failed to read backup content")?;
        self.verify_original_bytes(record, &bytes)?;
        validate_content(record.validation, &bytes).context("backup configuration is invalid")
    }

    fn verify_original_bytes(&self, record: &BackupRecord, bytes: &[u8]) -> Result<()> {
        if record.original.byte_len != bytes.len() as u64
            || record.original.sha256.as_deref() != Some(hash_bytes(bytes).as_str())
        {
            bail!("backup content integrity verification failed");
        }
        Ok(())
    }
}

pub fn stable_hash(content: &str) -> String {
    hash_bytes(content.as_bytes())
}

pub fn simple_diff(before: &str, after: &str) -> String {
    if before == after {
        return "no changes".to_string();
    }
    let safe_before = sanitize_for_diff(before);
    let safe_after = sanitize_for_diff(after);
    if safe_before == safe_after {
        return "--- before\n+++ after\n@@ sensitive value changed @@\n-<redacted sensitive value>\n+<redacted sensitive value>\n"
            .to_string();
    }
    TextDiff::from_lines(&safe_before, &safe_after)
        .unified_diff()
        .context_radius(3)
        .header("before", "after")
        .to_string()
}

pub fn mock_patch_plan() -> ConfigPatchPlan {
    ConfigPatchPlan {
        id: "m1-mock-patch".to_string(),
        target_file: "./example-config.json".to_string(),
        tool: ConfigToolKind::Dtoken,
        agent: None,
        before_hash: stable_hash("{}"),
        after_hash: stable_hash("{\"enabled\":true}"),
        diff: simple_diff("{}", "{\"enabled\":true}"),
        risk_level: RiskLevel::Low,
        requires_restart: false,
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn empty_hash() -> String {
    hash_bytes(&[])
}

fn validation_for_path(path: &Path) -> ValidationKind {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        ValidationKind::Json
    } else {
        ValidationKind::Utf8
    }
}

fn validate_content(kind: ValidationKind, bytes: &[u8]) -> Result<()> {
    bytes_to_utf8(bytes)?;
    if kind == ValidationKind::Json {
        validate_strict_json(bytes)?;
    }
    Ok(())
}

fn bytes_to_utf8(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).context("configuration must be valid UTF-8")
}

fn validate_strict_json(bytes: &[u8]) -> Result<()> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    StrictJsonSeed
        .deserialize(&mut deserializer)
        .context("configuration must contain exactly one valid JSON value")?;
    deserializer
        .end()
        .context("configuration contains trailing JSON data")?;
    Ok(())
}

struct StrictJsonSeed;

impl<'de> DeserializeSeed<'de> for StrictJsonSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

struct StrictJsonVisitor;

impl<'de> Visitor<'de> for StrictJsonVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a valid JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, _value: bool) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        StrictJsonSeed.deserialize(deserializer)
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        StrictJsonSeed.deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element_seed(StrictJsonSeed)?.is_some() {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("JSON contains a duplicate object key"));
            }
            map.next_value_seed(StrictJsonSeed)?;
        }
        Ok(())
    }
}

fn sanitize_for_diff(content: &str) -> String {
    if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(content) {
        redact_json_value(&mut json);
        return serde_json::to_string_pretty(&json)
            .unwrap_or_else(|_| "<configuration diff unavailable>".to_string());
    }
    content
        .lines()
        .map(|line| {
            if contains_sensitive_marker(line) {
                "<redacted sensitive setting>"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if is_sensitive_key(key) {
                    *value = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_json_value(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json_value(value);
            }
        }
        serde_json::Value::String(text) if contains_secret_like(text) => {
            *value = serde_json::Value::String("<redacted>".to_string());
        }
        _ => {}
    }
}

fn contains_sensitive_marker(value: &str) -> bool {
    is_sensitive_key(value) || contains_secret_like(value)
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized: String = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    let known_marker = [
        "apikey",
        "token",
        "password",
        "passwd",
        "secret",
        "authorization",
        "cookie",
        "credential",
        "privatekey",
        "bearer",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));
    known_marker || contains_secret_like(&format!("{key}=nonempty"))
}

fn inspect_target(path: &Path) -> Result<(FileSnapshot, Option<Vec<u8>>)> {
    ensure_path_has_no_symlinks(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok((FileSnapshot::missing(), None));
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect target {}", path.display()));
        }
    };
    if !metadata.is_file() {
        bail!("configuration target is not a regular file");
    }

    let mut file = File::open(path)
        .with_context(|| format!("failed to open configuration target {}", path.display()))?;
    let opened_metadata = file
        .metadata()
        .context("failed to inspect opened configuration target")?;
    if !opened_metadata.is_file() {
        bail!("opened configuration target is not a regular file");
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .context("failed to read configuration target")?;
    let final_metadata = file
        .metadata()
        .context("failed to re-check opened configuration target")?;
    if final_metadata.len() != bytes.len() as u64 {
        bail!("configuration changed while it was being read");
    }
    let permissions = snapshot_permissions(&final_metadata);
    Ok((
        FileSnapshot {
            exists: true,
            sha256: Some(hash_bytes(&bytes)),
            byte_len: bytes.len() as u64,
            permissions: Some(permissions),
        },
        Some(bytes),
    ))
}

fn snapshot_permissions(metadata: &fs::Metadata) -> PermissionSnapshot {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        PermissionSnapshot {
            readonly: metadata.permissions().readonly(),
            unix_mode: Some(metadata.permissions().mode() & 0o7777),
        }
    }
    #[cfg(not(unix))]
    {
        PermissionSnapshot {
            readonly: metadata.permissions().readonly(),
            unix_mode: None,
        }
    }
}

fn permissions_for_replacement(before: &FileSnapshot) -> PermissionSnapshot {
    before.permissions.clone().unwrap_or(PermissionSnapshot {
        readonly: false,
        #[cfg(unix)]
        unix_mode: Some(0o600),
        #[cfg(not(unix))]
        unix_mode: None,
    })
}

fn verify_permissions(
    actual: Option<&PermissionSnapshot>,
    expected: &PermissionSnapshot,
) -> Result<()> {
    let actual = actual.context("post-write permissions are unavailable")?;
    #[cfg(unix)]
    if actual.unix_mode != expected.unix_mode {
        bail!("post-write Unix permission verification failed");
    }
    #[cfg(not(unix))]
    if actual.readonly != expected.readonly {
        bail!("post-write readonly permission verification failed");
    }
    Ok(())
}

fn normalize_target_path(path: &Path) -> Result<PathBuf> {
    reject_parent_components(path)?;
    path.file_name()
        .context("configuration target must include a file name")?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to resolve current directory")?
            .join(path)
    };
    reject_parent_components(&absolute)?;
    let parent = absolute
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    ensure_existing_ancestors_have_no_symlinks(parent)?;
    let mut existing_ancestor = parent;
    loop {
        match fs::symlink_metadata(existing_ancestor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    bail!("symbolic links are not allowed in configuration paths");
                }
                if !metadata.is_dir() {
                    bail!("configuration target ancestor is not a directory");
                }
                break;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                existing_ancestor = existing_ancestor
                    .parent()
                    .context("configuration target has no existing ancestor")?;
            }
            Err(error) => {
                return Err(error).context("failed to inspect configuration target ancestor")
            }
        }
    }
    let canonical_ancestor = fs::canonicalize(existing_ancestor).with_context(|| {
        format!(
            "failed to normalize configuration target ancestor {}",
            existing_ancestor.display()
        )
    })?;
    let suffix = absolute
        .strip_prefix(existing_ancestor)
        .context("configuration target is outside its existing ancestor")?;
    let normalized = canonical_ancestor.join(suffix);
    ensure_path_has_no_symlinks(&normalized)?;
    Ok(normalized)
}

fn reject_parent_components(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        bail!("paths containing parent-directory components are not allowed");
    }
    Ok(())
}

fn ensure_existing_ancestors_have_no_symlinks(path: &Path) -> Result<()> {
    reject_parent_components(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("symbolic links are not allowed in configuration paths");
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => break,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect path {}", current.display()));
            }
        }
    }
    Ok(())
}

fn ensure_path_has_no_symlinks(path: &Path) -> Result<()> {
    reject_parent_components(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("symbolic links are not allowed in configuration paths");
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect path {}", current.display()));
            }
        }
    }
    Ok(())
}

fn ensure_regular_file(path: &Path, label: &str) -> Result<()> {
    ensure_path_has_no_symlinks(path)?;
    let metadata = fs::symlink_metadata(path).with_context(|| format!("{label} is unavailable"))?;
    if !metadata.is_file() {
        bail!("{label} is not a regular file");
    }
    Ok(())
}

fn ensure_private_directory(path: &Path, label: &str) -> Result<()> {
    ensure_path_has_no_symlinks(path)?;
    let metadata = fs::symlink_metadata(path).with_context(|| format!("{label} is unavailable"))?;
    if !metadata.is_dir() {
        bail!("{label} is not a directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("{label} permissions expose backup data to other users");
        }
    }
    #[cfg(target_os = "windows")]
    validate_private_windows_acl(path, label)?;
    Ok(())
}

fn ensure_private_file(path: &Path, label: &str) -> Result<()> {
    ensure_regular_file(path, label)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            bail!("{label} permissions expose backup data to other users");
        }
    }
    #[cfg(target_os = "windows")]
    validate_private_windows_acl(path, label)?;
    Ok(())
}

fn ensure_unique_targets(changes: &[PreparedChange]) -> Result<()> {
    let mut targets = HashSet::new();
    for change in changes {
        if !targets.insert(change.target_path.clone()) {
            bail!("a configuration transaction cannot target the same file twice");
        }
    }
    Ok(())
}

fn validate_rollback_id(rollback_id: &str) -> Result<()> {
    let parsed = Uuid::parse_str(rollback_id).context("invalid rollback identifier")?;
    if parsed.to_string() != rollback_id.to_ascii_lowercase() {
        bail!("rollback identifier is not in canonical UUID form");
    }
    Ok(())
}

fn validate_backup_content_reference(record: &BackupRecord) -> Result<()> {
    match (record.original.exists, record.content_file.as_deref()) {
        (true, Some(BACKUP_CONTENT_FILE)) | (false, None) => Ok(()),
        _ => bail!("backup manifest content reference is inconsistent"),
    }
}

fn validate_sha256(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("backup manifest contains an invalid SHA-256 digest");
    }
    Ok(())
}

fn validate_manifest_snapshot_binding(record: &BackupRecord) -> Result<()> {
    match (
        record.original.exists,
        record.original.sha256.as_deref(),
        record.original.permissions.as_ref(),
    ) {
        (true, Some(hash), Some(_)) => {
            validate_sha256(hash)?;
            if hash != record.plan_before_sha256 {
                bail!("backup manifest snapshot does not match the planned before hash");
            }
        }
        (false, None, None) if record.original.byte_len == 0 => {
            if record.plan_before_sha256 != empty_hash() {
                bail!("missing-file backup does not match the planned before hash");
            }
        }
        _ => bail!("backup manifest contains an inconsistent file snapshot"),
    }
    Ok(())
}

fn read_limited(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).context("failed to inspect bounded file")?;
    if metadata.len() > maximum {
        bail!("file exceeds the allowed size limit");
    }
    fs::read(path).context("failed to read bounded file")
}

fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir(path)?;
    set_private_dir_permissions(path)
}

fn set_private_dir_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(target_os = "windows")]
    {
        let username = std::env::var("USERNAME").context("USERNAME is required")?;
        let identity = std::env::var("USERDOMAIN")
            .ok()
            .filter(|domain| !domain.is_empty())
            .map_or(username.clone(), |domain| format!("{domain}\\{username}"));
        let user_grant = format!("{identity}:(OI)(CI)F");
        let status = Command::new("icacls.exe")
            .arg(path)
            .args(["/inheritance:r", "/grant:r"])
            .arg(user_grant)
            .arg("*S-1-5-18:(OI)(CI)F")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("failed to enforce private Windows backup ACL")?;
        if !status.success() {
            bail!("failed to enforce private Windows backup ACL");
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_private_windows_acl(path: &Path, label: &str) -> Result<()> {
    let script = r#"$acl=Get-Acl -LiteralPath $env:DTOKEN_PRIVATE_PATH;
$me=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;
$allowed=@($me,'S-1-5-18');
foreach($rule in $acl.Access){
  try{$sid=$rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value}catch{exit 2}
  if($rule.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and $allowed -notcontains $sid){exit 3}
}
exit 0"#;
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("DTOKEN_PRIVATE_PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to validate private Windows backup ACL")?;
    if !status.success() {
        bail!("{label} permissions expose backup data to other users");
    }
    Ok(())
}

fn write_new_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn now_timestamp() -> Result<String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?;
    Ok(format!(
        "{}.{:03}Z",
        duration.as_secs(),
        duration.subsec_millis()
    ))
}

fn check_expected_state(path: &Path, expected: &ExpectedFileState) -> Result<()> {
    let (current, _) = inspect_target(path)?;
    let matches = match expected {
        ExpectedFileState::Missing => !current.exists,
        ExpectedFileState::Hash(hash) => current.sha256.as_deref() == Some(hash.as_str()),
    };
    if !matches {
        bail!("configuration changed concurrently; refusing to overwrite it");
    }
    Ok(())
}

fn check_snapshot_unchanged(path: &Path, expected: &FileSnapshot) -> Result<()> {
    let (current, _) = inspect_target(path)?;
    if &current != expected {
        bail!("configuration metadata or content changed after backup");
    }
    Ok(())
}

fn atomic_replace_checked(
    target: &Path,
    bytes: &[u8],
    permissions: &PermissionSnapshot,
    expected: &ExpectedFileState,
) -> Result<()> {
    let normalized = normalize_target_path(target)?;
    if normalized != target {
        bail!("atomic replacement target is not normalized");
    }
    let parent = target
        .parent()
        .context("atomic replacement target has no parent directory")?;
    let temp_path = parent.join(format!(".dtoken-{}.tmp", Uuid::new_v4()));
    let mut guard = TempFileGuard::new(temp_path.clone());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut temp = options
        .open(&temp_path)
        .context("failed to create atomic replacement file")?;
    temp.write_all(bytes)
        .context("failed to write atomic replacement file")?;
    apply_permissions(&temp_path, permissions)?;
    temp.sync_all()
        .context("failed to sync atomic replacement file")?;
    drop(temp);

    check_expected_state(target, expected)?;
    atomic_rename(&temp_path, target)?;
    guard.disarm();
    sync_directory(parent)?;
    Ok(())
}

fn durable_remove_checked(target: &Path, expected: &ExpectedFileState) -> Result<()> {
    check_expected_state(target, expected)?;
    fs::remove_file(target)
        .context("failed to remove newly created configuration during restore")?;
    let parent = target
        .parent()
        .context("removed configuration target has no parent directory")?;
    sync_directory(parent)?;
    let (snapshot, _) = inspect_target(target)?;
    if snapshot.exists {
        bail!("configuration target still exists after durable removal");
    }
    Ok(())
}

fn apply_permissions(path: &Path, permissions: &PermissionSnapshot) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions
            .unix_mode
            .context("Unix replacement permissions are missing a mode")?;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .context("failed to apply Unix replacement permissions")?;
    }
    #[cfg(not(unix))]
    {
        let mut actual = fs::metadata(path)?.permissions();
        actual.set_readonly(permissions.readonly);
        fs::set_permissions(path, actual)
            .context("failed to apply replacement readonly permission")?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_rename(source: &Path, target: &Path) -> Result<()> {
    fs::rename(source, target).context("failed to atomically replace configuration target")
}

#[cfg(windows)]
fn atomic_rename(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both buffers are NUL-terminated, remain alive for the call, and are
    // derived directly from the source and target paths without pointer aliasing.
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error())
            .context("failed to atomically replace configuration target");
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("failed to open directory {} for sync", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn workspace_tempdir() -> TempDir {
        tempfile::tempdir_in(std::env::current_dir().expect("current directory"))
            .expect("temporary directory")
    }

    #[test]
    fn transaction_restores_first_target_when_second_changes_concurrently() {
        let root = workspace_tempdir();
        let first = root.path().join("first.json");
        let second = root.path().join("second.json");
        fs::write(&first, "{\"value\":1}").expect("first fixture");
        fs::write(&second, "{\"value\":2}").expect("second fixture");
        let patcher = ConfigPatcher::new(root.path().join("backups"));
        let first_change = patcher
            .prepare_change(
                &first,
                "{\"value\":10}",
                ConfigToolKind::Dtoken,
                None,
                RiskLevel::Low,
            )
            .expect("first plan");
        let second_change = patcher
            .prepare_change(
                &second,
                "{\"value\":20}",
                ConfigToolKind::Dtoken,
                None,
                RiskLevel::Low,
            )
            .expect("second plan");

        let result =
            patcher.apply_transaction_with_observer(&[first_change, second_change], |index, _| {
                if index == 0 {
                    fs::write(&second, "{\"external\":true}")?;
                }
                Ok(())
            });

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(first).expect("first result"),
            "{\"value\":1}"
        );
        assert_eq!(
            fs::read_to_string(second).expect("second result"),
            "{\"external\":true}"
        );
    }
}
