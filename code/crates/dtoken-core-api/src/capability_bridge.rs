//! R2.E: process-wide Runtime Capability Registry bridge for the request
//! pipeline.
//!
//! Kept out of `AppState` to avoid changing the public state shape; a single
//! process-wide registry accumulates seed/observed/learned facts for the
//! lifetime of the runtime. The request pipeline only reads capability
//! summaries (never blocking on learning) and attaches them to request logs
//! and ledger events as receipt evidence.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use dtoken_context_store::apply_private_file_acl;

use dtoken_capability_registry::{
    CapabilityRegistry, ModelObservation, RegistryLimits, ResolveOutcome,
};
use serde_json::{json, Value};

/// Optional persistence path for observed/learned facts. When set, the
/// registry is hydrated at first access and snapshotted on mutations so
/// learning survives process restarts (PRD P0.0.11.3 runtime facts).
fn registry_store_path() -> Option<PathBuf> {
    let default_path = || {
        DEFAULT_REGISTRY_STORE_PATH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    };
    #[cfg(test)]
    {
        if let Some(path) = default_path() {
            return Some(path);
        }
    }
    std::env::var_os("DTOKEN_CAPABILITY_REGISTRY_STORE")
        .map(PathBuf::from)
        .or_else(default_path)
}

static DEFAULT_REGISTRY_STORE_PATH: LazyLock<Mutex<Option<PathBuf>>> =
    LazyLock::new(|| Mutex::new(None));

struct RegistryState {
    registry: CapabilityRegistry,
    hydrated_store_path: Option<PathBuf>,
}

impl RegistryState {
    fn new() -> Self {
        let mut state = Self {
            registry: CapabilityRegistry::new(RegistryLimits::default()),
            hydrated_store_path: None,
        };
        hydrate_current_store(&mut state);
        state
    }
}

/// Binds capability learning to the persistent runtime data directory when the
/// caller did not provide an explicit `DTOKEN_CAPABILITY_REGISTRY_STORE`.
pub fn set_default_registry_store_path(path: impl Into<PathBuf>) {
    {
        let mut default = DEFAULT_REGISTRY_STORE_PATH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if default.is_none() {
            *default = Some(path.into());
        }
    }
    let _persist = persistence_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    hydrate_current_store(&mut state);
}

fn registry() -> &'static Mutex<RegistryState> {
    static REGISTRY: LazyLock<Mutex<RegistryState>> =
        LazyLock::new(|| Mutex::new(RegistryState::new()));
    &REGISTRY
}

fn persistence_lock() -> &'static Mutex<()> {
    static PERSISTENCE: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    &PERSISTENCE
}

fn hydrate_current_store(state: &mut RegistryState) -> Option<PathBuf> {
    let path = registry_store_path()?;
    if state.hydrated_store_path.as_ref() == Some(&path) {
        return Some(path);
    }
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(json) = serde_json::from_str::<Value>(&raw) {
            state.registry.hydrate(json);
        }
    }
    state.hydrated_store_path = Some(path.clone());
    Some(path)
}

fn mutate_and_persist(mut mutate: impl FnMut(&mut CapabilityRegistry) -> bool) {
    let _persist = persistence_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = hydrate_current_store(&mut state);
    if !mutate(&mut state.registry) {
        return;
    }
    let serialized = match state.registry.snapshot_json() {
        Ok(serialized) => serialized,
        Err(_) => return,
    };
    drop(state);
    if let Some(path) = path {
        let _ = write_snapshot_atomically(&path, serialized.as_bytes());
    }
}

fn write_snapshot_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = snapshot_parent(path);
    fs::create_dir_all(parent)?;
    let temp_path = snapshot_temp_path(parent, path)?;
    let mut file = open_new_private_file(&temp_path)?;
    let mut guard = SnapshotTempGuard::new(temp_path);
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(test)]
    if FAIL_NEXT_REPLACE_BEFORE_RENAME.swap(false, Ordering::SeqCst) {
        return Err(io::Error::other("injected snapshot replace failure"));
    }
    atomic_rename(guard.path(), path)?;
    guard.disarm();
    sync_directory(parent)?;
    restrict_private_file_path(path)?;
    Ok(())
}

fn snapshot_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn snapshot_temp_path(parent: &Path, path: &Path) -> io::Result<PathBuf> {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "capability registry path has no file name",
        ));
    };
    Ok(parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4())))
}

struct SnapshotTempGuard {
    path: PathBuf,
    armed: bool,
}

impl SnapshotTempGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SnapshotTempGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn open_new_private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(test)]
    if FAIL_NEXT_PRIVATE_PERMISSION.swap(false, Ordering::SeqCst) {
        let _ = fs::remove_file(path);
        return Err(io::Error::other("injected private permission failure"));
    }
    if let Err(error) = restrict_private_file(&file, path) {
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(file)
}

#[cfg(not(windows))]
fn atomic_rename(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn atomic_rename(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both path buffers are NUL-terminated, live for the duration of
    // the call, and come directly from Rust paths without pointer aliasing.
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_private_file(file: &File, _path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(windows)]
fn restrict_private_file(_file: &File, path: &Path) -> io::Result<()> {
    apply_private_file_acl(path).map_err(io::Error::other)
}

#[cfg(not(any(unix, windows)))]
fn restrict_private_file(_file: &File, _path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_private_file_path(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(windows)]
fn restrict_private_file_path(path: &Path) -> io::Result<()> {
    apply_private_file_acl(path).map_err(io::Error::other)
}

#[cfg(not(any(unix, windows)))]
fn restrict_private_file_path(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
static FAIL_NEXT_REPLACE_BEFORE_RENAME: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static FAIL_NEXT_PRIVATE_PERMISSION: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
fn reset_for_tests(store_path: Option<PathBuf>) {
    let _persist = persistence_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    {
        let mut default = DEFAULT_REGISTRY_STORE_PATH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *default = store_path;
    }
    let mut state = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.registry = CapabilityRegistry::new(RegistryLimits::default());
    state.hydrated_store_path = None;
    hydrate_current_store(&mut state);
}

#[cfg(test)]
fn fail_next_replace_before_rename_for_tests() {
    FAIL_NEXT_REPLACE_BEFORE_RENAME.store(true, Ordering::SeqCst);
}

#[cfg(test)]
fn fail_next_private_permission_for_tests() {
    FAIL_NEXT_PRIVATE_PERMISSION.store(true, Ordering::SeqCst);
}

/// Records a completed request observation (P0.0.11 learning loop).
/// Best-effort: never fails the request path; the CAS revision is taken under
/// the registry lock and persistence failures are dropped.
pub fn record_observation(observation: ModelObservation) {
    mutate_and_persist(|registry| {
        let revision = registry.current_revision();
        registry
            .record_observation(observation.clone(), revision)
            .is_ok()
    });
}

/// Confirms a previously inferred capability (promotes to Learned).
pub fn confirm_learned(capability: dtoken_capability_registry::ModelCapability) {
    mutate_and_persist(|registry| {
        let revision = registry.current_revision();
        registry
            .confirm_learned(capability.clone(), revision)
            .is_ok()
    });
}

/// Declares an explicit wildcard pattern from agent config/follow.
pub fn declare_wildcard(pattern: &str) {
    mutate_and_persist(|registry| {
        let revision = registry.current_revision();
        registry.declare_wildcard(pattern);
        registry.current_revision() != revision
    });
}

/// Resolves a request model to a capability summary for receipt evidence.
/// Never fails: unknown models yield an `inferred` summary with the default
/// strategy (`unknown: true`), matching PRD P0.0.11.4.
pub fn resolve_model_capability_summary(model: &str, protocol: Option<&str>) -> Value {
    let _persist = persistence_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    hydrate_current_store(&mut state);
    let outcome = state.registry.resolve(model, protocol);
    let cap = outcome.capability();
    let pattern = match &outcome {
        ResolveOutcome::PatternHit(_, pattern) => Some(pattern.as_str()),
        _ => None,
    };
    json!({
        "model_id": cap.model_id,
        "source": match cap.source {
            dtoken_capability_registry::CapabilitySource::Seed => "seed",
            dtoken_capability_registry::CapabilitySource::Observed => "observed",
            dtoken_capability_registry::CapabilitySource::Learned => "learned",
            dtoken_capability_registry::CapabilitySource::Inferred => "inferred",
        },
        "unknown": cap.unknown,
        "wildcard_pattern": pattern,
        "compression_strategy": match cap.compression_strategy {
            dtoken_capability_registry::CompressionStrategy::SemanticV2 => "semantic_v2",
            dtoken_capability_registry::CompressionStrategy::SlidingWindow => "sliding_window",
            dtoken_capability_registry::CompressionStrategy::FrozenFloor => "frozen_floor",
            dtoken_capability_registry::CompressionStrategy::ToolSchema => "tool_schema",
            dtoken_capability_registry::CompressionStrategy::Default => "default",
        },
        "protocol": cap.protocol,
        "context_window": cap.context_window,
        "supports_tool_call": cap.supports_tool_call,
        "supports_stream": cap.supports_stream,
        "supports_reasoning": cap.supports_reasoning,
        "supports_structured_output": cap.supports_structured_output,
        "revision": cap.revision,
    })
}

/// Compression policy hint derived from the capability (mapped by
/// dtoken-compress). Returns None when the capability is unknown (route policy
/// governs); callers must not block on it.
pub fn compression_hint_for(model: &str, protocol: Option<&str>) -> Option<String> {
    let _persist = persistence_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    hydrate_current_store(&mut state);
    let cap = state.registry.resolve(model, protocol).capability().clone();
    if cap.unknown {
        return None;
    }
    Some(
        match cap.compression_strategy {
            dtoken_capability_registry::CompressionStrategy::SemanticV2 => "semantic_v2",
            dtoken_capability_registry::CompressionStrategy::SlidingWindow => "sliding_window",
            dtoken_capability_registry::CompressionStrategy::FrozenFloor => "frozen_floor",
            dtoken_capability_registry::CompressionStrategy::ToolSchema => "tool_schema",
            dtoken_capability_registry::CompressionStrategy::Default => "default",
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Mutex as StdMutex, thread};

    use dtoken_capability_registry::ModelObservation;

    use super::*;

    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn temp_store_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dtoken-capability-bridge-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp capability dir");
        dir.join("capability-registry.json")
    }

    fn observation(
        model: &str,
        request_id: &str,
        tool: Option<bool>,
        stream: Option<bool>,
    ) -> ModelObservation {
        ModelObservation {
            model_id: model.to_string(),
            request_id: request_id.to_string(),
            observed_at_ms: 1,
            context_window_hint: None,
            max_output_tokens_hint: None,
            tool_call_ok: tool,
            stream_ok: stream,
            reasoning_ok: None,
            structured_output_ok: None,
            protocol: Some("openai".to_string()),
        }
    }

    fn read_snapshot(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).expect("read capability snapshot"))
            .expect("parse capability snapshot")
    }

    fn observed_entry<'a>(snapshot: &'a Value, model: &str) -> &'a Value {
        snapshot["observed"]
            .as_array()
            .expect("observed entries")
            .iter()
            .find(|entry| entry["model"] == model)
            .expect("observed model entry")
    }

    #[cfg(unix)]
    fn assert_private_file_mode(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .expect("snapshot metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(not(unix))]
    fn assert_private_file_mode(_path: &Path) {}

    fn owned_temp_files(parent: &Path) -> Vec<PathBuf> {
        fs::read_dir(parent)
            .expect("read temp parent")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(".capability-registry.json.") && name.ends_with(".tmp")
                    })
            })
            .collect()
    }

    #[test]
    fn unknown_model_resolves_to_inferred_without_failing() {
        let summary = resolve_model_capability_summary("brand-new-model-x", Some("responses"));
        assert_eq!(summary["unknown"], true);
        assert_eq!(summary["source"], "inferred");
        assert_eq!(summary["compression_strategy"], "default");
        assert_eq!(summary["model_id"], "brand-new-model-x");
    }

    #[test]
    fn capability_hint_is_none_for_unknown_model() {
        assert_eq!(
            compression_hint_for("brand-new-model-x", Some("responses")),
            None
        );
    }

    #[test]
    fn concurrent_observations_persist_latest_revision_and_fields() {
        let _guard = TEST_LOCK.lock().expect("capability bridge test lock");
        let path = temp_store_path("concurrent");
        let parent = path.parent().expect("store parent").to_path_buf();
        reset_for_tests(Some(path.clone()));
        let model = format!("concurrent-capability-{}", uuid::Uuid::new_v4());
        let observations = [
            observation(&model, "tool-1", Some(true), None),
            observation(&model, "tool-2", Some(true), None),
            observation(&model, "stream-1", None, Some(true)),
            observation(&model, "stream-2", None, Some(true)),
        ];

        let handles = observations
            .into_iter()
            .map(|observation| thread::spawn(move || record_observation(observation)))
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().expect("observation thread");
        }

        let summary = resolve_model_capability_summary(&model, Some("openai"));
        assert_eq!(summary["unknown"], false);
        assert_eq!(summary["supports_tool_call"], true);
        assert_eq!(summary["supports_stream"], true);
        let snapshot = read_snapshot(&path);
        assert_private_file_mode(&path);
        assert_eq!(snapshot["revision"], summary["revision"]);
        let entry = observed_entry(&snapshot, &model);
        assert_eq!(entry["fields"]["supports_tool_call"]["positive_count"], 2);
        assert_eq!(entry["fields"]["supports_stream"]["positive_count"], 2);
        assert!(owned_temp_files(&parent).is_empty());
        reset_for_tests(None);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn private_permission_failure_removes_created_temp_without_touching_target() {
        let _guard = TEST_LOCK.lock().expect("capability bridge test lock");
        let path = temp_store_path("permission-failure");
        let parent = path.parent().expect("store parent").to_path_buf();
        fs::write(&path, b"previous snapshot").expect("write previous snapshot");

        fail_next_private_permission_for_tests();
        let error = write_snapshot_atomically(&path, br#"{"schema_version":1}"#)
            .expect_err("injected private permission failure");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(
            fs::read(&path).expect("read previous snapshot"),
            b"previous snapshot"
        );
        assert!(owned_temp_files(&parent).is_empty());
        reset_for_tests(None);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn failed_atomic_replace_preserves_previous_file_and_removes_temp() {
        let _guard = TEST_LOCK.lock().expect("capability bridge test lock");
        let path = temp_store_path("failed-replace");
        let parent = path.parent().expect("store parent").to_path_buf();
        fs::write(&path, b"previous snapshot").expect("write previous snapshot");

        fail_next_replace_before_rename_for_tests();
        let error = write_snapshot_atomically(&path, br#"{"schema_version":1}"#)
            .expect_err("injected replace failure");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(
            fs::read(&path).expect("read previous snapshot"),
            b"previous snapshot"
        );
        assert!(owned_temp_files(&parent).is_empty());
        reset_for_tests(None);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn late_default_path_binding_hydrates_before_persisting_new_observations() {
        let _guard = TEST_LOCK.lock().expect("capability bridge test lock");
        reset_for_tests(None);
        let persisted_model = format!("late-hydrated-{}", uuid::Uuid::new_v4());
        let new_model = format!("late-new-{}", uuid::Uuid::new_v4());
        let path = temp_store_path("late-hydrate");
        let parent = path.parent().expect("store parent").to_path_buf();

        let mut persisted_registry = CapabilityRegistry::new(RegistryLimits::default());
        for request_id in ["persisted-1", "persisted-2"] {
            let revision = persisted_registry.current_revision();
            persisted_registry
                .record_observation(
                    observation(&persisted_model, request_id, Some(true), None),
                    revision,
                )
                .expect("record persisted fixture observation");
        }
        let snapshot = persisted_registry
            .snapshot_json()
            .expect("serialize persisted fixture");
        write_snapshot_atomically(&path, snapshot.as_bytes()).expect("write persisted fixture");

        let early = resolve_model_capability_summary(&persisted_model, Some("openai"));
        assert_eq!(early["unknown"], true);

        set_default_registry_store_path(path.clone());
        let hydrated = resolve_model_capability_summary(&persisted_model, Some("openai"));
        assert_eq!(hydrated["unknown"], false);
        assert_eq!(hydrated["supports_tool_call"], true);

        for request_id in ["new-1", "new-2"] {
            record_observation(observation(&new_model, request_id, Some(true), None));
        }
        let written = read_snapshot(&path);
        assert_private_file_mode(&path);
        observed_entry(&written, &persisted_model);
        observed_entry(&written, &new_model);
        assert!(owned_temp_files(&parent).is_empty());
        reset_for_tests(None);
        let _ = fs::remove_dir_all(parent);
    }
}
