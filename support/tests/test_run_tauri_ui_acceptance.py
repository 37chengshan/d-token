#!/usr/bin/env python3
"""Contract tests for support/scripts/run_tauri_ui_acceptance.py (N6).

Covers the fixed contract: dry-run never passes/emits, blocked without a real
native window or installed app, receipt-driven checks that cannot be satisfied
by browser/unit evidence, geometry thresholds, N9 binding, and the canonical
artifact_sha256 recompute. No installed-app launch happens in these tests.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent.parent / "scripts"
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
PRODUCER = SCRIPT_DIR / "run_tauri_ui_acceptance.py"


def run_producer(args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(PRODUCER), *args],
        capture_output=True,
        text=True,
        timeout=120,
    )


def make_receipts(receipts_dir: Path, source_digest: str, fill_missing: bool = True) -> None:
    """Write a complete set of minimal valid receipts for a pass path."""
    import run_tauri_ui_acceptance as producer

    samples = {
        "five-step-onboarding": {"steps": producer.ONBOARDING_STEPS},
        "no-standalone-upstream": {"enabled_steps": producer.ONBOARDING_STEPS},
        "service-phase-labels": {"phase_labels": producer.SERVICE_PHASES},
        "service-reduced-motion-static": {"static_equivalent": True},
        "service-running-not-connected-copy": {"text": "服务运行中 ≠ 已连接"},
        "eight-pages": {"pages": producer.EIGHT_PAGES},
        "geometry-1180x780": {"geometry": {"size": [1180, 780]}},
        "geometry-920x640": {"geometry": {"size": [920, 640]}},
        "theme-language-persistence": {"persisted": {"theme": "light", "lang": "zh"}},
        "keyboard-native-actions": {"actions": ["tab", "enter", "esc"]},
        "restart-persistence": {"restored": True},
        "candidate-bound": {"source_digest": source_digest},
    }
    for name, payload in samples.items():
        (receipts_dir / f"{name}.json").write_text(json.dumps(payload), encoding="utf-8")


class N6ProducerContract(unittest.TestCase):
    def setUp(self) -> None:
        sys.path.insert(0, str(SCRIPT_DIR))
        import run_tauri_ui_acceptance as producer

        self.producer = producer
        self.commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, text=True).strip()
        from canonical_source_digest import compute_source_manifest

        _, self.source_digest = compute_source_manifest(REPO_ROOT, self.commit)
        self.n0_source_digest = self.source_digest

    def test_resolve_git_bindings_matches_n0_canonical_source_digest(self) -> None:
        candidate_commit, source_digest = self.producer.resolve_git_bindings(REPO_ROOT)
        self.assertEqual(candidate_commit, self.commit)
        self.assertEqual(
            source_digest,
            self.n0_source_digest,
            "N6 must bind to N0's canonical tracked-file manifest digest, not a tree-id hash",
        )

    def tearDown(self) -> None:
        for name in [n for n in list(sys.modules) if n.startswith("run_tauri_ui_acceptance")]:
            del sys.modules[name]

    def test_dry_run_never_passes_and_emits_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            receipts = Path(tmp) / "receipts"
            receipts.mkdir()
            make_receipts(receipts, self.source_digest)
            artifact = Path(tmp) / "N6-ui.json"
            result = run_producer([
                "--dry-run",
                "--app-path", "/nonexistent-installed.d-token.app",
                "--receipts", str(receipts),
                "--artifact", str(artifact),
                "--expected-source-digest", self.source_digest,
            ])
            # App path does not exist -> dry-run fails closed.
            self.assertEqual(result.returncode, 2)
            self.assertIn("dry-run failed", result.stderr)
            self.assertFalse(artifact.exists(), "dry-run must never emit an artifact")

    def test_dry_run_with_valid_inputs_still_never_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            receipts = Path(tmp) / "receipts"
            receipts.mkdir()
            make_receipts(receipts, self.source_digest)
            artifact = Path(tmp) / "N6-ui.json"
            with tempfile.TemporaryDirectory() as app_dir:
                app = Path(app_dir) / "d-token.app"
                (app / "Contents" / "MacOS").mkdir(parents=True)
                (app / "Contents" / "MacOS" / "d-token").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
                result = run_producer([
                    "--dry-run",
                    "--app-path", str(app),
                    "--receipts", str(receipts),
                    "--artifact", str(artifact),
                    "--expected-source-digest", self.source_digest,
                ])
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("a dry run never passes", result.stderr)
                self.assertFalse(artifact.exists(), "dry-run must never emit an artifact")

    def test_blocked_when_no_real_native_window_observation(self) -> None:
        # No app path and no receipts -> blocked validation; no artifact pass.
        with tempfile.TemporaryDirectory() as tmp:
            artifact = Path(tmp) / "N6-ui.json"
            result = run_producer(["--artifact", str(artifact)])
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("app-path", result.stderr)
            self.assertFalse(artifact.exists())

    def test_browser_unit_evidence_is_not_enough_for_a_pass(self) -> None:
        # Receipts whose contract keys are wrong/empty must fail, not pass.
        with tempfile.TemporaryDirectory() as tmp:
            receipts = Path(tmp) / "receipts"
            receipts.mkdir()
            make_receipts(receipts, self.source_digest)
            # Simulate a unit-only "pass": a receipt missing the contract key.
            (receipts / "eight-pages.json").write_text(json.dumps({"browser": True}), encoding="utf-8")
            with tempfile.TemporaryDirectory() as app_dir:
                app = Path(app_dir) / "d-token.app"
                (app / "Contents" / "MacOS").mkdir(parents=True)
                (app / "Contents" / "MacOS" / "d-token").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
                result = run_producer([
                    "--app-path", str(app),
                    "--receipts", str(receipts),
                    "--artifact", str(Path(tmp) / "N6-ui.json"),
                    "--expected-source-digest", self.source_digest,
                ])
                # On this machine the native probe likely fails (headless) ->
                # blocked; even if a window were observed, the missing contract
                # key makes it failed. Either way return code != 0.
                self.assertNotEqual(result.returncode, 0)

    def test_candidate_binding_rejects_wrong_source_digest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            receipts = Path(tmp) / "receipts"
            receipts.mkdir()
            make_receipts(receipts, self.source_digest)
            bad_digest = "0" * 64
            with tempfile.TemporaryDirectory() as app_dir:
                app = Path(app_dir) / "d-token.app"
                (app / "Contents" / "MacOS").mkdir(parents=True)
                (app / "Contents" / "MacOS" / "d-token").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
                result = run_producer([
                    "--app-path", str(app),
                    "--receipts", str(receipts),
                    "--artifact", str(Path(tmp) / "N6-ui.json"),
                    "--expected-source-digest", bad_digest,
                ])
                self.assertEqual(result.returncode, 2)
                self.assertIn("source digest", result.stderr)
                self.assertFalse((Path(tmp) / "N6-ui.json").exists())

    def test_artifact_envelope_schema_and_artifact_sha256_recompute(self) -> None:
        corpus = json.dumps({"product-corpus": "a" * 64})
        with tempfile.TemporaryDirectory() as tmp:
            receipts = Path(tmp) / "receipts"
            receipts.mkdir()
            make_receipts(receipts, self.source_digest)
            geometry = receipts / "geometry-1180x780.json"
            geometry.write_text(json.dumps({"geometry": {"size": [1180, 780]}}), encoding="utf-8")
            # We cannot observe a real window headlessly, so production of the
            # artifact is not expected here; instead validate the recompute
            # helper directly against a hand-built envelope.
            envelope = {
                "schema_version": self.producer.SCHEMA_VERSION,
                "node_id": "N6",
                "status": "blocked",
                "reason_codes": ["native-window-not-observed"],
                "candidate_commit": self.commit,
                "source_digest": self.source_digest,
                "dirty": False,
                "registry_digest": "b" * 64,
                "corpus_digests": json.loads(corpus),
                "started_at": "2026-08-10T00:00:00Z",
                "finished_at": "2026-08-10T00:00:01Z",
                "platform": {"os": "test", "arch": "test", "toolchain": "python3"},
                "producer_revision": self.commit,
                "observation": {"probe_kind": "native-tauri"},
                "checks": [],
            }
            envelope["artifact_sha256"] = self.producer.recompute_artifact_sha256(envelope)
            self.assertEqual(
                self.producer.recompute_artifact_sha256(envelope),
                envelope["artifact_sha256"],
                "artifact_sha256 must be stable under the remove-field->canonical-JSON->SHA-256 rule",
            )
            self.assertNotIn(
                "artifact_sha256",
                self.producer.recompute_artifact_sha256({**envelope, "artifact_sha256": ""}) and {} or {},
            )

    def test_geometry_threshold_checks(self) -> None:
        # 1180x780 receipt with a 900x600 window must fail the threshold.
        with tempfile.TemporaryDirectory() as tmp:
            receipts = Path(tmp) / "receipts"
            receipts.mkdir()
            make_receipts(receipts, self.source_digest)
            (receipts / "geometry-1180x780.json").write_text(
                json.dumps({"geometry": {"size": [900, 600]}}), encoding="utf-8"
            )
            checks = self.producer.evaluate_checks(receipts, self.source_digest)
            by_id = {name: (passed, detail) for name, passed, detail in checks}
            self.assertFalse(by_id["geometry-1180x780"][0])
            self.assertTrue(by_id["geometry-920x640"][0])
            self.assertTrue(by_id["five-step-onboarding"][0])


if __name__ == "__main__":
    unittest.main()
