"""Contract tests for the fixed-name N1-N8/N10-U0..U9 evidence producer/stager
(C8 / PR-C8C).

Run:  python3 support/tests/test_n_evidence_producer.py

Pins the fail-closed staging contract (docs/releases/v0.0.1/VERIFICATION.md
§1/§4/§5, EVIDENCE.md §3/§4):

  * staging refuses to bind without a pass N0 artifact;
  * every artifact carries the unified envelope with recomputed
    artifact_sha256 and exact candidate/registry/corpus binding;
  * unsupported/external lanes emit blocked with precise reason codes and are
    never promoted by file existence or self-reported status;
  * U9 coverage seals 117/117 verified ONLY from prior fixed artifacts whose
    envelopes revalidate; 116/117, duplicate ids, producer mismatch and
    artifact-name mismatch all block; partial invocations still consume
    previously staged artifacts;
  * U5 blocks when the tracked C7A baseline is absent (never re-freezes);
  * mock/fixture/pre-N0/pass-only/unexecuted upstream output is rejected.
"""
from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
RUNNER = REPO_ROOT / "support" / "scripts" / "run_n_evidence_producer.py"
REGISTRY = REPO_ROOT / "support" / "fixtures" / "unified-capability-registry-v1.json"

C_FINAL = "a" * 40
SOURCE_DIGEST = "b" * 64
REGISTRY_DIGEST = "c" * 64
CORPUS_DIGESTS = {
    "benchmark": "d" * 64,
    "replay": "e" * 64,
    "fuzz": "f" * 64,
    "fault": "g" * 64,
    "quality": "h" * 64,
    "optical": "i" * 64,
}
ENVELOPE_FIELDS = [
    "schema_version", "node_id", "status", "reason_codes", "candidate_commit",
    "source_digest", "dirty", "registry_digest", "corpus_digests", "started_at",
    "finished_at", "platform", "producer_revision", "artifact_sha256",
]


def canonical_sha(data: dict) -> str:
    body = {key: value for key, value in data.items() if key != "artifact_sha256"}
    raw = json.dumps(
        body, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def make_envelope(node_id: str, status: str = "pass", **extra) -> dict:
    data = {
        "schema_version": "1",
        "node_id": node_id,
        "status": status,
        "reason_codes": [] if status == "pass" else ["test-synthetic"],
        "candidate_commit": C_FINAL,
        "source_digest": SOURCE_DIGEST,
        "dirty": False,
        "registry_digest": REGISTRY_DIGEST,
        "corpus_digests": CORPUS_DIGESTS,
        "started_at": "2026-08-10T00:00:00Z",
        "finished_at": "2026-08-10T00:01:00Z",
        "platform": {"os": "test", "arch": "test", "toolchain": "py3"},
        "producer_revision": "test@1",
    }
    data.update(extra)
    data["artifact_sha256"] = canonical_sha(data)
    return data


def write_n0(dir_path: Path) -> dict:
    n0 = make_envelope("N0", version="0.0.1")
    (dir_path / "N0-preflight.json").write_text(
        json.dumps(n0, indent=2) + "\n", encoding="utf-8"
    )
    return n0


def make_clean_worktree() -> Path:
    """A clean git worktree so the dirty gate does not block staging."""
    tmp = Path(tempfile.mkdtemp())
    subprocess.run(["git", "init", "-q"], cwd=tmp, check=True)
    subprocess.run(
        ["git", "commit", "-q", "--allow-empty", "-m", "test"],
        cwd=tmp, check=True,
    )
    return tmp


def run_runner(
    artifacts_root: Path,
    worktree: Path,
    *,
    nodes: str,
    extra: list[str] | None = None,
) -> subprocess.CompletedProcess:
    cmd = [
        sys.executable, str(RUNNER),
        "--n0-artifact", str(artifacts_root / "N0-preflight.json"),
        "--candidate-commit", C_FINAL,
        "--artifacts-root", str(artifacts_root),
        "--worktree-root", str(worktree),
        "--nodes", nodes,
    ]
    if extra:
        cmd.extend(extra)
    return subprocess.run(cmd, capture_output=True, text=True)


def stage_all_pass_artifacts(dir_path: Path) -> None:
    """Stage a synthetic pass envelope for every node (U9 excluded) using the
    real registry producer mapping for the coverage trace."""
    registry = json.loads(REGISTRY.read_text(encoding="utf-8"))
    nodes = {e["producer"].split(":", 1)[1] for e in registry["entries"]}
    for node in sorted(nodes):
        if node == "N0":
            continue  # N0-preflight.json is written by write_n0 (with version)
        filename = {
            "N0": "N0-preflight.json",
            "N1": "N1-receipt.json", "N2": "N2-codex.json", "N3": "N3-opencode.json",
            "N4": "N4-config.json", "N5": "N5-cc-switch.json", "N6": "N6-ui.json",
            "N7": "N7-macos.json", "N8": "N8-windows.json", "N9": "N9-release-status.json",
            "N10": "N10-unified-proof.json",
            "N10-U0": "N10-U0-contract.json", "N10-U1": "N10-U1-registry.json",
            "N10-U2": "N10-U2-property-fuzz.json", "N10-U3": "N10-U3-fault.json",
            "N10-U4": "N10-U4-replay-quality.json", "N10-U5": "N10-U5-perf.json",
            "N10-U6": "N10-U6-provider-paired.json", "N10-U7": "N10-U7-optical-active.json",
            "N10-U8": "N10-U8-platform.json", "N10-U9": "N10-U9-coverage.json",
        }.get(node)
        if filename:
            (dir_path / filename).write_text(
                json.dumps(make_envelope(node), indent=2) + "\n", encoding="utf-8"
            )


class NEvidenceProducerContractTest(unittest.TestCase):
    def test_refuses_bind_without_pass_n0(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            n0 = make_envelope("N0", status="failed", version="0.0.1")
            (root / "N0-preflight.json").write_text(
                json.dumps(n0, indent=2) + "\n", encoding="utf-8"
            )
            proc = run_runner(root, make_clean_worktree(), nodes="N10-U0")
            self.assertNotEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("not pass", proc.stdout + proc.stderr)

    def test_refuses_bind_without_n0_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proc = run_runner(root, make_clean_worktree(), nodes="N10-U0")
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("N0 artifact is required", proc.stdout + proc.stderr)

    def test_external_lanes_emit_blocked_not_executed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            proc = run_runner(
                root, make_clean_worktree(),
                nodes="N2,N5,N6,N10-U6,N10-U7",
                extra=["--skip-local"],
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            for filename, node_id in (
                ("N2-codex.json", "N2"),
                ("N5-cc-switch.json", "N5"),
                ("N6-ui.json", "N6"),
                ("N10-U6-provider-paired.json", "N10-U6"),
                ("N10-U7-optical-active.json", "N10-U7"),
            ):
                artifact = json.loads((root / filename).read_text(encoding="utf-8"))
                self.assertEqual(artifact["status"], "blocked", filename)
                self.assertTrue(artifact["reason_codes"], filename)
                self.assertIn("not-executed", artifact["reason_codes"][0], filename)
                self.assertIs(artifact["dirty"], False)
                for field in ENVELOPE_FIELDS:
                    self.assertIn(field, artifact, f"{filename} missing {field}")
                self.assertEqual(
                    artifact["artifact_sha256"], canonical_sha(artifact)
                )

    def test_upstream_evidence_without_source_digest_remains_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            evidence = root / "tauri-ui-acceptance" / "missing-digest.json"
            evidence.parent.mkdir(parents=True)
            artifact = make_envelope("N6")
            artifact.pop("source_digest")
            artifact["artifact_sha256"] = canonical_sha(artifact)
            evidence.write_text(json.dumps(artifact) + "\n", encoding="utf-8")
            proc = run_runner(
                root, make_clean_worktree(), nodes="N6", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            staged = json.loads((root / "N6-ui.json").read_text(encoding="utf-8"))
            self.assertEqual(staged["status"], "blocked")
            self.assertIn(
                "source_digest missing or mismatched with frozen candidate",
                staged["reason_codes"],
            )

    def test_u7_demo_fixture_rejected_as_real_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            fixture = root / "optical-exact-active" / "demo-evidence.json"
            fixture.parent.mkdir(parents=True)
            fixture.write_text(
                json.dumps(
                    make_envelope(
                        "N10-U7",
                        authority="demo-authority",
                        provider_id="demo-provider",
                        model="demo-model",
                        api_revision="demo-rev",
                        fresh_paired_eval=True,
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            proc = run_runner(
                root, make_clean_worktree(), nodes="N10-U7", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads(
                (root / "N10-U7-optical-active.json").read_text(encoding="utf-8")
            )
            self.assertEqual(artifact["status"], "blocked")
            self.assertTrue(
                any("demo" in reason for reason in artifact["reason_codes"]),
                artifact["reason_codes"],
            )


    def test_u7_requires_full_exact_active_runtime_contract(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            incomplete = root / "optical-exact-active" / "real-provider.json"
            incomplete.parent.mkdir(parents=True)
            incomplete.write_text(
                json.dumps(
                    make_envelope(
                        "N10-U7",
                        authority="prod-authority",
                        provider_id="anthropic-main",
                        route_id="route-main",
                        model="claude-exact",
                        api_revision="anthropic-messages-v1",
                        fresh_paired_eval=True,
                        automatic_active_observed=False,
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            proc = run_runner(
                root, make_clean_worktree(), nodes="N10-U7", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads(
                (root / "N10-U7-optical-active.json").read_text(encoding="utf-8")
            )
            self.assertEqual(artifact["status"], "blocked")
            self.assertTrue(
                any("automatic active" in reason for reason in artifact["reason_codes"]),
                artifact["reason_codes"],
            )

    def test_u7_preserves_original_validation_reasons_without_rewriting(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            invalid = root / "optical-exact-active" / "real-provider.json"
            invalid.parent.mkdir(parents=True)
            invalid.write_text(
                json.dumps(
                    make_envelope(
                        "N10-U7",
                        authority="prod-authority",
                        provider_id="anthropic-main",
                        route_id="route-main",
                        model="claude-exact",
                        api_revision="anthropic-messages-v1",
                        fresh_paired_eval=True,
                        automatic_active_observed=True,
                        exact_route_match=True,
                        capability_attested=True,
                        eval_gate_passed=True,
                        resource_gate_passed=True,
                        patch_validated=True,
                        post_validator_passed=True,
                        ledger_completed=True,
                        unsupported_protocol_preserved=True,
                        failure_preservation_passed=False,
                        failure_reasons=["provider returned literal sentinel failure"],
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            proc = run_runner(
                root, make_clean_worktree(), nodes="N10-U7", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads(
                (root / "N10-U7-optical-active.json").read_text(encoding="utf-8")
            )
            self.assertEqual(artifact["status"], "blocked")
            self.assertIn(
                "provider returned literal sentinel failure", artifact["reason_codes"]
            )
    def test_u7_accepts_only_complete_real_exact_active_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            evidence = root / "optical-exact-active" / "real-provider.json"
            evidence.parent.mkdir(parents=True)
            evidence.write_text(
                json.dumps(make_envelope(
                    "N10-U7",
                    authority="prod-authority",
                    provider_id="anthropic-main",
                    route_id="route-main",
                    model="claude-exact",
                    api_revision="anthropic-messages-v1",
                    renderer_profile_digest="renderer",
                    font_digest="font",
                    transform_contract_digest="transform",
                    corpus_digest="corpus",
                    tokenizer_revision="tokenizer",
                    pricing_revision="pricing",
                    cache_policy_revision="cache",
                    eval_artifact_digest="eval",
                    fresh_paired_eval=True,
                    automatic_active_observed=True,
                    exact_route_match=True,
                    capability_attested=True,
                    eval_gate_passed=True,
                    resource_gate_passed=True,
                    patch_validated=True,
                    post_validator_passed=True,
                    ledger_completed=True,
                    unsupported_protocol_preserved=True,
                    failure_preservation_passed=True,
                    ordinary_image_entry_used=False,
                    ui_switch_required=False,
                    preserve_text_on_gate_failure=True,
                    provider_response_preserved_verbatim=True,
                    claimable_actual_usage=True,
                    baseline_request_id="baseline-request",
                    optimized_request_id="optimized-request",
                )) + "\n",
                encoding="utf-8",
            )
            proc = run_runner(
                root, make_clean_worktree(), nodes="N10-U7", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads(
                (root / "N10-U7-optical-active.json").read_text(encoding="utf-8")
            )
            self.assertEqual(artifact["status"], "pass", artifact["reason_codes"])

    def test_u5_blocks_when_tracked_baseline_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            baseline = (
                REPO_ROOT / "code" / "crates" / "dtoken-benchmark" / "corpus" / "baseline.json"
            )
            if baseline.is_file():
                self.skipTest("tracked baseline present; negative not exercisable")
            proc = run_runner(root, make_clean_worktree(), nodes="N10-U5")
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads(
                (root / "N10-U5-perf.json").read_text(encoding="utf-8")
            )
            self.assertEqual(artifact["status"], "blocked")
            self.assertIn("u5-tracked-baseline-missing", artifact["reason_codes"])

    def test_u9_partial_invocation_consumes_prior_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            stage_all_pass_artifacts(root)
            # Only U9 is requested; it must consume the prior staged artifacts.
            proc = run_runner(
                root, make_clean_worktree(), nodes="N10-U9", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads(
                (root / "N10-U9-coverage.json").read_text(encoding="utf-8")
            )
            self.assertEqual(artifact["status"], "pass", artifact["reason_codes"])
            coverage = artifact["coverage"]
            self.assertEqual(coverage["total"], 117)
            self.assertEqual(coverage["verified"], 117)

    def test_n1_rejects_incomplete_accounting_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            receipt = root / "receipt" / "post-n0.json"
            receipt.parent.mkdir(parents=True)
            artifact = make_envelope(
                "N1",
                started_at="2026-08-10T00:02:00Z",
                finished_at="2026-08-10T00:03:00Z",
                request_receipts=[{"request_id": "request-1"}],
            )
            receipt.write_text(json.dumps(artifact) + "\n", encoding="utf-8")
            proc = run_runner(
                root, make_clean_worktree(), nodes="N1", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            staged = json.loads((root / "N1-receipt.json").read_text(encoding="utf-8"))
            self.assertEqual(staged["status"], "blocked")
            self.assertTrue(
                any("four accounting buckets" in reason for reason in staged["reason_codes"]),
                staged["reason_codes"],
            )

    def test_u6_rejects_missing_provider_actual_usage(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            paired = root / "provider-paired" / "post-n0.json"
            paired.parent.mkdir(parents=True)
            artifact = make_envelope(
                "N10-U6",
                started_at="2026-08-10T00:02:00Z",
                finished_at="2026-08-10T00:03:00Z",
                provider_id="provider-a",
                route_id="route-a",
                model="model-a",
                api_revision="api-v1",
                baseline={"request_id": "baseline-request"},
                optimized={"request_id": "optimized-request"},
            )
            paired.write_text(json.dumps(artifact) + "\n", encoding="utf-8")
            proc = run_runner(
                root, make_clean_worktree(), nodes="N10-U6", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            staged = json.loads(
                (root / "N10-U6-provider-paired.json").read_text(encoding="utf-8")
            )
            self.assertEqual(staged["status"], "blocked")
            self.assertTrue(
                any("provider actual usage" in reason for reason in staged["reason_codes"]),
                staged["reason_codes"],
            )

    def test_u9_116_of_117_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            stage_all_pass_artifacts(root)
            # Corrupt one single-entry node artifact so exactly one entry
            # loses evidence (N10-U8 -> optical-renderer, 1 entry).
            (root / "N10-U8-platform.json").write_text(
                json.dumps(make_envelope("N10-U8", status="blocked", reason_codes=["x"]), indent=2) + "\n",
                encoding="utf-8",
            )
            proc = run_runner(
                root, make_clean_worktree(), nodes="N10-U9", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads(
                (root / "N10-U9-coverage.json").read_text(encoding="utf-8")
            )
            self.assertEqual(artifact["status"], "blocked", artifact["reason_codes"])
            self.assertTrue(
                any(re.match(r"u9-coverage-incomplete:\d+/117", reason)
                    for reason in artifact["reason_codes"]),
                artifact["reason_codes"],
            )
            self.assertEqual(artifact["coverage"]["verified"], 116)

    def test_mock_fixture_upstream_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            fake = root / "fixtures" / "mock.json"
            fake.parent.mkdir(parents=True)
            fake.write_text(
                json.dumps(make_envelope("N2", fixture=True)) + "\n", encoding="utf-8"
            )
            proc = run_runner(
                root, make_clean_worktree(), nodes="N2", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads((root / "N2-codex.json").read_text(encoding="utf-8"))
            self.assertEqual(artifact["status"], "blocked")

    def test_pre_n0_stale_upstream_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            stale = root / "cc-switch-aba" / "old.json"
            stale.parent.mkdir(parents=True)
            env = make_envelope("N5", wrote_cc_switch=False, identity_restored=True)
            env["started_at"] = "2026-08-01T00:00:00Z"
            env["artifact_sha256"] = canonical_sha(env)
            stale.write_text(json.dumps(env, indent=2) + "\n", encoding="utf-8")
            proc = run_runner(
                root, make_clean_worktree(), nodes="N5", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads((root / "N5-cc-switch.json").read_text(encoding="utf-8"))
            self.assertEqual(artifact["status"], "blocked")
            self.assertTrue(
                any("pre-N0" in reason for reason in artifact["reason_codes"]),
                artifact["reason_codes"],
            )

    def test_wrong_candidate_upstream_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_n0(root)
            other = root / "opencode-real-routed" / "old-candidate.json"
            other.parent.mkdir(parents=True)
            env = make_envelope("N3")
            env["candidate_commit"] = "f" * 40
            env["artifact_sha256"] = canonical_sha(env)
            other.write_text(json.dumps(env, indent=2) + "\n", encoding="utf-8")
            proc = run_runner(
                root, make_clean_worktree(), nodes="N3", extra=["--skip-local"]
            )
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            artifact = json.loads((root / "N3-opencode.json").read_text(encoding="utf-8"))
            self.assertEqual(artifact["status"], "blocked")


if __name__ == "__main__":
    unittest.main()
