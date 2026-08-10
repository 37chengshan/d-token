"""Contract tests for the hot-sync review-gate observer and bridge identity
check (PR-13 / AGENT-7 / AGENT-11). Both artifacts are read-only observations
that must fail closed (status=blocked) when their pinned evidence is missing;
they never write configs and never self-assert a pass.
"""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def load(name: str) -> object:
    spec = importlib.util.spec_from_file_location(
        name, REPO_ROOT / "support" / "scripts" / f"{name}.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


HOT = load("run_hot_sync_review_gate_evidence")
BRIDGE = load("run_bridge_identity_check")


class HotSyncReviewGateEvidenceTests(unittest.TestCase):
    def test_observation_pins_automatic_apply_never_true_and_review_gate(self) -> None:
        report = HOT.observe()
        self.assertEqual(report["status"], "observed")
        self.assertTrue(report["ok"])
        by_check = {f["check"]: f["observed"] for f in report["findings"]}
        self.assertTrue(by_check["automatic_apply_allowed_never_true"])
        self.assertTrue(by_check["review_gated_observation_events"])
        self.assertTrue(by_check["snapshot_has_no_apply_instruction"])
        self.assertTrue(by_check["unit_tests_pin_no_auto_apply"])

    def test_cli_returns_zero_and_writes_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "hot-sync.json"
            code = HOT.main(["--out", str(out)])
            self.assertEqual(code, 0)
            data = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(data["status"], "observed")
            self.assertEqual(data["mode"], "read-only-observation")


class BridgeIdentityCheckTests(unittest.TestCase):
    def test_missing_bridge_fails_closed(self) -> None:
        report = BRIDGE.verify(REPO_ROOT / "does-not-exist")
        self.assertEqual(report["status"], "blocked")
        self.assertFalse(report["ok"])
        self.assertIn("bridge_is_non_empty_regular_file", report["blockers"])

    def test_cli_requires_absolute_bridge_path(self) -> None:
        code = BRIDGE.main(["--bridge", "relative/path"])
        self.assertEqual(code, 2)

    def test_verifier_gate_seam_exists_in_desktop(self) -> None:
        main = (REPO_ROOT / "code" / "apps" / "desktop" / "src-tauri" / "src" / "main.rs").read_text(
            encoding="utf-8"
        )
        self.assertIn("fn verify_dtoken_agent_bridge(", main)
        self.assertIn(
            "verify_dtoken_agent_bridge(&bridge_path)",
            main,
            "managed MCP/Hook request construction must call the bridge verifier",
        )


if __name__ == "__main__":
    unittest.main()
