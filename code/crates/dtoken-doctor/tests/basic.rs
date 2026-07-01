use dtoken_doctor::run_basic_doctor;

#[test]
fn doctor_returns_checks() {
    let report = run_basic_doctor(0);
    assert!(!report.checks.is_empty());
    assert!(report.checks.iter().any(|check| check.name == "local-only-bind"));
    assert!(report.checks.iter().any(|check| check.name == "port-available"));
}
