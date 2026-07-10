from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

REQUIRED = [
    'AGENTS.md',
    'CLAUDE.md',
    'docs/PRD.md',
    'docs/PLAN.md',
    'docs/index.md',
    'docs/DOCS_ORGANIZATION.md',
    'docs/TECH_STACK.md',
    'docs/ARCHITECTURE.md',
    'docs/SECURITY_BOUNDARIES.md',
    'docs/APP_FUNCTIONS_AND_PAGES.md',
    'docs/history/FINAL_REVIEW.md',
    'docs/history/M1_STATUS.md',
    'docs/history/M1_REVIEW.md',
    'docs/history/M1_FINAL_REVIEW.md',
    'docs/testing/TESTING_NO_TOUCH.md',
    'docs/history/M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md',
    'docs/history/M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md',
    'docs/history/M3_GUI_CONTROL_PLANE_MVP_REPORT.md',
    'docs/history/M1_M3_DELIVERY_AUDIT.md',
    'docs/history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md',
    'docs/history/M4_OPENCODE_TEST_RESULTS_2026-07-04.md',
    'docs/history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md',
    'docs/history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md',
    'docs/history/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md',
    'docs/product/UI_CAPABILITY_MATRIX.md',
    'docs/product/RUNTIME_CAPABILITY_MATRIX.md',
    'docs/release/RELEASE_COMPLETION_PLAN.md',
    'docs/release/RELEASE_READINESS_REPORT.md',
    'docs/release/release-plan-2026-07/README.md',
    'docs/testing/OPENCODE_LOCAL_TEST_PLAN.md',
    'docs/experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md',
    'docs/experiments/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md',
    'docs/notes/NOTES.md',
    'docs/notes/tech-notes.md',
    'code/README.md',
    'code/Cargo.toml',
    'code/crates/dtoken-types/src/lib.rs',
    'code/crates/dtoken-config/src/lib.rs',
    'code/crates/dtoken-router/src/lib.rs',
    'code/crates/dtoken-compress/src/lib.rs',
    'code/crates/dtoken-process/src/lib.rs',
    'code/crates/dtoken-doctor/src/lib.rs',
    'code/crates/dtoken-event-ledger/src/lib.rs',
    'code/crates/dtoken-preprocessor/src/lib.rs',
    'code/crates/dtoken-headroom-adapter/src/lib.rs',
    'code/crates/dtoken-project-policy/src/lib.rs',
    'code/crates/dtoken-config-patcher/src/lib.rs',
    'code/crates/dtoken-core-api/src/lib.rs',
    'code/crates/dtoken-proxy/src/main.rs',
    'code/configs/config.example.json',
    'support/README.md',
    'support/fixtures/README.md',
    'support/fixtures/rtk/version.txt',
    'support/fixtures/rtk/gain-all.json',
    'support/fixtures/rtk/discover.txt',
    'support/fixtures/rtk/session.txt',
    'support/fixtures/headroom/version.txt',
    'support/fixtures/headroom/stats.json',
    'support/fixtures/headroom/stats-history.json',
    'support/fixtures/headroom/metrics.txt',
    'support/fixtures/headroom/doctor.txt',
    'support/fake-bin/rtk',
    'support/fake-bin/headroom',
    'support/fake-bin/rtk.cmd',
    'support/fake-bin/headroom.cmd',
    'support/scripts/run_fixture_tests.sh',
    'support/scripts/run_fixture_tests.ps1',
    'support/scripts/run_fake_binary_tests.sh',
]

FORBIDDEN = [
    'docs/prd/d-token-prd-v0.1.md',
    'docs/prd/d-token-prd-v0.2.md',
    'docs/FINAL_REVIEW.md',
    'docs/M1_STATUS.md',
    'docs/M1_REVIEW.md',
    'docs/M1_FINAL_REVIEW.md',
    'docs/TESTING_NO_TOUCH.md',
    'docs/M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md',
    'docs/M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md',
    'docs/M3_GUI_CONTROL_PLANE_MVP_REPORT.md',
    'docs/M1_M3_DELIVERY_AUDIT.md',
    'docs/M4_REAL_CAPABILITY_OPENCODE_PLAN.md',
    'docs/M4_OPENCODE_TEST_RESULTS_2026-07-04.md',
    'docs/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md',
    'docs/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md',
    'docs/UI_CAPABILITY_MATRIX.md',
    'docs/RUNTIME_CAPABILITY_MATRIX.md',
    'docs/RELEASE_COMPLETION_PLAN.md',
    'docs/RELEASE_READINESS_REPORT.md',
    'docs/release-plan-2026-07',
    'docs/OPENCODE_LOCAL_TEST_PLAN.md',
    'docs/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md',
    'docs/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md',
    'docs/NOTES.md',
    'docs/tech-notes.md',
    'apps/desktop/README.md',
    'crates/dtoken-proxy/README.md',
    'crates/dtoken-router/README.md',
    'crates/dtoken-compress/README.md',
    'packages/sdk/README.md',
    'sidecars/cc-switch-bridge/README.md',
    'configs/providers.example.yaml',
    'scripts/check_structure.py',
    'scripts/dev/README.md',
]

missing = [p for p in REQUIRED if not (ROOT / p).exists()]
forbidden_present = [p for p in FORBIDDEN if (ROOT / p).exists()]
prd_sources = sorted(
    p.relative_to(ROOT).as_posix()
    for p in (ROOT / 'docs').rglob('*.md')
    if p.name.casefold() == 'prd.md' or p.name.casefold().startswith('d-token-prd-v')
)
unexpected_prd_sources = [p for p in prd_sources if p != 'docs/PRD.md']

if missing or forbidden_present or unexpected_prd_sources:
    if missing:
        print('Missing required files:')
        for p in missing:
            print(f'  - {p}')
    if forbidden_present:
        print('Forbidden legacy files still present:')
        for p in forbidden_present:
            print(f'  - {p}')
    if unexpected_prd_sources:
        print('Unexpected parallel PRD files present:')
        for p in unexpected_prd_sources:
            print(f'  - {p}')
    raise SystemExit(1)

print('Repository structure check passed.')
