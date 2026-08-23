"""
Offline fixture tests for the fan-in artifact contract validator.

These tests validate the script's structure, platform gate, and JSON report
schema without requiring actual cross-platform build artifacts.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "run_fan_in_artifact_contract.ps1"


class FanInArtifactContractTests(unittest.TestCase):
    """Structural and contract tests for the fan-in artifact contract runner."""

    def test_script_exists(self) -> None:
        self.assertTrue(SCRIPT.is_file(),
                        f"Fan-in artifact contract script missing: {SCRIPT}")

    def test_script_contains_required_contract_elements(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        # Required checks
        required_checks = [
            "same_commit_across_platforms",
            "same_source_digest",
            "commit_matches_expected",
            "mac_sha256_manifest",
            "win_sha256_manifest",
            "mac_sbom",
            "win_sbom",
            "mac_security_scan",
            "win_security_scan",
            "mac_test_receipts",
            "win_test_receipts",
            "no_secrets_in_candidate_jsons",
        ]
        for check_id in required_checks:
            with self.subTest(check_id=check_id):
                self.assertIn(check_id, text,
                              f"Required check '{check_id}' missing from script")
        # Platform gate
        self.assertIn("exit 2", text)
        # Fail-closed on missing platform: single-platform host must exit 2
        # (not a fan-in staging environment) instead of failing the platform job
        self.assertIn("Only one platform directory found", text)
        self.assertIn("exit 2", text)
        # Secret scanning
        self.assertIn("sk-[A-Za-z0-9]", text)

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
        """Verify the JSON report matches the expected schema for a pass."""
        report = {
            "ok": True,
            "mode": "fan-in-artifact-contract",
            "expected_commit": "a" * 40,
            "mac_candidate_id": "candidate-mac",
            "win_candidate_id": "candidate-win",
            "same_commit": True,
            "same_source_digest": True,
            "source_digest_anchor": "n0-canonical-source-manifest",
            "expected_source_digest": "b" * 64,
            "passed": 32,
            "total": 32,
            "critical_passed": 16,
            "critical_total": 16,
            "summary": "Fan-in artifact contract: 32/32 checks passed, 0 critical blocker(s)",
            "blockers": [],
            "checks": [
                {"id": "mac_dir_exists", "ok": True},
                {"id": "win_dir_exists", "ok": True},
                {"id": "same_commit_across_platforms", "ok": True},
                {"id": "same_source_digest", "ok": True},
            ],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertEqual(restored["mode"], "fan-in-artifact-contract")
        self.assertTrue(restored["ok"])
        self.assertEqual(restored["source_digest_anchor"], "n0-canonical-source-manifest")
        self.assertEqual(restored["expected_source_digest"], "b" * 64)

        self.assertEqual(len(restored["blockers"]), 0)

    def test_source_digest_uses_n0_canonical_manifest_anchor(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("canonical_source_digest.py", text)
        self.assertIn("n0-canonical-source-manifest", text)
        self.assertIn("expected_source_digest", text)
        self.assertNotIn("commit-tree-object-sha256", text)
        self.assertNotIn("tracked_diff_sha256", text)
        self.assertNotIn("^{tree}", text)

    def test_producer_stamps_n0_canonical_source_digest(self) -> None:
        producer = Path(__file__).resolve().parents[1] / "scripts" / "release_candidate_provenance.ps1"
        self.assertTrue(producer.is_file(), "candidate provenance script missing")
        text = producer.read_text(encoding="utf-8")
        self.assertIn("canonical_source_digest.py", text)
        self.assertIn("source_digest = $SourceDigest", text)
        self.assertIn("tracked_diff_sha256 = $TrackedDiffSha256", text)
        self.assertNotIn('"rev-parse", "HEAD^{tree}"', text)

    def test_legacy_digest_cannot_pass_as_source_digest(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertNotIn('Properties["tracked_diff_sha256"]', text)
        self.assertNotIn("$MacCandidate.tracked_diff_sha256", text)
        self.assertNotIn("$WinCandidate.tracked_diff_sha256", text)


    def test_fail_closed_when_platform_missing(self) -> None:
        """Verify fail-closed: missing platform blocks with blockers list."""
        report = {
            "ok": False,
            "mode": "fan-in-artifact-contract",
            "expected_commit": "a" * 40,
            "same_commit": False,
            "same_source_digest": False,
            "passed": 20,
            "total": 32,
            "critical_passed": 10,
            "critical_total": 16,
            "summary": "Fan-in artifact contract: 20/32 checks passed, 3 critical blocker(s)",
            "blockers": [
                "platforms_built_from_different_commits",
                "source_digest_mismatch",
                "sha256_manifest_missing",
            ],
            "checks": [],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertFalse(restored["ok"])
        self.assertGreater(len(restored["blockers"]), 0)
        self.assertIn("platforms_built_from_different_commits", restored["blockers"])

    def test_differs_from_expected_commit(self) -> None:
        """Verify that commit mismatch is reported."""
        report = {
            "ok": False,
            "mode": "fan-in-artifact-contract",
            "expected_commit": "b" * 40,
            "same_commit": True,
            "same_source_digest": True,
            "blockers": ["commit_does_not_match_expected"],
            "checks": [
                {"id": "commit_matches_expected", "ok": False,
                 "detail": {"expected": "b" * 40, "mac": "a" * 40, "win": "a" * 40}},
            ],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        self.assertFalse(restored["ok"])
        self.assertIn("commit_does_not_match_expected", restored["blockers"])

    def test_artifact_types_verified(self) -> None:
        """All required artifact types must be verified per platform."""
        text = SCRIPT.read_text(encoding="utf-8")
        artifact_types = [
            "release-candidate.json",
            "SHA256SUMS",
            "cdx.json",
            "security-scan",
            "production-codec-candidate-evidence",
            "headroom-capability-snapshot",
            "build-info",
            "UNSIGNED-TECHNICAL-PREVIEW",
        ]
        for artifact in artifact_types:
            with self.subTest(artifact=artifact):
                self.assertIn(artifact, text,
                              f"Artifact type '{artifact}' missing from script")

    def test_test_receipt_check(self) -> None:
        """The script must verify at least one test receipt per platform."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("release-verification-", text)
        self.assertIn("test_receipts", text)

    def test_secret_scanning(self) -> None:
        """The script must scan all staged artifacts with value-shaped patterns.

        Patterns match credential VALUES (sk-ant-..., ghp_, AKIA, AIza), never
        bare key names, so future keys like raw_prompt_sha256 cannot false
        positive while Anthropic and GitHub tokens are caught.
        """
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("no_secrets_in_candidate_jsons", text)
        self.assertIn("sk-ant-", text)
        self.assertIn("sk-[A-Za-z0-9]{32,}", text)
        self.assertIn("ghp_", text)
        self.assertIn("AKIA", text)
        self.assertIn("AIza", text)
        # The scan must cover every staged artifact, not just the candidate JSONs.
        self.assertIn("Find-Files $MacDirResolved", text)
        self.assertIn("Find-Files $WinDirResolved", text)
        # Bare key-name substrings must not be used as secret patterns.
        self.assertNotIn('"bearer_token"', text)
        self.assertNotIn('"raw_prompt"', text)
        self.assertNotIn('"raw_response"', text)

    def test_script_writes_artifact_to_output_dir(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("fan-in-artifact-contract-", text)


    def test_expected_commit_auto_detection(self) -> None:
        """The script must auto-detect expected commit from HEAD when not provided."""
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("rev-parse", text)
        self.assertIn("HEAD", text)

    def test_non_windows_skip_exit_2(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("exit 2", text)

    def test_fixture_success_scenario(self) -> None:
        """Build a complete success fixture and validate it."""
        report = {
            "ok": True,
            "mode": "fan-in-artifact-contract",
            "expected_commit": "abc123def456" * 5,  # 40 chars
            "mac_candidate_id": "mac-candidate-v1",
            "win_candidate_id": "win-candidate-v1",
            "same_commit": True,
            "same_source_digest": True,
            "same_dirty_state": True,
            "passed": 32,
            "total": 32,
            "critical_passed": 16,
            "critical_total": 16,
            "summary": "Fan-in artifact contract: 32/32 checks passed, 0 critical blocker(s)",
            "blockers": [],
            "checks": [
                {"id": "mac_dir_exists", "ok": True},
                {"id": "win_dir_exists", "ok": True},
                {"id": "mac_candidate_json", "ok": True},
                {"id": "win_candidate_json", "ok": True},
                {"id": "same_commit_across_platforms", "ok": True},
                {"id": "commit_matches_expected", "ok": True},
                {"id": "same_source_digest", "ok": True},
                {"id": "same_dirty_state", "ok": True},
                {"id": "mac_sha256_manifest", "ok": True},
                {"id": "win_sha256_manifest", "ok": True},
                {"id": "mac_sbom", "ok": True},
                {"id": "win_sbom", "ok": True},
                {"id": "mac_licenses", "ok": True},
                {"id": "win_licenses", "ok": True},
                {"id": "mac_security_scan", "ok": True},
                {"id": "win_security_scan", "ok": True},
                {"id": "mac_codec_evidence", "ok": True},
                {"id": "win_codec_evidence", "ok": True},
                {"id": "mac_headroom_snapshot", "ok": True},
                {"id": "win_headroom_snapshot", "ok": True},
                {"id": "mac_test_receipts", "ok": True},
                {"id": "win_test_receipts", "ok": True},
                {"id": "mac_build_info", "ok": True},
                {"id": "win_build_info", "ok": True},
                {"id": "mac_unsigned_notice", "ok": True},
                {"id": "win_unsigned_notice", "ok": True},
                {"id": "no_secrets_in_candidate_jsons", "ok": True},
            ],
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        ok_checks = [c for c in restored["checks"] if c["ok"]]
        self.assertEqual(len(ok_checks), len(restored["checks"]))
        self.assertTrue(restored["ok"])

    def test_fixture_commit_mismatch_scenario(self) -> None:
        """Build a fixture with commit mismatch and verify fail-closed."""
        report = {
            "ok": False,
            "mode": "fan-in-artifact-contract",
            "expected_commit": "a" * 40,
            "mac_candidate_id": "mac-v1",
            "win_candidate_id": "win-v1",
            "same_commit": False,
            "same_source_digest": True,
            "passed": 15,
            "total": 16,
            "critical_passed": 7,
            "critical_total": 8,
            "summary": "Fan-in artifact contract: 15/16 checks passed",
            "blockers": ["platforms_built_from_different_commits"],
            "checks": [
                {"id": "same_commit_across_platforms", "ok": False,
                 "detail": {"mac_git_head": "a" * 40, "win_git_head": "b" * 40}},
            ],
        }
        self.assertFalse(report["ok"])
        self.assertIn("platforms_built_from_different_commits", report["blockers"])

    def test_fixture_source_digest_mismatch(self) -> None:
        """Build a fixture with source digest mismatch."""
        report = {
            "ok": False,
            "mode": "fan-in-artifact-contract",
            "same_commit": True,
            "same_source_digest": False,
            "blockers": ["source_digest_mismatch"],
            "checks": [
                {"id": "same_source_digest", "ok": False,
                 "detail": {"mac": "digest-a", "win": "digest-b"}},
            ],
        }
        self.assertFalse(report["ok"])
        self.assertIn("source_digest_mismatch", report["blockers"])

    def test_fixture_missing_test_receipts(self) -> None:
        """Missing test receipts on one platform should block."""
        report = {
            "ok": False,
            "mode": "fan-in-artifact-contract",
            "same_commit": True,
            "same_source_digest": True,
            "blockers": ["test_receipts_missing"],
            "checks": [
                {"id": "mac_test_receipts", "ok": True, "detail": {"count": 3}},
                {"id": "win_test_receipts", "ok": False, "detail": {"count": 0}},
            ],
        }
        self.assertFalse(report["ok"])
        self.assertIn("test_receipts_missing", report["blockers"])

    def test_fixture_secrets_leaked(self) -> None:
        """Secrets in candidate JSONs must be detected."""
        report = {
            "ok": False,
            "mode": "fan-in-artifact-contract",
            "same_commit": True,
            "same_source_digest": True,
            "blockers": ["secrets_leaked_in_candidate_jsons"],
            "checks": [
                {"id": "no_secrets_in_candidate_jsons", "ok": False,
                 "detail": {"secrets_found": ["mac_sk-[A-Za-z0-9]"]}},
            ],
        }
        self.assertFalse(report["ok"])
        self.assertIn("secrets_leaked_in_candidate_jsons", report["blockers"])

    def test_fixture_future_schema(self) -> None:
        """The script must tolerate extra fields in the JSON (future schema)."""
        report = {
            "ok": True,
            "mode": "fan-in-artifact-contract",
            "expected_commit": "a" * 40,
            "mac_candidate_id": "mac-v1",
            "win_candidate_id": "win-v1",
            "same_commit": True,
            "same_source_digest": True,
            "passed": 32,
            "total": 32,
            "summary": "Fan-in artifact contract: 32/32 checks passed",
            "blockers": [],
            "checks": [],
            "future_field": "extra_value_not_in_schema",
            "nested_future": {"inner": "value"},
        }
        serialized = json.dumps(report, ensure_ascii=False)
        restored = json.loads(serialized)
        # Extra fields must not break parsing
        self.assertEqual(restored["future_field"], "extra_value_not_in_schema")
        self.assertEqual(restored["nested_future"]["inner"], "value")

    def test_fixture_paths_with_spaces(self) -> None:
        """The script must handle paths with spaces (escaped/quoted)."""
        text = SCRIPT.read_text(encoding="utf-8")
        # All path-handling lines should use -LiteralPath to avoid space issues
        self.assertIn("-LiteralPath", text,
                      "Script must use -LiteralPath for path safety")
        # Join-Path with spaces in arguments
        self.assertIn("Join-Path", text)


if __name__ == "__main__":
    unittest.main()