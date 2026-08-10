"""Contract tests for the Codebase Memory per-version acceptance producer.

The producer is no-touch pre-N0: it plans per-version indexing/graph/call
chain/dependency/impact probes for versions with official assets on a
compatible platform, and emits blocked for versions whose assets or platform
are unavailable. It never executes the upstream binary, never downloads, and
never writes inside the repository (output is stdout or an explicit path
outside the repo root).
"""

from __future__ import annotations

import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import re
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).parents[1] / "scripts" / "run_cbm_version_acceptance.py"
SPEC = importlib.util.spec_from_file_location("run_cbm_version_acceptance", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

REPO_ROOT = Path(__file__).parents[2].resolve()
MANIFEST = REPO_ROOT / "code" / "runtimes" / "codebase-memory" / "artifact-manifest.json"

SOURCE = SCRIPT.read_text(encoding="utf-8")


def load_real_manifest() -> dict:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


class ProducerContractTests(unittest.TestCase):
    def test_producer_plans_versions_with_official_assets_on_compatible_platform(self):
        manifest = load_real_manifest()
        entries = MODULE.version_acceptance(manifest, "aarch64-apple-darwin")
        by_version = {entry["version"]: entry for entry in entries}
        for version in ("v0.9.1-rc.1", "v0.9.0", "v0.8.1"):
            entry = by_version[version]
            self.assertEqual(entry["acceptance_status"], "planned", version)
            self.assertEqual(entry["block_reasons"], [], version)
            probes = [probe["id"] for probe in entry["plan"]["probes"]]
            self.assertEqual(
                probes,
                ["indexing", "graph_query", "call_chain", "dependency", "impact"],
                version,
            )
            self.assertIs(entry["plan"]["repo_zero_write"], True)
            self.assertIs(entry["plan"]["persistence"], False)
            self.assertEqual(entry["plan"]["requires"], ["post-n0-candidate-freeze", "C_final-source-digest-binding"])
            # Version identity is carried as manifest tag/commit fields; probe
            # commands bind the binary and the temporary project only.
            self.assertEqual(entry["tag"], by_version[version]["tag"])
            self.assertRegex(entry["commit"], r"^[0-9a-f]{40}$")
            commands = [" ".join(probe["command"]) for probe in entry["plan"]["probes"]]
            for command in commands:
                self.assertTrue(command.startswith("codebase-memory-mcp cli "), command)
                self.assertIn("--project", command, command)

    def test_producer_blocks_source_tag_only_versions(self):
        manifest = load_real_manifest()
        entries = MODULE.version_acceptance(manifest, "aarch64-apple-darwin")
        by_version = {entry["version"]: entry for entry in entries}
        for version in ("0.8.0", "0.7.0", "0.6.1"):
            entry = by_version[version]
            self.assertEqual(entry["acceptance_status"], "blocked", version)
            self.assertIn("assets_unavailable", entry["block_reasons"], version)
            self.assertIn("source_tag_only", entry["block_reasons"], version)
            self.assertIn("platform_incompatible", entry["block_reasons"], version)
            self.assertIsNone(entry["plan"], version)

    def test_producer_blocks_windows_arm64_for_v081(self):
        manifest = load_real_manifest()
        entries = MODULE.version_acceptance(manifest, "aarch64-pc-windows-msvc")
        by_version = {entry["version"]: entry for entry in entries}
        v081 = by_version["v0.8.1"]
        self.assertEqual(v081["acceptance_status"], "blocked")
        self.assertIn("platform_incompatible", v081["block_reasons"])
        self.assertIsNone(v081["plan"])
        # Other versions with official windows-arm64 assets stay planned.
        self.assertEqual(by_version["v0.9.0"]["acceptance_status"], "planned")
        self.assertEqual(by_version["v0.9.1-rc.1"]["acceptance_status"], "planned")

    def test_producer_blocks_when_unique_artifact_missing(self):
        manifest = load_real_manifest()
        entry = next(item for item in manifest["versions"] if item["version"] == "v0.9.0")
        entry["artifacts"] = []
        entries = MODULE.version_acceptance(manifest, "aarch64-apple-darwin")
        result = next(item for item in entries if item["version"] == "v0.9.0")
        self.assertEqual(result["acceptance_status"], "blocked")
        self.assertIn("no_unique_artifact_for_target", result["block_reasons"])

    def test_unsupported_host_platform_blocks_everything(self):
        manifest = load_real_manifest()
        entries = MODULE.version_acceptance(manifest, "x86_64-unknown-linux-gnu")
        self.assertTrue(all(entry["acceptance_status"] == "blocked" for entry in entries))
        self.assertTrue(all("platform_incompatible" in entry["block_reasons"] for entry in entries))

    def test_receipt_schema_counts_and_no_touch_flags(self):
        manifest = load_real_manifest()
        receipt = MODULE.build_receipt(manifest, "aarch64-apple-darwin", REPO_ROOT, "produce")
        # Unified evidence envelope (VERIFICATION.md §1) so the final verifier
        # can consume the N4 receipt and fail it closed pre-N0.
        self.assertEqual(receipt["schema_version"], "1")
        self.assertEqual(receipt["node_id"], "N4")
        self.assertEqual(receipt["status"], "blocked")
        self.assertIn("blocked_pre_n0", receipt["reason_codes"])
        self.assertIs(receipt["dirty"], False)
        for field in MODULE.ENVELOPE_FIELDS:
            self.assertIn(field, receipt)
        # artifact_sha256 is the canonical-JSON digest without the field.
        body = {key: value for key, value in receipt.items() if key != "artifact_sha256"}
        canonical = MODULE.canonical_json_bytes(body)
        import hashlib
        self.assertEqual(receipt["artifact_sha256"], hashlib.sha256(canonical).hexdigest())
        self.assertIs(receipt["no_touch"], True)
        self.assertIs(receipt["executed"], False)
        self.assertEqual(receipt["counts"], {"total": 6, "blocked": 3, "planned": 3, "executed": 0})
        self.assertEqual(receipt["zero_write"]["writes_in_repo"], 0)
        self.assertEqual(receipt["host"]["target"], "aarch64-apple-darwin")
        self.assertEqual(receipt["manifest_revision"], manifest["manifest_revision"])

    def test_endpoint_allowlist_contains_only_pinned_github_urls(self):
        manifest = load_real_manifest()
        urls = MODULE.endpoint_allowlist(manifest)
        self.assertTrue(urls)
        for url in urls:
            self.assertTrue(url.startswith("https://github.com/DeusData/codebase-memory-mcp/releases/"), url)
        hosts = {url.split("/")[2] for url in urls}
        self.assertEqual(hosts, {"github.com"})
        # Every download URL embeds a pinned manifest tag and asset name.
        for url in urls:
            if "/download/" in url:
                self.assertTrue(any(url.endswith(f"/{entry['tag']}/{item['archive']}")
                                    or url.endswith(f"/{entry['tag']}/{item.get('sigstore_bundle')}")
                                    for entry in manifest["versions"] for item in (entry.get("artifacts") or [])))

    def test_harness_has_no_network_imports(self):
        # Structural no-touch guarantee: the producer must never be able to
        # open network connections by itself; downloads are user-side only.
        pattern = re.compile(r"^\s*(?:import|from)\s+(?:urllib|requests|socket|http)", re.MULTILINE)
        self.assertIsNone(pattern.search(SOURCE), "harness must not import network modules")
        # subprocess is allowed only inside execute-mode functions (indented),
        # never at module top level (produce/dry-run stay no-touch).
        self.assertIsNone(re.search(r"^import subprocess$", SOURCE, re.MULTILINE))
        self.assertIn("    import subprocess", SOURCE)

    def test_v1_manifest_is_rejected(self):
        manifest = load_real_manifest()
        manifest["schema_version"] = 1
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest-v1.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaises(SystemExit):
                MODULE.load_manifest(path)

    def test_identity_policy_is_enforced(self):
        manifest = load_real_manifest()
        manifest["identity_policy"]["server_json_version_used"] = True
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest-bad-identity.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaisesRegex(SystemExit, "server.json"):
                MODULE.load_manifest(path)

    def test_runtime_policy_fail_closed_is_enforced(self):
        manifest = load_real_manifest()
        manifest["runtime_policy"]["runtime_download"] = True
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest-bad-policy.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaisesRegex(SystemExit, "fail-closed"):
                MODULE.load_manifest(path)


class NoTouchCliTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.tmp = Path(self._tmp.name)

    def test_execute_requires_frozen_n0_gate(self):
        # Pre-N0 there is no frozen candidate: execute must refuse via the
        # N0-artifact gate (missing artifacts and digest requirements).
        with self.assertRaisesRegex(SystemExit, "candidate_commit"):
            MODULE.main(["--execute", "--version", "v0.9.0", "--binary", "/bin/echo"])

    def test_output_inside_repo_root_is_rejected(self):
        with self.assertRaisesRegex(SystemExit, "zero-write violation"):
            MODULE.main(["--output", str(REPO_ROOT / "tmp-receipt-inside-repo.json")])
        self.assertFalse((REPO_ROOT / "tmp-receipt-inside-repo.json").exists())

    def test_produce_writes_receipt_only_outside_repo(self):
        output = self.tmp / "receipt.json"
        exit_code = MODULE.main(["--output", str(output), "--platform", "aarch64-apple-darwin"])
        self.assertEqual(exit_code, 0)
        receipt = json.loads(output.read_text(encoding="utf-8"))
        self.assertIs(receipt["no_touch"], True)
        self.assertIs(receipt["executed"], False)
        self.assertEqual(receipt["counts"]["blocked"], 3)
        self.assertEqual(receipt["counts"]["planned"], 3)
        # Nothing was created inside the repository by the producer.
        self.assertFalse(list(REPO_ROOT.glob("tmp-receipt-*")))

    def test_dry_run_prints_plan_without_executing(self):
        stream = io.StringIO()
        with contextlib.redirect_stdout(stream):
            exit_code = MODULE.main(["--mode", "dry-run", "--platform", "aarch64-apple-darwin"])
        self.assertEqual(exit_code, 0)
        rendered = stream.getvalue()
        self.assertIn("[planned] v0.9.0", rendered)
        self.assertIn("[blocked] 0.8.0", rendered)
        self.assertIn("no execution, no downloads, no repository writes", rendered)
        # dry-run never touches the filesystem outside the repo either.
        self.assertFalse(list(REPO_ROOT.glob("tmp-receipt-*")))

    def test_manifest_validation_is_fail_closed_in_cli(self):
        manifest = load_real_manifest()
        manifest["schema_version"] = 1
        path = self.tmp / "manifest-v1.json"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaisesRegex(SystemExit, "requires the v2"):
            MODULE.main(["--manifest", str(path), "--platform", "aarch64-apple-darwin"])


class ExecuteModeTests(unittest.TestCase):
    """Gated post-N0 execution path with a fake binary (no real downloads)."""

    CANDIDATE = "a" * 40
    SOURCE = "b" * 64
    REGISTRY = "c" * 64

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.tmp = Path(self._tmp.name)
        manifest = load_real_manifest()
        manifest_path = self.tmp / "artifact-manifest.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        self.manifest_path = manifest_path
        self.manifest_sha256 = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
        self.n0 = self.tmp / "N0-candidate.json"
        self.n0.write_text(self._n0_payload(), encoding="utf-8")

    def _n0_payload(self) -> str:
        started = "2026-08-10T00:00:00Z"
        data = {
            "schema_version": "1",
            "node_id": "N0",
            "status": "pass",
            "reason_codes": [],
            "candidate_commit": self.CANDIDATE,
            "source_digest": self.SOURCE,
            "dirty": False,
            "registry_digest": self.REGISTRY,
            "corpus_digests": {},
            "started_at": started,
            "finished_at": "2026-08-10T00:00:01Z",
            "platform": {"os": "darwin", "arch": "arm64", "toolchain": "python3.12"},
            "producer_revision": "test@1",
            "version": "v0.0.1",
        }
        data["artifact_sha256"] = MODULE.recompute_artifact_sha256(data)
        return json.dumps(data)

    def _base_args(self, output: Path) -> list:
        return [
            "--execute",
            "--version", "v0.9.0",
            "--platform", "aarch64-apple-darwin",
            "--binary", "/bin/echo",
            "--binary-sha256", hashlib.sha256(Path("/bin/echo").read_bytes()).hexdigest(),
            "--candidate-commit", self.CANDIDATE,
            "--source-digest", self.SOURCE,
            "--registry-digest", self.REGISTRY,
            "--corpus-digests", "{}",
            "--product-version", "v0.0.1",
            "--producer-revision", "test@1",
            "--n0-artifact", str(self.n0),
            "--manifest", str(self.manifest_path),
            "--manifest-sha256", self.manifest_sha256,
            "--output", str(output),
        ]

    def _source_built_args(self, output: Path) -> list:
        args = self._base_args(output)
        args += [
            "--source-built",
            "--source-commit", self._v090_commit(),
            "--build-provenance", '{"toolchain":"test","command":["make"],"builder":"test@1"}',
        ]
        return args

    @staticmethod
    def _v090_commit() -> str:
        manifest = load_real_manifest()
        entry = next(item for item in manifest["versions"] if item["version"] == "v0.9.0")
        return entry["commit"]

    def test_source_built_binary_passes_gated_execution(self):
        output = self.tmp / "receipt-pass.json"
        exit_code = MODULE.main(self._source_built_args(output))
        self.assertEqual(exit_code, 0)
        receipt = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(receipt["node_id"], "N4")
        self.assertEqual(receipt["status"], "pass")
        self.assertEqual(receipt["reason_codes"], [])
        self.assertEqual(receipt["provenance"], "source-built, not official binary")
        self.assertIs(receipt["n0_gate"]["ok"], True)
        self.assertIs(receipt["binary"]["hash_ok"], True)
        self.assertEqual(len(receipt["probes"]), 5)
        self.assertEqual(receipt["counts"], {"probes": 5, "passed": 5})
        self.assertIs(receipt["zero_write"]["git_status_unchanged"], True)
        self.assertIs(receipt["zero_write"]["temp_project_removed"], True)
        body = {key: value for key, value in receipt.items() if key != "artifact_sha256"}
        self.assertEqual(
            receipt["artifact_sha256"],
            hashlib.sha256(MODULE.canonical_json_bytes(body)).hexdigest(),
        )

    def test_official_binary_exact_hash_mismatch_fails_closed(self):
        output = self.tmp / "receipt-fail.json"
        args = self._base_args(output)
        # Declare the manifest official hash for v0.9.0; the actual /bin/echo
        # differs, so the stable binary_hash_mismatch reason must fire.
        manifest = load_real_manifest()
        entry = next(item for item in manifest["versions"] if item["version"] == "v0.9.0")
        official = next(item for item in entry["artifacts"] if item["target"] == "aarch64-apple-darwin")
        args[args.index("--binary-sha256") + 1] = official["binary_sha256"]
        exit_code = MODULE.main(args)
        self.assertEqual(exit_code, 1)
        receipt = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(receipt["status"], "failed")
        self.assertIn("binary_hash_mismatch", receipt["reason_codes"])
        self.assertIn("sigstore_bundle_required", receipt["reason_codes"])
        self.assertEqual(receipt["provenance"], "official-binary")

    def test_source_built_binary_for_assetless_version_is_eligible(self):
        # 0.8.0 has no official assets: blocked without --source-built,
        # eligible (gated) with a caller-supplied source-built binary.
        args = [
            "--execute",
            "--version", "0.8.0",
            "--platform", "aarch64-apple-darwin",
            "--binary", "/bin/echo",
            "--binary-sha256", hashlib.sha256(Path("/bin/echo").read_bytes()).hexdigest(),
            "--candidate-commit", self.CANDIDATE,
            "--source-digest", self.SOURCE,
            "--registry-digest", self.REGISTRY,
            "--corpus-digests", "{}",
            "--product-version", "v0.0.1",
            "--producer-revision", "test@1",
            "--n0-artifact", str(self.n0),
            "--manifest", str(self.manifest_path),
            "--manifest-sha256", self.manifest_sha256,
            "--output", str(self.tmp / "receipt-080.json"),
        ]
        # Without --source-built: blocked receipt, no execution.
        blocked_out = self.tmp / "receipt-080-blocked.json"
        blocked_args = [arg.replace(str(self.tmp / "receipt-080.json"), str(blocked_out)) for arg in args]
        exit_code = MODULE.main(blocked_args)
        self.assertEqual(exit_code, 0)
        blocked = json.loads(blocked_out.read_text(encoding="utf-8"))
        self.assertEqual(blocked["status"], "blocked")
        self.assertIn("assets_unavailable", blocked["reason_codes"])
        # With --source-built + pinned source commit/provenance: gated execution proceeds.
        args += [
            "--source-built",
            "--source-commit", self._v080_commit(),
            "--build-provenance", '{"toolchain":"test","command":["make"],"builder":"test@1"}',
        ]
        exit_code = MODULE.main(args)
        self.assertEqual(exit_code, 0)
        receipt = json.loads((self.tmp / "receipt-080.json").read_text(encoding="utf-8"))
        self.assertEqual(receipt["status"], "pass")
        self.assertEqual(receipt["provenance"], "source-built, not official binary")
        self.assertEqual(receipt["source_build"]["source_commit"], self._v080_commit())

    @staticmethod
    def _v080_commit() -> str:
        manifest = load_real_manifest()
        entry = next(item for item in manifest["versions"] if item["version"] == "0.8.0")
        return entry["commit"]

    def test_source_built_requires_pinned_source_commit_and_provenance(self):
        missing_commit = self._base_args(self.tmp / "x.json") + ["--source-built"]
        with self.assertRaisesRegex(SystemExit, "source-commit"):
            MODULE.main(missing_commit)
        wrong_commit = self._base_args(self.tmp / "x.json") + [
            "--source-built",
            "--source-commit", "f" * 40,
            "--build-provenance", '{"toolchain":"test"}',
        ]
        with self.assertRaisesRegex(SystemExit, "!= pinned commit"):
            MODULE.main(wrong_commit)
        missing_prov = self._base_args(self.tmp / "x.json") + [
            "--source-built",
            "--source-commit", self._v090_commit(),
        ]
        with self.assertRaisesRegex(SystemExit, "build-provenance"):
            MODULE.main(missing_prov)

    def test_execute_rejects_stale_or_mismatched_n0(self):
        # Wrong source_digest against the frozen N0 must refuse execution.
        output = self.tmp / "receipt-n0bad.json"
        args = self._base_args(output)
        args[args.index("--source-digest") + 1] = "d" * 64
        with self.assertRaisesRegex(SystemExit, "post-N0 gate not armed"):
            MODULE.main(args)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
