use dtoken_types::{
    OpticalGlobalPolicy, OpticalMode, OpticalPlanPublicSummary, OpticalPlanStatus,
    RouteOpticalPolicy,
};

#[test]
fn optical_global_policy_defaults_to_safe_shadow_only() {
    let policy = OpticalGlobalPolicy::default();

    assert!(!policy.enabled);
    assert_eq!(policy.default_mode, OpticalMode::ShadowPreview);
    assert!(!policy.allow_active);
    assert!(policy.public_summary_only);
    assert!(policy.max_images_per_request > 0);
}

#[test]
fn route_optical_policy_defaults_to_off() {
    let policy = RouteOpticalPolicy::default();

    assert!(!policy.enabled);
    assert_eq!(policy.mode, OpticalMode::Off);
    assert!(!policy.allow_tool_result);
    assert!(!policy.confirmed_experimental_lossy);
}

#[test]
fn public_summary_serializes_without_raw_content_fields() {
    let summary = OpticalPlanPublicSummary {
        plan_id: "plan-1".to_string(),
        mode: OpticalMode::ShadowPreview,
        final_status: OpticalPlanStatus::ValidatedShadowPlan,
        gates: Vec::new(),
        counters: Default::default(),
        buckets: Vec::new(),
        cache: Default::default(),
        economics: Default::default(),
        warnings: vec!["estimated only".to_string()],
    };

    let encoded = serde_json::to_string(&summary).expect("summary should serialize");

    assert!(encoded.contains("plan-1"));
    assert!(!encoded.contains("raw_prompt"));
    assert!(!encoded.contains("request_body"));
    assert!(!encoded.contains("rendered_text"));
    assert!(!encoded.contains("base64"));
}
