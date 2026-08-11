"""Contract tests for the unified corpus manifest (T-C0C / PR-C0C).

Run:  python3 support/tests/test_unified_corpus_manifests.py
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
VALIDATOR = REPO_ROOT / "support" / "scripts" / "validate_unified_corpus_manifests.py"
MANIFEST = REPO_ROOT / "support" / "fixtures" / "unified-corpus" / "manifest.json"
CORPUS_ROOT = MANIFEST.parent


def run_validator(manifest_path: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(VALIDATOR), str(manifest_path)],
        capture_output=True,
        text=True,
    )


class UnifiedCorpusManifestContractTest(unittest.TestCase):
    def load_manifest(self) -> dict:
        self.assertTrue(MANIFEST.exists(), "corpus manifest must exist")
        return json.loads(MANIFEST.read_text(encoding="utf-8"))

    def test_manifest_carries_schema_revision_digest_license_privacy(self) -> None:
        data = self.load_manifest()
        for key in ("schema", "schema_version", "revision", "license", "privacy"):
            self.assertIn(key, data, f"manifest missing {key}")
        self.assertTrue(data["license"])
        self.assertTrue(data["privacy"])

    def test_all_six_corpus_families_present(self) -> None:
        data = self.load_manifest()
        for family in ("benchmark", "replay", "fuzz", "fault", "quality", "optical"):
            self.assertIn(family, data["corpora"], f"missing corpus {family}")

    def test_every_file_path_resolves_relative_to_corpus_root(self) -> None:
        data = self.load_manifest()
        for name, corpus in data["corpora"].items():
            for item in corpus["files"]:
                path = CORPUS_ROOT / item["path"]
                self.assertTrue(
                    path.is_file(),
                    f"corpus {name}: path does not resolve: {item['path']}",
                )

    def test_validator_passes_on_real_manifest(self) -> None:
        result = run_validator(MANIFEST)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("contract OK", result.stdout)

    def test_validator_rejects_missing_family_fail_closed(self) -> None:
        data = self.load_manifest()
        del data["corpora"]["optical"]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(json.dumps(data), encoding="utf-8")
            result = run_validator(manifest)
            self.assertNotEqual(result.returncode, 0, "missing family must fail closed")
            self.assertIn("missing from manifest", result.stdout)

    def test_validator_rejects_digest_mismatch_fail_closed(self) -> None:
        data = self.load_manifest()
        data["corpora"]["benchmark"]["digest"] = "0" * 64
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(json.dumps(data), encoding="utf-8")
            result = run_validator(manifest)
            self.assertNotEqual(result.returncode, 0, "digest mismatch must fail closed")
            self.assertIn("digest mismatch", result.stdout)


if __name__ == "__main__":
    unittest.main()
