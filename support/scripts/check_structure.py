from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

REQUIRED = [
    'AGENTS.md',
    'CLAUDE.md',
    'docs/PRD.md',
    'docs/TECH_STACK.md',
    'docs/ARCHITECTURE.md',
    'docs/SECURITY_BOUNDARIES.md',
    'docs/FINAL_REVIEW.md',
    'docs/M1_STATUS.md',
    'docs/M1_REVIEW.md',
    'docs/M1_FINAL_REVIEW.md',
    'docs/TESTING_NO_TOUCH.md',
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
    'support/scripts/run_fixture_tests.sh',
    'support/scripts/run_fake_binary_tests.sh',
]

FORBIDDEN = [
    'docs/prd/d-token-prd-v0.1.md',
    'docs/prd/d-token-prd-v0.2.md',
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

if missing or forbidden_present:
    if missing:
        print('Missing required files:')
        for p in missing:
            print(f'  - {p}')
    if forbidden_present:
        print('Forbidden legacy files still present:')
        for p in forbidden_present:
            print(f'  - {p}')
    raise SystemExit(1)

print('Repository structure check passed.')
