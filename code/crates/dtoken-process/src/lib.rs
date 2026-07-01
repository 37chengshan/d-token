use anyhow::Result;
use dtoken_types::{InstanceProfile, ProcessStatus};
use std::collections::HashMap;
use tokio::process::Child;

#[derive(Default)]
pub struct ProcessManager {
    children: HashMap<String, Child>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self, instance: &InstanceProfile) -> ProcessStatus {
        if self.children.contains_key(&instance.id) {
            ProcessStatus::Running
        } else {
            instance.process_status.clone()
        }
    }

    pub async fn start_placeholder(&mut self, instance: &InstanceProfile) -> Result<ProcessStatus> {
        tracing::info!(instance_id = %instance.id, "M1 placeholder start requested");
        Ok(ProcessStatus::Starting)
    }

    pub async fn stop(&mut self, instance_id: &str) -> Result<ProcessStatus> {
        if let Some(mut child) = self.children.remove(instance_id) {
            let _ = child.kill().await;
        }
        Ok(ProcessStatus::Stopped)
    }
}
