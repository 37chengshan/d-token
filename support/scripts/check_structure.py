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
    'code/README.md',
    'support/README.md',
]

FORBIDDEN = [
    'README.md',
    'docs/prd/d-token-prd-v0.1.md',
    'docs/prd/d-token-prd-v0.2.md',
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
