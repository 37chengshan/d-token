from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

REQUIRED = [
    'README.md',
    'AGENTS.md',
    'CLAUDE.md',
    'docs/PRD.md',
    'docs/TECH_STACK.md',
    'docs/ARCHITECTURE.md',
    'docs/SECURITY_BOUNDARIES.md',
    'docs/FINAL_REVIEW.md',
    'docs/M1_STATUS.md',
    'code/README.md',
    'code/Cargo.toml',
    'code/crates/dtoken-types/src/lib.rs',
    'code/crates/dtoken-config/src/lib.rs',
    'code/crates/dtoken-router/src/lib.rs',
    'code/crates/dtoken-compress/src/lib.rs',
    'code/crates/dtoken-process/src/lib.rs',
    'code/crates/dtoken-doctor/src/lib.rs',
    'code/crates/dtoken-core-api/src/lib.rs',
    'code/crates/dtoken-proxy/src/main.rs',
    'code/configs/config.example.json',
    'support/README.md',
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
