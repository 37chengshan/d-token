use anyhow::Result;
use dtoken_types::{DtokenEvent, EventSource, EventStatus, LayeredStage};
use serde_json::json;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
        });
        ledger
    }
}

#[derive(Debug, Clone)]
pub struct JsonlEventLedger {
    path: PathBuf,
}

impl JsonlEventLedger {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn append(&self, event: &DtokenEvent) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(event)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<DtokenEvent>> {
        if !Path::new(&self.path).exists() {
            return Ok(vec![]);
        }
        let file = OpenOptions::new().read(true).open(&self.path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            events.push(serde_json::from_str::<DtokenEvent>(&line)?);
        }
        events.reverse();
        events.truncate(limit);
        Ok(events)
    }
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

    #[test]
    fn jsonl_ledger_persists_structured_events_without_full_payloads() {
        let path = std::env::temp_dir().join(format!("dtoken-ledger-{}.jsonl", Uuid::new_v4()));
        let ledger = JsonlEventLedger::new(&path);
        let event = DtokenEvent {
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
            metadata: json!({ "summary": "aggregate only" }),
        };

        ledger.append(&event).expect("append event");
        let recent = ledger.recent(10).expect("read recent events");

        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].saved_tokens, Some(60));
        assert!(recent[0].metadata.get("full_prompt").is_none());
        let _ = std::fs::remove_file(path);
    }
}
