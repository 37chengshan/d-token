use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Official,
    Custom,
    LocalChain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChainMode {
    Direct,
    ChainBridge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompressionPolicy {
    Off,
    Safe,
    Balanced,
    Aggressive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub protocol: Protocol,
    pub base_url: String,
    pub api_key_ref: Option<String>,
    pub models: Vec<String>,
    pub timeout_ms: u64,
    pub healthcheck_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfile {
    pub id: String,
    pub name: String,
    pub input_protocol: Protocol,
    pub instance_id: String,
    pub target_provider_id: String,
    pub fallback_provider_id: Option<String>,
    pub chain_mode: ChainMode,
    pub compression_policy: CompressionPolicy,
    pub bind_host: String,
    pub local_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceProfile {
    pub id: String,
    pub name: String,
    pub agent: String,
    pub route_id: String,
    pub local_port: u16,
    pub process_status: ProcessStatus,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    pub request_id: Uuid,
    pub route_id: String,
    pub provider_id: String,
    pub chain_mode: ChainMode,
    pub compression_policy: CompressionPolicy,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub tokens_saved: usize,
    pub transforms_applied: Vec<String>,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLog {
    pub request_id: Uuid,
    pub timestamp_ms: u64,
    pub agent: Option<String>,
    pub instance_id: Option<String>,
    pub input_protocol: Protocol,
    pub provider_id: Option<String>,
    pub route_id: Option<String>,
    pub chain_mode: Option<ChainMode>,
    pub model: Option<String>,
    pub input_tokens_before: Option<usize>,
    pub input_tokens_after: Option<usize>,
    pub output_tokens: Option<usize>,
    pub tokens_saved: Option<usize>,
    pub transforms_applied: Vec<String>,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub config_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub component: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
}
