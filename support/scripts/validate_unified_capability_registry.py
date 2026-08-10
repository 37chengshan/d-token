#!/usr/bin/env python3
"""Contract validator for the unified capability registry (T-C0D / PR-C0C / C8).

MUST (docs/releases/v0.0.1 QUALITY-1 / U1 / T-C0D, C8 hardening):
  * exactly 117 unique entry ids;
  * every entry carries id/source_capability/owner_crate/dispatch_symbol/
    tests(normal/error/recovery)/implementation_refs/real_evidence/gates/
    producer/status complete and non-empty;
  * status is only implemented_unverified or blocked before N0 (never
    verified, never placeholder/missing/not-in-scope);
  * owner_crate must be a workspace owner listed in the registry header;
  * dispatch_symbol must resolve through the tracked symbol source index
    (support/scripts/symbol_source_index_v1.json): resolved symbols must
    match their declared regexes in the tracked declaration file; pending
    symbols must carry a documented owning slice;
  * test paths must be actual test targets (test-bearing files: #[test] /
    #[tokio::test] / proptest! / #[cfg(test)] unit modules / def test_* /
    Pester-less PS1 assertions) with normal/error/recovery case evidence;
  * gates must belong to the registry gate vocabulary;
  * producer must be an entry-producer:<evidence-node> role in the header;
  * real_evidence must list ONLY fixed runtime artifact names for the entry's
    producer node (docs/source existence is implementation reference, never
    runtime evidence — EVIDENCE.md §3/§4).

Run:  python3 support/scripts/validate_unified_capability_registry.py
Exit 0 on success, 1 with a report on any contract violation (fail-closed).
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
REGISTRY = REPO_ROOT / "support" / "fixtures" / "unified-capability-registry-v1.json"
SYMBOL_INDEX = REPO_ROOT / "support" / "scripts" / "symbol_source_index_v1.json"
TRACKED_PATHS: set[str] | None = None

# Fixed runtime artifact names per evidence node (EVIDENCE.md §3).  These are
# the ONLY values allowed in an entry's real_evidence; anything else (docs,
# sources, implementation files, arbitrary paths) is not runtime evidence.
FIXED_ARTIFACT_NAMES: dict[str, list[str]] = {
    "N0": ["N0-preflight.json", "N0-provenance.json", "source-manifest.json"],
    "N1": ["N1-receipt.json"],
    "N2": ["N2-codex.json"],
    "N3": ["N3-opencode.json"],
    "N4": ["N4-config.json"],
    "N5": ["N5-cc-switch.json"],
    "N6": ["N6-ui.json"],
    "N7": ["N7-macos.json"],
    "N8": ["N8-windows.json"],
    "N9": ["N9-release-status.json"],
    "N10": ["N10-unified-proof.json"],
    "N10-U0": ["N10-U0-contract.json"],
    "N10-U1": ["N10-U1-registry.json"],
    "N10-U2": ["N10-U2-property-fuzz.json"],
    "N10-U3": ["N10-U3-fault.json"],
    "N10-U4": ["N10-U4-replay-quality.json"],
    "N10-U5": ["N10-U5-perf.json"],
    "N10-U6": ["N10-U6-provider-paired.json"],
    "N10-U7": ["N10-U7-optical-active.json"],
    "N10-U8": ["N10-U8-platform.json"],
    "N10-U9": ["N10-U9-coverage.json"],
}

PRODUCER_RE = re.compile(r"^entry-producer:(N0|N[1-8]|N9|N10|N10-U[0-9])$")


def tracked_paths() -> set[str]:
    global TRACKED_PATHS
    if TRACKED_PATHS is None:
        result = subprocess.run(
            ["git", "ls-files", "-z"],
            cwd=REPO_ROOT,
            check=False,
            capture_output=True,
        )
        if result.returncode != 0:
            return set()
        TRACKED_PATHS = {
            path for path in result.stdout.decode("utf-8").split("\0") if path
        }
    return TRACKED_PATHS


def validate_repo_path(
    path_value: object,
    field: str,
    entry_id: str,
    errors: list[str],
    *,
    must_be_test: bool = False,
) -> None:
    if not isinstance(path_value, str) or not path_value:
        errors.append(f"{entry_id}: {field} path must be a non-empty string")
        return
    path = Path(path_value)
    if path.is_absolute() or ".." in path.parts:
        errors.append(f"{entry_id}: {field} path must be repo-relative: {path_value}")
        return
    if path_value not in tracked_paths() or not (REPO_ROOT / path).is_file():
        errors.append(
            f"{entry_id}: {field} path does not resolve to a tracked file: {path_value}"
        )
        return
    if must_be_test and not is_test_bearing(path_value):
        errors.append(
            f"{entry_id}: {field} path is not an actual test target "
            f"(no #[test]/#[tokio::test]/proptest!/#[cfg(test)]/def test_*): {path_value}"
        )


def is_test_bearing(rel_path: str) -> bool:
    """Heuristic: the file is a real test target with executable cases."""
    text = (REPO_ROOT / rel_path).read_text(encoding="utf-8", errors="replace")
    if rel_path.endswith(".rs"):
        return bool(
            re.search(r"#\[(tokio::)?test\]|proptest!", text)
            or re.search(r"#\[cfg\(test\)\]\s*\n(?:[^\n]*\n){0,3}mod tests", text)
        )
    if rel_path.endswith(".py"):
        return bool(re.search(r"def test_[a-zA-Z0-9_]*\s*\(", text) or "unittest" in text)
    if rel_path.endswith(".sh"):
        return bool(re.search(r"set -e", text) and re.search(r"\bfail\(\)|assert", text))
    if rel_path.endswith(".ps1"):
        return bool(
            re.search(r"\b(Describe|function Test-|Pester|Assert)\b", text)
            or "cargo test" in text
        )
    return False


REQUIRED_ENTRY_KEYS = [
    "id",
    "source_capability",
    "owner_crate",
    "dispatch_symbol",
    "tests",
    "implementation_refs",
    "real_evidence",
    "gates",
    "producer",
    "status",
]
REQUIRED_TEST_KEYS = ["normal", "error", "recovery"]
ALLOWED_STATUSES = {"implemented_unverified", "blocked"}
EXPECTED_COUNT = 117
FORBIDDEN_TOKENS = ("placeholder", "missing", "not-in-scope", "TBD", "TODO", "XXX", "lorem")


def load_symbol_index() -> dict:
    try:
        data = json.loads(SYMBOL_INDEX.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    symbols = data.get("symbols")
    return symbols if isinstance(symbols, dict) else {}


def validate_dispatch_symbol(
    entry_id: str, symbol: object, index: dict, errors: list[str]
) -> None:
    if not isinstance(symbol, str) or not symbol:
        errors.append(f"{entry_id}: dispatch_symbol must be a non-empty string")
        return
    entry = index.get(symbol)
    if entry is None:
        errors.append(
            f"{entry_id}: dispatch_symbol '{symbol}' is absent from the tracked "
            f"symbol source index (support/scripts/symbol_source_index_v1.json)"
        )
        return
    if entry.get("status") == "resolved":
        rel_file = entry.get("file")
        if not isinstance(rel_file, str) or rel_file not in tracked_paths():
            errors.append(
                f"{entry_id}: dispatch_symbol '{symbol}' index file is not tracked: {rel_file}"
            )
            return
        text = (REPO_ROOT / rel_file).read_text(encoding="utf-8", errors="replace")
        regexes = entry.get("regexes") or []
        if not regexes:
            errors.append(
                f"{entry_id}: dispatch_symbol '{symbol}' index entry has no declaration regexes"
            )
            return
        for regex in regexes:
            try:
                ok = re.search(regex, text) is not None
            except re.error:
                ok = False
            if not ok:
                errors.append(
                    f"{entry_id}: dispatch_symbol '{symbol}' declaration regex no longer "
                    f"matches {rel_file}: {regex!r}"
                )
    elif entry.get("status") == "pending":
        owner = entry.get("owner")
        if not isinstance(owner, str) or not owner:
            errors.append(
                f"{entry_id}: dispatch_symbol '{symbol}' is pending but has no owning slice"
            )
        expected_file = entry.get("file")
        if expected_file and (
            expected_file not in tracked_paths()
            or not (REPO_ROOT / expected_file).is_file()
        ):
            errors.append(
                f"{entry_id}: dispatch_symbol '{symbol}' pending expected file is not "
                f"tracked: {expected_file}"
            )
    else:
        errors.append(
            f"{entry_id}: dispatch_symbol '{symbol}' index entry has invalid status "
            f"{entry.get('status')!r}"
        )


def validate(registry_path: Path) -> list[str]:
    errors: list[str] = []
    if not registry_path.exists():
        return [f"registry file missing: {registry_path}"]

    try:
        data = json.loads(registry_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return [f"registry is not valid JSON: {exc}"]

    owner_crates = set(data.get("owner_crates", []))
    evidence_nodes = set(data.get("evidence_nodes", []))
    gate_vocabulary = set(data.get("gate_vocabulary", []))
    if not gate_vocabulary:
        errors.append("registry header is missing gate_vocabulary")
    if not evidence_nodes:
        errors.append("registry header is missing evidence_nodes")
    index = load_symbol_index()
    if not index:
        errors.append("symbol source index missing or unreadable")

    entries = data.get("entries", [])
    if len(entries) != EXPECTED_COUNT:
        errors.append(f"expected exactly {EXPECTED_COUNT} entries, got {len(entries)}")

    ids = [entry.get("id") for entry in entries]
    duplicates = {item for item in ids if ids.count(item) > 1}
    if duplicates:
        errors.append(f"duplicate entry ids: {sorted(duplicates)}")

    for index_pos, entry in enumerate(entries):
        entry_id = entry.get("id", f"<entry {index_pos}>")
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
            errors.append(f"{entry_id}: owner_crate '{owner}' is not a workspace owner")

        validate_dispatch_symbol(entry_id, entry.get("dispatch_symbol"), index, errors)

        tests = entry.get("tests")
        if isinstance(tests, dict):
            for key in REQUIRED_TEST_KEYS:
                values = tests.get(key)
                if not isinstance(values, list) or not values:
                    errors.append(f"{entry_id}: tests.{key} must be a non-empty list")
                else:
                    for path_value in values:
                        validate_repo_path(
                            path_value,
                            f"tests.{key}",
                            entry_id,
                            errors,
                            must_be_test=True,
                        )
        impl_refs = entry.get("implementation_refs")
        if isinstance(impl_refs, list):
            for path_value in impl_refs:
                validate_repo_path(path_value, "implementation_refs", entry_id, errors)

        producer = entry.get("producer")
        if isinstance(producer, str):
            if not PRODUCER_RE.match(producer):
                errors.append(
                    f"{entry_id}: producer '{producer}' is not an entry-producer:<node> role"
                )
            else:
                node = producer.split(":", 1)[1]
                if node not in evidence_nodes:
                    errors.append(f"{entry_id}: producer node '{node}' is not an evidence node")
                evidence = entry.get("real_evidence")
                if isinstance(evidence, list) and node in FIXED_ARTIFACT_NAMES:
                    allowed = set(FIXED_ARTIFACT_NAMES[node])
                    for path_value in evidence:
                        if not isinstance(path_value, str) or path_value in ("", "/"):
                            errors.append(
                                f"{entry_id}: real_evidence entry must be a non-empty string"
                            )
                            continue
                        if "/" in path_value or "\\" in path_value:
                            errors.append(
                                f"{entry_id}: real_evidence '{path_value}' is a repo path; "
                                f"runtime evidence must be a fixed artifact name for node "
                                f"{node} (EVIDENCE.md §4)"
                            )
                        elif path_value not in allowed:
                            errors.append(
                                f"{entry_id}: real_evidence '{path_value}' is not a fixed "
                                f"artifact name for producer node {node} "
                                f"(allowed: {sorted(allowed)})"
                            )

        gates = entry.get("gates")
        if isinstance(gates, list):
            for gate in gates:
                if gate not in gate_vocabulary:
                    errors.append(f"{entry_id}: gate '{gate}' is not in the gate vocabulary")

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
