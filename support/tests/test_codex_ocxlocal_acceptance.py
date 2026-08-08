"""
Behavior tests for the Codex OCX-local routed acceptance evidence validator
(support/scripts/run_codex_ocxlocal_routed_acceptance.py).

These tests import and invoke the runner (main(), build_report(), schema
validator) with injected fake d-token evidence payloads that MIMIC THE REAL
Rust /export/json schema (request_logs, route_decisions, events, stats,
active_config.providers/routes) and the Prometheus text /metrics endpoint.
They do not bind any listener and do not contact the network.

Coverage: --check-schema positive and negative, real-shape chain correlation
via request_id (request_log -> route_decision -> event), agent normalization
to codex, provider metadata (127.0.0.1:10100, responses protocol,
credential_configured=false), dynamic two-model passthrough, metrics text
parser, old fake shape rejection, request_id correlation mismatch blocking,
zero-ledger blocking, fail-closed behavior on missing evidence, expected
model validation-set semantics (same-window non-expected Codex concurrency
ignored, missing expected model blocked, freshness ids/count restricted to
correlated expected models), metrics request_count as the fresh delta (never
a post absolute count), candidate commit binding (read-only git HEAD;
schema-only mode records unresolved; real success fails closed with
candidate_unbound when unresolvable), and fixture loading from
support/fixtures/codex-ocxlocal-routed/.
"""

from __future__ import annotations

import importlib.util
import json
import os
import re
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "run_codex_ocxlocal_routed_acceptance.py"

REPORT_KEYS = (
    "schema_version", "mode", "status", "ok", "schema_valid", "routed", "platform",
    "agent_id", "protocol", "requires_openai_auth", "route", "runtime", "ledger",
    "metrics", "freshness", "failure", "restore", "candidate", "redacted",
)

# --- Real Rust schema shapes (dtoken-types: RequestLog, RouteDecision,      ---
# DtokenEvent; dtoken-core-api: provider_metadata_payload, /export/json,      ---
# /metrics Prometheus text). Fixtures below mirror these exactly.             ---

PROVIDER_ID = "provider-ocxlocal"
ROUTE_ID = "route-ocxlocal"
PROVIDER_BASE_URL = "http://127.0.0.1:10100/v1"
METRICS_TEXT = (
    "# d-token runtime metrics\n"
    'dtoken_runtime_info{source="dtoken-owned-runtime",runtime="headroom-derived",external_headroom_dependency="false"} 1\n'
    "dtoken_requests_total 2\n"
    "dtoken_requests_failed_total 0\n"
    "dtoken_input_tokens_before_total 1000\n"
    "dtoken_input_tokens_after_total 800\n"
)


def load_runner() -> "module":
    spec = importlib.util.spec_from_file_location("codex_ocxlocal_acceptance_runner", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def args_for(module: "module", *extra: str) -> "SimpleNamespace":
    return module.build_parser().parse_args(list(extra))


def request_log(request_id: str, model: str, *, agent: str = "codex") -> dict:
    """Real RequestLog shape (dtoken-types)."""
    return {
        "request_id": request_id,
        "timestamp_ms": 1_700_000_000_000,
        "agent": agent,
        "instance_id": "ocxlocal-instance",
        "input_protocol": "open_ai",
        "provider_id": PROVIDER_ID,
        "route_id": ROUTE_ID,
        "chain_mode": "explicit",
        "model": model,
        "input_tokens_before": 500,
        "input_tokens_after": 400,
        "output_tokens": 200,
        "tokens_saved": 100,
        "transforms_applied": [],
        "latency_ms": 120,
        "error": None,
        "config_source": "active-config.json",
    }


def route_decision(request_id: str) -> dict:
    """Real RouteDecision shape (dtoken-types): NO agent_id, NO model."""
    return {
        "request_id": request_id,
        "route_id": ROUTE_ID,
        "provider_id": PROVIDER_ID,
        "chain_mode": "explicit",
        "compression_policy": "off",
        "reason": "routed",
        "input_protocol": "open_ai",
        "output_protocol": "open_ai",
    }


def ledger_event(request_id: str, *, agent: str = "codex") -> dict:
    """Real DtokenEvent shape (dtoken-types)."""
    return {
        "event_id": f"event-{request_id}",
        "timestamp_ms": 1_700_000_000_000,
        "project_id": "dtoken-runtime",
        "agent_id": agent,
        "instance_id": "dtoken-runtime-local",
        "command_id": None,
        "request_id": request_id,
        "stage": "routed",
        "source": "dtoken",
        "status": "ok",
        "input_tokens": 500,
        "output_tokens": 200,
        "saved_tokens": 100,
        "baseline_kind": None,
        "error_code": None,
        "metadata": {},
    }


def real_export(
    module: "module",
    *,
    models: tuple[str, ...] = ("model-a", "model-b"),
    agents: tuple[str, ...] | None = None,
    events: bool = True,
    event_agent: str = "codex",
    provider: bool = True,
    provider_base: str = PROVIDER_BASE_URL,
    provider_protocol: str = "open_ai",
    provider_credential: bool = False,
    provider_models: tuple[str, ...] = ("model-a", "model-b"),
    route_meta: bool = True,
    request_ids: tuple[str, ...] | None = None,
    decision_for: tuple[str | None, ...] | None = None,
    stats_total: int | None = 2,
) -> dict:
    """Real /export/json payload shape (dtoken-core-api runtime_export_json_payload).

    agents=None -> all request logs are agent=codex (one per model);
    decision_for=None -> every request gets its own matching decision;
    request_ids=None -> generated per model (req-1..N). Every per-model
    collection must cover len(models); a short one raises ValueError
    instead of IndexError deep inside a comprehension.
    """
    count = len(models)
    if agents is None:
        agents = ("codex",) * count
    if request_ids is None:
        request_ids = tuple(f"req-{index + 1}" for index in range(count))
    if decision_for is None:
        decision_for = (None,) * count
    for label, values in (("agents", agents), ("request_ids", request_ids), ("decision_for", decision_for)):
        if not isinstance(values, (tuple, list)):
            raise ValueError(f"{label} must be a tuple or list covering every model (got {type(values).__name__})")
        if len(values) < count:
            raise ValueError(f"{label} must cover every model (need {count}, got {len(values)})")
    requests = [
        request_log(request_ids[index], model, agent=agents[index])
        for index, model in enumerate(models)
    ]
    decisions = [
        route_decision(decision_for[index] if decision_for[index] else request_ids[index])
        for index in range(count)
    ]
    events_list = (
        [ledger_event(request_ids[index], agent=event_agent) for index in range(count)]
        if events
        else []
    )
    active_config = {
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": False,
        "config": {"status": "active", "source": "active-config.json"},
        "providers": (
            [
                {
                    "id": PROVIDER_ID,
                    "name": "OCX-local provider",
                    "kind": "custom",
                    "protocol": provider_protocol,
                    "base_url": provider_base,
                    "models": list(provider_models),
                    "timeout_ms": 30_000,
                    "healthcheck_path": "/health",
                    "credential_configured": provider_credential,
                }
            ]
            if provider
            else []
        ),
        "routes": (
            [
                {
                    "id": ROUTE_ID,
                    "name": "OCX-local route",
                    "input_protocol": "open_ai",
                    "target_provider_id": PROVIDER_ID,
                }
            ]
            if route_meta
            else []
        ),
        "instances": [],
        "optical": {"schema_version": 1, "global_policy": "passthrough", "route_policies": []},
        "privacy": "Runtime active config metadata only.",
    }
    stats = {
        "source": "dtoken-owned-runtime",
        "status": "active_runtime",
        "requests": {"total": stats_total, "failed": 0},
        "tokens_saved": 200,
    }
    return {
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "redacted": True,
        "raw_available": False,
        "privacy": "No raw prompt, raw response, API key, auth header, cookie, CCR body, or full command output is included.",
        "stats": stats,
        "history": {"source": "dtoken-owned-runtime", "history": []},
        "metrics": {"source": "dtoken-owned-runtime", "total_requests": len(models)},
        "events": events_list,
        "route_decisions": decisions,
        "request_logs": requests,
        "active_config": active_config,
        "ccr": {"source": "dtoken-owned-runtime", "status": "no_cache_entries"},
        "memory": {"source": "dtoken-owned-runtime"},
    }


def real_evidence(
    module: "module",
    *,
    health: bool = True,
    metrics_text: str | None = METRICS_TEXT,
    **export_kwargs: object,
) -> dict:
    """Synthetic d-token runtime evidence: /health + /export/json + /metrics text."""
    evidence: dict = {}
    evidence["health"] = (
        {"status": "healthy", "component": "dtoken-core-api", "version": "test", "runtime_identity": "dtoken-runtime-local"}
        if health
        else None
    )
    evidence["export"] = real_export(module, **export_kwargs) if health else None
    evidence["metrics_text"] = metrics_text
    return evidence


def old_fake_evidence(module: "module") -> dict:
    """The PREVIOUS fake shape that must NOT pass real-mode correlation:
    decisions carry agent_id/model, routes carry base_url/wire_api, metrics is
    a JSON dict, and the ledger is a bare {"events": [...]} wrapper."""
    return {
        "health": {"status": "healthy", "component": "dtoken-core-api", "version": "test", "runtime_identity": "dtoken-runtime-local"},
        "export": {
            "source": "not-dtoken-owned-runtime",
            "events": [],
            "route_decisions": [
                {"agent_id": "codex", "provider_id": "ocxlocal", "model": "model-a", "status": "ok", "request_id": "r0"}
            ],
            "request_logs": [],
            "active_config": {},
        },
        "metrics_text": None,
    }


class CodexOcxlocalSchemaCheckTests(unittest.TestCase):
    """--check-schema / --dry-run behavior: schema-only, never routed."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.module = load_runner()

    def _run_check_schema(self, *extra: str) -> tuple[int, dict]:
        module = self.module
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            out_path = handle.name
        try:
            exit_code = module.main(["--check-schema", "--report-out", out_path, *extra])
            report = json.loads(Path(out_path).read_text(encoding="utf-8"))
        finally:
            Path(out_path).unlink(missing_ok=True)
        return exit_code, report

    def test_check_schema_positive_exit_zero_schema_only(self) -> None:
        exit_code, report = self._run_check_schema("--upstream-no-auth")
        self.assertEqual(exit_code, 0)
        self.assertTrue(report["schema_valid"])
        # Schema-only proof must never masquerade as a routed acceptance.
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["status"], "schema-validated-only")

    def test_check_schema_positive_contract_fields(self) -> None:
        _, report = self._run_check_schema()
        self.assertEqual(report["agent_id"], "codex")
        self.assertEqual(report["protocol"], "responses")
        self.assertIs(report["requires_openai_auth"], False)
        self.assertEqual(report["mode"], "codex-ocxlocal-routed-acceptance")
        for key in REPORT_KEYS:
            self.assertIn(key, report)
        # Schema-only proof never binds a candidate commit: no git read, no
        # executed claim, so candidate stays unresolved.
        self.assertEqual(report["candidate"], {"commit": "", "resolved": False})

    def test_check_schema_negative_const_mismatch(self) -> None:
        module = self.module
        _, report = self._run_check_schema()
        # A report claiming a different agent must be rejected by the schema.
        bad = dict(report)
        bad["agent_id"] = "opencode"
        schema = module.load_schema()
        self.assertIsNotNone(schema)
        errors = module.validate_report_against_schema(bad, schema)  # type: ignore[arg-type]
        self.assertTrue(errors, "schema must reject agent_id != codex")

    def test_check_schema_negative_missing_required(self) -> None:
        module = self.module
        _, report = self._run_check_schema()
        bad = dict(report)
        del bad["redacted"]
        schema = module.load_schema()
        self.assertIsNotNone(schema)
        errors = module.validate_report_against_schema(bad, schema)  # type: ignore[arg-type]
        self.assertTrue(any("missing required" in error for error in errors))

    def test_check_schema_number_type_enforced(self) -> None:
        # fresh_window_seconds is the schema's only "number" node: strings,
        # bools, NaN/inf and negatives must all be rejected; finite floats pass.
        module = self.module
        _, report = self._run_check_schema()
        schema = module.load_schema()
        self.assertIsNotNone(schema)
        for bad_value in ("0.5", True, float("nan"), float("inf"), -1.0):
            bad = dict(report)
            bad["freshness"] = dict(report["freshness"])
            bad["freshness"]["fresh_window_seconds"] = bad_value
            errors = module.validate_report_against_schema(bad, schema)  # type: ignore[arg-type]
            self.assertTrue(errors, f"schema must reject fresh_window_seconds={bad_value!r}")
        ok = dict(report)
        ok["freshness"] = dict(report["freshness"])
        ok["freshness"]["fresh_window_seconds"] = 12.5
        self.assertEqual(module.validate_report_against_schema(ok, schema), [])  # type: ignore[arg-type]

    def test_check_schema_integer_type_rejects_bool(self) -> None:
        # Integer nodes (schema_version, request_count, ports, counts) must
        # reject bools: Python bool subclasses int, so the validator checks
        # explicitly and the tests must pin that behavior.
        module = self.module
        _, report = self._run_check_schema()
        schema = module.load_schema()
        self.assertIsNotNone(schema)
        for key in ("schema_version",):
            bad = dict(report)
            bad[key] = True
            errors = module.validate_report_against_schema(bad, schema)  # type: ignore[arg-type]
            self.assertTrue(errors, f"schema must reject bool for {key!r}")
        bad = dict(report)
        bad["metrics"] = dict(report["metrics"])
        bad["metrics"]["request_count"] = True
        errors = module.validate_report_against_schema(bad, schema)  # type: ignore[arg-type]
        self.assertTrue(errors, "schema must reject bool for metrics.request_count")

    def test_invalid_upstream_base_url_port_is_usage_error(self) -> None:
        # A non-numeric/out-of-range port in --upstream-base-url must be a
        # clean UsageError (exit 2), never a raw traceback.
        module = self.module
        for bad_url in ("http://127.0.0.1:abc/v1", "http://127.0.0.1:99999/v1"):
            exit_code = module.main(["--check-schema", "--upstream-base-url", bad_url])
            self.assertEqual(exit_code, 2, bad_url)

    def test_upstream_port_must_match_base_url(self) -> None:
        # Declaring --upstream-port that contradicts --upstream-base-url is a
        # usage error caught before any evidence fetch.
        module = self.module
        exit_code = module.main(
            [
                "--upstream-no-auth",
                "--upstream-base-url", "http://127.0.0.1:20200/v1",
                "--upstream-port", "10100",
            ]
        )
        self.assertEqual(exit_code, 2)

    def test_check_schema_negative_broken_artifact_fails_closed(self) -> None:
        module = self.module
        original = module.REPORT_SCHEMA_FILE
        try:
            with tempfile.TemporaryDirectory() as tmp:
                bad_schema = Path(tmp) / "report.schema.json"
                bad_schema.write_text('{"type": "object", "required": ["nope", "missing"], "additionalProperties": false}', encoding="utf-8")
                module.REPORT_SCHEMA_FILE = bad_schema
                exit_code, report = self._run_check_schema()
        finally:
            module.REPORT_SCHEMA_FILE = original
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["schema_valid"])
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "schema_invalid")

    def test_check_schema_missing_artifact_fails_closed(self) -> None:
        module = self.module
        original = module.REPORT_SCHEMA_FILE
        try:
            with tempfile.TemporaryDirectory() as tmp:
                module.REPORT_SCHEMA_FILE = Path(tmp) / "does-not-exist.schema.json"
                exit_code, report = self._run_check_schema()
        finally:
            module.REPORT_SCHEMA_FILE = original
        self.assertEqual(exit_code, 1)
        self.assertEqual(report["failure"]["class"], "schema_artifact_missing")
        # The detail must be a static message, never the absolute fixture path.
        self.assertNotIn("does-not-exist", report["failure"]["detail_redacted"])
        self.assertNotIn(str(Path(tmp)), report["failure"]["detail_redacted"])

    def test_dry_run_never_claims_routed(self) -> None:
        module = self.module
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            out_path = handle.name
        try:
            exit_code = module.main(["--dry-run", "--report-out", out_path])
            report = json.loads(Path(out_path).read_text(encoding="utf-8"))
        finally:
            Path(out_path).unlink(missing_ok=True)
        self.assertEqual(exit_code, 0)
        self.assertTrue(report["schema_valid"])
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])


class CodexOcxlocalEvidenceTests(unittest.TestCase):
    """Real-mode evidence correlation against the real Rust /export/json shape."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.module = load_runner()

    def _build(self, evidence: dict, *extra: str) -> dict:
        # Unit-level reports record the fresh metrics delta; callers pass a
        # valid >= 2 delta unless a test deliberately probes delta handling.
        return self.module.build_report(args_for(self.module, *extra), evidence, metrics_delta=2)

    def test_full_chain_verified_with_identity_and_counts(self) -> None:
        report = self._build(real_evidence(self.module), "--upstream-no-auth")
        self.assertTrue(report["ok"])
        self.assertTrue(report["routed"])
        self.assertEqual(report["status"], "routed-verified")
        self.assertEqual(report["runtime"]["runtime_identity"], "dtoken-runtime-local")
        self.assertTrue(report["runtime"]["component_verified"])
        self.assertEqual(report["ledger"]["request_match_count"], 2)
        self.assertEqual(report["ledger"]["events_observed"], 2)
        self.assertEqual(report["metrics"]["request_count"], 2)
        self.assertTrue(report["route"]["route_verified"])
        self.assertEqual(report["restore"]["status"], "no_changes_made")
        # Upstream evidence comes from provider metadata base_url (redacted).
        self.assertEqual(report["route"]["upstream"]["host"], "127.0.0.1")
        self.assertEqual(report["route"]["upstream"]["port"], 10100)

    def test_dynamic_two_model_passthrough(self) -> None:
        report = self._build(
            real_evidence(self.module, models=("model-a", "model-b")),
            "--upstream-no-auth",
        )
        # ocxlocal owns dynamic model switching: d-token must forward both
        # actual request_log models without pinning a single one.
        self.assertTrue(report["route"]["dynamic_model"])
        self.assertEqual(report["route"]["model_forwarded"], ["model-a", "model-b"])
        self.assertEqual(report["ledger"]["request_match_count"], 2)

    def test_single_model_passthrough_not_dynamic(self) -> None:
        # dynamic_model requires >= 2 distinct models; a single forwarded model
        # must never be mislabeled as dynamic switching.
        report = self._build(real_evidence(self.module, models=("model-a",)), "--upstream-no-auth")
        self.assertFalse(report["route"]["dynamic_model"])
        self.assertEqual(report["route"]["model_forwarded"], ["model-a"])

    def test_real_export_helper_arbitrary_model_count(self) -> None:
        # Helper robustness: models longer than the default request_ids /
        # agents / decision_for tuples must not IndexError, and a short
        # per-model collection must raise a clear ValueError.
        module = self.module
        with self.assertRaises(ValueError):
            real_export(module, models=("m1", "m2", "m3"), request_ids=("r1", "r2"))
        # A bare str/bytes is iterable but must never be treated as a
        # per-model collection (per-character indexing would silently build
        # garbage evidence).
        with self.assertRaises(ValueError):
            real_export(module, models=("m1", "m2"), agents="codex")
        with self.assertRaises(ValueError):
            real_export(module, models=("m1", "m2"), request_ids=b"r1r2")
        export = real_export(module, models=("m1", "m2", "m3"))
        self.assertEqual([log["model"] for log in export["request_logs"]], ["m1", "m2", "m3"])
        self.assertEqual([log["request_id"] for log in export["request_logs"]], ["req-1", "req-2", "req-3"])
        self.assertEqual(len(export["route_decisions"]), 3)
        self.assertEqual(len(export["events"]), 3)
        report = self._build(real_evidence(module, models=("m1", "m2", "m3"), stats_total=3), "--upstream-no-auth")
        self.assertTrue(report["ok"])
        self.assertEqual(report["route"]["model_forwarded"], ["m1", "m2", "m3"])
        self.assertEqual(report["ledger"]["request_match_count"], 3)

    def test_agent_case_normalized_to_codex(self) -> None:
        # agent field may be "Codex" in the ledger/request_logs; must normalize.
        report = self._build(
            real_evidence(self.module, agents=("Codex", "CODEX")),
            "--upstream-no-auth",
        )
        self.assertTrue(report["ok"])
        self.assertEqual(report["ledger"]["request_match_count"], 2)

    def test_old_fake_shape_rejected(self) -> None:
        # The previous fake shape (decisions with agent_id/model, JSON dict
        # metrics, bare ledger wrapper) must be blocked in real mode.
        evidence = old_fake_evidence(self.module)
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "evidence_missing")

    def test_metrics_text_parser(self) -> None:
        module = self.module
        self.assertEqual(module.parse_metrics_count(METRICS_TEXT), 2)
        self.assertEqual(module.parse_metrics_count("dtoken_requests_total 7\n"), 7)
        self.assertIsNone(module.parse_metrics_count("no metric here"))
        self.assertIsNone(module.parse_metrics_count(None))
        # No metric line in /metrics text -> fall back to export.stats for
        # evidence presence; request_count records the fresh delta (2 here).
        report = self._build(
            real_evidence(self.module, metrics_text="# empty\n"),
            "--upstream-no-auth",
        )
        self.assertTrue(report["ok"])
        self.assertEqual(report["metrics"]["request_count"], 2)

    def test_metrics_and_stats_both_missing_blocked(self) -> None:
        evidence = real_evidence(self.module, metrics_text="# empty\n")
        evidence["export"]["stats"] = {"source": "dtoken-owned-runtime", "requests": {"total": None}}
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "evidence_missing")

    def test_metrics_request_count_is_fresh_delta_not_absolute(self) -> None:
        # request_count must record the fresh metrics delta, never the post
        # absolute counter (27 here), which could be far larger or reset.
        module = self.module
        evidence = real_evidence(module, metrics_text="dtoken_requests_total 27\n")
        report = module.build_report(
            args_for(module, "--upstream-no-auth"),
            evidence,
            metrics_delta=2,
        )
        self.assertTrue(report["ok"])
        self.assertEqual(report["metrics"]["request_count"], 2)

    def test_metrics_delta_below_two_blocked(self) -> None:
        # Freshness fails closed directly on delta < 2, regardless of what
        # the post absolute counter shows.
        module = self.module
        evidence = real_evidence(module, metrics_text="dtoken_requests_total 100\n")
        report = module.build_report(
            args_for(module, "--upstream-no-auth"),
            evidence,
            metrics_delta=1,
        )
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "stale_only")

    def test_expected_model_missing_blocked(self) -> None:
        # The expected model set is the validation collection: one expected
        # model absent from the fresh logs blocks the whole report.
        module = self.module
        evidence = real_evidence(module, models=("model-a",), request_ids=("req-1",), stats_total=1)
        report = module.build_report(
            args_for(module, "--upstream-no-auth"),
            evidence,
            expected_models=["model-a", "model-b"],
            metrics_delta=2,
        )
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "stale_only")

    def test_request_id_correlation_mismatch_blocked(self) -> None:
        # A route_decision whose request_id matches NOTHING in request_logs
        # cannot prove the chain; the request_log must have its own decision.
        evidence = real_evidence(self.module, decision_for=("req-999", None))
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "correlation_mismatch")
        # Failure details must never embed raw request_ids, only hashes.
        detail = report["failure"]["detail_redacted"]
        self.assertNotIn("req-1", detail)
        self.assertNotIn("req-999", detail)

    def test_route_decision_route_provider_mismatch_blocked(self) -> None:
        evidence = real_evidence(self.module)
        decision = evidence["export"]["route_decisions"][0]
        decision["provider_id"] = "provider-other"
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "correlation_mismatch")

    def test_route_decision_output_protocol_not_responses_blocked(self) -> None:
        evidence = real_evidence(self.module)
        evidence["export"]["route_decisions"][0]["output_protocol"] = "anthropic"
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "upstream_mismatch")

    def test_missing_ledger_events_blocked(self) -> None:
        # Zero ledger events must be blocked; ok=true with 0 is forbidden.
        evidence = real_evidence(self.module, events=False)
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "evidence_missing")

    def test_ledger_event_agent_mismatch_blocked(self) -> None:
        evidence = real_evidence(self.module, event_agent="opencode")
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "correlation_mismatch")

    def test_provider_metadata_missing_blocked(self) -> None:
        evidence = real_evidence(self.module, provider=False)
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "upstream_not_found")

    def test_provider_base_url_wrong_port_blocked(self) -> None:
        evidence = real_evidence(self.module, provider_base="http://127.0.0.1:20200/v1")
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "upstream_mismatch")

    def test_provider_protocol_not_responses_blocked(self) -> None:
        evidence = real_evidence(self.module, provider_protocol="anthropic")
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "upstream_mismatch")

    def test_provider_credential_configured_true_blocked(self) -> None:
        # requires_openai_auth=false must map to credential_configured=false;
        # a provider that requires auth cannot back the auth-free upstream.
        evidence = real_evidence(self.module, provider_credential=True)
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "upstream_mismatch")

    def test_route_meta_missing_blocked(self) -> None:
        evidence = real_evidence(self.module, route_meta=False)
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "upstream_not_found")

    def test_route_target_provider_mismatch_blocked(self) -> None:
        # A route whose target_provider_id points at a different provider
        # cannot prove the chain even when route metadata exists.
        evidence = real_evidence(self.module)
        evidence["export"]["active_config"]["routes"][0]["target_provider_id"] = "provider-other"
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "correlation_mismatch")

    def test_failure_details_hash_provider_and_route_ids(self) -> None:
        # provider_id/route_id must never appear raw in failure details; only
        # truncated hashes are allowed (same rule as request_ids).
        module = self.module
        for kwargs, expected_class in (
            ({"provider": False}, "upstream_not_found"),
            ({"route_meta": False}, "upstream_not_found"),
            ({"provider_base": "http://127.0.0.1:20200/v1"}, "upstream_mismatch"),
        ):
            evidence = real_evidence(module, **kwargs)
            report = self._build(evidence, "--upstream-no-auth")
            self.assertEqual(report["failure"]["class"], expected_class)
            detail = report["failure"]["detail_redacted"]
            self.assertNotIn(PROVIDER_ID, detail, "provider_id must be hashed in failure details")
            self.assertNotIn(ROUTE_ID, detail, "route_id must be hashed in failure details")
            self.assertRegex(detail, r"[0-9a-f]{16}", "failure detail must carry a truncated sha256")

    def test_runtime_unreachable_blocked(self) -> None:
        evidence = real_evidence(self.module, health=False)
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "runtime_unreachable")

    def test_runtime_identity_mismatch_blocked(self) -> None:
        evidence = real_evidence(self.module)
        evidence["health"] = {"status": "healthy", "component": "other", "runtime_identity": "other-identity"}
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "runtime_identity_mismatch")

    def test_no_codex_request_log_blocked(self) -> None:
        evidence = real_evidence(self.module, agents=("opencode", "claude"))
        report = self._build(evidence, "--upstream-no-auth")
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "no_routed_codex_request")

    def test_upstream_port_mismatch_blocked(self) -> None:
        # Evidence targets 10100 but the declared expectation is 20200.
        evidence = real_evidence(self.module)
        report = self._build(evidence, "--upstream-no-auth", "--upstream-base-url", "http://127.0.0.1:20200/v1")
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "upstream_mismatch")

    def test_secret_ref_and_url_never_emitted(self) -> None:
        module = self.module
        evidence = real_evidence(self.module, provider_base="http://127.0.0.1:10100/v1?api_key=sk-SECRETKEY")
        report = self._build(evidence, "--api-key-ref", "keychain://SERVICE/ACCOUNT")
        payload = json.dumps(report)
        # The reference string and the upstream URL (with its token) must not
        # appear; only redacted fingerprints are allowed.
        self.assertNotIn("keychain://SERVICE/ACCOUNT", payload)
        self.assertNotIn("sk-SECRETKEY", payload)
        self.assertNotIn("api_key=sk-SECRETKEY", payload)
        auth = report["route"]["auth"]
        self.assertEqual(auth["declared"], "ref")
        self.assertTrue(auth["fingerprint_sha256"])
        self.assertTrue(report["route"]["upstream"]["fingerprint_sha256"])

    def test_missing_evidence_report_is_schema_valid(self) -> None:
        module = self.module
        evidence = real_evidence(self.module, health=False)
        report = self._build(evidence, "--upstream-no-auth")
        schema = module.load_schema()
        self.assertIsNotNone(schema)
        errors = module.validate_report_against_schema(report, schema)  # type: ignore[arg-type]
        self.assertEqual(errors, [], "even a blocked report must satisfy the schema")


class CodexOcxlocalFreshRunTests(unittest.TestCase):
    """End-to-end main() fresh-run behavior: mocked codex subprocess and
    pre/post d-token evidence fetches. Only NEW post-fetch request_ids may
    satisfy the run; schema-only mode must never spawn codex."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.module = load_runner()

    def _run(
        self,
        pre_evidence: dict,
        post_evidence: dict,
        *extra: str,
        codex_ok: bool = True,
        source_name: str = "config.toml",
        source_symlink: bool = False,
        staged_sha_mismatch: bool = False,
    ) -> tuple[int, dict]:
        module = self.module
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source = tmp_path / source_name
            if source_symlink:
                real = tmp_path / "real-config.toml"
                real.write_text('[model_providers]\nprovider = "ocxlocal"\n', encoding="utf-8")
                source.symlink_to(real)
            else:
                source.write_text('[model_providers]\nprovider = "ocxlocal"\n', encoding="utf-8")
            # Platform-independent expected mode for the staged config
            # (Windows reports 0o666 where POSIX reports 0o644).
            self.source_mode = oct(source.stat().st_mode & 0o777)
            temp_root = tmp_path / "temp-root"
            temp_root.mkdir()
            out_path = tmp_path / "report.json"
            fetch_calls = {"n": 0}
            sha_calls = {"n": 0}

            def fake_fetch(base_url: str, timeout: float) -> dict:
                fetch_calls["n"] += 1
                return pre_evidence if fetch_calls["n"] == 1 else post_evidence

            def fake_sha256(path: Path) -> str:
                sha_calls["n"] += 1
                # Call 2 is the staged-copy fingerprint check; only that call
                # may differ when staged_sha_mismatch is requested.
                return "B" * 64 if staged_sha_mismatch and sha_calls["n"] == 2 else "A" * 64

            with mock.patch.object(module, "run_codex_exec", return_value=codex_ok), mock.patch.object(
                module, "fetch_runtime_evidence", side_effect=fake_fetch
            ), mock.patch.object(module, "file_sha256", side_effect=fake_sha256):
                argv = [
                    "--upstream-no-auth",
                    "--codex-config", str(source),
                    "--temp-root", str(temp_root),
                    "--model", "model-a",
                    "--model", "model-b",
                    "--report-out", str(out_path),
                    *extra,
                ]
                exit_code = module.main(argv)
            report = json.loads(out_path.read_text(encoding="utf-8")) if out_path.exists() else {}
            self.assertEqual([], [p.name for p in temp_root.iterdir()], "staging dir must be cleaned up")
        return exit_code, report

    def test_two_new_models_pass(self) -> None:
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(
            module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2
        )
        exit_code, report = self._run(pre, post)
        self.assertEqual(exit_code, 0)
        self.assertTrue(report["ok"])
        self.assertTrue(report["routed"])
        self.assertEqual(report["status"], "routed-verified")
        self.assertEqual(report["runtime"]["runtime_identity"], "dtoken-runtime-local")
        self.assertEqual(report["ledger"]["request_match_count"], 2)
        self.assertEqual(report["ledger"]["events_observed"], 2)
        self.assertEqual(report["metrics"]["request_count"], 2)
        self.assertEqual(report["route"]["model_forwarded"], ["model-a", "model-b"])
        self.assertTrue(report["route"]["dynamic_model"])
        freshness = report["freshness"]
        self.assertEqual(freshness["new_request_ids_count"], 2)
        self.assertTrue(freshness["new_request_ids_hash"])
        self.assertGreaterEqual(freshness["fresh_window_seconds"], 0)
        self.assertEqual(freshness["source_mode"], self.source_mode)
        self.assertTrue(freshness["source_fingerprint_sha256"])
        # The artifact is bound to the candidate commit (read-only git HEAD)
        # AND the staged source config digest; both must be present.
        candidate = report["candidate"]
        self.assertTrue(candidate["resolved"])
        self.assertRegex(candidate["commit"], r"[0-9a-fA-F]{40,64}")

    def test_candidate_commit_unresolved_fails_closed(self) -> None:
        # A run that would otherwise pass must fail closed
        # (candidate_unbound) when the candidate commit cannot be resolved:
        # unbound evidence must never be claimable as routed acceptance.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        with mock.patch.object(module, "git_head", return_value=("", False)):
            exit_code, report = self._run(pre, post)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "candidate_unbound")
        self.assertEqual(report["candidate"], {"commit": "", "resolved": False})

    def test_blocked_report_keeps_class_when_commit_unresolved(self) -> None:
        # Blocked/failed artifacts record the unbound candidate as metadata
        # but never mask the original failure class.
        module = self.module
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        with mock.patch.object(module, "git_head", return_value=("", False)):
            exit_code, report = self._run(
                {"health": None, "export": None, "metrics_text": None},
                post,
            )
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertEqual(report["failure"]["class"], "runtime_unreachable")
        self.assertEqual(report["candidate"], {"commit": "", "resolved": False})

    def test_pre_fetch_evidence_missing_blocked(self) -> None:
        # A failed pre-fetch must fail closed: without a valid baseline,
        # pre-existing requests could satisfy the fresh window.
        module = self.module
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        exit_code, report = self._run(
            {"health": None, "export": None, "metrics_text": None},
            post,
        )
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "runtime_unreachable")

    def test_pre_fetch_foreign_source_blocked(self) -> None:
        # A pre-fetch export not owned by the d-token runtime is an invalid
        # baseline and must fail closed as well.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        pre["export"]["source"] = "not-dtoken-owned-runtime"
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        exit_code, report = self._run(pre, post)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "runtime_unreachable")

    def test_pre_counter_baseline_unavailable_blocked(self) -> None:
        # No pre-fetch counter at all (metrics text empty AND export stats
        # missing) must fail closed: an absolute post counter alone can never
        # prove a fresh delta.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=None, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        exit_code, report = self._run(pre, post)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "evidence_missing")

    def test_post_counter_missing_for_chosen_source_blocked(self) -> None:
        # The counter source is fixed on the pre side (metrics text here); a
        # post missing that same source must fail closed instead of silently
        # switching to export.stats.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="dtoken_requests_total 0\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2, metrics_text="# empty\n")
        exit_code, report = self._run(pre, post)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "evidence_missing")

    def test_staged_copy_mismatch_blocked(self) -> None:
        # The staged config.toml must byte-match the fingerprinted source; a
        # drifted copy (source changed between hash and copy) fails closed.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        exit_code, report = self._run(pre, post, staged_sha_mismatch=True)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "source_changed")

    def test_concurrent_non_expected_codex_logs_ignored(self) -> None:
        # A same-window Codex request for a non-expected model (concurrent
        # workload) must be ignored: it may not fail the run, enter the
        # forwarded model set or counts, or the freshness id hash/count.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(
            module,
            request_ids=("req-new-1", "req-new-2", "req-concurrent-1"),
            models=("model-a", "model-b", "model-other"),
            stats_total=2,
        )
        exit_code, report = self._run(pre, post)
        self.assertEqual(exit_code, 0)
        self.assertTrue(report["ok"])
        self.assertTrue(report["routed"])
        # Only the two expected models are correlated and forwarded.
        self.assertEqual(report["route"]["model_forwarded"], ["model-a", "model-b"])
        self.assertEqual(report["ledger"]["request_match_count"], 2)
        self.assertEqual(report["ledger"]["events_observed"], 2)
        # request_count is the fresh metrics delta (0 -> 2).
        self.assertEqual(report["metrics"]["request_count"], 2)
        freshness = report["freshness"]
        # The concurrent request_id must not enter the freshness record.
        self.assertEqual(freshness["new_request_ids_count"], 2)
        expected_hash = module._sha256("\n".join(sorted(["req-new-1", "req-new-2"])))
        self.assertEqual(freshness["new_request_ids_hash"], expected_hash)

    def test_codex_not_started_cannot_be_routed(self) -> None:
        # codex exec cannot spawn / never ran -> execution_failed, never routed.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        exit_code, report = self._run(pre, post, codex_ok=False)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "execution_failed")

    def test_stale_only_blocked(self) -> None:
        # Post-fetch adds no NEW request_ids -> stale-only, blocked.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0)
        exit_code, report = self._run(pre, post)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "stale_only")

    def test_one_model_blocked(self) -> None:
        # Two models passed, but only one appears in the fresh request_logs.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-a"), stats_total=2)
        exit_code, report = self._run(pre, post)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["ok"])
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "stale_only")

    def test_subprocess_failure_blocked(self) -> None:
        # subprocess fails (timeout / non-zero) -> execution_failed, blocked.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        exit_code, report = self._run(pre, post, codex_ok=False)
        self.assertEqual(exit_code, 1)
        self.assertFalse(report["routed"])
        self.assertEqual(report["failure"]["class"], "execution_failed")

    def test_reject_auth_source_filename(self) -> None:
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        exit_code, report = self._run(pre, post, source_name="auth-config.toml")
        self.assertEqual(exit_code, 2)
        self.assertFalse(report.get("routed", False))

    @unittest.skipIf(os.name == "nt", "symlink creation needs developer mode/admin on Windows")
    def test_reject_symlink_source(self) -> None:
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        exit_code, report = self._run(pre, post, source_symlink=True)
        self.assertEqual(exit_code, 2)
        self.assertFalse(report.get("routed", False))

    def test_report_has_no_absolute_paths_or_raw_output(self) -> None:
        # Artifact must not contain absolute paths, command output, prompt or
        # config content; only fingerprints/modes/hashes.
        module = self.module
        pre = real_evidence(module, request_ids=("req-old",), models=("model-a",), stats_total=0, metrics_text="# empty\n")
        post = real_evidence(module, request_ids=("req-new-1", "req-new-2"), models=("model-a", "model-b"), stats_total=2)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source = tmp_path / "config.toml"
            source.write_text('[model_providers]\nprovider = "ocxlocal"\n', encoding="utf-8")
            temp_root = tmp_path / "temp-root"
            temp_root.mkdir()
            out_path = tmp_path / "report.json"
            fetch_calls = {"n": 0}

            def fake_fetch(base_url: str, timeout: float) -> dict:
                fetch_calls["n"] += 1
                return pre if fetch_calls["n"] == 1 else post

            with mock.patch.object(module, "run_codex_exec", return_value=True), mock.patch.object(
                module, "fetch_runtime_evidence", side_effect=fake_fetch
            ):
                exit_code = module.main(
                    [
                        "--upstream-no-auth",
                        "--codex-config", str(source),
                        "--temp-root", str(temp_root),
                        "--model", "model-a",
                        "--model", "model-b",
                        "--report-out", str(out_path),
                    ]
                )
            self.assertEqual(exit_code, 0)
            payload = out_path.read_text(encoding="utf-8")
            self.assertNotIn(str(tmp_path), payload)
            self.assertNotIn(str(temp_root), payload)
            self.assertNotIn(module.ACCEPTANCE_PROMPT, payload)
            self.assertNotIn("model_providers", payload)
            self.assertNotIn("[model_providers]", payload)


class CodexOcxlocalNoListenerContractTests(unittest.TestCase):
    """The runner must never fabricate a listener or probe the upstream."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.module = load_runner()

    def test_no_temporary_listener_and_no_upstream_probe(self) -> None:
        source = SCRIPT.read_text(encoding="utf-8")
        # No socket binding, no temporary listener port, no direct upstream probe.
        self.assertNotIn("socket", source)
        self.assertNotIn("18102", source)
        self.assertNotIn("_bind_listener", source)
        # Evidence is consumed from the d-token service port only.
        self.assertIn("DTOKEN_SERVICE_PORT = 18001", source)
        self.assertIn("/health", source)
        self.assertIn("/export/json", source)
        self.assertIn("/metrics", source)

    def test_runner_never_resolves_or_emits_credentials(self) -> None:
        source = SCRIPT.read_text(encoding="utf-8")
        self.assertNotIn("keyring", source)
        self.assertNotIn("get_password", source)
        self.assertNotIn("sk-", source)

    def test_schema_and_template_artifacts_exist(self) -> None:
        module = self.module
        self.assertTrue(module.REPORT_SCHEMA_FILE.is_file(), "report.schema.json fixture missing")
        self.assertTrue(module.REPORT_TEMPLATE_FILE.is_file(), "report.template.json fixture missing")
        # The tracked schema/template fixtures live under support/fixtures/.
        self.assertIn("fixtures", str(module.REPORT_SCHEMA_FILE))
        schema = json.loads(module.REPORT_SCHEMA_FILE.read_text(encoding="utf-8"))
        template = json.loads(module.REPORT_TEMPLATE_FILE.read_text(encoding="utf-8"))
        # The template must itself satisfy the schema (template/schema no drift).
        errors = module.validate_report_against_schema(template, schema)
        self.assertEqual(errors, [], f"template violates schema: {errors}")

    def test_fixtures_loadable_via_load_schema(self) -> None:
        # load_schema() must read the tracked fixtures (not an ignored
        # artifacts dir) and the template must validate against it.
        module = self.module
        self.assertIn("fixtures", str(module.REPORT_SCHEMA_FILE))
        schema = module.load_schema()
        self.assertIsNotNone(schema, "load_schema() must load the tracked schema fixture")
        assert schema is not None
        self.assertEqual(schema["type"], "object")
        self.assertEqual(schema["properties"]["mode"]["const"], "codex-ocxlocal-routed-acceptance")
        template = json.loads(module.REPORT_TEMPLATE_FILE.read_text(encoding="utf-8"))
        self.assertEqual(template["schema_version"], 1)
        self.assertEqual(module.validate_report_against_schema(template, schema), [])

    def test_schema_enum_allows_correlation_mismatch(self) -> None:
        module = self.module
        schema = json.loads(module.REPORT_SCHEMA_FILE.read_text(encoding="utf-8"))
        classes = schema["properties"]["failure"]["properties"]["class"]["enum"]
        self.assertIn("correlation_mismatch", classes)


if __name__ == "__main__":
    unittest.main()
