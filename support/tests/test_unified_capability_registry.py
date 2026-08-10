"""Contract tests for the unified capability registry (T-C0D / PR-C0C).

Run:  python3 support/tests/test_unified_capability_registry.py
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
VALIDATOR = REPO_ROOT / "support" / "scripts" / "validate_unified_capability_registry.py"
REGISTRY = REPO_ROOT / "support" / "fixtures" / "unified-capability-registry-v1.json"


def run_validator(registry_path: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(VALIDATOR), str(registry_path)],
        capture_output=True,
        text=True,
    )


class UnifiedCapabilityRegistryContractTest(unittest.TestCase):
    def load_registry(self) -> dict:
        self.assertTrue(REGISTRY.exists(), "registry fixture must exist")
        return json.loads(REGISTRY.read_text(encoding="utf-8"))

    def test_registry_has_exactly_117_unique_entries(self) -> None:
        data = self.load_registry()
        entries = data["entries"]
        ids = [entry["id"] for entry in entries]
        self.assertEqual(len(entries), 117, "exactly 117 capability entries")
        self.assertEqual(len(set(ids)), 117, "all ids unique")

    def test_every_entry_is_complete_and_status_is_allowed(self) -> None:
        data = self.load_registry()
        for entry in data["entries"]:
            for key in (
                "id",
                "source_capability",
                "owner_crate",
                "dispatch_symbol",
                "tests",
                "real_evidence",
                "gates",
                "status",
            ):
                self.assertIn(key, entry, f"{entry.get('id')} missing {key}")
                self.assertTrue(entry[key], f"{entry.get('id')} empty {key}")
            self.assertIn(
                entry["status"],
                {"implemented_unverified", "blocked"},
                f"{entry['id']} status must be implemented_unverified/blocked before N0",
            )
            for test_key in ("normal", "error", "recovery"):
                self.assertTrue(
                    entry["tests"].get(test_key),
                    f"{entry['id']} tests.{test_key} must be non-empty",
                )

    def test_starts_with_nine_codecs_and_twelve_aggregates(self) -> None:
        data = self.load_registry()
        ids = [entry["id"] for entry in data["entries"]]
        codec_ids = [
            "json-codec", "log-codec", "search-codec", "build-codec", "diff-codec",
            "text-codec", "html-codec", "config-codec", "code-compressor",
        ]
        for codec in codec_ids:
            self.assertIn(codec, ids, f"9-codec start must include {codec}")
        aggregate_ids = [item for item in ids if item.startswith("aggregate.")]
        self.assertEqual(
            len(aggregate_ids), 12, "12 aggregate ID mappings must exist"
        )

    def test_validator_passes_on_real_registry(self) -> None:
        result = run_validator(REGISTRY)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("contract OK", result.stdout)

    def test_validator_rejects_wrong_count_fail_closed(self) -> None:
        data = self.load_registry()
        data["entries"] = data["entries"][:-1]  # 116 entries
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "registry.json"
            bad.write_text(json.dumps(data), encoding="utf-8")
            result = run_validator(bad)
            self.assertNotEqual(result.returncode, 0, "116 entries must fail closed")
            self.assertIn("expected exactly 117", result.stdout)

    def test_validator_rejects_forged_status_fail_closed(self) -> None:
        data = self.load_registry()
        data["entries"][0]["status"] = "verified"
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "registry.json"
            bad.write_text(json.dumps(data), encoding="utf-8")
            result = run_validator(bad)
            self.assertNotEqual(result.returncode, 0, "forged status must fail closed")
            self.assertIn("not allowed before N0", result.stdout)

    def test_validator_rejects_unresolved_test_path_fail_closed(self) -> None:
        data = self.load_registry()
        data["entries"][0]["tests"]["normal"] = ["code/does-not-exist/test.rs"]
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "registry.json"
            bad.write_text(json.dumps(data), encoding="utf-8")
            result = run_validator(bad)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("does not resolve", result.stdout)

    def test_validator_rejects_unresolved_evidence_path_fail_closed(self) -> None:
        data = self.load_registry()
        data["entries"][0]["real_evidence"] = ["/tmp/external-evidence.json"]
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "registry.json"
            bad.write_text(json.dumps(data), encoding="utf-8")
            result = run_validator(bad)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("must be repo-relative", result.stdout)



if __name__ == "__main__":
    unittest.main()
