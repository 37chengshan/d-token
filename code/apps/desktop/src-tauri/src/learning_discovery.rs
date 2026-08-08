use crate::agent_registry::{
    agent_descriptor, scan_current_process_agents, AgentEvidenceKind, AgentSessionCapability,
};
use dtoken_learning::{CachedScanJob, LearningCache, RootScanCheckpoint, SessionRecordMetadata};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::env;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Emitter;
use url::Url;

const CACHE_TTL_SECONDS: u64 = 15 * 60;
const MAX_ROOT_ENTRIES: usize = 5_000;
const MAX_SCAN_DEPTH: usize = 4;
const MAX_SESSION_LINE_BYTES: usize = 256 * 1024;
const MAX_SESSION_METADATA_BYTES_PER_FILE: usize = 512 * 1024;
const MAX_SESSION_METADATA_LINES_PER_FILE: usize = 16;
#[cfg(not(windows))]
const MAX_CLAUDE_MAPPING_VISITS: usize = 512;
const SESSION_ADAPTER_CACHE_VERSION: &str = "session-metadata-v3";

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct StartLearningDiscoveryRequest {
    pub project_roots: Option<Vec<String>>,
    pub skip_paths: Option<Vec<String>>,
    pub refresh: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LearningCandidate {
    id: String,
    path: String,
    agent: String,
    kind: String,
    mtime: Option<u64>,
    activity: String,
    confidence: String,
    value: String,
    reasons: Vec<String>,
    freshness: String,
    session_count: usize,
    project_count: usize,
    skipped_count: usize,
    write_eligible: bool,
    #[serde(default)]
    score: u8,
    #[serde(default)]
    project_markers: Vec<String>,
    #[serde(default)]
    language_fingerprints: Vec<String>,
    #[serde(default)]
    has_agent_doc: bool,
    #[serde(default)]
    risk_flags: Vec<String>,
    #[serde(default)]
    file_count_bucket: String,
    #[serde(default)]
    ranking_reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LearningSourceState {
    id: String,
    label: String,
    state: String,
    roots_found: usize,
    entry_count: usize,
    session_count: usize,
    #[serde(default)]
    conversation_count: usize,
    #[serde(default)]
    subagent_count: usize,
    #[serde(default)]
    linked_count: usize,
    #[serde(default)]
    unlinked_count: usize,
    #[serde(default)]
    corrupt_count: usize,
    #[serde(default)]
    unsupported_count: usize,
    #[serde(default)]
    truncated: bool,
    project_count: usize,
    skipped_count: usize,
    detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LearningProgress {
    scanned_agents: usize,
    total_agents: usize,
    session_records: usize,
    parsed_projects: usize,
    candidate_count: usize,
    skipped: usize,
    risks: usize,
    current_stage: String,
    cancellable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LearningJobSnapshot {
    ok: bool,
    job_id: String,
    state: String,
    phase: String,
    source: String,
    freshness: String,
    cache_state: String,
    saved_at_unix: u64,
    progress: LearningProgress,
    sources: Vec<LearningSourceState>,
    candidates: Vec<LearningCandidate>,
    writes_performed: bool,
    privacy: String,
}

#[derive(Clone)]
struct LearningJob {
    snapshot: LearningJobSnapshot,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedAdapterScan {
    source: LearningSourceState,
    candidates: Vec<LearningCandidate>,
    #[serde(default)]
    records: Vec<SessionRecordMetadata>,
}

#[derive(Clone)]
struct AgentSessionAdapter {
    id: &'static str,
    label: &'static str,
    roots: Vec<PathBuf>,
    parser_supported: bool,
}

#[derive(Default)]
struct RootSummary {
    session_count: usize,
    project_count: usize,
    skipped_count: usize,
    modified: Option<u64>,
    truncated: bool,
}

#[derive(Default, Deserialize)]
struct ProjectPathFields {
    cwd: Option<String>,
    project: Option<String>,
    root: Option<String>,
    source: Option<Value>,
}

#[derive(Default, Deserialize)]
struct CodexMetadataLine {
    #[serde(rename = "type")]
    record_type: Option<String>,
    cwd: Option<String>,
    project: Option<String>,
    root: Option<String>,
    payload: Option<ProjectPathFields>,
}

#[derive(Default, Deserialize)]
struct ClaudeMetadataLine {
    cwd: Option<String>,
}

#[derive(Default, Deserialize)]
struct GenericMetadataLine {
    cwd: Option<String>,
    directory: Option<String>,
    project: Option<String>,
    root: Option<String>,
    payload: Option<ProjectPathFields>,
}

#[derive(Deserialize)]
struct CursorWorkspaceMetadata {
    folder: String,
}

#[derive(Default, Deserialize)]
struct OpenCodeLegacyTime {
    created: Option<i64>,
    updated: Option<i64>,
}

#[derive(Default, Deserialize)]
struct OpenCodeLegacySessionMetadata {
    id: Option<String>,
    directory: Option<String>,
    time: Option<OpenCodeLegacyTime>,
}

struct OpenCodeSessionObservation {
    identity: String,
    source_root: PathBuf,
    project: Option<PathBuf>,
    created_at_unix: Option<u64>,
    updated_at_unix: Option<u64>,
    format_version: &'static str,
    size_bytes: u64,
    mtime_ns: u64,
}

static LEARNING_JOBS: OnceLock<Mutex<HashMap<String, LearningJob>>> = OnceLock::new();

pub(crate) fn start(
    request: StartLearningDiscoveryRequest,
    app: Option<tauri::AppHandle>,
) -> Value {
    let job_id = format!("learning-{}-{}", std::process::id(), unix_millis());
    let cache = open_learning_cache().ok();
    if let Some(cache) = cache.as_ref() {
        let _ = cache.recover_interrupted_jobs(unix_millis_u64());
    }
    let cached = if request.refresh.unwrap_or(false) {
        None
    } else {
        cache.as_ref().and_then(load_latest_snapshot)
    };
    let cache_state = cached
        .as_ref()
        .map(|snapshot| {
            if unix_seconds().saturating_sub(snapshot.saved_at_unix) <= CACHE_TTL_SECONDS {
                "fresh"
            } else {
                "stale"
            }
        })
        .unwrap_or("missing")
        .to_string();
    let mut snapshot = cached.unwrap_or_else(|| empty_snapshot(&job_id));
    snapshot.job_id = job_id.clone();
    snapshot.state = "running".to_string();
    snapshot.phase = "preparing".to_string();
    snapshot.cache_state = cache_state;
    snapshot.progress.current_stage = "preparing".to_string();
    snapshot.progress.cancellable = true;
    snapshot.freshness = super::now_timestamp();
    snapshot.ok = true;

    let jobs = LEARNING_JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for job in guard
        .values_mut()
        .filter(|job| matches!(job.snapshot.state.as_str(), "running" | "cancelling"))
    {
        job.cancelled.store(true, Ordering::Release);
    }
    if guard.len() >= 16 {
        guard.retain(|_, job| matches!(job.snapshot.state.as_str(), "running" | "cancelling"));
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    guard.insert(
        job_id.clone(),
        LearningJob {
            snapshot: snapshot.clone(),
            cancelled: Arc::clone(&cancelled),
        },
    );
    drop(guard);
    let _ = persist_job_snapshot(&snapshot);

    let background_job_id = job_id.clone();
    thread::spawn(move || run_job(background_job_id, request, app, cancelled));
    to_value(snapshot)
}

pub(crate) fn status(job_id: &str) -> Value {
    let jobs = LEARNING_JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    let in_memory = jobs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(job_id)
        .map(|job| job.snapshot.clone());
    if let Some(snapshot) = in_memory {
        return to_value(snapshot);
    }
    if let Some(snapshot) = open_learning_cache()
        .ok()
        .and_then(|cache| {
            let _ = cache.recover_interrupted_jobs(unix_millis_u64());
            cache.load_job(job_id).ok().flatten()
        })
        .and_then(cached_snapshot)
    {
        return to_value(snapshot);
    }
    json!({
        "ok": false,
        "job_id": job_id,
        "state": "missing",
        "source": "d-token-learning-discovery",
        "candidates": [],
        "sources": [],
        "writes_performed": false
    })
}

pub(crate) fn cancel(job_id: &str) -> Value {
    let jobs = LEARNING_JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(job) = guard.get_mut(job_id) else {
        drop(guard);
        return status(job_id);
    };
    job.cancelled.store(true, Ordering::Release);
    job.snapshot.state = "cancelling".to_string();
    job.snapshot.phase = "cancelling".to_string();
    job.snapshot.progress.current_stage = "cancelling".to_string();
    to_value(job.snapshot.clone())
}

pub(crate) fn candidates(job_id: &str) -> Value {
    let snapshot = status(job_id);
    json!({
        "ok": snapshot.get("ok").cloned().unwrap_or(Value::Bool(false)),
        "job_id": job_id,
        "state": snapshot.get("state").cloned().unwrap_or(Value::String("missing".to_string())),
        "source": "d-token-learning-discovery",
        "candidates": snapshot.get("candidates").cloned().unwrap_or_else(|| json!([])),
        "writes_performed": false
    })
}

fn run_job(
    job_id: String,
    request: StartLearningDiscoveryRequest,
    app: Option<tauri::AppHandle>,
    cancelled: Arc<AtomicBool>,
) {
    let adapters = session_adapters();
    let session_roots = adapters
        .iter()
        .flat_map(|adapter| adapter.roots.iter().cloned())
        .collect::<Vec<_>>();
    update_job(&job_id, |snapshot| {
        snapshot.phase = "identifying-agent-sources".to_string();
        snapshot.sources.clear();
        snapshot.candidates.clear();
        snapshot.progress = LearningProgress {
            scanned_agents: 0,
            total_agents: adapters.len(),
            session_records: 0,
            parsed_projects: 0,
            candidate_count: 0,
            skipped: 0,
            risks: 0,
            current_stage: "identifying-agent-sources".to_string(),
            cancellable: true,
        };
    });
    emit_job(&job_id, app.as_ref());

    let refresh = request.refresh.unwrap_or(false);
    let skipped = request
        .skip_paths
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let cache = open_learning_cache().ok();
    let mut session_records = Vec::new();
    let mut persistence_failed = cache.is_none();
    for adapter in adapters {
        if cancelled.load(Ordering::Acquire) {
            finish_cancelled(&job_id, app.as_ref());
            return;
        }
        let checkpoint = (!refresh && skipped.is_empty())
            .then(|| load_adapter_checkpoint(cache.as_ref(), &adapter))
            .flatten();
        let (source, mut found, mut records) = if let Some(checkpoint) = checkpoint {
            (checkpoint.source, checkpoint.candidates, checkpoint.records)
        } else {
            let scanned = scan_adapter(&adapter, &skipped, || cancelled.load(Ordering::Acquire));
            if !cancelled.load(Ordering::Acquire)
                && save_adapter_checkpoint(
                    cache.as_ref(),
                    &adapter,
                    &scanned.0,
                    &scanned.1,
                    &scanned.2,
                )
                .is_err()
            {
                persistence_failed = true;
            }
            scanned
        };
        session_records.append(&mut records);
        update_job(&job_id, |snapshot| {
            snapshot.progress.scanned_agents += 1;
            snapshot.progress.session_records += source.session_count;
            snapshot.progress.parsed_projects += source.project_count;
            snapshot.progress.skipped += source.skipped_count;
            snapshot.sources.push(source);
            snapshot.candidates.append(&mut found);
            snapshot.progress.candidate_count = snapshot.candidates.len();
            snapshot.phase = "scanning-session-metadata".to_string();
            snapshot.progress.current_stage = "scanning-session-metadata".to_string();
            snapshot.freshness = super::now_timestamp();
        });
        if let Some(cache) = cache.as_ref() {
            if cache
                .replace_session_records(&job_id, &session_records)
                .is_err()
            {
                persistence_failed = true;
            }
        }
        emit_job(&job_id, app.as_ref());
    }

    for root in request
        .project_roots
        .unwrap_or_default()
        .into_iter()
        .take(32)
    {
        let path = PathBuf::from(&root);
        if skipped.iter().any(|skip| skip == &path)
            || !super::learning_allowed_root(&path)
            || !path.is_dir()
        {
            continue;
        }
        let Some(path) = canonical_project_path(&path) else {
            continue;
        };
        if path_is_within_roots(&path, &session_roots) {
            continue;
        }
        let mut summary = scan_root_metadata(&path, 1, 512, || cancelled.load(Ordering::Acquire))
            .unwrap_or_default();
        summary.session_count = 0;
        summary.project_count = 1;
        let candidate = candidate_for_project(
            "Project",
            &path,
            &summary,
            "user-selected-project-root",
            "User-selected project root exists and passed the local safety boundary",
        );
        update_job(&job_id, |snapshot| {
            snapshot.progress.session_records += summary.session_count;
            snapshot.progress.parsed_projects += 1;
            snapshot.progress.skipped += summary.skipped_count;
            snapshot.candidates.push(candidate);
            snapshot.progress.candidate_count = snapshot.candidates.len();
        });
    }

    if cancelled.load(Ordering::Acquire) {
        finish_cancelled(&job_id, app.as_ref());
        return;
    }
    let mut candidates = snapshot_for(&job_id)
        .map(|snapshot| aggregate_project_candidates(snapshot.candidates))
        .unwrap_or_default();
    update_job(&job_id, |snapshot| {
        snapshot.phase = "scoring-candidates".to_string();
        snapshot.progress.current_stage = "scoring-candidates".to_string();
        snapshot.candidates = candidates.clone();
        snapshot.progress.candidate_count = snapshot.candidates.len();
    });
    emit_job(&job_id, app.as_ref());

    if !score_candidates_with(&mut candidates, &cancelled, dtoken_learning::index_project) {
        finish_cancelled(&job_id, app.as_ref());
        return;
    }
    candidates.retain(|candidate| candidate.write_eligible);
    candidates.sort_by(|left, right| {
        right
            .write_eligible
            .cmp(&left.write_eligible)
            .then(right.score.cmp(&left.score))
            .then(right.mtime.cmp(&left.mtime))
            .then(left.agent.cmp(&right.agent))
            .then(left.id.cmp(&right.id))
    });
    update_job(&job_id, |snapshot| {
        snapshot.candidates = candidates;
        snapshot.progress.candidate_count = snapshot.candidates.len();
        finish_learning_snapshot(snapshot, persistence_failed);
    });
    emit_job(&job_id, app.as_ref());
}

fn score_candidates_with(
    candidates: &mut [LearningCandidate],
    cancelled: &AtomicBool,
    mut indexer: impl FnMut(
        &Path,
        usize,
        usize,
    )
        -> Result<dtoken_learning::ProjectIndex, dtoken_learning::LearningError>,
) -> bool {
    for candidate in candidates {
        if cancelled.load(Ordering::Acquire) {
            return false;
        }
        let agent_count = candidate.agent.split(", ").count();
        match indexer(
            Path::new(&candidate.path),
            candidate.session_count,
            agent_count,
        ) {
            Ok(index) => {
                candidate.score = index.score;
                candidate.project_markers = index.project_markers;
                candidate.language_fingerprints = index.language_fingerprints;
                candidate.has_agent_doc = !index.agent_docs.is_empty();
                candidate.risk_flags = index.risk_flags;
                candidate.file_count_bucket = index.file_count_bucket;
                candidate.ranking_reasons = index.ranking_reasons;
                candidate.value = match candidate.score {
                    70..=u8::MAX => "high",
                    40..=69 => "medium",
                    _ => "low",
                }
                .to_string();
            }
            Err(_) => {
                candidate.write_eligible = false;
                candidate.value = "unavailable".to_string();
                candidate.risk_flags = vec!["project-index-unavailable".to_string()];
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return false;
        }
    }
    true
}

fn finish_learning_snapshot(snapshot: &mut LearningJobSnapshot, persistence_failed: bool) {
    let persistence_failed = persistence_failed || snapshot.cache_state == "unavailable";
    let partial = persistence_failed
        || snapshot
            .sources
            .iter()
            .any(|source| source.truncated || matches!(source.state.as_str(), "partial" | "error"));
    let terminal_state = if partial { "partial" } else { "complete" };
    snapshot.ok = true;
    snapshot.state = terminal_state.to_string();
    snapshot.phase = terminal_state.to_string();
    snapshot.cache_state = if persistence_failed {
        "unavailable"
    } else {
        "fresh"
    }
    .to_string();
    snapshot.saved_at_unix = unix_seconds();
    snapshot.freshness = super::now_timestamp();
    snapshot.progress.current_stage = terminal_state.to_string();
    snapshot.progress.cancellable = false;
}

fn scan_adapter(
    adapter: &AgentSessionAdapter,
    skipped: &[PathBuf],
    cancelled: impl Fn() -> bool,
) -> (
    LearningSourceState,
    Vec<LearningCandidate>,
    Vec<SessionRecordMetadata>,
) {
    let mut candidates = Vec::new();
    let mut records = Vec::new();
    let mut skipped_count = 0;
    let mut roots_found = 0;
    let mut entry_count = 0;
    let mut error_count = 0;
    let mut truncated = false;
    for root in &adapter.roots {
        if cancelled() {
            break;
        }
        if skipped.iter().any(|skip| root.starts_with(skip)) || !root.exists() {
            continue;
        }
        if !super::learning_allowed_root(root) {
            skipped_count += 1;
            continue;
        }
        match scan_root_metadata(root, MAX_SCAN_DEPTH, MAX_ROOT_ENTRIES, &cancelled) {
            Ok(summary) => {
                roots_found += 1;
                entry_count += summary.session_count;
                skipped_count += summary.skipped_count;
                truncated |= summary.truncated;
            }
            Err(_) => error_count += 1,
        }
    }
    if adapter.parser_supported {
        match adapter.id {
            "claude-code" => {
                for root in &adapter.roots {
                    match discover_claude_projects(root, &cancelled) {
                        Ok((mut projects, mut discovered, parser_skipped)) => {
                            skipped_count += parser_skipped;
                            candidates.append(&mut projects);
                            records.append(&mut discovered);
                        }
                        Err(_) if root.exists() => error_count += 1,
                        Err(_) => {}
                    }
                }
            }
            "codex" => match discover_codex_projects(&adapter.roots, &cancelled) {
                Ok((mut projects, mut discovered, parser_skipped)) => {
                    skipped_count += parser_skipped;
                    candidates.append(&mut projects);
                    records.append(&mut discovered);
                }
                Err(_) if roots_found > 0 => error_count += 1,
                Err(_) => {}
            },
            "opencode" => match discover_opencode_projects(&adapter.roots, &cancelled) {
                Ok((mut projects, mut discovered, parser_skipped, parser_truncated)) => {
                    skipped_count += parser_skipped;
                    truncated |= parser_truncated;
                    candidates.append(&mut projects);
                    records.append(&mut discovered);
                }
                Err(_) if roots_found > 0 => error_count += 1,
                Err(_) => {}
            },
            "cursor" => match discover_cursor_projects(&adapter.roots, &cancelled) {
                Ok((mut projects, mut discovered, parser_skipped)) => {
                    skipped_count += parser_skipped;
                    candidates.append(&mut projects);
                    records.append(&mut discovered);
                }
                Err(_) if roots_found > 0 => error_count += 1,
                Err(_) => {}
            },
            "gemini-cli" => match discover_gemini_projects(&adapter.roots, &cancelled) {
                Ok((mut projects, mut discovered, parser_skipped)) => {
                    skipped_count += parser_skipped;
                    candidates.append(&mut projects);
                    records.append(&mut discovered);
                }
                Err(_) if roots_found > 0 => error_count += 1,
                Err(_) => {}
            },
            "openclaw" => match discover_generic_jsonl_sessions(
                adapter.id,
                &adapter.roots,
                "openclaw-jsonl-v1",
                &cancelled,
            ) {
                Ok((mut projects, mut discovered, parser_skipped)) => {
                    skipped_count += parser_skipped;
                    candidates.append(&mut projects);
                    records.append(&mut discovered);
                }
                Err(_) if roots_found > 0 => error_count += 1,
                Err(_) => {}
            },
            _ => {}
        }
    }
    let session_count = records.len();
    let conversation_count = records
        .iter()
        .filter(|record| record.kind == "conversation")
        .count();
    let subagent_count = records
        .iter()
        .filter(|record| record.kind == "subagent")
        .count();
    let linked_count = records
        .iter()
        .filter(|record| record.parse_state == "linked")
        .count();
    let unlinked_count = records
        .iter()
        .filter(|record| record.parse_state == "unlinked")
        .count();
    let corrupt_count = records
        .iter()
        .filter(|record| record.parse_state == "corrupt")
        .count();
    let unsupported_count = if records.is_empty() && roots_found > 0 {
        entry_count
    } else {
        records
            .iter()
            .filter(|record| record.parse_state == "unsupported")
            .count()
    };
    candidates
        .retain(|candidate| !path_is_within_roots(Path::new(&candidate.path), &adapter.roots));
    candidates = aggregate_project_candidates(candidates);
    let project_count = candidates.len();
    truncated |= entry_count > MAX_ROOT_ENTRIES;
    let state = if (truncated || error_count > 0) && roots_found > 0 {
        "partial"
    } else if (!adapter.parser_supported && (roots_found > 0 || adapter.roots.is_empty()))
        || (records.is_empty() && entry_count > 0 && roots_found > 0)
    {
        "unsupported"
    } else if roots_found > 0 {
        "scanned"
    } else if error_count > 0 {
        "error"
    } else {
        "missing"
    };
    let detail = match state {
        "scanned" => "Session roots were enumerated with metadata-only limits",
        "partial" => {
            "One or more session roots reached a safety limit or unsupported format; displayed counts are partial"
        }
        "unsupported" => "The Agent was detected, but its session format is not yet safely parsed",
        "error" => "A detected root could not be read within the safety boundary",
        _ => "No supported session root was detected",
    };
    (
        LearningSourceState {
            id: adapter.id.to_string(),
            label: adapter.label.to_string(),
            state: state.to_string(),
            roots_found,
            entry_count,
            session_count,
            conversation_count,
            subagent_count,
            linked_count,
            unlinked_count,
            corrupt_count,
            unsupported_count,
            truncated,
            project_count,
            skipped_count,
            detail: detail.to_string(),
        },
        candidates,
        records,
    )
}

fn discover_claude_projects(
    root: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<(Vec<LearningCandidate>, Vec<SessionRecordMetadata>, usize), String> {
    if !root.is_dir() {
        return Ok((vec![], vec![], 0));
    }
    let mut evidence = HashMap::<PathBuf, RootSummary>::new();
    let mut records = Vec::new();
    let mut skipped_count = 0;
    for entry in fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .take(MAX_ROOT_ENTRIES)
    {
        if cancelled() {
            break;
        }
        let Ok(entry) = entry else {
            skipped_count += 1;
            continue;
        };
        let encoded_root = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&encoded_root) else {
            skipped_count += 1;
            continue;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            skipped_count += 1;
            continue;
        }
        let Some(encoded) = entry.file_name().to_str().map(str::to_owned) else {
            skipped_count += 1;
            continue;
        };
        let fallback_project = resolve_claude_project_mapping(&encoded);
        for file in collect_known_files(std::slice::from_ref(&encoded_root), |path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
        }) {
            if cancelled() {
                break;
            }
            let kind = if file
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("agent-"))
            {
                "subagent"
            } else {
                "conversation"
            };
            let mut project = None;
            let mut parse_state = "unlinked";
            match fs::File::open(&file) {
                Ok(handle) => {
                    let mut reader = BufReader::new(handle);
                    let mut line = Vec::new();
                    let mut remaining = MAX_SESSION_METADATA_BYTES_PER_FILE;
                    for _ in 0..10 {
                        let Some(within_limit) = read_bounded_line(
                            &mut reader,
                            &mut line,
                            &mut remaining,
                            MAX_SESSION_LINE_BYTES,
                        )
                        .map_err(|error| error.to_string())?
                        else {
                            break;
                        };
                        if !within_limit {
                            skipped_count += 1;
                            continue;
                        }
                        let Ok(metadata) = serde_json::from_slice::<ClaudeMetadataLine>(&line)
                        else {
                            continue;
                        };
                        if let Some(linked) = metadata
                            .cwd
                            .as_deref()
                            .and_then(|cwd| canonical_project_path(Path::new(cwd)))
                        {
                            project = Some(linked);
                            parse_state = "linked";
                            break;
                        }
                    }
                }
                Err(_) => {
                    parse_state = "corrupt";
                    skipped_count += 1;
                }
            }
            if project.is_none() {
                project.clone_from(&fallback_project);
                if project.is_some() && parse_state != "corrupt" {
                    parse_state = "linked";
                }
            }
            if let Some(project) = project.as_ref() {
                let summary = evidence.entry(project.clone()).or_default();
                summary.session_count += 1;
                summary.project_count = 1;
                summary.modified = summary.modified.max(
                    fs::metadata(&file)
                        .ok()
                        .and_then(|metadata| metadata.modified().ok())
                        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                        .map(|duration| duration.as_secs()),
                );
            }
            records.push(session_record_for_file(
                "claude-code",
                root,
                &file,
                kind,
                project.as_deref(),
                parse_state,
                "claude-jsonl-v1",
            ));
        }
    }
    let candidates = evidence
        .into_iter()
        .map(|(project, summary)| {
            candidate_for_project(
                "Claude Code",
                &project,
                &summary,
                "claude-jsonl-project-metadata-verified",
                "Claude JSONL cwd metadata or a unique legacy directory mapping references this project",
            )
        })
        .collect();
    Ok((candidates, records, skipped_count))
}

fn resolve_claude_project_mapping(encoded: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let _ = encoded;
        None
    }
    #[cfg(not(windows))]
    {
        if !encoded.starts_with('-') || encoded.len() < 2 {
            return None;
        }
        let mut matches = Vec::new();
        let mut visits = 0;
        resolve_claude_project_mapping_from(Path::new("/"), encoded, 0, &mut visits, &mut matches);
        matches.sort();
        matches.dedup();
        (matches.len() == 1).then(|| matches.remove(0))
    }
}

#[cfg(not(windows))]
fn resolve_claude_project_mapping_from(
    parent: &Path,
    remaining: &str,
    depth: usize,
    visits: &mut usize,
    matches: &mut Vec<PathBuf>,
) {
    if remaining.is_empty() {
        if let Some(project) = canonical_project_path(parent) {
            matches.push(project);
        }
        return;
    }
    if depth >= 64 || *visits >= MAX_CLAUDE_MAPPING_VISITS || matches.len() > 1 {
        return;
    }
    let Some(encoded_tail) = remaining.strip_prefix('-') else {
        return;
    };
    if encoded_tail.is_empty() {
        return;
    }
    let component_ends = encoded_tail
        .match_indices('-')
        .map(|(index, _)| index)
        .chain(std::iter::once(encoded_tail.len()));
    for component_end in component_ends {
        if component_end == 0 {
            continue;
        }
        if *visits >= MAX_CLAUDE_MAPPING_VISITS || matches.len() > 1 {
            break;
        }
        let name = &encoded_tail[..component_end];
        if name == "." || name == ".." || name.contains('/') {
            continue;
        }
        *visits += 1;
        let path = parent.join(name);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        resolve_claude_project_mapping_from(
            &path,
            &encoded_tail[component_end..],
            depth + 1,
            visits,
            matches,
        );
    }
}

fn discover_codex_projects(
    roots: &[PathBuf],
    cancelled: impl Fn() -> bool,
) -> Result<(Vec<LearningCandidate>, Vec<SessionRecordMetadata>, usize), String> {
    discover_codex_projects_with_budget(roots, cancelled, MAX_SESSION_METADATA_BYTES_PER_FILE)
}

fn discover_codex_projects_with_budget(
    roots: &[PathBuf],
    cancelled: impl Fn() -> bool,
    max_metadata_bytes_per_file: usize,
) -> Result<(Vec<LearningCandidate>, Vec<SessionRecordMetadata>, usize), String> {
    let mut evidence = HashMap::<PathBuf, RootSummary>::new();
    let mut records = Vec::new();
    let mut skipped_count = 0;
    let files = collect_known_files(roots, |path| {
        path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
    });
    for file in files {
        if cancelled() {
            break;
        }
        let mut file_projects = Vec::new();
        let mut file_had_supported_metadata = false;
        let mut kind = "conversation";
        let mut parse_state = "corrupt";
        if let Ok(handle) = fs::File::open(&file) {
            let mut reader = BufReader::new(handle);
            let mut line = Vec::new();
            let mut file_bytes = max_metadata_bytes_per_file;
            for _ in 0..MAX_SESSION_METADATA_LINES_PER_FILE {
                if cancelled() {
                    break;
                }
                let Some(within_line_limit) = read_bounded_line(
                    &mut reader,
                    &mut line,
                    &mut file_bytes,
                    MAX_SESSION_LINE_BYTES,
                )
                .map_err(|error| error.to_string())?
                else {
                    break;
                };
                if !within_line_limit {
                    skipped_count += 1;
                    continue;
                }
                let Ok(metadata) = serde_json::from_slice::<CodexMetadataLine>(&line) else {
                    continue;
                };
                if metadata.record_type.as_deref() != Some("session_meta") {
                    continue;
                }
                let payload = metadata.payload;
                if payload
                    .as_ref()
                    .and_then(|payload| payload.source.as_ref())
                    .is_some_and(|source| {
                        source
                            .as_object()
                            .is_some_and(|object| object.contains_key("subagent"))
                    })
                {
                    kind = "subagent";
                }
                let paths = [metadata.cwd, metadata.project, metadata.root]
                    .into_iter()
                    .chain(
                        payload
                            .into_iter()
                            .flat_map(|payload| [payload.cwd, payload.project, payload.root]),
                    );
                for path in paths.flatten() {
                    if let Some(project) = canonical_project_path(Path::new(&path)) {
                        file_projects.push(project);
                    }
                }
                file_had_supported_metadata = true;
                parse_state = if file_projects.is_empty() {
                    "unlinked"
                } else {
                    "linked"
                };
                break;
            }
        } else {
            skipped_count += 1;
        }
        if !file_had_supported_metadata {
            skipped_count += 1;
        }
        file_projects.sort();
        file_projects.dedup();
        let linked_project = file_projects.first().cloned();
        let source_root = roots
            .iter()
            .find(|root| file.starts_with(root))
            .map(PathBuf::as_path)
            .unwrap_or_else(|| roots.first().map(PathBuf::as_path).unwrap_or(&file));
        records.push(session_record_for_file(
            "codex",
            source_root,
            &file,
            kind,
            linked_project.as_deref(),
            parse_state,
            "codex-jsonl-v1",
        ));
        let modified = fs::metadata(&file)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        for project in file_projects {
            let summary = evidence.entry(project).or_default();
            summary.session_count += 1;
            summary.project_count = 1;
            summary.modified = summary.modified.max(modified);
        }
    }
    let candidates = evidence
        .into_iter()
        .map(|(project, summary)| {
            candidate_for_project(
                "Codex",
                &project,
                &summary,
                "codex-jsonl-project-metadata-verified",
                "Codex JSONL cwd/project/root metadata references this directory",
            )
        })
        .collect();
    Ok((candidates, records, skipped_count))
}

fn discover_cursor_projects(
    roots: &[PathBuf],
    cancelled: impl Fn() -> bool,
) -> Result<(Vec<LearningCandidate>, Vec<SessionRecordMetadata>, usize), String> {
    let mut evidence = HashMap::<PathBuf, RootSummary>::new();
    let mut records = Vec::new();
    let mut skipped_count = 0;
    for workspace in collect_known_files(roots, |path| {
        path.file_name().and_then(|name| name.to_str()) == Some("workspace.json")
    }) {
        if cancelled() {
            break;
        }
        let Ok(metadata) = fs::metadata(&workspace) else {
            skipped_count += 1;
            continue;
        };
        if metadata.len() > MAX_SESSION_LINE_BYTES as u64 {
            skipped_count += 1;
            continue;
        }
        let Ok(raw) = fs::read(&workspace) else {
            skipped_count += 1;
            continue;
        };
        let Ok(mapping) = serde_json::from_slice::<CursorWorkspaceMetadata>(&raw) else {
            skipped_count += 1;
            continue;
        };
        let Ok(uri) = Url::parse(&mapping.folder) else {
            skipped_count += 1;
            continue;
        };
        if uri.scheme() != "file" || uri.host_str().is_some_and(|host| host != "localhost") {
            skipped_count += 1;
            continue;
        }
        let Ok(path) = uri.to_file_path() else {
            skipped_count += 1;
            continue;
        };
        let Some(project) = canonical_project_path(&path) else {
            skipped_count += 1;
            continue;
        };
        let summary = evidence.entry(project).or_default();
        summary.project_count = 1;
        summary.modified = summary.modified.max(
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs()),
        );
    }
    for root in roots.iter().filter(|root| {
        root.file_name().and_then(|name| name.to_str()) == Some("projects")
            && root
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some(".cursor")
    }) {
        let Ok(entries) = fs::read_dir(root) else {
            skipped_count += 1;
            continue;
        };
        for entry in entries.take(MAX_ROOT_ENTRIES).flatten() {
            if cancelled() {
                break;
            }
            let encoded_root = entry.path();
            if !encoded_root.is_dir() {
                continue;
            }
            let encoded = entry.file_name().to_string_lossy().to_string();
            let project = (encoded != "empty-window")
                .then(|| resolve_claude_project_mapping(&format!("-{encoded}")))
                .flatten();
            for transcript in collect_known_files(std::slice::from_ref(&encoded_root), |path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
                    && path.components().any(|component| {
                        component.as_os_str().to_str() == Some("agent-transcripts")
                    })
            }) {
                let parse_state = if project.is_some() {
                    "linked"
                } else {
                    "unlinked"
                };
                if let Some(project) = project.as_ref() {
                    let summary = evidence.entry(project.clone()).or_default();
                    summary.session_count += 1;
                    summary.project_count = 1;
                    summary.modified = summary.modified.max(
                        fs::metadata(&transcript)
                            .ok()
                            .and_then(|metadata| metadata.modified().ok())
                            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                            .map(|duration| duration.as_secs()),
                    );
                }
                records.push(session_record_for_file(
                    "cursor",
                    root,
                    &transcript,
                    "conversation",
                    project.as_deref(),
                    parse_state,
                    "cursor-agent-transcript-v1",
                ));
            }
        }
    }
    let candidates = evidence
        .into_iter()
        .map(|(project, summary)| {
            candidate_for_project(
                "Cursor",
                &project,
                &summary,
                "cursor-workspace-folder-uri-verified",
                "Cursor workspace.json folder URI references this directory",
            )
        })
        .collect();
    Ok((candidates, records, skipped_count))
}

fn collect_known_files(roots: &[PathBuf], matches: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut queue = roots
        .iter()
        .cloned()
        .map(|root| (root, 0_usize))
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some((path, depth)) = queue.pop_front() {
        if visited >= MAX_ROOT_ENTRIES {
            break;
        }
        visited += 1;
        if is_sensitive_learning_path(&path) {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            if matches(&path) {
                files.push(path);
            }
            continue;
        }
        if !metadata.is_dir() || depth >= MAX_SCAN_DEPTH {
            continue;
        }
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            queue.push_back((entry.path(), depth + 1));
        }
    }
    files
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    remaining_total: &mut usize,
    max_line_bytes: usize,
) -> std::io::Result<Option<bool>> {
    line.clear();
    let mut saw_bytes = false;
    let mut overflow = false;
    loop {
        if *remaining_total == 0 {
            return Ok(saw_bytes.then_some(false));
        }
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(saw_bytes.then_some(!overflow));
        }
        let available = buffer.len().min(*remaining_total);
        let newline = buffer[..available].iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available, |index| index + 1);
        let content_len = newline.map_or(consumed, |index| index);
        if line.len().saturating_add(content_len) <= max_line_bytes {
            line.extend_from_slice(&buffer[..content_len]);
        } else {
            overflow = true;
        }
        reader.consume(consumed);
        *remaining_total -= consumed;
        saw_bytes = true;
        if newline.is_some() {
            return Ok(Some(!overflow));
        }
    }
}

fn discover_opencode_projects(
    roots: &[PathBuf],
    cancelled: impl Fn() -> bool,
) -> Result<
    (
        Vec<LearningCandidate>,
        Vec<SessionRecordMetadata>,
        usize,
        bool,
    ),
    String,
> {
    let mut observations = HashMap::<String, OpenCodeSessionObservation>::new();
    let mut corrupt_records = Vec::new();
    let mut skipped_count = 0;
    let mut truncated = false;

    for root in roots.iter().filter(|path| path.is_dir()) {
        let session_root = if root.file_name().and_then(|name| name.to_str()) == Some("storage") {
            root.join("session")
        } else {
            root.clone()
        };
        if session_root.file_name().and_then(|name| name.to_str()) != Some("session")
            || session_root
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                != Some("storage")
            || !session_root.is_dir()
        {
            continue;
        }
        let Some(base) = session_root.parent().and_then(Path::parent) else {
            continue;
        };
        for file in collect_known_files(std::slice::from_ref(&session_root), |path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("json")
        }) {
            if cancelled() {
                break;
            }
            let metadata = match fs::metadata(&file) {
                Ok(metadata) if metadata.len() <= MAX_SESSION_LINE_BYTES as u64 => metadata,
                _ => {
                    skipped_count += 1;
                    corrupt_records.push(session_record_for_file(
                        "opencode",
                        &session_root,
                        &file,
                        "conversation",
                        None,
                        "corrupt",
                        "opencode-legacy-json-v1",
                    ));
                    continue;
                }
            };
            let parsed = fs::read(&file)
                .ok()
                .and_then(|raw| serde_json::from_slice::<OpenCodeLegacySessionMetadata>(&raw).ok());
            let Some(parsed) = parsed else {
                skipped_count += 1;
                corrupt_records.push(session_record_for_file(
                    "opencode",
                    &session_root,
                    &file,
                    "conversation",
                    None,
                    "corrupt",
                    "opencode-legacy-json-v1",
                ));
                continue;
            };
            let Some(session_id) = parsed.id.filter(|id| !id.is_empty() && id.len() <= 256) else {
                skipped_count += 1;
                corrupt_records.push(session_record_for_file(
                    "opencode",
                    &session_root,
                    &file,
                    "conversation",
                    None,
                    "corrupt",
                    "opencode-legacy-json-v1",
                ));
                continue;
            };
            let identity = format!("{}:{session_id}", base.display());
            let project = parsed
                .directory
                .as_deref()
                .filter(|path| path.len() <= 4096)
                .and_then(|path| canonical_project_path(Path::new(path)));
            let created_at_unix = parsed
                .time
                .as_ref()
                .and_then(|time| normalize_opencode_timestamp(time.created));
            let updated_at_unix = parsed
                .time
                .as_ref()
                .and_then(|time| normalize_opencode_timestamp(time.updated))
                .or(created_at_unix);
            observations.insert(
                identity.clone(),
                OpenCodeSessionObservation {
                    identity,
                    source_root: base.to_path_buf(),
                    project,
                    created_at_unix,
                    updated_at_unix,
                    format_version: "opencode-legacy-json-v1",
                    size_bytes: metadata.len(),
                    mtime_ns: metadata_mtime_ns(&metadata),
                },
            );
        }
    }

    for database in roots.iter().filter(|path| {
        path.is_file() && path.file_name().and_then(|name| name.to_str()) == Some("opencode.db")
    }) {
        if cancelled() {
            break;
        }
        let connection = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| error.to_string())?;
        let project_columns = sqlite_columns(&connection, "project")?;
        if !["id", "worktree", "time_updated"]
            .iter()
            .all(|required| project_columns.iter().any(|column| column == required))
        {
            return Err("unsupported_project_schema".to_string());
        }
        let session_columns = sqlite_columns(&connection, "session")?;
        if !["id", "project_id"]
            .iter()
            .all(|required| session_columns.iter().any(|column| column == required))
        {
            return Err("unsupported_session_schema".to_string());
        }
        let session_expr = |column: &str| {
            if session_columns.iter().any(|candidate| candidate == column) {
                format!("s.{column}")
            } else {
                "NULL".to_string()
            }
        };
        let query = format!(
            "SELECT s.id, {}, {}, {}, p.worktree, p.time_updated
             FROM session s
             LEFT JOIN project p ON p.id = s.project_id
             ORDER BY s.id
             LIMIT {}",
            session_expr("directory"),
            session_expr("time_created"),
            session_expr("time_updated"),
            MAX_ROOT_ENTRIES + 1
        );
        let mut statement = connection
            .prepare(&query)
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        let base = database.parent().unwrap_or(database);
        let database_mtime_ns = fs::metadata(database)
            .ok()
            .as_ref()
            .map(metadata_mtime_ns)
            .unwrap_or_default();
        for row in rows {
            if cancelled() {
                break;
            }
            let Ok((session_id, directory, created, updated, worktree, project_updated)) = row
            else {
                skipped_count += 1;
                continue;
            };
            if session_id.is_empty() || session_id.len() > 256 {
                skipped_count += 1;
                continue;
            }
            let identity = format!("{}:{session_id}", base.display());
            let project = directory
                .as_deref()
                .filter(|path| path.len() <= 4096)
                .and_then(|path| canonical_project_path(Path::new(path)))
                .or_else(|| {
                    worktree
                        .as_deref()
                        .filter(|path| path.len() <= 4096)
                        .and_then(|path| canonical_project_path(Path::new(path)))
                });
            let created_at_unix = normalize_opencode_timestamp(created);
            let updated_at_unix = normalize_opencode_timestamp(updated)
                .or_else(|| normalize_opencode_timestamp(project_updated));
            observations.insert(
                identity.clone(),
                OpenCodeSessionObservation {
                    identity,
                    source_root: base.to_path_buf(),
                    project,
                    created_at_unix,
                    updated_at_unix,
                    format_version: "opencode-sqlite-v2",
                    size_bytes: 0,
                    mtime_ns: database_mtime_ns,
                },
            );
        }
    }

    let mut observations = observations.into_values().collect::<Vec<_>>();
    observations.sort_by(|left, right| left.identity.cmp(&right.identity));
    if observations.len() > MAX_ROOT_ENTRIES {
        skipped_count += observations.len() - MAX_ROOT_ENTRIES;
        observations.truncate(MAX_ROOT_ENTRIES);
        truncated = true;
    }
    let mut records = corrupt_records;
    let mut evidence = HashMap::<PathBuf, RootSummary>::new();
    for observation in observations {
        if let Some(project) = observation.project.as_ref() {
            let summary = evidence.entry(project.clone()).or_default();
            summary.session_count += 1;
            summary.project_count = 1;
            summary.modified = summary.modified.max(observation.updated_at_unix);
        }
        records.push(SessionRecordMetadata {
            id: stable_session_record_id("opencode", observation.identity.as_bytes()),
            agent_id: "opencode".to_string(),
            source_root_id: stable_candidate_id("opencode", &observation.source_root),
            kind: "conversation".to_string(),
            project_path: observation
                .project
                .as_ref()
                .map(|project| project.to_string_lossy().to_string()),
            created_at_unix: observation.created_at_unix,
            updated_at_unix: observation.updated_at_unix,
            format_version: observation.format_version.to_string(),
            parse_state: if observation.project.is_some() {
                "linked"
            } else {
                "unlinked"
            }
            .to_string(),
            size_bytes: observation.size_bytes,
            mtime_ns: observation.mtime_ns,
        });
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let mut candidates = evidence
        .into_iter()
        .map(|(project, summary)| {
            let mut candidate = candidate_for_project(
                "OpenCode",
                &project,
                &summary,
                "opencode-project-metadata-verified",
                "OpenCode session directory metadata or a safe legacy fallback references this directory",
            );
            candidate.reasons.push(
                "message, part, title, summary, and auth tables were not queried".to_string(),
            );
            candidate
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((candidates, records, skipped_count, truncated))
}

fn sqlite_columns(connection: &Connection, table: &str) -> Result<Vec<String>, String> {
    let pragma = match table {
        "project" => "PRAGMA table_info(project)",
        "session" => "PRAGMA table_info(session)",
        _ => return Err("unsupported_table".to_string()),
    };
    connection
        .prepare(pragma)
        .map_err(|error| error.to_string())?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn normalize_opencode_timestamp(value: Option<i64>) -> Option<u64> {
    value
        .and_then(|value| u64::try_from(value).ok())
        .map(|value| {
            if value > 10_000_000_000 {
                value / 1_000
            } else {
                value
            }
        })
}

fn metadata_mtime_ns(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

fn discover_gemini_projects(
    roots: &[PathBuf],
    cancelled: impl Fn() -> bool,
) -> Result<(Vec<LearningCandidate>, Vec<SessionRecordMetadata>, usize), String> {
    let mut evidence = HashMap::<PathBuf, RootSummary>::new();
    let mut records = Vec::new();
    let mut skipped_count = 0;
    for root in roots.iter().filter(|root| root.is_dir()) {
        let Ok(entries) = fs::read_dir(root) else {
            skipped_count += 1;
            continue;
        };
        for entry in entries.take(MAX_ROOT_ENTRIES).flatten() {
            if cancelled() {
                break;
            }
            let source = entry.path();
            if !source.is_dir() {
                continue;
            }
            let project = fs::read_to_string(source.join(".project_root"))
                .ok()
                .filter(|value| value.len() <= 4096)
                .and_then(|value| canonical_project_path(Path::new(value.trim())));
            for chat in collect_known_files(std::slice::from_ref(&source), |path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
                    && path
                        .components()
                        .any(|component| component.as_os_str().to_str() == Some("chats"))
            }) {
                if let Some(project) = project.as_ref() {
                    let summary = evidence.entry(project.clone()).or_default();
                    summary.session_count += 1;
                    summary.project_count = 1;
                    summary.modified = summary.modified.max(
                        fs::metadata(&chat)
                            .ok()
                            .and_then(|metadata| metadata.modified().ok())
                            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                            .map(|duration| duration.as_secs()),
                    );
                }
                records.push(session_record_for_file(
                    "gemini-cli",
                    root,
                    &chat,
                    "conversation",
                    project.as_deref(),
                    if project.is_some() {
                        "linked"
                    } else {
                        "unlinked"
                    },
                    "gemini-chat-json-v1",
                ));
            }
        }
    }
    let candidates = evidence
        .into_iter()
        .map(|(project, summary)| {
            candidate_for_project(
                "Gemini CLI",
                &project,
                &summary,
                "gemini-project-root-metadata-verified",
                "Gemini .project_root metadata references this directory",
            )
        })
        .collect();
    Ok((candidates, records, skipped_count))
}

fn discover_generic_jsonl_sessions(
    agent_id: &str,
    roots: &[PathBuf],
    format_version: &str,
    cancelled: impl Fn() -> bool,
) -> Result<(Vec<LearningCandidate>, Vec<SessionRecordMetadata>, usize), String> {
    let mut evidence = HashMap::<PathBuf, RootSummary>::new();
    let mut records = Vec::new();
    let mut skipped_count = 0;
    for file in collect_known_files(roots, |path| {
        path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
    }) {
        if cancelled() {
            break;
        }
        let source_root = roots
            .iter()
            .find(|root| file.starts_with(root))
            .map(PathBuf::as_path)
            .unwrap_or_else(|| roots.first().map(PathBuf::as_path).unwrap_or(&file));
        let mut project = None;
        let mut saw_json = false;
        if let Ok(handle) = fs::File::open(&file) {
            let mut reader = BufReader::new(handle);
            let mut line = Vec::new();
            let mut remaining = MAX_SESSION_METADATA_BYTES_PER_FILE;
            for _ in 0..MAX_SESSION_METADATA_LINES_PER_FILE {
                let Some(within_limit) = read_bounded_line(
                    &mut reader,
                    &mut line,
                    &mut remaining,
                    MAX_SESSION_LINE_BYTES,
                )
                .map_err(|error| error.to_string())?
                else {
                    break;
                };
                if !within_limit {
                    skipped_count += 1;
                    continue;
                }
                let Ok(metadata) = serde_json::from_slice::<GenericMetadataLine>(&line) else {
                    continue;
                };
                saw_json = true;
                let paths = [
                    metadata.cwd,
                    metadata.directory,
                    metadata.project,
                    metadata.root,
                ]
                .into_iter()
                .chain(
                    metadata
                        .payload
                        .into_iter()
                        .flat_map(|payload| [payload.cwd, payload.project, payload.root, None]),
                );
                project = paths
                    .flatten()
                    .find_map(|path| canonical_project_path(Path::new(&path)));
                if project.is_some() {
                    break;
                }
            }
        }
        let parse_state = if project.is_some() {
            "linked"
        } else if saw_json {
            "unlinked"
        } else {
            skipped_count += 1;
            "corrupt"
        };
        if let Some(project) = project.as_ref() {
            let summary = evidence.entry(project.clone()).or_default();
            summary.session_count += 1;
            summary.project_count = 1;
            summary.modified = summary.modified.max(
                fs::metadata(&file)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs()),
            );
        }
        records.push(session_record_for_file(
            agent_id,
            source_root,
            &file,
            "conversation",
            project.as_deref(),
            parse_state,
            format_version,
        ));
    }
    let label = if agent_id == "openclaw" {
        "OpenClaw"
    } else {
        agent_id
    };
    let candidates = evidence
        .into_iter()
        .map(|(project, summary)| {
            candidate_for_project(
                label,
                &project,
                &summary,
                "jsonl-project-metadata-verified",
                "Allowlisted JSONL cwd/project metadata references this directory",
            )
        })
        .collect();
    Ok((candidates, records, skipped_count))
}

fn scan_root_metadata(
    root: &Path,
    max_depth: usize,
    max_entries: usize,
    cancelled: impl Fn() -> bool,
) -> Result<RootSummary, String> {
    let metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("symlink_root".to_string());
    }
    let mut result = RootSummary::default();
    let mut queue = VecDeque::from([(root.to_path_buf(), 0_usize)]);
    let mut visited = 0;
    while let Some((path, depth)) = queue.pop_front() {
        if cancelled() || visited >= max_entries {
            result.skipped_count += queue.len();
            result.truncated = visited >= max_entries;
            break;
        }
        visited += 1;
        if path != root && is_sensitive_learning_path(&path) {
            result.skipped_count += 1;
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_symlink() => metadata,
            _ => {
                result.skipped_count += 1;
                continue;
            }
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        result.modified = result.modified.max(modified);
        if metadata.is_file() {
            result.session_count += 1;
            continue;
        }
        if !metadata.is_dir() || depth >= max_depth {
            continue;
        }
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(_) => {
                result.skipped_count += 1;
                continue;
            }
        };
        for entry in entries.flatten() {
            queue.push_back((entry.path(), depth + 1));
        }
    }
    Ok(result)
}

fn candidate_for_project(
    agent_label: &str,
    path: &Path,
    summary: &RootSummary,
    confidence: &str,
    reason: &str,
) -> LearningCandidate {
    LearningCandidate {
        id: stable_candidate_id("project", path),
        path: path.to_string_lossy().to_string(),
        agent: agent_label.to_string(),
        kind: "project".to_string(),
        mtime: summary.modified,
        activity: if summary.modified.is_some() {
            "observed"
        } else {
            "unknown"
        }
        .to_string(),
        confidence: confidence.to_string(),
        value: "unscored".to_string(),
        reasons: vec![
            reason.to_string(),
            "only whitelisted metadata was indexed".to_string(),
        ],
        freshness: super::now_timestamp(),
        session_count: summary.session_count,
        project_count: summary.project_count.max(1),
        skipped_count: summary.skipped_count,
        write_eligible: true,
        score: 0,
        project_markers: Vec::new(),
        language_fingerprints: Vec::new(),
        has_agent_doc: false,
        risk_flags: Vec::new(),
        file_count_bucket: String::new(),
        ranking_reasons: Vec::new(),
    }
}

fn canonical_project_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute()
        || !path.is_dir()
        || !super::learning_allowed_root(path)
        || is_sensitive_learning_path(path)
    {
        return None;
    }
    let canonical = fs::canonicalize(path).ok()?;
    (canonical.is_dir()
        && super::learning_allowed_root(&canonical)
        && !is_sensitive_learning_path(&canonical))
    .then_some(canonical)
}

fn project_identity(path: &Path) -> String {
    git_common_dir(path)
        .map(|git_dir| format!("git:{}", git_dir.to_string_lossy()))
        .unwrap_or_else(|| format!("path:{}", path.to_string_lossy()))
}

fn git_common_dir(project: &Path) -> Option<PathBuf> {
    let dot_git = project.join(".git");
    let metadata = fs::symlink_metadata(&dot_git).ok()?;
    let git_dir = if metadata.is_dir() {
        fs::canonicalize(&dot_git).ok()?
    } else if metadata.is_file() && metadata.len() <= 4 * 1024 {
        let marker = fs::read_to_string(&dot_git).ok()?;
        let git_dir = marker
            .lines()
            .next()?
            .trim()
            .strip_prefix("gitdir:")?
            .trim();
        let git_dir = PathBuf::from(git_dir);
        let resolved = if git_dir.is_absolute() {
            git_dir
        } else {
            project.join(git_dir)
        };
        match fs::canonicalize(&resolved) {
            Ok(canonical) => canonical,
            Err(_) => return declared_stale_worktree_common_dir(&resolved),
        }
    } else {
        return None;
    };

    let common = git_dir
        .parent()
        .filter(|parent| parent.file_name().and_then(|name| name.to_str()) == Some("worktrees"))
        .and_then(Path::parent)
        .unwrap_or(&git_dir);
    (common.is_dir()).then(|| common.to_path_buf())
}

fn declared_stale_worktree_common_dir(git_dir: &Path) -> Option<PathBuf> {
    if !git_dir.is_absolute()
        || git_dir.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return None;
    }
    let worktrees = git_dir.parent()?;
    if worktrees.file_name().and_then(|name| name.to_str()) != Some("worktrees") {
        return None;
    }
    let common = worktrees.parent()?;
    (common.file_name().and_then(|name| name.to_str()) == Some(".git"))
        .then(|| common.to_path_buf())
}

fn path_is_within_roots(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        path == root || (root.is_dir() && path.starts_with(root))
    })
}

fn aggregate_project_candidates(candidates: Vec<LearningCandidate>) -> Vec<LearningCandidate> {
    let mut by_identity = HashMap::<String, LearningCandidate>::new();
    for mut candidate in candidates {
        if candidate.kind != "project" || !candidate.write_eligible {
            continue;
        }
        let Some(path) = canonical_project_path(Path::new(&candidate.path)) else {
            continue;
        };
        candidate.path = path.to_string_lossy().to_string();
        candidate.id = stable_candidate_id("project", &path);
        let identity = project_identity(&path);
        if let Some(existing) = by_identity.get_mut(&identity) {
            let candidate_is_preferred = candidate.mtime.unwrap_or(0) > existing.mtime.unwrap_or(0)
                || (candidate.mtime == existing.mtime && candidate.path < existing.path);
            if candidate_is_preferred {
                existing.path.clone_from(&candidate.path);
                existing.id.clone_from(&candidate.id);
                existing.activity.clone_from(&candidate.activity);
            }
            let mut agents = existing
                .agent
                .split(", ")
                .chain(candidate.agent.split(", "))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            agents.sort();
            agents.dedup();
            existing.agent = agents.join(", ");
            existing.session_count = existing
                .session_count
                .saturating_add(candidate.session_count);
            existing.project_count = existing
                .project_count
                .saturating_add(candidate.project_count);
            existing.skipped_count = existing
                .skipped_count
                .saturating_add(candidate.skipped_count);
            existing.mtime = existing.mtime.max(candidate.mtime);
            existing.reasons.extend(candidate.reasons);
            existing.reasons.sort();
            existing.reasons.dedup();
            existing.confidence = "multi-source-project-metadata-verified".to_string();
        } else {
            by_identity.insert(identity, candidate);
        }
    }
    by_identity.into_values().collect()
}

fn session_adapters() -> Vec<AgentSessionAdapter> {
    scan_current_process_agents()
        .map(|snapshot| session_adapters_from_detected(&snapshot.agents))
        .unwrap_or_default()
}

fn session_adapters_from_detected(
    detected_agents: &[dtoken_config_patcher::agent::DetectedAgent],
) -> Vec<AgentSessionAdapter> {
    detected_agents
        .iter()
        .filter_map(|detected| {
            let descriptor = agent_descriptor(&detected.id)?;
            let roots = detected
                .evidence
                .iter()
                .filter(|evidence| evidence.kind == AgentEvidenceKind::SessionRoot)
                .map(|evidence| evidence.path.clone())
                .collect::<Vec<_>>();
            Some(AgentSessionAdapter {
                id: descriptor.id,
                label: descriptor.name,
                roots,
                parser_supported: descriptor.session_capability == AgentSessionCapability::Metadata,
            })
        })
        .collect()
}

fn is_sensitive_learning_path(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    [
        "/.env",
        "auth.json",
        "/credentials",
        "/secrets",
        "/keychains",
        "/.ssh",
        "/.gnupg",
        "/cookies",
        "/tokens",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn empty_snapshot(job_id: &str) -> LearningJobSnapshot {
    LearningJobSnapshot {
        ok: true,
        job_id: job_id.to_string(),
        state: "idle".to_string(),
        phase: "idle".to_string(),
        source: "d-token-learning-discovery".to_string(),
        freshness: super::now_timestamp(),
        cache_state: "missing".to_string(),
        saved_at_unix: 0,
        progress: LearningProgress {
            scanned_agents: 0,
            total_agents: 6,
            session_records: 0,
            parsed_projects: 0,
            candidate_count: 0,
            skipped: 0,
            risks: 0,
            current_stage: "idle".to_string(),
            cancellable: false,
        },
        sources: Vec::new(),
        candidates: Vec::new(),
        writes_performed: false,
        privacy: "Metadata only; prompt, response, source code, command output, credentials, tokens, and auth files are not returned or cached".to_string(),
    }
}

fn update_job(job_id: &str, update: impl FnOnce(&mut LearningJobSnapshot)) {
    let jobs = LEARNING_JOBS.get_or_init(|| Mutex::new(HashMap::new()));
    let snapshot = {
        let mut guard = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(job) = guard.get_mut(job_id) else {
            return;
        };
        update(&mut job.snapshot);
        job.snapshot.clone()
    };
    if persist_job_snapshot(&snapshot).is_err() {
        if let Some(job) = jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(job_id)
        {
            job.snapshot.cache_state = "unavailable".to_string();
            if job.snapshot.state == "complete" {
                job.snapshot.state = "partial".to_string();
                job.snapshot.phase = "partial".to_string();
                job.snapshot.progress.current_stage = "partial".to_string();
            }
        }
    }
}

fn snapshot_for(job_id: &str) -> Option<LearningJobSnapshot> {
    LEARNING_JOBS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(job_id)
        .map(|job| job.snapshot.clone())
}

fn finish_cancelled(job_id: &str, app: Option<&tauri::AppHandle>) {
    update_job(job_id, |snapshot| {
        snapshot.ok = true;
        snapshot.state = "cancelled".to_string();
        snapshot.phase = "cancelled".to_string();
        snapshot.progress.current_stage = "cancelled".to_string();
        snapshot.progress.cancellable = false;
        snapshot.freshness = super::now_timestamp();
    });
    emit_job(job_id, app);
}

fn emit_job(job_id: &str, app: Option<&tauri::AppHandle>) {
    let Some(app) = app else { return };
    if let Some(snapshot) = snapshot_for(job_id) {
        let _ = app.emit("dtoken:learning-discovery", snapshot);
    }
}

fn learning_cache_path() -> PathBuf {
    super::app_state_path(&["learning", "discovery-cache.sqlite3"])
}

fn open_learning_cache() -> Result<LearningCache, String> {
    LearningCache::open(learning_cache_path()).map_err(|error| error.to_string())
}

fn cached_snapshot(job: CachedScanJob) -> Option<LearningJobSnapshot> {
    let mut snapshot = serde_json::from_value::<LearningJobSnapshot>(job.snapshot).ok()?;
    snapshot.state.clone_from(&job.state);
    snapshot.phase.clone_from(&job.phase);
    snapshot.saved_at_unix = job.saved_at_unix;
    if matches!(
        job.state.as_str(),
        "complete" | "partial" | "cancelled" | "error" | "failed"
    ) {
        snapshot.progress.current_stage.clone_from(&job.state);
        snapshot.progress.cancellable = false;
    }
    Some(snapshot)
}

fn load_latest_snapshot(cache: &LearningCache) -> Option<LearningJobSnapshot> {
    cache
        .latest_recoverable_job()
        .ok()
        .flatten()
        .and_then(cached_snapshot)
}

fn persist_job_snapshot(snapshot: &LearningJobSnapshot) -> Result<(), String> {
    let persisted = snapshot_for_persistence(snapshot);
    let snapshot_value = serde_json::to_value(&persisted).map_err(|error| error.to_string())?;
    open_learning_cache()?
        .save_job(&CachedScanJob {
            job_id: persisted.job_id.clone(),
            state: persisted.state.clone(),
            phase: persisted.phase.clone(),
            saved_at_unix: persisted.saved_at_unix,
            updated_at_ms: unix_millis_u64(),
            snapshot: snapshot_value,
        })
        .map_err(|error| error.to_string())
}

fn snapshot_for_persistence(snapshot: &LearningJobSnapshot) -> LearningJobSnapshot {
    let mut persisted = snapshot.clone();
    persisted.candidates = aggregate_project_candidates(persisted.candidates);
    persisted.progress.candidate_count = persisted.candidates.len();
    persisted
}

fn load_adapter_checkpoint(
    cache: Option<&LearningCache>,
    adapter: &AgentSessionAdapter,
) -> Option<CachedAdapterScan> {
    let cache = cache?;
    let (root_key, fingerprint) = adapter_checkpoint_identity(adapter)?;
    let checkpoint = cache
        .root_checkpoint_if_unchanged(
            &root_key,
            adapter.id,
            SESSION_ADAPTER_CACHE_VERSION,
            fingerprint,
        )
        .ok()??;
    serde_json::from_value(checkpoint.payload).ok()
}

fn save_adapter_checkpoint(
    cache: Option<&LearningCache>,
    adapter: &AgentSessionAdapter,
    source: &LearningSourceState,
    candidates: &[LearningCandidate],
    records: &[SessionRecordMetadata],
) -> Result<(), String> {
    if adapter.roots.is_empty() {
        return Ok(());
    }
    let cache = cache.ok_or_else(|| "learning cache unavailable".to_string())?;
    let (root_key, fingerprint) = adapter_checkpoint_identity(adapter)
        .ok_or_else(|| "learning root checkpoint unavailable".to_string())?;
    let payload = serde_json::to_value(CachedAdapterScan {
        source: source.clone(),
        candidates: candidates.to_vec(),
        records: records.to_vec(),
    })
    .map_err(|error| error.to_string())?;
    cache
        .save_root_checkpoint(&RootScanCheckpoint {
            root_key,
            adapter_id: adapter.id.to_string(),
            adapter_version: SESSION_ADAPTER_CACHE_VERSION.to_string(),
            root_mtime_ns: fingerprint,
            state: "complete".to_string(),
            payload,
            updated_at_ms: unix_millis_u64(),
        })
        .map_err(|error| error.to_string())
}

fn adapter_checkpoint_identity(adapter: &AgentSessionAdapter) -> Option<(String, u64)> {
    let mut roots = adapter.roots.clone();
    roots.sort();
    let mut key_hash = 0xcbf29ce484222325_u64;
    for root in &roots {
        for byte in root.to_string_lossy().bytes() {
            key_hash = (key_hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    let mut fingerprint_roots = roots.clone();
    if adapter.id == "opencode" {
        fingerprint_roots.extend(
            roots
                .iter()
                .filter(|path| {
                    path.file_name().and_then(|name| name.to_str()) == Some("opencode.db")
                })
                .filter_map(|database| {
                    let file_name = database.file_name()?.to_str()?;
                    let wal = database.with_file_name(format!("{file_name}-wal"));
                    wal.exists().then_some(wal)
                }),
        );
        fingerprint_roots.sort();
        fingerprint_roots.dedup();
    }
    let mut hash = 0xcbf29ce484222325_u64;
    let mut queue = fingerprint_roots
        .iter()
        .cloned()
        .map(|path| (path, 0_usize))
        .collect::<VecDeque<_>>();
    let mut visited = 0_usize;
    while let Some((path, depth)) = queue.pop_front() {
        if visited >= MAX_ROOT_ENTRIES {
            break;
        }
        visited += 1;
        let metadata = fs::symlink_metadata(&path).ok()?;
        if metadata.file_type().is_symlink() || is_sensitive_learning_path(&path) {
            continue;
        }
        for byte in path.to_string_lossy().bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        for byte in metadata.len().to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
            .unwrap_or_default();
        for byte in modified_ns.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        if metadata.is_dir() && depth < MAX_SCAN_DEPTH {
            let mut children = fs::read_dir(&path)
                .ok()?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            children.sort();
            queue.extend(children.into_iter().map(|child| (child, depth + 1)));
        }
    }
    if visited == 0 {
        return None;
    }
    let root_key = format!("{}-{key_hash:016x}", adapter.id);
    let sqlite_safe_fingerprint = (hash & (i64::MAX as u64)).max(1);
    Some((root_key, sqlite_safe_fingerprint))
}

fn stable_candidate_id(agent_id: &str, path: &Path) -> String {
    let hash = format!("{agent_id}:{}", path.display())
        .bytes()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    format!("{agent_id}-{hash:016x}")
}

fn stable_session_record_id(agent_id: &str, identity: impl AsRef<[u8]>) -> String {
    let mut digest = Sha256::new();
    digest.update(agent_id.as_bytes());
    digest.update(b"\0");
    digest.update(identity.as_ref());
    format!("session-{:x}", digest.finalize())
}

fn session_record_for_file(
    agent_id: &str,
    root: &Path,
    file: &Path,
    kind: &str,
    project_path: Option<&Path>,
    parse_state: &str,
    format_version: &str,
) -> SessionRecordMetadata {
    let metadata = fs::metadata(file).ok();
    let updated_at_unix = metadata.as_ref().and_then(|metadata| {
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
    });
    let mtime_ns = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or_default();
    SessionRecordMetadata {
        id: stable_session_record_id(agent_id, file.to_string_lossy().as_bytes()),
        agent_id: agent_id.to_string(),
        source_root_id: stable_candidate_id(agent_id, root),
        kind: kind.to_string(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        created_at_unix: None,
        updated_at_unix,
        format_version: format_version.to_string(),
        parse_state: parse_state.to_string(),
        size_bytes: metadata.as_ref().map_or(0, fs::Metadata::len),
        mtime_ns,
    }
}

fn to_value(snapshot: LearningJobSnapshot) -> Value {
    serde_json::to_value(snapshot).unwrap_or_else(|_| {
        json!({
            "ok": false,
            "state": "error",
            "source": "d-token-learning-discovery",
            "candidates": [],
            "sources": [],
            "writes_performed": false
        })
    })
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn unix_millis_u64() -> u64 {
    unix_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_does_not_block_cancel_status_on_the_global_job_lock() {
        use std::{
            sync::mpsc::sync_channel,
            time::{Duration, Instant},
        };

        let fixture = env::temp_dir().join(format!("dtoken-learning-cancel-{}", unix_millis()));
        fs::create_dir_all(&fixture).expect("create scoring fixture");
        let candidate = candidate_for_project(
            "Codex",
            &fixture,
            &RootSummary {
                session_count: 1,
                project_count: 1,
                ..RootSummary::default()
            },
            "verified",
            "scoring fixture",
        );
        let job_id = format!("learning-cancel-test-{}", unix_millis());
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut snapshot = empty_snapshot(&job_id);
        snapshot.state = "running".to_string();
        LEARNING_JOBS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                job_id.clone(),
                LearningJob {
                    snapshot,
                    cancelled: Arc::clone(&cancelled),
                },
            );

        let (entered_tx, entered_rx) = sync_channel(1);
        let (release_tx, release_rx) = sync_channel(1);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::spawn(move || {
            let mut candidates = vec![candidate];
            score_candidates_with(
                &mut candidates,
                &worker_cancelled,
                |_path, _sessions, _agents| {
                    entered_tx.send(()).expect("signal scoring start");
                    release_rx.recv().expect("release scoring fixture");
                    Ok(dtoken_learning::ProjectIndex::default())
                },
            )
        });
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("scoring worker entered indexer");

        let started = Instant::now();
        let response = cancel(&job_id);
        let cancel_elapsed = started.elapsed();
        assert_eq!(response["state"], "cancelling");
        assert!(cancelled.load(Ordering::Acquire));
        assert!(
            cancel_elapsed < Duration::from_millis(250),
            "cancel waited {cancel_elapsed:?} for scoring"
        );

        release_tx.send(()).expect("release scoring worker");
        assert!(!worker.join().expect("join scoring worker"));
        LEARNING_JOBS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&job_id);
        fs::remove_dir_all(fixture).expect("remove scoring fixture");
    }

    #[test]
    fn metadata_scan_counts_files_without_reading_content() {
        let root = env::temp_dir().join(format!("dtoken-learning-scan-{}", unix_millis()));
        fs::create_dir_all(root.join("nested")).expect("create fixture root");
        fs::write(root.join("nested/session.jsonl"), "must-not-be-read").expect("write fixture");
        fs::write(root.join("auth.json"), "must-be-skipped").expect("write auth fixture");

        let summary = scan_root_metadata(&root, 4, 100, || false).expect("scan metadata");

        assert_eq!(summary.session_count, 1);
        assert!(summary.skipped_count >= 1);
        fs::remove_dir_all(root).expect("remove fixture root");
    }

    #[test]
    fn session_adapters_retain_detected_agents_without_session_evidence() {
        use dtoken_config_patcher::agent::{
            AgentDetectionEvidence, AgentInstallCapability, DetectedAgent,
        };

        let detected = vec![
            DetectedAgent {
                id: "codex".to_string(),
                name: "Codex".to_string(),
                install_capability: AgentInstallCapability::ConfirmRequired,
                session_capability: AgentSessionCapability::Metadata,
                evidence: vec![AgentDetectionEvidence {
                    kind: AgentEvidenceKind::SessionRoot,
                    path: PathBuf::from("/tmp/codex-sessions"),
                }],
            },
            DetectedAgent {
                id: "opencode".to_string(),
                name: "OpenCode".to_string(),
                install_capability: AgentInstallCapability::OneClick,
                session_capability: AgentSessionCapability::Metadata,
                evidence: vec![AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Executable,
                    path: PathBuf::from("/tmp/opencode"),
                }],
            },
            DetectedAgent {
                id: "commandcode".to_string(),
                name: "CommandCode".to_string(),
                install_capability: AgentInstallCapability::ManualOnly,
                session_capability: AgentSessionCapability::DescriptorOnly,
                evidence: vec![AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Executable,
                    path: PathBuf::from("/tmp/commandcode"),
                }],
            },
            DetectedAgent {
                id: "claude-desktop".to_string(),
                name: "Claude Desktop".to_string(),
                install_capability: AgentInstallCapability::ManualOnly,
                session_capability: AgentSessionCapability::Unsupported,
                evidence: vec![AgentDetectionEvidence {
                    kind: AgentEvidenceKind::AppBundle,
                    path: PathBuf::from("/tmp/Claude.app"),
                }],
            },
        ];
        let adapters = session_adapters_from_detected(&detected);
        let ids = adapters
            .iter()
            .map(|adapter| adapter.id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec!["codex", "opencode", "commandcode", "claude-desktop"]
        );
        assert_eq!(
            adapters[0].roots,
            vec![PathBuf::from("/tmp/codex-sessions")]
        );
        assert!(adapters[1].roots.is_empty());
        assert!(adapters[1].parser_supported);
        assert!(adapters[2].roots.is_empty());
        assert!(!adapters[2].parser_supported);
        assert!(adapters[3].roots.is_empty());
        assert!(!adapters[3].parser_supported);
    }

    #[test]
    fn detected_agents_without_session_roots_report_capability_aware_states() {
        let metadata = AgentSessionAdapter {
            id: "opencode",
            label: "OpenCode",
            roots: vec![],
            parser_supported: true,
        };
        let descriptor_only = AgentSessionAdapter {
            id: "commandcode",
            label: "CommandCode",
            roots: vec![],
            parser_supported: false,
        };
        let unsupported = AgentSessionAdapter {
            id: "claude-desktop",
            label: "Claude Desktop",
            roots: vec![],
            parser_supported: false,
        };

        let (metadata_source, metadata_candidates, metadata_records) =
            scan_adapter(&metadata, &[], || false);
        let (descriptor_source, descriptor_candidates, descriptor_records) =
            scan_adapter(&descriptor_only, &[], || false);
        let (unsupported_source, unsupported_candidates, unsupported_records) =
            scan_adapter(&unsupported, &[], || false);

        assert_eq!(metadata_source.state, "missing");
        assert_eq!(metadata_source.roots_found, 0);
        assert_eq!(descriptor_source.state, "unsupported");
        assert_eq!(descriptor_source.roots_found, 0);
        assert_eq!(unsupported_source.state, "unsupported");
        assert_eq!(unsupported_source.roots_found, 0);
        assert!(metadata_candidates.is_empty());
        assert!(metadata_records.is_empty());
        assert!(descriptor_candidates.is_empty());
        assert!(descriptor_records.is_empty());
        assert!(unsupported_candidates.is_empty());
        assert!(unsupported_records.is_empty());
    }

    #[test]
    fn empty_root_adapter_does_not_fail_checkpoint_persistence() {
        let cache_root = tempfile::tempdir().expect("cache fixture");
        let cache = LearningCache::open(cache_root.path().join("learning.sqlite3")).expect("cache");
        let adapter = AgentSessionAdapter {
            id: "opencode",
            label: "OpenCode",
            roots: vec![],
            parser_supported: true,
        };
        let (source, candidates, records) = scan_adapter(&adapter, &[], || false);

        save_adapter_checkpoint(Some(&cache), &adapter, &source, &candidates, &records)
            .expect("empty-root source should not require a checkpoint");
        assert!(load_adapter_checkpoint(Some(&cache), &adapter).is_none());
    }

    #[test]
    fn opencode_project_discovery_reads_only_project_path_metadata() {
        let root = env::temp_dir().join(format!("dtoken-opencode-projects-{}", unix_millis()));
        let project = root.join("project-a");
        fs::create_dir_all(&project).expect("create project fixture");
        let database = root.join("opencode.db");
        let connection = Connection::open(&database).expect("create OpenCode fixture database");
        connection
            .execute_batch(
                "CREATE TABLE project (
                   id TEXT PRIMARY KEY,
                   worktree TEXT NOT NULL,
                   time_updated INTEGER NOT NULL,
                   private_summary TEXT
                 );
                 CREATE TABLE session (
                   id TEXT PRIMARY KEY,
                   project_id TEXT NOT NULL,
                   private_prompt TEXT
                 );",
            )
            .expect("create project table");
        connection
            .execute(
                "INSERT INTO project (id, worktree, time_updated, private_summary) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["project-secret-id", project.display().to_string(), 1_783_929_600_000_i64, "must-not-escape"],
            )
            .expect("insert project fixture");
        connection
            .execute(
                "INSERT INTO session (id, project_id, private_prompt) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    "session-secret-id",
                    "project-secret-id",
                    "prompt-must-not-escape"
                ],
            )
            .expect("insert session fixture");
        drop(connection);

        let (candidates, records, skipped, truncated) =
            discover_opencode_projects(std::slice::from_ref(&database), || false)
                .expect("discover projects");

        assert_eq!(skipped, 0);
        assert!(!truncated);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].parse_state, "linked");
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].path,
            fs::canonicalize(&project)
                .expect("canonical project fixture")
                .display()
                .to_string()
        );
        assert_eq!(candidates[0].session_count, 1);
        assert!(candidates[0].write_eligible);
        let serialized = serde_json::to_string(&candidates).expect("serialize candidates");
        assert!(!serialized.contains("project-secret-id"));
        assert!(!serialized.contains("session-secret-id"));
        assert!(!serialized.contains("must-not-escape"));
        assert!(!serialized.contains("prompt-must-not-escape"));
        fs::remove_dir_all(root).expect("remove OpenCode fixture");
    }

    #[test]
    fn opencode_prefers_session_metadata_and_merges_legacy_without_duplicate_ids() {
        let root = env::temp_dir().join(format!("dtoken-opencode-merged-{}", unix_millis()));
        let storage_sessions = root.join("storage/session/project-key");
        let worktree = root.join("worktree");
        let session_project = root.join("session-project");
        let legacy_project = root.join("legacy-project");
        for path in [
            &storage_sessions,
            &worktree,
            &session_project,
            &legacy_project,
        ] {
            fs::create_dir_all(path).expect("create OpenCode fixture directory");
        }
        fs::write(
            storage_sessions.join("overlap.json"),
            json!({
                "id": "private-overlap-id",
                "directory": legacy_project,
                "time": {"created": 1_783_929_600_000_i64, "updated": 1_783_929_601_000_i64},
                "prompt": "must-not-escape"
            })
            .to_string(),
        )
        .expect("write overlapping legacy metadata");
        fs::write(
            storage_sessions.join("legacy-only.json"),
            json!({
                "id": "private-legacy-id",
                "directory": legacy_project,
                "time": {"created": 1_783_929_602_000_i64}
            })
            .to_string(),
        )
        .expect("write legacy-only metadata");

        let database = root.join("opencode.db");
        let connection = Connection::open(&database).expect("create OpenCode database");
        connection
            .execute_batch(
                "CREATE TABLE project (
                   id TEXT PRIMARY KEY,
                   worktree TEXT NOT NULL,
                   time_updated INTEGER NOT NULL
                 );
                 CREATE TABLE session (
                   id TEXT PRIMARY KEY,
                   project_id TEXT NOT NULL,
                   directory TEXT,
                   time_created INTEGER,
                   time_updated INTEGER
                 );",
            )
            .expect("create modern OpenCode schema");
        connection
            .execute(
                "INSERT INTO project (id, worktree, time_updated) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    "private-project-id",
                    worktree.display().to_string(),
                    1_783_929_500_000_i64
                ],
            )
            .expect("insert OpenCode project");
        for (id, updated) in [
            ("private-overlap-id", 1_783_929_700_000_i64),
            ("private-sqlite-id", 1_783_929_800_000_i64),
        ] {
            connection
                .execute(
                    "INSERT INTO session (
                       id, project_id, directory, time_created, time_updated
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        id,
                        "private-project-id",
                        session_project.display().to_string(),
                        updated - 1_000,
                        updated
                    ],
                )
                .expect("insert OpenCode session");
        }
        drop(connection);

        let (candidates, records, skipped, truncated) =
            discover_opencode_projects(&[root.join("storage/session"), database], || false)
                .expect("discover merged OpenCode metadata");

        assert_eq!(skipped, 0);
        assert!(!truncated);
        assert_eq!(records.len(), 3);
        assert_eq!(
            records
                .iter()
                .filter(|record| record.format_version == "opencode-sqlite-v2")
                .count(),
            2
        );
        assert!(records
            .iter()
            .all(|record| record.id.starts_with("session-")));
        let session_candidate = candidates
            .iter()
            .find(|candidate| {
                candidate.path
                    == fs::canonicalize(&session_project)
                        .unwrap()
                        .display()
                        .to_string()
            })
            .expect("session directory candidate");
        assert_eq!(session_candidate.session_count, 2);
        let legacy_candidate = candidates
            .iter()
            .find(|candidate| {
                candidate.path
                    == fs::canonicalize(&legacy_project)
                        .unwrap()
                        .display()
                        .to_string()
            })
            .expect("legacy directory candidate");
        assert_eq!(legacy_candidate.session_count, 1);
        assert!(candidates.iter().all(|candidate| {
            candidate.path != fs::canonicalize(&worktree).unwrap().display().to_string()
        }));
        let serialized = serde_json::to_string(&(candidates, records)).expect("serialize metadata");
        assert!(!serialized.contains("private-overlap-id"));
        assert!(!serialized.contains("private-legacy-id"));
        assert!(!serialized.contains("private-sqlite-id"));
        assert!(!serialized.contains("must-not-escape"));
        fs::remove_dir_all(root).expect("remove merged OpenCode fixture");
    }

    #[test]
    fn opencode_unknown_schema_fails_partial_instead_of_guessing() {
        let root = env::temp_dir().join(format!("dtoken-opencode-schema-{}", unix_millis()));
        fs::create_dir_all(&root).expect("create schema fixture");
        let database = root.join("opencode.db");
        let connection = Connection::open(&database).expect("create schema database");
        connection
            .execute_batch(
                "CREATE TABLE project (id TEXT PRIMARY KEY);
                 CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT);",
            )
            .expect("create unsupported schema");
        drop(connection);
        let adapter = AgentSessionAdapter {
            id: "opencode",
            label: "OpenCode",
            roots: vec![database],
            parser_supported: true,
        };

        let (source, candidates, records) = scan_adapter(&adapter, &[], || false);

        assert_eq!(source.state, "partial");
        assert!(source.detail.contains("unsupported format"));
        assert!(candidates.is_empty());
        assert!(records.is_empty());
        fs::remove_dir_all(root).expect("remove schema fixture");
    }

    #[cfg(not(windows))]
    #[test]
    fn claude_project_mapping_is_unique_existing_and_fail_closed() {
        let fixture = env::temp_dir().join(format!("dtoken-claude-projects-{}", unix_millis()));
        let claude_root = fixture.join("claude-projects");
        let project = fixture.join("real-project-with-hyphen");
        fs::create_dir_all(&claude_root).expect("create Claude root");
        fs::create_dir_all(&project).expect("create project fixture");
        let canonical = fs::canonicalize(&project).expect("canonical project fixture");
        let encoded = canonical.display().to_string().replace('/', "-");
        let encoded_root = claude_root.join(&encoded);
        fs::create_dir_all(&encoded_root).expect("create encoded Claude project root");
        fs::write(encoded_root.join("session.jsonl"), "private-session-body")
            .expect("write Claude session fixture");

        let (candidates, records, skipped_count) =
            discover_claude_projects(&claude_root, || false).expect("discover Claude projects");

        assert_eq!(records.len(), 1);
        assert_eq!(skipped_count, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, canonical.display().to_string());
        assert_eq!(candidates[0].kind, "project");
        assert!(!serde_json::to_string(&candidates)
            .expect("serialize Claude candidates")
            .contains("private-session-body"));

        let ambiguous_parent = fixture.join("ambiguous");
        let first = ambiguous_parent.join("alpha-beta");
        let second = ambiguous_parent.join("alpha/beta");
        fs::create_dir_all(&first).expect("create first ambiguous path");
        fs::create_dir_all(&second).expect("create second ambiguous path");
        let ambiguous = fs::canonicalize(&first)
            .expect("canonical ambiguous path")
            .display()
            .to_string()
            .replace('/', "-");
        assert!(resolve_claude_project_mapping(&ambiguous).is_none());
        assert!(resolve_claude_project_mapping("-definitely-not-an-existing-project").is_none());
        fs::remove_dir_all(fixture).expect("remove Claude fixture");
    }

    #[cfg(not(windows))]
    #[test]
    fn claude_project_mapping_ignores_unrelated_dense_parent_entries() {
        let fixture = env::temp_dir().join(format!("dtoken-claude-dense-{}", unix_millis()));
        fs::create_dir_all(&fixture).expect("create dense parent fixture");
        for index in 0..=MAX_ROOT_ENTRIES {
            fs::write(fixture.join(format!("unrelated-{index:05}")), [])
                .expect("create unrelated entry");
        }
        let project = fixture.join("target-project-with-hyphen");
        fs::create_dir_all(&project).expect("create target project");
        let canonical = fs::canonicalize(&project).expect("canonical target project");
        let mut matches = Vec::new();
        let mut visits = 0;

        resolve_claude_project_mapping_from(
            &fixture,
            "-target-project-with-hyphen",
            0,
            &mut visits,
            &mut matches,
        );
        matches.sort();
        matches.dedup();

        assert_eq!(matches, vec![canonical]);
        assert!(visits <= MAX_CLAUDE_MAPPING_VISITS);
        fs::remove_dir_all(fixture).expect("remove dense parent fixture");
    }

    #[test]
    fn codex_jsonl_stops_after_bounded_project_metadata() {
        let fixture = env::temp_dir().join(format!("dtoken-codex-projects-{}", unix_millis()));
        let sessions = fixture.join("sessions/2026/07/13");
        let project = fixture.join("project-a");
        fs::create_dir_all(&sessions).expect("create Codex session root");
        fs::create_dir_all(&project).expect("create Codex project fixture");
        let canonical = fs::canonicalize(&project).expect("canonical Codex project fixture");
        let valid = json!({
            "type": "session_meta",
            "payload": {
                "id": "private-session-id",
                "cwd": canonical.display().to_string(),
                "private_prompt": "prompt-must-not-escape"
            }
        });
        let mut raw = format!("{valid}\n{{damaged-json\n");
        raw.push_str(
            &json!({
                "message": { "cwd": canonical.display().to_string() },
                "private_response": "response-must-not-escape"
            })
            .to_string(),
        );
        raw.push('\n');
        raw.push_str(&"x".repeat(MAX_SESSION_LINE_BYTES + 1));
        raw.push('\n');
        fs::write(sessions.join("session.jsonl"), raw).expect("write Codex JSONL fixture");

        let (candidates, records, skipped_count) =
            discover_codex_projects(&[fixture.join("sessions")], || false)
                .expect("discover Codex projects");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].parse_state, "linked");
        assert_eq!(skipped_count, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, canonical.display().to_string());
        assert_eq!(candidates[0].session_count, 1);
        let serialized = serde_json::to_string(&candidates).expect("serialize Codex candidates");
        assert!(!serialized.contains("private-session-id"));
        assert!(!serialized.contains("prompt-must-not-escape"));
        assert!(!serialized.contains("response-must-not-escape"));
        fs::remove_dir_all(fixture).expect("remove Codex fixture");
    }

    #[test]
    fn codex_metadata_budget_is_shared_across_session_headers_not_bodies() {
        let fixture = env::temp_dir().join(format!("dtoken-codex-budget-{}", unix_millis()));
        let sessions = fixture.join("sessions/2026/07/15");
        let project = fixture.join("project-a");
        fs::create_dir_all(&sessions).expect("create Codex session root");
        fs::create_dir_all(&project).expect("create Codex project fixture");
        let canonical = fs::canonicalize(&project).expect("canonical Codex project fixture");
        let metadata = json!({
            "type": "session_meta",
            "payload": {
                "id": "private-session-id",
                "cwd": canonical.display().to_string()
            }
        });
        for index in 0..12 {
            let raw = format!("{metadata}\n{}\n", "x".repeat(4 * 1024));
            fs::write(sessions.join(format!("session-{index}.jsonl")), raw)
                .expect("write Codex session fixture");
        }

        let (candidates, records, skipped_count) =
            discover_codex_projects_with_budget(&[fixture.join("sessions")], || false, 4 * 1024)
                .expect("discover all Codex session headers");

        assert_eq!(records.len(), 12);
        assert_eq!(skipped_count, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].session_count, 12);
        fs::remove_dir_all(fixture).expect("remove Codex fixture");
    }

    #[test]
    fn codex_active_and_archived_sessions_are_enumerated_and_classified() {
        let fixture = env::temp_dir().join(format!("dtoken-codex-roots-{}", unix_millis()));
        let active = fixture.join("sessions/2026/07/15");
        let archived = fixture.join("archived_sessions");
        let project = fixture.join("project-a");
        fs::create_dir_all(&active).expect("create active root");
        fs::create_dir_all(&archived).expect("create archived root");
        fs::create_dir_all(&project).expect("create project");
        let canonical = fs::canonicalize(&project).expect("canonical project");
        fs::write(
            active.join("conversation.jsonl"),
            format!(
                "{}\nprivate-body-must-not-be-read\n",
                json!({"type":"session_meta","payload":{"cwd":canonical,"source":"cli"}})
            ),
        )
        .expect("write active session");
        fs::write(
            archived.join("subagent.jsonl"),
            format!(
                "{}\nprivate-body-must-not-be-read\n",
                json!({"type":"session_meta","payload":{"cwd":canonical,"source":{"subagent":{"thread_spawn":{"depth":1}}}}})
            ),
        )
        .expect("write archived session");

        let (candidates, records, skipped) = discover_codex_projects(
            &[fixture.join("sessions"), fixture.join("archived_sessions")],
            || false,
        )
        .expect("discover both roots");

        assert_eq!(records.len(), 2);
        assert_eq!(
            records
                .iter()
                .filter(|record| record.kind == "subagent")
                .count(),
            1
        );
        assert!(records.iter().all(|record| record.parse_state == "linked"));
        assert_eq!(skipped, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].session_count, 2);
        assert!(!serde_json::to_string(&records)
            .expect("serialize records")
            .contains("private-body-must-not-be-read"));
        fs::remove_dir_all(fixture).expect("remove fixture");
    }

    #[cfg(not(windows))]
    #[test]
    fn claude_prefers_jsonl_cwd_and_classifies_agent_sessions() {
        let fixture = env::temp_dir().join(format!("dtoken-claude-cwd-{}", unix_millis()));
        let root = fixture.join("projects");
        let encoded = root.join("legacy-name-that-cannot-map");
        let project = fixture.join("project-with-hyphen");
        fs::create_dir_all(&encoded).expect("create Claude encoded root");
        fs::create_dir_all(&project).expect("create project");
        let canonical = fs::canonicalize(&project).expect("canonical project");
        let header = json!({"cwd": canonical, "private_prompt": "must-not-escape"});
        fs::write(encoded.join("session.jsonl"), format!("{header}\nsecret\n"))
            .expect("write main session");
        fs::write(
            encoded.join("agent-worker.jsonl"),
            format!("{header}\nsecret\n"),
        )
        .expect("write subagent session");

        let (candidates, records, skipped) =
            discover_claude_projects(&root, || false).expect("discover Claude metadata");

        assert_eq!(records.len(), 2);
        assert_eq!(
            records
                .iter()
                .filter(|record| record.kind == "subagent")
                .count(),
            1
        );
        assert!(records.iter().all(|record| record.parse_state == "linked"));
        assert_eq!(skipped, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].session_count, 2);
        assert!(!serde_json::to_string(&records)
            .expect("serialize records")
            .contains("must-not-escape"));
        fs::remove_dir_all(fixture).expect("remove fixture");
    }

    #[cfg(not(windows))]
    #[test]
    fn cursor_projects_root_enumerates_only_agent_transcripts() {
        let fixture = env::temp_dir().join(format!("dtoken-cursor-current-{}", unix_millis()));
        let root = fixture.join(".cursor/projects");
        let project = fixture.join("real-project");
        fs::create_dir_all(&project).expect("create project");
        let canonical = fs::canonicalize(&project).expect("canonical project");
        let encoded = canonical
            .display()
            .to_string()
            .replace('/', "-")
            .trim_start_matches('-')
            .to_string();
        let transcript_root = root.join(encoded).join("agent-transcripts/session-1");
        fs::create_dir_all(&transcript_root).expect("create transcript root");
        fs::write(transcript_root.join("session-1.jsonl"), "private-body")
            .expect("write transcript");
        fs::write(root.join("ignored.json"), "private-body").expect("write ignored file");

        let (candidates, records, skipped) =
            discover_cursor_projects(&[root], || false).expect("discover Cursor transcripts");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].parse_state, "linked");
        assert_eq!(skipped, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, canonical.display().to_string());
        fs::remove_dir_all(fixture).expect("remove fixture");
    }

    #[test]
    fn gemini_and_openclaw_enumerate_metadata_without_session_bodies() {
        let fixture = env::temp_dir().join(format!("dtoken-more-agents-{}", unix_millis()));
        let project = fixture.join("project-a");
        let gemini = fixture.join("gemini/tmp/project-key");
        let openclaw = fixture.join("openclaw/agents/main/sessions");
        fs::create_dir_all(gemini.join("chats")).expect("create Gemini chats");
        fs::create_dir_all(&openclaw).expect("create OpenClaw sessions");
        fs::create_dir_all(&project).expect("create project");
        let canonical = fs::canonicalize(&project).expect("canonical project");
        fs::write(
            gemini.join(".project_root"),
            canonical.display().to_string(),
        )
        .expect("write Gemini project root");
        fs::write(gemini.join("chats/chat-1.json"), "private-body").expect("write Gemini chat");
        fs::write(
            openclaw.join("session-1.jsonl"),
            format!("{}\nprivate-body\n", json!({"cwd": canonical})),
        )
        .expect("write OpenClaw session");

        let (gemini_candidates, gemini_records, _) =
            discover_gemini_projects(&[fixture.join("gemini/tmp")], || false)
                .expect("discover Gemini");
        let (openclaw_candidates, openclaw_records, _) = discover_generic_jsonl_sessions(
            "openclaw",
            &[fixture.join("openclaw/agents")],
            "openclaw-jsonl-v1",
            || false,
        )
        .expect("discover OpenClaw");

        assert_eq!(gemini_records.len(), 1);
        assert_eq!(gemini_candidates.len(), 1);
        assert_eq!(openclaw_records.len(), 1);
        assert_eq!(openclaw_records[0].parse_state, "linked");
        assert_eq!(openclaw_candidates.len(), 1);
        let serialized =
            serde_json::to_string(&(gemini_records, openclaw_records)).expect("serialize metadata");
        assert!(!serialized.contains("private-body"));
        fs::remove_dir_all(fixture).expect("remove fixture");
    }

    #[test]
    fn bounded_line_reader_enforces_line_and_total_byte_limits() {
        let mut reader = BufReader::new(&b"oversized-line\nsecond\n"[..]);
        let mut line = Vec::new();
        let mut total = 8;

        let accepted = read_bounded_line(&mut reader, &mut line, &mut total, 4)
            .expect("read bounded line")
            .expect("line exists");

        assert!(!accepted);
        assert_eq!(total, 0);
        assert!(line.len() <= 4);
    }

    #[test]
    fn cursor_reads_only_workspace_json_folder_file_uri() {
        let fixture = env::temp_dir().join(format!("dtoken-cursor-projects-{}", unix_millis()));
        let workspace_root = fixture.join("workspaceStorage");
        let project = fixture.join("project-a");
        let ignored = fixture.join("ignored-project");
        fs::create_dir_all(workspace_root.join("valid")).expect("create valid workspace root");
        fs::create_dir_all(workspace_root.join("ignored")).expect("create ignored workspace root");
        fs::create_dir_all(&project).expect("create Cursor project fixture");
        fs::create_dir_all(&ignored).expect("create ignored project fixture");
        let canonical = fs::canonicalize(&project).expect("canonical Cursor project fixture");
        let uri = Url::from_file_path(&canonical).expect("create folder URI");
        fs::write(
            workspace_root.join("valid/workspace.json"),
            json!({
                "folder": uri.as_str(),
                "private_state": "cursor-secret-must-not-escape"
            })
            .to_string(),
        )
        .expect("write Cursor workspace mapping");
        fs::write(
            workspace_root.join("ignored/other.json"),
            json!({
                "folder": Url::from_file_path(&ignored).unwrap().as_str()
            })
            .to_string(),
        )
        .expect("write non-whitelisted Cursor file");
        fs::write(
            workspace_root.join("ignored/workspace.json"),
            json!({
                "workspace": Url::from_file_path(&ignored).unwrap().as_str()
            })
            .to_string(),
        )
        .expect("write unknown Cursor workspace format");

        let (candidates, records, skipped_count) =
            discover_cursor_projects(&[workspace_root], || false)
                .expect("discover Cursor projects");

        assert!(records.is_empty());
        assert_eq!(skipped_count, 1);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, canonical.display().to_string());
        let serialized = serde_json::to_string(&candidates).expect("serialize Cursor candidates");
        assert!(!serialized.contains("cursor-secret-must-not-escape"));
        assert!(!serialized.contains(&ignored.display().to_string()));
        fs::remove_dir_all(fixture).expect("remove Cursor fixture");
    }

    #[test]
    fn unsupported_commandcode_counts_entries_but_not_sessions_or_candidates() {
        let fixture = env::temp_dir().join(format!("dtoken-commandcode-{}", unix_millis()));
        fs::create_dir_all(&fixture).expect("create CommandCode root");
        fs::write(fixture.join("unknown-session.bin"), "private-body")
            .expect("write unsupported session fixture");
        let adapter = AgentSessionAdapter {
            id: "commandcode",
            label: "CommandCode",
            roots: vec![fixture.clone()],
            parser_supported: false,
        };

        let (source, candidates, records) = scan_adapter(&adapter, &[], || false);

        assert_eq!(source.state, "unsupported");
        assert_eq!(source.roots_found, 1);
        assert_eq!(source.entry_count, 1);
        assert_eq!(source.session_count, 0);
        assert_eq!(source.project_count, 0);
        assert!(source.detail.contains("not yet safely parsed"));
        assert!(candidates.is_empty());
        assert!(records.is_empty());
        fs::remove_dir_all(fixture).expect("remove CommandCode fixture");
    }

    #[test]
    fn adapter_checkpoint_reuses_unchanged_metadata_and_invalidates_on_file_change() {
        let fixture = env::temp_dir().join(format!("dtoken-learning-checkpoint-{}", unix_millis()));
        fs::create_dir_all(&fixture).expect("create checkpoint root");
        let session = fixture.join("session.jsonl");
        fs::write(&session, "one").expect("write initial session");
        let adapter = AgentSessionAdapter {
            id: "codex",
            label: "Codex",
            roots: vec![fixture.clone()],
            parser_supported: true,
        };
        let cache_root = tempfile::tempdir().expect("cache fixture");
        let cache = LearningCache::open(cache_root.path().join("learning.sqlite3")).expect("cache");
        let source = LearningSourceState {
            id: "codex".to_string(),
            label: "Codex".to_string(),
            state: "scanned".to_string(),
            roots_found: 1,
            entry_count: 1,
            session_count: 1,
            conversation_count: 1,
            subagent_count: 0,
            linked_count: 1,
            unlinked_count: 0,
            corrupt_count: 0,
            unsupported_count: 0,
            truncated: false,
            project_count: 0,
            skipped_count: 0,
            detail: "metadata".to_string(),
        };
        save_adapter_checkpoint(Some(&cache), &adapter, &source, &[], &[])
            .expect("save checkpoint");
        assert!(load_adapter_checkpoint(Some(&cache), &adapter).is_some());

        fs::write(&session, "one-two").expect("change session metadata");
        assert!(load_adapter_checkpoint(Some(&cache), &adapter).is_none());
        fs::remove_dir_all(fixture).expect("remove checkpoint root");
    }

    #[test]
    fn opencode_checkpoint_invalidates_when_sqlite_wal_changes() {
        let fixture = env::temp_dir().join(format!("dtoken-opencode-wal-{}", unix_millis()));
        fs::create_dir_all(&fixture).expect("create WAL fixture");
        let database = fixture.join("opencode.db");
        let wal = fixture.join("opencode.db-wal");
        fs::write(&database, "database").expect("write database fixture");
        fs::write(&wal, "wal-one").expect("write WAL fixture");
        let adapter = AgentSessionAdapter {
            id: "opencode",
            label: "OpenCode",
            roots: vec![database],
            parser_supported: true,
        };
        let cache_root = tempfile::tempdir().expect("cache fixture");
        let cache = LearningCache::open(cache_root.path().join("learning.sqlite3")).expect("cache");
        let source = LearningSourceState {
            id: "opencode".to_string(),
            label: "OpenCode".to_string(),
            state: "scanned".to_string(),
            roots_found: 1,
            entry_count: 1,
            session_count: 1,
            conversation_count: 1,
            subagent_count: 0,
            linked_count: 1,
            unlinked_count: 0,
            corrupt_count: 0,
            unsupported_count: 0,
            truncated: false,
            project_count: 0,
            skipped_count: 0,
            detail: "metadata".to_string(),
        };
        save_adapter_checkpoint(Some(&cache), &adapter, &source, &[], &[])
            .expect("save checkpoint");
        assert!(load_adapter_checkpoint(Some(&cache), &adapter).is_some());

        fs::write(&wal, "wal-one-two").expect("change WAL fixture");
        assert!(load_adapter_checkpoint(Some(&cache), &adapter).is_none());
        fs::remove_dir_all(fixture).expect("remove WAL fixture");
    }

    #[test]
    fn metadata_limit_and_persistence_failure_finish_as_partial() {
        let fixture = env::temp_dir().join(format!("dtoken-learning-truncated-{}", unix_millis()));
        fs::create_dir_all(fixture.join("first/second")).expect("create truncated fixture");
        let summary = scan_root_metadata(&fixture, 4, 2, || false).expect("scan bounded root");
        assert!(summary.truncated);

        let mut snapshot = empty_snapshot("learning-partial-test");
        snapshot.sources.push(LearningSourceState {
            id: "codex".to_string(),
            label: "Codex".to_string(),
            state: "partial".to_string(),
            roots_found: 1,
            entry_count: 2,
            session_count: 1,
            conversation_count: 1,
            subagent_count: 0,
            linked_count: 1,
            unlinked_count: 0,
            corrupt_count: 0,
            unsupported_count: 0,
            truncated: true,
            project_count: 1,
            skipped_count: 1,
            detail: "partial".to_string(),
        });
        finish_learning_snapshot(&mut snapshot, false);
        assert_eq!(snapshot.state, "partial");
        assert_eq!(snapshot.cache_state, "fresh");

        let mut persistence_snapshot = empty_snapshot("learning-persistence-test");
        finish_learning_snapshot(&mut persistence_snapshot, true);
        assert_eq!(persistence_snapshot.state, "partial");
        assert_eq!(persistence_snapshot.cache_state, "unavailable");
        fs::remove_dir_all(fixture).expect("remove truncated fixture");
    }

    #[test]
    fn project_candidates_aggregate_agents_and_sessions_by_canonical_path() {
        let fixture = env::temp_dir().join(format!("dtoken-project-aggregate-{}", unix_millis()));
        fs::create_dir_all(&fixture).expect("create aggregate project fixture");
        let canonical = fs::canonicalize(&fixture).expect("canonical aggregate fixture");
        let claude = candidate_for_project(
            "Claude Code",
            &canonical,
            &RootSummary {
                session_count: 2,
                project_count: 1,
                ..RootSummary::default()
            },
            "claude-project-directory-mapping-verified",
            "Claude evidence",
        );
        let codex = candidate_for_project(
            "Codex",
            &canonical,
            &RootSummary {
                session_count: 3,
                project_count: 1,
                ..RootSummary::default()
            },
            "codex-jsonl-project-metadata-verified",
            "Codex evidence",
        );

        let candidates = aggregate_project_candidates(vec![claude, codex]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, canonical.display().to_string());
        assert_eq!(candidates[0].agent, "Claude Code, Codex");
        assert_eq!(candidates[0].kind, "project");
        assert_eq!(candidates[0].session_count, 5);
        assert_eq!(candidates[0].project_count, 2);
        assert!(candidates[0].write_eligible);
        assert!(candidates[0]
            .reasons
            .iter()
            .any(|reason| reason == "Claude evidence"));
        assert!(candidates[0]
            .reasons
            .iter()
            .any(|reason| reason == "Codex evidence"));
        fs::remove_dir_all(fixture).expect("remove aggregate fixture");
    }

    #[test]
    fn project_candidates_deduplicate_git_worktrees_without_merging_distinct_repositories() {
        let fixture = env::temp_dir().join(format!("dtoken-project-worktrees-{}", unix_millis()));
        let main = fixture.join("main");
        let worktree = fixture.join("worktree");
        let other = fixture.join("other/main");
        fs::create_dir_all(main.join(".git/worktrees/worktree")).expect("create main git metadata");
        fs::create_dir_all(&worktree).expect("create linked worktree");
        fs::create_dir_all(other.join(".git")).expect("create distinct repository");
        fs::write(
            worktree.join(".git"),
            format!(
                "gitdir: {}\n",
                main.join(".git/worktrees/worktree").display()
            ),
        )
        .expect("write worktree git marker");

        let main_candidate = candidate_for_project(
            "Codex",
            &main,
            &RootSummary {
                session_count: 2,
                project_count: 1,
                modified: Some(10),
                ..RootSummary::default()
            },
            "codex-jsonl-project-metadata-verified",
            "main checkout evidence",
        );
        let worktree_candidate = candidate_for_project(
            "Claude Code",
            &worktree,
            &RootSummary {
                session_count: 3,
                project_count: 1,
                modified: Some(20),
                ..RootSummary::default()
            },
            "claude-project-directory-mapping-verified",
            "worktree evidence",
        );
        let other_candidate = candidate_for_project(
            "Codex",
            &other,
            &RootSummary {
                session_count: 1,
                project_count: 1,
                modified: Some(30),
                ..RootSummary::default()
            },
            "codex-jsonl-project-metadata-verified",
            "distinct repository evidence",
        );

        let candidates =
            aggregate_project_candidates(vec![main_candidate, worktree_candidate, other_candidate]);

        assert_eq!(candidates.len(), 2);
        let merged = candidates
            .iter()
            .find(|candidate| candidate.session_count == 5)
            .expect("merged worktree candidate");
        assert_eq!(
            merged.path,
            fs::canonicalize(&worktree).unwrap().display().to_string()
        );
        assert_eq!(merged.agent, "Claude Code, Codex");
        assert!(candidates.iter().any(|candidate| {
            candidate.path == fs::canonicalize(&other).unwrap().display().to_string()
                && candidate.session_count == 1
        }));
        fs::remove_dir_all(fixture).expect("remove worktree fixture");
    }

    #[test]
    fn project_candidates_deduplicate_stale_absolute_worktree_markers() {
        let fixture =
            env::temp_dir().join(format!("dtoken-project-stale-worktrees-{}", unix_millis()));
        let first = fixture.join("first");
        let second = fixture.join("second");
        let unrelated = fixture.join("unrelated");
        fs::create_dir_all(&first).expect("create first stale worktree");
        fs::create_dir_all(&second).expect("create second stale worktree");
        fs::create_dir_all(&unrelated).expect("create unrelated stale worktree");
        let missing_common = fixture.join("removed-repository/.git");
        fs::write(
            first.join(".git"),
            format!(
                "gitdir: {}\n",
                missing_common.join("worktrees/first").display()
            ),
        )
        .expect("write first stale marker");
        fs::write(
            second.join(".git"),
            format!(
                "gitdir: {}\n",
                missing_common.join("worktrees/second").display()
            ),
        )
        .expect("write second stale marker");
        fs::write(
            unrelated.join(".git"),
            format!(
                "gitdir: {}\n",
                fixture
                    .join("another-removed-repository/.git/worktrees/unrelated")
                    .display()
            ),
        )
        .expect("write unrelated stale marker");

        let candidates = aggregate_project_candidates(vec![
            candidate_for_project(
                "Codex",
                &first,
                &RootSummary {
                    session_count: 2,
                    project_count: 1,
                    ..RootSummary::default()
                },
                "verified",
                "first stale worktree",
            ),
            candidate_for_project(
                "Codex",
                &second,
                &RootSummary {
                    session_count: 3,
                    project_count: 1,
                    ..RootSummary::default()
                },
                "verified",
                "second stale worktree",
            ),
            candidate_for_project(
                "Codex",
                &unrelated,
                &RootSummary {
                    session_count: 1,
                    project_count: 1,
                    ..RootSummary::default()
                },
                "verified",
                "unrelated stale worktree",
            ),
        ]);

        assert_eq!(candidates.len(), 2);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.session_count == 5));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.session_count == 1));
        fs::remove_dir_all(fixture).expect("remove stale worktree fixture");
    }

    #[cfg(unix)]
    #[test]
    fn project_candidates_deduplicate_safe_symlink_aliases() {
        use std::os::unix::fs::symlink;

        let fixture = env::temp_dir().join(format!("dtoken-project-symlink-{}", unix_millis()));
        let project = fixture.join("project");
        let alias = fixture.join("project-alias");
        fs::create_dir_all(&project).expect("create symlink project fixture");
        symlink(&project, &alias).expect("create project symlink alias");
        let direct = candidate_for_project(
            "Codex",
            &project,
            &RootSummary {
                session_count: 1,
                project_count: 1,
                ..RootSummary::default()
            },
            "verified",
            "direct path",
        );
        let aliased = candidate_for_project(
            "Claude Code",
            &alias,
            &RootSummary {
                session_count: 2,
                project_count: 1,
                ..RootSummary::default()
            },
            "verified",
            "symlink alias",
        );

        let candidates = aggregate_project_candidates(vec![direct, aliased]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].path,
            fs::canonicalize(&project).unwrap().display().to_string()
        );
        assert_eq!(candidates[0].session_count, 3);
        fs::remove_dir_all(fixture).expect("remove symlink fixture");
    }

    #[test]
    fn recovered_cached_snapshot_exposes_a_terminal_progress_state() {
        let mut snapshot = empty_snapshot("learning-recovered");
        snapshot.state = "running".to_string();
        snapshot.phase = "scanning-session-metadata".to_string();
        snapshot.progress.current_stage = "scanning-session-metadata".to_string();
        snapshot.progress.cancellable = true;
        let cached = CachedScanJob {
            job_id: snapshot.job_id.clone(),
            state: "partial".to_string(),
            phase: "restart-recovered".to_string(),
            saved_at_unix: 42,
            updated_at_ms: 42_000,
            snapshot: serde_json::to_value(snapshot).expect("serialize cached snapshot"),
        };

        let recovered = cached_snapshot(cached).expect("recover cached snapshot");

        assert_eq!(recovered.state, "partial");
        assert_eq!(recovered.phase, "restart-recovered");
        assert_eq!(recovered.progress.current_stage, "partial");
        assert!(!recovered.progress.cancellable);
    }

    #[test]
    fn intermediate_snapshot_persistence_aggregates_duplicate_project_ids() {
        let fixture =
            env::temp_dir().join(format!("dtoken-learning-persist-dedup-{}", unix_millis()));
        fs::create_dir_all(&fixture).expect("create persisted project fixture");
        let mut snapshot = empty_snapshot("learning-persist-dedup");
        snapshot.candidates = vec![
            candidate_for_project(
                "Claude Code",
                &fixture,
                &RootSummary {
                    session_count: 2,
                    project_count: 1,
                    ..RootSummary::default()
                },
                "verified",
                "Claude evidence",
            ),
            candidate_for_project(
                "Codex",
                &fixture,
                &RootSummary {
                    session_count: 3,
                    project_count: 1,
                    ..RootSummary::default()
                },
                "verified",
                "Codex evidence",
            ),
        ];

        let persisted = snapshot_for_persistence(&snapshot);

        assert_eq!(snapshot.candidates.len(), 2, "live scan state is unchanged");
        assert_eq!(persisted.candidates.len(), 1);
        assert_eq!(persisted.candidates[0].session_count, 5);
        assert_eq!(persisted.candidates[0].agent, "Claude Code, Codex");
        fs::remove_dir_all(fixture).expect("remove persisted project fixture");
    }
}
