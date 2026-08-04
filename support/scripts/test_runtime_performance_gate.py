from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("run_runtime_performance_gate.py")
SPEC = importlib.util.spec_from_file_location("runtime_performance_gate", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RuntimePerformanceGateContractTests(unittest.TestCase):
    def test_percentile_uses_nearest_rank(self) -> None:
        values = [float(value) for value in range(1, 101)]
        self.assertEqual(MODULE.percentile(values, 0.95), 95.0)
        self.assertEqual(MODULE.percentile(values, 0.99), 99.0)

    def test_threshold_violation_fails_closed(self) -> None:
        metrics = {
            "request_count": 80,
            "p95_ms": 251.0,
            "p99_ms": 499.0,
            "failure_rate_percent": 0.0,
            "peak_rss_mib": 32.0,
            "error_path_observed": True,
            "client_disconnect_observed": True,
            "healthy_after_paths": True,
            "ledger_request_count": 82,
            "route_decision_count": 82,
        }
        thresholds = {
            "min_request_count": 64,
            "max_p95_ms": 250.0,
            "max_p99_ms": 500.0,
            "max_failure_rate_percent": 0.0,
            "max_peak_rss_mib": 256.0,
        }
        checks = MODULE.evaluate_checks(metrics, thresholds)
        failed = [check["id"] for check in checks if not check["ok"]]
        self.assertEqual(failed, ["latency_p95_ms"])

    def test_missing_path_evidence_fails_closed(self) -> None:
        metrics = {
            "request_count": 64,
            "p95_ms": 10.0,
            "p99_ms": 20.0,
            "failure_rate_percent": 0.0,
            "peak_rss_mib": 32.0,
            "error_path_observed": False,
            "client_disconnect_observed": False,
            "healthy_after_paths": True,
            "ledger_request_count": 64,
            "route_decision_count": 64,
        }
        thresholds = {
            "min_request_count": 64,
            "max_p95_ms": 250.0,
            "max_p99_ms": 500.0,
            "max_failure_rate_percent": 0.0,
            "max_peak_rss_mib": 256.0,
        }
        failed = [
            check["id"]
            for check in MODULE.evaluate_checks(metrics, thresholds)
            if not check["ok"]
        ]
        self.assertEqual(failed, ["error_path_observed", "client_disconnect_observed"])


if __name__ == "__main__":
    unittest.main()
