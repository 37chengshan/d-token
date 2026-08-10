"""Contract tests for the Codebase Memory v2 artifact manifest.

The v2 manifest is the single addressable registry of the five most recent
official upstream releases plus the user-approved prerelease pin. Identity is
tag/commit/hash only; missing-asset versions fail closed; distribution is
user-download only (d-token never downloads updates).
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

MANIFEST = Path(__file__).parents[2] / "code" / "runtimes" / "codebase-memory" / "artifact-manifest.json"

OFFICIAL_ORDER = ["v0.9.0", "v0.8.1", "0.8.0", "0.7.0", "0.6.1"]
DESKTOP_TARGETS = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
]


class ManifestV2ContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        cls.versions = cls.manifest["versions"]
        cls.by_version = {entry["version"]: entry for entry in cls.versions}

    def test_schema_is_v2(self):
        self.assertEqual(self.manifest["schema_version"], 2)
        self.assertTrue(self.manifest["manifest_revision"].startswith("v2-"))

    def test_addresses_five_official_versions_in_audit_order(self):
        official = [entry for entry in self.versions if entry["kind"] == "official"]
        self.assertEqual([entry["version"] for entry in official], OFFICIAL_ORDER)
        self.assertEqual(self.manifest["counts"]["official"], 5)
        for entry in official:
            self.assertFalse(entry["release_prerelease"])
            self.assertTrue(entry["license"] == "MIT")
            self.assertEqual(len(entry["commit"]), 40)
            self.assertTrue(entry["tag"])

    def test_approved_prerelease_is_present_and_not_counted_as_official(self):
        prereleases = [entry for entry in self.versions if entry["kind"] == "approved-prerelease"]
        self.assertEqual([entry["version"] for entry in prereleases], ["v0.9.1-rc.1"])
        self.assertEqual(self.manifest["counts"]["approved_prerelease"], 1)
        pin = prereleases[0]
        self.assertTrue(pin["release_prerelease"])
        self.assertTrue(pin["release_immutable"])
        self.assertEqual(pin["runtime_status"], "verified")
        # The prerelease pin is the same identity as the top-level current pin.
        self.assertEqual(pin["commit"], self.manifest["upstream"]["commit"])
        self.assertEqual(pin["tag"], self.manifest["upstream"]["tag"])

    def test_identity_never_uses_server_json_version(self):
        policy = self.manifest["identity_policy"]
        self.assertEqual(policy["identity_sources"], ["tag", "commit", "sha256"])
        self.assertIs(policy["server_json_version_used"], False)
        for entry in self.versions:
            self.assertTrue(entry["tag"])
            self.assertEqual(len(entry["commit"]), 40)
            for artifact in entry.get("artifacts") or []:
                self.assertTrue(artifact["archive_sha256"])
                if artifact.get("binary_sha256") is not None:
                    self.assertEqual(len(artifact["binary_sha256"]), 64)
                else:
                    # Null binary hash must keep that version runtime-blocked.
                    self.assertEqual(entry["runtime_status"], "blocked")

    def test_missing_asset_versions_fail_closed(self):
        for version in ("0.8.0", "0.7.0", "0.6.1"):
            entry = self.by_version[version]
            self.assertEqual(entry["asset_status"], "source_tag_only")
            self.assertEqual(entry["runtime_status"], "blocked")
            self.assertEqual(entry["artifacts"], [])
            self.assertTrue(all(value == "incompatible" for value in entry["platform_matrix"].values()))
            self.assertIn("never claimed runtime-compatible", entry["note"])

    def test_v081_windows_arm64_is_explicitly_incompatible(self):
        entry = self.by_version["v0.8.1"]
        self.assertEqual(entry["asset_status"], "assets_available")
        self.assertEqual(entry["runtime_status"], "blocked")
        self.assertEqual(entry["platform_matrix"]["aarch64-pc-windows-msvc"], "incompatible")
        self.assertTrue(entry["platform_matrix"]["x86_64-pc-windows-msvc"], "compatible")
        targets = {artifact["target"] for artifact in entry["artifacts"]}
        self.assertNotIn("aarch64-pc-windows-msvc", targets)

    def test_platform_matrix_is_complete_for_every_version(self):
        for entry in self.versions:
            self.assertEqual(sorted(entry["platform_matrix"]), sorted(DESKTOP_TARGETS))
            for target in DESKTOP_TARGETS:
                self.assertIn(entry["platform_matrix"][target], ("compatible", "incompatible"))

    def test_runtime_policy_is_fail_closed_and_user_download_only(self):
        policy = self.manifest["runtime_policy"]
        for key in ("runtime_download", "upstream_installer", "upstream_ui", "auto_update", "auto_watch"):
            self.assertIs(policy[key], False)
        distribution = self.manifest["identity_policy"]["distribution"]
        self.assertEqual(distribution, "user_downloads_official_assets")
        self.assertIs(self.manifest["identity_policy"]["d_token_downloads_updates"], False)

    def test_verified_versions_carry_full_hashes_and_sigstore(self):
        # Only the current approved prerelease pin is runtime-verified.
        pin = self.by_version["v0.9.1-rc.1"]
        self.assertEqual(pin["runtime_status"], "verified")
        for artifact in pin["artifacts"]:
            self.assertEqual(len(artifact["archive_sha256"]), 64)
            self.assertEqual(len(artifact["binary_sha256"]), 64)
            self.assertTrue(artifact["sigstore_bundle"])
        # v0.9.0 records full hashes from its time as the pinned version, but
        # runtime stays blocked until the configuration-compatibility fix lands.
        v090 = self.by_version["v0.9.0"]
        self.assertEqual(v090["runtime_status"], "blocked")
        for artifact in v090["artifacts"]:
            self.assertEqual(len(artifact["archive_sha256"]), 64)
            self.assertEqual(len(artifact["binary_sha256"]), 64)
            self.assertTrue(artifact["sigstore_bundle"])
        self.assertEqual(self.manifest["sigstore"]["bundle_format"], "cosign_message_signature_v0.3")
        self.assertTrue(self.manifest["sigstore"]["certificate_identity"])
        self.assertTrue(self.manifest["sigstore"]["certificate_oidc_issuer"])

    def test_counts_match_registry(self):
        kinds = [entry["kind"] for entry in self.versions]
        self.assertEqual(kinds.count("official"), self.manifest["counts"]["official"])
        self.assertEqual(
            kinds.count("approved-prerelease"),
            self.manifest["counts"]["approved_prerelease"],
        )
        verified = sum(1 for entry in self.versions if entry["runtime_status"] == "verified")
        self.assertEqual(verified, self.manifest["counts"]["runtime_verified"])
        source_only = sum(1 for entry in self.versions if entry["asset_status"] == "source_tag_only")
        self.assertEqual(source_only, self.manifest["counts"]["source_tag_only"])


if __name__ == "__main__":
    unittest.main()
