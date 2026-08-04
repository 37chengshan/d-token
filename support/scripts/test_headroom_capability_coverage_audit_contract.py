#!/usr/bin/env python3
"""Static and fixture contract checks for the PowerShell 5.1 coverage audit.

This host does not need PowerShell. The checks pin the JSON contract and ensure
the audit cannot regress to source-text or fixture-step evidence.
"""

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "support" / "scripts" / "run_headroom_capability_coverage_audit.ps1"
GENERATOR = ROOT / "support" / "scripts" / "generate_headroom_capability_snapshot.py"
REGISTRY_RUNNER = ROOT / "support" / "scripts" / "run_candidate_capability_registry_snapshot.ps1"
RELEASE_VERIFICATION = ROOT / "support" / "scripts" / "run_release_verification.ps1"
REVISION = "a" * 64
ACTIVE_REQUIRED_IDS = {
    "compression",
    "ccr",
    "memory_learn",
    "output_shape",
    "integrated_rtk",
    "proxy_sdk_mcp",
    "stats_history_metrics",
    "routing_protocols",
    "doctor_dashboard_logs_export",
    "project_policy",
    "backup_rollback",
}
OPTICAL_ID = "optical_active_transform"
ALLOWED_STATES = {"ready", "degraded", "unavailable", "blocked", "implemented_disabled"}
ALLOWED_FRESHNESS = {"live", "recent", "stale", "unknown"}


def make_snapshot() -> dict[str, object]:
    capabilities = [
        {
            "id": capability_id,
            "state": "ready",
            "implementation_revision": f"impl-{capability_id}",
            "source_evidence_revision": f"native-proof-{capability_id}",
            "evidence_revision": REVISION,
            "freshness": "recent",
            "reason_codes": ["native_runtime_evidence"],
        }
        for capability_id in sorted(ACTIVE_REQUIRED_IDS)
    ]
    capabilities.append(
        {
            "id": OPTICAL_ID,
            "state": "implemented_disabled",
            "implementation_revision": "impl-optical-active",
            "source_evidence_revision": "native-proof-optical-active",
            "evidence_revision": REVISION,
            "freshness": "recent",
            "reason_codes": ["policy_shadow_only"],
        }
    )
    return {
        "schema_version": 1,
        "source": "dtoken-owned-runtime",
        "revision": REVISION,
        "source_registry_revision": "headroom-backend-capabilities-v2",
        "collected_at": "2026-07-30T12:00:00Z",
        "capabilities": capabilities,
    }


def evaluate_fixture(snapshot: dict[str, object], candidate_revision: str = REVISION) -> tuple[bool, int, bool]:
    assert snapshot.get("source") == "dtoken-owned-runtime"
    assert snapshot.get("revision") == candidate_revision
    assert re.fullmatch(r"[0-9a-fA-F]{64}", str(snapshot.get("revision", "")))
    assert re.search(r"(Z|[+-][0-9]{2}:[0-9]{2})$", str(snapshot.get("collected_at", "")))

    capability_list = snapshot.get("capabilities")
    assert isinstance(capability_list, list)
    by_id = {item["id"]: item for item in capability_list if isinstance(item, dict) and "id" in item}
    assert len(by_id) == len(capability_list)

    active_complete = 0
    for capability_id in ACTIVE_REQUIRED_IDS:
        capability = by_id[capability_id]
        assert capability.get("state") in ALLOWED_STATES
        assert re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._:/+-]{0,127}", capability["implementation_revision"])
        assert re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._:/+-]{0,127}", capability["source_evidence_revision"])
        assert capability.get("evidence_revision") == candidate_revision
        assert capability.get("freshness") in ALLOWED_FRESHNESS
        assert isinstance(capability.get("reason_codes"), list)
        if capability["state"] in {"ready", "degraded"} and capability["freshness"] != "unknown":
            active_complete += 1

    optical = by_id[OPTICAL_ID]
    optical_compliant = (
        optical.get("state") == "implemented_disabled"
        and optical.get("evidence_revision") == candidate_revision
        and optical.get("freshness") in {"live", "recent", "stale"}
        and bool({"policy_shadow_only", "policy_disabled"} & set(optical.get("reason_codes", [])))
    )
    return active_complete == len(ACTIVE_REQUIRED_IDS) and optical_compliant, active_complete, optical_compliant


class HeadroomCapabilityCoverageContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.script = SCRIPT.read_text(encoding="utf-8")
        cls.generator = GENERATOR.read_text(encoding="utf-8")
        cls.registry_runner = REGISTRY_RUNNER.read_text(encoding="utf-8")
        cls.release_verification = RELEASE_VERIFICATION.read_text(encoding="utf-8")

    def test_script_is_snapshot_only_and_candidate_bound(self) -> None:
        for required_text in (
            "[string]$CapabilitySnapshot",
            "[string]$ExpectedCandidateId",
            "candidate.capability_snapshot_path",
            'ExpectedSnapshotSource = "dtoken-owned-runtime"',
            'CandidateBindingSource = "explicit-candidate-id"',
            "snapshot revision does not match the candidate revision",
            "evidence_revision does not match the candidate revision",
            "source_evidence_revision is missing",
            "source_registry_revision is missing or invalid",
            "collected_at must be an ISO-8601 timestamp with a timezone",
        ):
            self.assertIn(required_text, self.script)

        for forbidden_text in (
            'Read-Text "code\\',
            "function HasAny",
            "function StepOk",
            'StepOk "fixture_runtime"',
            "Get-ReleaseCandidateProvenance",
        ):
            self.assertNotIn(forbidden_text, self.script)

    def test_script_pins_fail_closed_active_and_optical_rules(self) -> None:
        for capability_id in ACTIVE_REQUIRED_IDS | {OPTICAL_ID}:
            self.assertIn(f'"{capability_id}"', self.script)
        self.assertIn('@("ready", "degraded") -cnotcontains $State', self.script)
        self.assertIn('$Freshness -ceq "unknown"', self.script)
        self.assertIn('$OpticalEvaluation.state -cne "implemented_disabled"', self.script)
        self.assertIn('counts_toward_headroom_active_complete = $false', self.script)
        self.assertIn('$Ok = ($SnapshotContractValid -and $HeadroomActiveComplete -and $OpticalPolicyCompliant)', self.script)

    def test_release_bus_uses_explicit_native_artifacts_in_order(self) -> None:
        registry_step = self.release_verification.index('"candidate_capability_registry"')
        wrapper_step = self.release_verification.index('"candidate_capability_snapshot"')
        coverage_step = self.release_verification.index('"headroom_capability_coverage"')
        self.assertLess(registry_step, wrapper_step)
        self.assertLess(wrapper_step, coverage_step)
        for required_text in (
            "run_candidate_capability_registry_snapshot.ps1",
            "generate_headroom_capability_snapshot.py",
            "-RegistryPath $CapabilityRegistryPath",
            '"--candidate-id", $Candidate.candidate_id',
            "-CapabilitySnapshot $CapabilitySnapshotPath",
            "-ExpectedCandidateId $Candidate.candidate_id",
            "capability_snapshot_path",
        ):
            self.assertIn(required_text, self.release_verification)

    def test_registry_runner_is_loopback_nonce_bound_and_does_not_rebuild(self) -> None:
        for required_text in (
            '"--host", "127.0.0.1"',
            '"http://127.0.0.1:$Port/capabilities"',
            "DTOKEN_RUNTIME_INSTANCE_NONCE",
            "$Health.runtime_instance_nonce -eq $RuntimeNonce",
            'config.source -ne "explicit_cli_config"',
            "Stop-Process -Id $RuntimeProcess.Id",
        ):
            self.assertIn(required_text, self.registry_runner)
        for forbidden_text in ("cargo build", "Get-CimInstance", "0.0.0.0", "https://"):
            self.assertNotIn(forbidden_text, self.registry_runner)

    def test_ready_fixture_is_complete_without_counting_optical(self) -> None:
        fixture = json.loads(json.dumps(make_snapshot()))
        ok, active_complete, optical_compliant = evaluate_fixture(fixture)
        self.assertTrue(ok)
        self.assertEqual(active_complete, len(ACTIVE_REQUIRED_IDS))
        self.assertTrue(optical_compliant)

    def test_active_required_unavailable_blocked_or_disabled_fails(self) -> None:
        for state in ("unavailable", "blocked", "implemented_disabled"):
            with self.subTest(state=state):
                fixture = make_snapshot()
                capability = next(item for item in fixture["capabilities"] if item["id"] == "compression")
                capability.update(state=state, reason_codes=["runtime_not_ready"])
                ok, active_complete, _ = evaluate_fixture(fixture)
                self.assertFalse(ok)
                self.assertEqual(active_complete, len(ACTIVE_REQUIRED_IDS) - 1)

    def test_unknown_freshness_and_revision_mismatch_fail(self) -> None:
        fixture = make_snapshot()
        capability = next(item for item in fixture["capabilities"] if item["id"] == "ccr")
        capability["freshness"] = "unknown"
        ok, _, _ = evaluate_fixture(fixture)
        self.assertFalse(ok)

        with self.assertRaises(AssertionError):
            evaluate_fixture(make_snapshot(), candidate_revision="b" * 64)

    def test_missing_capability_or_required_fields_fail_closed(self) -> None:
        missing_capability = make_snapshot()
        missing_capability["capabilities"] = [
            item for item in missing_capability["capabilities"] if item["id"] != "memory_learn"
        ]
        with self.assertRaises(KeyError):
            evaluate_fixture(missing_capability)

        invalid_reason_codes = make_snapshot()
        capability = next(item for item in invalid_reason_codes["capabilities"] if item["id"] == "output_shape")
        capability["reason_codes"] = "not-an-array"
        with self.assertRaises(AssertionError):
            evaluate_fixture(invalid_reason_codes)

        mismatched_evidence = make_snapshot()
        capability = next(item for item in mismatched_evidence["capabilities"] if item["id"] == "integrated_rtk")
        capability["evidence_revision"] = "b" * 64
        with self.assertRaises(AssertionError):
            evaluate_fixture(mismatched_evidence)

    def test_optical_shadow_compliance_never_fills_an_active_gap(self) -> None:
        fixture = make_snapshot()
        capability = next(item for item in fixture["capabilities"] if item["id"] == "routing_protocols")
        capability.update(state="blocked", reason_codes=["route_evidence_missing"])
        ok, active_complete, optical_compliant = evaluate_fixture(fixture)
        self.assertFalse(ok)
        self.assertTrue(optical_compliant)
        self.assertEqual(active_complete, len(ACTIVE_REQUIRED_IDS) - 1)


if __name__ == "__main__":
    unittest.main()
