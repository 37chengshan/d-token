#!/usr/bin/env python3
"""Check required d-token repository structure.

This script is intentionally small and dependency-free so it can run in CI on
Windows, macOS, and Linux.
"""

from __future__ import annotations

from pathlib import Path
import sys

REQUIRED_PATHS = [
    "README.md",
    "docs/prd/d-token-prd-v0.1.md",
    "docs/architecture/system-architecture.md",
    "docs/decisions/ADR-0001-product-scope.md",
    "docs/research/open-source-reference.md",
    "apps/desktop/README.md",
    "crates/dtoken-proxy/README.md",
    "crates/dtoken-router/README.md",
    "crates/dtoken-compress/README.md",
    "packages/sdk/README.md",
    "sidecars/cc-switch-bridge/README.md",
    "configs/providers.example.yaml",
    "scripts/dev/README.md",
]


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    missing = []

    for rel_path in REQUIRED_PATHS:
        path = root / rel_path
        if not path.exists():
            missing.append(rel_path)

    if missing:
        print("Missing required paths:")
        for item in missing:
            print(f"- {item}")
        return 1

    print("d-token repository structure check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
