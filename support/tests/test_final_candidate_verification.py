"""Contract tests for PR-9 final-candidate verification orchestration.

Covers (acceptance contract):
* DAG shape: N0 -> N1..N8 (parallel, consume-only) -> N9; N0 failure blocks
  downstream; N9 requires N1..N8 settled and critical nodes verified.
* Strong candidate binding: changed HEAD/tree invalidates old N0 and all node
  evidence; stale artifacts are rejected (EVID-1).
* Old-pass rejection: the 2026-08-04 G7 artifact (candidate_id=G7, unbound)
  is rejected for N3.
* Node dependencies: N5/N6 consume Main Computer evidence only — the
  orchestrator never spawns a subprocess for them and rejects fabricated
  fields (wrote_cc_switch, screenshots, browser_preview_only).
* N8 external: absent external evidence -> status external; external/blocked
  keep the final conclusion blocked.
* N9 preconditions: unsettled nodes, unverified critical nodes, and a failed
  N0 all force blocked.
* Three conclusions only: signed-release / unsigned-technical-preview /
  blocked; unsigned MUST keep release_ready=false and
  distribution_ready_public=false.

These tests never run the real N0 gates or any external verification: N0 is
replaced by a fake spec file (--n0-spec-file) and every run is dry-run or a
bounded execute against tmp evidence.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

try:
    import jsonschema
except ImportError:  # pragma: no cover - optional contract-test dependency
    jsonschema = None

WORKTREE = Path(__file__).resolve().parents[2]
SCRIPT = WORKTREE / "support" / "scripts" / "run_final_candidate_verification.py"
RECORDER = WORKTREE / "support" / "scripts" / "run_tauri_ui_acceptance.py"
SCHEMA_FIXTURE = WORKTREE / "support" / "fixtures" / "final-candidate-verification" / "node-artifact.schema.json"

C1 = "a" * 40  # candidate commit (old)
C2 = "c" * 40  # candidate commit (current)
D1 = "1" * 64  # source digest (old)
D2 = "2" * 64  # source digest (current)

FAKE_N0_OK = {
    "commands": [
        {"id": "fake-n0", "description": "fake N0 gate for contract tests (never the real VERIFICATION.md §1 gate)",
         "argv": [sys.executable, "-c", "import sys; sys.exit(0)"], "cwd": None, "expected_rc": 0},
    ],
}
FAKE_N0_FAIL = {
    "commands": [
        {"id": "fake-n0-fail", "description": "failing fake N0 gate for contract tests",
         "argv": [sys.executable, "-c", "import sys; sys.exit(1)"], "cwd": None, "expected_rc": 0},
    ],
}


def bound(commit: str, digest: str) -> dict:
    return {"candidate_commit": commit, "source_digest": digest}


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def green_evidence(artifacts: Path, commit: str = C2, digest: str = D2,
                   signing: bool = False) -> None:
    """Full green evidence set for every node plus the aggregator's G0-G8."""
    steps = [
        "release_gate_contract", "release_candidate_provenance",
        "codec_candidate_evidence", "release_performance_gate",
        "frontend_vitest", "frontend_typecheck", "desktop_build",
        "rust_workspace_tests", "release_artifact_redaction_audit",
        "release_cleanup_audit",
    ]
    write_json(artifacts / "release-verification-20260808T000000Z.json",
               {"ok": True, "status": "passed",
                "steps": [{"id": s, "ok": True} for s in steps],
                **bound(commit, digest)})
    write_json(artifacts / "release-handoff-manifest-20260808T000000Z.json",
               {"ok": True, "status": "passed", **bound(commit, digest)})
    write_json(artifacts / "windows-dacl-acl-tests-20260808T000000Z.json",
               {"ok": True, "status": "passed", **bound(commit, digest)})
    write_json(artifacts / "managed-rtk-candidate" / "candidate-20260808T000000Z.json",
               {"status": "pass", **bound(commit, digest)})
    write_json(artifacts / "opencode-real-routed" / "real-routed-20260808T000000Z.json",
               {"status": "pass", "candidate_id": commit, "ok": True,
                **bound(commit, digest)})
    write_json(artifacts / "fan-in-artifact-contract-20260808T000000Z.json",
               {"ok": True, "status": "passed", "same_commit": True,
                "same_source_digest": True, "expected_commit": commit,
                **bound(commit, digest)})
    write_json(artifacts / "codex-ocxlocal-routed" / "codex-20260808T000000Z.json",
               {"ok": True, "status": "routed-verified", "routed": True,
                **bound(commit, digest)})
    write_json(artifacts / "agent-config-acceptance" / "config-20260808T000000Z.json",
               {"ok": True, "status": "verified", **bound(commit, digest)})
    write_json(artifacts / "cc-switch-aba" / "aba-20260808T000000Z.json",
               {"ok": True, "status": "passed", "wrote_cc_switch": False,
                "identity_restored": True, "route_identity_restored": True,
                **bound(commit, digest)})
    write_json(artifacts / "upgrade-autostart-credential" / "upgrade-rollback-audit-20260808T000000Z.json",
               {"ok": True, "status": "verified", **bound(commit, digest)})
    write_json(artifacts / "upgrade-autostart-credential" / "autostart-credential-roundtrip-20260808T000000Z.json",
               {"ok": True, "status": "verified", **bound(commit, digest)})
    write_json(artifacts / "receipt" / "optimization-receipt-20260808T000000Z.json",
               {"ok": True, "status": "passed", **bound(commit, digest)})
    write_json(artifacts / "tauri-ui-acceptance" / "tauri-ui-acceptance-20260808T000000Z.json",
               valid_ui_evidence(commit, digest))
    if signing:
        write_json(artifacts / "signing" / "macos-signing-20260808T000000Z.json",
                   {"ok": True, "notarized": True, "gatekeeper": True,
                    **bound(commit, digest)})
        write_json(artifacts / "signing" / "windows-signing-20260808T000000Z.json",
                   {"ok": True, "authenticode": True, **bound(commit, digest)})


def valid_ui_evidence(commit: str, digest: str) -> dict:
    return {
        "ok": True, "status": "verified", "recorded_by": "Main (Computer)",
        "browser_preview_only": False,
        "pages_tested": ["overview", "install", "codebase-memory", "analytics",
                         "optical", "doctor", "changes", "settings"],
        "window": {"sizes_tested": ["1180x780", "920x640"]},
        "themes_tested": ["light", "dark"],
        "locales_tested": ["zh", "en"],
        "reduce_motion_verified": True,
        "native_action_verified": True,
        "restart_restore_verified": True,
        "sidebar": {"S1": True, "S2": True, "S3": True, "S4": True},
        "optical": {"no_production_active_toggle": True,
                    "allow_active_false_prompt": True,
                    "preview_readonly": True,
                    "shadow_no_raw_payloads": True,
                    "no_image_compression_entry": True,
                    "active_off_by_default": True},
        "screenshots": [
            {"path": "/tmp/dtoken_audit/native_1180x780_light_zh.png",
             "taken_at": "2026-08-08T10:00:00Z", "window_pid": 74157,
             "width": 1180, "height": 780, "window_title": "d-token"},
            {"path": "/tmp/dtoken_audit/native_920x640_dark_en.png",
             "taken_at": "2026-08-08T10:05:00Z", "window_pid": 74157,
             "width": 920, "height": 640, "window_title": "d-token"},
        ],
        **bound(commit, digest),
    }


def run_cli(tmp: Path, extra: list[str], n0_spec: dict | None = FAKE_N0_OK) -> subprocess.CompletedProcess:
    spec_file = tmp / "n0-spec.json"
    write_json(spec_file, n0_spec)
    args = [
        str(SCRIPT),
        "--artifacts-dir", str(tmp / "artifacts"),
        "--n9-output", str(tmp / "evidence-out.md"),
        "--n0-spec-file", str(spec_file),
        *extra,
    ]
    return subprocess.run(args, capture_output=True, text=True, cwd=str(WORKTREE))


def stdout_json(proc: subprocess.CompletedProcess) -> dict:
    return json.loads(proc.stdout)


def list_files(root: Path) -> list[Path]:
    if not root.exists():
        return []
    return sorted(p for p in root.rglob("*") if p.is_file())


def load_latest(artifacts: Path, sub: str, prefix: str) -> dict:
    matches = sorted((artifacts / sub).glob(f"{prefix}*.json"), key=lambda p: p.stat().st_mtime)
    return json.loads(matches[-1].read_text(encoding="utf-8"))


class OrchestratorContractTests(unittest.TestCase):
    """CLI-level contract tests (subprocess, real script)."""

    def test_dry_run_zero_side_effects_and_dag_plan(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2])
            self.assertEqual(proc.returncode, 0, proc.stderr)
            plan = stdout_json(proc)
            self.assertEqual(plan["mode"], "plan")
            # DAG order: N0 planned first, N1..N8, N9 last.
            self.assertEqual(plan["n0"]["node_id"], "N0")
            self.assertEqual([n["node_id"] for n in plan["nodes"]],
                             ["N1", "N2", "N3", "N4", "N5", "N6", "N7", "N8"])
            self.assertIn("aggregator", plan["n9"])
            self.assertEqual(plan["n9"]["aggregator"]["bound_mode"], True)
            self.assertEqual(plan["n9"]["aggregator"]["expected_commit"], C2)
            self.assertEqual(plan["n9"]["aggregator"]["expected_source_digest"], D2)
            self.assertEqual(set(plan["n9"]["conclusion_set"]),
                             {"signed-release", "unsigned-technical-preview", "blocked"})
            # Zero side effects: no artifacts written, no output file, no
            # aggregator call.
            self.assertEqual(list_files(tmp), [tmp / "n0-spec.json"])
            self.assertFalse((tmp / "evidence-out.md").exists())

    def test_dry_run_requires_candidate_binding(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            proc = run_cli(Path(td), [])
            self.assertEqual(proc.returncode, 2)
            self.assertIn("error", stdout_json(proc))

    def test_candidate_worktree_binding_and_dirty_refusal(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            repo = tmp / "candidate"
            repo.mkdir()
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.email", "t@t"], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.name", "t"], check=True)
            (repo / "f.txt").write_text("v1\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "f.txt"], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "c1"], check=True)
            head = subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"],
                                  capture_output=True, text=True, check=True).stdout.strip()
            tree = subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD^{tree}"],
                                  capture_output=True, text=True, check=True).stdout.strip()
            import hashlib
            digest = hashlib.sha256(tree.encode("ascii")).hexdigest()

            proc = run_cli(tmp, ["--candidate-worktree", str(repo)])
            self.assertEqual(proc.returncode, 0, proc.stderr)
            plan = stdout_json(proc)
            self.assertEqual(plan["candidate"]["commit"], head)
            self.assertEqual(plan["candidate"]["source_digest"], digest)
            self.assertFalse(plan["candidate"]["worktree_dirty"])

            # Explicit flags must match the worktree.
            proc = run_cli(tmp, ["--candidate-worktree", str(repo),
                                 "--candidate-commit", C1, "--candidate-source-digest", D1])
            self.assertEqual(proc.returncode, 2)

            # Dirty candidate: dry-run plans, execute refuses.
            (repo / "f.txt").write_text("v2\n", encoding="utf-8")
            proc = run_cli(tmp, ["--candidate-worktree", str(repo)])
            self.assertEqual(proc.returncode, 0)
            self.assertTrue(stdout_json(proc)["candidate"]["worktree_dirty"])
            proc = run_cli(tmp, ["--candidate-worktree", str(repo), "--execute"])
            self.assertEqual(proc.returncode, 2)
            self.assertIn("dirty", proc.stdout)

            # Clean worktree execute runs the fake N0 then settles (blocked
            # without evidence) — proves real binding flows through execute.
            subprocess.run(["git", "-C", str(repo), "add", "f.txt"], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "c2"], check=True)
            proc = run_cli(tmp, ["--candidate-worktree", str(repo), "--execute"])
            self.assertEqual(proc.returncode, 1)
            report = stdout_json(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertEqual(report["node_states"]["N8"], "external")

    def test_execute_unsigned_technical_preview(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            green_evidence(artifacts)
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"])
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            report = stdout_json(proc)
            self.assertEqual(report["conclusion"], "unsigned-technical-preview")
            self.assertFalse(report["release_ready"])
            self.assertFalse(report["distribution_ready_public"])
            self.assertEqual(report["labels"], ["unsigned", "unnotarized"])
            # All nodes verified, N0 ran the fake gate, N9 artifact written.
            self.assertTrue(report["n0_ok"])
            self.assertEqual(set(report["node_states"].values()), {"verified"})
            nodes = artifacts / "final-candidate-verification" / "nodes"
            self.assertEqual(sorted(p.name[:2] for p in nodes.glob("*.json")),
                             ["N0", "N1", "N2", "N3", "N4", "N5", "N6", "N7", "N8", "N9"])
            n0 = load_latest(artifacts, nodes, "N0")
            self.assertEqual(n0["candidate_commit"], C2)
            self.assertTrue(n0["commands"][0]["ok"])
            self.assertEqual(n0["commands"][0]["returncode"], 0)
            n9 = load_latest(artifacts, nodes, "N9")
            self.assertEqual(n9["conclusion"], "unsigned-technical-preview")
            self.assertFalse(n9["release_ready"])
            self.assertFalse(n9["distribution_ready_public"])
            # Aggregator report + rendered evidence exist; evidence output
            # must not claim release-ready.
            self.assertTrue(list(artifacts.glob("release-gate-status-*.json")))
            evidence = (tmp / "evidence-out.md").read_text(encoding="utf-8")
            self.assertIn("unsigned technical preview", evidence)
            self.assertIn(C2, evidence)

    def test_execute_signed_release(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            green_evidence(artifacts, signing=True)
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"])
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            report = stdout_json(proc)
            self.assertEqual(report["conclusion"], "signed-release")
            self.assertTrue(report["release_ready"])
            self.assertTrue(report["distribution_ready_public"])
            self.assertEqual(report["missing_items"], [])

    @unittest.skipUnless(jsonschema is not None, "jsonschema not installed")
    def test_n9_execute_artifact_validates_against_tracked_schema(self) -> None:
        """A real N9 artifact written by execute mode must conform to the
        tracked node-artifact schema (top-level required fields incl.
        ``detail``).  The assertion is sharp: dropping any required field
        fails validation, so a missing ``detail`` cannot pass silently."""
        schema = json.loads(SCHEMA_FIXTURE.read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            # No evidence: execute settles N1..N8 (blocked/external) and
            # still writes the real N9 artifact.  Independent of the
            # aggregator's PR-7 supplemental state in this worktree.
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"])
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            nodes = artifacts / "final-candidate-verification" / "nodes"
            n9 = load_latest(artifacts, nodes, "N9")
            self.assertEqual(n9["node_id"], "N9")
            self.assertIsInstance(n9["detail"], str)
            jsonschema.validate(instance=n9, schema=schema)
            # The check is not vacuous: removing the required detail field
            # must fail validation.
            missing_detail = {k: v for k, v in n9.items() if k != "detail"}
            with self.assertRaises(jsonschema.ValidationError):
                jsonschema.validate(instance=missing_detail, schema=schema)

    def test_blocked_when_critical_evidence_missing(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            green_evidence(artifacts)
            (artifacts / "cc-switch-aba" / "aba-20260808T000000Z.json").unlink()
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"])
            self.assertEqual(proc.returncode, 1)
            report = stdout_json(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertFalse(report["release_ready"])
            self.assertEqual(report["node_states"]["N5"], "blocked")
            self.assertTrue(any("N5" in item for item in report["missing_items"]))

    def test_n8_external_keeps_final_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"])
            self.assertEqual(proc.returncode, 1)
            report = stdout_json(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertEqual(report["node_states"]["N8"], "external")
            self.assertTrue(any("N8" in item for item in report["missing_items"]))
            self.assertTrue(any("external" in item.lower() for item in report["missing_items"]))
            nodes = tmp / "artifacts" / "final-candidate-verification" / "nodes"
            n8 = load_latest(tmp / "artifacts", nodes, "N8")
            self.assertEqual(n8["status"], "external")
            self.assertFalse(n8["ok"])

    def test_n8_blocked_keeps_final_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            green_evidence(artifacts)
            write_json(artifacts / "fan-in-artifact-contract-20260808T000000Z.json",
                       {"ok": False, "same_commit": False, "same_source_digest": False,
                        "expected_commit": C2, **bound(C2, D2)})
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"])
            self.assertEqual(proc.returncode, 1)
            report = stdout_json(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertEqual(report["node_states"]["N8"], "blocked")

    def test_candidate_change_invalidates_old_nodes(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            # All evidence bound to the OLD candidate.
            green_evidence(artifacts, commit=C1, digest=D1)
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"])
            self.assertEqual(proc.returncode, 1)
            report = stdout_json(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertEqual(set(report["node_states"].values()), {"blocked"})
            nodes = tmp / "artifacts" / "final-candidate-verification" / "nodes"
            n3 = load_latest(tmp / "artifacts", nodes, "N3")
            self.assertEqual(n3["status"], "blocked")
            self.assertTrue(n3["stale"], "old candidate artifacts must be reported stale (EVID-1)")
            self.assertIn("stale", n3["detail"])
            n0 = load_latest(tmp / "artifacts", nodes, "N0")
            self.assertEqual(n0["candidate_commit"], C2,
                             "N0 must be re-run against the new candidate; the old N0 never reused")

    def test_old_g7_rejected_for_n3(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            green_evidence(artifacts)
            # The 2026-08-04 old G7 artifact: candidate_id=G7, status pass,
            # but NO candidate binding (schema v2, EVIDENCE.md section 1).
            # It is the ONLY OpenCode evidence here: the rerun never happened.
            (artifacts / "opencode-real-routed" / "real-routed-20260808T000000Z.json").unlink()
            write_json(artifacts / "opencode-real-routed" / "real-routed-20260804T112429Z-76558.json",
                       {"schema_version": 2, "status": "pass", "candidate_id": "G7",
                        "exit_code": 0, "created_at": "2026-08-04T11:24:29Z"})
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"])
            self.assertEqual(proc.returncode, 1)
            report = stdout_json(proc)
            self.assertEqual(report["node_states"]["N3"], "blocked")
            nodes = tmp / "artifacts" / "final-candidate-verification" / "nodes"
            n3 = load_latest(tmp / "artifacts", nodes, "N3")
            self.assertEqual(n3["status"], "blocked")
            stale_commits = {s["candidate_commit"] for s in n3["stale"]}
            self.assertIn(None, stale_commits)  # unbound old G7 rejected

    def test_n0_failure_blocks_downstream(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            artifacts = tmp / "artifacts"
            green_evidence(artifacts)
            proc = run_cli(tmp, ["--candidate-commit", C2, "--candidate-source-digest", D2, "--execute"],
                           n0_spec=FAKE_N0_FAIL)
            self.assertEqual(proc.returncode, 1)
            report = stdout_json(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertFalse(report["n0_ok"])
            nodes = tmp / "artifacts" / "final-candidate-verification" / "nodes"
            names = sorted(p.name[:2] for p in nodes.glob("*.json"))
            self.assertEqual(names, ["N0"], "N0 failure must stop the DAG before N1..N9")
            n0 = load_latest(tmp / "artifacts", nodes, "N0")
            self.assertEqual(n0["status"], "failed")
            self.assertFalse(n0["commands"][0]["ok"])


class NodeConsumptionContractTests(unittest.TestCase):
    """In-process contract tests: consumption never executes, N5/N6 only
    consume real Main Computer evidence, N9 preconditions are fail-closed."""

    @classmethod
    def setUpClass(cls) -> None:
        sys.path.insert(0, str(SCRIPT.parent))
        import run_final_candidate_verification as rfcv
        cls.rfcv = rfcv

    def test_consume_nodes_never_spawn_subprocesses(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            artifacts = Path(td) / "artifacts"
            green_evidence(artifacts)

            def raiser(*args, **kwargs):  # pragma: no cover - must never run
                raise AssertionError("consumption must never spawn a subprocess")

            original = self.rfcv.subprocess.run
            self.rfcv.subprocess.run = raiser
            try:
                for nid in ("N1", "N2", "N3", "N4", "N5", "N6", "N7", "N8"):
                    state = self.rfcv.consume_node(nid, artifacts, C2, D2)
                    self.assertEqual(state["status"], "verified", nid)
            finally:
                self.rfcv.subprocess.run = original

    def test_n5_rejects_fabricated_cc_switch_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            artifacts = Path(td) / "artifacts"
            green_evidence(artifacts)
            # A run that claims to have WRITTEN cc-switch is not evidence of
            # the read-only A->B->A contract.
            write_json(artifacts / "cc-switch-aba" / "aba-20260808T000000Z.json",
                       {"ok": True, "status": "passed", "wrote_cc_switch": True,
                        "identity_restored": True, **bound(C2, D2)})
            state = self.rfcv.consume_node("N5", artifacts, C2, D2)
            self.assertEqual(state["status"], "blocked")
            write_json(artifacts / "cc-switch-aba" / "aba-20260808T000000Z.json",
                       {"ok": True, "status": "passed", "wrote_cc_switch": False,
                        "identity_restored": False, **bound(C2, D2)})
            self.assertEqual(self.rfcv.consume_node("N5", artifacts, C2, D2)["status"], "blocked")

    def test_n4_status_contract(self) -> None:
        """N4 accepts the real PR-6 producer's ``verified`` status on a bound
        artifact and still rejects failed/blocked bound artifacts."""
        with tempfile.TemporaryDirectory() as td:
            artifacts = Path(td) / "artifacts"
            path = artifacts / "agent-config-acceptance" / "config-20260808T000000Z.json"
            write_json(path, {"ok": True, "status": "verified", **bound(C2, D2)})
            self.assertEqual(self.rfcv.consume_node("N4", artifacts, C2, D2)["status"], "verified")
            write_json(path, {"ok": True, "status": "failed", **bound(C2, D2)})
            self.assertEqual(self.rfcv.consume_node("N4", artifacts, C2, D2)["status"], "blocked")
            write_json(path, {"ok": True, "status": "blocked", **bound(C2, D2)})
            self.assertEqual(self.rfcv.consume_node("N4", artifacts, C2, D2)["status"], "blocked")

    def test_n6_rejects_fabricated_ui_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            artifacts = Path(td) / "artifacts"
            green_evidence(artifacts)
            path = artifacts / "tauri-ui-acceptance" / "tauri-ui-acceptance-20260808T000000Z.json"
            # Browser preview alone can never substitute native Computer evidence.
            ui = valid_ui_evidence(C2, D2)
            ui["browser_preview_only"] = True
            write_json(path, ui)
            self.assertEqual(self.rfcv.consume_node("N6", artifacts, C2, D2)["status"], "blocked")
            # Screenshots without a timestamp are not verifiable evidence.
            ui = valid_ui_evidence(C2, D2)
            ui["screenshots"][0].pop("taken_at")
            write_json(path, ui)
            self.assertEqual(self.rfcv.consume_node("N6", artifacts, C2, D2)["status"], "blocked")
            # Missing optical assertions (no production active toggle etc.).
            ui = valid_ui_evidence(C2, D2)
            ui["optical"]["no_production_active_toggle"] = False
            write_json(path, ui)
            self.assertEqual(self.rfcv.consume_node("N6", artifacts, C2, D2)["status"], "blocked")
            # Real evidence verifies.
            write_json(path, valid_ui_evidence(C2, D2))
            self.assertEqual(self.rfcv.consume_node("N6", artifacts, C2, D2)["status"], "verified")

    def test_n6_malformed_unhashable_evidence_blocks_without_crashing(self) -> None:
        """Unhashable or malformed list entries (e.g. JSON objects inside
        pages_tested, dict-valued sizes_tested) must yield a structured
        blocked reason — never a TypeError from set() — and consumption of
        the node must stay fail-closed."""
        rfcv = self.rfcv
        with tempfile.TemporaryDirectory() as td:
            artifacts = Path(td) / "artifacts"
            green_evidence(artifacts)
            path = artifacts / "tauri-ui-acceptance" / "tauri-ui-acceptance-20260808T000000Z.json"
            cases = [
                ("pages_tested", ["overview", {"page": "install"}]),   # unhashable entry
                ("window", {"sizes_tested": ["1180x780", {"w": 920}]}),
                ("themes_tested", ["light", ["dark"]]),
                ("locales_tested", ["zh", {"lang": "en"}]),
                ("pages_tested", "overview"),                          # non-list value
                ("themes_tested", None),                               # missing value
            ]
            for field, value in cases:
                bad = valid_ui_evidence(C2, D2)
                bad[field] = value
                ok, reasons = rfcv.tauri_ui_validate(bad)
                self.assertFalse(ok, field)
                self.assertTrue(any(field in r for r in reasons), (field, reasons))
                # Consumption must not raise; the node is blocked.
                write_json(path, bad)
                self.assertEqual(rfcv.consume_node("N6", artifacts, C2, D2)["status"],
                                 "blocked", field)

    def test_n9_preconditions_fail_closed(self) -> None:
        rfcv = self.rfcv
        n0_ok = {"status": "verified", "ok": True}
        nodes_ok = {nid: {"status": "verified"} for nid in rfcv.NODE_IDS}
        gate_ok = {"release_ready": True, "blockers": []}
        signing = {"macos": {"ok": True}, "windows": {"ok": True}}

        # All settled + verified -> signed release.
        out = rfcv.conclude(n0_ok, nodes_ok, gate_ok, signing)
        self.assertEqual(out["conclusion"], "signed-release")

        # Unsettled node (planned) -> blocked, N9 precondition listed.
        unsettled = dict(nodes_ok)
        unsettled["N5"] = {"status": "planned"}
        out = rfcv.conclude(n0_ok, unsettled, gate_ok, signing)
        self.assertEqual(out["conclusion"], "blocked")
        self.assertTrue(any("settled" in item for item in out["missing_items"]))

        # Critical node not verified -> blocked.
        unverified = dict(nodes_ok)
        unverified["N6"] = {"status": "blocked"}
        out = rfcv.conclude(n0_ok, unverified, gate_ok, signing)
        self.assertEqual(out["conclusion"], "blocked")
        self.assertTrue(any("critical" in item for item in out["missing_items"]))

        # N0 not verified -> blocked.
        out = rfcv.conclude({"status": "failed", "ok": False}, nodes_ok, gate_ok, signing)
        self.assertEqual(out["conclusion"], "blocked")

        # N8 external keeps blocked even with a green aggregation.
        external = dict(nodes_ok)
        external["N8"] = {"status": "external"}
        out = rfcv.conclude(n0_ok, external, gate_ok, signing)
        self.assertEqual(out["conclusion"], "blocked")

        # Gate not release-ready -> blocked.
        out = rfcv.conclude(n0_ok, nodes_ok, {"release_ready": False, "blockers": ["G6 (Ops): unavailable"]}, signing)
        self.assertEqual(out["conclusion"], "blocked")

        # Green gates without signing -> unsigned technical preview, never
        # release-ready.
        out = rfcv.conclude(n0_ok, nodes_ok, gate_ok,
                            {"macos": {"ok": False}, "windows": {"ok": False}})
        self.assertEqual(out["conclusion"], "unsigned-technical-preview")
        self.assertFalse(out["release_ready"])
        self.assertFalse(out["distribution_ready_public"])
        self.assertEqual(out["labels"], ["unsigned", "unnotarized"])

    def test_three_conclusions_only_and_flag_consistency(self) -> None:
        rfcv = self.rfcv
        self.assertEqual(set(rfcv.CONCLUSIONS),
                         {"signed-release", "unsigned-technical-preview", "blocked"})
        n0_ok = {"status": "verified", "ok": True}
        nodes_ok = {nid: {"status": "verified"} for nid in rfcv.NODE_IDS}
        for label in ("signed-release", "unsigned-technical-preview", "blocked"):
            if label == "signed-release":
                out = rfcv.conclude(n0_ok, nodes_ok, {"release_ready": True, "blockers": []},
                                    {"macos": {"ok": True}, "windows": {"ok": True}})
            elif label == "unsigned-technical-preview":
                out = rfcv.conclude(n0_ok, nodes_ok, {"release_ready": True, "blockers": []},
                                    {"macos": {"ok": False}, "windows": {"ok": False}})
            else:
                out = rfcv.conclude({"status": "failed", "ok": False}, nodes_ok,
                                    {"release_ready": True, "blockers": []},
                                    {"macos": {"ok": True}, "windows": {"ok": True}})
            self.assertEqual(out["conclusion"], label)
            if label == "signed-release":
                self.assertTrue(out["release_ready"] and out["distribution_ready_public"])
            else:
                self.assertFalse(out["release_ready"])
                self.assertFalse(out["distribution_ready_public"])

    def test_node_artifact_schema_is_tracked_and_valid(self) -> None:
        schema = json.loads(SCHEMA_FIXTURE.read_text(encoding="utf-8"))
        self.assertEqual(schema["$id"], "final-candidate-verification/node-artifact/v1")
        self.assertIn("N9", schema["properties"]["node_id"]["enum"])
        self.assertIn("signed-release", schema["properties"]["conclusion"]["enum"])

    def test_recorder_writes_and_orchestrator_consumes(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            evidence_file = tmp / "recorded-ui.json"
            write_json(evidence_file, valid_ui_evidence(C2, D2))
            artifacts = tmp / "artifacts"
            # Recorder validates without side effects.
            proc = subprocess.run(
                [sys.executable, str(RECORDER), "--evidence", str(evidence_file),
                 "--candidate-commit", C2, "--candidate-source-digest", D2,
                 "--artifacts-dir", str(artifacts), "--validate-only"],
                capture_output=True, text=True, cwd=str(WORKTREE))
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(list_files(artifacts), [])
            # Recorder writes the bound artifact.
            proc = subprocess.run(
                [sys.executable, str(RECORDER), "--evidence", str(evidence_file),
                 "--candidate-commit", C2, "--candidate-source-digest", D2,
                 "--artifacts-dir", str(artifacts)],
                capture_output=True, text=True, cwd=str(WORKTREE))
            self.assertEqual(proc.returncode, 0, proc.stderr)
            written = list_files(artifacts)
            self.assertEqual(len(written), 1)
            # The orchestrator consumes the recorder's artifact as N6.
            state = self.rfcv.consume_node("N6", artifacts, C2, D2)
            self.assertEqual(state["status"], "verified")

    def test_recorder_rejects_fabricated_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            ui = valid_ui_evidence(C2, D2)
            ui["screenshots"] = []
            evidence_file = tmp / "fake-ui.json"
            write_json(evidence_file, ui)
            artifacts = tmp / "artifacts"
            proc = subprocess.run(
                [sys.executable, str(RECORDER), "--evidence", str(evidence_file),
                 "--candidate-commit", C2, "--candidate-source-digest", D2,
                 "--artifacts-dir", str(artifacts)],
                capture_output=True, text=True, cwd=str(WORKTREE))
            self.assertEqual(proc.returncode, 1)
            self.assertIn("never fabricated", proc.stdout)
            self.assertEqual(list_files(artifacts), [])


if __name__ == "__main__":
    unittest.main()
