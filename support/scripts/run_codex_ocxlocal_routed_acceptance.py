#!/usr/bin/env python3
"""N2 Codex ocxlocal routed-flow acceptance producer (PR-2, W2).

Implements PLAN §12.3 PR-2 codex-ocxlocal-acceptance:

  Codex -> d-token:18001/v1 -> ocxlocal:10100/v1 -> dynamic model upstream

The producer proves the routed flow with the semantics frozen in the CF-3
contract and the Agent capability matrix:

  * agent_id=codex, capability_tier=route, evidence_kind=route;
  * Responses wire protocol (`ProviderWireApi::OpenAiResponses`), no OpenAI
    authentication semantics (requires_openai_auth=false);
  * dynamic model passthrough (models=[] allows the request model to flow
    through; the model is never pinned to an install-time value - C8 closed);
  * structured artifact: route verified, request match count, Ledger/metrics
    counts, failure classification, restore status;
  * the report consumes the frozen CF-3 envelope schema
    (support/schemas/candidate-evidence-envelope.schema.json) with non-null
    route_decision_digest/capability_digest;
  * privacy: no secrets, no complete prompt/response, no source code, no
    absolute paths, no raw command output (six privacy assertions all true);
  * a config transaction with exact restore is NOT routed-flow evidence (C8):
    the restore evidence declares config_changes_made=false and
    config_transaction_not_substitute=true.

Modes
-----
--dry-run
    Read-only preflight: validates CLI args, the N2 report schema against the
    frozen CF-3 envelope schema, the report template against the N2 report
    schema, and the privacy scan. It does NOT contact the network, does NOT
    write any artifact, and never claims a real routed flow
    (`real_run_executed=false`, status=preflight only).

--collect (W12 N2 execution)
    Real routed-flow collection: health-checks the d-token listener
    (127.0.0.1:18001) and the ocxlocal upstream (127.0.0.1:10100), sends ONE
    minimal Responses request through the routed surface WITHOUT any
    Authorization header, verifies the returned model matches the requested
    model (dynamic passthrough), counts matching Ledger entries, classifies
    failures, and writes the CF-3 envelope plus companion evidence payloads
    below support/artifacts/final-candidate/<C_final>/ (0600, atomic).

Exit codes: 0 = success; 1 = contract/validation failure; 2 = usage error.

Self-proof for this slice (PR-2): `python3 -m pytest
support/tests/test_codex_ocxlocal_acceptance.py -q` + `--dry-run`. The real
collection path is executed in W12 on C_final.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
CF3_SCHEMA_PATH = ROOT / "support" / "schemas" / "candidate-evidence-envelope.schema.json"
REPORT_SCHEMA_PATH = (
    ROOT / "support" / "fixtures" / "codex-ocxlocal-routed" / "report.schema.json"
)
REPORT_TEMPLATE_PATH = (
    ROOT / "support" / "fixtures" / "codex-ocxlocal-routed" / "report.template.json"
)
MATRIX_PATH = ROOT / "support" / "fixtures" / "agent-capability-matrix.json"
DEFAULT_ARTIFACT_DIR = ROOT / "support" / "artifacts" / "final-candidate"

NODE_ID = "N2"
AGENT_ID = "codex"
CAPABILITY_TIER = "route"
EVIDENCE_KIND = "route"
SCHEMA_VERSION = "1.0.0"
DEFAULT_DTOKEN_PORT = 18001
DEFAULT_OCCLOCAL_PORT = 10100

PRIVACY_KEYS = (
    "no_secret",
    "no_complete_prompt_or_response",
    "no_source_code",
    "no_absolute_paths",
    "no_raw_command_output",
    "redaction_verified",
)

OPTICAL_DIGESTS = (
    "renderer_digest",
    "profile_digest",
    "font_digest",
    "tokenizer_digest",
    "pricing_digest",
    "cache_digest",
    "eval_digest",
)

# Route evidence is frozen as const semantics; the schema and the builder both
# refuse to claim a pinned model or OpenAI auth semantics.
ROUTE_CONST = {
    "protocol": "responses",
    "requires_openai_auth": False,
    "models_passthrough": {
        "models_list_empty": True,
        "model_dynamic": True,
        "model_pinned_to_install_time": False,
    },
}

SECRET_PATTERNS = (
    r"sk-[A-Za-z0-9]{16,}",
    r"Bearer\s+[A-Za-z0-9._~+/=-]{16,}",
    r"AKIA[0-9A-Z]{16}",
    r"ghp_[A-Za-z0-9]{20,}",
    r"xox[baprs]-[A-Za-z0-9-]{10,}",
    r"api[_-]?key\s*[:=]\s*\S{8,}",
    r"authorization\s*[:=]\s*\S{8,}",
    r"client[_-]?secret\s*[:=]\s*\S{8,}",
    r"password\s*[:=]\s*\S{8,}",
    r"token\s*[:=]\s*\S{16,}",
)
ABSOLUTE_PATH_PATTERNS = (
    r"(?<![A-Za-z0-9_.-])/Users/",
    r"(?<![A-Za-z0-9_.-])/home/",
    r"(?<![A-Za-z0-9_.-])/tmp/",
    r"(?<![A-Za-z0-9_.-])/var/",
    r"^[A-Za-z]:[\\/]",
    r"(?<![A-Za-z0-9_.-])~/[A-Za-z0-9_.-]",
)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: Any) -> bytes:
    """Deterministic JSON bytes used for every digest (digest stability)."""
    return json.dumps(
        value, sort_keys=True, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def rfc3339_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def platform_info() -> dict:
    raw_os = platform.system().lower()
    os_name = {"darwin": "macos", "windows": "windows"}.get(raw_os, raw_os)
    if os_name not in {"macos", "windows", "linux"}:
        os_name = "macos"
    machine = platform.machine().lower()
    if machine in {"x86_64", "amd64"}:
        arch = "x86_64"
    elif machine in {"arm64", "aarch64"}:
        arch = "aarch64"
    else:
        arch = machine
    return {
        "os": os_name,
        "arch": arch,
        "toolchain": f"cpython-{platform.python_version()}",
    }


def _validator_factory(schema: dict) -> Callable[[dict], list[str]]:
    """Build a Draft 2020-12 validator with the CF-3 registry attached.

    The N2 report schema consumes the frozen CF-3 envelope via its $id; the
    referencing Registry maps that $id to the frozen schema file so no copy of
    the envelope contract can drift.
    """
    from jsonschema import Draft202012Validator
    from referencing import Registry, Resource

    cf3 = load_json(CF3_SCHEMA_PATH)
    registry = Registry().with_resource(cf3["$id"], Resource.from_contents(cf3))
    validator = Draft202012Validator(schema, registry=registry)

    def validate(instance: dict) -> list[str]:
        return [error.message for error in validator.iter_errors(instance)]

    return validate


def report_schema_errors(report: dict) -> list[str]:
    """Validate an N2 report against report.schema.json (consumes CF-3)."""
    schema = load_json(REPORT_SCHEMA_PATH)
    return _validator_factory(schema)(report)


def cf3_envelope_errors(envelope: dict) -> list[str]:
    """Validate the embedded envelope against the frozen CF-3 schema directly."""
    schema = load_json(CF3_SCHEMA_PATH)
    return _validator_factory(schema)(envelope)


def scan_privacy(payload: Any, extra_forbidden: tuple[str, ...] = ()) -> list[str]:
    """Recursively scan a payload for secrets / absolute paths / prompt content.

    Returns a list of violation descriptions; an empty list means the payload
    satisfies the six privacy assertions.
    """
    import re

    violations: list[str] = []

    def visit(value: Any, path: str) -> None:
        if isinstance(value, dict):
            for key, child in value.items():
                visit(child, f"{path}.{key}" if path else str(key))
        elif isinstance(value, list):
            for index, child in enumerate(value):
                visit(child, f"{path}[{index}]")
        elif isinstance(value, str):
            for pattern in SECRET_PATTERNS:
                if re.search(pattern, value):
                    violations.append(f"secret pattern at {path or '<root>'}")
                    break
            for pattern in ABSOLUTE_PATH_PATTERNS:
                if re.search(pattern, value):
                    violations.append(f"absolute path pattern at {path or '<root>'}")
                    break
            for forbidden in extra_forbidden:
                if forbidden and forbidden in value:
                    violations.append(f"forbidden literal at {path or '<root>'}")
                    break

    visit(payload, "")
    return violations


def privacy_assertions() -> dict:
    return {key: True for key in PRIVACY_KEYS}


def build_route_evidence(
    *,
    model: str,
    provider: str,
    route_verified: bool,
    request_match_count: int,
    failure_classifications: list[str] | None = None,
    upstream_label: str = "ocxlocal:10100",
) -> dict:
    """Build the route evidence payload with frozen dynamic-passthrough semantics.

    Raises ValueError when the caller asks for a pinned model, OpenAI auth
    semantics, or a non-empty models list - the C8/model-fixed regression guard.
    """
    if not model or not model.strip():
        raise ValueError("model must be a non-empty dynamic model id")
    if not provider or not provider.strip():
        raise ValueError("provider must be a non-empty provider id")
    if request_match_count < 0:
        raise ValueError("request_match_count must be >= 0")
    if route_verified and request_match_count == 0:
        raise ValueError("route_verified=true requires request_match_count >= 1")
    # Deep-copy the frozen consts so callers can never mutate the shared
    # models_passthrough semantics (the returned dict must stay independent).
    import copy

    return {
        **copy.deepcopy(ROUTE_CONST),
        "provider": provider,
        "model": model,
        "upstream": upstream_label,
        "route_verified": route_verified,
        "request_match_count": request_match_count,
        "failure_classifications": list(failure_classifications or []),
    }


def build_ledger_evidence(counts: dict) -> dict:
    required = ("events", "route_decisions", "request_logs", "ccr_metadata")
    for key in required:
        if key not in counts or not isinstance(counts[key], int) or counts[key] < 0:
            raise ValueError(f"ledger count {key!r} must be a non-negative integer")
    return {key: counts[key] for key in required}


def build_metrics_evidence(
    *, requests_sent: int, responses_received: int, duration_seconds: float
) -> dict:
    if requests_sent < 0 or responses_received < 0 or responses_received > requests_sent:
        raise ValueError("metrics counts are inconsistent")
    return {
        "requests_sent": requests_sent,
        "responses_received": responses_received,
        "duration_seconds": round(duration_seconds, 3),
    }


def build_failure_evidence(stage: str | None, classifications: list[str], exit_code: int) -> dict:
    return {
        "stage": stage,
        "classifications": list(classifications),
        "exit_code": exit_code,
    }


def build_restore_evidence() -> dict:
    """C8: a config transaction with exact restore is NOT routed-flow evidence."""
    return {
        "config_changes_made": False,
        "restore_status": "not_applicable_no_config_write",
        "config_transaction_not_substitute": True,
        "routed_flow_evidence": True,
    }


def cross_field_errors(envelope: dict) -> list[str]:
    """Cross-field contracts the JSON Schema alone cannot enforce (fail-closed)."""
    errors: list[str] = []
    if envelope.get("status") == "pass":
        if not envelope.get("corpus_digests", {}).get("route"):
            errors.append("status=pass requires a route evidence digest in corpus_digests")
    if envelope.get("agent_id") != AGENT_ID:
        errors.append(f"agent_id must be {AGENT_ID!r}")
    if envelope.get("capability_tier") != CAPABILITY_TIER:
        errors.append(f"capability_tier must be {CAPABILITY_TIER!r}")
    if envelope.get("evidence_kind") != EVIDENCE_KIND:
        errors.append(f"evidence_kind must be {EVIDENCE_KIND!r}")
    started = envelope.get("started_at")
    finished = envelope.get("finished_at")
    if started and finished:
        try:
            start = datetime.fromisoformat(started.replace("Z", "+00:00"))
            end = datetime.fromisoformat(finished.replace("Z", "+00:00"))
            if end < start:
                errors.append("finished_at precedes started_at (RFC3339)")
        except ValueError as exc:
            errors.append(f"invalid RFC3339 timestamp: {exc}")
    return errors


def build_envelope(
    *,
    candidate_commit: str,
    source_digest: str,
    registry_digest: str,
    status: str,
    reason_codes: list[str],
    started_at: str,
    finished_at: str,
    route_decision_digest: str,
    capability_digest: str,
    corpus_digests: dict,
    producer_revision: str,
) -> dict:
    """Build a CF-3 CandidateEvidenceEnvelope for the N2 codex routed report."""
    reasons = {
        digest: f"not applicable for {EVIDENCE_KIND} evidence: {digest}"
        for digest in OPTICAL_DIGESTS
    }
    envelope = {
        "schema_version": SCHEMA_VERSION,
        "node_id": NODE_ID,
        "status": status,
        "reason_codes": list(reason_codes),
        "candidate_commit": candidate_commit,
        "source_digest": source_digest,
        "registry_digest": registry_digest,
        "corpus_digests": dict(corpus_digests),
        "started_at": started_at,
        "finished_at": finished_at,
        "platform": platform_info(),
        "producer_revision": producer_revision,
        "artifact_sha256": "",  # canonical digest computed after the envelope is final
        "agent_id": AGENT_ID,
        "capability_tier": CAPABILITY_TIER,
        "privacy_assertions": privacy_assertions(),
        "not_applicable_reasons": reasons,
        "evidence_kind": EVIDENCE_KIND,
        "route_decision_digest": route_decision_digest,
        "capability_digest": capability_digest,
        **{digest: None for digest in OPTICAL_DIGESTS},
    }
    # artifact_sha256 = SHA-256 over the canonical envelope bytes with the
    # artifact_sha256 member omitted (self-reference-free, deterministic).
    envelope["artifact_sha256"] = sha256_bytes(
        canonical_json({k: v for k, v in envelope.items() if k != "artifact_sha256"})
    )
    return envelope


def write_report(
    artifact_dir: Path,
    stamp: str,
    envelope: dict,
    route_evidence: dict,
    ledger_evidence: dict,
    metrics_evidence: dict,
    failure_evidence: dict,
    restore_evidence: dict,
) -> Path:
    """Write the envelope plus companion evidence payloads atomically (0600).

    Returns the envelope path. Never contains secrets, prompts, absolute paths,
    or raw command output; every payload is validated by the privacy scan first.
    """
    payloads = {
        "route": route_evidence,
        "ledger": ledger_evidence,
        "metrics": metrics_evidence,
        "failure": failure_evidence,
        "restore": restore_evidence,
    }
    for name, payload in payloads.items():
        violations = scan_privacy(payload)
        if violations:
            raise ValueError(f"privacy violation in {name} evidence: {violations}")
    violations = scan_privacy(envelope)
    if violations:
        raise ValueError(f"privacy violation in envelope: {violations}")

    artifact_dir.mkdir(parents=True, exist_ok=True)
    envelope_path = artifact_dir / f"N2-codex-routed-{stamp}.json"
    companion = {
        "route": artifact_dir / f"N2-codex-routed-{stamp}.route.json",
        "ledger": artifact_dir / f"N2-codex-routed-{stamp}.ledger.json",
        "metrics": artifact_dir / f"N2-codex-routed-{stamp}.metrics.json",
        "failure": artifact_dir / f"N2-codex-routed-{stamp}.failure.json",
        "restore": artifact_dir / f"N2-codex-routed-{stamp}.restore.json",
    }

    def write_atomic(path: Path, value: Any) -> None:
        fd, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=str(path.parent))
        try:
            os.fchmod(fd, 0o600)
            with os.fdopen(fd, "w", encoding="utf-8") as handle:
                json.dump(value, handle, ensure_ascii=False, separators=(",", ":"))
                handle.write("\n")
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temporary, path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)

    write_atomic(envelope_path, envelope)
    for name, path in companion.items():
        write_atomic(path, payloads[name])
    return envelope_path


# ---------------------------------------------------------------------------
# Real collection (W12 N2 execution)
# ---------------------------------------------------------------------------

def _http_json(url: str, payload: dict | None, timeout: float) -> tuple[int, Any]:
    """Minimal loopback HTTP helper. Never sends an Authorization header."""
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    request = Request(url, data=data, headers=headers, method="POST" if payload is not None else "GET")
    try:
        with urlopen(request, timeout=timeout) as response:
            body = response.read()
            try:
                return response.status, json.loads(body.decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError):
                return response.status, None
    except HTTPError as exc:
        return exc.code, None
    except (URLError, OSError):
        return 0, None


def _count_matching_ledger(runtime_root: Path, model: str, started_at: str) -> dict:
    """Count Ledger lines that match the dynamic model in the run window.

    Counts only - file content is never echoed into the report. Returns the
    per-file counts plus the number of matching route-decisions/request-logs.
    """
    counts: dict[str, int] = {
        "events": 0,
        "route_decisions": 0,
        "request_logs": 0,
        "ccr_metadata": 0,
    }
    matching = {"route_decisions": 0, "request_logs": 0}
    file_names = {
        "events": "events.jsonl",
        "route_decisions": "route-decisions.jsonl",
        "request_logs": "request-logs.jsonl",
        "ccr_metadata": "ccr-metadata.jsonl",
    }
    if not runtime_root.is_dir():
        return {"counts": counts, "matching": matching, "runtime_root_present": False}
    for key, file_name in file_names.items():
        path = runtime_root / file_name
        if not path.is_file():
            continue
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except (OSError, UnicodeDecodeError):
            continue
        counts[key] = len(lines)
        if key not in matching:
            continue
        for line in lines:
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if not isinstance(item, dict):
                continue
            if item.get("model") != model:
                continue
            timestamp = item.get("timestamp") or item.get("created_at") or item.get("started_at")
            if timestamp and str(timestamp) < started_at:
                continue
            matching[key] += 1
    return {"counts": counts, "matching": matching, "runtime_root_present": True}


def collect_evidence(
    *,
    model: str,
    provider: str,
    dtoken_port: int,
    ocxlocal_port: int,
    runtime_root: Path,
    timeout: float,
) -> dict:
    """Run the real routed-flow collection and return the evidence payloads.

    The returned dict has the keys: status, reason_codes, failure_stage,
    route_evidence, ledger_evidence, metrics_evidence, failure_evidence,
    restore_evidence. On any failure the envelope must be written with the
    failing status and reason codes; a dry-run/fixture never impersonates a
    real routed flow because this function only ever reports what it actually
    observed on the loopback surfaces.
    """
    started_at = rfc3339_now()
    started_epoch = time.monotonic()
    dtoken_base = f"http://127.0.0.1:{dtoken_port}"
    ocxlocal_base = f"http://127.0.0.1:{ocxlocal_port}"

    reason_codes: list[str] = []
    failure_stage: str | None = None

    # 1. d-token listener health (127.0.0.1 only).
    health_status, health_body = _http_json(f"{dtoken_base}/health", None, timeout)
    listener_ok = health_status == 200 and isinstance(health_body, dict) and health_body.get("status") == "healthy"
    if not listener_ok:
        reason_codes.append("listener_unreachable")
        failure_stage = "listener_health"

    # 2. ocxlocal upstream reachability (read-only, no auth header).
    upstream_ok = False
    if listener_ok:
        upstream_status, _ = _http_json(f"{ocxlocal_base}/v1/models", None, timeout)
        upstream_ok = upstream_status == 200
        if not upstream_ok:
            reason_codes.append("upstream_unreachable")
            failure_stage = "upstream_health"

    # 3. One minimal Responses request through the routed surface.
    responses_ok = False
    returned_model: str | None = None
    if listener_ok and upstream_ok:
        responses_payload = {
            "model": model,
            "input": "respond with a single word",
            "stream": False,
        }
        responses_status, responses_body = _http_json(
            f"{dtoken_base}/v1/responses", responses_payload, timeout
        )
        responses_ok = responses_status == 200 and isinstance(responses_body, dict)
        if responses_ok:
            returned_model = responses_body.get("model")
            if isinstance(returned_model, str) and returned_model != model:
                reason_codes.append("model_mismatch_dynamic_passthrough")
                failure_stage = "responses_model"
        else:
            reason_codes.append("responses_request_failed")
            failure_stage = "responses_request"

    # 4. Ledger matching for the dynamic model.
    ledger = _count_matching_ledger(runtime_root, model, started_at)
    request_match_count = (
        ledger["matching"]["route_decisions"] + ledger["matching"]["request_logs"]
    )
    if listener_ok and upstream_ok and responses_ok and request_match_count == 0:
        reason_codes.append("ledger_no_matching_request")
        failure_stage = "ledger_match"

    duration_seconds = time.monotonic() - started_epoch
    status = "pass" if not reason_codes else "fail"
    route_verified = status == "pass"

    return {
        "status": status,
        "reason_codes": reason_codes,
        "failure_stage": failure_stage,
        "route_evidence": build_route_evidence(
            model=model,
            provider=provider,
            route_verified=route_verified,
            request_match_count=request_match_count,
            failure_classifications=reason_codes,
        ),
        "ledger_evidence": build_ledger_evidence(ledger["counts"]),
        "metrics_evidence": build_metrics_evidence(
            requests_sent=1 if listener_ok and upstream_ok else 0,
            responses_received=1 if responses_ok else 0,
            duration_seconds=duration_seconds,
        ),
        "failure_evidence": build_failure_evidence(
            stage=failure_stage,
            classifications=reason_codes,
            exit_code=0 if status == "pass" else 1,
        ),
        "restore_evidence": build_restore_evidence(),
    }


def build_and_write_report(
    *,
    candidate_commit: str,
    source_digest: str,
    registry_digest: str,
    producer_revision: str,
    artifact_dir: Path,
    stamp: str,
    evidence: dict,
) -> Path:
    """Assemble the CF-3 envelope, bind the evidence digests, and write it."""
    route_digest = sha256_bytes(canonical_json(evidence["route_evidence"]))
    capability_digest = sha256_bytes(
        canonical_json(
            {
                "ledger": evidence["ledger_evidence"],
                "metrics": evidence["metrics_evidence"],
                "failure": evidence["failure_evidence"],
                "restore": evidence["restore_evidence"],
            }
        )
    )
    envelope = build_envelope(
        candidate_commit=candidate_commit,
        source_digest=source_digest,
        registry_digest=registry_digest,
        status=evidence["status"],
        reason_codes=evidence["reason_codes"],
        started_at=evidence["started_at"] if "started_at" in evidence else rfc3339_now(),
        finished_at=rfc3339_now(),
        route_decision_digest=route_digest,
        capability_digest=capability_digest,
        corpus_digests={
            "route": route_digest,
            "ledger": sha256_bytes(canonical_json(evidence["ledger_evidence"])),
            "metrics": sha256_bytes(canonical_json(evidence["metrics_evidence"])),
            "failure": sha256_bytes(canonical_json(evidence["failure_evidence"])),
            "restore": sha256_bytes(canonical_json(evidence["restore_evidence"])),
        },
        producer_revision=producer_revision,
    )
    errors = cf3_envelope_errors(envelope) + report_schema_errors({"envelope": envelope, **{
        name: evidence[f"{name}_evidence"] for name in ("route", "ledger", "metrics", "failure", "restore")
    }}) + cross_field_errors(envelope)
    if errors:
        raise ValueError(f"report contract validation failed: {errors}")
    return write_report(
        artifact_dir,
        stamp,
        envelope,
        evidence["route_evidence"],
        evidence["ledger_evidence"],
        evidence["metrics_evidence"],
        evidence["failure_evidence"],
        evidence["restore_evidence"],
    )


# ---------------------------------------------------------------------------
# Dry-run / preflight
# ---------------------------------------------------------------------------

def run_dry_run(*, producer_revision: str, verbose: bool = False) -> dict:
    """Read-only preflight: validate the fixture contract, write nothing.

    Never contacts the network and never writes an artifact; the result marks
    real_run_executed=false and status=preflight so it cannot impersonate a
    real routed flow.
    """
    checks: dict[str, Any] = {}
    for name, path in (
        ("cf3_schema", CF3_SCHEMA_PATH),
        ("report_schema", REPORT_SCHEMA_PATH),
        ("report_template", REPORT_TEMPLATE_PATH),
        ("matrix", MATRIX_PATH),
    ):
        checks[name] = {"exists": path.is_file()}
        if path.is_file():
            try:
                load_json(path)
                checks[name]["valid_json"] = True
            except (OSError, json.JSONDecodeError):
                checks[name]["valid_json"] = False

    schema_errors: list[str] = []
    template_errors: list[str] = []
    privacy_violations: list[str] = []
    try:
        schema = load_json(REPORT_SCHEMA_PATH)
        from jsonschema import Draft202012Validator

        Draft202012Validator.check_schema(schema)
        schema_errors = _validator_factory(schema)(load_json(REPORT_TEMPLATE_PATH))
        template = load_json(REPORT_TEMPLATE_PATH)
        privacy_violations = scan_privacy(template)
        envelope = template.get("envelope", {})
        schema_errors += cf3_envelope_errors(envelope)
        schema_errors += cross_field_errors(envelope)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        schema_errors.append(f"fixture load/validate failed: {exc}")

    ok = (
        all(check["exists"] and check["valid_json"] for check in checks.values())
        and not schema_errors
        and not privacy_violations
    )
    result = {
        "mode": "dry-run",
        "status": "preflight",
        "real_run_executed": False,
        "ok": ok,
        "checks": checks,
        "schema_errors": schema_errors,
        "template_privacy_violations": privacy_violations,
        "producer_revision": producer_revision,
    }
    if verbose:
        result["verbose"] = True
    return result


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def registry_digest() -> str:
    """SHA-256 of the frozen CF-3 Agent capability matrix (the registry)."""
    return sha256_file(MATRIX_PATH)


def producer_revision() -> str:
    """Revision of this tracked producer: its own SHA-256."""
    return sha256_file(Path(__file__).resolve())


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="run_codex_ocxlocal_routed_acceptance.py",
        description="N2 Codex ocxlocal routed-flow acceptance producer (PR-2).",
    )
    parser.add_argument("--dry-run", action="store_true", help="read-only preflight; writes nothing")
    parser.add_argument("--collect", action="store_true", help="real routed-flow collection (W12 N2)")
    parser.add_argument("--candidate-commit", help="C_final 40-hex commit (required for --collect)")
    parser.add_argument("--source-digest", help="candidate source SHA-256 (dirty=false, required for --collect)")
    parser.add_argument("--model", help="dynamic model id to route (required for --collect)")
    parser.add_argument("--provider", default="ocxlocal", help="provider id (default: ocxlocal)")
    parser.add_argument("--dtoken-port", type=int, default=DEFAULT_DTOKEN_PORT, help="d-token listener port (default 18001)")
    parser.add_argument("--ocxlocal-port", type=int, default=DEFAULT_OCCLOCAL_PORT, help="ocxlocal upstream port (default 10100)")
    parser.add_argument("--runtime-root", type=Path, help="d-token runtime data dir holding the Ledger JSONL files")
    parser.add_argument("--output-dir", type=Path, default=None, help="artifact dir (default: support/artifacts/final-candidate/<commit>)")
    parser.add_argument("--timeout", type=float, default=15.0, help="loopback HTTP timeout seconds (default 15)")
    parser.add_argument("--verbose", action="store_true", help="verbose dry-run output")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_arg_parser()
    args = parser.parse_args(argv)

    if args.dry_run and args.collect:
        parser.error("--dry-run and --collect are mutually exclusive")
    if not args.dry_run and not args.collect:
        parser.error("choose --dry-run or --collect")

    revision = producer_revision()

    if args.dry_run:
        result = run_dry_run(producer_revision=revision, verbose=args.verbose)
        print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
        return 0 if result["ok"] else 1

    # --collect (W12)
    missing = []
    for flag, value in (
        ("--candidate-commit", args.candidate_commit),
        ("--source-digest", args.source_digest),
        ("--model", args.model),
        ("--runtime-root", args.runtime_root),
    ):
        if not value:
            missing.append(flag)
    if missing:
        parser.error("--collect requires: " + ", ".join(missing))
    if not isinstance(args.candidate_commit, str) or len(args.candidate_commit) != 40 or not all(c in "0123456789abcdef" for c in args.candidate_commit):
        parser.error("--candidate-commit must be a 40-hex SHA-1")
    if not isinstance(args.source_digest, str) or len(args.source_digest) != 64 or not all(c in "0123456789abcdef" for c in args.source_digest):
        parser.error("--source-digest must be a 64-hex SHA-256")
    if not isinstance(args.model, str) or not args.model.strip():
        parser.error("--model must be a non-empty dynamic model id")

    output_dir = args.output_dir or DEFAULT_ARTIFACT_DIR / str(args.candidate_commit)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    try:
        evidence = collect_evidence(
            model=args.model,
            provider=args.provider,
            dtoken_port=args.dtoken_port,
            ocxlocal_port=args.ocxlocal_port,
            runtime_root=Path(args.runtime_root),
            timeout=args.timeout,
        )
        evidence["started_at"] = rfc3339_now()
        path = build_and_write_report(
            candidate_commit=args.candidate_commit,
            source_digest=args.source_digest,
            registry_digest=registry_digest(),
            producer_revision=revision,
            artifact_dir=output_dir,
            stamp=stamp,
            evidence=evidence,
        )
        summary = {
            "mode": "collect",
            "status": evidence["status"],
            "reason_codes": evidence["reason_codes"],
            "artifact": str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path),
            "report_path": str(path),
        }
        print(json.dumps(summary, ensure_ascii=False, indent=2, sort_keys=True))
        return 0 if evidence["status"] == "pass" else 1
    except (ValueError, OSError) as exc:
        print(json.dumps({"mode": "collect", "status": "failed", "error": str(exc)}, ensure_ascii=False), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
