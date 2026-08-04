from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT_PATH = (
    Path(__file__).resolve().parents[1]
    / "scripts"
    / "generate_candidate_audit_manifest.py"
)
MODULE_SPEC = importlib.util.spec_from_file_location(
    "generate_candidate_audit_manifest", SCRIPT_PATH
)
assert MODULE_SPEC is not None and MODULE_SPEC.loader is not None
AUDIT = importlib.util.module_from_spec(MODULE_SPEC)
MODULE_SPEC.loader.exec_module(AUDIT)


class CandidateAuditManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.output = self.root / "audit" / "candidate.json"

    def clean_provenance(self, *, dirty: bool = False) -> dict[str, object]:
        return {
            "head": "a" * 40,
            "dirty": dirty,
            "status_entry_count": 1 if dirty else 0,
            "status_sha256": "b" * 64,
            "worktree_diff_sha256": "c" * 64,
            "worktree_diff_algorithm": "sha256-git-binary-diff-plus-untracked-v1",
            "tracked_diff_sha256": "d" * 64,
            "untracked_file_count": 0,
            "untracked_manifest_sha256": "e" * 64,
        }

    def write_json(self, relative: str, payload: object) -> Path:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def create_spec(self) -> dict[str, object]:
        reports = {
            "vitest": {"ok": True},
            "e2e": {"status": "passed"},
            "tauri": {"success": True},
            "real-opencode": {
                "status": "pass",
                "config_transaction": {
                    "restore_status": "restored_exact",
                    "original_bytes_restored": True,
                },
                "evidence": {
                    "route_request_provider_model_verified": True,
                    "provider_forward_verified": True,
                },
                "privacy": {
                    "auth_json_read": False,
                    "literal_credentials_recorded": False,
                    "prompt_recorded": False,
                    "response_recorded": False,
                    "nonce_recorded": False,
                },
            },
        }
        report_entries = []
        for kind, payload in reports.items():
            path = self.write_json(f"reports/{kind}.json", payload)
            report_entries.append(
                {
                    "kind": kind,
                    "path": path.relative_to(self.root).as_posix(),
                    "expected_sha256": AUDIT.sha256_file(path),
                }
            )

        app_binary = self.root / "artifacts" / "d-token.app" / "Contents" / "MacOS" / "d-token"
        app_binary.parent.mkdir(parents=True, exist_ok=True)
        app_binary.write_bytes(b"app-binary")
        sidecar = self.root / "artifacts" / "dtoken-proxy"
        sidecar.write_bytes(b"sidecar-binary")
        dmg = self.root / "artifacts" / "d-token.dmg"
        dmg.write_bytes(b"dmg-image")

        gate_ids = (
            "structure",
            "rust-workspace",
            "vitest",
            "e2e",
            "tauri",
            "real-opencode",
        )
        gates = [
            {
                "id": gate_id,
                "argv": [sys.executable, "-c", "raise SystemExit(0)"],
                "cwd": ".",
                "timeout_seconds": 10,
            }
            for gate_id in gate_ids
        ]
        gates[0]["argv"].append("token-1234567890abcdef")
        return {
            "schema_version": 1,
            "gates": gates,
            "reports": report_entries,
            "artifacts": [
                {"kind": "app", "path": "artifacts/d-token.app"},
                {"kind": "sidecar", "path": "artifacts/dtoken-proxy"},
                {"kind": "dmg", "path": "artifacts/d-token.dmg"},
            ],
        }

    def build(self, spec: dict[str, object], *, dirty: bool = False) -> dict[str, object]:
        with mock.patch.object(
            AUDIT,
            "collect_git_provenance",
            return_value=self.clean_provenance(dirty=dirty),
        ):
            return AUDIT.build_manifest(self.root, self.output, spec)

    def test_clean_candidate_passes_and_main_writes_atomically(self) -> None:
        spec = self.create_spec()
        spec_path = self.write_json("audit-spec.json", spec)
        stdout = io.StringIO()
        with mock.patch.object(
            AUDIT,
            "collect_git_provenance",
            return_value=self.clean_provenance(),
        ), contextlib.redirect_stdout(stdout):
            result = AUDIT.main(
                [
                    "--root",
                    str(self.root),
                    "--spec",
                    str(spec_path),
                    "--output",
                    str(self.output),
                ]
            )

        self.assertEqual(result, 0)
        manifest = json.loads(self.output.read_text(encoding="utf-8"))
        self.assertEqual(manifest["overall_status"], "pass")
        self.assertTrue(all(gate["exit_code"] == 0 for gate in manifest["gates"]))
        self.assertTrue(all(report["status"] == "pass" for report in manifest["reports"]))
        self.assertTrue(all(artifact["status"] == "pass" for artifact in manifest["artifacts"]))
        self.assertTrue(all(artifact["sha256"] for artifact in manifest["artifacts"]))
        serialized = self.output.read_text(encoding="utf-8")
        self.assertNotIn("token-1234567890abcdef", serialized)
        self.assertIn("[REDACTED]", serialized)
        self.assertFalse(any(self.output.parent.glob(f".{self.output.name}.*.tmp")))
        if os.name != "nt":
            self.assertEqual(self.output.stat().st_mode & 0o777, 0o600)

    def test_missing_report_and_artifact_block_pass(self) -> None:
        spec = self.create_spec()
        (self.root / "reports" / "e2e.json").unlink()
        (self.root / "artifacts" / "d-token.dmg").unlink()

        manifest = self.build(spec)

        self.assertEqual(manifest["overall_status"], "blocked")
        self.assertIn("report_missing:e2e", manifest["blockers"])
        self.assertIn("artifact_missing:dmg", manifest["blockers"])

    def test_nonzero_gate_exit_fails_candidate(self) -> None:
        spec = self.create_spec()
        structure = next(gate for gate in spec["gates"] if gate["id"] == "structure")
        structure["argv"] = [sys.executable, "-c", "raise SystemExit(7)"]

        manifest = self.build(spec)

        self.assertEqual(manifest["overall_status"], "failed")
        self.assertIn("gate_failed:structure", manifest["failures"])
        recorded = next(gate for gate in manifest["gates"] if gate["id"] == "structure")
        self.assertEqual(recorded["exit_code"], 7)

    def test_dirty_worktree_blocks_candidate(self) -> None:
        manifest = self.build(self.create_spec(), dirty=True)

        self.assertEqual(manifest["overall_status"], "blocked")
        self.assertIn("git_worktree_dirty", manifest["blockers"])
        self.assertTrue(manifest["candidate"]["dirty"])

    def test_tampered_report_hash_fails_candidate(self) -> None:
        spec = self.create_spec()
        self.write_json("reports/vitest.json", {"ok": True, "tampered": True})

        manifest = self.build(spec)

        self.assertEqual(manifest["overall_status"], "failed")
        self.assertIn("report_failed:vitest", manifest["failures"])
        report = next(item for item in manifest["reports"] if item["kind"] == "vitest")
        self.assertEqual(report["reason"], "report_sha256_mismatch")

    def test_prompt_bearing_gate_is_rejected_before_execution(self) -> None:
        spec = self.create_spec()
        spec["gates"][0]["argv"] = ["tool", "--prompt=raw prompt"]

        with self.assertRaisesRegex(AUDIT.AuditError, "payload/credential option"):
            self.build(spec)

    def test_generated_output_is_excluded_from_candidate_diff(self) -> None:
        def git_result(_root: Path, *arguments: str) -> bytes:
            if arguments == ("rev-parse", "HEAD"):
                return ("a" * 40 + "\n").encode("ascii")
            return b""

        with mock.patch.object(AUDIT, "run_read_only_git", side_effect=git_result) as run_git:
            provenance = AUDIT.collect_git_provenance(self.root, self.output)

        exclusion = ":(exclude)audit/candidate.json"
        metadata_calls = [call.args[1:] for call in run_git.call_args_list]
        self.assertTrue(
            all(
                exclusion in arguments
                for arguments in metadata_calls
                if arguments[0] != "rev-parse"
            )
        )
        self.assertFalse(provenance["dirty"])


if __name__ == "__main__":
    unittest.main()
