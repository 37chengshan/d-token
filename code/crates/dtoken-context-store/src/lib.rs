//! SQLite query projections for redacted runtime state. Raw CCR bytes remain in the separate
//! encrypted private store and are never columns in this database.

use anyhow::{bail, Context, Result};
use dtoken_event_ledger::JsonlStore;
use dtoken_types::{
    agent::{AgentDiscoverySnapshot, DetectedAgent},
    CapabilityEvidence, Protocol, ProviderCacheObservation,
};
use rusqlite::{params, types::Type, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

/// Feature-gated crash-point shim: real injection under `failpoints`, a
/// compile-time no-op otherwise.
#[cfg(feature = "failpoints")]
mod failpoint {
    pub(super) use dtoken_event_ledger::failpoints::maybe_crash;
}

#[cfg(not(feature = "failpoints"))]
mod failpoint {
    pub(super) fn maybe_crash(_name: &'static str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(any(unix, windows))]
use std::{ffi::OsString, path::PathBuf};

#[cfg(windows)]
mod windows_private_acl;

mod memory_capability;

pub use memory_capability::{
    prepare_memory_capability_transaction_for_runtime, MemoryCapabilityPermission,
    MemoryCapabilityRouteBinding, MemoryCapabilityTransactionPlan,
    MemoryCapabilityTransactionReceipt, MemoryCapabilityTransactionRequest,
    MemoryCapabilityTransactionState, PreparedMemoryCapabilityTransaction,
    ResolvedMemoryCapability,
};

#[cfg(windows)]
pub use windows_private_acl::{
    apply_private_directory_acl, apply_private_file_acl, verify_private_directory_acl,
    verify_private_file_acl,
};

const SCHEMA_VERSION: i64 = 7;
const MAX_FAILURE_PATTERN_TTL_MS: u64 = 180 * 24 * 60 * 60 * 1_000;
const MAX_FAILURE_PATTERN_RESULTS: usize = 1_000;

pub struct ContextStore {
    connection: Connection,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestProjection {
    pub request_id: String,
    pub route_id: String,
    pub provider_id: String,
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub status: String,
    pub input_units: Option<u64>,
    pub output_units: Option<u64>,
    pub saved_units: Option<u64>,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CcrMetadataProjection {
    pub recovery_ref: String,
    pub request_id: String,
    pub expires_at_ms: Option<u64>,
    pub plaintext_bytes: Option<u64>,
    pub state: String,
    pub updated_at_ms: u64,
}

/// A closed vocabulary prevents raw upstream errors, prompts, responses, or
/// command output from entering the learning store.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    ProviderAuth,
    ProviderCredentialUnavailable,
    ProviderInvalidJson,
    ProviderInvalidResponse,
    ProviderNetwork,
    ProviderRateOrTimeout,
    ProviderRouteNotFound,
    ProviderServerError,
    ProviderStreamErrorEvent,
    ProviderStreamIncomplete,
    ProviderTimeout,
    RouteUnavailable,
    RuntimeCancelled,
}

impl FailureClass {
    pub fn from_reason_code(value: &str) -> Option<Self> {
        Some(match value {
            "provider_auth" => Self::ProviderAuth,
            "provider_credential_invalid"
            | "provider_credential_reference_invalid"
            | "provider_credential_unavailable" => Self::ProviderCredentialUnavailable,
            "provider_invalid_json" => Self::ProviderInvalidJson,
            "provider_invalid_response" => Self::ProviderInvalidResponse,
            "provider_network" => Self::ProviderNetwork,
            "provider_rate_or_timeout" => Self::ProviderRateOrTimeout,
            "provider_route_not_found" => Self::ProviderRouteNotFound,
            "provider_server_error" => Self::ProviderServerError,
            "provider_stream_error_event" => Self::ProviderStreamErrorEvent,
            "provider_stream_incomplete" => Self::ProviderStreamIncomplete,
            "provider_timeout" => Self::ProviderTimeout,
            "route_unavailable" => Self::RouteUnavailable,
            "runtime_cancelled" => Self::RuntimeCancelled,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderAuth => "provider_auth",
            Self::ProviderCredentialUnavailable => "provider_credential_unavailable",
            Self::ProviderInvalidJson => "provider_invalid_json",
            Self::ProviderInvalidResponse => "provider_invalid_response",
            Self::ProviderNetwork => "provider_network",
            Self::ProviderRateOrTimeout => "provider_rate_or_timeout",
            Self::ProviderRouteNotFound => "provider_route_not_found",
            Self::ProviderServerError => "provider_server_error",
            Self::ProviderStreamErrorEvent => "provider_stream_error_event",
            Self::ProviderStreamIncomplete => "provider_stream_incomplete",
            Self::ProviderTimeout => "provider_timeout",
            Self::RouteUnavailable => "route_unavailable",
            Self::RuntimeCancelled => "runtime_cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailurePatternObservation {
    pub failure_class: FailureClass,
    pub route_id: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub observed_at_ms: u64,
    pub evidence_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailurePatternRecord {
    pub pattern_id: String,
    pub failure_class: FailureClass,
    pub route_id: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub first_observed_at_ms: u64,
    pub last_observed_at_ms: u64,
    pub occurrences: u64,
    pub evidence_revision: String,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryProvenance {
    pub source: String,
    pub source_revision: String,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryWriteRequest {
    pub project_id: String,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub kind: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub dedup_key: String,
    pub entry_revision: String,
    pub provenance: MemoryProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecord {
    pub memory_id: String,
    pub project_id: String,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub kind: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub dedup_key: String,
    pub entry_revision: String,
    pub provenance: MemoryProvenance,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryAccessScope {
    pub project_id: String,
    pub agent_id: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryShareGrantRequest {
    pub project_id: String,
    pub owner_agent_id: String,
    pub grantee_agent_id: String,
    pub grant_revision: String,
    pub granted_at_ms: u64,
}

/// A share authorization is deliberately bound to one project, owner, grantee,
/// and session.  It is not a project-wide Agent permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryShareGrantScope {
    pub project_id: String,
    pub owner_agent_id: String,
    pub grantee_agent_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedMemoryShareGrantRequest {
    pub scope: MemoryShareGrantScope,
    pub grant_revision: String,
    pub granted_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryShareGrant {
    pub grant_id: String,
    pub project_id: String,
    pub owner_agent_id: String,
    pub grantee_agent_id: String,
    pub session_id: String,
    pub grant_revision: String,
    pub granted_at_ms: u64,
    pub revoked_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryTombstone {
    pub memory_id: String,
    pub project_id: String,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub entry_revision: String,
    pub deleted_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryExportEntry {
    pub memory_id: String,
    pub project_id: String,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub kind: String,
    pub summary: Option<String>,
    pub keywords: Vec<String>,
    pub entry_revision: String,
    pub provenance: MemoryProvenance,
    pub state: String,
    pub deleted_at_ms: Option<u64>,
}

pub fn parse_memory_write_request(value: &Value) -> Result<MemoryWriteRequest> {
    let object = value
        .as_object()
        .context("memory write request must be an object")?;
    reject_unknown_fields(
        object,
        &[
            "project_id",
            "agent_id",
            "session_id",
            "kind",
            "summary",
            "keywords",
            "dedup_key",
            "entry_revision",
            "provenance",
        ],
        "memory write request",
    )?;
    let provenance = object
        .get("provenance")
        .and_then(Value::as_object)
        .context("memory provenance must be an object")?;
    reject_unknown_fields(
        provenance,
        &["source", "source_revision", "observed_at_ms"],
        "memory provenance",
    )?;
    let keywords = object
        .get("keywords")
        .and_then(Value::as_array)
        .context("memory keywords must be an array")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .context("memory keyword must be a string")
        })
        .collect::<Result<Vec<_>>>()?;
    let request = MemoryWriteRequest {
        project_id: required_string(object, "project_id")?,
        agent_id: optional_string(object, "agent_id")?,
        session_id: optional_string(object, "session_id")?,
        kind: required_string(object, "kind")?,
        summary: required_string(object, "summary")?,
        keywords,
        dedup_key: required_string(object, "dedup_key")?,
        entry_revision: required_string(object, "entry_revision")?,
        provenance: MemoryProvenance {
            source: required_string(provenance, "source")?,
            source_revision: required_string(provenance, "source_revision")?,
            observed_at_ms: provenance
                .get("observed_at_ms")
                .and_then(Value::as_u64)
                .context("memory provenance observed_at_ms must be a positive integer")?,
        },
    };
    validate_memory_write(&request)?;
    Ok(request)
}

impl ContextStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        ensure_private_storage_platform_support()?;
        let path = path.as_ref();
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!("context store path must be a regular file");
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create context-store directory {}", parent.display()))?;
            restrict_store_directory_permissions(parent)?;
        }
        let mut connection = Connection::open(path)
            .with_context(|| format!("open context store {}", path.display()))?;
        connection.busy_timeout(Duration::from_millis(500))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        migrate(&mut connection)?;
        restrict_store_permissions(path)?;
        restrict_store_sidecar_permissions(path)?;
        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn schema_version(&self) -> Result<i64> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn upsert_capability(&self, evidence: &CapabilityEvidence) -> Result<()> {
        let encoded = serde_json::to_string(evidence)?;
        self.connection.execute(
            "INSERT INTO capability_evidence (
                 capability_id, evidence_json, evidence_revision, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(capability_id) DO UPDATE SET
                 evidence_json = excluded.evidence_json,
                 evidence_revision = excluded.evidence_revision,
                 updated_at_ms = excluded.updated_at_ms",
            params![evidence.id, encoded, evidence.evidence_revision, now_ms()],
        )?;
        Ok(())
    }

    pub fn capability(&self, id: &str) -> Result<Option<CapabilityEvidence>> {
        let encoded = self
            .connection
            .query_row(
                "SELECT evidence_json FROM capability_evidence WHERE capability_id = ?1",
                [id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        encoded
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub fn upsert_request(&self, projection: &RequestProjection) -> Result<()> {
        self.connection.execute(
            "INSERT INTO request_projection (
                 request_id, route_id, provider_id, agent_id, project_id, status,
                 input_units, output_units, saved_units, observed_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(request_id) DO UPDATE SET
                 status = excluded.status,
                 input_units = excluded.input_units,
                 output_units = excluded.output_units,
                 saved_units = excluded.saved_units,
                 observed_at_ms = excluded.observed_at_ms",
            params![
                projection.request_id,
                projection.route_id,
                projection.provider_id,
                projection.agent_id,
                projection.project_id,
                projection.status,
                to_i64(projection.input_units),
                to_i64(projection.output_units),
                to_i64(projection.saved_units),
                capped_i64(projection.observed_at_ms),
            ],
        )?;
        Ok(())
    }

    /// Rebuilds the disposable SQLite request view from the append-only
    /// runtime request journal.  Callers must pass the journal's request logs
    /// in append order; the table is replaced inside one transaction so a
    /// crash mid-rebuild leaves either the old view or the new view, never a
    /// partial one.  This is what makes the cross-store terminal commit
    /// recoverable: the JSONL journal is the durable source of truth and the
    /// SQLite view is always a rebuildable projection of it.
    pub fn replace_request_projections(&mut self, projections: &[RequestProjection]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM request_projection", [])?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO request_projection (
                     request_id, route_id, provider_id, agent_id, project_id, status,
                     input_units, output_units, saved_units, observed_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for projection in projections {
                statement.execute(params![
                    projection.request_id,
                    projection.route_id,
                    projection.provider_id,
                    projection.agent_id,
                    projection.project_id,
                    projection.status,
                    to_i64(projection.input_units),
                    to_i64(projection.output_units),
                    to_i64(projection.saved_units),
                    capped_i64(projection.observed_at_ms),
                ])?;
            }
        }
        // Crash point: the DELETE + INSERTs are staged inside the transaction
        // but not committed.  A crash here rolls back, leaving the previous
        // projection intact — the unique safe terminal state for a rebuild.
        failpoint::maybe_crash("context_store:crash_before_projection_replace_commit")?;
        transaction.commit()?;
        Ok(())
    }

    /// Rebuilds the disposable SQLite request view from the append-only
    /// request journal (PR-C6B).  The JSONL journal is the durable source of
    /// truth; `visit_all` replays every complete record in journal order and
    /// the table is replaced atomically, so the SQLite view is always a
    /// rebuildable projection of the journal and a crash mid-rebuild leaves
    /// either the old view or the new view, never a partial one.
    pub fn rebuild_request_projections_from_journal(
        &mut self,
        journal: &JsonlStore,
    ) -> Result<()> {
        let mut projections = Vec::new();
        journal.visit_all(|projection| {
            projections.push(projection);
            Ok(())
        })?;
        self.replace_request_projections(&projections)
    }

    /// Records only an allowlisted failure classification and redacted route
    /// identity. Replayed or out-of-order observations do not inflate the
    /// occurrence count.
    pub fn record_failure_pattern(
        &self,
        observation: &FailurePatternObservation,
        ttl_ms: u64,
    ) -> Result<FailurePatternRecord> {
        validate_failure_pattern_observation(observation, ttl_ms)?;
        let pattern_id = failure_pattern_id(observation);
        let expires_at_ms = observation
            .observed_at_ms
            .checked_add(ttl_ms)
            .context("failure pattern expiry overflow")?;
        self.connection.execute(
            "INSERT INTO failure_pattern (
                 pattern_id, failure_class, route_id, provider_id, model,
                 agent_id, project_id, first_observed_at_ms, last_observed_at_ms,
                 occurrences, evidence_revision, expires_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1, ?9, ?10)
             ON CONFLICT(pattern_id) DO UPDATE SET
                 last_observed_at_ms = CASE
                   WHEN excluded.last_observed_at_ms > failure_pattern.last_observed_at_ms
                   THEN excluded.last_observed_at_ms
                   ELSE failure_pattern.last_observed_at_ms
                 END,
                 occurrences = CASE
                   WHEN excluded.last_observed_at_ms > failure_pattern.last_observed_at_ms
                   THEN failure_pattern.occurrences + 1
                   ELSE failure_pattern.occurrences
                 END,
                 evidence_revision = CASE
                   WHEN excluded.last_observed_at_ms > failure_pattern.last_observed_at_ms
                   THEN excluded.evidence_revision
                   ELSE failure_pattern.evidence_revision
                 END,
                 expires_at_ms = CASE
                   WHEN excluded.expires_at_ms > failure_pattern.expires_at_ms
                   THEN excluded.expires_at_ms
                   ELSE failure_pattern.expires_at_ms
                 END",
            params![
                pattern_id,
                observation.failure_class.as_str(),
                observation.route_id,
                observation.provider_id,
                observation.model,
                observation.agent_id,
                observation.project_id,
                capped_i64(observation.observed_at_ms),
                observation.evidence_revision,
                capped_i64(expires_at_ms),
            ],
        )?;
        self.failure_pattern(&pattern_id)?
            .context("failure pattern was not readable after upsert")
    }

    pub fn failure_pattern(&self, pattern_id: &str) -> Result<Option<FailurePatternRecord>> {
        validate_failure_pattern_id(pattern_id)?;
        self.connection
            .query_row(
                "SELECT pattern_id, failure_class, route_id, provider_id, model,
                        agent_id, project_id, first_observed_at_ms, last_observed_at_ms,
                        occurrences, evidence_revision, expires_at_ms
                 FROM failure_pattern
                 WHERE pattern_id = ?1",
                [pattern_id],
                failure_pattern_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_failure_patterns(
        &self,
        project_id: Option<&str>,
        agent_id: Option<&str>,
        observed_at_ms: u64,
        limit: usize,
    ) -> Result<Vec<FailurePatternRecord>> {
        if let Some(project_id) = project_id {
            validate_failure_scope_identifier("project_id", project_id)?;
        }
        if let Some(agent_id) = agent_id {
            validate_failure_scope_identifier("agent_id", agent_id)?;
        }
        if observed_at_ms == 0 {
            bail!("failure pattern query time must be positive");
        }
        let limit = limit.clamp(1, MAX_FAILURE_PATTERN_RESULTS);
        let mut statement = self.connection.prepare(
            "SELECT pattern_id, failure_class, route_id, provider_id, model,
                    agent_id, project_id, first_observed_at_ms, last_observed_at_ms,
                    occurrences, evidence_revision, expires_at_ms
             FROM failure_pattern
             WHERE expires_at_ms > ?1
               AND (?2 IS NULL OR project_id = ?2)
               AND (?3 IS NULL OR agent_id = ?3)
             ORDER BY last_observed_at_ms DESC, pattern_id
             LIMIT ?4",
        )?;
        let rows = statement.query_map(
            params![
                capped_i64(observed_at_ms),
                project_id,
                agent_id,
                i64::try_from(limit).unwrap_or(i64::MAX),
            ],
            failure_pattern_from_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn prune_failure_patterns(&self, observed_at_ms: u64) -> Result<usize> {
        if observed_at_ms == 0 {
            bail!("failure pattern prune time must be positive");
        }
        self.connection
            .execute(
                "DELETE FROM failure_pattern WHERE expires_at_ms <= ?1",
                [capped_i64(observed_at_ms)],
            )
            .map_err(Into::into)
    }

    pub fn upsert_ccr_metadata(&self, projection: &CcrMetadataProjection) -> Result<()> {
        self.connection.execute(
            "INSERT INTO ccr_metadata (
                 recovery_ref, request_id, expires_at_ms, plaintext_bytes, state, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(recovery_ref) DO UPDATE SET
                 expires_at_ms = excluded.expires_at_ms,
                 plaintext_bytes = excluded.plaintext_bytes,
                 state = excluded.state,
                 updated_at_ms = excluded.updated_at_ms",
            params![
                projection.recovery_ref,
                projection.request_id,
                to_i64(projection.expires_at_ms),
                to_i64(projection.plaintext_bytes),
                projection.state,
                capped_i64(projection.updated_at_ms),
            ],
        )?;
        Ok(())
    }

    /// Rebuilds the disposable SQLite CCR view from the append-only runtime
    /// journal.  Callers must pass only the latest redacted record per
    /// recovery reference; raw CCR bytes are never accepted here.
    pub fn replace_ccr_metadata(&mut self, projections: &[CcrMetadataProjection]) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM ccr_metadata", [])?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO ccr_metadata (
                     recovery_ref, request_id, expires_at_ms, plaintext_bytes, state, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for projection in projections {
                statement.execute(params![
                    projection.recovery_ref,
                    projection.request_id,
                    to_i64(projection.expires_at_ms),
                    to_i64(projection.plaintext_bytes),
                    projection.state,
                    capped_i64(projection.updated_at_ms),
                ])?;
            }
        }
        // Crash point: staged but not committed — rolls back to the previous
        // CCR view, the unique safe terminal state for a rebuild.
        failpoint::maybe_crash("context_store:crash_before_ccr_replace_commit")?;
        transaction.commit()?;
        Ok(())
    }

    /// Rebuilds the disposable SQLite CCR view from the append-only CCR
    /// journal (PR-C6B).  Only the latest redacted record per recovery
    /// reference is retained (the journal is append-only and may hold
    /// multiple lifecycle states per reference); raw CCR bytes stay in the
    /// private store and are never materialized here.
    pub fn rebuild_ccr_metadata_from_journal(
        &mut self,
        journal: &JsonlStore,
    ) -> Result<()> {
        let mut by_reference = std::collections::BTreeMap::<String, CcrMetadataProjection>::new();
        journal.visit_all(|projection: CcrMetadataProjection| {
            by_reference.insert(projection.recovery_ref.clone(), projection);
            Ok(())
        })?;
        let projections = by_reference.into_values().collect::<Vec<_>>();
        self.replace_ccr_metadata(&projections)
    }

    /// Reads only the durable lifecycle state for a CCR reference.  The
    /// encrypted original stays in the separate private store; callers use
    /// this during recovery to distinguish a completed send from a retained
    /// failed stream without reopening any request body.
    pub fn ccr_metadata_state(&self, recovery_ref: &str) -> Result<Option<String>> {
        if recovery_ref.trim().is_empty() || recovery_ref.len() > 512 {
            bail!("CCR recovery reference is invalid");
        }
        self.connection
            .query_row(
                "SELECT state FROM ccr_metadata WHERE recovery_ref = ?1",
                [recovery_ref],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Removes disposable CCR projections whose authenticated expiry has
    /// passed, returning how many were removed.  The encrypted original's TTL
    /// authority stays in the private store (the envelope carries the
    /// authenticated expiry); this only keeps the SQLite view from
    /// advertising expired availability between journal-driven rebuilds.
    pub fn prune_expired_ccr_metadata(&self, now_ms: u64) -> Result<usize> {
        let changed = self.connection.execute(
            "DELETE FROM ccr_metadata
             WHERE expires_at_ms IS NOT NULL AND expires_at_ms <= ?1",
            [capped_i64(now_ms)],
        )?;
        Ok(changed)
    }

    pub fn upsert_provider_cache_observation(
        &self,
        observation: &ProviderCacheObservation,
    ) -> Result<()> {
        observation
            .validate()
            .map_err(|error| anyhow::anyhow!("invalid provider cache observation: {error:?}"))?;
        self.connection.execute(
            "INSERT INTO provider_cache_observation (
                 route_id, provider_id, model, protocol, config_revision, observation_kind,
                 read_units, creation_units, creation_5m_units, creation_1h_units,
                 source_revision, observed_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(route_id, provider_id, model, protocol, config_revision) DO UPDATE SET
                 observation_kind = excluded.observation_kind,
                 read_units = excluded.read_units,
                 creation_units = excluded.creation_units,
                 creation_5m_units = excluded.creation_5m_units,
                 creation_1h_units = excluded.creation_1h_units,
                 source_revision = excluded.source_revision,
                 observed_at_ms = excluded.observed_at_ms
             WHERE excluded.observed_at_ms >= provider_cache_observation.observed_at_ms",
            params![
                observation.route_id,
                observation.provider_id,
                observation.model,
                serde_json::to_string(&observation.protocol)?,
                observation.config_revision,
                serde_json::to_string(&observation.kind)?,
                to_i64(observation.read_units),
                to_i64(observation.creation_units),
                to_i64(observation.creation_5m_units),
                to_i64(observation.creation_1h_units),
                observation.source_revision,
                capped_i64(observation.observed_at_ms),
            ],
        )?;
        Ok(())
    }

    pub fn provider_cache_observation(
        &self,
        route_id: &str,
        provider_id: &str,
        model: &str,
        protocol: &Protocol,
        config_revision: &str,
    ) -> Result<Option<ProviderCacheObservation>> {
        type ObservationRow = (
            String,
            Option<u64>,
            Option<u64>,
            Option<u64>,
            Option<u64>,
            String,
            u64,
        );
        let protocol_json = serde_json::to_string(protocol)?;
        let row: Option<ObservationRow> = self
            .connection
            .query_row(
                "SELECT observation_kind, read_units, creation_units, creation_5m_units,
                        creation_1h_units, source_revision, observed_at_ms
                 FROM provider_cache_observation
                 WHERE route_id = ?1 AND provider_id = ?2 AND model = ?3
                   AND protocol = ?4 AND config_revision = ?5",
                params![route_id, provider_id, model, protocol_json, config_revision],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((kind_json, read, creation, creation_5m, creation_1h, revision, observed_at)) =
            row
        else {
            return Ok(None);
        };
        let observation = ProviderCacheObservation::new(
            route_id,
            provider_id,
            model,
            protocol.clone(),
            config_revision,
            serde_json::from_str(&kind_json)?,
            read,
            creation,
            creation_5m,
            creation_1h,
            revision,
            observed_at,
        )
        .map_err(|error| anyhow::anyhow!("invalid stored provider cache observation: {error:?}"))?;
        Ok(Some(observation))
    }

    pub fn upsert_memory(&self, request: &MemoryWriteRequest) -> Result<MemoryRecord> {
        validate_memory_write(request)?;
        let agent_id = normalized_optional_scope(request.agent_id.as_deref());
        let session_id = normalized_optional_scope(request.session_id.as_deref());
        let summary = request.summary.trim();
        let keywords = normalized_keywords(&request.keywords)?;
        let keywords_json = serde_json::to_string(&keywords)?;
        let existing = self.memory_by_dedup(
            &request.project_id,
            &agent_id,
            &session_id,
            &request.dedup_key,
        )?;
        if let Some(existing) = existing {
            if existing.entry_revision == request.entry_revision {
                if existing.kind == request.kind
                    && existing.summary == summary
                    && existing.keywords == keywords
                    && existing.provenance == request.provenance
                {
                    return Ok(existing);
                }
                bail!("memory entry revision conflicts with different summary metadata");
            }
            if request.provenance.observed_at_ms < existing.provenance.observed_at_ms {
                bail!("memory entry update is older than the stored provenance");
            }
            self.connection.execute(
                "UPDATE memory_entry SET
                   kind = ?1, summary = ?2, keywords_json = ?3,
                   entry_revision = ?4, provenance_source = ?5,
                   provenance_revision = ?6, observed_at_ms = ?7,
                   state = 'active', updated_at_ms = ?7, deleted_at_ms = NULL
                 WHERE memory_id = ?8 AND state = 'active'",
                params![
                    request.kind,
                    summary,
                    keywords_json,
                    request.entry_revision,
                    request.provenance.source,
                    request.provenance.source_revision,
                    capped_i64(request.provenance.observed_at_ms),
                    existing.memory_id,
                ],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO memory_entry (
                   memory_id, project_id, agent_id, session_id, kind, summary,
                   keywords_json, dedup_key, entry_revision, provenance_source,
                   provenance_revision, observed_at_ms, state, created_at_ms, updated_at_ms
                 ) VALUES (
                   'mem-' || lower(hex(randomblob(16))), ?1, ?2, ?3, ?4, ?5,
                   ?6, ?7, ?8, ?9, ?10, ?11, 'active', ?11, ?11
                 )",
                params![
                    request.project_id,
                    agent_id,
                    session_id,
                    request.kind,
                    summary,
                    keywords_json,
                    request.dedup_key,
                    request.entry_revision,
                    request.provenance.source,
                    request.provenance.source_revision,
                    capped_i64(request.provenance.observed_at_ms),
                ],
            )?;
        }
        self.memory_by_dedup(
            &request.project_id,
            &agent_id,
            &session_id,
            &request.dedup_key,
        )?
        .context("memory entry was not readable after upsert")
    }

    pub fn grant_memory_share(
        &self,
        _request: &MemoryShareGrantRequest,
    ) -> Result<MemoryShareGrant> {
        bail!("memory share grant requires an exact session scope")
    }

    pub fn grant_memory_share_for_scope(
        &self,
        request: &ScopedMemoryShareGrantRequest,
    ) -> Result<MemoryShareGrant> {
        validate_scoped_memory_share_grant(request)?;
        if let Some(existing) = self.memory_share_grant(
            &request.scope.project_id,
            &request.scope.owner_agent_id,
            &request.scope.grantee_agent_id,
            &request.scope.session_id,
        )? {
            if existing.grant_revision == request.grant_revision && existing.revoked_at_ms.is_none()
            {
                return Ok(existing);
            }
            self.connection.execute(
                "UPDATE memory_share_grant SET
                   grant_revision = ?1, granted_at_ms = ?2, revoked_at_ms = NULL
                 WHERE grant_id = ?3",
                params![
                    request.grant_revision,
                    capped_i64(request.granted_at_ms),
                    existing.grant_id,
                ],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO memory_share_grant (
                 grant_id, project_id, owner_agent_id, grantee_agent_id,
                   session_id, grant_revision, granted_at_ms, revoked_at_ms
                 ) VALUES (
                   'grant-' || lower(hex(randomblob(16))), ?1, ?2, ?3, ?4, ?5, ?6, NULL
                 )",
                params![
                    request.scope.project_id,
                    request.scope.owner_agent_id,
                    request.scope.grantee_agent_id,
                    request.scope.session_id,
                    request.grant_revision,
                    capped_i64(request.granted_at_ms),
                ],
            )?;
        }
        self.memory_share_grant(
            &request.scope.project_id,
            &request.scope.owner_agent_id,
            &request.scope.grantee_agent_id,
            &request.scope.session_id,
        )?
        .context("memory share grant was not readable after upsert")
    }

    pub fn revoke_memory_share(
        &self,
        _project_id: &str,
        _owner_agent_id: &str,
        _grantee_agent_id: &str,
        _expected_revision: &str,
        _revoked_at_ms: u64,
    ) -> Result<bool> {
        bail!("memory share revoke requires an exact session scope")
    }

    pub fn revoke_memory_share_for_scope(
        &self,
        scope: &MemoryShareGrantScope,
        expected_revision: &str,
        revoked_at_ms: u64,
    ) -> Result<bool> {
        validate_memory_share_scope(scope)?;
        validate_revision("expected grant revision", expected_revision)?;
        if revoked_at_ms == 0 {
            bail!("memory share revoke time must be positive");
        }
        let changed = self.connection.execute(
            "UPDATE memory_share_grant SET revoked_at_ms = ?1
             WHERE project_id = ?2 AND owner_agent_id = ?3 AND grantee_agent_id = ?4
               AND session_id = ?5 AND grant_revision = ?6 AND revoked_at_ms IS NULL",
            params![
                capped_i64(revoked_at_ms),
                scope.project_id,
                scope.owner_agent_id,
                scope.grantee_agent_id,
                scope.session_id,
                expected_revision,
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn list_visible_memory(&self, scope: &MemoryAccessScope) -> Result<Vec<MemoryRecord>> {
        validate_memory_access_scope(scope)?;
        let session_id = normalized_optional_scope(scope.session_id.as_deref());
        let mut statement = self.connection.prepare(
            "SELECT memory_id, project_id, agent_id, session_id, kind, summary,
                    keywords_json, dedup_key, entry_revision, provenance_source,
                    provenance_revision, observed_at_ms, created_at_ms, updated_at_ms
             FROM memory_entry AS memory
             WHERE memory.project_id = ?1 AND memory.state = 'active'
               AND (
                 memory.agent_id = '' OR memory.agent_id = ?2 OR EXISTS (
                   SELECT 1 FROM memory_share_grant AS grant_record
                   WHERE grant_record.project_id = memory.project_id
                     AND grant_record.owner_agent_id = memory.agent_id
                     AND grant_record.grantee_agent_id = ?2
                     AND grant_record.session_id = memory.session_id
                     AND grant_record.session_id = ?3
                     AND grant_record.revoked_at_ms IS NULL
                 )
               )
               AND (memory.session_id = '' OR memory.session_id = ?3)
             ORDER BY memory.updated_at_ms DESC, memory.memory_id",
        )?;
        let rows = statement.query_map(
            params![scope.project_id, scope.agent_id, session_id],
            memory_record_from_row,
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Returns the safe, typed memory projection only when the caller's scope
    /// can see it. This store never persists or rehydrates raw prompt/response
    /// payloads, so this retrieval has no raw-content mode.
    pub fn visible_memory_metadata(
        &self,
        memory_id: &str,
        scope: &MemoryAccessScope,
    ) -> Result<Option<MemoryRecord>> {
        validate_memory_id(memory_id)?;
        validate_memory_access_scope(scope)?;
        let session_id = normalized_optional_scope(scope.session_id.as_deref());
        self.connection
            .query_row(
                "SELECT memory_id, project_id, agent_id, session_id, kind, summary,
                        keywords_json, dedup_key, entry_revision, provenance_source,
                        provenance_revision, observed_at_ms, created_at_ms, updated_at_ms
                 FROM memory_entry AS memory
                 WHERE memory.memory_id = ?1 AND memory.project_id = ?2
                   AND memory.state = 'active'
                   AND (
                     memory.agent_id = '' OR memory.agent_id = ?3 OR EXISTS (
                       SELECT 1 FROM memory_share_grant AS grant_record
                       WHERE grant_record.project_id = memory.project_id
                         AND grant_record.owner_agent_id = memory.agent_id
                         AND grant_record.grantee_agent_id = ?3
                         AND grant_record.session_id = memory.session_id
                         AND grant_record.session_id = ?4
                         AND grant_record.revoked_at_ms IS NULL
                     )
                   )
                   AND (memory.session_id = '' OR memory.session_id = ?4)",
                params![memory_id, scope.project_id, scope.agent_id, session_id],
                memory_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn tombstone_memory(
        &self,
        memory_id: &str,
        scope: &MemoryAccessScope,
        expected_revision: &str,
        tombstone_revision: &str,
        deleted_at_ms: u64,
    ) -> Result<MemoryTombstone> {
        validate_memory_id(memory_id)?;
        validate_memory_access_scope(scope)?;
        validate_revision("expected memory revision", expected_revision)?;
        validate_revision("tombstone revision", tombstone_revision)?;
        if deleted_at_ms == 0 {
            bail!("memory deletion time must be positive");
        }
        let existing = self
            .memory_for_delete(memory_id)?
            .context("memory entry does not exist")?;
        if existing.project_id != scope.project_id
            || existing
                .agent_id
                .as_deref()
                .is_some_and(|agent_id| agent_id != scope.agent_id)
            || existing.session_id != scope.session_id
        {
            bail!("memory entry is outside the exact deletion scope");
        }
        if existing.entry_revision != expected_revision {
            bail!("memory entry changed before deletion");
        }
        let changed = self.connection.execute(
            "UPDATE memory_entry SET
               summary = '', keywords_json = '[]', entry_revision = ?1,
               state = 'deleted', updated_at_ms = ?2, deleted_at_ms = ?2
             WHERE memory_id = ?3 AND entry_revision = ?4 AND state = 'active'",
            params![
                tombstone_revision,
                capped_i64(deleted_at_ms),
                memory_id,
                expected_revision,
            ],
        )?;
        if changed != 1 {
            bail!("memory entry changed before deletion");
        }
        Ok(MemoryTombstone {
            memory_id: memory_id.to_string(),
            project_id: existing.project_id,
            agent_id: existing.agent_id,
            session_id: existing.session_id,
            entry_revision: tombstone_revision.to_string(),
            deleted_at_ms,
        })
    }

    pub fn export_project_memory(
        &self,
        scope: &MemoryAccessScope,
        include_tombstones: bool,
    ) -> Result<Vec<MemoryExportEntry>> {
        validate_memory_access_scope(scope)?;
        let session_id = normalized_optional_scope(scope.session_id.as_deref());
        let mut statement = self.connection.prepare(
            "SELECT memory.memory_id, memory.project_id, memory.agent_id, memory.session_id,
                    memory.kind, memory.summary, memory.keywords_json, memory.entry_revision,
                    memory.provenance_source, memory.provenance_revision,
                    memory.observed_at_ms, memory.state, memory.deleted_at_ms
             FROM memory_entry AS memory
             WHERE memory.project_id = ?1
               AND (memory.state = 'active' OR (?4 = 1 AND memory.state = 'deleted'))
               AND (
                 memory.agent_id = '' OR memory.agent_id = ?2 OR EXISTS (
                   SELECT 1 FROM memory_share_grant AS grant_record
                   WHERE grant_record.project_id = memory.project_id
                     AND grant_record.owner_agent_id = memory.agent_id
                     AND grant_record.grantee_agent_id = ?2
                     AND grant_record.session_id = memory.session_id
                     AND grant_record.session_id = ?3
                     AND grant_record.revoked_at_ms IS NULL
                 )
               )
               AND (memory.session_id = '' OR memory.session_id = ?3)
             ORDER BY memory.updated_at_ms DESC, memory.memory_id",
        )?;
        let rows = statement.query_map(
            params![
                scope.project_id,
                scope.agent_id,
                session_id,
                i64::from(include_tombstones),
            ],
            memory_export_from_row,
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    fn memory_by_dedup(
        &self,
        project_id: &str,
        agent_id: &str,
        session_id: &str,
        dedup_key: &str,
    ) -> Result<Option<MemoryRecord>> {
        self.connection
            .query_row(
                "SELECT memory_id, project_id, agent_id, session_id, kind, summary,
                        keywords_json, dedup_key, entry_revision, provenance_source,
                        provenance_revision, observed_at_ms, created_at_ms, updated_at_ms
                 FROM memory_entry
                 WHERE project_id = ?1 AND agent_id = ?2 AND session_id = ?3
                   AND dedup_key = ?4 AND state = 'active'",
                params![project_id, agent_id, session_id, dedup_key],
                memory_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    fn memory_for_delete(&self, memory_id: &str) -> Result<Option<MemoryRecord>> {
        self.connection
            .query_row(
                "SELECT memory_id, project_id, agent_id, session_id, kind, summary,
                        keywords_json, dedup_key, entry_revision, provenance_source,
                        provenance_revision, observed_at_ms, created_at_ms, updated_at_ms
                 FROM memory_entry WHERE memory_id = ?1 AND state = 'active'",
                [memory_id],
                memory_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    fn memory_share_grant(
        &self,
        project_id: &str,
        owner_agent_id: &str,
        grantee_agent_id: &str,
        session_id: &str,
    ) -> Result<Option<MemoryShareGrant>> {
        self.connection
            .query_row(
                "SELECT grant_id, project_id, owner_agent_id, grantee_agent_id,
                        session_id, grant_revision, granted_at_ms, revoked_at_ms
                 FROM memory_share_grant
                 WHERE project_id = ?1 AND owner_agent_id = ?2 AND grantee_agent_id = ?3
                   AND session_id = ?4",
                params![project_id, owner_agent_id, grantee_agent_id, session_id],
                |row| {
                    Ok(MemoryShareGrant {
                        grant_id: row.get(0)?,
                        project_id: row.get(1)?,
                        owner_agent_id: row.get(2)?,
                        grantee_agent_id: row.get(3)?,
                        session_id: row.get(4)?,
                        grant_revision: row.get(5)?,
                        granted_at_ms: row.get(6)?,
                        revoked_at_ms: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replace_agent_snapshot(&mut self, snapshot: &AgentDiscoverySnapshot) -> Result<()> {
        if snapshot.observed_at_ms == 0 || snapshot.evidence_revision.trim().is_empty() {
            bail!("Agent discovery snapshot is missing revision or observation time");
        }
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM agent_detection", [])?;
        for agent in &snapshot.agents {
            transaction.execute(
                "INSERT INTO agent_detection (
                     agent_id, agent_json, evidence_revision, observed_at_ms
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    agent.id,
                    serde_json::to_string(agent)?,
                    snapshot.evidence_revision,
                    capped_i64(snapshot.observed_at_ms),
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO runtime_revisions (domain, revision, freshness, collected_at_ms)
             VALUES ('agent_detection', ?1, 'live', ?2)
             ON CONFLICT(domain) DO UPDATE SET
                 revision = excluded.revision,
                 freshness = excluded.freshness,
                 collected_at_ms = excluded.collected_at_ms",
            params![
                snapshot.evidence_revision,
                capped_i64(snapshot.observed_at_ms)
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn agent_snapshot(&self) -> Result<Option<AgentDiscoverySnapshot>> {
        let revision = self
            .connection
            .query_row(
                "SELECT revision, collected_at_ms FROM runtime_revisions
                 WHERE domain = 'agent_detection'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
            )
            .optional()?;
        let Some((evidence_revision, observed_at_ms)) = revision else {
            return Ok(None);
        };
        let mut statement = self
            .connection
            .prepare("SELECT agent_json FROM agent_detection ORDER BY agent_id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut agents = Vec::new();
        for row in rows {
            agents.push(serde_json::from_str::<DetectedAgent>(&row?)?);
        }
        Ok(Some(AgentDiscoverySnapshot {
            observed_at_ms,
            evidence_revision,
            agents,
        }))
    }

    pub fn projection_counts(&self) -> Result<(u64, u64)> {
        let requests =
            self.connection
                .query_row("SELECT COUNT(*) FROM request_projection", [], |row| {
                    row.get::<_, u64>(0)
                })?;
        let ccr = self
            .connection
            .query_row("SELECT COUNT(*) FROM ccr_metadata", [], |row| {
                row.get::<_, u64>(0)
            })?;
        Ok((requests, ccr))
    }

    pub fn table_names(&self) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn column_names(&self, table: &str) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?;
        let rows = statement.query_map([table], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}

fn failure_pattern_from_row(row: &Row<'_>) -> rusqlite::Result<FailurePatternRecord> {
    let failure_class_text: String = row.get(1)?;
    let failure_class = FailureClass::from_reason_code(&failure_class_text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            Type::Text,
            format!("unsupported failure class {failure_class_text}").into(),
        )
    })?;
    Ok(FailurePatternRecord {
        pattern_id: row.get(0)?,
        failure_class,
        route_id: row.get(2)?,
        provider_id: row.get(3)?,
        model: row.get(4)?,
        agent_id: row.get(5)?,
        project_id: row.get(6)?,
        first_observed_at_ms: row.get(7)?,
        last_observed_at_ms: row.get(8)?,
        occurrences: row.get(9)?,
        evidence_revision: row.get(10)?,
        expires_at_ms: row.get(11)?,
    })
}

fn memory_record_from_row(row: &Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let keywords_json: String = row.get(6)?;
    let keywords = serde_json::from_str(&keywords_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, Type::Text, Box::new(error))
    })?;
    let agent_id: String = row.get(2)?;
    let session_id: String = row.get(3)?;
    Ok(MemoryRecord {
        memory_id: row.get(0)?,
        project_id: row.get(1)?,
        agent_id: nonempty_scope(agent_id),
        session_id: nonempty_scope(session_id),
        kind: row.get(4)?,
        summary: row.get(5)?,
        keywords,
        dedup_key: row.get(7)?,
        entry_revision: row.get(8)?,
        provenance: MemoryProvenance {
            source: row.get(9)?,
            source_revision: row.get(10)?,
            observed_at_ms: row.get(11)?,
        },
        created_at_ms: row.get(12)?,
        updated_at_ms: row.get(13)?,
    })
}

fn memory_export_from_row(row: &Row<'_>) -> rusqlite::Result<MemoryExportEntry> {
    let keywords_json: String = row.get(6)?;
    let state: String = row.get(11)?;
    let keywords = if state == "active" {
        serde_json::from_str(&keywords_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(6, Type::Text, Box::new(error))
        })?
    } else {
        Vec::new()
    };
    let summary: String = row.get(5)?;
    let agent_id: String = row.get(2)?;
    let session_id: String = row.get(3)?;
    Ok(MemoryExportEntry {
        memory_id: row.get(0)?,
        project_id: row.get(1)?,
        agent_id: nonempty_scope(agent_id),
        session_id: nonempty_scope(session_id),
        kind: row.get(4)?,
        summary: (state == "active").then_some(summary),
        keywords,
        entry_revision: row.get(7)?,
        provenance: MemoryProvenance {
            source: row.get(8)?,
            source_revision: row.get(9)?,
            observed_at_ms: row.get(10)?,
        },
        state,
        deleted_at_ms: row.get(12)?,
    })
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<()> {
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        bail!("{context} contains unsupported field {field}");
    }
    Ok(())
}

fn required_string(object: &Map<String, Value>, field: &str) -> Result<String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("memory field {field} must be a string"))
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Result<Option<String>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("memory field {field} must be a string or null"),
    }
}

fn validate_memory_write(request: &MemoryWriteRequest) -> Result<()> {
    validate_scope_identifier("project_id", &request.project_id)?;
    if let Some(agent_id) = request.agent_id.as_deref() {
        validate_scope_identifier("agent_id", agent_id)?;
    }
    if let Some(session_id) = request.session_id.as_deref() {
        validate_scope_identifier("session_id", session_id)?;
    }
    validate_scope_identifier("memory kind", &request.kind)?;
    validate_scope_identifier("memory dedup key", &request.dedup_key)?;
    validate_revision("memory entry revision", &request.entry_revision)?;
    validate_scope_identifier("memory provenance source", &request.provenance.source)?;
    validate_revision(
        "memory provenance revision",
        &request.provenance.source_revision,
    )?;
    if request.provenance.observed_at_ms == 0 {
        bail!("memory provenance observation time must be positive");
    }
    let summary = request.summary.trim();
    if summary.is_empty()
        || summary.chars().count() > 4_096
        || summary.chars().any(|character| character == '\0')
    {
        bail!("memory summary must contain 1-4096 safe characters");
    }
    normalized_keywords(&request.keywords)?;
    Ok(())
}

fn validate_failure_pattern_observation(
    observation: &FailurePatternObservation,
    ttl_ms: u64,
) -> Result<()> {
    validate_failure_route_component("route_id", &observation.route_id)?;
    validate_failure_route_component("provider_id", &observation.provider_id)?;
    if let Some(model) = observation.model.as_deref() {
        validate_failure_route_component("model", model)?;
    }
    if let Some(agent_id) = observation.agent_id.as_deref() {
        validate_failure_scope_identifier("agent_id", agent_id)?;
    }
    if let Some(project_id) = observation.project_id.as_deref() {
        validate_failure_scope_identifier("project_id", project_id)?;
    }
    validate_failure_route_component("evidence_revision", &observation.evidence_revision)?;
    if observation.observed_at_ms == 0 {
        bail!("failure pattern observation time must be positive");
    }
    if ttl_ms == 0 || ttl_ms > MAX_FAILURE_PATTERN_TTL_MS {
        bail!("failure pattern TTL is outside the supported range");
    }
    Ok(())
}

fn validate_failure_route_component(field: &str, value: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 192
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.' | ':' | '/' | '@'))
        })
        || is_secret_shaped_failure_metadata(value)
    {
        bail!("{field} must be a bounded, non-secret identifier");
    }
    Ok(())
}

fn validate_failure_scope_identifier(field: &str, value: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':'))
        })
        || is_secret_shaped_failure_metadata(value)
    {
        bail!("{field} must be a bounded, non-secret opaque identifier");
    }
    Ok(())
}

fn is_secret_shaped_failure_metadata(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("xox")
        || [
            "authorization",
            "bearer",
            "api_key",
            "apikey",
            "cookie",
            "password",
            "secret",
            "token=",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn failure_pattern_id(observation: &FailurePatternObservation) -> String {
    let mut digest = Sha256::new();
    for value in [
        observation.failure_class.as_str(),
        observation.route_id.as_str(),
        observation.provider_id.as_str(),
        observation.model.as_deref().unwrap_or_default(),
        observation.agent_id.as_deref().unwrap_or_default(),
        observation.project_id.as_deref().unwrap_or_default(),
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("failure-{:x}", digest.finalize())
}

fn validate_failure_pattern_id(pattern_id: &str) -> Result<()> {
    let Some(digest) = pattern_id.strip_prefix("failure-") else {
        bail!("failure pattern id has an invalid prefix");
    };
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("failure pattern id has an invalid encoding");
    }
    Ok(())
}

fn validate_memory_access_scope(scope: &MemoryAccessScope) -> Result<()> {
    validate_scope_identifier("project_id", &scope.project_id)?;
    validate_scope_identifier("agent_id", &scope.agent_id)?;
    if let Some(session_id) = scope.session_id.as_deref() {
        validate_scope_identifier("session_id", session_id)?;
    }
    Ok(())
}

fn validate_scoped_memory_share_grant(request: &ScopedMemoryShareGrantRequest) -> Result<()> {
    validate_memory_share_scope(&request.scope)?;
    validate_revision("memory share grant revision", &request.grant_revision)?;
    if request.granted_at_ms == 0 {
        bail!("memory share grant time must be positive");
    }
    Ok(())
}

fn validate_memory_share_scope(scope: &MemoryShareGrantScope) -> Result<()> {
    validate_scope_identifier("project_id", &scope.project_id)?;
    validate_scope_identifier("owner_agent_id", &scope.owner_agent_id)?;
    validate_scope_identifier("grantee_agent_id", &scope.grantee_agent_id)?;
    validate_scope_identifier("session_id", &scope.session_id)?;
    if scope.owner_agent_id == scope.grantee_agent_id {
        bail!("memory share grant requires two different Agents");
    }
    Ok(())
}

fn validate_scope_identifier(field: &str, value: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.' | ':' | '/' | '@'))
        })
    {
        bail!("{field} must be a bounded opaque identifier");
    }
    Ok(())
}

fn validate_revision(field: &str, value: &str) -> Result<()> {
    validate_scope_identifier(field, value)
}

fn validate_memory_id(memory_id: &str) -> Result<()> {
    let Some(value) = memory_id.strip_prefix("mem-") else {
        bail!("memory id has an invalid prefix");
    };
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("memory id has an invalid encoding");
    }
    Ok(())
}

fn normalized_keywords(keywords: &[String]) -> Result<Vec<String>> {
    if keywords.len() > 32 {
        bail!("memory keywords exceed the 32 item limit");
    }
    let mut normalized = Vec::with_capacity(keywords.len());
    for keyword in keywords {
        let keyword = keyword.trim().to_lowercase();
        if keyword.is_empty()
            || keyword.chars().count() > 64
            || keyword.chars().any(char::is_control)
        {
            bail!("memory keyword must contain 1-64 safe characters");
        }
        normalized.push(keyword);
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalized_optional_scope(value: Option<&str>) -> String {
    value.map(str::trim).unwrap_or_default().to_string()
}

fn nonempty_scope(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn migrate(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version INTEGER PRIMARY KEY,
             applied_at_ms INTEGER NOT NULL
         );",
    )?;
    let current: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if current > SCHEMA_VERSION {
        bail!("context store schema is newer than this runtime");
    }
    if current < 1 {
        transaction.execute_batch(
            "CREATE TABLE capability_evidence (
                 capability_id TEXT PRIMARY KEY NOT NULL,
                 evidence_json TEXT NOT NULL,
                 evidence_revision TEXT,
                 updated_at_ms INTEGER NOT NULL
             );
             CREATE TABLE runtime_revisions (
                 domain TEXT PRIMARY KEY NOT NULL,
                 revision TEXT NOT NULL,
                 freshness TEXT NOT NULL,
                 collected_at_ms INTEGER NOT NULL
             );
             CREATE TABLE request_projection (
                 request_id TEXT PRIMARY KEY NOT NULL,
                 route_id TEXT NOT NULL,
                 provider_id TEXT NOT NULL,
                 agent_id TEXT,
                 project_id TEXT,
                 status TEXT NOT NULL,
                 input_units INTEGER,
                 output_units INTEGER,
                 saved_units INTEGER,
                 observed_at_ms INTEGER NOT NULL
             );
             CREATE TABLE ccr_metadata (
                 recovery_ref TEXT PRIMARY KEY NOT NULL,
                 request_id TEXT NOT NULL,
                 expires_at_ms INTEGER,
                 plaintext_bytes INTEGER,
                 state TEXT NOT NULL,
                 updated_at_ms INTEGER NOT NULL
             );",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (1, ?1)",
            [now_ms()],
        )?;
    }
    if current < 2 {
        transaction.execute_batch(
            "CREATE TABLE agent_detection (
                 agent_id TEXT PRIMARY KEY NOT NULL,
                 agent_json TEXT NOT NULL,
                 evidence_revision TEXT NOT NULL,
                 observed_at_ms INTEGER NOT NULL
             );",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (2, ?1)",
            [now_ms()],
        )?;
    }
    if current < 3 {
        transaction.execute_batch(
            "CREATE TABLE provider_cache_observation (
                 route_id TEXT NOT NULL,
                 provider_id TEXT NOT NULL,
                 model TEXT NOT NULL,
                 protocol TEXT NOT NULL,
                 config_revision TEXT NOT NULL,
                 observation_kind TEXT NOT NULL,
                 read_units INTEGER,
                 creation_units INTEGER,
                 creation_5m_units INTEGER,
                 creation_1h_units INTEGER,
                 source_revision TEXT NOT NULL,
                 observed_at_ms INTEGER NOT NULL,
                 PRIMARY KEY (route_id, provider_id, model, protocol, config_revision)
             );",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (3, ?1)",
            [now_ms()],
        )?;
    }
    if current < 4 {
        transaction.execute_batch(
            "CREATE TABLE memory_entry (
                 memory_id TEXT PRIMARY KEY NOT NULL,
                 project_id TEXT NOT NULL,
                 agent_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 summary TEXT NOT NULL,
                 keywords_json TEXT NOT NULL,
                 dedup_key TEXT NOT NULL,
                 entry_revision TEXT NOT NULL,
                 provenance_source TEXT NOT NULL,
                 provenance_revision TEXT NOT NULL,
                 observed_at_ms INTEGER NOT NULL,
                 state TEXT NOT NULL CHECK (state IN ('active', 'deleted')),
                 created_at_ms INTEGER NOT NULL,
                 updated_at_ms INTEGER NOT NULL,
                 deleted_at_ms INTEGER
             );
             CREATE UNIQUE INDEX memory_entry_active_dedup
               ON memory_entry(project_id, agent_id, session_id, dedup_key)
               WHERE state = 'active';
             CREATE INDEX memory_entry_scope_lookup
               ON memory_entry(project_id, agent_id, session_id, state, updated_at_ms);
             CREATE INDEX memory_entry_summary_terms
               ON memory_entry(project_id, kind, keywords_json)
               WHERE state = 'active';
             CREATE TABLE memory_share_grant (
                 grant_id TEXT PRIMARY KEY NOT NULL,
                 project_id TEXT NOT NULL,
                 owner_agent_id TEXT NOT NULL,
                 grantee_agent_id TEXT NOT NULL,
                 grant_revision TEXT NOT NULL,
                 granted_at_ms INTEGER NOT NULL,
                 revoked_at_ms INTEGER,
                 UNIQUE(project_id, owner_agent_id, grantee_agent_id),
                 CHECK (owner_agent_id <> grantee_agent_id)
             );
             CREATE INDEX memory_share_grant_lookup
               ON memory_share_grant(project_id, grantee_agent_id, owner_agent_id, revoked_at_ms);",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (4, ?1)",
            [now_ms()],
        )?;
    }
    if current < 5 {
        transaction.execute_batch(
            "ALTER TABLE memory_share_grant RENAME TO memory_share_grant_v4;
             CREATE TABLE memory_share_grant (
                 grant_id TEXT PRIMARY KEY NOT NULL,
                 project_id TEXT NOT NULL,
                 owner_agent_id TEXT NOT NULL,
                 grantee_agent_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 grant_revision TEXT NOT NULL,
                 granted_at_ms INTEGER NOT NULL,
                 revoked_at_ms INTEGER,
                 UNIQUE(project_id, owner_agent_id, grantee_agent_id, session_id),
                 CHECK (owner_agent_id <> grantee_agent_id)
             );",
        )?;
        // v4 grants were project-wide. Keep their audit history but revoke
        // them during migration: no legacy permission may silently acquire a
        // session scope it was never granted.
        transaction.execute(
            "INSERT INTO memory_share_grant (
                 grant_id, project_id, owner_agent_id, grantee_agent_id,
                 session_id, grant_revision, granted_at_ms, revoked_at_ms
             )
             SELECT grant_id, project_id, owner_agent_id, grantee_agent_id,
                    '', grant_revision, granted_at_ms,
                    COALESCE(revoked_at_ms, ?1)
             FROM memory_share_grant_v4",
            [now_ms()],
        )?;
        transaction.execute_batch(
            "DROP TABLE memory_share_grant_v4;
             CREATE INDEX memory_share_grant_lookup
               ON memory_share_grant(
                   project_id, grantee_agent_id, owner_agent_id, session_id, revoked_at_ms
               );",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (5, ?1)",
            [now_ms()],
        )?;
    }
    if current < 6 {
        transaction.execute_batch(
            "CREATE TABLE failure_pattern (
                 pattern_id TEXT PRIMARY KEY NOT NULL,
                 failure_class TEXT NOT NULL,
                 route_id TEXT NOT NULL,
                 provider_id TEXT NOT NULL,
                 model TEXT,
                 agent_id TEXT,
                 project_id TEXT,
                 first_observed_at_ms INTEGER NOT NULL,
                 last_observed_at_ms INTEGER NOT NULL,
                 occurrences INTEGER NOT NULL,
                 evidence_revision TEXT NOT NULL,
                 expires_at_ms INTEGER NOT NULL
             );
             CREATE INDEX failure_pattern_scope_lookup
               ON failure_pattern(project_id, agent_id, expires_at_ms, last_observed_at_ms);
             CREATE INDEX failure_pattern_route_lookup
               ON failure_pattern(route_id, provider_id, failure_class, last_observed_at_ms);",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (6, ?1)",
            [now_ms()],
        )?;
    }
    if current < 7 {
        transaction.execute_batch(
            "CREATE TABLE memory_capability_transaction (
                 transaction_id TEXT PRIMARY KEY NOT NULL,
                 target_path TEXT NOT NULL UNIQUE,
                 document_sha256 TEXT NOT NULL,
                 nonce_sha256 TEXT NOT NULL,
                 project_id TEXT NOT NULL,
                 agent_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 route_id TEXT NOT NULL,
                 provider_id TEXT NOT NULL,
                 model TEXT NOT NULL,
                 config_revision TEXT NOT NULL,
                 permissions_json TEXT NOT NULL,
                 source_revision TEXT NOT NULL,
                 review_digest TEXT NOT NULL,
                 state TEXT NOT NULL,
                 issued_at_ms INTEGER NOT NULL,
                 expires_at_ms INTEGER NOT NULL,
                 applied_at_ms INTEGER,
                 verified_at_ms INTEGER,
                 revoked_at_ms INTEGER,
                 failure_reason TEXT
             );
             CREATE INDEX memory_capability_active_lookup
               ON memory_capability_transaction(
                   state, project_id, agent_id, session_id, expires_at_ms
               );",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (7, ?1)",
            [now_ms()],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn now_ms() -> i64 {
    i64::try_from(dtoken_types::time::now_ms())
        .unwrap_or(i64::MAX)
        .max(1)
}

fn capped_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn to_i64(value: Option<u64>) -> Option<i64> {
    value.map(capped_i64)
}

/// ContextStore is private persistence.  Do not silently enable it on a
/// platform where we have not implemented and verified an equivalent of the
/// Unix owner-only permissions below.
#[cfg(unix)]
fn ensure_private_storage_platform_support() -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn ensure_private_storage_platform_support() -> Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn ensure_private_storage_platform_support() -> Result<()> {
    bail!("context store private persistence is unavailable on this platform")
}

#[cfg(unix)]
fn restrict_store_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(unix)]
fn restrict_store_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn restrict_store_sidecar_permissions(path: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = OsString::from(path.as_os_str());
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        match std::fs::symlink_metadata(&sidecar) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    bail!("context store SQLite sidecar must be a regular file");
                }
                restrict_store_permissions(&sidecar)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_store_directory_permissions(_path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        apply_private_directory_acl(_path)
    }
    #[cfg(not(windows))]
    {
        ensure_private_storage_platform_support()
    }
}

#[cfg(not(unix))]
fn restrict_store_permissions(_path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        apply_private_file_acl(_path)
    }
    #[cfg(not(windows))]
    {
        ensure_private_storage_platform_support()
    }
}

#[cfg(all(test, unix))]
mod permission_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn open_restricts_parent_database_and_existing_sqlite_sidecars() {
        let root = std::env::temp_dir().join(format!(
            "dtoken-context-store-permissions-{}",
            uuid::Uuid::new_v4()
        ));
        let path = root.join("runtime-state.sqlite3");
        let store = ContextStore::open(&path).expect("open private context store");

        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = OsString::from(path.as_os_str());
            sidecar.push(suffix);
            let sidecar = PathBuf::from(sidecar);
            if sidecar.exists() {
                assert_eq!(
                    std::fs::metadata(sidecar).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }

        drop(store);
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(all(test, windows))]
mod windows_permission_tests {
    use super::*;
    use std::{ffi::OsString, path::PathBuf};

    #[test]
    fn open_applies_protected_private_dacl_to_parent_database_and_sqlite_sidecars() {
        let path = std::env::temp_dir()
            .join(format!(
                "dtoken-context-store-private-{}",
                uuid::Uuid::new_v4()
            ))
            .join("runtime-state.sqlite3");
        let store = ContextStore::open(&path).expect("open private context store");
        store
            .upsert_ccr_metadata(&CcrMetadataProjection {
                recovery_ref: format!("ccr:{}", uuid::Uuid::new_v4()),
                request_id: "req-private-acl".to_string(),
                expires_at_ms: Some(123),
                plaintext_bytes: Some(456),
                state: "retained".to_string(),
                updated_at_ms: 789,
            })
            .expect("write SQLite projection");

        let parent = path.parent().expect("context store parent");
        verify_private_directory_acl(parent).expect("verify parent DACL");
        verify_private_file_acl(&path).expect("verify database DACL");
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = OsString::from(path.as_os_str());
            sidecar.push(suffix);
            let sidecar = PathBuf::from(sidecar);
            verify_private_file_acl(&sidecar)
                .unwrap_or_else(|error| panic!("verify sidecar {}: {error:#}", sidecar.display()));
        }

        drop(store);
        let _ = std::fs::remove_dir_all(parent);
    }
}

#[cfg(test)]
mod ccr_metadata_projection_tests {
    use super::*;

    fn fixture_store(name: &str) -> (PathBuf, ContextStore) {
        let root = std::env::temp_dir().join(format!(
            "dtoken-context-store-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        let path = root.join("runtime-state.sqlite3");
        let store = ContextStore::open(&path).expect("open private context store");
        (root, store)
    }

    fn projection(recovery_ref: &str, state: &str, updated_at_ms: u64) -> CcrMetadataProjection {
        CcrMetadataProjection {
            recovery_ref: recovery_ref.to_string(),
            request_id: format!("req-{recovery_ref}"),
            expires_at_ms: Some(10_000),
            plaintext_bytes: Some(64),
            state: state.to_string(),
            updated_at_ms,
        }
    }

    #[test]
    fn replace_ccr_metadata_rolls_back_atomically_on_partial_failure() {
        let (root, mut store) = fixture_store("replace-rollback");
        store
            .upsert_ccr_metadata(&projection("ccr:a", "finalized", 1))
            .expect("seed projection");

        // A duplicate primary key in the replacement batch must fail the
        // whole transaction: the disposable view is only ever swapped
        // atomically, so a partial replace can never hide the previous truth.
        let duplicate_batch = [
            projection("ccr:b", "pending_retained", 2),
            projection("ccr:b", "finalized", 3),
        ];
        assert!(store.replace_ccr_metadata(&duplicate_batch).is_err());
        assert_eq!(
            store
                .ccr_metadata_state("ccr:a")
                .expect("read projection")
                .as_deref(),
            Some("finalized")
        );
        assert_eq!(
            store.ccr_metadata_state("ccr:b").expect("read projection"),
            None
        );

        drop(store);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prune_expired_ccr_metadata_removes_only_expired_rows() {
        let (root, store) = fixture_store("prune-expired");
        store
            .upsert_ccr_metadata(&CcrMetadataProjection {
                expires_at_ms: Some(1_000),
                ..projection("ccr:expired", "finalized", 1)
            })
            .expect("seed expired projection");
        store
            .upsert_ccr_metadata(&CcrMetadataProjection {
                expires_at_ms: Some(3_000),
                ..projection("ccr:alive", "pending_retained", 2)
            })
            .expect("seed live projection");

        let pruned = store
            .prune_expired_ccr_metadata(1_500)
            .expect("prune expired projections");
        assert_eq!(pruned, 1);
        assert_eq!(
            store
                .ccr_metadata_state("ccr:expired")
                .expect("read projection"),
            None
        );
        assert_eq!(
            store
                .ccr_metadata_state("ccr:alive")
                .expect("read projection")
                .as_deref(),
            Some("pending_retained")
        );

        drop(store);
        let _ = std::fs::remove_dir_all(root);
    }
}
