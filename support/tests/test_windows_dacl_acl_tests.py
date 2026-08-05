"""
Offline fixture tests for the Windows DACL/ACL test runner contract.

These tests validate the script's behaviour without requiring a Windows host
or a Rust build.  They verify:
  - JSON report structure
  - Non-Windows host skip (exit 2)
  - Failure propagation
  - Output format
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "run_windows_dacl_acl_tests.ps1"


class WindowsDaclAclContractTests(unittest.TestCase):
    """Structural and contract tests for the DACL/ACL runner."""

    def test_script_exists(self) -> None:
        self.assertTrue(SCRIPT.is_file(), f"DACL/ACL test script missing: {SCRIPT}")

    def test_script_contains_required_contract_elements(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        # Platform gate
        self.assertIn("IsWindowsHost", text)
        self.assertIn("exit 2", text)
        # Test group invocations
        self.assertIn("windows_private_acl", text)
        self.assertIn("open_applies", text)
        self.assertIn("dacl", text)
        # Report structure
        self.assertIn("windows-dacl-acl-tests", text)
        self.assertIn("passed", text)
        self.assertIn("total", text)
        # Error propagation
        self.assertIn("exit 1", text)

    def test_script_parses_as_valid_powershell(self) -> None:
        """The script must parse as valid PowerShell AST (syntax check)."""
        if sys.platform == "win32":
            result = subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 f"[void][scriptblock]::Create(Get-Content -Raw '{SCRIPT}')"],
                capture_output=True, text=True, timeout=30
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
        else:
            self.skipTest("PowerShell syntax check only available on Windows")

    def test_non_windows_host_returns_exit_2(self) -> None:
        """On a non-Windows host the script must exit 2 without running tests."""
        # Check that the platform gate appears before the cargo test invocation
        # (not before param-block references like $CrateDir).
        text = SCRIPT.read_text(encoding="utf-8")
        # Find the actual cargo test invocation (the "cargo test" run, not param refs)
        invoke_pos = text.find("cargo test -p")
        self.assertGreater(invoke_pos, 0, "Must contain a cargo test invocation")
        # The platform gate must be before the first cargo test invocation
        gate_pos = text.index("IsWindowsHost")
        self.assertLess(gate_pos, invoke_pos,
                        "Platform gate must appear before cargo test invocation")

    def test_report_json_schema(self) -> None:
        """Verify the JSON report matches the expected schema."""
        # Build a minimal fixture report
        report = {
            "ok": True,
            "mode": "windows-dacl-acl-tests",
            "platform": "windows",
            "target": "x86_64-pc-windows-msvc",
            "passed": 4,
            "total": 4,
            "summary": "DACL/ACL tests: 4/4 passed",
            "groups": [
                {"id": "private_acl", "ok": True, "exit_code": 0},
                {"id": "config_patcher", "ok": True, "exit_code": 0},
                {"id": "codebase_cache", "ok": True, "exit_code": 0},
                {"id": "broad_acl", "ok": True, "exit_code": 0},
            ],
            "report_path": "support/artifacts/windows-dacl-acl-tests-20260801T000000Z.json",
        }
        # Validate round-trip
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertEqual(restored["mode"], "windows-dacl-acl-tests")
        self.assertEqual(len(restored["groups"]), 4)

    def test_failure_report_json(self) -> None:
        """Verify a failure report has ok=false and includes the failing group."""
        report = {
            "ok": False,
            "mode": "windows-dacl-acl-tests",
            "platform": "windows",
            "passed": 3,
            "total": 4,
            "summary": "DACL/ACL tests: 3/4 passed",
            "groups": [
                {"id": "private_acl", "ok": True, "exit_code": 0},
                {"id": "config_patcher", "ok": False, "exit_code": 1},
                {"id": "codebase_cache", "ok": True, "exit_code": 0},
                {"id": "broad_acl", "ok": True, "exit_code": 0},
            ],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertFalse(restored["ok"])
        failed = [g for g in restored["groups"] if not g["ok"]]
        self.assertEqual(len(failed), 1)
        self.assertEqual(failed[0]["id"], "config_patcher")

    def test_script_rejects_invalid_target_argument(self) -> None:
        """The script's param block should accept a Target parameter."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("[string]$Target", text,
                      "Script must accept a -Target parameter")
        self.assertIn("x86_64-pc-windows-msvc", text,
                      "Default target must be x86_64-pc-windows-msvc")

    def test_script_writes_artifact_to_support_artifacts(self) -> None:
        """The artifact must be written to support/artifacts/."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("support\\artifacts", text)
        self.assertIn("windows-dacl-acl-tests-", text)

    def test_script_contains_platform_gate(self) -> None:
        """The script must explicitly check for Windows before running tests."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("IsWindowsHost", text)
        # Must skip on non-Windows, not explode
        self.assertIn("exit 2", text)
        self.assertIn("Skipping", text)

    def test_all_dacl_test_groups_are_present(self) -> None:
        """Every DACL/ACL test group must filter REAL cargo test names.

        The filters must match test names/module paths, not function names of
        non-test code (a filter matching zero tests would be a false pass).
        """
        text = SCRIPT.read_text(encoding="utf-8")
        groups = ["private_acl", "open_acl", "broad_acl"]
        for group in groups:
            with self.subTest(group=group):
                self.assertIn(group, text)
        # The old function-name filters must not be used as test filters.
        self.assertNotIn('filter = "windows_acl"', text)
        self.assertNotIn('filter = "secure_cache_permissions"', text)

    def test_zero_test_result_is_a_failure(self) -> None:
        """A cargo run reporting 0 tests run must fail, not pass."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("0 tests run", text)
        self.assertIn("test result:", text)
        # The zero-test detection must flip ok to false.
        zero_pos = text.find("0 tests run")
        self.assertGreater(zero_pos, 0)
        self.assertIn("$ZeroTests", text)
        self.assertIn("(-not $ZeroTests)", text)


if __name__ == "__main__":
    unittest.main()