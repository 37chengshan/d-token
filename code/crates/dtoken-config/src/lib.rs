use anyhow::{Context, Result};
use dtoken_types::{InstanceProfile, ProviderProfile, RouteProfile};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DtokenConfig {
    pub providers: Vec<ProviderProfile>,
    pub routes: Vec<RouteProfile>,
    pub instances: Vec<InstanceProfile>,
}

impl DtokenConfig {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let config = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;
        Ok(config)
    }

    pub fn find_provider(&self, id: &str) -> Option<&ProviderProfile> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn find_route(&self, id: &str) -> Option<&RouteProfile> {
        self.routes.iter().find(|r| r.id == id)
    }

    pub fn find_instance(&self, id: &str) -> Option<&InstanceProfile> {
        self.instances.iter().find(|i| i.id == id)
    }
}
