"""Contract tests for the G0-G9 release gate status aggregator.

The aggregator (support/scripts/generate_release_gate_status.py) is the
cross-platform, fail-closed view of the release gates (docs/PLAN.md §1C.2,
docs/releases/v0.0.1/VERIFICATION.md §6).  These tests pin the contract that
matters for release honesty:

  * every consumed artifact must be listed explicitly in an evidence manifest;
    the four expected digests are REQUIRED arguments;
  * a gate NEVER passes on "script parses", metadata-only presence, an
    unsigned notice, or a status=pass stamp alone — every N0-N10 artifact is
    envelope-validated and its artifact_sha256 is recomputed and compared;
  * missing, stale, old-candidate, digest-mismatch, or pass-only evidence is
    rejected;
  * G6 (PowerShell Ops gates) is `unavailable`/`blocked`, never passed, when
    the executed release-verification report was not produced (e.g. no pwsh);
  * G8 (Candidate) is `blocked` when dual-platform fan-in evidence binding
    both platforms to the same non-empty commit/source digest is absent or
    failed (双平台同 commit fan-in 证据缺失);
  * the default exit code is 1 whenever any gate is not `passed`; --report-only
    is diagnostics-only and still reports release_ready=false.

Run:  python3 support/tests/test_release_gate_status_aggregation.py
"""
from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
AGGREGATOR = REPO_ROOT / "support" / "scripts" / "generate_release_gate_status.py"
REGISTRY_PATH = REPO_ROOT / "support" / "fixtures" / "unified-capability-registry-v1.json"

C_FINAL = "a" * 40
SOURCE_DIGEST = "b" * 64
REGISTRY_DIGEST = "c" * 64
CORPUS_DIGESTS = {"benchmark": "d" * 64}
N0_FINISHED = "2026-08-09T10:00:00Z"
NODE_STARTED = "2026-08-09T10:00:01Z"
NODE_FINISHED = "2026-08-09T11:00:00Z"
N0_STARTED = "2026-08-09T09:00:00Z"

EXPECTED_ARGS = [
    "--expected-commit",
    C_FINAL,
    "--expected-source-digest",
    SOURCE_DIGEST,
    "--expected-registry-digest",
    REGISTRY_DIGEST,
    "--expected-corpus-digests",
    json.dumps(CORPUS_DIGESTS),
    "--expected-version",
    "0.0.1",
]


def _recompute(data: dict) -> str:
    import hashlib as _h
    body = {k: v for k, v in data.items() if k != "artifact_sha256"}
    raw = json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    return _h.sha256(raw).hexdigest()


def canonical_sha(data: dict) -> str:
    body = {key: value for key, value in data.items() if key != "artifact_sha256"}
    raw = json.dumps(
        body, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def make_artifact(node_id: str, *, commit: str = C_FINAL, status: str = "pass",
                  started: str | None = None, finished: str | None = None,
                  extra: dict | None = None, omit_envelope: bool = False) -> dict:
    data: dict = {
        "schema_version": "1",
        "node_id": node_id,
        "status": status,
        "reason_codes": [],
        "candidate_commit": commit,
        "source_digest": SOURCE_DIGEST,
        "dirty": False,
        "registry_digest": REGISTRY_DIGEST,
        "corpus_digests": CORPUS_DIGESTS,
        "started_at": started or (N0_STARTED if node_id == "N0" else NODE_STARTED),
        "finished_at": finished or (N0_FINISHED if node_id == "N0" else NODE_FINISHED),
        "platform": {"os": "darwin", "arch": "arm64", "toolchain": "python3"},
        "producer_revision": "test-producer@1",
    }
    if node_id == "N0":
        data["version"] = "0.0.1"
    if extra:
        data.update(extra)
    if omit_envelope:
        data.pop("candidate_commit", None)
    data["artifact_sha256"] = canonical_sha(data)
    return data


def make_legacy_release_verification(ok: bool = True) -> dict:
    return {
        "ok": ok,
        "mode": "release-verification",
        "steps": [
            {"id": "release_artifact_redaction_audit", "ok": True},
            {"id": "release_cleanup_audit", "ok": True},
        ],
    }


def write_json(path: Path, data: dict) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data), encoding="utf-8")
    return path


def write_full_pass_evidence(dir: Path) -> dict:
    """Synthetic explicit manifest that should yield every gate passed."""
    artifacts: dict[str, list[str]] = {}
    node_files = {
        "N0": ["N0-preflight.json"],
        "N1": ["N1-receipt.json"],
        "N2": ["N2-codex.json"],
        "N3": ["N3-opencode.json"],
        "N4": ["N4-config.json", "N4-cbm-versions.json"],
        "N5": ["N5-cc-switch.json"],
        "N6": ["N6-ui.json"],
        "N7": ["N7-macos.json"],
        "N8": ["N8-windows.json"],
        "N10-U0": ["N10-U0-contract.json"],
        "N10-U1": ["N10-U1-registry.json"],
        "N10-U2": ["N10-U2-property-fuzz.json"],
        "N10-U3": ["N10-U3-fault.json"],
        "N10-U4": ["N10-U4-replay-quality.json"],
        "N10-U5": ["N10-U5-perf.json"],
        "N10-U6": ["N10-U6-provider-paired.json"],
        "N10-U7": ["N10-U7-optical-active.json"],
        "N10-U8": ["N10-U8-platform.json"],
        "N10-U9": ["N10-U9-coverage.json"],
    }
    # Per-node artifact list mirroring the registry producer mapping, so the
    # U9 coverage seal traces every entry to a valid pass artifact.
    registry_path = REPO_ROOT / "support" / "fixtures" / "unified-capability-registry-v1.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    per_entry = []
    for entry in registry["entries"]:
        producer = entry.get("producer", "")
        node = producer.split(":", 1)[1] if producer.startswith("entry-producer:") else None
        per_entry.append(
            {
                "id": entry.get("id"),
                "producer_node": node,
                "artifact": (node_files.get(node) or [None])[0],
                "verified": True,
                "reason": None,
            }
        )
    for role, filenames in node_files.items():
        paths = []
        for filename in filenames:
            extra: dict = {}
            if role == "N10-U9":
                extra = {
                    "coverage": {
                        "total": 117,
                        "verified": 117,
                        "unverified": 0,
                        "verified_ids": [item["id"] for item in per_entry],
                        "per_entry": per_entry,
                    }
                }
            paths.append(str(write_json(dir / filename,
                                        make_artifact(role, extra=extra))))
        artifacts[role] = paths
    # Independent managed-RTK platform receipts (G5): executed, bound, usage>0.
    for role, filename in (
        ("managed_rtk_macos", "managed-rtk-macos-20260810T000000Z.json"),
        ("managed_rtk_windows", "managed-rtk-windows-20260810T000000Z.json"),
        ("managed_rtk_wsl", "managed-rtk-wsl-20260810T000000Z.json"),
    ):
        artifacts[role] = [
            str(write_json(
                dir / filename,
                {
                    "status": "pass",
                    "usage_count": 3,
                    "candidate_commit": C_FINAL,
                    "candidate_id": C_FINAL,
                    "source_digest": SOURCE_DIGEST,
                },
            ))
        ]
    artifacts["release_verification"] = [
        str(write_json(dir / "release-verification-20260809T000000Z.json",
                       make_legacy_release_verification()))
    ]
    artifacts["release_handoff_manifest"] = [
        str(write_json(dir / "release-handoff-manifest-20260809T000000Z.json",
                       {"ok": True}))
    ]
    artifacts["fan_in_artifact_contract"] = [
        str(write_json(dir / "fan-in-artifact-contract-20260809T000000Z.json",
                       {"ok": True, "same_commit": True, "same_source_digest": True}))
    ]
    return {"schema_version": "1", "artifacts": artifacts}


def run_aggregator(
    manifest: dict,
    root: Path,
    *,
    report_only: bool = False,
    extra_args: list[str] | None = None,
) -> subprocess.CompletedProcess:
    manifest_path = write_json(root / "evidence-manifest.json", manifest)
    cmd = [
        sys.executable,
        str(AGGREGATOR),
        "--evidence-manifest",
        str(manifest_path),
        "--output-dir",
        str(root),
    ] + EXPECTED_ARGS
    if extra_args:
        cmd.extend(extra_args)
    if report_only:
        cmd.append("--report-only")
    return subprocess.run(cmd, capture_output=True, text=True)


class ReleaseGateStatusAggregatorContractTests(unittest.TestCase):

    def test_aggregator_script_exists_and_parses(self) -> None:
        self.assertTrue(AGGREGATOR.is_file(), "aggregator script is missing")
        compile(AGGREGATOR.read_text(encoding="utf-8"), str(AGGREGATOR), "exec")

    def test_expected_digests_are_required(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest_path = write_json(root / "evidence-manifest.json",
                                       {"schema_version": "1", "artifacts": {}})
            cmd = [sys.executable, str(AGGREGATOR), "--evidence-manifest",
                   str(manifest_path), "--output-dir", str(root)]
            proc = subprocess.run(cmd, capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0,
                                "expected digests are REQUIRED (fail-closed)")

    def test_fail_closed_on_empty_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proc = run_aggregator({"schema_version": "1", "artifacts": {}}, root)
            self.assertEqual(proc.returncode, 1, "empty evidence must exit non-zero")
            report = json.loads(proc.stdout)
            self.assertIs(report["release_ready"], False)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G6"], "unavailable",
                             "PowerShell Ops gates must be unavailable (never passed) "
                             "when the release-verification report was not produced")
            self.assertEqual(states["G8"], "blocked",
                             "G8 must be blocked when dual-platform fan-in evidence is absent")
            self.assertEqual(states["G0"], "blocked")
            self.assertEqual(states["G7"], "blocked")
            self.assertEqual(len(states), 10, "G0-G9 must all be reported")
            self.assertTrue(report["blockers"], "blockers list must be non-empty")

    def test_report_only_still_reports_not_ready(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proc = run_aggregator({"schema_version": "1", "artifacts": {}}, root,
                                  report_only=True)
            self.assertEqual(proc.returncode, 0, "--report-only exits 0 for diagnostics")
            report = json.loads(proc.stdout)
            self.assertIs(report["release_ready"], False,
                          "--report-only must never report a release as ready")
            report_path = Path(report["report_path"])
            self.assertTrue(report_path.is_file(), "report artifact must be written")

    def test_full_pass_evidence_yields_all_gates_passed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            report = json.loads(proc.stdout)
            self.assertIs(report["release_ready"], True)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            for gate in ("G0", "G1", "G2", "G3", "G4", "G5", "G6", "G7", "G8", "G9"):
                self.assertEqual(states[gate], "passed", f"{gate} should pass with full evidence")
            self.assertEqual(report["blockers"], [])

    def test_verification_report_ok_false_blocks_g6(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            manifest["artifacts"]["release_verification"] = [
                str(write_json(root / "release-verification-20260809T000000Z.json",
                               make_legacy_release_verification(ok=False)))
            ]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G6"], "blocked")

    def test_parse_only_evidence_never_passes_g6(self) -> None:
        # A host that can only "parse" the PowerShell scripts (e.g. macOS
        # without pwsh) produces NO executed release-verification report.  The
        # aggregator must not invent a pass from script presence or a plain
        # text file listed in the manifest.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            script_path = root / "run_release_verification.ps1"
            script_path.write_text("param()\nWrite-Output 'parseable only'\n",
                                   encoding="utf-8")
            manifest["artifacts"]["release_verification"] = [str(script_path)]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertNotEqual(states["G6"], "passed",
                                "script parseability must never count as a G6 pass")

    def test_g8_blocked_when_fan_in_absent(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            del manifest["artifacts"]["fan_in_artifact_contract"]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G8"], "blocked")

    def test_g8_blocked_when_fan_in_same_commit_false(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            manifest["artifacts"]["fan_in_artifact_contract"] = [
                str(write_json(root / "fan-in-artifact-contract-20260809T000000Z.json",
                               {"ok": True, "same_commit": False,
                                "same_source_digest": True}))
            ]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G8"], "blocked",
                             "fan-in ok without same_commit must still block G8")

    def test_g7_blocked_when_n3_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            del manifest["artifacts"]["N3"]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G7"], "blocked",
                             "G7 (Real OpenCode) requires the N3 envelope artifact")

    def test_g7_legacy_opencode_old_candidate_rejected(self) -> None:
        # G7 evidence is the N3 envelope artifact (candidate bound to C_final).
        # A legacy opencode report with candidate_id != C_final is rejected and
        # must never contribute to G7 (VERIFICATION.md §6: G7 candidate id must
        # equal C_final).
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            manifest["artifacts"]["opencode_real_routed"] = [
                str(write_json(root / "opencode-real-routed" /
                               "real-routed-20260809T000000Z.json",
                               {"candidate_id": "G7", "status": "pass"}))
            ]
            proc = run_aggregator(manifest, root)
            report = json.loads(proc.stdout)
            verdicts = report["artifact_verdicts"]["opencode_real_routed"]
            self.assertFalse(verdicts[0]["ok"],
                             "legacy opencode candidate_id must equal C_final")
            self.assertTrue(
                any("old-candidate" in v for v in verdicts[0]["violations"])
            )
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G7"], "passed",
                             "G7 passes only on the N3 envelope artifact, never on "
                             "legacy opencode reports")

    def test_old_candidate_envelope_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            manifest["artifacts"]["N3"] = [
                str(write_json(root / "N3-opencode.json",
                               make_artifact("N3", commit="f" * 40)))
            ]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            states = {g["gate"]: g["state"] for g in report["gates"]}
            self.assertEqual(states["G7"], "blocked",
                             "old-candidate N3 evidence must block G7")
            verdicts = report["artifact_verdicts"]["N3"]
            self.assertFalse(verdicts[0]["ok"])

    def test_source_digest_mismatch_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            bad = make_artifact("N1")
            bad["source_digest"] = "1" * 64
            bad["artifact_sha256"] = canonical_sha(bad)
            manifest["artifacts"]["N1"] = [
                str(write_json(root / "N1-receipt.json", bad))
            ]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            self.assertFalse(report["artifact_verdicts"]["N1"][0]["ok"])

    def test_pass_only_evidence_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            manifest["artifacts"]["N1"] = [
                str(write_json(root / "N1-receipt.json",
                               make_artifact("N1", omit_envelope=True)))
            ]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            verdicts = report["artifact_verdicts"]["N1"]
            self.assertFalse(verdicts[0]["ok"],
                             "status=pass without the envelope is pass-only evidence")
            self.assertTrue(
                any("missing envelope field" in v for v in verdicts[0]["violations"])
            )

    def test_stale_evidence_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            manifest["artifacts"]["N5"] = [
                str(write_json(root / "N5-cc-switch.json",
                               make_artifact("N5", started="2026-08-09T09:30:00Z")))
            ]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            verdicts = report["artifact_verdicts"]["N5"]
            self.assertFalse(verdicts[0]["ok"])
            self.assertTrue(any("stale" in v for v in verdicts[0]["violations"]))

    def test_missing_artifact_path_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            manifest["artifacts"]["N6"] = [str(root / "does-not-exist.json")]
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            self.assertFalse(report["artifact_verdicts"]["N6"][0]["ok"])
            self.assertIn("missing", report["artifact_verdicts"]["N6"][0]["violations"][0])

    def _mutate_u9(self, root, mutator):
        manifest = write_full_pass_evidence(root)
        u9_path = root / "N10-U9-coverage.json"
        u9 = json.loads(u9_path.read_text(encoding="utf-8"))
        mutator(u9)
        u9["artifact_sha256"] = canonical_sha(u9)
        u9_path.write_text(json.dumps(u9, indent=2) + "\n", encoding="utf-8")
        return run_aggregator(manifest, root)

    def test_u9_duplicate_entry_ids_block(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)

            def dup(u9):
                u9["coverage"]["per_entry"][0]["id"] = u9["coverage"]["per_entry"][1]["id"]

            proc = self._mutate_u9(root, dup)
            self.assertEqual(proc.returncode, 1, "duplicate U9 ids must block")
            report = json.loads(proc.stdout)
            self.assertTrue(any("duplicate entry ids" in g["detail"] for g in report["gates"]))

    def test_u9_producer_mismatch_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)

            def mism(u9):
                u9["coverage"]["per_entry"][0]["producer_node"] = "N2"

            proc = self._mutate_u9(root, mism)
            self.assertEqual(proc.returncode, 1, "U9 producer mismatch must block")
            report = json.loads(proc.stdout)
            self.assertTrue(any("producer_node" in g["detail"] for g in report["gates"]))

    def test_u9_verified_set_mismatch_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)

            def idset(u9):
                u9["coverage"]["verified_ids"] = u9["coverage"]["verified_ids"][:-1]

            proc = self._mutate_u9(root, idset)
            self.assertEqual(proc.returncode, 1, "U9 verified_ids set mismatch must block")
            report = json.loads(proc.stdout)
            self.assertTrue(any("verified_ids" in g["detail"] for g in report["gates"]))

    def test_report_schema(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 0)
            report = json.loads(proc.stdout)
            for field in ("ok", "mode", "generated_at", "host_platform", "release_ready",
                          "release_ready_hint", "candidate_commit", "source_digest",
                          "registry_digest", "corpus_digests", "expected_version",
                          "manifest_path", "blockers", "gates", "evidence",
                          "artifact_verdicts", "manifest_errors", "report_path"):
                self.assertIn(field, report, f"report missing field {field}")
            self.assertEqual(report["mode"], "release-gate-status")
            self.assertEqual(len(report["gates"]), 10, "G0-G9 must all be reported")

    def test_aggregator_never_reads_scripts_parse_output(self) -> None:
        source = AGGREGATOR.read_text(encoding="utf-8")
        self.assertNotIn("release_scripts_parse", source,
                         "aggregator must not treat the contract test's parse-only check "
                         "as evidence")
        self.assertIn("--report-only", source)
        self.assertIn("return 1", source)
        self.assertIn("blocked", source)
        self.assertIn("unavailable", source)
        self.assertIn("release-verification-", source)
        self.assertIn("fan-in-artifact-contract-", source)
        self.assertIn("G6", source)
        self.assertIn("release-verification", source)
        self.assertIn("NOT executed", source)
        self.assertIn("G8", source)
        self.assertIn("fan-in-artifact-contract", source)
        self.assertIn("双平台同 commit", source)


if __name__ == "__main__":
    unittest.main()


    def test_u9_duplicate_ids_block_g9(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            u9_path = root / "N10-U9-coverage.json"
            u9 = json.loads(u9_path.read_text(encoding="utf-8"))
            coverage = u9["coverage"]
            coverage["per_entry"][0]["id"] = coverage["per_entry"][1]["id"]
            u9["coverage"] = coverage
            u9["artifact_sha256"] = _recompute(u9)
            u9_path.write_text(json.dumps(u9, indent=2) + "\n", encoding="utf-8")
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            self.assertEqual(report["gates"][-1]["state"], "blocked")
            detail = report["gates"][-1]["detail"]
            self.assertIn("duplicate", detail)

    def test_u9_producer_mismatch_blocks_g9(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = write_full_pass_evidence(root)
            u9_path = root / "N10-U9-coverage.json"
            u9 = json.loads(u9_path.read_text(encoding="utf-8"))
            coverage = u9["coverage"]
            registry = json.loads(REGISTRY_PATH.read_text(encoding="utf-8"))
            first = registry["entries"][0]
            entry = next(
                item for item in coverage["per_entry"] if item["id"] == first["id"]
            )
            entry["producer_node"] = "N2"
            u9["coverage"] = coverage
            u9["artifact_sha256"] = _recompute(u9)
            u9_path.write_text(json.dumps(u9, indent=2) + "\n", encoding="utf-8")
            proc = run_aggregator(manifest, root)
            self.assertEqual(proc.returncode, 1)
            report = json.loads(proc.stdout)
            self.assertEqual(report["gates"][-1]["state"], "blocked")
            detail = report["gates"][-1]["detail"]
            self.assertIn("producer_node", detail)
