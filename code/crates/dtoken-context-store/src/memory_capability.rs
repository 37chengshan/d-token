use super::{
    capped_i64, restrict_store_directory_permissions, restrict_store_permissions,
    validate_memory_access_scope, validate_scope_identifier, ContextStore, MemoryAccessScope,
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

const MEMORY_CAPABILITY_SCHEMA_VERSION: u8 = 1;
const MAX_MEMORY_CAPABILITY_DOCUMENT_BYTES: usize = 32 * 1024;
const MAX_MEMORY_CAPABILITY_TTL_MS: u64 = 365 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCapabilityPermission {
    List,
    Export,
    Write,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryCapabilityRouteBinding {
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub config_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryCapabilityTransactionRequest {
    pub scope: MemoryAccessScope,
    pub route_binding: MemoryCapabilityRouteBinding,
    pub permissions: Vec<MemoryCapabilityPermission>,
    pub source_revision: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryCapabilityTransactionPlan {
    pub schema_version: u8,
    pub transaction_id: String,
    pub target_path: PathBuf,
    pub scope: MemoryAccessScope,
    pub route_binding: MemoryCapabilityRouteBinding,
    pub permissions: Vec<MemoryCapabilityPermission>,
    pub source_revision: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub document_sha256: String,
    pub review_digest: String,
    pub requires_explicit_confirmation: bool,
}

#[derive(Debug)]
pub struct PreparedMemoryCapabilityTransaction {
    pub plan: MemoryCapabilityTransactionPlan,
    document: MemoryCapabilityDocument,
    encoded_document: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCapabilityTransactionState {
    Planned,
    Active,
    Verified,
    Revoked,
    Expired,
    Failed,
}

impl MemoryCapabilityTransactionState {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "planned" => Ok(Self::Planned),
            "active" => Ok(Self::Active),
            "verified" => Ok(Self::Verified),
            "revoked" => Ok(Self::Revoked),
            "expired" => Ok(Self::Expired),
            "failed" => Ok(Self::Failed),
            _ => bail!("memory capability transaction state is invalid"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryCapabilityTransactionReceipt {
    pub schema_version: u8,
    pub transaction_id: String,
    pub state: MemoryCapabilityTransactionState,
    pub target_path: PathBuf,
    pub document_sha256: String,
    pub review_digest: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub verified_at_ms: Option<u64>,
    pub revoked_at_ms: Option<u64>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMemoryCapability {
    pub transaction_id: String,
    pub scope: MemoryAccessScope,
    pub route_binding: MemoryCapabilityRouteBinding,
    pub permissions: Vec<MemoryCapabilityPermission>,
    pub source_revision: String,
    pub expires_at_ms: u64,
}

impl ResolvedMemoryCapability {
    pub fn permits(&self, permission: MemoryCapabilityPermission) -> bool {
        self.permissions.binary_search(&permission).is_ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct MemoryCapabilityDocument {
    schema_version: u8,
    transaction_id: String,
    capability_nonce: String,
    scope: MemoryAccessScope,
    route_binding: MemoryCapabilityRouteBinding,
    permissions: Vec<MemoryCapabilityPermission>,
    source_revision: String,
    issued_at_ms: u64,
    expires_at_ms: u64,
}

#[derive(Debug)]
struct MemoryCapabilityRecord {
    transaction_id: String,
    target_path: PathBuf,
    document_sha256: String,
    nonce_sha256: String,
    scope: MemoryAccessScope,
    route_binding: MemoryCapabilityRouteBinding,
    permissions: Vec<MemoryCapabilityPermission>,
    source_revision: String,
    review_digest: String,
    state: MemoryCapabilityTransactionState,
    issued_at_ms: u64,
    expires_at_ms: u64,
    verified_at_ms: Option<u64>,
    revoked_at_ms: Option<u64>,
}

pub fn prepare_memory_capability_transaction_for_runtime(
    runtime_dir: &Path,
    request: &MemoryCapabilityTransactionRequest,
) -> Result<PreparedMemoryCapabilityTransaction> {
    if !runtime_dir.is_absolute()
        || runtime_dir
            .components()
            .any(|component| component == Component::ParentDir)
    {
        bail!("memory capability runtime directory must be absolute and traversal-free");
    }
    prepare_memory_capability_transaction_at_root(&runtime_dir.join("memory-capabilities"), request)
}

impl ContextStore {
    pub fn prepare_memory_capability_transaction(
        &self,
        request: &MemoryCapabilityTransactionRequest,
    ) -> Result<PreparedMemoryCapabilityTransaction> {
        prepare_memory_capability_transaction_at_root(&self.memory_capability_root(), request)
    }

    pub fn apply_memory_capability_transaction(
        &self,
        prepared: PreparedMemoryCapabilityTransaction,
        reviewed_digest: &str,
    ) -> Result<MemoryCapabilityTransactionReceipt> {
        if reviewed_digest != prepared.plan.review_digest {
            bail!("memory capability review digest is stale");
        }
        validate_memory_capability_document(&prepared.document)?;
        if sha256(&prepared.encoded_document) != prepared.plan.document_sha256 {
            bail!("memory capability document changed after review");
        }
        if fs::symlink_metadata(&prepared.plan.target_path).is_ok() {
            bail!("memory capability target already exists");
        }
        let permissions_json = serde_json::to_string(&prepared.plan.permissions)
            .context("serialize memory capability permissions")?;
        self.connection.execute(
            "INSERT INTO memory_capability_transaction (
                 transaction_id, target_path, document_sha256, nonce_sha256,
                 project_id, agent_id, session_id, route_id, provider_id, model,
                 config_revision, permissions_json, source_revision, review_digest,
                 state, issued_at_ms, expires_at_ms, applied_at_ms, verified_at_ms,
                 revoked_at_ms, failure_reason
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                 'planned', ?15, ?16, NULL, NULL, NULL, NULL
             )",
            params![
                prepared.plan.transaction_id,
                prepared.plan.target_path.to_string_lossy(),
                prepared.plan.document_sha256,
                sha256(prepared.document.capability_nonce.as_bytes()),
                prepared.plan.scope.project_id,
                prepared.plan.scope.agent_id,
                prepared
                    .plan
                    .scope
                    .session_id
                    .as_deref()
                    .unwrap_or_default(),
                prepared.plan.route_binding.route_id,
                prepared.plan.route_binding.provider_id,
                prepared.plan.route_binding.model,
                prepared.plan.route_binding.config_revision,
                permissions_json,
                prepared.plan.source_revision,
                prepared.plan.review_digest,
                capped_i64(prepared.plan.issued_at_ms),
                capped_i64(prepared.plan.expires_at_ms),
            ],
        )?;

        if let Err(error) = write_private_capability_document(
            &self.memory_capability_root(),
            &prepared.plan.target_path,
            &prepared.encoded_document,
        ) {
            let _ = self.fail_memory_capability_transaction(
                &prepared.plan.transaction_id,
                "capability_document_write_failed",
            );
            return Err(error);
        }
        let applied_at_ms = dtoken_types::time::now_ms().max(prepared.plan.issued_at_ms);
        if self.connection.execute(
            "UPDATE memory_capability_transaction
                 SET state = 'active', applied_at_ms = ?1, failure_reason = NULL
                 WHERE transaction_id = ?2 AND state = 'planned'",
            params![capped_i64(applied_at_ms), prepared.plan.transaction_id],
        )? != 1
        {
            let _ = remove_owned_capability_document(
                &prepared.plan.target_path,
                &prepared.plan.document_sha256,
            );
            let _ = self.fail_memory_capability_transaction(
                &prepared.plan.transaction_id,
                "capability_projection_commit_failed",
            );
            bail!("memory capability projection could not be activated");
        }

        match self
            .verify_memory_capability_transaction(&prepared.plan.transaction_id, applied_at_ms)
        {
            Ok(receipt) => Ok(receipt),
            Err(error) => {
                let _ = remove_owned_capability_document(
                    &prepared.plan.target_path,
                    &prepared.plan.document_sha256,
                );
                let _ = self.fail_memory_capability_transaction(
                    &prepared.plan.transaction_id,
                    "capability_post_write_verify_failed",
                );
                Err(error)
            }
        }
    }

    pub fn verify_memory_capability_transaction(
        &self,
        transaction_id: &str,
        observed_at_ms: u64,
    ) -> Result<MemoryCapabilityTransactionReceipt> {
        validate_memory_capability_transaction_id(transaction_id)?;
        if observed_at_ms == 0 {
            bail!("memory capability verification time must be positive");
        }
        let record = self
            .memory_capability_record(transaction_id)?
            .context("memory capability transaction does not exist")?;
        if record.state == MemoryCapabilityTransactionState::Revoked {
            bail!("memory capability transaction is revoked");
        }
        if observed_at_ms >= record.expires_at_ms {
            self.connection.execute(
                "UPDATE memory_capability_transaction
                 SET state = 'expired' WHERE transaction_id = ?1 AND state <> 'revoked'",
                [transaction_id],
            )?;
            bail!("memory capability transaction is expired");
        }
        let document = verify_capability_document_against_record(&record)?;
        validate_memory_capability_document(&document)?;
        self.connection.execute(
            "UPDATE memory_capability_transaction
             SET state = 'verified', verified_at_ms = ?1, failure_reason = NULL
             WHERE transaction_id = ?2 AND state IN ('active', 'verified')",
            params![capped_i64(observed_at_ms), transaction_id],
        )?;
        Ok(receipt_from_record(
            &self
                .memory_capability_record(transaction_id)?
                .context("verified memory capability transaction disappeared")?,
            "Memory capability document and route-bound scope were verified.",
        ))
    }

    pub fn resolve_memory_capability_file(
        &self,
        path: &Path,
        expected_route_binding: &MemoryCapabilityRouteBinding,
        observed_at_ms: u64,
    ) -> Result<ResolvedMemoryCapability> {
        if observed_at_ms == 0 {
            bail!("memory capability resolution time must be positive");
        }
        validate_memory_capability_route_binding(expected_route_binding)?;
        let (bytes, document) = read_memory_capability_document(path)?;
        validate_memory_capability_document(&document)?;
        let record = self
            .memory_capability_record(&document.transaction_id)?
            .context("memory capability transaction does not exist")?;
        if record.state != MemoryCapabilityTransactionState::Verified {
            bail!("memory capability transaction is not verified");
        }
        if observed_at_ms >= record.expires_at_ms {
            self.connection.execute(
                "UPDATE memory_capability_transaction
                 SET state = 'expired' WHERE transaction_id = ?1 AND state = 'verified'",
                [&record.transaction_id],
            )?;
            bail!("memory capability transaction is expired");
        }
        if path != record.target_path || sha256(&bytes) != record.document_sha256 {
            bail!("memory capability file does not match its verified transaction");
        }
        verify_capability_document_fields(&document, &record)?;
        if &record.route_binding != expected_route_binding {
            bail!("memory capability route binding does not match the active route");
        }
        Ok(ResolvedMemoryCapability {
            transaction_id: record.transaction_id,
            scope: record.scope,
            route_binding: record.route_binding,
            permissions: record.permissions,
            source_revision: record.source_revision,
            expires_at_ms: record.expires_at_ms,
        })
    }

    pub fn revoke_memory_capability_transaction(
        &self,
        transaction_id: &str,
        reviewed_document_sha256: &str,
        revoked_at_ms: u64,
    ) -> Result<MemoryCapabilityTransactionReceipt> {
        validate_memory_capability_transaction_id(transaction_id)?;
        if revoked_at_ms == 0 {
            bail!("memory capability revocation time must be positive");
        }
        let record = self
            .memory_capability_record(transaction_id)?
            .context("memory capability transaction does not exist")?;
        if record.document_sha256 != reviewed_document_sha256 {
            bail!("memory capability revocation review is stale");
        }
        if record.state == MemoryCapabilityTransactionState::Revoked {
            return Ok(receipt_from_record(
                &record,
                "Memory capability transaction was already revoked.",
            ));
        }
        remove_owned_capability_document(&record.target_path, &record.document_sha256)?;
        self.connection.execute(
            "UPDATE memory_capability_transaction
             SET state = 'revoked', revoked_at_ms = ?1, failure_reason = NULL
             WHERE transaction_id = ?2",
            params![capped_i64(revoked_at_ms), transaction_id],
        )?;
        Ok(receipt_from_record(
            &self
                .memory_capability_record(transaction_id)?
                .context("revoked memory capability transaction disappeared")?,
            "Memory capability document was removed and the transaction was revoked.",
        ))
    }

    fn memory_capability_root(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("memory-capabilities")
    }

    fn fail_memory_capability_transaction(&self, transaction_id: &str, reason: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE memory_capability_transaction
             SET state = 'failed', failure_reason = ?1 WHERE transaction_id = ?2",
            params![reason, transaction_id],
        )?;
        Ok(())
    }

    fn memory_capability_record(
        &self,
        transaction_id: &str,
    ) -> Result<Option<MemoryCapabilityRecord>> {
        self.connection
            .query_row(
                "SELECT transaction_id, target_path, document_sha256, nonce_sha256,
                        project_id, agent_id, session_id, route_id, provider_id, model,
                        config_revision, permissions_json, source_revision, review_digest,
                        state, issued_at_ms, expires_at_ms, verified_at_ms, revoked_at_ms
                 FROM memory_capability_transaction WHERE transaction_id = ?1",
                [transaction_id],
                memory_capability_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }
}

fn prepare_memory_capability_transaction_at_root(
    capability_root: &Path,
    request: &MemoryCapabilityTransactionRequest,
) -> Result<PreparedMemoryCapabilityTransaction> {
    validate_memory_capability_request(request)?;
    let transaction_id = format!("memcap-{}", Uuid::new_v4());
    let capability_nonce = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let permissions = normalized_permissions(&request.permissions)?;
    let document = MemoryCapabilityDocument {
        schema_version: MEMORY_CAPABILITY_SCHEMA_VERSION,
        transaction_id: transaction_id.clone(),
        capability_nonce,
        scope: request.scope.clone(),
        route_binding: request.route_binding.clone(),
        permissions: permissions.clone(),
        source_revision: request.source_revision.clone(),
        issued_at_ms: request.issued_at_ms,
        expires_at_ms: request.expires_at_ms,
    };
    validate_memory_capability_document(&document)?;
    let encoded_document =
        serde_json::to_vec_pretty(&document).context("serialize memory capability document")?;
    if encoded_document.len() > MAX_MEMORY_CAPABILITY_DOCUMENT_BYTES {
        bail!("memory capability document exceeds its size limit");
    }
    let document_sha256 = sha256(&encoded_document);
    let target_path = capability_root.join(format!("{transaction_id}.json"));
    let review_digest = memory_capability_review_digest(
        &transaction_id,
        &target_path,
        &request.scope,
        &request.route_binding,
        &permissions,
        &request.source_revision,
        request.issued_at_ms,
        request.expires_at_ms,
        &document_sha256,
    )?;
    Ok(PreparedMemoryCapabilityTransaction {
        plan: MemoryCapabilityTransactionPlan {
            schema_version: MEMORY_CAPABILITY_SCHEMA_VERSION,
            transaction_id,
            target_path,
            scope: request.scope.clone(),
            route_binding: request.route_binding.clone(),
            permissions,
            source_revision: request.source_revision.clone(),
            issued_at_ms: request.issued_at_ms,
            expires_at_ms: request.expires_at_ms,
            document_sha256,
            review_digest,
            requires_explicit_confirmation: true,
        },
        document,
        encoded_document,
    })
}

fn memory_capability_record_from_row(row: &Row<'_>) -> rusqlite::Result<MemoryCapabilityRecord> {
    let permissions_json: String = row.get(11)?;
    let permissions = serde_json::from_str(&permissions_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let state: String = row.get(14)?;
    let state = MemoryCapabilityTransactionState::parse(&state).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, error.into())
    })?;
    let session_id: String = row.get(6)?;
    Ok(MemoryCapabilityRecord {
        transaction_id: row.get(0)?,
        target_path: PathBuf::from(row.get::<_, String>(1)?),
        document_sha256: row.get(2)?,
        nonce_sha256: row.get(3)?,
        scope: MemoryAccessScope {
            project_id: row.get(4)?,
            agent_id: row.get(5)?,
            session_id: (!session_id.is_empty()).then_some(session_id),
        },
        route_binding: MemoryCapabilityRouteBinding {
            route_id: row.get(7)?,
            provider_id: row.get(8)?,
            model: row.get(9)?,
            config_revision: row.get(10)?,
        },
        permissions,
        source_revision: row.get(12)?,
        review_digest: row.get(13)?,
        state,
        issued_at_ms: u64::try_from(row.get::<_, i64>(15)?).unwrap_or_default(),
        expires_at_ms: u64::try_from(row.get::<_, i64>(16)?).unwrap_or_default(),
        verified_at_ms: row
            .get::<_, Option<i64>>(17)?
            .and_then(|value| u64::try_from(value).ok()),
        revoked_at_ms: row
            .get::<_, Option<i64>>(18)?
            .and_then(|value| u64::try_from(value).ok()),
    })
}

fn validate_memory_capability_request(request: &MemoryCapabilityTransactionRequest) -> Result<()> {
    validate_memory_access_scope(&request.scope)?;
    validate_memory_capability_route_binding(&request.route_binding)?;
    normalized_permissions(&request.permissions)?;
    validate_scope_identifier(
        "memory capability source revision",
        &request.source_revision,
    )?;
    if request.issued_at_ms == 0
        || request.expires_at_ms <= request.issued_at_ms
        || request.expires_at_ms.saturating_sub(request.issued_at_ms) > MAX_MEMORY_CAPABILITY_TTL_MS
    {
        bail!("memory capability TTL is outside the supported range");
    }
    Ok(())
}

fn validate_memory_capability_document(document: &MemoryCapabilityDocument) -> Result<()> {
    if document.schema_version != MEMORY_CAPABILITY_SCHEMA_VERSION {
        bail!("memory capability document schema is unsupported");
    }
    validate_memory_capability_transaction_id(&document.transaction_id)?;
    if document.capability_nonce.len() != 64
        || !document
            .capability_nonce
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("memory capability nonce is invalid");
    }
    validate_memory_capability_request(&MemoryCapabilityTransactionRequest {
        scope: document.scope.clone(),
        route_binding: document.route_binding.clone(),
        permissions: document.permissions.clone(),
        source_revision: document.source_revision.clone(),
        issued_at_ms: document.issued_at_ms,
        expires_at_ms: document.expires_at_ms,
    })
}

fn validate_memory_capability_route_binding(binding: &MemoryCapabilityRouteBinding) -> Result<()> {
    validate_scope_identifier("memory capability route_id", &binding.route_id)?;
    validate_scope_identifier("memory capability provider_id", &binding.provider_id)?;
    validate_scope_identifier("memory capability model", &binding.model)?;
    validate_scope_identifier(
        "memory capability config revision",
        &binding.config_revision,
    )
}

fn validate_memory_capability_transaction_id(transaction_id: &str) -> Result<()> {
    let Some(value) = transaction_id.strip_prefix("memcap-") else {
        bail!("memory capability transaction id has an invalid prefix");
    };
    Uuid::parse_str(value).context("memory capability transaction id is invalid")?;
    Ok(())
}

fn normalized_permissions(
    permissions: &[MemoryCapabilityPermission],
) -> Result<Vec<MemoryCapabilityPermission>> {
    let permissions = permissions.iter().copied().collect::<BTreeSet<_>>();
    if permissions.is_empty() {
        bail!("memory capability requires at least one permission");
    }
    Ok(permissions.into_iter().collect())
}

#[allow(clippy::too_many_arguments)]
fn memory_capability_review_digest(
    transaction_id: &str,
    target_path: &Path,
    scope: &MemoryAccessScope,
    route_binding: &MemoryCapabilityRouteBinding,
    permissions: &[MemoryCapabilityPermission],
    source_revision: &str,
    issued_at_ms: u64,
    expires_at_ms: u64,
    document_sha256: &str,
) -> Result<String> {
    let value = serde_json::json!({
        "schema_version": MEMORY_CAPABILITY_SCHEMA_VERSION,
        "transaction_id": transaction_id,
        "target_path": target_path,
        "scope": scope,
        "route_binding": route_binding,
        "permissions": permissions,
        "source_revision": source_revision,
        "issued_at_ms": issued_at_ms,
        "expires_at_ms": expires_at_ms,
        "document_sha256": document_sha256,
    });
    Ok(sha256(&serde_json::to_vec(&value)?))
}

fn write_private_capability_document(root: &Path, target: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_MEMORY_CAPABILITY_DOCUMENT_BYTES {
        bail!("memory capability document size is invalid");
    }
    if !root.is_absolute() || target.parent() != Some(root) {
        bail!("memory capability target is outside the private capability root");
    }
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!("memory capability root must be a real directory")
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(root)
                .with_context(|| format!("create memory capability root {}", root.display()))?;
        }
        Err(error) => return Err(error.into()),
    }
    restrict_store_directory_permissions(root)?;
    if fs::symlink_metadata(target).is_ok() {
        bail!("memory capability target already exists");
    }
    let temp = target.with_extension("json.tmp");
    if fs::symlink_metadata(&temp).is_ok() {
        bail!("memory capability temporary target already exists");
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .with_context(|| format!("create memory capability temp file {}", temp.display()))?;
    if let Err(error) = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        restrict_store_permissions(&temp)?;
        fs::rename(&temp, target)
            .with_context(|| format!("install memory capability file {}", target.display()))?;
        restrict_store_permissions(target)?;
        Ok(())
    })() {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

fn read_memory_capability_document(path: &Path) -> Result<(Vec<u8>, MemoryCapabilityDocument)> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("memory capability file is unavailable: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("memory capability file must be a real regular file");
    }
    verify_private_capability_file_permissions(path)?;
    if metadata.len() > u64::try_from(MAX_MEMORY_CAPABILITY_DOCUMENT_BYTES).unwrap_or(u64::MAX) {
        bail!("memory capability file exceeds its size limit");
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or_default());
    fs::File::open(path)?
        .take(
            u64::try_from(MAX_MEMORY_CAPABILITY_DOCUMENT_BYTES)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_MEMORY_CAPABILITY_DOCUMENT_BYTES {
        bail!("memory capability file exceeds its size limit");
    }
    let document = serde_json::from_slice(&bytes).context("memory capability file is invalid")?;
    Ok((bytes, document))
}

fn verify_capability_document_against_record(
    record: &MemoryCapabilityRecord,
) -> Result<MemoryCapabilityDocument> {
    let (bytes, document) = read_memory_capability_document(&record.target_path)?;
    if sha256(&bytes) != record.document_sha256 {
        bail!("memory capability document hash does not match its transaction");
    }
    verify_capability_document_fields(&document, record)?;
    Ok(document)
}

fn verify_capability_document_fields(
    document: &MemoryCapabilityDocument,
    record: &MemoryCapabilityRecord,
) -> Result<()> {
    if document.transaction_id != record.transaction_id
        || sha256(document.capability_nonce.as_bytes()) != record.nonce_sha256
        || document.scope != record.scope
        || document.route_binding != record.route_binding
        || document.permissions != record.permissions
        || document.source_revision != record.source_revision
        || document.issued_at_ms != record.issued_at_ms
        || document.expires_at_ms != record.expires_at_ms
    {
        bail!("memory capability document does not match its transaction metadata");
    }
    Ok(())
}

fn remove_owned_capability_document(path: &Path, expected_sha256: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            bail!("memory capability clear requires a regular owned file")
        }
        Ok(_) => {}
    }
    let (bytes, _) = read_memory_capability_document(path)?;
    if sha256(&bytes) != expected_sha256 {
        bail!("memory capability clear ownership hash does not match");
    }
    fs::remove_file(path)
        .with_context(|| format!("remove memory capability file {}", path.display()))?;
    Ok(())
}

fn receipt_from_record(
    record: &MemoryCapabilityRecord,
    summary: &str,
) -> MemoryCapabilityTransactionReceipt {
    MemoryCapabilityTransactionReceipt {
        schema_version: MEMORY_CAPABILITY_SCHEMA_VERSION,
        transaction_id: record.transaction_id.clone(),
        state: record.state,
        target_path: record.target_path.clone(),
        document_sha256: record.document_sha256.clone(),
        review_digest: record.review_digest.clone(),
        issued_at_ms: record.issued_at_ms,
        expires_at_ms: record.expires_at_ms,
        verified_at_ms: record.verified_at_ms,
        revoked_at_ms: record.revoked_at_ms,
        summary: summary.to_string(),
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(unix)]
fn verify_private_capability_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        bail!("memory capability file permissions are not private");
    }
    Ok(())
}

#[cfg(windows)]
fn verify_private_capability_file_permissions(path: &Path) -> Result<()> {
    super::verify_private_file_acl(path)
}

#[cfg(not(any(unix, windows)))]
fn verify_private_capability_file_permissions(_path: &Path) -> Result<()> {
    bail!("memory capability private files are unavailable on this platform")
}
