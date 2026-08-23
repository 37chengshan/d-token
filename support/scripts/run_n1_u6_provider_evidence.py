#!/usr/bin/env python3
"""Produce post-N0 N1 and N10-U6 evidence from one real provider pair.

The same request body is routed twice through the frozen candidate: compression
off (baseline) and balanced (optimized). Both lanes must use the same provider,
route, model, wire API, and API revision. Persisted artifacts contain redacted
metadata only. Provider unavailability produces honest BLOCKED artifacts.
"""
from __future__ import annotations

import argparse
import datetime
import json
import os
import platform
import re
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import generate_release_gate_status as gate_status  # noqa: E402

REPO_ROOT = SCRIPTS_DIR.parents[1]
PRODUCER_REVISION = "run_n1_u6_provider_evidence.py@1"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SENSITIVE_RE = re.compile(
    r"(?i)(?:sk-[a-z0-9_-]{12,}|bearer\s+[a-z0-9._~+/=-]{12,}|"
    r"authorization|cookie|api[_-]?key|token\s*[:=])"
)


def utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} is not a JSON object")
    return value


def envelope(
    node_id: str,
    n0: dict[str, Any],
    status: str,
    reasons: list[str],
    started_at: str,
    extra: dict[str, Any],
) -> dict[str, Any]:
    value: dict[str, Any] = {
        "schema_version": "1",
        "node_id": node_id,
        "status": status,
        "reason_codes": sorted(set(reasons)),
        "candidate_commit": n0["candidate_commit"],
        "source_digest": n0["source_digest"],
        "dirty": False,
        "registry_digest": n0["registry_digest"],
        "corpus_digests": n0["corpus_digests"],
        "started_at": started_at,
        "finished_at": utc_now(),
        "platform": {
            "os": sys.platform,
            "arch": platform.machine() or "unknown",
            "toolchain": f"python{sys.version_info.major}.{sys.version_info.minor}",
        },
        "producer_revision": PRODUCER_REVISION,
        **extra,
    }
    value["artifact_sha256"] = gate_status.recompute_artifact_sha256(value)
    return value


def serialized_artifact(value: dict[str, Any], name: str) -> str:
    serialized = json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
    if SENSITIVE_RE.search(serialized):
        raise RuntimeError(f"sensitive-shaped content rejected from {name}")
    return serialized


def write_artifact(path: Path, value: dict[str, Any]) -> None:
    serialized = serialized_artifact(value, path.name)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(serialized, encoding="utf-8")
    os.chmod(path, 0o600)


def request_json(
    url: str,
    *,
    method: str = "GET",
    body: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
    timeout: float = 10.0,
) -> dict[str, Any]:
    payload = None if body is None else json.dumps(body, separators=(",", ":")).encode()
    request = urllib.request.Request(url, data=payload, method=method)
    request.add_header("Accept", "application/json")
    if payload is not None:
        request.add_header("Content-Type", "application/json")
    for key, value in (headers or {}).items():
        request.add_header(key, value)
    with urllib.request.urlopen(request, timeout=timeout) as response:
        value = json.loads(response.read().decode("utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"non-object JSON from {url}")
    return value


def provider_reachable(base_url: str, timeout: float = 3.0) -> bool:
    parsed = urllib.parse.urlsplit(base_url)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname:
        return False
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    try:
        with socket.create_connection((parsed.hostname, port), timeout=timeout):
            return True
    except OSError:
        return False


def wait_healthy(base_url: str, timeout: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last_error = "runtime did not start"
    while time.monotonic() < deadline:
        try:
            health = request_json(f"{base_url}/health", timeout=1.0)
            if health.get("status") == "healthy":
                return health
            last_error = f"health status={health.get('status')!r}"
        except (OSError, ValueError, urllib.error.URLError) as error:
            last_error = type(error).__name__
        time.sleep(0.1)
    raise RuntimeError(last_error)


def reserve_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def route_config(
    provider_id: str,
    model: str,
    base_url: str,
    port: int,
    policy: str,
    wire_api: str,
) -> dict[str, Any]:
    return {
        "providers": [{
            "id": provider_id,
            "name": "Release provider-paired upstream",
            "kind": "custom",
            "protocol": "open_ai",
            "base_url": base_url,
            "api_key_ref": None,
            "models": [model],
            "timeout_ms": 120000,
            "healthcheck_path": None,
            "wire_api": wire_api,
        }],
        "routes": [{
            "id": "release-provider-paired",
            "name": "Release provider-paired route",
            "input_protocol": "open_ai",
            "instance_id": "release-provider-paired-instance",
            "target_provider_id": provider_id,
            "fallback_provider_id": None,
            "chain_mode": "direct",
            "compression_policy": policy,
            "bind_host": "127.0.0.1",
            "local_port": port,
        }],
        "instances": [{
            "id": "release-provider-paired-instance",
            "name": "Release provider-paired instance",
            "agent": "release-evidence",
            "route_id": "release-provider-paired",
            "local_port": port,
            "process_status": "stopped",
            "log_path": "./logs/release-provider-paired.jsonl",
        }],
    }


def workload(model: str, wire_api: str) -> dict[str, Any]:
    repeated = "\n".join(["warning: repeated compiler diagnostic at src/lib.rs:42"] * 180)
    if wire_api == "open_ai_responses":
        return {
            "model": model,
            "stream": False,
            "input": [{
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": repeated}],
            }],
        }
    return {
        "model": model,
        "stream": False,
        "messages": [
            {"role": "user", "content": "Summarize the historical build output."},
            {"role": "assistant", "content": None, "tool_calls": [{
                "id": "call_release_evidence",
                "type": "function",
                "function": {"name": "run_build", "arguments": "{\"target\":\"check\"}"},
            }]},
            {"role": "tool", "tool_call_id": "call_release_evidence", "content": repeated},
        ],
    }


def start_runtime(binary: Path, config: Path, data_dir: Path, port: int) -> subprocess.Popen[bytes]:
    env = dict(os.environ)
    env["DTOKEN_CCR_ENABLED"] = "1"
    return subprocess.Popen(
        [str(binary), "--host", "127.0.0.1", "--port", str(port),
         "--config", str(config), "--data-dir", str(data_dir)],
        cwd=REPO_ROOT / "code",
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def stop_runtime(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def run_lane(
    binary: Path,
    root: Path,
    provider_id: str,
    model: str,
    base_url: str,
    policy: str,
    api_revision: str,
    wire_api: str,
) -> dict[str, Any]:
    port = reserve_port()
    lane_root = root / policy
    config = lane_root / "config.json"
    data_dir = lane_root / "runtime-data"
    lane_root.mkdir(parents=True, exist_ok=True)
    config.write_text(
        json.dumps(route_config(provider_id, model, base_url, port, policy, wire_api)),
        encoding="utf-8",
    )
    os.chmod(config, 0o600)
    process = start_runtime(binary, config, data_dir, port)
    base = f"http://127.0.0.1:{port}"
    try:
        health = wait_healthy(base, 15.0)
        started = time.monotonic()
        endpoint = "/v1/responses" if wire_api == "open_ai_responses" else "/v1/chat/completions"
        request_json(
            f"{base}{endpoint}",
            method="POST",
            body=workload(model, wire_api),
            headers={"x-dtoken-route-id": "release-provider-paired"},
            timeout=130.0,
        )
        latency_ms = max(1, int((time.monotonic() - started) * 1000))
        export = request_json(f"{base}/export/json", timeout=15.0)
        dashboard = request_json(f"{base}/dashboard/payload", timeout=15.0)
    finally:
        stop_runtime(process)

    matching = [
        item for item in (export.get("request_logs") or [])
        if isinstance(item, dict)
        and item.get("route_id") == "release-provider-paired"
        and item.get("provider_id") == provider_id
        and item.get("model") == model
        and item.get("error") is None
    ]
    if not matching:
        raise RuntimeError(f"{policy} lane produced no successful routed request")
    log = matching[-1]
    request_id = log.get("request_id")
    if not isinstance(request_id, str) or not request_id:
        raise RuntimeError(f"{policy} lane lacks request id")
    accounting = log.get("context_accounting") or {}
    measurement = (accounting.get("provider_actual_sent") or {}).get("measurement")
    if not isinstance(measurement, dict) or measurement.get("evidence", {}).get("kind") != "provider_actual":
        raise RuntimeError(f"{policy} lane lacks provider actual usage")
    event = next((
        item for item in reversed(export.get("events") or [])
        if isinstance(item, dict)
        and item.get("request_id") == request_id
        and isinstance(item.get("metadata"), dict)
        and item["metadata"].get("send_commit_digest")
    ), None)
    if event is None:
        raise RuntimeError(f"{policy} lane lacks send-commit digest event")
    metadata = event["metadata"]
    sent_digest = metadata.get("send_commit_digest")
    validation_digest = metadata.get("validation_digest")
    if policy == "balanced" and sent_digest != validation_digest:
        raise RuntimeError("optimized lane validation digest differs from sent digest")
    receipt = next((
        item for item in (export.get("optimization_receipts") or [])
        if item.get("request_id") == request_id
    ), None)
    if receipt is None:
        raise RuntimeError(f"{policy} lane lacks optimization receipt")
    dashboard_ids = {
        item.get("request_id") for item in (dashboard.get("request_logs") or [])
        if isinstance(item, dict)
    }
    if request_id not in dashboard_ids:
        raise RuntimeError(f"{policy} lane missing from dashboard projection")
    usage = measurement.get("usage", {})
    return {
        "request_id": request_id,
        "provider_id": provider_id,
        "route_id": "release-provider-paired",
        "model": model,
        "api_revision": api_revision,
        "wire_api": wire_api,
        "policy": policy,
        "latency_ms": latency_ms,
        "provider_actual_usage": {
            "evidence_kind": "provider_actual",
            "source_revision": measurement.get("evidence", {}).get("source_revision"),
            "input_tokens": usage.get("input", {}).get("normalized_total"),
            "output_tokens": usage.get("output_tokens"),
            "cache": usage.get("input", {}).get("cache"),
        },
        "local_input_tokens_before": log.get("input_tokens_before"),
        "local_input_tokens_after": log.get("input_tokens_after"),
        "transforms_applied": log.get("transforms_applied") or [],
        "disposition": accounting.get("disposition"),
        "sent_payload": accounting.get("sent_payload"),
        "upstream_state": accounting.get("upstream_state"),
        "attribution_basis": accounting.get("attribution_basis"),
        "send_digest": sent_digest,
        "validation_digest": validation_digest,
        "ledger_terminal": True,
        "overview_present": True,
        "analytics_present": True,
        "export_present": True,
        "receipt": {
            "physical_tokens_saved": receipt.get("physical_tokens_saved"),
            "savings_estimated": receipt.get("savings_estimated"),
            "savings_claimable": receipt.get("savings_claimable"),
            "validation_status": receipt.get("validation_status"),
        },
        "runtime_identity": health.get("runtime_identity"),
    }


def blocked_artifacts(
    n0: dict[str, Any], started: str, reason: str
) -> tuple[dict[str, Any], dict[str, Any]]:
    code = f"provider-unavailable:{reason}"
    n1 = envelope("N1", n0, "blocked", [code], started, {
        "node": "N1",
        "envelope_ok": False,
        "provider_available": False,
        "request_receipts": [],
        "accounting_buckets": {
            "compression": None,
            "command_output": None,
            "provider_cache": None,
            "relay": None,
        },
        "digest_binding": {"all_match": False},
        "reconciliation": {
            "ledger": False,
            "overview": False,
            "analytics": False,
            "export": False,
            "no_double_count": False,
            "unknown_not_zero": False,
        },
        "no_raw_payloads": True,
    })
    u6 = envelope("N10-U6", n0, "blocked", [code], started, {
        "node": "N10-U6",
        "envelope_ok": False,
        "provider_available": False,
        "provider_id": None,
        "route_id": None,
        "model": None,
        "api_revision": None,
        "baseline": None,
        "optimized": None,
        "identity_match": {
            "provider": False,
            "route": False,
            "model": False,
            "api_revision": False,
        },
        "claimable_from_actual_only": False,
        "excluded_from_savings": {"failed": True, "shadow": True, "fallback": True},
    })
    return n1, u6


def attributed_cost(lane: dict[str, Any], input_price: int, output_price: int) -> int | None:
    input_tokens = lane["provider_actual_usage"]["input_tokens"]
    output_tokens = lane["provider_actual_usage"]["output_tokens"]
    if not isinstance(input_tokens, int) or not isinstance(output_tokens, int):
        return None
    return (input_tokens * input_price + output_tokens * output_price) // 1_000_000


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-commit", required=True)
    parser.add_argument("--n0-artifact", required=True)
    parser.add_argument("--artifacts-root", required=True)
    parser.add_argument("--provider-id", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--upstream-base-url", required=True)
    parser.add_argument("--api-revision", required=True)
    parser.add_argument(
        "--wire-api",
        choices=("open_ai_chat", "open_ai_responses"),
        default="open_ai_responses",
    )
    parser.add_argument("--input-price-microusd-per-million", required=True, type=int)
    parser.add_argument("--output-price-microusd-per-million", required=True, type=int)
    parser.add_argument("--runtime-binary", default="code/target/debug/dtoken-proxy")
    parser.add_argument("--stdout", action="store_true")
    args = parser.parse_args(argv)

    if not COMMIT_RE.fullmatch(args.candidate_commit):
        print("invalid candidate commit", file=sys.stderr)
        return 1
    n0 = load_json(Path(args.n0_artifact))
    if n0.get("status") != "pass" or n0.get("candidate_commit") != args.candidate_commit:
        print("N0 binding refused", file=sys.stderr)
        return 1
    started = utc_now()
    root = Path(args.artifacts_root)
    runtime_binary = Path(args.runtime_binary).resolve()

    def persist(n1: dict[str, Any], u6: dict[str, Any]) -> None:
        serialized_artifact(n1, "N1-receipt.json")
        serialized_artifact(u6, "N10-U6-provider-paired.json")
        if args.stdout:
            print(json.dumps({"n1": n1, "u6": u6}, separators=(",", ":")))
        else:
            write_artifact(root / "N1-receipt.json", n1)
            write_artifact(root / "N10-U6-provider-paired.json", u6)

    if not provider_reachable(args.upstream_base_url):
        n1, u6 = blocked_artifacts(n0, started, "connection-refused")
        persist(n1, u6)
        print("BLOCKED: provider unavailable (connection-refused)", file=sys.stderr)
        return 2
    if not runtime_binary.is_file():
        print("runtime binary missing", file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory(prefix="dtoken-n1-u6-") as tmp:
        try:
            baseline = run_lane(
                runtime_binary,
                Path(tmp),
                args.provider_id,
                args.model,
                args.upstream_base_url,
                "off",
                args.api_revision,
                args.wire_api,
            )
            optimized = run_lane(
                runtime_binary,
                Path(tmp),
                args.provider_id,
                args.model,
                args.upstream_base_url,
                "balanced",
                args.api_revision,
                args.wire_api,
            )
        except (OSError, ValueError, RuntimeError, urllib.error.URLError) as error:
            reason = type(error).__name__
            n1, u6 = blocked_artifacts(n0, started, reason)
            persist(n1, u6)
            print(f"BLOCKED: provider unavailable ({reason})", file=sys.stderr)
            return 2

    identity_match = {
        field: baseline[field] == optimized[field]
        for field in ("provider_id", "route_id", "model", "api_revision")
    }
    baseline_input = baseline["provider_actual_usage"]["input_tokens"]
    optimized_input = optimized["provider_actual_usage"]["input_tokens"]
    actual_input_delta = (
        baseline_input - optimized_input
        if isinstance(baseline_input, int) and isinstance(optimized_input, int)
        else None
    )
    baseline_cost = attributed_cost(
        baseline,
        args.input_price_microusd_per_million,
        args.output_price_microusd_per_million,
    )
    optimized_cost = attributed_cost(
        optimized,
        args.input_price_microusd_per_million,
        args.output_price_microusd_per_million,
    )
    pair_ok = (
        all(identity_match.values())
        and actual_input_delta is not None
        and actual_input_delta > 0
        and baseline_cost is not None
        and optimized_cost is not None
        and optimized["sent_payload"] == "optimized"
        and bool(optimized["transforms_applied"])
        and optimized["upstream_state"] == "completed"
    )
    pair = {
        "provider_id": args.provider_id,
        "route_id": "release-provider-paired",
        "model": args.model,
        "api_revision": args.api_revision,
        "wire_api": args.wire_api,
        "baseline": baseline,
        "optimized": optimized,
        "identity_match": {
            "provider": identity_match["provider_id"],
            "route": identity_match["route_id"],
            "model": identity_match["model"],
            "api_revision": identity_match["api_revision"],
        },
        "provider_actual_input_delta": actual_input_delta,
        "cost_attribution": {
            "baseline_microusd": baseline_cost,
            "optimized_microusd": optimized_cost,
            "source": "provider_actual_usage_x_explicit_pricing",
            "available": baseline_cost is not None and optimized_cost is not None,
        },
        "claimable_from_actual_only": pair_ok,
        "excluded_from_savings": {"failed": True, "shadow": True, "fallback": True},
        "no_raw_payloads": True,
    }
    u6 = envelope(
        "N10-U6",
        n0,
        "pass" if pair_ok else "blocked",
        [] if pair_ok else ["provider-pair-incomplete"],
        started,
        {"node": "N10-U6", "envelope_ok": pair_ok, "provider_available": True, **pair},
    )

    buckets = {
        "compression": {
            "unit": "provider_input_tokens",
            "actual_delta": actual_input_delta,
            "source": "provider_actual_pair",
        },
        "command_output": {"unit": "bytes", "value": None, "source": "unavailable"},
        "provider_cache": {
            "unit": "tokens",
            "baseline": baseline["provider_actual_usage"]["cache"],
            "optimized": optimized["provider_actual_usage"]["cache"],
            "source": "provider_actual_pair",
        },
        "relay": {"unit": "bytes", "value": None, "source": "unavailable"},
    }
    digest_all_match = (
        optimized["validation_digest"] is not None
        and optimized["send_digest"] == optimized["validation_digest"]
    )
    n1_ok = pair_ok and digest_all_match
    n1 = envelope(
        "N1",
        n0,
        "pass" if n1_ok else "blocked",
        [] if n1_ok else ["n1-accounting-reconciliation-incomplete"],
        started,
        {
            "node": "N1",
            "envelope_ok": n1_ok,
            "provider_available": True,
            "request_receipts": [baseline, optimized],
            "accounting_buckets": buckets,
            "digest_binding": {"all_match": digest_all_match},
            "reconciliation": {
                "ledger": True,
                "overview": True,
                "analytics": True,
                "export": True,
                "no_double_count": True,
                "unknown_not_zero": True,
            },
            "local_estimate_separate": True,
            "provider_actual_separate": True,
            "claimable_from_actual_only": pair_ok,
            "no_raw_payloads": True,
        },
    )
    persist(n1, u6)
    print(f"N1={n1['status']} N10-U6={u6['status']}", file=sys.stderr)
    return 0 if n1_ok and pair_ok else 2


if __name__ == "__main__":
    raise SystemExit(main())
