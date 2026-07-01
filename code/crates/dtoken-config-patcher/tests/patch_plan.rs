use dtoken_config_patcher::{mock_patch_plan, simple_diff, stable_hash};
use dtoken_types::{ConfigToolKind, RiskLevel};

#[test]
fn stable_hash_is_deterministic() {
    assert_eq!(stable_hash("abc"), stable_hash("abc"));
}

#[test]
fn diff_reports_size_change() {
    let diff = simple_diff("{}", "{\"enabled\":true}");
    assert!(diff.contains("before"));
    assert!(diff.contains("after"));
}

#[test]
fn mock_patch_plan_has_safe_defaults() {
    let plan = mock_patch_plan();
    assert_eq!(plan.tool, ConfigToolKind::Dtoken);
    assert_eq!(plan.risk_level, RiskLevel::Low);
}
