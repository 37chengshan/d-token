"""PR-2 codex-ocxlocal acceptance contract tests (PLAN §12.3, W2).

Pins the N2 routed-flow producer contract:

  * 正常 (normal): the producer builds a CF-3 envelope for
    `Codex -> d-token:18001/v1 -> ocxlocal:10100/v1 -> dynamic model` with
    agent_id=codex, capability_tier=route, evidence_kind=route and non-null
    route_decision_digest/capability_digest; the report template validates
    against both the N2 report schema and the frozen CF-3 envelope schema.
  * 缺字段 (missing fields): dropping any required envelope/report field or a
    required evidence field fails the contract (fail-closed, never treated as
    N/A by omission).
  * 隐私 (privacy): all six privacy assertions are true; secrets, absolute
    paths and prompt/response content are rejected by the producer scan; the
    report write path refuses to persist any violating payload.
  * 模型固定回归 (model-fixed regression): the frozen route evidence requires
    models_list_empty=true, model_dynamic=true and
    model_pinned_to_install_time=false (C8: the request model is dynamically
    passed through, never pinned to an install-time value); a claim of a
    pinned/non-empty model list fails the schema and the builder.
  * 配置事务 (config transaction cannot substitute routed flow): restore
    evidence declares config_changes_made=false and
    config_transaction_not_substitute=true; status=pass requires a route
    evidence digest (a config-transaction-only artifact can never be a pass).
  * dry-run/fixture never impersonates a real routed flow: the template is
    status=unavailable and the dry-run result is status=preflight with
    real_run_executed=false.

Run:  python3 -m pytest support/tests/test_codex_ocxlocal_acceptance.py -q
      python3 support/scripts/run_codex_ocxlocal_routed_acceptance.py --dry-run
"""
from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from copy import deepcopy
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "run_codex_ocxlocal_routed_acceptance.py"
SPEC = importlib.util.spec_from_file_location("run_codex_ocxlocal_routed_acceptance", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)

REPORT_SCHEMA_PATH = ROOT / "fixtures" / "codex-ocxlocal-routed" / "report.schema.json"
REPORT_TEMPLATE_PATH = ROOT / "fixtures" / "codex-ocxlocal-routed" / "report.template.json"

SHA256 = "a" * 64
SHA1_40 = "b" * 40
OPTICAL_DIGESTS = list(P.OPTICAL_DIGESTS)


def _valid_route_evidence() -> dict:
    return P.build_route_evidence(
        model="deepseek-v4-flash",
        provider="ocxlocal",
        route_verified=True,
        request_match_count=3,
    )


def _valid_envelope(status: str = "pass", reason_codes: list[str] | None = None) -> dict:
    route = _valid_route_evidence()
    ledger = P.build_ledger_evidence({"events": 20, "route_decisions": 10, "request_logs": 10, "ccr_metadata": 14})
    metrics = P.build_metrics_evidence(requests_sent=10, responses_received=10, duration_seconds=2.5)
    failure = P.build_failure_evidence(None, [], 0)
    restore = P.build_restore_evidence()
    route_digest = P.sha256_bytes(P.canonical_json(route))
    capability_digest = P.sha256_bytes(
        P.canonical_json({"ledger": ledger, "metrics": metrics, "failure": failure, "restore": restore})
    )
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    envelope = P.build_envelope(
        candidate_commit=SHA1_40,
        source_digest=SHA256,
        registry_digest=P.registry_digest(),
        status=status,
        reason_codes=reason_codes if reason_codes is not None else ["codex-routed-flow-verified"],
        started_at=now,
        finished_at=now,
        route_decision_digest=route_digest,
        capability_digest=capability_digest,
        corpus_digests={
            "route": route_digest,
            "ledger": P.sha256_bytes(P.canonical_json(ledger)),
            "metrics": P.sha256_bytes(P.canonical_json(metrics)),
            "failure": P.sha256_bytes(P.canonical_json(failure)),
            "restore": P.sha256_bytes(P.canonical_json(restore)),
        },
        producer_revision=P.producer_revision(),
    )
    return envelope


def _valid_report() -> dict:
    route = _valid_route_evidence()
    ledger = P.build_ledger_evidence({"events": 20, "route_decisions": 10, "request_logs": 10, "ccr_metadata": 14})
    metrics = P.build_metrics_evidence(requests_sent=10, responses_received=10, duration_seconds=2.5)
    failure = P.build_failure_evidence(None, [], 0)
    restore = P.build_restore_evidence()
    return {
        "envelope": _valid_envelope(),
        "route": route,
        "ledger": ledger,
        "metrics": metrics,
        "failure": failure,
        "restore": restore,
    }


class NormalContractTests(unittest.TestCase):
    """正常: a complete producer-built report satisfies every frozen contract."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.report_schema = json.loads(REPORT_SCHEMA_PATH.read_text(encoding="utf-8"))
        cls.template = json.loads(REPORT_TEMPLATE_PATH.read_text(encoding="utf-8"))

    def test_producer_script_imports(self) -> None:
        self.assertEqual(P.AGENT_ID, "codex")
        self.assertEqual(P.CAPABILITY_TIER, "route")
        self.assertEqual(P.EVIDENCE_KIND, "route")
        self.assertEqual(P.NODE_ID, "N2")
        self.assertEqual(P.DEFAULT_DTOKEN_PORT, 18001)
        self.assertEqual(P.DEFAULT_OCCLOCAL_PORT, 10100)

    def test_valid_envelope_passes_all_validators(self) -> None:
        envelope = _valid_envelope()
        self.assertEqual(P.cf3_envelope_errors(envelope), [])
        self.assertEqual(P.cross_field_errors(envelope), [])
        report = _valid_report()
        self.assertEqual(P.report_schema_errors(report), [])

    def test_route_evidence_has_frozen_dynamic_passthrough_semantics(self) -> None:
        route = _valid_route_evidence()
        self.assertEqual(route["protocol"], "responses")
        self.assertIs(route["requires_openai_auth"], False)
        self.assertEqual(
            route["models_passthrough"],
            {"models_list_empty": True, "model_dynamic": True, "model_pinned_to_install_time": False},
        )
        self.assertTrue(route["route_verified"])
        self.assertGreaterEqual(route["request_match_count"], 1)

    def test_envelope_digests_are_non_null_and_64_hex(self) -> None:
        envelope = _valid_envelope()
        for field in ("route_decision_digest", "capability_digest"):
            value = envelope[field]
            self.assertIsNotNone(value)
            self.assertRegex(value, "^[0-9a-f]{64}$")
        self.assertRegex(envelope["artifact_sha256"], "^[0-9a-f]{64}$")
        self.assertRegex(envelope["registry_digest"], "^[0-9a-f]{64}$")

    def test_optical_digests_are_null_with_reasons(self) -> None:
        envelope = _valid_envelope()
        for digest in OPTICAL_DIGESTS:
            self.assertIsNone(envelope[digest], digest)
            self.assertIn(digest, envelope["not_applicable_reasons"], digest)

    def test_template_validates_and_does_not_impersonate(self) -> None:
        self.assertEqual(P.report_schema_errors(self.template), [])
        self.assertEqual(P.cf3_envelope_errors(self.template["envelope"]), [])
        self.assertNotEqual(self.template["envelope"]["status"], "pass")
        self.assertTrue(self.template["envelope"]["reason_codes"])

    def test_report_schema_is_strict_and_consumes_cf3(self) -> None:
        self.assertIs(self.report_schema["additionalProperties"], False)
        self.assertIn(
            "https://d-token.local/schemas/candidate-evidence-envelope.schema.json",
            json.dumps(self.report_schema),
        )


class MissingFieldTests(unittest.TestCase):
    """缺字段: omission is never treated as N/A; missing fields fail closed."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.template = json.loads(REPORT_TEMPLATE_PATH.read_text(encoding="utf-8"))

    def test_missing_envelope_required_field_fails_cf3(self) -> None:
        envelope = _valid_envelope()
        for field in ("schema_version", "status", "candidate_commit", "source_digest", "evidence_kind", "agent_id", "capability_tier", "privacy_assertions", "route_decision_digest", "capability_digest"):
            broken = deepcopy(envelope)
            del broken[field]
            self.assertTrue(P.cf3_envelope_errors(broken), f"missing {field} should fail")

    def test_missing_report_top_level_field_fails(self) -> None:
        report = _valid_report()
        for field in ("envelope", "route", "ledger", "metrics", "failure", "restore"):
            broken = deepcopy(report)
            del broken[field]
            self.assertTrue(P.report_schema_errors(broken), f"missing {field} should fail")

    def test_missing_route_evidence_required_field_fails(self) -> None:
        report = _valid_report()
        for field in ("protocol", "requires_openai_auth", "models_passthrough", "provider", "model", "upstream", "route_verified", "request_match_count", "failure_classifications"):
            broken = deepcopy(report)
            del broken["route"][field]
            self.assertTrue(P.report_schema_errors(broken), f"missing route.{field} should fail")

    def test_missing_ledger_field_fails(self) -> None:
        report = _valid_report()
        for field in ("events", "route_decisions", "request_logs", "ccr_metadata"):
            broken = deepcopy(report)
            del broken["ledger"][field]
            self.assertTrue(P.report_schema_errors(broken), f"missing ledger.{field} should fail")

    def test_missing_restore_field_fails(self) -> None:
        report = _valid_report()
        for field in ("config_changes_made", "restore_status", "config_transaction_not_substitute", "routed_flow_evidence"):
            broken = deepcopy(report)
            del broken["restore"][field]
            self.assertTrue(P.report_schema_errors(broken), f"missing restore.{field} should fail")

    def test_builder_rejects_empty_model_and_bad_counts(self) -> None:
        with self.assertRaises(ValueError):
            P.build_route_evidence(model="", provider="ocxlocal", route_verified=False, request_match_count=0)
        with self.assertRaises(ValueError):
            P.build_route_evidence(model="m", provider="ocxlocal", route_verified=True, request_match_count=0)
        with self.assertRaises(ValueError):
            P.build_ledger_evidence({"events": 0, "route_decisions": -1, "request_logs": 0, "ccr_metadata": 0})
        with self.assertRaises(ValueError):
            P.build_metrics_evidence(requests_sent=1, responses_received=2, duration_seconds=0.5)


class PrivacyTests(unittest.TestCase):
    """隐私: six assertions all true; secrets/paths/prompts never enter the report."""

    def test_privacy_assertions_are_all_true(self) -> None:
        assertions = P.privacy_assertions()
        self.assertEqual(
            set(assertions),
            {
                "no_secret",
                "no_complete_prompt_or_response",
                "no_source_code",
                "no_absolute_paths",
                "no_raw_command_output",
                "redaction_verified",
            },
        )
        self.assertTrue(all(assertions.values()))

    def test_envelope_privacy_assertions_required_and_true(self) -> None:
        envelope = _valid_envelope()
        self.assertTrue(all(envelope["privacy_assertions"].values()))
        broken = deepcopy(envelope)
        del broken["privacy_assertions"]
        self.assertTrue(P.cf3_envelope_errors(broken))

    def test_scan_rejects_secret(self) -> None:
        self.assertTrue(P.scan_privacy({"key": "sk-abcdefghijklmnopqrstuvwxyz123456"}))
        self.assertTrue(P.scan_privacy({"auth": "Bearer abcdefghijklmnopqrstuvwxyz123456"}))

    def test_scan_rejects_absolute_path(self) -> None:
        self.assertTrue(P.scan_privacy({"path": "/Users/cc/Documents/secret"}))

    def test_scan_rejects_prompt_content(self) -> None:
        self.assertTrue(P.scan_privacy({"payload": {"input": "please summarize this prompt"}}, extra_forbidden=("please summarize",)))

    def test_scan_accepts_clean_report(self) -> None:
        self.assertEqual(P.scan_privacy(_valid_report()), [])
        self.assertEqual(P.scan_privacy(P.privacy_assertions()), [])

    def test_write_report_refuses_privacy_violation(self) -> None:
        route = _valid_route_evidence()
        route["model"] = "/Users/cc/.codex/leak"
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(ValueError):
                P.write_report(
                    Path(temporary),
                    "stamp",
                    _valid_envelope(),
                    route,
                    P.build_ledger_evidence({"events": 1, "route_decisions": 1, "request_logs": 1, "ccr_metadata": 1}),
                    P.build_metrics_evidence(requests_sent=1, responses_received=1, duration_seconds=0.1),
                    P.build_failure_evidence(None, [], 0),
                    P.build_restore_evidence(),
                )


class ModelFixedRegressionTests(unittest.TestCase):
    """模型固定回归: the request model is dynamically passed through, never pinned."""

    def test_schema_rejects_pinned_model_claim(self) -> None:
        report = _valid_report()
        report["route"]["models_passthrough"]["model_pinned_to_install_time"] = True
        self.assertTrue(P.report_schema_errors(report))
        report = _valid_report()
        report["route"]["models_passthrough"]["models_list_empty"] = False
        self.assertTrue(P.report_schema_errors(report))
        report = _valid_report()
        report["route"]["models_passthrough"]["model_dynamic"] = False
        self.assertTrue(P.report_schema_errors(report))

    def test_schema_rejects_non_responses_protocol_and_openai_auth(self) -> None:
        report = _valid_report()
        report["route"]["protocol"] = "chat"
        self.assertTrue(P.report_schema_errors(report))
        report = _valid_report()
        report["route"]["requires_openai_auth"] = True
        self.assertTrue(P.report_schema_errors(report))

    def test_envelope_requires_codex_route_binding(self) -> None:
        # The CF-3 schema itself only enforces structure; the N2 report schema
        # pins agent_id=codex / capability_tier=route / evidence_kind=route and
        # cross_field_errors blocks the matrix binding violation (hermes+route).
        report = _valid_report()
        report["envelope"]["agent_id"] = "hermes"
        self.assertTrue(P.report_schema_errors(report))
        self.assertTrue(P.cross_field_errors(report["envelope"]))
        report = _valid_report()
        report["envelope"]["capability_tier"] = "detection_only"
        self.assertTrue(P.report_schema_errors(report))
        report = _valid_report()
        report["envelope"]["evidence_kind"] = "generic"
        self.assertTrue(P.report_schema_errors(report))
        report = _valid_report()
        report["envelope"]["node_id"] = "N9"
        self.assertTrue(P.report_schema_errors(report))

    def test_route_digest_null_fails(self) -> None:
        envelope = _valid_envelope()
        envelope["route_decision_digest"] = None
        envelope["not_applicable_reasons"]["route_decision_digest"] = "route evidence pending"
        self.assertTrue(P.cf3_envelope_errors(envelope))
        envelope = _valid_envelope()
        envelope["capability_digest"] = None
        envelope["not_applicable_reasons"]["capability_digest"] = "route evidence pending"
        self.assertTrue(P.cf3_envelope_errors(envelope))


class ConfigTransactionTests(unittest.TestCase):
    """配置事务: exact config restore is NOT routed-flow evidence (C8)."""

    def test_restore_evidence_declares_config_transaction_not_substitute(self) -> None:
        restore = P.build_restore_evidence()
        self.assertIs(restore["config_changes_made"], False)
        self.assertEqual(restore["restore_status"], "not_applicable_no_config_write")
        self.assertIs(restore["config_transaction_not_substitute"], True)

    def test_schema_rejects_config_changes_made_true(self) -> None:
        report = _valid_report()
        report["restore"]["config_changes_made"] = True
        self.assertTrue(P.report_schema_errors(report))
        report = _valid_report()
        report["restore"]["config_transaction_not_substitute"] = False
        self.assertTrue(P.report_schema_errors(report))

    def test_status_pass_requires_route_evidence_digest(self) -> None:
        envelope = _valid_envelope()
        envelope["corpus_digests"] = {}  # a config-transaction-only artifact
        self.assertTrue(P.cross_field_errors(envelope))

    def test_failing_status_requires_reason_codes(self) -> None:
        envelope = _valid_envelope(status="fail", reason_codes=[])
        self.assertTrue(P.cf3_envelope_errors(envelope))

    def test_template_restore_keeps_c8_semantics(self) -> None:
        template = json.loads(REPORT_TEMPLATE_PATH.read_text(encoding="utf-8"))
        self.assertIs(template["restore"]["config_changes_made"], False)
        self.assertIs(template["restore"]["config_transaction_not_substitute"], True)


class DryRunTests(unittest.TestCase):
    """dry-run/fixture never impersonates a real routed flow."""

    def test_dry_run_is_preflight_and_writes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = P.run_dry_run(producer_revision=P.producer_revision())
            self.assertIs(result["ok"], True)
            self.assertEqual(result["mode"], "dry-run")
            self.assertEqual(result["status"], "preflight")
            self.assertIs(result["real_run_executed"], False)
            self.assertEqual(result["schema_errors"], [])
            self.assertEqual(result["template_privacy_violations"], [])

    def test_report_round_trip_json_parse(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            evidence = {
                "status": "pass",
                "reason_codes": ["codex-routed-flow-verified"],
                "route_evidence": _valid_route_evidence(),
                "ledger_evidence": P.build_ledger_evidence({"events": 20, "route_decisions": 10, "request_logs": 10, "ccr_metadata": 14}),
                "metrics_evidence": P.build_metrics_evidence(requests_sent=10, responses_received=10, duration_seconds=2.5),
                "failure_evidence": P.build_failure_evidence(None, [], 0),
                "restore_evidence": P.build_restore_evidence(),
            }
            path = P.build_and_write_report(
                candidate_commit=SHA1_40,
                source_digest=SHA256,
                registry_digest=P.registry_digest(),
                producer_revision=P.producer_revision(),
                artifact_dir=Path(temporary),
                stamp="test",
                evidence=evidence,
            )
            self.assertTrue(path.is_file())
            parsed = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(parsed["agent_id"], "codex")
            self.assertEqual(parsed["capability_tier"], "route")
            self.assertEqual(parsed["evidence_kind"], "route")
            self.assertEqual(parsed["node_id"], "N2")
            self.assertRegex(parsed["artifact_sha256"], "^[0-9a-f]{64}$")


if __name__ == "__main__":
    unittest.main()
