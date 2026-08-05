"""Contract tests for the G0-G8 release gate status aggregator.

The aggregator (support/scripts/generate_release_gate_status.py) is the
cross-platform, fail-closed view of the Headroom-migration release gates
(docs/PLAN.md §1C.2).  These tests pin the contract that matters for release
honesty:

  * a gate NEVER passes on "script parses", metadata-only presence, or an
    unsigned notice — every passed gate requires an executed report artifact
    with ok=true;
  * G6 (PowerShell Ops gates) is `unavailable`/`blocked`, never passed, when
    the release-verification report was not produced (e.g. no pwsh on macOS);
  * G8 (Candidate) is `blocked` when dual-platform fan-in evidence binding both
    platforms to the same non-empty commit/source digest is absent or failed;
  * the default exit code is 1 whenever any gate is not `passed`; --report-only
    is diagnostics-only and still reports release_ready=false.

Run:  python3 support/tests/test_release_gate_status_aggregation.py
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
AGGREGATOR = REPO_ROOT / "support" / "scripts" / "generate_release_gate_status.py"


def run_aggregator(artifacts_dir: Path, report_only: bool = False) -> subprocess.CompletedProcess:
    cmd = [sys.executable, str(AGGREGATOR), "--artifacts-dir", str(artifacts_dir),
           "--output-dir", str(artifacts_dir)]
    if report_only:
        cmd.append("--report-only")
    return subprocess.run(cmd, capture_output=True, text=True)


def write_json(path: Path, data: dict) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data), encoding="utf-8")
    return path


def make_verification_report() -> dict:
    step_ids = [
        "release_gate_contract",
        "release_candidate_provenance",
        "frontend_vitest",
        "frontend_typecheck",
        "desktop_build",
        "rust_workspace_tests",
        "codec_candidate_evidence",
        "release_performance_gate",
        "release_artifact_redaction_audit",
        "release_cleanup_audit",
    ]
    return {
        "ok": True,
        "mode": "release-verification",
        "candidate": {"candidate_id": "G8"},
        "steps": [{"id": sid, "ok": True, "summary": {}} for sid in step_ids],
    }


def make_full_pass_evidence(dir: Path) -> None:
    """Synthetic evidence set that should yield every gate passed."""
    write_json(dir / "release-verification-20260805T000000Z.json", make_verification_report())
    write_json(dir / "release-handoff-manifest-20260805T000000Z.json", {"ok": True})
    write_json(dir / "windows-dacl-acl-tests-20260805T000000Z.json", {"ok": True, "passed": 3, "total": 3})
    write_json(dir / "managed-rtk-candidate" / "candidate-20260805T000000Z.json", {"ok": True})
    write_json(dir / "opencode-real-routed" / "real-routed-20260805T000000Z.json",
               {"candidate_id": "G7", "status": "pass", "gate": {"ok": True}})
    write_json(dir / "fan-in-artifact-contract-20260805T000000Z.json",
               {"ok": True, "same_commit": True, "same_source_digest": True,
                "critical_passed": 16, "critical_total": 16})


class ReleaseGateStatusAggregatorContractTests(unittest.TestCase):

    def test_aggregator_script_exists_and_parses(self) -> None:
        self.assertTrue(AGGREGATOR.is_file(), "aggregator script is missing")
        # Static compile check (imports resolve, syntax valid).
        compile(AGGREGATOR.read_text(encoding="utf-8"), str(AGGREGATOR), "exec")

    def test_fail_closed_on_empty_artifacts_dir(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1, "empty evidence must exit non-zero")
            report = json.loads(proc.stdout)
            self.assertIs(report["release_ready"], False)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G6"], "unavailable",
                             "PowerShell Ops gates must be unavailable (never passed) when "
                             "the release-verification report was not produced")
            self.assertEqual(states["G8"], "blocked",
                             "G8 must be blocked when dual-platform fan-in evidence is absent")
            self.assertEqual(states["G0"], "unavailable")
            self.assertEqual(states["G7"], "unavailable")
            self.assertTrue(report["blockers"], "blockers list must be non-empty")

    def test_report_only_still_reports_not_ready(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            proc = run_aggregator(Path(tmp), report_only=True)
            self.assertEqual(proc.returncode, 0, "--report-only exits 0 for diagnostics")
            report = json.loads(proc.stdout)
            self.assertIs(report["release_ready"], False,
                          "--report-only must never report a release as ready")
            report_path = Path(report["report_path"])
            self.assertTrue(report_path.is_file(), "report artifact must be written")

    def test_full_pass_evidence_yields_all_gates_passed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            make_full_pass_evidence(Path(tmp))
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 0)
            report = json.loads(proc.stdout)
            self.assertIs(report["release_ready"], True)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            for gate in ("G0", "G1", "G2", "G3", "G4", "G5", "G6", "G7", "G8"):
                self.assertEqual(states[gate], "passed", f"{gate} should pass with full evidence")
            self.assertEqual(report["blockers"], [])

    def test_verification_report_ok_false_blocks_g6(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            bad = make_verification_report()
            bad["ok"] = False
            write_json(Path(tmp) / "release-verification-20260805T000000Z.json", bad)
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G6"], "blocked")
            self.assertEqual(states["G0"], "blocked")

    def test_parse_only_evidence_never_passes_g6(self) -> None:
        # A host that can only "parse" the PowerShell scripts (e.g. macOS
        # without pwsh) produces NO release-verification report.  The
        # aggregator must not invent a pass from script presence.
        with tempfile.TemporaryDirectory() as tmp:
            dir_ = Path(tmp)
            # Presence of the scripts themselves is not evidence:
            (dir_ / "run_release_verification.ps1").write_text(
                "param()\nWrite-Output 'parseable only'\n", encoding="utf-8")
            proc = run_aggregator(dir_)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertNotEqual(states["G6"], "passed",
                                "script parseability must never count as a G6 pass")

    def test_g8_blocked_when_fan_in_report_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            write_json(Path(tmp) / "fan-in-artifact-contract-20260805T000000Z.json",
                       {"ok": False, "same_commit": False, "same_source_digest": False})
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G8"], "blocked")

    def test_g8_blocked_when_fan_in_ok_but_same_commit_false(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            write_json(Path(tmp) / "fan-in-artifact-contract-20260805T000000Z.json",
                       {"ok": True, "same_commit": False, "same_source_digest": True})
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G8"], "blocked",
                             "fan-in ok without same_commit must still block G8")

    def test_g5_partial_with_macos_rtk_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            write_json(Path(tmp) / "managed-rtk-candidate" / "candidate-20260805T000000Z.json",
                       {"ok": True})
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G5"], "partial",
                             "macOS-only RTK evidence is partial until fan-in proves "
                             "Windows/WSL runner receipts")

    def test_g5_partial_with_macos_rtk_status_pass_schema(self) -> None:
        # managed-RTK candidate receipts stamp status=pass (no ok field); a
        # green macOS probe without fan-in evidence must be partial, not blocked.
        with tempfile.TemporaryDirectory() as tmp:
            write_json(Path(tmp) / "managed-rtk-candidate" / "candidate-20260805T000000Z.json",
                       {"status": "pass", "candidate_id": "G5"})
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G5"], "partial")

    def test_g5_blocked_when_rtk_status_failed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            write_json(Path(tmp) / "managed-rtk-candidate" / "candidate-20260805T000000Z.json",
                       {"status": "failed"})
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G5"], "blocked")

    def test_g3_partial_without_windows_dacl_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            write_json(Path(tmp) / "release-verification-20260805T000000Z.json",
                       make_verification_report())
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G3"], "partial",
                             "workspace store tests green but Windows MSVC DACL 实测缺失")

    def test_g7_requires_status_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            write_json(Path(tmp) / "opencode-real-routed" / "real-routed-20260805T000000Z.json",
                       {"candidate_id": "G7", "status": "failed"})
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G7"], "blocked")

    def test_report_schema(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            make_full_pass_evidence(Path(tmp))
            proc = run_aggregator(Path(tmp))
            self.assertEqual(proc.returncode, 0)
            report = json.loads(proc.stdout)
            for field in ("ok", "mode", "generated_at", "host_platform", "release_ready",
                          "release_ready_hint", "blockers", "gates", "evidence", "report_path"):
                self.assertIn(field, report, f"report missing field {field}")
            self.assertEqual(report["mode"], "release-gate-status")
            self.assertEqual(len(report["gates"]), 9, "G0-G8 must all be reported")

    def test_aggregator_never_reads_scripts_parse_output(self) -> None:
        source = AGGREGATOR.read_text(encoding="utf-8")
        self.assertNotIn("release_scripts_parse", source,
                         "aggregator must not treat the contract test's parse-only check as evidence")
        self.assertIn("release-verification-", source)
        self.assertIn("fan-in-artifact-contract-", source)


if __name__ == "__main__":
    unittest.main()
