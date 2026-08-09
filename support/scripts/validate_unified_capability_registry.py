#!/usr/bin/env python3
"""Contract validator for the unified capability registry (T-C0D / PR-C0C).

MUST (docs/releases/v0.0.1 QUALITY-1 / U1 / T-C0D):
  * exactly 117 unique entry ids;
  * every entry carries id/source_capability/owner_crate/dispatch_symbol/
    tests(normal/error/recovery)/real_evidence/gates/status complete and
    non-empty;
  * status is only implemented_unverified or blocked before N0 (never
    verified, never placeholder/missing/not-in-scope);
  * owner_crate must be a workspace crate listed in the registry header.

Run:  python3 support/scripts/validate_unified_capability_registry.py
Exit 0 on success, 1 with a report on any contract violation (fail-closed).
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
REGISTRY = REPO_ROOT / "support" / "fixtures" / "unified-capability-registry-v1.json"

REQUIRED_ENTRY_KEYS = [
    "id",
    "source_capability",
    "owner_crate",
    "dispatch_symbol",
    "tests",
    "real_evidence",
    "gates",
    "status",
]
REQUIRED_TEST_KEYS = ["normal", "error", "recovery"]
ALLOWED_STATUSES = {"implemented_unverified", "blocked"}
EXPECTED_COUNT = 117
FORBIDDEN_TOKENS = ("placeholder", "missing", "not-in-scope", "TBD", "TODO", "XXX", "lorem")


def validate(registry_path: Path) -> list[str]:
    errors: list[str] = []
    if not registry_path.exists():
        return [f"registry file missing: {registry_path}"]

    try:
        data = json.loads(registry_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return [f"registry is not valid JSON: {exc}"]

    owner_crates = set(data.get("owner_crates", []))
    entries = data.get("entries", [])
    if len(entries) != EXPECTED_COUNT:
        errors.append(f"expected exactly {EXPECTED_COUNT} entries, got {len(entries)}")

    ids = [entry.get("id") for entry in entries]
    duplicates = {item for item in ids if ids.count(item) > 1}
    if duplicates:
        errors.append(f"duplicate entry ids: {sorted(duplicates)}")

    for index, entry in enumerate(entries):
        entry_id = entry.get("id", f"<entry {index}>")
        for key in REQUIRED_ENTRY_KEYS:
            value = entry.get(key)
            if value is None or value == "" or value == []:
                errors.append(f"{entry_id}: missing or empty field '{key}'")
        status = entry.get("status")
        if status not in ALLOWED_STATUSES:
            errors.append(
                f"{entry_id}: status '{status}' not allowed before N0 "
                f"(only {sorted(ALLOWED_STATUSES)})"
            )
        owner = entry.get("owner_crate")
        if owner and owner not in owner_crates:
            errors.append(f"{entry_id}: owner_crate '{owner}' is not a workspace crate")
        tests = entry.get("tests")
        if isinstance(tests, dict):
            for key in REQUIRED_TEST_KEYS:
                values = tests.get(key)
                if not isinstance(values, list) or not values:
                    errors.append(f"{entry_id}: tests.{key} must be a non-empty list")
        for field in ("id", "source_capability", "dispatch_symbol", "owner_crate"):
            text = entry.get(field)
            if isinstance(text, str) and any(
                token.lower() in text.lower() for token in FORBIDDEN_TOKENS
            ):
                errors.append(f"{entry_id}: field '{field}' contains a forbidden token")

    return errors


def main() -> int:
    registry_path = Path(sys.argv[1]) if len(sys.argv) > 1 else REGISTRY
    errors = validate(registry_path)
    if errors:
        print(f"registry contract FAILED ({len(errors)} violations):")
        for error in errors:
            print(f"  - {error}")
        return 1
    print(f"registry contract OK: exactly {EXPECTED_COUNT} unique complete entries")
    return 0


if __name__ == "__main__":
    sys.exit(main())
