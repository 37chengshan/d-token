use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, VecDeque},
    env, fs,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

const MAX_ENTRIES: usize = 5_000;
const MAX_DEPTH: usize = 4;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const LEARNING_CACHE_SCHEMA_VERSION: i64 = 2;
const MAX_JOB_SNAPSHOT_BYTES: usize = 1024 * 1024;
const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
const MAX_ROOT_PAYLOAD_BYTES: usize = 1024 * 1024;
const MAX_PERSISTED_SOURCES: usize = 64;
const MAX_PERSISTED_CANDIDATES: usize = 1024;
const MAX_PERSISTED_SESSION_RECORDS: usize = 20_000;

type PersistedPayloadRows = Vec<(String, String)>;
type SplitSnapshot = (String, PersistedPayloadRows, PersistedPayloadRows);

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    ".next",
    ".venv",
    "venv",
    "__pycache__",
    ".cache",
    ".idea",
];

const SENSITIVE_NAMES: &[&str] = &[
    ".env",
    ".env.local",
    ".env.production",
    "auth.json",
    "credentials",
    "secrets",
    ".ssh",
    ".gnupg",
    "cookies",
    "tokens",
    "id_rsa",
    "id_ed25519",
];

const PROJECT_MARKERS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "tsconfig.json",
    "vite.config.ts",
    "tauri.conf.json",
    ".git",
];

const AGENT_DOCS: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    ".cursorrules",
    ".clinerules",
];

#[derive(Debug, Error)]
pub enum LearningError {
    #[error("project path is outside the allowed learning boundary")]
    UnsafeRoot,
    #[error("project path is unavailable or unreadable")]
    UnavailableRoot,
}

#[derive(Debug, Error)]
pub enum LearningCacheError {
    #[error("learning cache path must be a regular file")]
    UnsafePath,
    #[error("learning cache is corrupt: {0}")]
    Corrupt(String),
    #[error("learning cache schema {0} is newer than this runtime")]
    NewerSchema(i64),
    #[error("learning cache payload is invalid: {0}")]
    InvalidPayload(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct LearningCache {
    path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CachedScanJob {
    pub job_id: String,
    pub state: String,
    pub phase: String,
    pub saved_at_unix: u64,
    pub updated_at_ms: u64,
    pub snapshot: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RootScanCheckpoint {
    pub root_key: String,
    pub adapter_id: String,
    pub adapter_version: String,
    pub root_mtime_ns: u64,
    pub state: String,
    pub payload: Value,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecordMetadata {
    pub id: String,
    pub agent_id: String,
    pub source_root_id: String,
    pub kind: String,
    pub project_path: Option<String>,
    pub created_at_unix: Option<u64>,
    pub updated_at_unix: Option<u64>,
    pub format_version: String,
    pub parse_state: String,
    pub size_bytes: u64,
    pub mtime_ns: u64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct SourceSummary {
    pub readme: bool,
    pub docs: usize,
    pub scripts: usize,
    pub configs: usize,
    pub agent_docs: Vec<String>,
    pub sensitive_entries_skipped: usize,
    pub unreadable_entries: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ProjectIndex {
    pub project_name: String,
    pub project_path: String,
    pub last_modified_unix: Option<u64>,
    pub file_count: usize,
    pub file_count_bucket: String,
    pub has_git: bool,
    pub project_markers: Vec<String>,
    pub agent_docs: Vec<String>,
    pub language_fingerprints: Vec<String>,
    pub top_level_directories: Vec<String>,
    pub score: u8,
    pub ranking_reasons: Vec<String>,
    pub risk_flags: Vec<String>,
    pub source_summary: SourceSummary,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct LearningContext {
    pub project_name: String,
    pub project_path: String,
    pub summary: String,
    pub tech_stack: Vec<String>,
    pub architecture_notes: Vec<String>,
    pub important_files: Vec<String>,
    pub common_commands: Vec<String>,
    pub conventions: Vec<String>,
    pub risks: Vec<String>,
    pub agent_instructions: Vec<String>,
    pub dtoken_recommendations: Vec<String>,
    pub open_questions: Vec<String>,
    pub generated_at: String,
    pub source_summary: SourceSummary,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ProjectAnalysis {
    pub source: String,
    pub state: String,
    pub index: ProjectIndex,
    pub context: LearningContext,
    pub writes_performed: bool,
    pub privacy: String,
}

#[derive(Default)]
struct ScanEvidence {
    root: PathBuf,
    project_name: String,
    file_count: usize,
    max_modified: Option<u64>,
    has_git: bool,
    markers: BTreeSet<String>,
    agent_docs: BTreeSet<String>,
    languages: BTreeSet<String>,
    top_dirs: BTreeSet<String>,
    important_files: BTreeSet<String>,
    package_scripts: BTreeSet<String>,
    readme: bool,
    docs: usize,
    scripts: usize,
    configs: usize,
    sensitive_skipped: usize,
    unreadable: usize,
    truncated: bool,
}

impl LearningCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LearningCacheError> {
        let path = path.as_ref().to_path_buf();
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(LearningCacheError::UnsafePath);
            }
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        match initialize_learning_cache(&path) {
            Ok(()) => {}
            Err(LearningCacheError::Corrupt(_)) => {
                if let Ok(metadata) = fs::symlink_metadata(&path) {
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        return Err(LearningCacheError::UnsafePath);
                    }
                    fs::remove_file(&path)?;
                }
                initialize_learning_cache(&path)?;
            }
            Err(error) => return Err(error),
        }
        restrict_learning_cache_permissions(&path)?;
        Ok(Self { path })
    }

    pub fn schema_version(&self) -> Result<i64, LearningCacheError> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn recover_interrupted_jobs(
        &self,
        recovered_at_ms: u64,
    ) -> Result<usize, LearningCacheError> {
        if recovered_at_ms == 0 {
            return Err(LearningCacheError::InvalidPayload(
                "recovery time must be positive".to_string(),
            ));
        }
        let connection = self.connect()?;
        connection
            .execute(
                "UPDATE scan_jobs
                 SET state = 'partial', phase = 'restart-recovered', updated_at_ms = ?1
                 WHERE state IN ('running', 'cancelling')",
                [capped_i64(recovered_at_ms)],
            )
            .map_err(Into::into)
    }

    pub fn save_job(&self, job: &CachedScanJob) -> Result<(), LearningCacheError> {
        validate_opaque_id("job id", &job.job_id)?;
        validate_job_state(&job.state)?;
        validate_opaque_id("job phase", &job.phase)?;
        if job.updated_at_ms == 0 {
            return Err(LearningCacheError::InvalidPayload(
                "job update time must be positive".to_string(),
            ));
        }
        let (snapshot_json, sources, candidates) = split_snapshot(&job.snapshot)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO scan_jobs (
               job_id, state, phase, snapshot_json, saved_at_unix, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(job_id) DO UPDATE SET
               state = excluded.state,
               phase = excluded.phase,
               snapshot_json = excluded.snapshot_json,
               saved_at_unix = excluded.saved_at_unix,
               updated_at_ms = excluded.updated_at_ms",
            params![
                job.job_id,
                job.state,
                job.phase,
                snapshot_json,
                capped_i64(job.saved_at_unix),
                capped_i64(job.updated_at_ms),
            ],
        )?;
        transaction.execute(
            "DELETE FROM session_evidence WHERE job_id = ?1",
            [&job.job_id],
        )?;
        transaction.execute(
            "DELETE FROM project_candidates WHERE job_id = ?1",
            [&job.job_id],
        )?;
        for (source_id, evidence_json) in sources {
            transaction.execute(
                "INSERT INTO session_evidence (job_id, source_id, evidence_json)
                 VALUES (?1, ?2, ?3)",
                params![job.job_id, source_id, evidence_json],
            )?;
        }
        for (candidate_id, candidate_json) in candidates {
            transaction.execute(
                "INSERT INTO project_candidates (job_id, candidate_id, candidate_json)
                 VALUES (?1, ?2, ?3)",
                params![job.job_id, candidate_id, candidate_json],
            )?;
        }
        transaction.execute(
            "DELETE FROM scan_jobs
             WHERE job_id NOT IN (
               SELECT job_id FROM scan_jobs ORDER BY updated_at_ms DESC, job_id DESC LIMIT 16
             )",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_job(&self, job_id: &str) -> Result<Option<CachedScanJob>, LearningCacheError> {
        validate_opaque_id("job id", job_id)?;
        let connection = self.connect()?;
        load_job_from_connection(&connection, "WHERE job_id = ?1", [job_id])
    }

    pub fn latest_recoverable_job(&self) -> Result<Option<CachedScanJob>, LearningCacheError> {
        let connection = self.connect()?;
        load_job_from_connection(
            &connection,
            "WHERE state IN ('complete', 'cancelled', 'partial')
             ORDER BY updated_at_ms DESC, job_id DESC LIMIT 1",
            [],
        )
    }

    pub fn save_root_checkpoint(
        &self,
        checkpoint: &RootScanCheckpoint,
    ) -> Result<(), LearningCacheError> {
        validate_opaque_id("root key", &checkpoint.root_key)?;
        validate_opaque_id("adapter id", &checkpoint.adapter_id)?;
        validate_opaque_id("adapter version", &checkpoint.adapter_version)?;
        validate_opaque_id("root state", &checkpoint.state)?;
        if checkpoint.root_mtime_ns == 0 || checkpoint.updated_at_ms == 0 {
            return Err(LearningCacheError::InvalidPayload(
                "root checkpoint timestamps must be positive".to_string(),
            ));
        }
        let payload_json =
            bounded_json(&checkpoint.payload, MAX_ROOT_PAYLOAD_BYTES, "root payload")?;
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO scan_roots (
               root_key, adapter_id, adapter_version, root_mtime_ns,
               state, payload_json, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(root_key) DO UPDATE SET
               adapter_id = excluded.adapter_id,
               adapter_version = excluded.adapter_version,
               root_mtime_ns = excluded.root_mtime_ns,
               state = excluded.state,
               payload_json = excluded.payload_json,
               updated_at_ms = excluded.updated_at_ms",
            params![
                checkpoint.root_key,
                checkpoint.adapter_id,
                checkpoint.adapter_version,
                capped_i64(checkpoint.root_mtime_ns),
                checkpoint.state,
                payload_json,
                capped_i64(checkpoint.updated_at_ms),
            ],
        )?;
        Ok(())
    }

    pub fn root_checkpoint_if_unchanged(
        &self,
        root_key: &str,
        adapter_id: &str,
        adapter_version: &str,
        root_mtime_ns: u64,
    ) -> Result<Option<RootScanCheckpoint>, LearningCacheError> {
        validate_opaque_id("root key", root_key)?;
        validate_opaque_id("adapter id", adapter_id)?;
        validate_opaque_id("adapter version", adapter_version)?;
        if root_mtime_ns == 0 {
            return Ok(None);
        }
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT root_key, adapter_id, adapter_version, root_mtime_ns,
                        state, payload_json, updated_at_ms
                 FROM scan_roots
                 WHERE root_key = ?1 AND adapter_id = ?2 AND adapter_version = ?3
                   AND root_mtime_ns = ?4",
                params![
                    root_key,
                    adapter_id,
                    adapter_version,
                    capped_i64(root_mtime_ns),
                ],
                |row| {
                    let payload_json: String = row.get(5)?;
                    let payload = serde_json::from_str(&payload_json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(RootScanCheckpoint {
                        root_key: row.get(0)?,
                        adapter_id: row.get(1)?,
                        adapter_version: row.get(2)?,
                        root_mtime_ns: row.get(3)?,
                        state: row.get(4)?,
                        payload,
                        updated_at_ms: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replace_session_records(
        &self,
        job_id: &str,
        records: &[SessionRecordMetadata],
    ) -> Result<(), LearningCacheError> {
        validate_opaque_id("job id", job_id)?;
        if records.len() > MAX_PERSISTED_SESSION_RECORDS {
            return Err(LearningCacheError::InvalidPayload(format!(
                "session record collection exceeds the {MAX_PERSISTED_SESSION_RECORDS} item limit"
            )));
        }
        let mut ids = BTreeSet::new();
        for record in records {
            validate_session_record(record)?;
            if !ids.insert(record.id.as_str()) {
                return Err(LearningCacheError::InvalidPayload(
                    "session record ids must be unique within a scan job".to_string(),
                ));
            }
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM session_records WHERE job_id = ?1", [job_id])?;
        for record in records {
            transaction.execute(
                "INSERT INTO session_records (
                   job_id, session_record_id, agent_id, source_root_id, kind,
                   project_path, created_at_unix, updated_at_unix, format_version,
                   parse_state, size_bytes, mtime_ns
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    job_id,
                    record.id,
                    record.agent_id,
                    record.source_root_id,
                    record.kind,
                    record.project_path,
                    record.created_at_unix.map(capped_i64),
                    record.updated_at_unix.map(capped_i64),
                    record.format_version,
                    record.parse_state,
                    capped_i64(record.size_bytes),
                    capped_i64(record.mtime_ns),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn load_session_records(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionRecordMetadata>, LearningCacheError> {
        validate_opaque_id("job id", job_id)?;
        let limit = limit.min(MAX_PERSISTED_SESSION_RECORDS);
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT session_record_id, agent_id, source_root_id, kind,
                    project_path, created_at_unix, updated_at_unix, format_version,
                    parse_state, size_bytes, mtime_ns
             FROM session_records
             WHERE job_id = ?1
             ORDER BY agent_id, session_record_id
             LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![job_id, i64::try_from(limit).unwrap_or(i64::MAX)],
            |row| {
                Ok(SessionRecordMetadata {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    source_root_id: row.get(2)?,
                    kind: row.get(3)?,
                    project_path: row.get(4)?,
                    created_at_unix: row.get(5)?,
                    updated_at_unix: row.get(6)?,
                    format_version: row.get(7)?,
                    parse_state: row.get(8)?,
                    size_bytes: row.get(9)?,
                    mtime_ns: row.get(10)?,
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn connect(&self) -> Result<Connection, LearningCacheError> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_millis(750))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(connection)
    }
}

fn initialize_learning_cache(path: &Path) -> Result<(), LearningCacheError> {
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_millis(750))?;
    let integrity = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
        .map_err(|error| LearningCacheError::Corrupt(error.to_string()))?;
    if integrity != "ok" {
        return Err(LearningCacheError::Corrupt(integrity));
    }
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )?;
    migrate_learning_cache(&mut connection)
}

fn migrate_learning_cache(connection: &mut Connection) -> Result<(), LearningCacheError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at_ms INTEGER NOT NULL
         );",
    )?;
    let current = transaction.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if current > LEARNING_CACHE_SCHEMA_VERSION {
        return Err(LearningCacheError::NewerSchema(current));
    }
    if current < 1 {
        transaction.execute_batch(
            "CREATE TABLE scan_jobs (
               job_id TEXT PRIMARY KEY NOT NULL,
               state TEXT NOT NULL,
               phase TEXT NOT NULL,
               snapshot_json TEXT NOT NULL,
               saved_at_unix INTEGER NOT NULL,
               updated_at_ms INTEGER NOT NULL
             );
             CREATE INDEX scan_jobs_recovery
               ON scan_jobs(state, updated_at_ms DESC);
             CREATE TABLE session_evidence (
               job_id TEXT NOT NULL REFERENCES scan_jobs(job_id) ON DELETE CASCADE,
               source_id TEXT NOT NULL,
               evidence_json TEXT NOT NULL,
               PRIMARY KEY(job_id, source_id)
             );
             CREATE TABLE project_candidates (
               job_id TEXT NOT NULL REFERENCES scan_jobs(job_id) ON DELETE CASCADE,
               candidate_id TEXT NOT NULL,
               candidate_json TEXT NOT NULL,
               PRIMARY KEY(job_id, candidate_id)
             );
             CREATE TABLE scan_roots (
               root_key TEXT PRIMARY KEY NOT NULL,
               adapter_id TEXT NOT NULL,
               adapter_version TEXT NOT NULL,
               root_mtime_ns INTEGER NOT NULL,
               state TEXT NOT NULL,
               payload_json TEXT NOT NULL,
               updated_at_ms INTEGER NOT NULL
             );
             CREATE INDEX scan_roots_checkpoint
               ON scan_roots(adapter_id, adapter_version, root_mtime_ns);",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (1, ?1)",
            [capped_i64(unix_millis_u64())],
        )?;
    }
    if current < 2 {
        transaction.execute_batch(
            "CREATE TABLE session_records (
               job_id TEXT NOT NULL REFERENCES scan_jobs(job_id) ON DELETE CASCADE,
               session_record_id TEXT NOT NULL,
               agent_id TEXT NOT NULL,
               source_root_id TEXT NOT NULL,
               kind TEXT NOT NULL,
               project_path TEXT,
               created_at_unix INTEGER,
               updated_at_unix INTEGER,
               format_version TEXT NOT NULL,
               parse_state TEXT NOT NULL,
               size_bytes INTEGER NOT NULL,
               mtime_ns INTEGER NOT NULL,
               PRIMARY KEY(job_id, session_record_id)
             );
             CREATE INDEX session_records_source
               ON session_records(job_id, agent_id, parse_state);",
        )?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (2, ?1)",
            [capped_i64(unix_millis_u64())],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn load_job_from_connection<const N: usize>(
    connection: &Connection,
    clause: &str,
    parameters: [&str; N],
) -> Result<Option<CachedScanJob>, LearningCacheError> {
    let sql = format!(
        "SELECT job_id, state, phase, snapshot_json, saved_at_unix, updated_at_ms
         FROM scan_jobs {clause}"
    );
    let row = connection
        .query_row(&sql, params_from_iter(parameters), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, u64>(5)?,
            ))
        })
        .optional()?;
    let Some((job_id, state, phase, snapshot_json, saved_at_unix, updated_at_ms)) = row else {
        return Ok(None);
    };
    if snapshot_json.len() > MAX_JOB_SNAPSHOT_BYTES {
        return Err(LearningCacheError::InvalidPayload(
            "stored job snapshot exceeds the read limit".to_string(),
        ));
    }
    let mut snapshot = serde_json::from_str::<Value>(&snapshot_json)?;
    let object = snapshot.as_object_mut().ok_or_else(|| {
        LearningCacheError::InvalidPayload("stored job snapshot is not an object".to_string())
    })?;
    object.insert("job_id".to_string(), Value::String(job_id.clone()));
    object.insert("state".to_string(), Value::String(state.clone()));
    object.insert("phase".to_string(), Value::String(phase.clone()));
    object.insert("saved_at_unix".to_string(), Value::from(saved_at_unix));
    object.insert(
        "sources".to_string(),
        Value::Array(load_payload_rows(
            connection,
            "session_evidence",
            "source_id",
            "evidence_json",
            &job_id,
            MAX_PERSISTED_SOURCES,
        )?),
    );
    object.insert(
        "candidates".to_string(),
        Value::Array(load_payload_rows(
            connection,
            "project_candidates",
            "candidate_id",
            "candidate_json",
            &job_id,
            MAX_PERSISTED_CANDIDATES,
        )?),
    );
    Ok(Some(CachedScanJob {
        job_id,
        state,
        phase,
        saved_at_unix,
        updated_at_ms,
        snapshot,
    }))
}

fn load_payload_rows(
    connection: &Connection,
    table: &str,
    id_column: &str,
    payload_column: &str,
    job_id: &str,
    limit: usize,
) -> Result<Vec<Value>, LearningCacheError> {
    let sql = format!(
        "SELECT {payload_column} FROM {table}
         WHERE job_id = ?1 ORDER BY {id_column} LIMIT ?2"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![job_id, i64::try_from(limit).unwrap_or(i64::MAX)],
        |row| row.get::<_, String>(0),
    )?;
    let mut values = Vec::new();
    for row in rows {
        let encoded = row?;
        if encoded.len() > MAX_EVIDENCE_BYTES {
            return Err(LearningCacheError::InvalidPayload(
                "stored evidence exceeds the read limit".to_string(),
            ));
        }
        values.push(serde_json::from_str(&encoded)?);
    }
    Ok(values)
}

fn split_snapshot(snapshot: &Value) -> Result<SplitSnapshot, LearningCacheError> {
    let mut base = snapshot.clone();
    let object = base.as_object_mut().ok_or_else(|| {
        LearningCacheError::InvalidPayload("job snapshot must be an object".to_string())
    })?;
    let sources = split_payload_array(
        object
            .remove("sources")
            .unwrap_or_else(|| Value::Array(Vec::new())),
        "source",
        MAX_PERSISTED_SOURCES,
    )?;
    let candidates = split_payload_array(
        object
            .remove("candidates")
            .unwrap_or_else(|| Value::Array(Vec::new())),
        "candidate",
        MAX_PERSISTED_CANDIDATES,
    )?;
    let snapshot_json = bounded_json(&base, MAX_JOB_SNAPSHOT_BYTES, "job snapshot")?;
    Ok((snapshot_json, sources, candidates))
}

fn split_payload_array(
    value: Value,
    kind: &str,
    limit: usize,
) -> Result<PersistedPayloadRows, LearningCacheError> {
    let values = value.as_array().ok_or_else(|| {
        LearningCacheError::InvalidPayload(format!("{kind} collection must be an array"))
    })?;
    if values.len() > limit {
        return Err(LearningCacheError::InvalidPayload(format!(
            "{kind} collection exceeds the {limit} item limit"
        )));
    }
    values
        .iter()
        .map(|value| {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| LearningCacheError::InvalidPayload(format!("{kind} id is missing")))?
                .to_string();
            validate_opaque_id(&format!("{kind} id"), &id)?;
            let encoded = bounded_json(value, MAX_EVIDENCE_BYTES, kind)?;
            Ok((id, encoded))
        })
        .collect()
}

fn bounded_json(value: &Value, max_bytes: usize, kind: &str) -> Result<String, LearningCacheError> {
    let encoded = serde_json::to_string(value)?;
    if encoded.len() > max_bytes {
        return Err(LearningCacheError::InvalidPayload(format!(
            "{kind} exceeds the {max_bytes} byte limit"
        )));
    }
    Ok(encoded)
}

fn validate_opaque_id(field: &str, value: &str) -> Result<(), LearningCacheError> {
    if value.is_empty()
        || value.len() > 160
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':'))
        })
    {
        return Err(LearningCacheError::InvalidPayload(format!(
            "{field} must be a bounded opaque identifier"
        )));
    }
    Ok(())
}

fn validate_job_state(state: &str) -> Result<(), LearningCacheError> {
    if !matches!(
        state,
        "idle" | "running" | "cancelling" | "complete" | "cancelled" | "partial" | "error"
    ) {
        return Err(LearningCacheError::InvalidPayload(
            "job state is unsupported".to_string(),
        ));
    }
    Ok(())
}

fn validate_session_record(record: &SessionRecordMetadata) -> Result<(), LearningCacheError> {
    validate_opaque_id("session record id", &record.id)?;
    validate_opaque_id("session record agent id", &record.agent_id)?;
    validate_opaque_id("session record source root id", &record.source_root_id)?;
    validate_opaque_id("session record format version", &record.format_version)?;
    if !matches!(
        record.kind.as_str(),
        "conversation" | "subagent" | "external" | "task"
    ) {
        return Err(LearningCacheError::InvalidPayload(
            "session record kind is unsupported".to_string(),
        ));
    }
    if !matches!(
        record.parse_state.as_str(),
        "linked" | "unlinked" | "corrupt" | "unsupported" | "policy-skipped"
    ) {
        return Err(LearningCacheError::InvalidPayload(
            "session record parse state is unsupported".to_string(),
        ));
    }
    if let Some(path) = record.project_path.as_deref() {
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err(LearningCacheError::InvalidPayload(
                "session record project path is invalid".to_string(),
            ));
        }
    }
    Ok(())
}

fn capped_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn unix_millis_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
        .max(1)
}

#[cfg(unix)]
fn restrict_learning_cache_permissions(path: &Path) -> Result<(), LearningCacheError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_learning_cache_permissions(_path: &Path) -> Result<(), LearningCacheError> {
    Ok(())
}

pub fn index_project(
    path: &Path,
    session_count: usize,
    agent_count: usize,
) -> Result<ProjectIndex, LearningError> {
    let evidence = scan_project(path)?;
    Ok(build_index(&evidence, session_count, agent_count))
}

pub fn analyze_project(path: &Path) -> Result<ProjectAnalysis, LearningError> {
    let evidence = scan_project(path)?;
    let index = build_index(&evidence, 0, 0);
    let mut commands = evidence
        .package_scripts
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>();
    if evidence.markers.contains("Cargo.toml") {
        commands.extend([
            "cargo test --workspace".to_string(),
            "cargo clippy --workspace".to_string(),
        ]);
    }
    if evidence.markers.contains("pyproject.toml") {
        commands.push("python -m pytest".to_string());
    }
    if evidence.markers.contains("go.mod") {
        commands.push("go test ./...".to_string());
    }
    commands.sort();
    commands.dedup();
    commands.truncate(10);

    let architecture_notes = evidence
        .top_dirs
        .iter()
        .filter(|name| {
            matches!(
                name.as_str(),
                "src"
                    | "app"
                    | "apps"
                    | "crates"
                    | "packages"
                    | "services"
                    | "docs"
                    | "tests"
                    | "test"
            )
        })
        .map(|name| format!("Top-level `{name}` directory is present"))
        .take(8)
        .collect::<Vec<_>>();
    let mut conventions = Vec::new();
    if !evidence.agent_docs.is_empty() {
        conventions.push(format!(
            "Read the existing {} before making project changes",
            evidence
                .agent_docs
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if evidence.markers.contains("Cargo.toml") {
        conventions.push("Keep Rust changes formatted and covered by workspace checks".to_string());
    }
    if evidence.markers.contains("package.json") {
        conventions.push(
            "Use the repository package scripts instead of inventing ad-hoc commands".to_string(),
        );
    }
    let mut risks = Vec::new();
    if evidence.sensitive_skipped > 0 {
        risks.push(format!(
            "{} sensitive entries were excluded from analysis",
            evidence.sensitive_skipped
        ));
    }
    if evidence.truncated {
        risks.push("The bounded scan reached its entry limit; results are partial".to_string());
    }
    if evidence.unreadable > 0 {
        risks.push(format!(
            "{} entries were unreadable and skipped",
            evidence.unreadable
        ));
    }
    let mut open_questions = Vec::new();
    if !evidence.readme {
        open_questions.push("No README was found in the bounded project scan".to_string());
    }
    if evidence.agent_docs.is_empty() {
        open_questions.push("No project-level Agent instruction document was found".to_string());
    }
    let context = LearningContext {
        project_name: evidence.project_name.clone(),
        project_path: evidence.root.to_string_lossy().to_string(),
        summary: format!(
            "{} is a local project with {} indexed files, {} configuration markers, and {} documentation files.",
            evidence.project_name,
            evidence.file_count,
            evidence.markers.len(),
            evidence.docs
        ),
        tech_stack: index.language_fingerprints.clone(),
        architecture_notes,
        important_files: evidence.important_files.iter().take(16).cloned().collect(),
        common_commands: commands,
        conventions,
        risks,
        agent_instructions: vec![
            "Respect existing project instructions and keep changes scoped to the requested task".to_string(),
            "Do not read, log, or modify credentials, auth files, tokens, or environment secrets".to_string(),
            "Verify changed behavior with the repository's own checks before reporting completion".to_string(),
        ],
        dtoken_recommendations: vec![
            "Use d-token local routing while preserving the Agent's resolved upstream and model".to_string(),
            "Keep project learning content inside a reviewed d-token managed block with rollback evidence".to_string(),
        ],
        open_questions,
        generated_at: format!("unix:{}", unix_seconds()),
        source_summary: index.source_summary.clone(),
    };
    Ok(ProjectAnalysis {
        source: "dtoken-learning-core".to_string(),
        state: "complete".to_string(),
        index,
        context,
        writes_performed: false,
        privacy: "Bounded local analysis reads only project structure and allowlisted manifests; sensitive paths, dependency trees, source bodies, session bodies, and credentials are excluded".to_string(),
    })
}

fn scan_project(path: &Path) -> Result<ScanEvidence, LearningError> {
    if !path.is_absolute() || path.parent().is_none() || is_sensitive_path(path) {
        return Err(LearningError::UnsafeRoot);
    }
    let canonical = fs::canonicalize(path).map_err(|_| LearningError::UnavailableRoot)?;
    if !canonical.is_dir()
        || canonical.parent().is_none()
        || is_home(&canonical)
        || is_sensitive_path(&canonical)
    {
        return Err(LearningError::UnsafeRoot);
    }
    let root_metadata =
        fs::symlink_metadata(&canonical).map_err(|_| LearningError::UnavailableRoot)?;
    if root_metadata.file_type().is_symlink() {
        return Err(LearningError::UnsafeRoot);
    }
    let project_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string();
    let mut evidence = ScanEvidence {
        root: canonical.clone(),
        project_name,
        ..ScanEvidence::default()
    };
    let mut queue = VecDeque::from([(canonical.clone(), 0_usize)]);
    let mut visited = 0_usize;
    while let Some((current, depth)) = queue.pop_front() {
        if visited >= MAX_ENTRIES {
            evidence.truncated = true;
            break;
        }
        visited += 1;
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => {
                evidence.unreadable += 1;
                continue;
            }
        };
        for entry in entries.flatten() {
            if visited >= MAX_ENTRIES {
                evidence.truncated = true;
                break;
            }
            visited += 1;
            let child = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if is_sensitive_name(&name) || is_sensitive_path(&child) {
                evidence.sensitive_skipped += 1;
                continue;
            }
            let metadata = match fs::symlink_metadata(&child) {
                Ok(metadata) if !metadata.file_type().is_symlink() => metadata,
                _ => {
                    evidence.unreadable += 1;
                    continue;
                }
            };
            evidence.max_modified = evidence.max_modified.max(modified_unix(&metadata));
            if metadata.is_dir() {
                if depth == 0 {
                    evidence.top_dirs.insert(name.clone());
                }
                if name == ".git" {
                    evidence.has_git = true;
                    evidence.markers.insert(name);
                    continue;
                }
                if SKIP_DIRS.iter().any(|skip| *skip == name) || depth >= MAX_DEPTH {
                    continue;
                }
                queue.push_back((child, depth + 1));
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            evidence.file_count += 1;
            let relative = child
                .strip_prefix(&canonical)
                .unwrap_or(&child)
                .to_string_lossy()
                .to_string();
            classify_file(&name, &relative, depth, &mut evidence);
            if depth == 0 && name == "package.json" && metadata.len() <= MAX_MANIFEST_BYTES {
                collect_package_scripts(&child, &mut evidence);
            }
        }
    }
    Ok(evidence)
}

fn classify_file(name: &str, relative: &str, depth: usize, evidence: &mut ScanEvidence) {
    if PROJECT_MARKERS.contains(&name) {
        evidence.markers.insert(name.to_string());
        evidence.important_files.insert(relative.to_string());
    }
    if depth == 0 && AGENT_DOCS.contains(&name) {
        evidence.agent_docs.insert(name.to_string());
        evidence.important_files.insert(relative.to_string());
    }
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("readme") {
        evidence.readme = true;
        evidence.docs += 1;
        evidence.important_files.insert(relative.to_string());
    } else if relative.starts_with("docs/")
        || matches!(
            Path::new(name).extension().and_then(|ext| ext.to_str()),
            Some("md" | "mdx" | "rst")
        )
    {
        evidence.docs += 1;
    }
    if matches!(
        name,
        "package.json"
            | "Cargo.toml"
            | "pyproject.toml"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "Makefile"
            | "justfile"
    ) {
        evidence.configs += 1;
    }
    if matches!(
        Path::new(name).extension().and_then(|ext| ext.to_str()),
        Some("sh" | "zsh" | "fish" | "ps1" | "bat" | "cmd")
    ) {
        evidence.scripts += 1;
    }
    match name {
        "package.json" | "tsconfig.json" | "vite.config.ts" => {
            evidence
                .languages
                .insert("TypeScript/JavaScript".to_string());
        }
        "Cargo.toml" => {
            evidence.languages.insert("Rust".to_string());
        }
        "pyproject.toml" => {
            evidence.languages.insert("Python".to_string());
        }
        "go.mod" => {
            evidence.languages.insert("Go".to_string());
        }
        "pom.xml" | "build.gradle" => {
            evidence.languages.insert("Java/JVM".to_string());
        }
        _ => {}
    }
}

fn collect_package_scripts(path: &Path, evidence: &mut ScanEvidence) {
    let Ok(raw) = fs::read(path) else { return };
    let Ok(value) = serde_json::from_slice::<Value>(&raw) else {
        return;
    };
    let Some(scripts) = value.get("scripts").and_then(Value::as_object) else {
        return;
    };
    for name in scripts.keys().take(32) {
        if !name.is_empty()
            && name.len() <= 48
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '-' | '_'))
        {
            evidence.package_scripts.insert(format!("npm run {name}"));
        }
    }
}

fn build_index(evidence: &ScanEvidence, session_count: usize, agent_count: usize) -> ProjectIndex {
    let now = unix_seconds();
    let age_days = evidence
        .max_modified
        .map(|mtime| now.saturating_sub(mtime) / 86_400)
        .unwrap_or(u64::MAX);
    let recency = match age_days {
        0..=1 => 1.0,
        2..=7 => 0.85,
        8..=30 => 0.6,
        31..=90 => 0.35,
        _ => 0.1,
    };
    let activity = ((session_count as f64 + 1.0).log10() / 2.0).min(1.0);
    let confidence = (if evidence.has_git { 0.4 } else { 0.0 })
        + (if evidence.markers.is_empty() {
            0.0
        } else {
            0.3
        })
        + (if evidence.languages.is_empty() {
            0.0
        } else {
            0.2
        })
        + (if evidence.file_count >= 20 && evidence.file_count < 20_000 {
            0.1
        } else {
            0.0
        });
    let doc_gap = if evidence.agent_docs.is_empty() {
        0.9
    } else {
        0.2
    };
    let complexity = ((evidence.languages.len() as f64 / 4.0) * 0.5
        + if evidence.file_count > 20_000 {
            0.2
        } else if evidence.file_count > 200 {
            0.4
        } else {
            0.25
        })
    .min(1.0);
    let risk = if evidence.truncated || evidence.unreadable > 0 {
        0.5
    } else {
        0.0
    };
    let total =
        (0.22 * recency + 0.18 * activity + 0.18 * confidence + 0.14 * doc_gap + 0.10 * complexity
            - 0.10 * risk)
            .clamp(0.0, 1.0);
    let mut reasons = Vec::new();
    if age_days <= 7 {
        reasons.push("recent-activity".to_string());
    }
    if session_count > 0 {
        reasons.push("agent-sessions".to_string());
    }
    if agent_count > 1 {
        reasons.push("multi-agent".to_string());
    }
    if evidence.has_git {
        reasons.push("git-project".to_string());
    }
    if evidence.agent_docs.is_empty() {
        reasons.push("missing-agent-doc".to_string());
    }
    if evidence.truncated {
        reasons.push("bounded-index-partial".to_string());
    }
    let mut risks = Vec::new();
    if evidence.sensitive_skipped > 0 {
        risks.push("sensitive-entries-excluded".to_string());
    }
    if evidence.unreadable > 0 {
        risks.push("unreadable-entries".to_string());
    }
    if evidence.truncated {
        risks.push("scan-truncated".to_string());
    }
    ProjectIndex {
        project_name: evidence.project_name.clone(),
        project_path: evidence.root.to_string_lossy().to_string(),
        last_modified_unix: evidence.max_modified,
        file_count: evidence.file_count,
        file_count_bucket: file_count_bucket(evidence.file_count).to_string(),
        has_git: evidence.has_git,
        project_markers: evidence.markers.iter().cloned().collect(),
        agent_docs: evidence.agent_docs.iter().cloned().collect(),
        language_fingerprints: evidence.languages.iter().cloned().collect(),
        top_level_directories: evidence.top_dirs.iter().cloned().collect(),
        score: (total * 100.0).round() as u8,
        ranking_reasons: reasons,
        risk_flags: risks,
        source_summary: SourceSummary {
            readme: evidence.readme,
            docs: evidence.docs,
            scripts: evidence.scripts,
            configs: evidence.configs,
            agent_docs: evidence.agent_docs.iter().cloned().collect(),
            sensitive_entries_skipped: evidence.sensitive_skipped,
            unreadable_entries: evidence.unreadable,
            truncated: evidence.truncated,
        },
    }
}

fn is_home(path: &Path) -> bool {
    env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .and_then(|home| fs::canonicalize(home).ok())
        .as_deref()
        == Some(path)
}

fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_NAMES.iter().any(|candidate| *candidate == lower)
        || matches!(
            Path::new(&lower).extension().and_then(|ext| ext.to_str()),
            Some("pem" | "p12" | "key" | "keystore")
        )
}

fn is_sensitive_path(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(value) => value.to_str().is_some_and(is_sensitive_name),
        Component::ParentDir => true,
        _ => false,
    })
}

fn modified_unix(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn file_count_bucket(count: usize) -> &'static str {
    match count {
        0..=19 => "tiny",
        20..=199 => "small",
        200..=1_999 => "medium",
        2_000..=19_999 => "large",
        _ => "huge",
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_uses_allowlisted_structure_and_excludes_sensitive_entries() {
        let root = tempfile::tempdir().expect("fixture");
        fs::create_dir_all(root.path().join("src")).expect("src");
        fs::create_dir_all(root.path().join(".git")).expect("git");
        fs::write(
            root.path().join("README.md"),
            "# private body must not be returned",
        )
        .expect("readme");
        fs::write(
            root.path().join("AGENTS.md"),
            "secret instructions must not be returned",
        )
        .expect("agents");
        fs::write(root.path().join("Cargo.toml"), "[package]\nname='fixture'").expect("cargo");
        fs::write(
            root.path().join("package.json"),
            r#"{"scripts":{"test":"secret-command-body","bad key":"ignored"}}"#,
        )
        .expect("package");
        fs::write(root.path().join(".env"), "API_KEY=must-not-escape").expect("env");

        let analysis = analyze_project(root.path()).expect("analysis");
        let serialized = serde_json::to_string(&analysis).expect("serialize");

        assert!(analysis
            .index
            .language_fingerprints
            .contains(&"Rust".to_string()));
        assert!(analysis
            .index
            .language_fingerprints
            .contains(&"TypeScript/JavaScript".to_string()));
        assert!(analysis
            .context
            .common_commands
            .contains(&"npm run test".to_string()));
        assert!(analysis.index.source_summary.sensitive_entries_skipped >= 1);
        assert!(!serialized.contains("private body"));
        assert!(!serialized.contains("secret instructions"));
        assert!(!serialized.contains("secret-command-body"));
        assert!(!serialized.contains("must-not-escape"));
    }

    #[test]
    fn index_score_reflects_real_agent_activity_and_document_gap() {
        let root = tempfile::tempdir().expect("fixture");
        fs::create_dir_all(root.path().join(".git")).expect("git");
        fs::write(root.path().join("Cargo.toml"), "[package]").expect("cargo");

        let index = index_project(root.path(), 24, 2).expect("index");

        assert!(index.score > 40);
        assert!(index
            .ranking_reasons
            .contains(&"agent-sessions".to_string()));
        assert!(index.ranking_reasons.contains(&"multi-agent".to_string()));
        assert!(index
            .ranking_reasons
            .contains(&"missing-agent-doc".to_string()));
    }

    #[test]
    fn home_and_sensitive_roots_fail_closed() {
        if let Some(home) = env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
            assert!(matches!(
                analyze_project(Path::new(&home)),
                Err(LearningError::UnsafeRoot)
            ));
        }
        let root = tempfile::tempdir().expect("fixture");
        let sensitive = root.path().join("secrets");
        fs::create_dir_all(&sensitive).expect("sensitive");
        assert!(matches!(
            analyze_project(&sensitive),
            Err(LearningError::UnsafeRoot)
        ));
    }

    #[test]
    fn learning_cache_recovers_interrupted_job_with_partial_evidence_after_restart() {
        let root = tempfile::tempdir().expect("fixture");
        let path = root.path().join("learning.sqlite3");
        let cache = LearningCache::open(&path).expect("open cache");
        cache
            .save_job(&CachedScanJob {
                job_id: "learning-restart-1".to_string(),
                state: "running".to_string(),
                phase: "scanning-session-metadata".to_string(),
                saved_at_unix: 1_783_900_000,
                updated_at_ms: 1_783_900_000_000,
                snapshot: serde_json::json!({
                    "job_id": "learning-restart-1",
                    "state": "running",
                    "phase": "scanning-session-metadata",
                    "saved_at_unix": 1_783_900_000,
                    "sources": [{"id": "codex", "state": "scanned"}],
                    "candidates": [{"id": "project-1", "path": "/tmp/project"}]
                }),
            })
            .expect("save running job");
        drop(cache);

        let reopened = LearningCache::open(&path).expect("reopen cache");
        assert_eq!(
            reopened
                .recover_interrupted_jobs(1_783_900_001_000)
                .unwrap(),
            1
        );
        let recovered = reopened
            .load_job("learning-restart-1")
            .expect("load recovered job")
            .expect("recovered job");
        assert_eq!(recovered.state, "partial");
        assert_eq!(recovered.phase, "restart-recovered");
        assert_eq!(recovered.snapshot["sources"][0]["id"], "codex");
        assert_eq!(recovered.snapshot["candidates"][0]["id"], "project-1");
    }

    #[test]
    fn root_checkpoint_requires_exact_adapter_version_and_mtime() {
        let root = tempfile::tempdir().expect("fixture");
        let cache = LearningCache::open(root.path().join("learning.sqlite3")).expect("cache");
        cache
            .save_root_checkpoint(&RootScanCheckpoint {
                root_key: "codex-root-1".to_string(),
                adapter_id: "codex".to_string(),
                adapter_version: "metadata-v1".to_string(),
                root_mtime_ns: 42,
                state: "complete".to_string(),
                payload: serde_json::json!({"session_count": 7}),
                updated_at_ms: 100,
            })
            .expect("save checkpoint");

        assert!(cache
            .root_checkpoint_if_unchanged("codex-root-1", "codex", "metadata-v1", 42)
            .expect("exact checkpoint")
            .is_some());
        assert!(cache
            .root_checkpoint_if_unchanged("codex-root-1", "codex", "metadata-v2", 42)
            .expect("version invalidation")
            .is_none());
        assert!(cache
            .root_checkpoint_if_unchanged("codex-root-1", "codex", "metadata-v1", 43)
            .expect("mtime invalidation")
            .is_none());
    }

    #[test]
    fn session_record_index_round_trips_metadata_only_and_replaces_atomically() {
        let root = tempfile::tempdir().expect("fixture");
        let cache = LearningCache::open(root.path().join("learning.sqlite3")).expect("cache");
        cache
            .save_job(&CachedScanJob {
                job_id: "learning-records-1".to_string(),
                state: "complete".to_string(),
                phase: "complete".to_string(),
                saved_at_unix: 100,
                updated_at_ms: 100_000,
                snapshot: serde_json::json!({
                    "job_id": "learning-records-1",
                    "state": "complete",
                    "phase": "complete",
                    "saved_at_unix": 100,
                    "sources": [],
                    "candidates": []
                }),
            })
            .expect("save job");
        let record = SessionRecordMetadata {
            id: "sha256-0123456789abcdef".to_string(),
            agent_id: "codex".to_string(),
            source_root_id: "codex-active".to_string(),
            kind: "conversation".to_string(),
            project_path: Some("/tmp/project-a".to_string()),
            created_at_unix: Some(90),
            updated_at_unix: Some(99),
            format_version: "codex-jsonl-v1".to_string(),
            parse_state: "linked".to_string(),
            size_bytes: 4096,
            mtime_ns: 99_000_000_000,
        };
        cache
            .replace_session_records("learning-records-1", std::slice::from_ref(&record))
            .expect("replace records");

        assert_eq!(
            cache
                .load_session_records("learning-records-1", 10)
                .expect("load records"),
            vec![record]
        );

        cache
            .replace_session_records("learning-records-1", &[])
            .expect("clear records");
        assert!(cache
            .load_session_records("learning-records-1", 10)
            .expect("load cleared records")
            .is_empty());
    }

    #[test]
    fn corrupt_learning_cache_is_rebuilt_without_exposing_old_bytes() {
        let root = tempfile::tempdir().expect("fixture");
        let path = root.path().join("learning.sqlite3");
        fs::write(&path, b"not-a-sqlite-database secret-must-not-survive").expect("corrupt cache");

        let cache = LearningCache::open(&path).expect("rebuild corrupt cache");
        assert_eq!(cache.schema_version().expect("schema"), 2);
        assert!(cache
            .latest_recoverable_job()
            .expect("latest job")
            .is_none());
        let bytes = fs::read(path).expect("rebuilt bytes");
        assert!(!bytes
            .windows(b"secret-must-not-survive".len())
            .any(|window| window == b"secret-must-not-survive"));
    }
}
