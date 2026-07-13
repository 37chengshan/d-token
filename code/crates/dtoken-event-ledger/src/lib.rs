use anyhow::{anyhow, Context, Result};
use dtoken_types::{DtokenEvent, EventSource, EventStatus, LayeredStage};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct EventLedger {
    events: Vec<DtokenEvent>,
}

impl EventLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, event: DtokenEvent) -> Result<()> {
        self.events.push(event);
        Ok(())
    }

    pub fn recent(&self, limit: usize) -> Vec<DtokenEvent> {
        self.events.iter().rev().take(limit).cloned().collect()
    }

    pub fn mock_m1() -> Self {
        let mut ledger = Self::new();
        let _ = ledger.record(DtokenEvent {
            event_id: Uuid::new_v4(),
            timestamp_ms: now_ms(),
            project_id: Some("demo-project".to_string()),
            agent_id: Some("manual".to_string()),
            instance_id: Some("demo-instance".to_string()),
            command_id: None,
            request_id: None,
            stage: LayeredStage::InstallStep,
            source: EventSource::Dtoken,
            status: EventStatus::Ok,
            input_tokens: None,
            output_tokens: None,
            saved_tokens: None,
            baseline_kind: None,
            error_code: None,
            metadata: json!({ "message": "M1 event ledger is initialized" }),
            context_accounting: None,
        });
        ledger
    }
}

#[derive(Debug, Clone)]
pub struct JsonlEventLedger {
    store: JsonlStore,
}

impl JsonlEventLedger {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            store: JsonlStore::new(path),
        }
    }

    pub fn append(&self, event: &DtokenEvent) -> Result<()> {
        self.store.append(event)
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<DtokenEvent>> {
        self.store.recent(limit)
    }
}

/// A concurrency-safe JSONL primitive shared by every d-token runtime metadata log.
///
/// Records are committed as complete newline-terminated JSON values. Writers for the same path
/// share an in-process lock, repair an invalid tail before appending, and sync committed bytes to
/// disk.
#[derive(Debug, Clone)]
pub struct JsonlStore {
    path: PathBuf,
    io_lock: Arc<LedgerIoLock>,
}

impl JsonlStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let io_lock = shared_path_lock(&path);
        Self { path, io_lock }
    }

    pub fn append<T: Serialize + DeserializeOwned>(&self, value: &T) -> Result<()> {
        let mut record = serde_json::to_vec(value).context("serialize JSONL record")?;
        record.push(b'\n');

        let mut io_state = self
            .io_lock
            .lock()
            .map_err(|_| anyhow!("JSONL I/O lock is poisoned"))?;
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create JSONL directory {}", parent.display()))?;
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.path)
            .with_context(|| format!("open JSONL store {}", self.path.display()))?;

        let observed_len = file
            .metadata()
            .with_context(|| format!("inspect JSONL length in {}", self.path.display()))?
            .len();
        if io_state.known_good_len != Some(observed_len) {
            io_state.known_good_len = Some(repair_corrupt_tail::<T>(&mut file, &self.path)?);
        }
        let append_offset = file
            .seek(SeekFrom::End(0))
            .with_context(|| format!("seek to end of JSONL store {}", self.path.display()))?;
        io_state.known_good_len = None;
        file.write_all(&record)
            .with_context(|| format!("append JSONL record to {}", self.path.display()))?;
        file.flush()
            .with_context(|| format!("flush JSONL store {}", self.path.display()))?;
        file.sync_data()
            .with_context(|| format!("sync JSONL store {}", self.path.display()))?;
        io_state.known_good_len = Some(append_offset + record.len() as u64);
        Ok(())
    }

    pub fn recent<T: DeserializeOwned>(&self, limit: usize) -> Result<Vec<T>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| anyhow!("JSONL I/O lock is poisoned"))?;
        if !Path::new(&self.path).exists() {
            return Ok(vec![]);
        }
        let file = OpenOptions::new()
            .read(true)
            .open(&self.path)
            .with_context(|| format!("open JSONL store {}", self.path.display()))?;
        let mut reader = BufReader::new(file);
        let mut records = VecDeque::with_capacity(limit.min(1024));
        let mut line = Vec::new();

        loop {
            line.clear();
            let bytes_read = reader
                .read_until(b'\n', &mut line)
                .with_context(|| format!("read JSONL store {}", self.path.display()))?;
            if bytes_read == 0 {
                break;
            }
            if !line.ends_with(b"\n") {
                break;
            }
            let payload = &line[..line.len() - 1];
            if payload.iter().all(u8::is_ascii_whitespace) {
                continue;
            }

            match serde_json::from_slice::<T>(payload) {
                Ok(record) => {
                    if records.len() == limit {
                        records.pop_front();
                    }
                    records.push_back(record);
                }
                Err(error) => {
                    let is_tail = reader
                        .fill_buf()
                        .with_context(|| format!("inspect JSONL tail in {}", self.path.display()))?
                        .is_empty();
                    if is_tail {
                        break;
                    }
                    return Err(error).with_context(|| {
                        format!("parse non-tail JSONL record in {}", self.path.display())
                    });
                }
            }
        }

        Ok(records.into_iter().rev().collect())
    }
}

#[derive(Debug, Default)]
struct LedgerIoState {
    known_good_len: Option<u64>,
}

type LedgerIoLock = Mutex<LedgerIoState>;
type LedgerLockRegistry = Mutex<HashMap<PathBuf, Weak<LedgerIoLock>>>;

fn shared_path_lock(path: &Path) -> Arc<LedgerIoLock> {
    static LOCKS: OnceLock<LedgerLockRegistry> = OnceLock::new();
    let registry = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return lock;
    }

    let lock = Arc::new(Mutex::new(LedgerIoState::default()));
    locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
    lock
}

fn repair_corrupt_tail<T: DeserializeOwned>(file: &mut File, path: &Path) -> Result<u64> {
    let reader_file = File::open(path)
        .with_context(|| format!("inspect event ledger tail in {}", path.display()))?;
    let mut reader = BufReader::new(reader_file);
    let mut line = Vec::new();
    let mut valid_prefix_len = 0_u64;

    loop {
        line.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut line)
            .with_context(|| format!("inspect event ledger tail in {}", path.display()))?;
        if bytes_read == 0 {
            break;
        }

        let terminated = line.ends_with(b"\n");
        if !terminated {
            break;
        }
        let payload = &line[..line.len() - 1];
        if !payload.iter().all(u8::is_ascii_whitespace)
            && serde_json::from_slice::<T>(payload).is_err()
        {
            break;
        }
        valid_prefix_len += bytes_read as u64;
    }

    let current_len = file
        .metadata()
        .with_context(|| format!("inspect event ledger length in {}", path.display()))?
        .len();
    if valid_prefix_len < current_len {
        file.set_len(valid_prefix_len)
            .with_context(|| format!("isolate corrupt event ledger tail in {}", path.display()))?;
    }
    Ok(valid_prefix_len)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!("dtoken-ledger-{}.jsonl", Uuid::new_v4()))
    }

    fn test_event(sequence: usize) -> DtokenEvent {
        DtokenEvent {
            event_id: Uuid::new_v4(),
            timestamp_ms: now_ms(),
            project_id: Some("p1".to_string()),
            agent_id: None,
            instance_id: None,
            command_id: None,
            request_id: None,
            stage: LayeredStage::RtkFilter,
            source: EventSource::Rtk,
            status: EventStatus::Ok,
            input_tokens: Some(100),
            output_tokens: Some(40),
            saved_tokens: Some(60),
            baseline_kind: None,
            error_code: None,
            metadata: json!({ "sequence": sequence, "summary": "aggregate only" }),
            context_accounting: None,
        }
    }

    #[test]
    fn jsonl_ledger_persists_structured_events_without_full_payloads() {
        let path = test_path();
        let ledger = JsonlEventLedger::new(&path);
        let event = test_event(0);

        ledger.append(&event).expect("append event");
        let recent = ledger.recent(10).expect("read recent events");

        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].saved_tokens, Some(60));
        assert!(recent[0].metadata.get("full_prompt").is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recent_ignores_an_unterminated_tail_and_preserves_valid_events() {
        let path = test_path();
        let ledger = JsonlEventLedger::new(&path);
        let first = test_event(1);
        let second = test_event(2);
        ledger.append(&first).expect("append first event");
        ledger.append(&second).expect("append second event");

        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open ledger for fault injection");
        file.write_all(br#"{"event_id":"truncated"#)
            .expect("inject truncated tail");
        file.flush().expect("flush injected tail");

        let recent = ledger
            .recent(10)
            .expect("a truncated tail must not hide valid events");
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].event_id, second.event_id);
        assert_eq!(recent[1].event_id, first.event_id);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn append_repairs_a_corrupt_tail_before_committing_the_next_event() {
        let path = test_path();
        let ledger = JsonlEventLedger::new(&path);
        let first = test_event(1);
        let second = test_event(2);
        ledger.append(&first).expect("append first event");

        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open ledger for fault injection");
        file.write_all(b"{}\n")
            .expect("inject schema-invalid JSON tail");
        file.flush().expect("flush injected tail");

        ledger
            .append(&second)
            .expect("append should isolate the corrupt tail");
        let recent = ledger.recent(10).expect("read repaired ledger");
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].event_id, second.event_id);
        assert_eq!(recent[1].event_id, first.event_id);

        let contents = std::fs::read_to_string(&path).expect("read repaired JSONL");
        assert!(contents.ends_with('\n'));
        assert_eq!(contents.lines().count(), 2);
        assert!(contents
            .lines()
            .all(|line| serde_json::from_str::<DtokenEvent>(line).is_ok()));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn concurrent_appends_produce_complete_unique_jsonl_records() {
        const THREADS: usize = 8;
        const EVENTS_PER_THREAD: usize = 40;
        let path = test_path();
        let ledger = Arc::new(JsonlEventLedger::new(&path));
        let barrier = Arc::new(Barrier::new(THREADS));

        let handles = (0..THREADS)
            .map(|thread_index| {
                let ledger = Arc::clone(&ledger);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    for event_index in 0..EVENTS_PER_THREAD {
                        let sequence = thread_index * EVENTS_PER_THREAD + event_index;
                        ledger
                            .append(&test_event(sequence))
                            .expect("concurrent append must succeed");
                    }
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().expect("writer thread must not panic");
        }

        let expected = THREADS * EVENTS_PER_THREAD;
        let recent = ledger.recent(expected).expect("read concurrent events");
        assert_eq!(recent.len(), expected);
        let sequences = recent
            .iter()
            .map(|event| {
                event.metadata["sequence"]
                    .as_u64()
                    .expect("sequence metadata")
            })
            .collect::<HashSet<_>>();
        assert_eq!(sequences.len(), expected);

        let bytes = std::fs::read(&path).expect("read concurrent JSONL");
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(
            bytes.iter().filter(|byte| **byte == b'\n').count(),
            expected
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn generic_jsonl_store_repairs_tail_and_serializes_concurrent_metadata_writers() {
        const THREADS: usize = 6;
        const RECORDS_PER_THREAD: usize = 24;
        let path = test_path();
        let store = JsonlStore::new(&path);
        store.append(&json!({"sequence": 0})).expect("seed store");
        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open store for fault injection");
        file.write_all(b"truncated-tail")
            .expect("inject truncated tail");
        file.flush().expect("flush injected tail");

        let store = Arc::new(store);
        let barrier = Arc::new(Barrier::new(THREADS));
        let handles = (0..THREADS)
            .map(|thread_index| {
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    for record_index in 0..RECORDS_PER_THREAD {
                        let sequence = 1 + thread_index * RECORDS_PER_THREAD + record_index;
                        store
                            .append(&json!({"sequence": sequence}))
                            .expect("concurrent metadata append");
                    }
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().expect("writer thread must not panic");
        }

        let expected = 1 + THREADS * RECORDS_PER_THREAD;
        let recent = store
            .recent::<Value>(expected)
            .expect("read generic metadata records");
        assert_eq!(recent.len(), expected);
        let sequences = recent
            .iter()
            .filter_map(|record| record["sequence"].as_u64())
            .collect::<HashSet<_>>();
        assert_eq!(sequences.len(), expected);
        let bytes = std::fs::read(&path).expect("read generic JSONL");
        assert!(bytes.ends_with(b"\n"));
        assert!(bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .all(|line| serde_json::from_slice::<Value>(line).is_ok()));

        let _ = std::fs::remove_file(path);
    }
}
