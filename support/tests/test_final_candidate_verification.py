"""Direct contract tests for the final-candidate verification orchestrator.

The orchestrator (support/scripts/run_final_candidate_verification.py)
implements the N0 -> N1-N8/N10 -> N9 verification DAG.  These tests pin the
contract that matters for release honesty:

  * the DAG is acyclic, N0 runs first, N1-N8/N10 in the middle, N9 last;
  * every evidence artifact must carry the unified envelope bound to C_final,
    source/registry/corpus digests, dirty=false, and a recomputable
    artifact_sha256;
  * missing, stale (started before the N0 freeze finished), old-candidate,
    digest-mismatch, dirty, or pass-only evidence is rejected;
  * evidence is never selected by mtime/latest;
  * N9 emits exactly signed | public-unsigned | blocked; unsigned always keeps
    release_ready=false and distribution_ready_public=false.

Run:  python3 support/tests/test_final_candidate_verification.py
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
ORCHESTRATOR = REPO_ROOT / "support" / "scripts" / "run_final_candidate_verification.py"

C_FINAL = "a" * 40
SOURCE_DIGEST = "b" * 64
REGISTRY_DIGEST = "c" * 64
CORPUS_DIGESTS = {"benchmark": "d" * 64, "replay": "e" * 64}
N0_FINISHED = "2026-08-09T10:00:00Z"
NODE_STARTED = "2026-08-09T10:00:01Z"
NODE_FINISHED = "2026-08-09T11:00:00Z"
N0_STARTED = "2026-08-09T09:00:00Z"

EXPECTED = {
    "commit": C_FINAL,
    "source_digest": SOURCE_DIGEST,
    "registry_digest": REGISTRY_DIGEST,
    "corpus_digests": CORPUS_DIGESTS,
    "version": "0.0.1",
    "producer_revision": "test-producer@1",
}


def load_orchestrator_module():
    spec = importlib.util.spec_from_file_location(
        "run_final_candidate_verification", ORCHESTRATOR
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def canonical_sha(data: dict) -> str:
    """Independent implementation of the documented artifact_sha256 rule."""
    body = {key: value for key, value in data.items() if key != "artifact_sha256"}
    raw = json.dumps(
        body, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def make_artifact(
    node_id: str,
    started: str,
    finished: str,
    *,
    commit: str = C_FINAL,
    source_digest: str = SOURCE_DIGEST,
    registry_digest: str = REGISTRY_DIGEST,
    corpus: dict | None = None,
    version: str = "0.0.1",
    status: str = "pass",
    dirty: bool = False,
    extra: dict | None = None,
    omit_envelope: bool = False,
) -> dict:
    data: dict = {
        "schema_version": "1",
        "node_id": node_id,
        "status": status,
        "reason_codes": [],
        "candidate_commit": commit,
        "source_digest": source_digest,
        "dirty": dirty,
        "registry_digest": registry_digest,
        "corpus_digests": corpus if corpus is not None else CORPUS_DIGESTS,
        "started_at": started,
        "finished_at": finished,
        "platform": {"os": "darwin", "arch": "arm64", "toolchain": "python3"},
        "producer_revision": "test-producer@1",
    }
    if node_id == "N0":
        data["version"] = version
    if extra:
        data.update(extra)
    if omit_envelope:
        data.pop("candidate_commit", None)
    data["artifact_sha256"] = canonical_sha(data)
    return data


def write_node(root: Path, node_id: str, data: dict) -> Path:
    module = load_orchestrator_module()
    root.mkdir(parents=True, exist_ok=True)
    path = root / module.NODE_ARTIFACTS[node_id][0]
    path.write_text(json.dumps(data), encoding="utf-8")
    return path


def write_full_evidence(root: Path, *, signing: bool = False) -> None:
    module = load_orchestrator_module()
    for role, filenames in module.NODE_ARTIFACTS.items():
        started = N0_STARTED if role == "N0" else NODE_STARTED
        finished = N0_FINISHED if role == "N0" else NODE_FINISHED
        for filename in filenames:
            extra: dict = {}
            if role == "N7" and signing:
                extra["signing"] = {
                    "developer_id": True,
                    "notarized": True,
                    "gatekeeper": True,
                }
            if role == "N8" and signing:
                extra["signing"] = {"authenticode": True}
            if role == "N10-U9":
                extra = {"registry_total": 117, "verified_total": 117}
            if role == "N8" and filename == "fan-in-artifact-contract.json":
                extra["same_commit"] = True
                extra["same_source_digest"] = True
            data = make_artifact(role, started, finished, extra=extra)
            path = root / filename
            path.write_text(json.dumps(data), encoding="utf-8")
    # Legacy G6 evidence: executed PowerShell release-verification report and
    # handoff manifest (declared explicitly, never discovered).
    (root / "release-verification-20260809T000000Z.json").write_text(
        json.dumps({
            "ok": True,
            "mode": "release-verification",
            "steps": [
                {"id": "release_artifact_redaction_audit", "ok": True},
                {"id": "release_cleanup_audit", "ok": True},
            ],
        }),
        encoding="utf-8",
    )
    (root / "release-handoff-manifest-20260809T000000Z.json").write_text(
        json.dumps({"ok": True}), encoding="utf-8"
    )


def run_orchestrator(
    root: Path,
    *,
    extra_args: list[str] | None = None,
    report_only: bool = False,
    expected: dict | None = None,
) -> subprocess.CompletedProcess:
    exp = expected or EXPECTED
    cmd = [
        sys.executable,
        str(ORCHESTRATOR),
        "--candidate-commit",
        exp["commit"],
        "--source-digest",
        exp["source_digest"],
        "--registry-digest",
        exp["registry_digest"],
        "--corpus-digests",
        json.dumps(exp["corpus_digests"]),
        "--expected-version",
        "0.0.1",
        "--producer-revision",
        "test-producer@1",
        "--artifacts-root",
        str(root),
        "--output-dir",
        str(root),
        "--evidence-report-out",
        str(root / "final-candidate-evidence.md"),
    ]
    if extra_args:
        cmd.extend(extra_args)
    cmd.extend([
        "--legacy-evidence",
        f"release_verification={root / 'release-verification-20260809T000000Z.json'}",
        "--legacy-evidence",
        f"release_handoff_manifest={root / 'release-handoff-manifest-20260809T000000Z.json'}",
    ])
    if report_only:
        cmd.append("--report-only")
    return subprocess.run(cmd, capture_output=True, text=True)


def parse_report(proc: subprocess.CompletedProcess) -> dict:
    return json.loads(proc.stdout)


class FinalCandidateVerificationContractTests(unittest.TestCase):

    def test_orchestrator_exists_and_parses(self) -> None:
        self.assertTrue(ORCHESTRATOR.is_file(), "orchestrator script is missing")
        compile(ORCHESTRATOR.read_text(encoding="utf-8"), str(ORCHESTRATOR), "exec")

    def test_dag_is_acyclic_n0_first_n9_last(self) -> None:
        module = load_orchestrator_module()
        ok, order = module.validate_dag()
        self.assertTrue(ok, f"DAG must be valid: {order}")
        nodes = order.split(" -> ")
        self.assertEqual(nodes[0], "N0", "N0 must run first")
        self.assertEqual(nodes[-1], "N9", "N9 must run last")
        self.assertEqual(
            set(nodes[1:-1]), set(module.FAN_OUT_NODES), "fan-out must be N1-N8/N10"
        )
        cyclic_ok, _ = module.dag_is_acyclic([("A", "B"), ("B", "A")])
        self.assertFalse(cyclic_ok, "cycle must be detected")

    def test_canonical_sha_rule(self) -> None:
        module = load_orchestrator_module()
        data = make_artifact("N1", NODE_STARTED, NODE_FINISHED)
        self.assertEqual(
            module.gate_status.recompute_artifact_sha256(data),
            canonical_sha(data),
            "aggregator recompute must match the documented canonical rule",
        )
        tampered = dict(data)
        tampered["artifact_sha256"] = "0" * 64
        self.assertNotEqual(
            module.gate_status.recompute_artifact_sha256(tampered), "0" * 64
        )

    def test_dry_run_ok_without_artifacts_and_writes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proc = run_orchestrator(root, extra_args=["--dry-run"])
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn("N0", proc.stdout)
            self.assertIn("N9", proc.stdout)
            self.assertIn("DAG", proc.stdout)
            self.assertFalse((root / "N9-release-status.json").exists(),
                             "dry-run must not write the N9 report")
            self.assertEqual(sorted(p.name for p in root.iterdir()), [])

    def test_dry_run_requires_candidate_commit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cmd = [sys.executable, str(ORCHESTRATOR), "--dry-run"]
            proc = subprocess.run(cmd, capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0,
                                "candidate commit is required even for dry-run")

    def test_dry_run_rejects_malformed_arguments(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proc = run_orchestrator(
                root,
                extra_args=["--dry-run", "--candidate-commit", "not-a-commit"],
            )
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("candidate", proc.stdout.lower())
            proc = run_orchestrator(
                root,
                extra_args=["--dry-run", "--corpus-digests", '{"benchmark": "nope"}'],
            )
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("corpus", proc.stdout.lower())

    def test_missing_n0_blocks_everything(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            proc = run_orchestrator(Path(tmp))
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertIs(report["release_ready"], False)
            self.assertIs(report["n0_gate"]["ok"], False)

    def test_old_candidate_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_node(root, "N0", make_artifact("N0", N0_STARTED, N0_FINISHED,
                                                 commit="f" * 40))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertFalse(report["node_verdicts"]["N0"]["ok"])
            self.assertTrue(
                any("candidate_commit" in v["violations"][0]
                    for v in report["node_verdicts"]["N0"]["artifacts"]),
                "old candidate must be rejected with an explicit violation",
            )

    def test_source_digest_mismatch_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_node(root, "N0", make_artifact("N0", N0_STARTED, N0_FINISHED,
                                                 source_digest="1" * 64))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertFalse(report["node_verdicts"]["N0"]["ok"])
            self.assertIn("source_digest",
                          report["node_verdicts"]["N0"]["artifacts"][0]["violations"][0])

    def test_registry_digest_mismatch_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_node(root, "N0", make_artifact("N0", N0_STARTED, N0_FINISHED,
                                                 registry_digest="2" * 64))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertFalse(report["node_verdicts"]["N0"]["ok"])
            self.assertIn("registry_digest",
                          report["node_verdicts"]["N0"]["artifacts"][0]["violations"][0])

    def test_corpus_digest_mismatch_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_node(root, "N0", make_artifact(
                "N0", N0_STARTED, N0_FINISHED,
                corpus={"benchmark": "3" * 64, "replay": "e" * 64},
            ))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertFalse(report["node_verdicts"]["N0"]["ok"])
            self.assertIn("corpus_digests",
                          report["node_verdicts"]["N0"]["artifacts"][0]["violations"][0])

    def test_dirty_true_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_node(root, "N0", make_artifact("N0", N0_STARTED, N0_FINISHED,
                                                 dirty=True))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertFalse(report["node_verdicts"]["N0"]["ok"])
            self.assertIn("dirty",
                          report["node_verdicts"]["N0"]["artifacts"][0]["violations"][0])

    def test_pass_only_evidence_rejected(self) -> None:
        # status=pass without the envelope: must be rejected.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_node(root, "N0", make_artifact("N0", N0_STARTED, N0_FINISHED,
                                                 omit_envelope=True))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertFalse(report["node_verdicts"]["N0"]["ok"])
            self.assertTrue(
                any("missing envelope field" in v["violations"][0]
                    for v in report["node_verdicts"]["N0"]["artifacts"]),
                "status=pass without the envelope is pass-only evidence",
            )

    def test_artifact_sha256_mismatch_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            data = make_artifact("N0", N0_STARTED, N0_FINISHED)
            data["artifact_sha256"] = "0" * 64
            write_node(root, "N0", data)
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertFalse(report["node_verdicts"]["N0"]["ok"])
            self.assertIn("artifact_sha256",
                          report["node_verdicts"]["N0"]["artifacts"][0]["violations"][0])

    def test_stale_evidence_rejected_by_timestamp_not_mtime(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_full_evidence(root)
            # N3 started before the N0 freeze finished -> stale.
            stale = make_artifact("N3", "2026-08-09T09:30:00Z", NODE_FINISHED)
            path = write_node(root, "N3", stale)
            # Give the stale file a NEWER mtime than every other artifact:
            # mtime must never be the selection/rejection mechanism.
            newest = max(p.stat().st_mtime for p in root.iterdir() if p.is_file())
            os.utime(path, (newest + 60, newest + 60))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertFalse(report["node_verdicts"]["N3"]["ok"])
            self.assertTrue(
                any("stale" in v["violations"][0]
                    for v in report["node_verdicts"]["N3"]["artifacts"]),
                "evidence started before the N0 freeze is stale and must be rejected",
            )

    def test_never_selects_evidence_by_mtime(self) -> None:
        source = ORCHESTRATOR.read_text(encoding="utf-8")
        self.assertNotIn("latest_file", source)
        self.assertNotIn("st_mtime", source)
        self.assertNotIn("glob", source)
        self.assertIn("artifacts_root", source)

    def test_old_candidate_downstream_node_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_full_evidence(root)
            write_node(root, "N3", make_artifact(
                "N3", NODE_STARTED, NODE_FINISHED, commit="f" * 40,
            ))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertFalse(report["node_verdicts"]["N3"]["ok"])

    def test_unsigned_conclusion_when_no_signing_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_full_evidence(root)
            proc = run_orchestrator(root, report_only=True)
            self.assertEqual(proc.returncode, 0)
            report = parse_report(proc)
            self.assertEqual(report["conclusion"], "public-unsigned")
            self.assertIs(report["release_ready"], False,
                          "unsigned always means release_ready=false")
            self.assertIs(report["distribution_ready_public"], False)
            self.assertIn("unsigned", " ".join(report["reason_codes"]))
            self.assertTrue(all(g["state"] == "passed" for g in report["gates"]),
                            "public-unsigned requires every gate passed")

    def test_signed_conclusion_with_full_signing_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_full_evidence(root, signing=True)
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 0, proc.stdout)
            report = parse_report(proc)
            self.assertEqual(report["conclusion"], "signed")
            self.assertIs(report["release_ready"], True)
            self.assertIs(report["distribution_ready_public"], True)
            self.assertTrue(all(g["state"] == "passed" for g in report["gates"]))
            self.assertEqual(report["coverage"]["verified_total"], 117)

    def test_partial_signing_never_yields_signed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_full_evidence(root, signing=True)
            # Windows Authenticode attestation missing -> must NOT be signed.
            write_node(root, "N8", make_artifact("N8", NODE_STARTED, NODE_FINISHED,
                                                 extra={"same_commit": True,
                                                        "same_source_digest": True}))
            proc = run_orchestrator(root, report_only=True)
            report = parse_report(proc)
            self.assertNotEqual(report["conclusion"], "signed")
            self.assertIs(report["release_ready"], False)

    def test_missing_n10_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_full_evidence(root, signing=True)
            (root / "N10-U9-coverage.json").unlink()
            proc = run_orchestrator(root, report_only=True)
            report = parse_report(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertIs(report["release_ready"], False)
            self.assertFalse(report["node_verdicts"]["N10-U9"]["ok"])

    def test_coverage_below_117_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_full_evidence(root, signing=True)
            write_node(root, "N10-U9", make_artifact(
                "N10-U9", NODE_STARTED, NODE_FINISHED,
                extra={"registry_total": 117, "verified_total": 116},
            ))
            proc = run_orchestrator(root, report_only=True)
            report = parse_report(proc)
            self.assertEqual(report["conclusion"], "blocked")
            self.assertFalse(report["coverage"]["ok"])

    def test_version_mismatch_blocks_n0(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_node(root, "N0", make_artifact("N0", N0_STARTED, N0_FINISHED,
                                                 version="9.9.9"))
            proc = run_orchestrator(root)
            self.assertEqual(proc.returncode, 1)
            report = parse_report(proc)
            self.assertFalse(report["node_verdicts"]["N0"]["ok"])
            self.assertIn("version",
                          report["node_verdicts"]["N0"]["artifacts"][0]["violations"][0])

    def test_n9_report_schema_and_self_hash(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_full_evidence(root)
            proc = run_orchestrator(root, report_only=True)
            self.assertEqual(proc.returncode, 0)
            report = parse_report(proc)
            for field in ("schema_version", "node_id", "status", "conclusion",
                          "release_ready", "distribution_ready_public", "reason_codes",
                          "candidate_commit", "source_digest", "registry_digest",
                          "corpus_digests", "version", "dirty", "started_at",
                          "finished_at", "platform", "producer_revision",
                          "n0_gate", "node_verdicts", "gates", "blockers",
                          "signing", "coverage", "manifest", "artifact_paths",
                          "artifact_sha256"):
                self.assertIn(field, report, f"N9 report missing field {field}")
            self.assertEqual(report["node_id"], "N9")
            self.assertEqual(report["schema_version"], "1")
            self.assertEqual(
                canonical_sha(report),
                report["artifact_sha256"],
                "N9 report self-hash must follow the canonical rule",
            )
            n9_path = root / "N9-release-status.json"
            self.assertTrue(n9_path.is_file())
            on_disk = json.loads(n9_path.read_text(encoding="utf-8"))
            self.assertEqual(on_disk["artifact_sha256"], report["artifact_sha256"])
            evidence_report = root / "final-candidate-evidence.md"
            self.assertTrue(evidence_report.is_file(),
                            "evidence report must be rendered from the tracked template")
            self.assertIn(C_FINAL, evidence_report.read_text(encoding="utf-8"))
            self.assertIn("public-unsigned", evidence_report.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
