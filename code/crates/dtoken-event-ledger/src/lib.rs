use anyhow::Result;
use dtoken_types::{DtokenEvent, EventSource, EventStatus, LayeredStage};
use serde_json::json;
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

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
