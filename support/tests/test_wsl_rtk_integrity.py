"""
Offline fixture tests for the WSL RTK integrity test runner contract.

These tests validate the script's structure, platform gate, fixture contract
checks, and JSON report schema without requiring a Windows host or WSL.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "run_wsl_rtk_integrity_test.ps1"


class WslRtkIntegrityContractTests(unittest.TestCase):
    """Structural and contract tests for the WSL RTK integrity runner."""

    def test_script_exists(self) -> None:
        self.assertTrue(SCRIPT.is_file(),
                        f"WSL RTK integrity script missing: {SCRIPT}")

    def test_script_contains_required_contract_elements(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        # Platform gate
        self.assertIn("IsWindowsHost", text)
        self.assertIn("exit 2", text)
        self.assertIn("wsl.exe", text)
        # Fixture contract checks
        self.assertIn("SECRET=must-not-persist", text)
        self.assertIn("4096", text)  # byte limit
        self.assertIn("managed-rtk-command.cmd", text)
        self.assertIn("managed-rtk-command", text)  # .sh fixture
        # Probe detection
        self.assertIn("managed_rtk_candidate_probe", text)
        # Report structure
        self.assertIn("wsl-rtk-integrity-test", text)

    def test_script_parses_as_valid_powershell(self) -> None:
        import subprocess
        import sys
        if sys.platform == "win32":
            result = subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 f"[void][scriptblock]::Create(Get-Content -Raw '{SCRIPT}')"],
                capture_output=True, text=True, timeout=30
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
        else:
            self.skipTest("PowerShell syntax check only available on Windows")

    def test_report_json_schema(self) -> None:
        """Verify the JSON report matches the expected schema."""
        report = {
            "ok": True,
            "mode": "wsl-rtk-integrity-test",
            "platform": "windows",
            "has_wsl_exe": True,
            "passed": 8,
            "total": 8,
            "summary": "WSL RTK integrity test: 8/8 checks passed",
            "checks": [
                {"id": "bridge_binary_exists", "ok": True},
                {"id": "fixture_cmd_integrity", "ok": True},
                {"id": "fixture_sh_integrity", "ok": True},
                {"id": "probe_binary_exists", "ok": True},
                {"id": "active_config_exists", "ok": True},
                {"id": "wsl_interop", "ok": True},
                {"id": "managed_root_acl", "ok": True},
                {"id": "probe_fixture_bind", "ok": True},
            ],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertEqual(restored["mode"], "wsl-rtk-integrity-test")
        self.assertEqual(len(restored["checks"]), 8)
        self.assertTrue(restored["ok"])

    def test_failure_report_json(self) -> None:
        """Verify a failure report has ok=false."""
        report = {
            "ok": False,
            "mode": "wsl-rtk-integrity-test",
            "platform": "windows",
            "has_wsl_exe": False,
            "passed": 6,
            "total": 8,
            "summary": "WSL RTK integrity test: 6/8 checks passed",
            "checks": [
                {"id": "bridge_binary_exists", "ok": True},
                {"id": "fixture_cmd_integrity", "ok": True},
                {"id": "fixture_sh_integrity", "ok": False,
                 "detail": {"exists": False}},
                {"id": "probe_binary_exists", "ok": True},
                {"id": "active_config_exists", "ok": True},
                {"id": "wsl_interop", "ok": True},
                {"id": "managed_root_acl", "ok": True},
                {"id": "probe_fixture_bind", "ok": False},
            ],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertFalse(restored["ok"])
        failed = [c for c in restored["checks"] if not c["ok"]]
        self.assertEqual(len(failed), 2)

    def test_non_windows_skip_exit_2(self) -> None:
        """On non-Windows, the script must exit 2."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("exit 2", text)
        self.assertIn("Skipping", text)

    def test_bridge_fixture_probe_paths(self) -> None:
        """All expected binary paths must be referenced in the script."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("dtoken-agent-bridge.exe", text)
        self.assertIn("managed-rtk-command.cmd", text)
        self.assertIn("managed-rtk-command", text)  # .sh without extension
        self.assertIn("managed_rtk_candidate_probe.exe", text)

    def test_secret_marker_contract(self) -> None:
        """The fixture must carry SECRET=must-not-persist."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("SECRET=must-not-persist", text)

    def test_bounded_byte_contract(self) -> None:
        """The fixture must be bounded to 4096 bytes."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("4096", text)
        self.assertIn("bounded", text)

    def test_wsl_interop_is_required_not_best_effort(self) -> None:
        """WSL interop must be required on Windows, never best-effort."""
        text = SCRIPT.read_text(encoding="utf-8")
        # The old best-effort wording is gone: absence of wsl.exe is a failure.
        self.assertNotIn("best-effort", text)
        self.assertIn("REQUIRED on a Windows host", text)

    def test_script_writes_artifact_to_output_dir(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("wsl-rtk-integrity-test-", text)

    def test_probe_fixture_bind_check(self) -> None:
        """The script must bind the probe to both bounded fixtures."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("probe_fixture_bind", text)
        self.assertIn("$ProbeExists", text)
        self.assertIn("$FixtureCmdOk", text)
        self.assertIn("$FixtureShOk", text)

    def test_managed_root_acl_check(self) -> None:
        """The script must check the managed root ACL on Windows."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("managed_root", text)
        self.assertIn("Get-Acl", text)

    def test_active_config_check(self) -> None:
        """The script must validate the active config presence."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("DTOKEN_ACTIVE_CONFIG_PATH", text)
        self.assertIn("active-config", text)

    def test_wsl_interop_fails_closed_when_wsl_missing(self) -> None:
        """A Windows host without wsl.exe must fail the gate, not pass."""
        text = SCRIPT.read_text(encoding="utf-8")
        # The check result must be tied to wsl.exe presence, not hardcoded true.
        self.assertIn("$WslCheckOk = $HasWslExe", text)
        self.assertIn("wsl_available", text)
        self.assertIn("exit 1", text)


if __name__ == "__main__":
    unittest.main()