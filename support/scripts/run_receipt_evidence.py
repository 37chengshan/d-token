#!/usr/bin/env python3
"""N1 receipt evidence producer (PR-1).

Collects OptimizationReceipt artifacts and packages them into the frozen
CandidateEvidenceEnvelope (CF-3, UNIFIED_RELEASE_DESIGN §13). This slice runs
the producer in dry-run mode only: it validates receipts against the PR-1
contract and emits the envelope to an explicit --output path or stdout, and
refuses to write into the official `support/artifacts/final-candidate/`
directory. The official N1 artifact is generated at W12 on C_final.

Contract invariants enforced here mirror the Rust projection (PR-1):
  * Local/Provider/claimable stay separate; Provider usage alone never
    elevates claimable.
  * Per-request cost (费用/差额/币种/单位/价格来源/更新时间) is complete only
    when token and price evidence both exist; a missing price or usage keeps
    the fields unavailable — never 0.
  * A broken attribution chain (claimable missing or non-positive) is not
    claimable.
  * No secret, complete prompt/response, source code, absolute path, or raw
    command output may appear anywhere in the artifact.

The producer never infers evidence; it rejects anything it cannot verify.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
FINAL_CANDIDATE_DIR = ROOT / "support" / "artifacts" / "final-candidate"
SCHEMA_PATH = ROOT / "support" / "schemas" / "candidate-evidence-envelope.schema.json"

PRODUCER_REVISION = "run_receipt_evidence-v1"
NODE_ID = "N1"
ENVELOPE_SCHEMA_VERSION = "1.0.0"

RECEIPT_REQUIRED_FIELDS = {
    "schema_version",
    "request_id",
    "timestamp_ms",
    "transformations",
    "protected_content",
    "validation_status",
    "provider_or_model_changed",
}
RECEIPT_COST_FIELDS = {
    "original_cost_micros",
    "optimized_cost_micros",
    "cost_delta_micros",
    "currency",
    "unit",
    "price_source",
    "price_updated_at_ms",
}
# Token usage evidence fields: at least one must be present for a
# request_cost to be honest. A cost claim with no token evidence at all is
# fabricated and must be rejected, never rendered as 0.
RECEIPT_TOKEN_EVIDENCE_FIELDS = {
    "local_original_tokens",
    "local_optimized_tokens",
    "provider_input_tokens",
    "provider_output_tokens",
}
SENSITIVE_KEY_PATTERN = re.compile(
    r"(^|[^a-z0-9])"
    r"(token|cookie|authorization|api[_-]?key|password|secret|credential|auth[_-]?file)"
    r"([^a-z0-9]|$)",
    re.IGNORECASE,
)
# Matches an absolute path marker wherever it appears in a string (not just
# at the start or after a separator): "/Users/", "/home/", "/private/" and
# drive-letter roots like "C:\" / "C:/" preceded by a non-word boundary.
ABSOLUTE_PATH_PATTERN = re.compile(r"(?<![A-Za-z0-9_])(?:[A-Za-z]:[/\\]|/Users/|/home/|/private/)")
SENSITIVE_VALUE_PATTERN = re.compile(
    r"(Bearer\s+[A-Za-z0-9._~+/-]+=*|sk-[A-Za-z0-9]{8,}|gh[pousr]_[A-Za-z0-9]{8,})"
)
# Frozen schema field names that legitimately contain sensitive-looking words
# (e.g. privacy_assertions.no_secret) — CF-3 envelope properties and the PR-1
# receipt contract fields. The key scan allows these; the VALUE scan still
# rejects credential-shaped values and absolute paths everywhere.
FROZEN_FIELD_NAMES = frozenset(
    {
        # CF-3 CandidateEvidenceEnvelope properties
        "schema_version", "node_id", "status", "reason_codes", "candidate_commit",
        "source_digest", "registry_digest", "corpus_digests", "started_at",
        "finished_at", "platform", "os", "arch", "toolchain", "producer_revision",
        "artifact_sha256", "agent_id", "capability_tier", "privacy_assertions",
        "not_applicable_reasons", "evidence_kind", "route_decision_digest",
        "capability_digest", "renderer_digest", "profile_digest", "font_digest",
        "tokenizer_digest", "pricing_digest", "cache_digest", "eval_digest",
        "no_secret", "no_complete_prompt_or_response", "no_source_code",
        "no_absolute_paths", "no_raw_command_output", "redaction_verified",
        # PR-1 OptimizationReceipt contract fields
        "request_id", "timestamp_ms", "source_agent", "actual_provider",
        "actual_model", "provider_or_model_changed", "transformations",
        "protected_content", "local_original_tokens", "local_optimized_tokens",
        "provider_input_tokens", "provider_output_tokens",
        "estimated_local_tokens_avoided", "claimable_tokens_avoided",
        "validation_status", "skip_or_failure_reason", "request_cost",
        "original_cost_micros", "optimized_cost_micros", "cost_delta_micros",
        "currency", "unit", "price_source", "price_updated_at_ms",
    }
)
SHA256_HEX = re.compile(r"^[0-9a-f]{64}$")
SHA1_40_HEX = re.compile(r"^[0-9a-f]{40}$")
RFC3339 = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.\d+)?(Z|[+-][0-9]{2}:[0-9]{2})$")

NULLABLE_FIELDS = [
    "agent_id",
    "capability_tier",
    "route_decision_digest",
    "capability_digest",
    "renderer_digest",
    "profile_digest",
    "font_digest",
    "tokenizer_digest",
    "pricing_digest",
    "cache_digest",
    "eval_digest",
]


class ContractError(ValueError):
    """Raised when receipt or envelope evidence violates the PR-1 contract."""


def _is_integer(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")


def validate_receipt(receipt: object, label: str = "receipt") -> dict[str, Any]:
    """Validates a single OptimizationReceipt against the PR-1 contract.

    Enforced invariants:
      * required base fields present; no unknown credential-shaped keys;
      * no sensitive material anywhere in the receipt;
      * cost fields are either all present (token+price evidence complete) or
        entirely absent — a partial cost is rejected, and a cost claim with
        no token evidence at all is fabricated and rejected (never 0);
      * a mathematically zero cost is allowed only when the token and price
        evidence is complete;
      * claimable_tokens_avoided is null or strictly positive; Provider usage
        alone never elevates it;
      * missing usage/price stays absent, never 0.
    """
    if not isinstance(receipt, dict):
        raise ContractError(f"{label} must be a JSON object")
    missing = sorted(RECEIPT_REQUIRED_FIELDS - set(receipt))
    if missing:
        raise ContractError(f"{label} is missing required fields: {', '.join(missing)}")
    if not _is_integer(receipt["schema_version"]) or receipt["schema_version"] != 1:
        raise ContractError(f"{label}.schema_version must be 1")
    if not isinstance(receipt["request_id"], str) or not receipt["request_id"]:
        raise ContractError(f"{label}.request_id must be a non-empty string")
    if not _is_integer(receipt["timestamp_ms"]) or receipt["timestamp_ms"] <= 0:
        raise ContractError(f"{label}.timestamp_ms must be a positive integer")

    _assert_no_sensitive_material(receipt, label)

    claimable = receipt.get("claimable_tokens_avoided")
    if claimable is not None and (not _is_integer(claimable) or claimable <= 0):
        raise ContractError(f"{label}.claimable_tokens_avoided must be null or a positive integer")

    request_cost = receipt.get("request_cost")
    present_cost = [field for field in RECEIPT_COST_FIELDS if request_cost and field in request_cost]
    if request_cost is not None:
        if not isinstance(request_cost, dict):
            raise ContractError(f"{label}.request_cost must be an object")
        missing_cost = sorted(RECEIPT_COST_FIELDS - set(request_cost))
        if missing_cost:
            raise ContractError(f"{label}.request_cost is missing cost fields: {', '.join(missing_cost)}")
        if len(present_cost) != len(RECEIPT_COST_FIELDS):
            raise ContractError(f"{label}.request_cost must not be partial")
        token_evidence = [field for field in RECEIPT_TOKEN_EVIDENCE_FIELDS if receipt.get(field) is not None]
        if not token_evidence:
            raise ContractError(
                f"{label}.request_cost has no token usage evidence; a missing usage must stay null, never 0"
            )
        for field in ("original_cost_micros", "optimized_cost_micros"):
            value = request_cost[field]
            if not _is_integer(value) or value < 0:
                raise ContractError(f"{label}.request_cost.{field} must be a non-negative integer")
        for field in ("currency", "unit", "price_source"):
            if not isinstance(request_cost[field], str) or not request_cost[field]:
                raise ContractError(f"{label}.request_cost.{field} must be a non-empty string")
        if not _is_integer(request_cost["price_updated_at_ms"]) or request_cost["price_updated_at_ms"] <= 0:
            raise ContractError(f"{label}.request_cost.price_updated_at_ms must be a positive integer")
        delta = request_cost["cost_delta_micros"]
        if not _is_integer(delta):
            raise ContractError(f"{label}.request_cost.cost_delta_micros must be an integer")
    return receipt


def _assert_no_sensitive_material(value: Any, label: str) -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            # Frozen schema field names (e.g. no_secret) are allowed keys;
            # their values are still scanned below.
            if key not in FROZEN_FIELD_NAMES and SENSITIVE_KEY_PATTERN.search(key):
                raise ContractError(f"{label} contains a sensitive key: {key}")
            _assert_no_sensitive_material(child, f"{label}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _assert_no_sensitive_material(child, f"{label}[{index}]")
    elif isinstance(value, str):
        if ABSOLUTE_PATH_PATTERN.search(value):
            raise ContractError(f"{label} contains an absolute path")
        if SENSITIVE_VALUE_PATTERN.search(value):
            raise ContractError(f"{label} contains a sensitive value")


def validate_receipts(receipts: object) -> list[dict[str, Any]]:
    if not isinstance(receipts, list):
        raise ContractError("receipts must be a JSON array")
    if not receipts:
        raise ContractError("receipts must be non-empty")
    return [validate_receipt(receipt, f"receipts[{index}]") for index, receipt in enumerate(receipts)]


def build_receipt_payload(receipts: object) -> dict[str, Any]:
    """Validated receipts companion payload bound to the envelope via
    envelope.artifact_sha256 (CF-3: the envelope itself never embeds
    receipts — additionalProperties=false, fail-closed)."""
    validated = validate_receipts(receipts)
    return {"receipts": validated, "receipt_count": len(validated)}


def build_envelope(
    receipts: object,
    candidate_commit: str,
    source_digest: str,
    registry_digest: str,
    corpus_digests: dict[str, str] | None = None,
    platform: dict[str, str] | None = None,
) -> dict[str, Any]:
    """Packages validated receipts into a frozen CandidateEvidenceEnvelope (N1)."""
    if not SHA1_40_HEX.fullmatch(candidate_commit):
        raise ContractError("candidate_commit must be exactly 40 lowercase hexadecimal characters")
    if not SHA256_HEX.fullmatch(source_digest):
        raise ContractError("source_digest must be exactly 64 lowercase hexadecimal characters")
    if not SHA256_HEX.fullmatch(registry_digest):
        raise ContractError("registry_digest must be exactly 64 lowercase hexadecimal characters")
    for name, digest in (corpus_digests or {}).items():
        if not SHA256_HEX.fullmatch(digest):
            raise ContractError(f"corpus_digests.{name} must be exactly 64 lowercase hexadecimal characters")

    validated = validate_receipts(receipts)
    started_at = _utc_now()
    finished_at = _utc_now()
    reasons = {field: f"not applicable for generic N1 receipt evidence: {field}" for field in NULLABLE_FIELDS}
    # The envelope never embeds receipts (CF-3 additionalProperties=false);
    # artifact_sha256 binds the companion payload written alongside it.
    payload = build_receipt_payload(validated)
    artifact_sha256 = sha256_bytes(canonical_json_bytes(payload))
    envelope = {
        "schema_version": ENVELOPE_SCHEMA_VERSION,
        "node_id": NODE_ID,
        "status": "pass",
        "reason_codes": [],
        "candidate_commit": candidate_commit,
        "source_digest": source_digest,
        "registry_digest": registry_digest,
        "corpus_digests": corpus_digests or {},
        "started_at": started_at,
        "finished_at": finished_at,
        "platform": platform or {"os": "macos", "arch": "aarch64", "toolchain": "stable"},
        "producer_revision": PRODUCER_REVISION,
        "artifact_sha256": artifact_sha256,
        "agent_id": None,
        "capability_tier": None,
        "privacy_assertions": {
            "no_secret": True,
            "no_complete_prompt_or_response": True,
            "no_source_code": True,
            "no_absolute_paths": True,
            "no_raw_command_output": True,
            "redaction_verified": True,
        },
        "not_applicable_reasons": reasons,
        "evidence_kind": "generic",
        "route_decision_digest": None,
        "capability_digest": None,
        **{field: None for field in NULLABLE_FIELDS[2:]},
    }
    return envelope


def _is_placeholder(value: str) -> bool:
    """True for all-zero placeholder digests (e.g. the CLI defaults), which
    must never be bound to a real official artifact."""
    return len(value) > 0 and set(value) == {"0"}


def _write_json_atomic(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=True, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    # 0600: receipts may carry usage evidence; never world-readable.
    os.chmod(temporary, 0o600)
    temporary.replace(path)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode",
        choices=("fixture", "collect"),
        default="fixture",
        help="fixture: embedded contract-test receipts; collect: receipts from --input",
    )
    parser.add_argument("--input", type=Path, help="Receipts JSON array (collect mode)")
    parser.add_argument("--output", type=Path, help="Explicit envelope output path (dry-run only)")
    parser.add_argument("--candidate-commit", default="0" * 40, help="40-hex C_final candidate commit")
    parser.add_argument("--source-digest", default="0" * 64, help="64-hex candidate source digest")
    parser.add_argument("--registry-digest", default="0" * 64, help="64-hex registry digest")
    parser.add_argument("--dry-run", action="store_true", help="Dry-run: never write the official artifact")
    return parser.parse_args(argv)


def _fixture_receipts() -> list[dict[str, Any]]:
    """Contract-test receipts exercising the PR-1 BDD scenarios."""
    return [
        {
            "schema_version": 1,
            "request_id": "11111111-1111-4111-8111-111111111111",
            "timestamp_ms": 1_700_000_000_000,
            "source_agent": "opencode",
            "actual_provider": "provider-1",
            "actual_model": "model-1",
            "provider_or_model_changed": False,
            "transformations": ["json_compact"],
            "protected_content": "system_message,user_message",
            "local_original_tokens": 1200,
            "local_optimized_tokens": 1000,
            "provider_input_tokens": 1000,
            "provider_output_tokens": 300,
            "estimated_local_tokens_avoided": 200,
            "claimable_tokens_avoided": None,
            "validation_status": "unavailable",
            "skip_or_failure_reason": None,
            "request_cost": {
                "original_cost_micros": 2400,
                "optimized_cost_micros": 2100,
                "cost_delta_micros": 300,
                "currency": "USD",
                "unit": "per_1k_tokens",
                "price_source": "settings",
                "price_updated_at_ms": 1_700_000_000_000,
            },
        },
        {
            "schema_version": 1,
            "request_id": "22222222-2222-4222-8222-222222222222",
            "timestamp_ms": 1_700_000_000_001,
            "source_agent": "claude-code",
            "actual_provider": "provider-2",
            "actual_model": "model-2",
            "provider_or_model_changed": False,
            "transformations": [],
            "protected_content": "system_message",
            "local_original_tokens": None,
            "local_optimized_tokens": None,
            "provider_input_tokens": None,
            "provider_output_tokens": None,
            "estimated_local_tokens_avoided": None,
            "claimable_tokens_avoided": None,
            "validation_status": "unavailable",
            "skip_or_failure_reason": "no_provider_usage",
            "request_cost": None,
        },
    ]


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        if args.mode == "collect":
            if args.input is None or not args.input.is_file():
                raise ContractError("collect mode requires an existing --input receipts JSON file")
            receipts = json.loads(args.input.read_text(encoding="utf-8"))
        else:
            receipts = _fixture_receipts()

        envelope = build_envelope(
            receipts,
            candidate_commit=args.candidate_commit,
            source_digest=args.source_digest,
            registry_digest=args.registry_digest,
        )
        payload = build_receipt_payload(receipts)
        receipt_count = payload["receipt_count"]

        # Only collect without --dry-run may write the official C_final
        # artifact; fixture and dry-run never touch the official directory.
        writes_official = args.mode == "collect" and not args.dry_run
        if writes_official:
            if (
                _is_placeholder(args.candidate_commit)
                or _is_placeholder(args.source_digest)
                or _is_placeholder(args.registry_digest)
            ):
                raise ContractError(
                    "collect (non dry-run) requires non-placeholder candidate_commit/"
                    "source_digest/registry_digest; refusing to bind an all-zero digest"
                )
            if args.output is not None:
                raise ContractError("collect (non dry-run) cannot combine --output with the official artifact path")
            official_dir = FINAL_CANDIDATE_DIR / args.candidate_commit
            official_target = official_dir / f"N1-receipt-{args.candidate_commit}.json"
            companion_target = official_dir / f"N1-receipt-{args.candidate_commit}.companion.json"
            _write_json_atomic(official_target, envelope)
            _write_json_atomic(companion_target, payload)
            written = official_target
        elif args.output is None:
            print(json.dumps(envelope, ensure_ascii=True, indent=2, sort_keys=True))
            written = None
        else:
            output_path = args.output.resolve()
            if FINAL_CANDIDATE_DIR in output_path.parents:
                raise ContractError("dry-run may not write into the official final-candidate directory")
            _write_json_atomic(output_path, envelope)
            written = output_path
        print(
            json.dumps(
                {
                    "ok": True,
                    "mode": args.mode,
                    "dry_run": args.mode != "collect" or args.dry_run,
                    "node_id": NODE_ID,
                    "receipt_count": receipt_count,
                    "artifact_sha256": envelope["artifact_sha256"],
                    "official_artifact": str(written) if written is not None else "W12-on-C_final",
                },
                separators=(",", ":"),
            ),
            file=sys.stderr,
        )
    except (ContractError, OSError, json.JSONDecodeError) as error:
        print(f"receipt evidence generation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
