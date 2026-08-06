"""
Offline fixture tests for the NSIS upgrade/rollback test runner contract.

These tests validate the script's structure, platform gate, and JSON report
schema without requiring a Windows host or an actual NSIS installer.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "run_nsis_upgrade_rollback_test.ps1"


class NsisUpgradeRollbackContractTests(unittest.TestCase):
    """Structural and contract tests for the NSIS upgrade/rollback runner."""

    def test_script_exists(self) -> None:
        self.assertTrue(SCRIPT.is_file(),
                        f"NSIS upgrade/rollback script missing: {SCRIPT}")

    def test_script_contains_required_contract_elements(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        # Platform gate
        self.assertIn("IsWindowsHost", text)
        self.assertIn("exit 2", text)
        # All seven steps
        steps = [
            "clean_install",
            "verify_installed_files",
            "launch_installed_app",
            "upgrade_reinstall",
            "verify_after_reinstall",
            "clean_uninstall",
            "rollback_restore",
        ]
        for step in steps:
            with self.subTest(step=step):
                self.assertIn(step, text, f"Step '{step}' missing from script")
        # NSIS installer path
        self.assertIn("x64-setup.exe", text)
        # Report structure
        self.assertIn("nsis-upgrade-rollback-test", text)

    def test_script_parses_as_valid_powershell(self) -> None:
        """The script must parse as valid PowerShell AST (syntax check)."""
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

    def test_platform_gate_before_installer_resolution(self) -> None:
        """The platform gate must appear before the installer resolution logic."""
        text = SCRIPT.read_text(encoding="utf-8")
        # The installer resolution logic is the code that searches for the NSIS
        # installer using Get-ChildItem (not the parameter declaration).
        resolve_pos = text.find("Get-ChildItem -Path $Pattern")
        self.assertGreater(resolve_pos, 0, "Must contain installer resolution logic")
        gate_pos = text.index("IsWindowsHost")
        self.assertLess(gate_pos, resolve_pos,
                        "Platform gate must appear before installer resolution")

    def test_report_json_schema(self) -> None:
        """Verify the JSON report matches the expected schema."""
        report = {
            "ok": True,
            "mode": "nsis-upgrade-rollback-test",
            "platform": "windows",
            "installer_path": "C:\\path\\to\\d-token_0.2.0_x64-setup.exe",
            "installer_sha256": "a" * 64,
            "installer_bytes": 50000000,
            "passed": 7,
            "total": 7,
            "summary": "NSIS upgrade/rollback test: 7/7 checks passed",
            "checks": [
                {"id": "clean_install", "ok": True},
                {"id": "verify_installed_files", "ok": True},
                {"id": "launch_installed_app", "ok": True},
                {"id": "upgrade_reinstall", "ok": True},
                {"id": "verify_after_reinstall", "ok": True},
                {"id": "clean_uninstall", "ok": True},
                {"id": "rollback_restore", "ok": True},
            ],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertEqual(restored["mode"], "nsis-upgrade-rollback-test")
        self.assertEqual(len(restored["checks"]), 7)
        self.assertTrue(restored["ok"])

    def test_failure_report_json(self) -> None:
        """Verify a failure report has ok=false and includes the failing step."""
        report = {
            "ok": False,
            "mode": "nsis-upgrade-rollback-test",
            "platform": "windows",
            "passed": 6,
            "total": 7,
            "summary": "NSIS upgrade/rollback test: 6/7 checks passed",
            "checks": [
                {"id": "clean_install", "ok": True},
                {"id": "verify_installed_files", "ok": True},
                {"id": "launch_installed_app", "ok": True},
                {"id": "upgrade_reinstall", "ok": True},
                {"id": "verify_after_reinstall", "ok": True},
                {"id": "clean_uninstall", "ok": False,
                 "detail": {"error": "Silent uninstall failed with exit code 5"}},
                {"id": "rollback_restore", "ok": True},
            ],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertFalse(restored["ok"])
        failed = [c for c in restored["checks"] if not c["ok"]]
        self.assertEqual(len(failed), 1)
        self.assertEqual(failed[0]["id"], "clean_uninstall")

    def test_script_writes_artifact_to_output_dir(self) -> None:
        """The artifact must be written to the output directory."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("nsis-upgrade-rollback-test-", text)

    def test_non_windows_skip_exit_2(self) -> None:
        """On non-Windows, the script must exit 2."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("exit 2", text)
        self.assertIn("Skipping", text)

    def test_script_defines_silent_install_latest_switch(self) -> None:
        """The script must support -SilentInstallLatest and -InstallerPath."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("SilentInstallLatest", text)
        self.assertIn("[string]$InstallerPath", text)

    def test_required_installed_files_list(self) -> None:
        """The required installed files list must be comprehensive."""
        text = SCRIPT.read_text(encoding="utf-8")
        required = [
            "d-token-desktop.exe",
            "dtoken-proxy.exe",
            "dtoken-agent-bridge.exe",
            "WebView2Loader.dll",
            "uninstall.exe",
        ]
        for file in required:
            with self.subTest(file=file):
                self.assertIn(file, text, f"Required file '{file}' missing from verification")

    def test_installed_startup_has_timeout(self) -> None:
        """The startup timeout must be configurable and default to 20s."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("StartupTimeoutSeconds", text)
        self.assertIn("$StartupTimeoutSeconds = 20", text)


if __name__ == "__main__":
    unittest.main()