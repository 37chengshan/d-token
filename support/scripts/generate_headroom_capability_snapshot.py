#!/usr/bin/env python3
"""Convert a native CapabilityRegistrySnapshot into the release audit wrapper.

The converter is deliberately offline and does not infer capability state. It
selects the aggregate release capabilities from a native `/capabilities`
artifact, preserves their native evidence metadata, and binds the wrapper to
an explicit release candidate id.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


MAX_INPUT_BYTES = 1024 * 1024
WRAPPER_SCHEMA_VERSION = 1
NATIVE_TOP_LEVEL_FIELDS = {"schema_version", "revision", "collected_at_ms", "capabilities"}
NATIVE_CAPABILITY_FIELDS = {
    "id",
    "state",
    "implementation_revision",
    "config_revision",
    "evidence_revision",
    "freshness",
    "reason_codes",
    "last_verified_at_ms",
}
REQUIRED_CAPABILITY_IDS = (
    "compression",
    "ccr",
    "memory_learn",
    "output_shape",
    "integrated_rtk",
    "proxy_sdk_mcp",
    "stats_history_metrics",
    "routing_protocols",
    "doctor_dashboard_logs_export",
    "project_policy",
    "backup_rollback",
    "optical_active_transform",
)
ALLOWED_STATES = {"ready", "degraded", "unavailable", "blocked", "implemented_disabled"}
ALLOWED_FRESHNESS = {"live", "recent", "stale", "unknown"}
VERIFIED_STATES = {"ready", "degraded", "implemented_disabled"}
ID_PATTERN = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")
METADATA_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:/+-]{0,127}$")
REASON_PATTERN = re.compile(r"^[a-z0-9][a-z0-9._-]{0,63}$")
CANDIDATE_PATTERN = re.compile(r"^[0-9a-f]{64}$")


class ContractError(ValueError):
    """Raised when native registry evidence cannot safely enter the release audit."""


def _is_integer(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _require_exact_fields(value: dict[str, Any], expected: set[str], label: str) -> None:
    missing = sorted(expected - set(value))
    unknown = sorted(set(value) - expected)
    if missing:
        raise ContractError(f"{label} is missing required fields: {', '.join(missing)}")
    if unknown:
        raise ContractError(f"{label} contains unknown fields")


def _require_optional_metadata(value: object, label: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str) or not METADATA_PATTERN.fullmatch(value):
        raise ContractError(f"{label} must be null or a bounded metadata revision")
    return value


def _validate_native_capability(value: object, index: int) -> dict[str, Any]:
    label = f"capabilities[{index}]"
    if not isinstance(value, dict):
        raise ContractError(f"{label} must be an object")
    _require_exact_fields(value, NATIVE_CAPABILITY_FIELDS, label)

    capability_id = value["id"]
    if not isinstance(capability_id, str) or not ID_PATTERN.fullmatch(capability_id):
        raise ContractError(f"{label}.id must be a canonical capability id")

    state = value["state"]
    if not isinstance(state, str) or state not in ALLOWED_STATES:
        raise ContractError(f"{label}.state is unknown")

    freshness = value["freshness"]
    if not isinstance(freshness, str) or freshness not in ALLOWED_FRESHNESS:
        raise ContractError(f"{label}.freshness is unknown")

    implementation_revision = value["implementation_revision"]
    if not isinstance(implementation_revision, str) or not METADATA_PATTERN.fullmatch(
        implementation_revision
    ):
        raise ContractError(f"{label}.implementation_revision is invalid")

    config_revision = _require_optional_metadata(value["config_revision"], f"{label}.config_revision")
    evidence_revision = _require_optional_metadata(
        value["evidence_revision"], f"{label}.evidence_revision"
    )

    reason_codes = value["reason_codes"]
    if (
        not isinstance(reason_codes, list)
        or not reason_codes
        or any(not isinstance(code, str) or not REASON_PATTERN.fullmatch(code) for code in reason_codes)
        or len(set(reason_codes)) != len(reason_codes)
    ):
        raise ContractError(f"{label}.reason_codes must be unique bounded metadata codes")

    last_verified_at_ms = value["last_verified_at_ms"]
    if last_verified_at_ms is not None and (
        not _is_integer(last_verified_at_ms) or last_verified_at_ms <= 0
    ):
        raise ContractError(f"{label}.last_verified_at_ms must be null or a positive integer")

    if state in VERIFIED_STATES:
        if evidence_revision is None:
            raise ContractError(f"{label}.evidence_revision is required for state {state}")
        if last_verified_at_ms is None:
            raise ContractError(f"{label}.last_verified_at_ms is required for state {state}")
        if freshness == "unknown":
            raise ContractError(f"{label}.freshness cannot be unknown for state {state}")

    return {
        "id": capability_id,
        "state": state,
        "implementation_revision": implementation_revision,
        "config_revision": config_revision,
        "evidence_revision": evidence_revision,
        "freshness": freshness,
        "reason_codes": list(reason_codes),
        "last_verified_at_ms": last_verified_at_ms,
    }


def generate_snapshot(native_registry: object, candidate_id: str) -> dict[str, Any]:
    if not CANDIDATE_PATTERN.fullmatch(candidate_id):
        raise ContractError("candidate_id must be exactly 64 lowercase hexadecimal characters")
    if not isinstance(native_registry, dict):
        raise ContractError("native registry must be a JSON object")
    _require_exact_fields(native_registry, NATIVE_TOP_LEVEL_FIELDS, "native registry")

    schema_version = native_registry["schema_version"]
    if not _is_integer(schema_version) or schema_version != 1:
        raise ContractError("native registry schema_version must be 1")

    registry_revision = native_registry["revision"]
    if not isinstance(registry_revision, str) or not METADATA_PATTERN.fullmatch(registry_revision):
        raise ContractError("native registry revision must be a bounded metadata revision")

    collected_at_ms = native_registry["collected_at_ms"]
    if not _is_integer(collected_at_ms) or collected_at_ms <= 0:
        raise ContractError("native registry collected_at_ms must be a positive integer")
    try:
        collected_at = datetime.fromtimestamp(collected_at_ms / 1000, tz=timezone.utc)
    except (OverflowError, OSError, ValueError) as error:
        raise ContractError("native registry collected_at_ms is outside the supported range") from error

    capability_values = native_registry["capabilities"]
    if not isinstance(capability_values, list) or not capability_values:
        raise ContractError("native registry capabilities must be a non-empty array")

    by_id: dict[str, dict[str, Any]] = {}
    for index, value in enumerate(capability_values):
        capability = _validate_native_capability(value, index)
        capability_id = capability["id"]
        if capability_id in by_id:
            raise ContractError(f"native registry contains duplicate capability id: {capability_id}")
        by_id[capability_id] = capability

    missing_ids = [capability_id for capability_id in REQUIRED_CAPABILITY_IDS if capability_id not in by_id]
    if missing_ids:
        raise ContractError(f"native registry is missing aggregate capability ids: {', '.join(missing_ids)}")

    wrapped_capabilities = []
    for capability_id in REQUIRED_CAPABILITY_IDS:
        native = by_id[capability_id]
        wrapped_capabilities.append(
            {
                "id": capability_id,
                "state": native["state"],
                "implementation_revision": native["implementation_revision"],
                "source_evidence_revision": native["evidence_revision"],
                "evidence_revision": candidate_id,
                "freshness": native["freshness"],
                "reason_codes": list(native["reason_codes"]),
            }
        )

    return {
        "schema_version": WRAPPER_SCHEMA_VERSION,
        "source": "dtoken-owned-runtime",
        "revision": candidate_id,
        "source_registry_revision": registry_revision,
        "collected_at": collected_at.isoformat(timespec="milliseconds").replace("+00:00", "Z"),
        "capabilities": wrapped_capabilities,
    }


def _read_json(path: Path) -> object:
    if not path.is_file():
        raise ContractError("native registry input must be an existing file")
    if path.stat().st_size > MAX_INPUT_BYTES:
        raise ContractError("native registry input exceeds the metadata-only size limit")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ContractError("native registry input is not valid UTF-8 JSON") from error


def _write_json_atomic(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    temporary_path = Path(temporary_name)
    try:
        os.chmod(temporary_path, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(value, handle, ensure_ascii=True, indent=2, sort_keys=True)
            handle.write("\n")
        os.replace(temporary_path, path)
    finally:
        if temporary_path.exists():
            temporary_path.unlink()


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", required=True, type=Path, help="Native /capabilities JSON artifact")
    parser.add_argument("--candidate-id", required=True, help="Exact 64-hex release candidate id")
    parser.add_argument("--output", required=True, type=Path, help="Audit wrapper output path")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        registry_path = args.registry.resolve()
        output_path = args.output.resolve()
        if registry_path == output_path:
            raise ContractError("output path must differ from the native registry input")
        snapshot = generate_snapshot(_read_json(registry_path), args.candidate_id)
        _write_json_atomic(output_path, snapshot)
    except (ContractError, OSError) as error:
        print(f"capability snapshot generation failed: {error}", file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "ok": True,
                "candidate_id": args.candidate_id,
                "registry_path": str(registry_path),
                "snapshot_path": str(output_path),
                "source_registry_revision": snapshot["source_registry_revision"],
                "capability_count": len(snapshot["capabilities"]),
            },
            separators=(",", ":"),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
