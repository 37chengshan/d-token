"""Contract tests for the N0 candidate evidence producer (PR-N0).

Run:  python3 support/tests/test_generate_n0_candidate_evidence.py

Pins the N0 producer contract (docs/releases/v0.0.1/VERIFICATION.md §1/§3):

  * source_digest = SHA-256 of the canonical JSON of {path: sha256} over the
    tracked files at C_final (UTF-8 path sorted; gitignored output excluded);
  * registry_digest binds support/fixtures/unified-capability-registry-v1.json;
  * corpus_digests cover the fixed benchmark/replay/fuzz/fault/quality/optical
    families from support/fixtures/unified-corpus/manifest.json;
  * the generated N0-preflight.json / N0-provenance.json carry the unified
    envelope and pass generate_release_gate_status.envelope_violations with the
    frozen digests; artifact_sha256 recomputes exactly (field removed first);
  * a frozen candidate commit is required (40 hex), and artifacts are written
    under support/artifacts/final-candidate/<C_final>/.
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
PRODUCER = REPO_ROOT / "support" / "scripts" / "generate_n0_candidate_evidence.py"
GATES = REPO_ROOT / "support" / "scripts" / "generate_release_gate_status.py"

EXPECTED_CORPUS_FAMILIES = {
    "benchmark",
    "replay",
    "fuzz",
    "fault",
    "quality",
    "optical",
}


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def canonical_sha(data: dict) -> str:
    """Independent implementation of the artifact_sha256 rule."""
    body = {key: value for key, value in data.items() if key != "artifact_sha256"}
    raw = json.dumps(
        body, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def git_head() -> str:
    proc = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return proc.stdout.strip()


class N0CandidateEvidenceProducerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.producer = load_module("generate_n0_candidate_evidence", PRODUCER)
        cls.gates = load_module("generate_release_gate_status", GATES)
        cls.commit = git_head()

    def test_commit_is_40_hex(self) -> None:
        self.assertRegex(self.commit, r"^[0-9a-f]{40}$")

    def test_source_manifest_algorithm(self) -> None:
        manifest, source_digest = self.producer.compute_source_manifest(
            REPO_ROOT, self.commit
        )
        self.assertGreater(len(manifest), 100, "repo must have tracked files")
        # Paths sorted by UTF-8; values are 64-hex SHA-256.
        self.assertEqual(list(manifest), sorted(manifest))
        for path, digest in manifest.items():
            self.assertRegex(digest, r"^[0-9a-f]{64}$")
        # source_digest = SHA-256 of canonical JSON of the manifest.
        canonical = self.producer.gate_status.canonical_json_bytes(manifest)
        self.assertEqual(source_digest, hashlib.sha256(canonical).hexdigest())

    def test_registry_digest_binds_fixture(self) -> None:
        digest = self.producer.registry_digest(self.producer.REGISTRY)
        self.assertRegex(digest, r"^[0-9a-f]{64}$")
        # Deterministic: recomputing yields the same value.
        self.assertEqual(digest, self.producer.registry_digest(self.producer.REGISTRY))

    def test_corpus_digests_cover_fixed_families(self) -> None:
        digests = self.producer.corpus_digests(self.producer.CORPUS_MANIFEST)
        self.assertEqual(set(digests), EXPECTED_CORPUS_FAMILIES)
        for digest in digests.values():
            self.assertRegex(digest, r"^[0-9a-f]{64}$")

    def test_producer_writes_three_artifacts_with_valid_envelope(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "final-candidate" / self.commit
            proc = subprocess.run(
                [
                    sys.executable,
                    str(PRODUCER),
                    "--candidate-commit",
                    self.commit,
                    "--artifacts-root",
                    str(root),
                    "--skip-gates",
                    "--quiet",
                ],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            expected_names = {
                "source-manifest.json",
                "N0-preflight.json",
                "N0-provenance.json",
            }
            written = {path.name for path in root.iterdir()}
            self.assertTrue(expected_names.issubset(written))

            preflight = json.loads((root / "N0-preflight.json").read_text())
            provenance = json.loads((root / "N0-provenance.json").read_text())
            expected = {
                "commit": self.commit,
                "source_digest": preflight["source_digest"],
                "registry_digest": preflight["registry_digest"],
                "corpus_digests": preflight["corpus_digests"],
                "version": "0.0.1",
                "producer_revision": None,
            }
            for artifact in (preflight, provenance):
                self.assertEqual(artifact["node_id"], "N0")
                self.assertEqual(artifact["status"], "pass")
                self.assertIs(artifact["dirty"], False)
                self.assertEqual(
                    artifact["artifact_sha256"], canonical_sha(artifact)
                )
                self.assertEqual(
                    self.gates.envelope_violations(artifact, expected), []
                )

            source = json.loads((root / "source-manifest.json").read_text())
            self.assertEqual(source["source_digest"], preflight["source_digest"])
            self.assertEqual(source["candidate_commit"], self.commit)
            self.assertEqual(
                source["artifact_sha256"], canonical_sha(source)
            )

    def test_rejects_non_hex_commit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            proc = subprocess.run(
                [
                    sys.executable,
                    str(PRODUCER),
                    "--candidate-commit",
                    "not-a-commit",
                    "--artifacts-root",
                    str(Path(tmp) / "x"),
                    "--skip-gates",
                    "--quiet",
                ],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode, 1)
            self.assertIn("40 lowercase hex", proc.stdout + proc.stderr)


if __name__ == "__main__":
    unittest.main()
