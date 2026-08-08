#!/usr/bin/env python3
"""Codex -> d-token loopback listener (18001) -> ocxlocal upstream (10100)
routed acceptance evidence validator (macOS-first, offline-testable).

PR-2 fail-closed contract:
  - This runner NEVER binds a temporary listener and NEVER probes the upstream
    itself. Real mode consumes and correlates d-token runtime evidence only:
      * GET 127.0.0.1:18001/health       -> runtime identity (HealthResponse)
      * GET 127.0.0.1:18001/export/json  -> primary evidence: request_logs,
        route_decisions, events, stats, active_config (providers/routes)
      * GET 127.0.0.1:18001/metrics      -> Prometheus text (dtoken_requests_total)
    The chain is verified by correlating the SAME request_id across
    request_logs -> route_decisions -> events, with agent normalized to codex,
    provider metadata (active_config.providers) confirming
    127.0.0.1:10100, responses-family protocol, and auth-free semantics
    (credential_configured=false). route_decision.output_protocol must be a
    responses-family protocol. Models are forwarded from the actual
    request_logs (ocxlocal owns dynamic switching). Any missing or mismatched
    evidence — including zero ledger events — is blocked.
  - --check-schema / --dry-run only proves the report schema against the
    schema/template fixture under support/fixtures/codex-ocxlocal-routed/.
    Schema-only mode never sets ok=true and never claims a routed chain.
  - Every real-mode artifact is bound to the candidate commit (read-only
    git rev-parse HEAD of the repository root) plus the staged source config
    digest (freshness.source_fingerprint_sha256); a run that would pass
    without a resolvable commit fails closed (candidate_unbound).
  - Redaction: auth references (keychain:// / os-credential://) and upstream
    URLs are never emitted verbatim; only redacted fingerprints are recorded.
    Internal identifiers (request_id/provider_id/route_id) appear in failure
    details only as truncated sha256 hashes. Raw prompts and responses never
    enter the report.
  - The runner performs no config writes, so restore.status is
    "no_changes_made"; Codex config files are never modified or read.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any, Callable
from urllib.parse import urlsplit

MODE = "codex-ocxlocal-routed-acceptance"
LOOPBACK = "127.0.0.1"
DTOKEN_SERVICE_PORT = 18001
OCXLOCAL_EXPECTED_PORT = 10100
RUNTIME_IDENTITY = "dtoken-runtime-local"
CORE_API_COMPONENT = "dtoken-core-api"
AGENT_ID = "codex"
PROTOCOL = "responses"
REQUIRES_OPENAI_AUTH = False
SCHEMA_VERSION = 1

REF_PREFIXES = ("keychain://", "os-credential://")

# Fixed, non-sensitive acceptance prompt passed to `codex exec` for each model.
ACCEPTANCE_PROMPT = "Reply with exactly: d-token routed acceptance ok"
# Filename substrings that make a --codex-config source unacceptable.
FORBIDDEN_SOURCE_PARTS = ("auth", "secret", "credential", "key", "token", "cookie")

_SCHEMA_DIR = Path(__file__).resolve().parents[1] / "fixtures" / "codex-ocxlocal-routed"
REPORT_SCHEMA_FILE = _SCHEMA_DIR / "report.schema.json"
REPORT_TEMPLATE_FILE = _SCHEMA_DIR / "report.template.json"
REPO_ROOT = Path(__file__).resolve().parents[2]

# Status values allowed by the schema artifact.
STATUS_SCHEMA_ONLY = "schema-validated-only"
STATUS_ROUTED = "routed-verified"
STATUS_BLOCKED = "blocked"
STATUS_FAILED = "failed"

FAIL_CLASSES = (
    "none",
    "usage",
    "schema_invalid",
    "schema_artifact_missing",
    "runtime_unreachable",
    "runtime_identity_mismatch",
    "evidence_missing",
    "no_routed_codex_request",
    "upstream_not_found",
    "upstream_mismatch",
    "correlation_mismatch",
    "execution_failed",
    "stale_only",
    "source_changed",
    "candidate_unbound",
)


class UsageError(Exception):
    """Invalid or fail-closed invocation."""


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--model",
        action="append",
        dest="models",
        help="Model to route (repeatable; at least 2 distinct values)",
    )
    parser.add_argument("--upstream-base-url", help="Declared ocxlocal upstream (default http://127.0.0.1:10100/v1)")
    parser.add_argument(
        "--api-key-ref",
        help="keychain://SERVICE/ACCOUNT or os-credential://SERVICE/ACCOUNT (declared only, never resolved or emitted)",
    )
    parser.add_argument(
        "--upstream-no-auth",
        action="store_true",
        help="Explicitly declare the auth-free ocxlocal upstream",
    )
    parser.add_argument("--upstream-port", type=int, default=OCXLOCAL_EXPECTED_PORT, help="Expected ocxlocal upstream port")
    parser.add_argument("--check-schema", action="store_true", help="Validate the report schema against the artifact; no network activity")
    parser.add_argument("--dry-run", action="store_true", help="Alias of --check-schema (schema-only proof, never claims routed)")
    parser.add_argument("--report-out", help="Write the redacted JSON report to this path (default: stdout)")
    parser.add_argument("--timeout", type=float, default=15.0, help="d-token evidence fetch timeout in seconds")
    parser.add_argument("--codex-config", help="Absolute path to the real Codex config.toml staged into the temp CODEX_HOME")
    parser.add_argument("--temp-root", help="Existing absolute directory under which a fresh CODEX_HOME/workdir is created")
    parser.add_argument("--codex-bin", default="codex", help="Codex executable (default: codex)")
    return parser


def validate_reference(ref: str | None, no_auth: bool) -> None:
    if bool(ref) == bool(no_auth):
        raise UsageError("Exactly one of --api-key-ref or --upstream-no-auth is required in real mode.")
    if ref is not None and not ref.startswith(REF_PREFIXES):
        raise UsageError("API key reference must be keychain:// or os-credential://; literal keys are rejected.")


def validate_port(port: int) -> None:
    if not (1 <= port <= 65535):
        raise UsageError(f"Port must be in [1, 65535], got {port}.")


def validate_codex_source(path_str: str | None) -> Path:
    """Real-mode source config: absolute, regular, not a symlink, and the
    filename must not contain auth/secret/credential/key/token/cookie."""
    if not path_str:
        raise UsageError("--codex-config is required in real mode.")
    path = Path(path_str).expanduser()
    if not path.is_absolute():
        raise UsageError("--codex-config must be an absolute path.")
    if path.is_symlink():
        raise UsageError("--codex-config must not be a symlink.")
    if not path.is_file():
        raise UsageError("--codex-config must be a regular file.")
    name = path.name.lower()
    if any(part in name for part in FORBIDDEN_SOURCE_PARTS):
        raise UsageError("--codex-config filename must not contain auth/secret/credential/key/token/cookie.")
    return path


def validate_models(models: list[str] | None) -> list[str]:
    if not models:
        raise UsageError("--model is required in real mode (at least 2 distinct values).")
    distinct: list[str] = []
    for model in models:
        if model not in distinct:
            distinct.append(model)
    if len(distinct) < 2:
        raise UsageError("--model requires at least 2 distinct values.")
    return distinct


def validate_temp_root(path_str: str | None) -> Path:
    if not path_str:
        raise UsageError("--temp-root is required in real mode.")
    path = Path(path_str).expanduser()
    if not path.is_absolute():
        raise UsageError("--temp-root must be an absolute path.")
    if not path.is_dir():
        raise UsageError("--temp-root must be an existing directory.")
    return path


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_mode_str(path: Path) -> str:
    # POSIX: the real permission bits. Windows: st_mode is synthesized by the
    # OS layer (typically 0o666/0o444) and is only an informational attribution
    # there — content integrity is enforced by file_sha256, not by this field.
    return oct(path.stat().st_mode & 0o777)


def extract_request_ids(export: Any) -> set[str]:
    if not isinstance(export, dict):
        return set()
    logs = export.get("request_logs")
    if not isinstance(logs, list):
        return set()
    return {
        log.get("request_id")
        for log in logs
        if isinstance(log, dict) and isinstance(log.get("request_id"), str)
    }


def run_codex_exec(
    codex_bin: str,
    model: str,
    codex_home: Path,
    workdir: Path,
    prompt: str,
    timeout: float,
) -> bool:
    """Run one `codex exec --skip-git-repo-check -m MODEL <prompt>` with
    CODEX_HOME pointing at the staged temp home. stdout/stderr are captured
    and never enter the report. Any non-zero exit or timeout returns False."""
    env = dict(os.environ)
    env["CODEX_HOME"] = str(codex_home)
    command = [codex_bin, "exec", "--skip-git-repo-check", "-m", model, prompt]
    try:
        result = subprocess.run(
            command,
            env=env,
            cwd=str(workdir),
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def _sha256(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def git_head() -> tuple[str, bool]:
    """Read-only candidate commit resolution: git rev-parse HEAD in the repo
    root. Returns (commit, True) when HEAD is a full 40-64 hex object id, or
    ("", False) when git is unavailable, HEAD is not a full object id, or the
    checkout is not a repository. Never writes anything and never prints."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True,
            timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        return "", False
    if result.returncode != 0:
        return "", False
    commit = result.stdout.strip()
    if not re.fullmatch(r"[0-9a-fA-F]{40,64}", commit):
        return "", False
    return commit, True


def _redact_id(identifier: str) -> str:
    """Truncated hash of an internal identifier (request_id/provider_id/
    route_id) for failure details — never raw."""
    return _sha256(identifier)[:16]


def auth_redacted(ref: str | None) -> dict[str, Any]:
    """Redacted auth record: backend kind + fingerprint, never the reference."""
    if ref is None:
        return {"declared": "none", "fingerprint_sha256": "", "redacted": True}
    return {"declared": "ref", "fingerprint_sha256": _sha256(ref), "redacted": True}


def parse_expected_upstream(url: str | None) -> dict[str, Any]:
    """Redacted upstream record from a base URL: scheme/host/port + fingerprint.

    The full URL (including any path/query/token) is never emitted. A
    non-numeric or out-of-range port (or malformed IPv6 host) is a usage
    error, never a raw traceback.
    """
    text = (url or f"http://{LOOPBACK}:{OCXLOCAL_EXPECTED_PORT}/v1").strip()
    try:
        parsed = urlsplit(text)
        scheme = parsed.scheme or "http"
        host = parsed.hostname or LOOPBACK
        port = parsed.port or (443 if scheme == "https" else 80)
    except ValueError as exc:
        raise UsageError(f"Invalid --upstream-base-url {text!r}: {exc}") from exc
    return {
        "scheme": scheme,
        "host": host,
        "port": port,
        "fingerprint_sha256": _sha256(text),
        "redacted": True,
    }


def base_report(args: argparse.Namespace) -> dict[str, Any]:
    upstream = parse_expected_upstream(args.upstream_base_url)
    return {
        "schema_version": SCHEMA_VERSION,
        "mode": MODE,
        "status": STATUS_SCHEMA_ONLY,
        "ok": False,
        "schema_valid": True,
        "routed": False,
        "platform": platform.system().lower(),
        "agent_id": AGENT_ID,
        "protocol": PROTOCOL,
        "requires_openai_auth": REQUIRES_OPENAI_AUTH,
        "route": {
            "dynamic_model": False,
            "model_forwarded": [],
            "route_verified": False,
            "upstream": upstream,
            "auth": auth_redacted(args.api_key_ref),
        },
        "runtime": {
            "port": DTOKEN_SERVICE_PORT,
            "runtime_identity": "",
            "component_verified": False,
            "health_status": "not_checked",
        },
        "ledger": {"request_match_count": 0, "events_observed": 0, "redacted": True},
        "metrics": {"request_count": 0, "redacted": True},
        "freshness": {
            "source_fingerprint_sha256": "",
            "source_mode": "",
            "fresh_window_seconds": 0.0,
            "new_request_ids_hash": "",
            "new_request_ids_count": 0,
        },
        "failure": {"class": "none", "detail_redacted": ""},
        "restore": {
            "status": "no_changes_made",
            "note": "runner performs no config writes; nothing to restore",
        },
        "candidate": {"commit": "", "resolved": False},
        "redacted": True,
    }


def _fail(report: dict[str, Any], failure_class: str, detail: str) -> dict[str, Any]:
    assert failure_class in FAIL_CLASSES, failure_class
    report["ok"] = False
    report["routed"] = False
    report["status"] = STATUS_FAILED if failure_class.startswith("runtime_") else STATUS_BLOCKED
    report["failure"] = {"class": failure_class, "detail_redacted": detail}
    return report


# ---------------------------------------------------------------------------
# Evidence consumers (d-token runtime on 127.0.0.1:18001)
# ---------------------------------------------------------------------------


def http_get_json(base_url: str, path: str, timeout: float) -> Any | None:
    """GET a JSON payload; returns None on any failure (caller classifies)."""
    try:
        with urllib.request.urlopen(base_url + path, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8"))
    except Exception:
        return None


def http_get_text(base_url: str, path: str, timeout: float) -> str | None:
    """GET a plain-text payload (/metrics is Prometheus text, not JSON)."""
    try:
        with urllib.request.urlopen(base_url + path, timeout=timeout) as response:
            return response.read().decode("utf-8")
    except Exception:
        return None


def fetch_runtime_evidence(
    base_url: str,
    timeout: float,
    fetch_json: Callable[[str, str, float], Any] | None = None,
    fetch_text: Callable[[str, str, float], Any] | None = None,
) -> dict[str, Any]:
    get_json = fetch_json or http_get_json
    get_text = fetch_text or http_get_text
    return {
        "health": get_json(base_url, "/health", timeout),
        "export": get_json(base_url, "/export/json", timeout),
        "metrics_text": get_text(base_url, "/metrics", timeout),
    }


def verify_runtime_identity(health: Any) -> tuple[bool, str]:
    if not isinstance(health, dict):
        return False, ""
    if health.get("component") != CORE_API_COMPONENT:
        return False, ""
    rid = health.get("runtime_identity")
    if not isinstance(rid, str) or rid != RUNTIME_IDENTITY:
        return False, ""
    return True, rid


def normalize_agent(value: Any) -> str:
    """Agent field normalization: case-insensitive, whitespace-trimmed."""
    return str(value or "").strip().lower()


def model_key(value: Any) -> str:
    """Model identity from a request_log field: whitespace-trimmed string."""
    return str(value or "").strip()


def protocol_is_responses(value: Any) -> bool:
    """True when a wire/protocol value is responses-family.

    Accepts the Rust enum spellings that map to OpenAiResponses:
    "open_ai" (Protocol::OpenAi), "OpenAiResponses" / "open_ai_responses"
    (ProviderWireApi), or the explicit label "responses".
    """
    if not isinstance(value, str):
        return False
    normalized = "".join(ch for ch in value.lower() if ch.isalnum())
    return normalized in ("openai", "openairesponses", "responses")


def parse_metrics_count(text: str | None) -> int | None:
    """Parse dtoken_requests_total from Prometheus text (/metrics)."""
    if not isinstance(text, str):
        return None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("dtoken_requests_total") and "{" not in stripped.split()[0]:
            parts = stripped.split()
            if len(parts) == 2 and parts[0] == "dtoken_requests_total" and parts[1].isdigit():
                return int(parts[1])
    return None


def find_provider_meta(export: Any, provider_id: str) -> dict[str, Any] | None:
    if not isinstance(export, dict):
        return None
    providers = (export.get("active_config") or {}).get("providers")
    if not isinstance(providers, list):
        return None
    for provider in providers:
        if isinstance(provider, dict) and provider.get("id") == provider_id:
            return provider
    return None


def find_route_meta(export: Any, route_id: str) -> dict[str, Any] | None:
    if not isinstance(export, dict):
        return None
    routes = (export.get("active_config") or {}).get("routes")
    if not isinstance(routes, list):
        return None
    for route in routes:
        if isinstance(route, dict) and route.get("id") == route_id:
            return route
    return None


def export_stats_total(export: Any) -> int | None:
    """Fallback request total from export.stats.requests.total."""
    if not isinstance(export, dict):
        return None
    requests = (export.get("stats") or {}).get("requests")
    if not isinstance(requests, dict):
        return None
    total = requests.get("total")
    return total if isinstance(total, int) else None


def correlate_chain(
    export: Any,
    expected_port: int,
    only_ids: set[str] | None = None,
    expected_models: list[str] | None = None,
) -> tuple[dict[str, Any], tuple[str, str] | None]:
    """Correlate request_logs -> route_decisions -> events via request_id.

    When only_ids is given, correlation is restricted to those request_ids
    (the fresh post-fetch window minus pre-fetch ids). When expected_models is
    given it is the validation set: logs whose model is NOT in it are ignored
    (concurrent same-window non-expected Codex traffic), and only correlated
    expected-model request_ids are reported. Returns (chain, failure). chain
    keys: models, request_ids, request_match_count, events_observed, upstream
    (redacted record from provider metadata). Any gap (missing array,
    non-codex agent, mismatched request_id, wrong provider
    base_url/protocol/credential semantics, missing route meta, zero events)
    returns a fail-closed (None, (failure_class, detail)).
    """
    if not isinstance(export, dict) or export.get("source") != "dtoken-owned-runtime":
        return {}, ("evidence_missing", "export/json missing or not dtoken-owned-runtime")
    logs = export.get("request_logs")
    decisions = export.get("route_decisions")
    events = export.get("events")
    if not isinstance(logs, list) or not isinstance(decisions, list) or not isinstance(events, list):
        return {}, ("evidence_missing", "export/json request_logs/route_decisions/events must be arrays")

    expected_set = set(expected_models) if expected_models is not None else None
    codex_logs = [
        log
        for log in logs
        if isinstance(log, dict)
        and normalize_agent(log.get("agent")) == AGENT_ID
        and (only_ids is None or log.get("request_id") in only_ids)
        and (expected_set is None or model_key(log.get("model")) in expected_set)
    ]
    if not codex_logs:
        return {}, ("no_routed_codex_request", "no fresh request_log with agent=codex in the post-fetch window")

    models: list[str] = []
    matched_request_ids: list[str] = []
    matched_requests = 0
    matched_events = 0
    upstream: dict[str, Any] | None = None

    for log in codex_logs:
        request_id = log.get("request_id")
        route_id = log.get("route_id")
        provider_id = log.get("provider_id")
        model = log.get("model")
        if not all(isinstance(v, str) and v for v in (request_id, route_id, provider_id)):
            return {}, ("evidence_missing", f"request_log missing request_id/route_id/provider_id: {_redact_id(str(request_id))!r}")
        decision = next((d for d in decisions if isinstance(d, dict) and d.get("request_id") == request_id), None)
        if decision is None:
            return {}, ("correlation_mismatch", f"no route_decision for request_id={_redact_id(request_id)}")
        if decision.get("route_id") != route_id or decision.get("provider_id") != provider_id:
            return {}, ("correlation_mismatch", f"route_decision route/provider mismatch for request_id={_redact_id(request_id)}")
        if not protocol_is_responses(decision.get("output_protocol")):
            return {}, ("upstream_mismatch", f"route_decision output_protocol not responses for request_id={_redact_id(request_id)}")

        event = next(
            (e for e in events if isinstance(e, dict) and e.get("request_id") == request_id),
            None,
        )
        if event is None:
            return {}, ("evidence_missing", f"no ledger event for request_id={_redact_id(request_id)}")
        event_agent = normalize_agent(event.get("agent_id"))
        if event_agent != AGENT_ID:
            return {}, ("correlation_mismatch", f"ledger event agent {event_agent!r} != codex for request_id={_redact_id(request_id)}")
        matched_events += 1

        provider = find_provider_meta(export, provider_id)
        if provider is None:
            return {}, ("upstream_not_found", f"provider metadata missing for provider_id={_redact_id(provider_id)}")
        base = provider.get("base_url")
        if not isinstance(base, str) or not base:
            return {}, ("evidence_missing", f"provider {_redact_id(provider_id)} base_url missing")
        try:
            parsed = urlsplit(base)
            host = parsed.hostname
            port = parsed.port or (443 if parsed.scheme == "https" else 80)
        except ValueError:
            return {}, ("upstream_mismatch", f"provider {_redact_id(provider_id)} base_url is not a valid URL")
        if host != LOOPBACK or port != expected_port:
            return {}, ("upstream_mismatch", f"provider {_redact_id(provider_id)} base_url is not {LOOPBACK}:{expected_port}")
        if not protocol_is_responses(provider.get("protocol")):
            return {}, ("upstream_mismatch", f"provider {_redact_id(provider_id)} protocol not responses-family")
        if provider.get("credential_configured") is not False:
            return {}, ("upstream_mismatch", f"provider {_redact_id(provider_id)} credential_configured must be false (auth-free upstream)")
        route_meta = find_route_meta(export, route_id)
        if route_meta is None:
            return {}, ("upstream_not_found", f"route metadata missing for route_id={_redact_id(route_id)}")
        route_target = route_meta.get("target_provider_id")
        if isinstance(route_target, str) and route_target and route_target != provider_id:
            return {}, ("correlation_mismatch", f"route {_redact_id(route_id)} target_provider_id {_redact_id(route_target)!r} != provider_id {_redact_id(provider_id)!r}")

        keyed = model_key(model)
        if keyed and keyed not in models:
            models.append(keyed)
        matched_request_ids.append(request_id)
        matched_requests += 1
        if upstream is None:
            upstream = parse_expected_upstream(base)

    if matched_requests == 0 or matched_events == 0:
        return {}, ("evidence_missing", "no correlated codex request/event evidence (ok=true with 0 is forbidden)")
    return {
        "models": models,
        "request_ids": sorted(set(matched_request_ids)),
        "request_match_count": matched_requests,
        "events_observed": matched_events,
        "upstream": upstream,
    }, None


def build_report(
    args: argparse.Namespace,
    evidence: dict[str, Any],
    *,
    only_ids: set[str] | None = None,
    source_fingerprint: str = "",
    source_mode: str = "",
    fresh_window_seconds: float = 0.0,
    expected_models: list[str] | None = None,
    metrics_delta: int = 0,
) -> dict[str, Any]:
    """Correlate d-token runtime + ledger/metrics evidence; fail closed on any gap.

    only_ids restricts the fresh window to post-fetch request_ids; logs are
    additionally filtered to expected_models when given, so concurrent
    non-expected Codex requests in the same window are ignored and never
    enter counts, the forwarded model set, or the freshness id set. The
    correlated model set must then equal expected_models exactly.
    metrics.request_count records the fresh metrics_delta (fail-closed below
    2), never a post absolute counter value. source_* / fresh_window fill the
    freshness record used to attribute the run (never absolute paths or raw
    output).
    """
    report = base_report(args)

    health = evidence.get("health")
    if health is None:
        return _fail(
            report,
            "runtime_unreachable",
            f"d-token runtime not reachable on {LOOPBACK}:{DTOKEN_SERVICE_PORT}",
        )
    rid_ok, rid = verify_runtime_identity(health)
    if not rid_ok:
        return _fail(report, "runtime_identity_mismatch", "runtime identity/component do not match dtoken-runtime-local")
    report["runtime"]["runtime_identity"] = rid
    report["runtime"]["component_verified"] = True
    report["runtime"]["health_status"] = str(health.get("status", "unknown"))

    chain, failure = correlate_chain(
        evidence.get("export"),
        args.upstream_port,
        only_ids=only_ids,
        expected_models=expected_models,
    )
    if failure is not None:
        return _fail(report, failure[0], failure[1])
    assert chain["upstream"] is not None

    if expected_models is not None:
        expected_set = set(expected_models)
        correlated_set = set(chain["models"])
        missing = sorted(expected_set - correlated_set)
        unexpected = sorted(correlated_set - expected_set)
        if missing or unexpected:
            return _fail(
                report,
                "stale_only",
                f"fresh model set mismatch: missing={missing} unexpected={unexpected}",
            )
        if len(correlated_set) < 2:
            return _fail(report, "stale_only", "fewer than 2 distinct models in fresh request_logs")

    expectation = parse_expected_upstream(args.upstream_base_url)
    routed = chain["upstream"]
    if (expectation["host"], expectation["port"]) != (routed["host"], routed["port"]):
        return _fail(report, "upstream_mismatch", "routed upstream does not match the declared expectation")
    report["route"]["upstream"] = routed

    metric_count = parse_metrics_count(evidence.get("metrics_text"))
    if metric_count is None:
        metric_count = export_stats_total(evidence.get("export"))
    if metric_count is None:
        return _fail(report, "evidence_missing", "metrics text has no dtoken_requests_total and export.stats has no requests.total")
    if metrics_delta < 2:
        return _fail(report, "stale_only", f"fresh metrics delta {metrics_delta} < 2")

    report["route"]["model_forwarded"] = chain["models"]
    report["route"]["dynamic_model"] = len(set(chain["models"])) >= 2
    report["route"]["route_verified"] = True
    report["ledger"]["request_match_count"] = chain["request_match_count"]
    report["ledger"]["events_observed"] = chain["events_observed"]
    report["metrics"]["request_count"] = metrics_delta
    correlated_ids = chain["request_ids"]
    report["freshness"] = {
        "source_fingerprint_sha256": source_fingerprint,
        "source_mode": source_mode,
        "fresh_window_seconds": round(fresh_window_seconds, 3),
        "new_request_ids_hash": _sha256("\n".join(sorted(correlated_ids))) if correlated_ids else "",
        "new_request_ids_count": len(correlated_ids),
    }
    report["routed"] = True
    report["ok"] = True
    report["status"] = STATUS_ROUTED
    return report


# ---------------------------------------------------------------------------
# Schema validation against the tracked fixture under support/fixtures/
# ---------------------------------------------------------------------------


def load_schema() -> dict[str, Any] | None:
    if not REPORT_SCHEMA_FILE.is_file():
        return None
    try:
        payload = json.loads(REPORT_SCHEMA_FILE.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return None
    return payload if isinstance(payload, dict) else None


def _validate_node(value: Any, schema_node: Any, errors: list[str], path: str) -> None:
    if not isinstance(schema_node, dict):
        return
    if "const" in schema_node and value != schema_node["const"]:
        errors.append(f"{path}: expected const {schema_node['const']!r}, got {value!r}")
        return
    if "enum" in schema_node and value not in schema_node["enum"]:
        errors.append(f"{path}: value {value!r} not in allowed enum")
        return
    node_type = schema_node.get("type")
    if node_type == "object":
        if not isinstance(value, dict):
            errors.append(f"{path}: expected object, got {type(value).__name__}")
            return
        properties = schema_node.get("properties", {})
        for key in schema_node.get("required", []):
            if key not in value:
                errors.append(f"{path}: missing required property {key!r}")
        for key, item in value.items():
            if schema_node.get("additionalProperties") is False and key not in properties:
                errors.append(f"{path}: unexpected property {key!r}")
            elif key in properties:
                _validate_node(item, properties[key], errors, f"{path}.{key}")
    elif node_type == "array":
        if not isinstance(value, list):
            errors.append(f"{path}: expected array, got {type(value).__name__}")
            return
        items = schema_node.get("items", {})
        for index, item in enumerate(value):
            _validate_node(item, items, errors, f"{path}[{index}]")
    elif node_type == "string":
        if not isinstance(value, str):
            errors.append(f"{path}: expected string, got {type(value).__name__}")
    elif node_type == "integer":
        if not isinstance(value, int) or isinstance(value, bool):
            errors.append(f"{path}: expected integer, got {type(value).__name__}")
        elif "minimum" in schema_node and value < schema_node["minimum"]:
            errors.append(f"{path}: value {value!r} below minimum {schema_node['minimum']}")
    elif node_type == "number":
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            errors.append(f"{path}: expected number, got {type(value).__name__}")
        elif not math.isfinite(value):
            errors.append(f"{path}: expected finite number, got {value!r}")
        elif "minimum" in schema_node and value < schema_node["minimum"]:
            errors.append(f"{path}: value {value!r} below minimum {schema_node['minimum']}")
    elif node_type == "boolean":
        if not isinstance(value, bool):
            errors.append(f"{path}: expected boolean, got {type(value).__name__}")


def validate_report_against_schema(report: dict[str, Any], schema: dict[str, Any]) -> list[str]:
    """Structural JSON-Schema subset validator: type, const, enum, required,
    properties, additionalProperties, items, minimum. Zero external deps."""
    errors: list[str] = []
    _validate_node(report, schema, errors, "$")
    return errors


def schema_only_report(args: argparse.Namespace) -> dict[str, Any]:
    """Schema-only proof: ok stays False, routed stays False, no network I/O."""
    report = base_report(args)
    report["status"] = STATUS_SCHEMA_ONLY
    report["schema_valid"] = True
    return report


def emit(report: dict[str, Any], report_out: str | None) -> None:
    payload = json.dumps(report, indent=2, sort_keys=True)
    if report_out:
        with open(report_out, "w", encoding="utf-8") as handle:
            handle.write(payload + "\n")
    else:
        print(payload)


def run_schema_check(args: argparse.Namespace) -> int:
    report = schema_only_report(args)
    schema = load_schema()
    if schema is None:
        report = _fail(report, "schema_artifact_missing", "report schema fixture missing under support/fixtures/")
        report["schema_valid"] = False
    else:
        errors = validate_report_against_schema(report, schema)
        report["schema_valid"] = not errors
        if errors:
            detail = "; ".join(errors)[:300]
            report = _fail(report, "schema_invalid", detail)
    emit(report, args.report_out)
    return 0 if report["schema_valid"] else 1


def run_routed(args: argparse.Namespace) -> int:
    validate_reference(args.api_key_ref, args.upstream_no_auth)
    validate_port(args.upstream_port)
    if (
        args.upstream_base_url is not None
        and parse_expected_upstream(args.upstream_base_url)["port"] != args.upstream_port
    ):
        raise UsageError("--upstream-port must match the port in --upstream-base-url.")
    source = validate_codex_source(args.codex_config)
    models = validate_models(args.models)
    temp_root = validate_temp_root(args.temp_root)
    base_url = f"http://{LOOPBACK}:{DTOKEN_SERVICE_PORT}"

    # Pre-fetch: record existing request_ids and request count so only fresh,
    # attributable requests can satisfy this run. A missing or foreign
    # pre-fetch evidence payload is fail-closed: without a valid baseline,
    # pre-existing requests could satisfy the fresh window.
    pre_evidence = fetch_runtime_evidence(base_url, args.timeout)
    pre_export = pre_evidence.get("export")
    if not isinstance(pre_export, dict) or pre_export.get("source") != "dtoken-owned-runtime":
        return _fail_report(args, "runtime_unreachable", "pre-fetch evidence missing or not dtoken-owned-runtime")
    pre_ids = extract_request_ids(pre_export)
    # Choose ONE counter source up front (metrics text if it parses, else
    # export.stats) and use it on BOTH sides; mixing sources would let a post
    # absolute counter masquerade as a fresh delta. No baseline -> fail closed.
    pre_metrics_count = parse_metrics_count(pre_evidence.get("metrics_text"))
    pre_stats_total = export_stats_total(pre_export)
    use_metrics = pre_metrics_count is not None
    pre_count = pre_metrics_count if use_metrics else pre_stats_total
    if pre_count is None:
        return _fail_report(args, "evidence_missing", "pre-fetch counter baseline unavailable (no dtoken_requests_total and no export.stats total)")

    # Stage the source config: stat/hash before copy, then copy2 into the
    # temp CODEX_HOME as config.toml. auth.json is never copied or read.
    staging = Path(tempfile.mkdtemp(prefix="codex-ocxlocal-acceptance-", dir=temp_root))
    codex_home = staging / "CODEX_HOME"
    workdir = staging / "workdir"
    codex_home.mkdir(parents=True, exist_ok=True)
    workdir.mkdir(parents=True, exist_ok=True)
    config_dst = codex_home / "config.toml"
    source_mode = source_mode_str(source)
    source_fingerprint = file_sha256(source)
    try:
        shutil.copy2(source, config_dst)
    except OSError:
        shutil.rmtree(staging, ignore_errors=True)
        return _fail_report(args, "source_changed", "could not stage source config into temp CODEX_HOME")
    if file_sha256(config_dst) != source_fingerprint:
        shutil.rmtree(staging, ignore_errors=True)
        return _fail_report(args, "source_changed", "staged config copy does not match the fingerprinted source")

    started = time.monotonic()
    try:
        for model in models:
            ok = run_codex_exec(args.codex_bin, model, codex_home, workdir, ACCEPTANCE_PROMPT, args.timeout)
            if not ok:
                return _fail_report(args, "execution_failed", f"codex exec for model failed or timed out")
        fresh_window_seconds = time.monotonic() - started

        # Source must be unchanged for the whole run (hash + mode).
        if file_sha256(source) != source_fingerprint or source_mode_str(source) != source_mode:
            return _fail_report(args, "source_changed", "source config changed during the run")

        post_evidence = fetch_runtime_evidence(base_url, args.timeout)
        post_ids = extract_request_ids(post_evidence.get("export"))
        new_ids = post_ids - pre_ids
        if len(new_ids) < 2:
            return _fail_report(args, "stale_only", f"only {len(new_ids)} new request_id(s); need at least 2")
        if use_metrics:
            post_count = parse_metrics_count(post_evidence.get("metrics_text"))
        else:
            post_count = export_stats_total(post_evidence.get("export"))
        if post_count is None:
            return _fail_report(args, "evidence_missing", "post-fetch counter missing for the chosen baseline source")
        metrics_delta = post_count - pre_count
        if metrics_delta < 2:
            return _fail_report(args, "stale_only", f"fresh metrics delta {metrics_delta} < 2")

        report = build_report(
            args,
            post_evidence,
            only_ids=new_ids,
            source_fingerprint=source_fingerprint,
            source_mode=source_mode,
            fresh_window_seconds=fresh_window_seconds,
            expected_models=models,
            metrics_delta=metrics_delta,
        )
    finally:
        shutil.rmtree(staging, ignore_errors=True)

    return finalize_report(args, report)


def _fail_report(args: argparse.Namespace, failure_class: str, detail: str) -> int:
    report = _fail(base_report(args), failure_class, detail)
    return finalize_report(args, report)


def finalize_report(args: argparse.Namespace, report: dict[str, Any]) -> int:
    # Every real-mode artifact is bound to the candidate commit (read-only
    # git rev-parse HEAD). A report that would otherwise pass must fail closed
    # when the commit is unresolvable; blocked/failed artifacts just record
    # resolved=false. Schema-only mode never reaches this function.
    commit, resolved = git_head()
    report["candidate"] = {"commit": commit, "resolved": resolved}
    if not resolved and report["ok"]:
        report = _fail(report, "candidate_unbound", "candidate commit unresolvable (read-only git rev-parse HEAD failed or not a repository)")
    schema = load_schema()
    if schema is not None:
        errors = validate_report_against_schema(report, schema)
        report["schema_valid"] = not errors
        if errors and report["ok"]:
            report = _fail(report, "schema_invalid", "; ".join(errors)[:300])
    else:
        report["schema_valid"] = False
        report = _fail(report, "schema_artifact_missing", "report schema fixture missing under support/fixtures/")
    emit(report, args.report_out)
    return 0 if report["ok"] and report["schema_valid"] else 1


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.check_schema or args.dry_run:
            return run_schema_check(args)
        return run_routed(args)
    except UsageError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
