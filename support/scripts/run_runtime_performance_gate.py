#!/usr/bin/env python3
"""Run a metadata-only performance gate against a real local dtoken-proxy.

The upstream is an in-process loopback HTTP server. Requests still traverse the
release-profile dtoken-proxy, its route decision, persistence, and response path.
No request or response bodies are written to the report.
"""

from __future__ import annotations

import argparse
import atexit
import concurrent.futures
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen


def env_int(name: str, default: int) -> int:
    value = os.environ.get(name)
    return default if value is None else int(value)


def env_float(name: str, default: float) -> float:
    value = os.environ.get(name)
    return default if value is None else float(value)


def percentile(values: list[float], quantile: float) -> float:
    if not values:
        raise ValueError("percentile requires at least one sample")
    ordered = sorted(values)
    index = max(0, math.ceil(quantile * len(ordered)) - 1)
    return ordered[index]


def reserve_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def process_rss_bytes(pid: int) -> tuple[int, str]:
    if os.name == "nt":
        class ProcessMemoryCounters(ctypes.Structure):
            _fields_ = [
                ("cb", ctypes.c_ulong),
                ("PageFaultCount", ctypes.c_ulong),
                ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t),
            ]

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        psapi = ctypes.WinDLL("psapi", use_last_error=True)
        kernel32.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        kernel32.OpenProcess.restype = ctypes.c_void_p
        kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
        kernel32.CloseHandle.restype = ctypes.c_int
        psapi.GetProcessMemoryInfo.argtypes = [
            ctypes.c_void_p,
            ctypes.POINTER(ProcessMemoryCounters),
            ctypes.c_ulong,
        ]
        psapi.GetProcessMemoryInfo.restype = ctypes.c_int
        handle = kernel32.OpenProcess(0x1000 | 0x0010, False, pid)
        if not handle:
            raise OSError(ctypes.get_last_error(), "OpenProcess failed")
        try:
            counters = ProcessMemoryCounters()
            counters.cb = ctypes.sizeof(counters)
            if not psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
                raise OSError(ctypes.get_last_error(), "GetProcessMemoryInfo failed")
            return int(counters.WorkingSetSize), "windows_working_set"
        finally:
            kernel32.CloseHandle(handle)

    proc_status = Path(f"/proc/{pid}/status")
    if proc_status.exists():
        for line in proc_status.read_text(encoding="utf-8").splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) * 1024, "linux_proc_vmrss"
        raise RuntimeError("VmRSS is unavailable in /proc status")

    output = subprocess.check_output(
        ["ps", "-o", "rss=", "-p", str(pid)], text=True, timeout=5
    ).strip()
    if not output:
        raise RuntimeError("ps did not return RSS")
    return int(output) * 1024, "macos_ps_rss"


class UpstreamState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.normal_requests = 0
        self.error_requests = 0
        self.disconnect_marker_requests = 0

    def record(self, kind: str) -> None:
        with self.lock:
            if kind == "normal":
                self.normal_requests += 1
            elif kind == "error":
                self.error_requests += 1
            elif kind == "disconnect":
                self.disconnect_marker_requests += 1


class GateThreadingHttpServer(ThreadingHTTPServer):
    def handle_error(self, request: Any, client_address: Any) -> None:
        error = sys.exc_info()[1]
        if isinstance(error, (BrokenPipeError, ConnectionResetError)):
            return
        super().handle_error(request, client_address)


def make_upstream_handler(state: UpstreamState) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def _send_json(self, status: int, value: dict[str, Any]) -> None:
            payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            try:
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def do_GET(self) -> None:  # noqa: N802
            if self.path == "/health":
                self._send_json(200, {"status": "healthy"})
            else:
                self._send_json(404, {"error": "not_found"})

        def do_POST(self) -> None:  # noqa: N802
            length = int(self.headers.get("content-length", "0"))
            body = self.rfile.read(length)
            if not self.path.endswith("/v1/chat/completions"):
                self._send_json(404, {"error": "not_found"})
                return
            try:
                request_value = json.loads(body or b"{}")
                messages = request_value.get("messages", [])
                marker_text = " ".join(str(item.get("content", "")) for item in messages)
            except (json.JSONDecodeError, AttributeError, TypeError):
                self._send_json(400, {"error": "invalid_request"})
                return

            if "dtoken-gate-upstream-error" in marker_text:
                state.record("error")
                self._send_json(503, {"error": {"type": "gate_expected_error"}})
                return
            if "dtoken-gate-client-disconnect" in marker_text:
                state.record("disconnect")
                time.sleep(0.75)
            else:
                state.record("normal")
                time.sleep(0.005)

            self._send_json(
                200,
                {
                    "id": "gate-response",
                    "object": "chat.completion",
                    "model": request_value.get("model", "gate/model"),
                    "choices": [
                        {
                            "index": 0,
                            "message": {"role": "assistant", "content": "runtime-performance-ok"},
                            "finish_reason": "stop",
                        }
                    ],
                    "usage": {"prompt_tokens": 8, "completion_tokens": 2, "total_tokens": 10},
                },
            )

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


def post_json(url: str, marker: str, timeout: float = 5.0) -> tuple[bool, float, int]:
    body = json.dumps(
        {
            "model": "gate/model",
            "messages": [{"role": "user", "content": marker}],
            "stream": False,
        },
        separators=(",", ":"),
    ).encode("utf-8")
    request = Request(url, data=body, headers={"content-type": "application/json"}, method="POST")
    started = time.perf_counter()
    try:
        with urlopen(request, timeout=timeout) as response:
            response_body = response.read()
            status = int(response.status)
        parsed = json.loads(response_body)
        content = parsed["choices"][0]["message"]["content"]
        ok = status == 200 and content == "runtime-performance-ok"
        return ok, (time.perf_counter() - started) * 1000.0, status
    except HTTPError as error:
        error.read()
        return False, (time.perf_counter() - started) * 1000.0, int(error.code)
    except Exception:
        return False, (time.perf_counter() - started) * 1000.0, 0


def wait_for_json(url: str, timeout_seconds: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        try:
            with urlopen(url, timeout=1.0) as response:
                if response.status == 200:
                    return json.loads(response.read())
        except Exception:
            time.sleep(0.1)
    raise RuntimeError("runtime did not become healthy before the deadline")


def send_client_disconnect(port: int) -> None:
    body = json.dumps(
        {
            "model": "gate/model",
            "messages": [{"role": "user", "content": "dtoken-gate-client-disconnect"}],
        },
        separators=(",", ":"),
    ).encode("utf-8")
    request = (
        f"POST /v1/chat/completions HTTP/1.1\r\n"
        f"Host: 127.0.0.1:{port}\r\n"
        "Content-Type: application/json\r\n"
        f"Content-Length: {len(body)}\r\n"
        "Connection: close\r\n\r\n"
    ).encode("ascii") + body
    with socket.create_connection(("127.0.0.1", port), timeout=2.0) as client:
        client.sendall(request)
        # Give the loopback listener enough time to accept and forward the
        # complete request, then close well before the delayed upstream reply.
        time.sleep(0.15)
        client.shutdown(socket.SHUT_RDWR)


def evaluate_checks(metrics: dict[str, Any], thresholds: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        {
            "id": "minimum_request_count",
            "ok": metrics["request_count"] >= thresholds["min_request_count"],
            "measured": metrics["request_count"],
            "limit": thresholds["min_request_count"],
        },
        {
            "id": "latency_p95_ms",
            "ok": metrics["p95_ms"] <= thresholds["max_p95_ms"],
            "measured": metrics["p95_ms"],
            "limit": thresholds["max_p95_ms"],
        },
        {
            "id": "latency_p99_ms",
            "ok": metrics["p99_ms"] <= thresholds["max_p99_ms"],
            "measured": metrics["p99_ms"],
            "limit": thresholds["max_p99_ms"],
        },
        {
            "id": "failure_rate_percent",
            "ok": metrics["failure_rate_percent"] <= thresholds["max_failure_rate_percent"],
            "measured": metrics["failure_rate_percent"],
            "limit": thresholds["max_failure_rate_percent"],
        },
        {
            "id": "runtime_peak_rss_mib",
            "ok": metrics["peak_rss_mib"] <= thresholds["max_peak_rss_mib"],
            "measured": metrics["peak_rss_mib"],
            "limit": thresholds["max_peak_rss_mib"],
        },
        {"id": "error_path_observed", "ok": bool(metrics["error_path_observed"])},
        {"id": "client_disconnect_observed", "ok": bool(metrics["client_disconnect_observed"])},
        {"id": "runtime_healthy_after_error_and_disconnect", "ok": bool(metrics["healthy_after_paths"])},
        {"id": "ledger_request_count", "ok": metrics["ledger_request_count"] >= metrics["request_count"]},
        {"id": "route_decision_count", "ok": metrics["route_decision_count"] >= metrics["request_count"]},
    ]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--proxy", required=True)
    parser.add_argument("--candidate-id", required=True)
    parser.add_argument("--candidate-revision", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--measured-requests", type=int, default=env_int("DTOKEN_PERF_MEASURED_REQUESTS", 48))
    parser.add_argument("--concurrency", type=int, default=env_int("DTOKEN_PERF_CONCURRENCY", 8))
    parser.add_argument("--soak-seconds", type=float, default=env_float("DTOKEN_PERF_SOAK_SECONDS", 5.0))
    parser.add_argument("--soak-concurrency", type=int, default=env_int("DTOKEN_PERF_SOAK_CONCURRENCY", 4))
    parser.add_argument("--max-p95-ms", type=float, default=env_float("DTOKEN_PERF_MAX_P95_MS", 250.0))
    parser.add_argument("--max-p99-ms", type=float, default=env_float("DTOKEN_PERF_MAX_P99_MS", 500.0))
    parser.add_argument(
        "--max-failure-rate-percent",
        type=float,
        default=env_float("DTOKEN_PERF_MAX_FAILURE_RATE_PERCENT", 0.0),
    )
    parser.add_argument("--max-peak-rss-mib", type=float, default=env_float("DTOKEN_PERF_MAX_PEAK_RSS_MIB", 256.0))
    parser.add_argument("--min-request-count", type=int, default=env_int("DTOKEN_PERF_MIN_REQUEST_COUNT", 64))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.measured_requests < args.concurrency or args.concurrency < 2:
        raise SystemExit("measured requests must cover at least one concurrent wave and concurrency must be >= 2")
    if args.soak_seconds < 1.0 or args.soak_concurrency < 2:
        raise SystemExit("short soak must run for at least one second with concurrency >= 2")

    proxy_path = Path(args.proxy).resolve()
    if not proxy_path.is_file():
        raise SystemExit(f"release proxy is missing: {proxy_path}")
    proxy_sha256 = hashlib.sha256(proxy_path.read_bytes()).hexdigest()

    upstream_port = reserve_loopback_port()
    proxy_port = reserve_loopback_port()
    state = UpstreamState()
    upstream = GateThreadingHttpServer(("127.0.0.1", upstream_port), make_upstream_handler(state))
    upstream_thread = threading.Thread(target=upstream.serve_forever, daemon=True)
    upstream_thread.start()

    rss_samples: list[int] = []
    rss_backend = "unavailable"
    stop_sampling = threading.Event()
    proxy_process: subprocess.Popen[bytes] | None = None
    temp_dir: Path | None = None

    def cleanup() -> None:
        stop_sampling.set()
        if proxy_process is not None and proxy_process.poll() is None:
            proxy_process.terminate()
            try:
                proxy_process.wait(timeout=5.0)
            except subprocess.TimeoutExpired:
                proxy_process.kill()
                proxy_process.wait(timeout=5.0)
        upstream.shutdown()
        upstream.server_close()
        if temp_dir is not None:
            shutil.rmtree(temp_dir, ignore_errors=True)

    atexit.register(cleanup)

    temp_dir = Path(tempfile.mkdtemp(prefix="dtoken-performance-gate-"))
    try:
        config_path = temp_dir / "runtime-config.json"
        data_dir = temp_dir / "runtime-data"
        data_dir.mkdir()
        config_path.write_text(
            json.dumps(
                {
                    "providers": [
                        {
                            "id": "gate-upstream",
                            "name": "Performance Gate Upstream",
                            "kind": "custom",
                            "protocol": "open_ai",
                            "base_url": f"http://127.0.0.1:{upstream_port}",
                            "api_key_ref": None,
                            "models": ["gate/model"],
                            "timeout_ms": 5000,
                            "healthcheck_path": "/health",
                        }
                    ],
                    "routes": [
                        {
                            "id": "performance-gate-route",
                            "name": "Performance Gate Route",
                            "input_protocol": "open_ai",
                            "instance_id": "performance-gate-instance",
                            "target_provider_id": "gate-upstream",
                            "fallback_provider_id": None,
                            "chain_mode": "direct",
                            "compression_policy": "balanced",
                            "bind_host": "127.0.0.1",
                            "local_port": proxy_port,
                        }
                    ],
                    "instances": [
                        {
                            "id": "performance-gate-instance",
                            "name": "Performance Gate Instance",
                            "agent": "OpenCode",
                            "route_id": "performance-gate-route",
                            "local_port": proxy_port,
                            "process_status": "stopped",
                            "log_path": "./logs/performance-gate.jsonl",
                        }
                    ],
                },
                separators=(",", ":"),
            ),
            encoding="utf-8",
        )

        stdout_path = temp_dir / "proxy.stdout"
        stderr_path = temp_dir / "proxy.stderr"
        creation_flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
        with stdout_path.open("wb") as stdout_file, stderr_path.open("wb") as stderr_file:
            proxy_process = subprocess.Popen(
                [
                    str(proxy_path),
                    "--port",
                    str(proxy_port),
                    "--config",
                    str(config_path),
                    "--data-dir",
                    str(data_dir),
                ],
                stdout=stdout_file,
                stderr=stderr_file,
                creationflags=creation_flags,
            )
            health_url = f"http://127.0.0.1:{proxy_port}/health"
            health = wait_for_json(health_url, 15.0)
            if health.get("runtime_identity") != "dtoken-runtime-local":
                raise RuntimeError("runtime identity mismatch")

            def sample_rss() -> None:
                nonlocal rss_backend
                while not stop_sampling.is_set():
                    try:
                        rss_value, rss_backend = process_rss_bytes(proxy_process.pid)
                        rss_samples.append(rss_value)
                    except Exception:
                        pass
                    stop_sampling.wait(0.05)

            sampler = threading.Thread(target=sample_rss, daemon=True)
            sampler.start()

            request_url = f"http://127.0.0.1:{proxy_port}/v1/chat/completions"
            for _ in range(5):
                ok, _, _ = post_json(request_url, "dtoken-gate-warmup")
                if not ok:
                    raise RuntimeError("release runtime warmup request failed")

            latencies: list[float] = []
            successes = 0
            with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
                futures = [
                    executor.submit(post_json, request_url, "dtoken-gate-measured")
                    for _ in range(args.measured_requests)
                ]
                for future in concurrent.futures.as_completed(futures):
                    ok, elapsed_ms, _ = future.result()
                    latencies.append(elapsed_ms)
                    successes += int(ok)

            soak_deadline = time.monotonic() + args.soak_seconds

            def soak_worker() -> list[tuple[bool, float]]:
                results: list[tuple[bool, float]] = []
                while time.monotonic() < soak_deadline:
                    ok, elapsed_ms, _ = post_json(request_url, "dtoken-gate-short-soak")
                    results.append((ok, elapsed_ms))
                    time.sleep(0.02)
                return results

            with concurrent.futures.ThreadPoolExecutor(max_workers=args.soak_concurrency) as executor:
                for worker_results in executor.map(lambda _index: soak_worker(), range(args.soak_concurrency)):
                    for ok, elapsed_ms in worker_results:
                        latencies.append(elapsed_ms)
                        successes += int(ok)

            _, _, error_status = post_json(request_url, "dtoken-gate-upstream-error")
            error_path_observed = error_status >= 400 and state.error_requests >= 1

            send_client_disconnect(proxy_port)
            disconnect_deadline = time.monotonic() + 3.0
            while state.disconnect_marker_requests < 1 and time.monotonic() < disconnect_deadline:
                time.sleep(0.05)
            time.sleep(0.9)
            post_path_health = wait_for_json(health_url, 3.0)

            stop_sampling.set()
            sampler.join(timeout=2.0)
            if not rss_samples:
                raise RuntimeError("runtime RSS measurement is unavailable on this platform")

            request_count = len(latencies)
            failures = request_count - successes
            request_log_path = data_dir / "request-logs.jsonl"
            ledger_request_count = 0
            if request_log_path.exists():
                ledger_request_count = sum(
                    1
                    for line in request_log_path.read_text(encoding="utf-8").splitlines()
                    if line.strip()
                )
            route_decision_path = data_dir / "route-decisions.jsonl"
            route_decision_count = 0
            if route_decision_path.exists():
                route_decision_count = sum(
                    1
                    for line in route_decision_path.read_text(encoding="utf-8").splitlines()
                    if line.strip()
                )

            thresholds = {
                "max_p95_ms": args.max_p95_ms,
                "max_p99_ms": args.max_p99_ms,
                "max_failure_rate_percent": args.max_failure_rate_percent,
                "max_peak_rss_mib": args.max_peak_rss_mib,
                "min_request_count": args.min_request_count,
            }
            metrics = {
                "request_count": request_count,
                "success_count": successes,
                "failure_count": failures,
                "failure_rate_percent": round((failures / request_count) * 100.0, 4),
                "p50_ms": round(percentile(latencies, 0.50), 3),
                "p95_ms": round(percentile(latencies, 0.95), 3),
                "p99_ms": round(percentile(latencies, 0.99), 3),
                "max_ms": round(max(latencies), 3),
                "peak_rss_mib": round(max(rss_samples) / (1024 * 1024), 3),
                "final_rss_mib": round(rss_samples[-1] / (1024 * 1024), 3),
                "rss_sample_count": len(rss_samples),
                "ledger_request_count": ledger_request_count,
                "route_decision_count": route_decision_count,
                "error_path_observed": error_path_observed,
                "client_disconnect_observed": state.disconnect_marker_requests >= 1,
                "healthy_after_paths": post_path_health.get("status") == "healthy",
            }
            checks = evaluate_checks(metrics, thresholds)
            failed_checks = [check["id"] for check in checks if not check["ok"]]
            report = {
                "schema_version": 1,
                "scope": "release-runtime-performance-gate",
                "generated_at": datetime.now(timezone.utc).isoformat(),
                "candidate": {
                    "candidate_id": args.candidate_id,
                    "git_head": args.candidate_revision,
                },
                "platform": {
                    "system": platform.system(),
                    "release": platform.release(),
                    "machine": platform.machine(),
                    "python": platform.python_version(),
                },
                "runtime": {
                    "profile": "release",
                    "binary_sha256": proxy_sha256,
                    "runtime_identity": health.get("runtime_identity"),
                    "binding": "127.0.0.1",
                    "route": "performance-gate-route",
                    "provider": "gate-upstream",
                    "rss_backend": rss_backend,
                },
                "workload": {
                    "measured_requests_configured": args.measured_requests,
                    "concurrency": args.concurrency,
                    "short_soak_seconds": args.soak_seconds,
                    "short_soak_concurrency": args.soak_concurrency,
                    "warmup_requests_excluded": 5,
                    "expected_error_requests": state.error_requests,
                    "client_disconnect_requests": state.disconnect_marker_requests,
                },
                "metrics": metrics,
                "measurement": {
                    "latency_clock": "monotonic_perf_counter",
                    "percentile_method": "nearest_rank",
                    "rss_sample_interval_ms": 50,
                    "success_contract": "http_200_and_expected_metadata_marker",
                },
                "thresholds": thresholds,
                "checks": checks,
                "gate": {"ok": not failed_checks, "failed_checks": failed_checks},
                "privacy": {
                    "metadata_only": True,
                    "raw_request_stored": False,
                    "raw_response_stored": False,
                    "process_output_stored": False,
                },
                "limitations": [
                    "RSS covers the dtoken-proxy process only, not the desktop WebView process group.",
                    "Client disconnect proves bounded survival and health; it does not claim upstream transport cancellation propagation.",
                    "This short soak is a release regression gate, not the separate 30-minute or 24-hour endurance certification.",
                ],
            }
            output_path = Path(args.output)
            output_path.parent.mkdir(parents=True, exist_ok=True)
            output_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

        if proxy_process.poll() is None:
            proxy_process.terminate()
            try:
                proxy_process.wait(timeout=5.0)
            except subprocess.TimeoutExpired:
                proxy_process.kill()
                proxy_process.wait(timeout=5.0)

    finally:
        cleanup()
        atexit.unregister(cleanup)
    print(json.dumps(report, separators=(",", ":"), sort_keys=True))
    return 0 if report["gate"]["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
