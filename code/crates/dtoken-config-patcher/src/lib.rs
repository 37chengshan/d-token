use anyhow::{Context, Result};
use dtoken_types::{ConfigPatchPlan, ConfigToolKind, RiskLevel, RollbackPoint};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use uuid::Uuid;

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
        let target_file = target_file.as_ref();
        let before = fs::read_to_string(target_file).unwrap_or_default();
        let before_hash = stable_hash(&before);
        let after_hash = stable_hash(next_content);
        let diff = simple_diff(&before, next_content);
        Ok(ConfigPatchPlan {
            id: Uuid::new_v4().to_string(),
            target_file: target_file.display().to_string(),
            tool,
            agent,
            before_hash,
            after_hash,
            diff,
            risk_level,
            requires_restart: true,
        })
    }

    pub fn backup(&self, plan: &ConfigPatchPlan) -> Result<RollbackPoint> {
        let rollback_id = Uuid::new_v4().to_string();
        let backup_dir = self.backup_root.join(&rollback_id);
        fs::create_dir_all(&backup_dir).with_context(|| {
            format!("failed to create backup directory {}", backup_dir.display())
        })?;

        let target = PathBuf::from(&plan.target_file);
        if target.exists() {
            let file_name = target
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("config.backup");
            fs::copy(&target, backup_dir.join(file_name))
                .with_context(|| format!("failed to backup {}", target.display()))?;
        }

        Ok(RollbackPoint {
            id: rollback_id,
            created_at: "m1-placeholder".to_string(),
            backup_dir: backup_dir.display().to_string(),
            target_files: vec![plan.target_file.clone()],
            patch_ids: vec![plan.id.clone()],
        })
    }

    pub fn apply_replace(
        &self,
        plan: &ConfigPatchPlan,
        next_content: &str,
    ) -> Result<RollbackPoint> {
        let target = PathBuf::from(&plan.target_file);
        let before = fs::read_to_string(&target).unwrap_or_default();
        let before_hash = stable_hash(&before);
        if before_hash != plan.before_hash {
            anyhow::bail!("config hash mismatch; refusing to apply stale patch plan");
        }
        let rollback = self.backup(plan)?;
        fs::write(&target, next_content)
            .with_context(|| format!("failed to write {}", target.display()))?;
        Ok(rollback)
    }
}

pub fn stable_hash(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn simple_diff(before: &str, after: &str) -> String {
    if before == after {
        return "no changes".to_string();
    }
    format!(
        "--- before\n+++ after\n@@\n-before: {} bytes\n+after: {} bytes",
        before.len(),
        after.len()
    )
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
