use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use dtoken_config::DtokenConfig;
use dtoken_config_patcher::mock_patch_plan;
use dtoken_doctor::{run_basic_doctor, run_dev_no_touch_doctor};
use dtoken_event_ledger::EventLedger;
use dtoken_headroom_adapter::{
    detect_headroom_status, headroom_cache_summary_mock, headroom_metrics_report,
    headroom_stats_history_json, headroom_stats_mock,
};
use dtoken_preprocessor::{
    detect_rtk_status, list_preprocessors, rtk_discover_report, rtk_gain_mock, rtk_session_report,
};
use dtoken_project_policy::{
    default_policy, demo_projects, RecommendationInput, RuleBasedRecommendationEngine,
};
use dtoken_types::{
    AgentSupportProfile, HealthResponse, LayeredSavingsReport, Protocol, ProtocolInjectionProfile,
    SupportLevel,
};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<DtokenConfig>,
    pub default_doctor_port: u16,
}

impl AppState {
    pub fn new(config: DtokenConfig) -> Self {
        Self {
            config: Arc::new(config),
            default_doctor_port: 18001,
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats))
        .route("/stats-history", get(stats_history))
        .route("/metrics", get(metrics))
        .route("/providers", get(providers))
        .route("/routes", get(routes))
        .route("/instances", get(instances))
        .route("/chain/status", get(chain_status))
        .route("/route-decisions/recent", get(recent_route_decisions))
        .route("/logs/recent", get(recent_logs))
        .route("/doctor/run", post(doctor_run))
        .route("/preprocessors", get(preprocessors))
        .route("/rtk/status", get(rtk_status))
        .route("/rtk/gain", get(rtk_gain))
        .route("/rtk/discover", get(rtk_discover))
        .route("/rtk/session", get(rtk_session))
        .route("/headroom/status", get(headroom_status))
        .route("/headroom/proxy", get(headroom_proxy))
        .route("/headroom/mcp", get(headroom_mcp))
        .route("/headroom/stats", get(headroom_stats))
        .route("/headroom/cache-summary", get(headroom_cache_summary))
        .route("/projects", get(projects))
        .route("/projects/default-policy", get(project_default_policy))
        .route(
            "/projects/recommend-compression",
            post(project_recommend_compression),
        )
        .route("/config/patches/plan", post(config_patch_plan))
        .route("/config/rollback-points", get(config_rollback_points))
        .route("/events", get(events))
        .route("/compatibility/agents", get(agent_compatibility))
        .route("/protocol-injection", get(protocol_injection))
        .route("/install/scan", get(install_scan))
        .route("/install/plan", post(install_plan))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        component: "dtoken-core-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn stats() -> Json<Value> {
    let rtk = rtk_gain_mock().await;
    let headroom = headroom_stats_mock().await;
    let layered = LayeredSavingsReport {
        rtk_savings: rtk.saved_tokens,
        headroom_savings: headroom.tokens_saved,
        end_to_end_savings: rtk.saved_tokens + headroom.tokens_saved,
        attribution_confidence: "fixture_estimated".to_string(),
        note: "M1-M2 fixture attribution; production must use a unified baseline before treating this as exact".to_string(),
    };
    Json(json!({
        "status": "fixture",
        "requests": { "total": 0, "failed": 0 },
        "tokens": {
            "input": rtk.input_tokens,
            "output": rtk.output_tokens,
            "saved": layered.end_to_end_savings,
            "savings_percent": rtk.savings_percent
        },
        "layered": layered
    }))
}

async fn stats_history() -> Json<Value> {
    Json(headroom_stats_history_json())
}

async fn metrics() -> String {
    let headroom = headroom_metrics_report().await;
    format!(
        "# d-token fixture metrics\n\
dtoken_requests_total 0\n\
dtoken_rtk_saved_tokens {}\n\
dtoken_headroom_metrics_available {}\n\
{}",
        rtk_gain_mock().await.saved_tokens,
        if headroom.available { 1 } else { 0 },
        headroom.prometheus_text
    )
}

async fn providers(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.config.providers))
}

async fn routes(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.config.routes))
}

async fn instances(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.config.instances))
}

async fn chain_status() -> Json<Value> {
    Json(json!({ "status": "not_connected", "mode": "m1_placeholder" }))
}

async fn recent_route_decisions() -> Json<Value> {
    Json(json!([]))
}

async fn recent_logs() -> Json<Value> {
    Json(json!([]))
}

async fn doctor_run(State(state): State<AppState>) -> Json<Value> {
    let mut report = run_dev_no_touch_doctor();
    let port_report = run_basic_doctor(state.default_doctor_port);
    report.ok = report.ok && port_report.ok;
    report.checks.extend(port_report.checks);
    Json(json!(report))
}

async fn preprocessors() -> Json<Value> {
    Json(json!(list_preprocessors().await))
}

async fn rtk_status() -> Json<Value> {
    Json(json!(detect_rtk_status().await))
}

async fn rtk_gain() -> Json<Value> {
    Json(json!(rtk_gain_mock().await))
}

async fn rtk_discover() -> Json<Value> {
    Json(json!(rtk_discover_report().await))
}

async fn rtk_session() -> Json<Value> {
    Json(json!(rtk_session_report().await))
}

async fn headroom_status() -> Json<Value> {
    Json(json!(detect_headroom_status().await))
}

async fn headroom_proxy() -> Json<Value> {
    Json(json!({
        "running": true,
        "proxy_url": "http://127.0.0.1:8787",
        "source": "fixture"
    }))
}

async fn headroom_mcp() -> Json<Value> {
    Json(json!({
        "configured": true,
        "tools": ["headroom_compress", "headroom_retrieve", "headroom_stats"],
        "source": "fixture"
    }))
}

async fn headroom_stats() -> Json<Value> {
    Json(json!(headroom_stats_mock().await))
}

async fn headroom_cache_summary() -> Json<Value> {
    Json(json!(headroom_cache_summary_mock()))
}

async fn projects() -> Json<Value> {
    Json(json!(demo_projects()))
}

async fn project_default_policy() -> Json<Value> {
    Json(json!(default_policy("demo-project")))
}

async fn project_recommend_compression() -> Json<Value> {
    let engine = RuleBasedRecommendationEngine::default();
    let input = RecommendationInput {
        project_id: "demo-project".to_string(),
        language_stack: vec!["rust".to_string(), "typescript".to_string()],
        avg_request_tokens_7d: 20_000,
        avg_cli_noise_ratio_7d: 0.25,
        retrieve_rate_7d: 0.02,
        failure_rate_7d: 0.01,
        high_risk_task: false,
        manual_override: false,
    };
    Json(json!(engine.recommend(input)))
}

async fn config_patch_plan() -> Json<Value> {
    Json(json!(mock_patch_plan()))
}

async fn config_rollback_points() -> Json<Value> {
    Json(json!([]))
}

async fn events() -> Json<Value> {
    Json(json!(EventLedger::mock_m1().recent(20)))
}

async fn agent_compatibility() -> Json<Value> {
    Json(json!(agent_support_profiles()))
}

async fn protocol_injection() -> Json<Value> {
    Json(json!(protocol_injection_profiles()))
}

async fn install_scan() -> Json<Value> {
    Json(json!({
        "status": "m1_mock",
        "checks": [
            { "name": "dtoken-core", "ok": true },
            { "name": "rtk", "ok": false, "note": "detected dynamically via /rtk/status" },
            { "name": "headroom", "ok": false, "note": "detected dynamically via /headroom/status" }
        ]
    }))
}

fn agent_support_profiles() -> Vec<AgentSupportProfile> {
    use SupportLevel::{Full, None, Partial};

    vec![
        agent(
            "Claude Code",
            true,
            "PreToolUse shell hook",
            Full,
            true,
            "headroom wrap claude",
            Full,
            Partial,
            Full,
            "Native Windows RTK falls back to instructions; WSL gives full hook behavior.",
        ),
        agent(
            "VS Code Copilot Chat",
            true,
            "Copilot hook",
            Full,
            false,
            "not directly listed by Headroom wrap",
            Full,
            Partial,
            Full,
            "RTK supports VS Code Copilot hook; Headroom covers Copilot CLI/subscription path.",
        ),
        agent(
            "GitHub Copilot CLI",
            true,
            "Copilot preToolUse modifiedArgs",
            Full,
            true,
            "headroom wrap copilot",
            Full,
            Partial,
            Full,
            "Headroom can route Copilot subscription traffic through an OpenAI-compatible proxy.",
        ),
        agent(
            "Cursor",
            true,
            "preToolUse hook",
            Full,
            true,
            "manual Headroom proxy setup",
            Full,
            Partial,
            Full,
            "Headroom prints base URLs for Cursor settings.",
        ),
        agent(
            "Gemini CLI",
            true,
            "BeforeTool hook",
            Full,
            false,
            "shared memory/learn only",
            Full,
            Partial,
            Full,
            "Headroom README highlights shared memory with Gemini, not wrap support.",
        ),
        agent(
            "OpenCode",
            true,
            "TypeScript plugin",
            Full,
            true,
            "headroom wrap opencode",
            Full,
            Partial,
            Full,
            "Both projects support plugin/config injection.",
        ),
        agent(
            "OpenClaw",
            true,
            "TypeScript plugin",
            Full,
            true,
            "headroom wrap openclaw",
            Full,
            Partial,
            Full,
            "Headroom installs as ContextEngine plugin.",
        ),
        agent(
            "Pi",
            true,
            "TypeScript extension",
            Full,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK extension support only.",
        ),
        agent(
            "Hermes",
            true,
            "Python plugin",
            Full,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK plugin fails open if unavailable.",
        ),
        agent(
            "Cline / Roo Code",
            true,
            "rules file",
            Partial,
            true,
            "headroom wrap cline",
            Full,
            Partial,
            Full,
            "RTK rewrite is instruction-level; Headroom can inject config.",
        ),
        agent(
            "Windsurf",
            true,
            "rules file",
            Partial,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK prompt-level guidance.",
        ),
        agent(
            "Codex CLI",
            true,
            "AGENTS.md instructions",
            Partial,
            true,
            "headroom wrap codex",
            Full,
            Partial,
            Full,
            "Headroom shares memory with Claude; RTK is instruction-level.",
        ),
        agent(
            "Kilo Code",
            true,
            ".kilocode/rules",
            Partial,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK prompt-level guidance.",
        ),
        agent(
            "Google Antigravity",
            true,
            ".agents/rules",
            Partial,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK prompt-level guidance.",
        ),
        agent(
            "Mistral Vibe",
            true,
            "planned RTK support",
            None,
            true,
            "headroom wrap vibe",
            Partial,
            None,
            Partial,
            "RTK is blocked on upstream; Headroom lists Vibe wrap.",
        ),
        agent(
            "Aider",
            false,
            "not listed",
            None,
            true,
            "headroom wrap aider",
            Full,
            Partial,
            Full,
            "Headroom starts proxy and launches Aider.",
        ),
        agent(
            "Continue",
            false,
            "not listed",
            None,
            true,
            "headroom wrap continue",
            Full,
            Partial,
            Full,
            "Headroom injects config.",
        ),
        agent(
            "Goose",
            false,
            "not listed",
            None,
            true,
            "headroom wrap goose",
            Full,
            Partial,
            Full,
            "Headroom starts proxy and launches Goose.",
        ),
        agent(
            "OpenHands",
            false,
            "not listed",
            None,
            true,
            "headroom wrap openhands",
            Full,
            Partial,
            Full,
            "Headroom starts proxy and launches OpenHands.",
        ),
        agent(
            "Cortex Code",
            false,
            "not listed",
            None,
            true,
            "Headroom library only",
            Partial,
            Partial,
            Partial,
            "Headroom README lists library-only savings.",
        ),
    ]
}

fn agent(
    agent: &str,
    rtk_supported: bool,
    rtk_method: &str,
    rtk_transparent_rewrite: SupportLevel,
    headroom_supported: bool,
    headroom_method: &str,
    macos_support: SupportLevel,
    windows_native_support: SupportLevel,
    windows_wsl_support: SupportLevel,
    notes: &str,
) -> AgentSupportProfile {
    AgentSupportProfile {
        agent: agent.to_string(),
        rtk_supported,
        rtk_method: rtk_method.to_string(),
        rtk_transparent_rewrite,
        headroom_supported,
        headroom_method: headroom_method.to_string(),
        macos_support,
        windows_native_support,
        windows_wsl_support,
        notes: notes.to_string(),
    }
}

fn protocol_injection_profiles() -> Vec<ProtocolInjectionProfile> {
    vec![
        ProtocolInjectionProfile {
            protocol: Protocol::OpenAi,
            compatible_clients: vec![
                "OpenAI SDK".to_string(),
                "OpenAI-compatible clients".to_string(),
                "Vercel AI SDK".to_string(),
                "LiteLLM".to_string(),
                "Copilot subscription bridge".to_string(),
            ],
            injection_method: "Proxy/base-url injection for OpenAI-compatible chat or responses traffic".to_string(),
            development_default: "fixture-only; no live proxy or user service is contacted".to_string(),
            managed_mode_action: "install or patch client/provider route after diff, backup, confirmation, doctor, and rollback point".to_string(),
            safety_notes: vec![
                "Do not rewrite real provider config in development mode".to_string(),
                "Strip internal headers before upstream forwarding".to_string(),
                "Keep route decisions explainable in the event ledger".to_string(),
            ],
        },
        ProtocolInjectionProfile {
            protocol: Protocol::Anthropic,
            compatible_clients: vec![
                "Anthropic SDK".to_string(),
                "Claude Code compatible traffic".to_string(),
                "Anthropic-compatible proxy clients".to_string(),
                "Bedrock Anthropic shape".to_string(),
                "Vertex Anthropic publisher shape".to_string(),
            ],
            injection_method: "Proxy/base-url injection for Anthropic messages traffic with live-zone compression boundaries".to_string(),
            development_default: "fixture-only; no live proxy or user service is contacted".to_string(),
            managed_mode_action: "install or patch Anthropic route settings transactionally in M4".to_string(),
            safety_notes: vec![
                "Never compress human content blindly".to_string(),
                "Preserve cache hot-zone stability".to_string(),
                "CCR raw cache is summarized only by default".to_string(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_matrix_covers_rtk_and_headroom_union() {
        let agents = agent_support_profiles();
        for required in [
            "Claude Code",
            "Codex CLI",
            "Cursor",
            "Gemini CLI",
            "OpenCode",
            "OpenClaw",
            "Pi",
            "Hermes",
            "Cline / Roo Code",
            "Windsurf",
            "Kilo Code",
            "Google Antigravity",
            "Aider",
            "Continue",
            "Goose",
            "OpenHands",
            "Cortex Code",
        ] {
            assert!(
                agents.iter().any(|agent| agent.agent == required),
                "missing {required}"
            );
        }
    }

    #[test]
    fn protocol_injection_covers_openai_and_anthropic() {
        let profiles = protocol_injection_profiles();
        assert!(profiles
            .iter()
            .any(|profile| profile.protocol == Protocol::OpenAi));
        assert!(profiles
            .iter()
            .any(|profile| profile.protocol == Protocol::Anthropic));
    }
}

async fn install_plan() -> Json<Value> {
    Json(json!({
        "status": "m1_mock",
        "transaction": {
            "id": "m1-install-plan",
            "steps": ["scan", "select_tools", "show_diff", "backup", "apply", "doctor"],
            "requires_user_confirmation": true
        }
    }))
}
