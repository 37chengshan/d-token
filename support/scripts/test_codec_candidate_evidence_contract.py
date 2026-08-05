#!/usr/bin/env python3
from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class CodecCandidateEvidenceContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.runner = (ROOT / "support/scripts/run_codec_candidate_evidence.ps1").read_text(
            encoding="utf-8"
        )
        self.release_verification = (
            ROOT / "support/scripts/run_release_verification.ps1"
        ).read_text(encoding="utf-8")
        self.handoff = (ROOT / "support/scripts/run_release_handoff_manifest.ps1").read_text(
            encoding="utf-8"
        )
        self.macos = (ROOT / ".github/workflows/macos-release.yml").read_text(encoding="utf-8")
        self.windows = (ROOT / ".github/workflows/windows-release.yml").read_text(
            encoding="utf-8"
        )
        self.public_release = (
            ROOT / ".github/workflows/public-unsigned-prerelease.yml"
        ).read_text(encoding="utf-8")

    def test_runner_is_candidate_bound_release_profile_and_metadata_only(self) -> None:
        for required in (
            "Get-ReleaseCandidateProvenance",
            "codec_candidate_probe",
            "--release",
            "--candidate-id",
            "--candidate-revision",
            "production-codec-candidate-evidence.json",
            "raw_corpus_stored",
            "raw_request_stored",
            "raw_response_stored",
            "secret_values_stored",
        ):
            self.assertIn(required, self.runner)

    def test_all_release_buses_require_codec_candidate_evidence(self) -> None:
        self.assertIn('"codec_candidate_evidence"', self.release_verification)
        self.assertIn('"codec_candidate_evidence"', self.handoff)
        self.assertIn("run_codec_candidate_evidence.ps1", self.macos)
        self.assertIn("run_codec_candidate_evidence.ps1", self.windows)

        mac_stamp = self.macos.index("Write-ReleaseCandidateStamp")
        mac_codec = self.macos.index("run_codec_candidate_evidence.ps1")
        mac_perf = self.macos.index("run_release_performance_gate.ps1")
        self.assertLess(mac_stamp, mac_codec)
        self.assertLess(mac_codec, mac_perf)

        win_stamp = self.windows.index("Write-ReleaseCandidateStamp")
        win_codec = self.windows.index("run_codec_candidate_evidence.ps1")
        win_perf = self.windows.index("run_release_performance_gate.ps1")
        self.assertLess(win_stamp, win_codec)
        self.assertLess(win_codec, win_perf)

    def test_public_prerelease_cross_checks_both_platform_reports(self) -> None:
        for required in (
            "mac_codec_evidence",
            "win_codec_evidence",
            "production-codec-candidate-evidence.json",
            'assert codec.get("candidate", {}).get("candidate_id") == candidate.get("candidate_id")',
            'assert mac_codec.get("evidence_revision") == win_codec.get("evidence_revision")',
            "macos-arm64-production-codec-candidate-evidence.json",
            "windows-x64-production-codec-candidate-evidence.json",
        ):
            self.assertIn(required, self.public_release)


if __name__ == "__main__":
    unittest.main()
