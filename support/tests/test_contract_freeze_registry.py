"""CF-3 contract-freeze-registry tests.

Validates the frozen CandidateEvidenceEnvelope schema (Draft 2020-12) and the
frozen four-source Agent capability matrix. No network, no dependency install,
no git operations. Every source of the four-source union is anchored to its own
real artifact rather than self-verified against the matrix:
  * Rust 23-agent registry: parses agent.rs KNOWN_AGENT_ADAPTERS ids;
  * cc-switch: read-only SQLite query of the committed fixture
    (select distinct app_type from providers union select app_type from proxy_config)
    — the generator is never executed because its non---check mode prints the
    secret marker;
  * target spec v2.1: parses the terms table Agent row (Claude Code/Codex/
    OpenCode/Cline/Cursor) from docs/d-token_AI_product_target_spec_v2.md;
  * UNIFIED_RELEASE_DESIGN §5 product-chain line (OpenCode/Claude Code/Codex/
    Cursor/Cline/OpenClaw).
Cross-artifact contracts (envelope<->matrix binding, RFC3339 timestamp
ordering) are exercised by executable helpers — JSON Schema alone cannot bind
across files or compare two timestamp fields, so this is explicitly NOT pure
schema validation.
"""

from __future__ import annotations

import json
import re
import sqlite3
import unittest
from copy import deepcopy
from datetime import datetime
from pathlib import Path

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA_PATH = ROOT / "schemas" / "candidate-evidence-envelope.schema.json"
MATRIX_PATH = ROOT / "fixtures" / "agent-capability-matrix.json"
CC_SWITCH_DB = ROOT / "fixtures" / "cc-switch" / "cc-switch.db"
TARGET_SPEC_PATH = ROOT.parent / "docs" / "d-token_AI_product_target_spec_v2.md"
UNIFIED_DESIGN_PATH = ROOT.parent / ".ai-bridge" / "release-iteration" / "UNIFIED_RELEASE_DESIGN.md"
AGENT_RS = ROOT.parent / "code" / "crates" / "dtoken-config-patcher" / "src" / "agent.rs"
AGENT_TRANSPORT_RS = ROOT.parent / "code" / "crates" / "dtoken-config-patcher" / "src" / "agent_transport.rs"

REQUIRED_BASE_FIELDS = [
    "schema_version",
    "node_id",
    "status",
    "reason_codes",
    "candidate_commit",
    "source_digest",
    "registry_digest",
    "corpus_digests",
    "started_at",
    "finished_at",
    "platform",
    "producer_revision",
    "artifact_sha256",
    "agent_id",
    "capability_tier",
    "privacy_assertions",
    "not_applicable_reasons",
    "evidence_kind",
    "route_decision_digest",
    "capability_digest",
    "renderer_digest",
    "profile_digest",
    "font_digest",
    "tokenizer_digest",
    "pricing_digest",
    "cache_digest",
    "eval_digest",
]

OPTICAL_DIGESTS = [
    "renderer_digest",
    "profile_digest",
    "font_digest",
    "tokenizer_digest",
    "pricing_digest",
    "cache_digest",
    "eval_digest",
]

NULLABLE_FIELDS = [
    "agent_id",
    "capability_tier",
    "route_decision_digest",
    "capability_digest",
] + OPTICAL_DIGESTS

SHA256 = "a" * 64
SHA1_40 = "b" * 40

ASPECT_NAMES = ("discovery", "configuration", "authentication", "change_detection", "verification", "rollback")
ASPECT_FIELDS = (
    "official_source",
    "version",
    "config_path",
    "field_ownership",
    "write_policy",
    "auth_owner",
    "watcher",
    "failure_recovery_acceptance",
    "ui_status",
)
WRITE_POLICIES = {"always_read_only", "user_selected_managed", "unsupported"}
TIERS = {"route", "mcp_hook", "detection_only"}


def _load() -> tuple[dict, dict]:
    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    matrix = json.loads(MATRIX_PATH.read_text(encoding="utf-8"))
    return schema, matrix


def _base_envelope() -> dict:
    """A fully valid generic envelope: no agent, every nullable field null with a reason."""
    reasons = {field: f"not applicable for generic envelope: {field}" for field in NULLABLE_FIELDS}
    return {
        "schema_version": "1.0.0",
        "node_id": "N9",
        "status": "pass",
        "reason_codes": ["frozen-contract-ok"],
        "candidate_commit": SHA1_40,
        "source_digest": SHA256,
        "registry_digest": SHA256,
        "corpus_digests": {"primary": SHA256},
        "started_at": "2026-08-12T00:00:00Z",
        "finished_at": "2026-08-12T01:00:00Z",
        "platform": {"os": "macos", "arch": "aarch64", "toolchain": "stable"},
        "producer_revision": "cf-3-contract-test",
        "artifact_sha256": SHA256,
        "agent_id": None,
        "capability_tier": None,
        "privacy_assertions": {
            "no_secret": True,
            "no_complete_prompt_or_response": True,
            "no_source_code": True,
            "no_absolute_paths": True,
            "no_raw_command_output": True,
            "redaction_verified": True,
        },
        "not_applicable_reasons": reasons,
        "evidence_kind": "generic",
        "route_decision_digest": None,
        "capability_digest": None,
        **{digest: None for digest in OPTICAL_DIGESTS},
    }


def _route_envelope() -> dict:
    envelope = _base_envelope()
    envelope["evidence_kind"] = "route"
    envelope["node_id"] = "N2"
    envelope["agent_id"] = "codex"
    envelope["capability_tier"] = "route"
    envelope["route_decision_digest"] = SHA256
    envelope["capability_digest"] = SHA256
    # Optical digests stay null and keep their N/A reasons; only non-null
    # fields must drop the contradictory reason.
    envelope["not_applicable_reasons"].pop("agent_id", None)
    envelope["not_applicable_reasons"].pop("capability_tier", None)
    envelope["not_applicable_reasons"].pop("route_decision_digest", None)
    envelope["not_applicable_reasons"].pop("capability_digest", None)
    return envelope


def _optical_envelope() -> dict:
    envelope = _base_envelope()
    envelope["evidence_kind"] = "optical"
    envelope["node_id"] = "N10"
    for digest in OPTICAL_DIGESTS:
        envelope[digest] = SHA256
        envelope["not_applicable_reasons"].pop(digest, None)
    return envelope


class CandidateEvidenceEnvelopeSchemaTests(unittest.TestCase):
    """Contract requirement 1/2/7: schema validity, strictness, N/A semantics."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.schema, _ = _load()
        Draft202012Validator.check_schema(cls.schema)
        cls.validator = Draft202012Validator(cls.schema)

    def test_schema_self_valid_draft2020_12(self) -> None:
        Draft202012Validator.check_schema(self.schema)

    def test_schema_is_strict_additional_properties_false(self) -> None:
        self.assertIs(self.schema["additionalProperties"], False)
        self.assertIn("allOf", self.schema)

    def test_schema_requires_all_base_fields(self) -> None:
        self.assertEqual(set(self.schema["required"]), set(REQUIRED_BASE_FIELDS))

    def test_privacy_assertions_are_required_and_true(self) -> None:
        props = self.schema["properties"]["privacy_assertions"]
        self.assertIs(props["additionalProperties"], False)
        self.assertEqual(
            set(props["required"]),
            {
                "no_secret",
                "no_complete_prompt_or_response",
                "no_source_code",
                "no_absolute_paths",
                "no_raw_command_output",
                "redaction_verified",
            },
        )
        for assertion in props["required"]:
            self.assertEqual(props["properties"][assertion], {"const": True}, assertion)

    def test_platform_is_strict(self) -> None:
        platform = self.schema["properties"]["platform"]
        self.assertIs(platform["additionalProperties"], False)
        self.assertEqual(set(platform["required"]), {"os", "arch", "toolchain"})
        self.assertEqual(set(platform["properties"]["os"]["enum"]), {"macos", "windows", "linux"})
        self.assertEqual(set(platform["properties"]["arch"]["enum"]), {"x86_64", "aarch64"})

    def test_digest_and_commit_patterns(self) -> None:
        props = self.schema["properties"]
        self.assertEqual(props["candidate_commit"]["pattern"], "^[0-9a-f]{40}$")
        self.assertEqual(props["source_digest"]["pattern"], "^[0-9a-f]{64}$")
        for field in ["registry_digest", "artifact_sha256", "route_decision_digest", "capability_digest"] + OPTICAL_DIGESTS:
            self.assertEqual(props[field]["pattern"], "^[0-9a-f]{64}$", field)

    def test_generic_fixture_valid(self) -> None:
        self.assertEqual(list(self.validator.iter_errors(_base_envelope())), [])

    def test_route_fixture_valid(self) -> None:
        self.assertEqual(list(self.validator.iter_errors(_route_envelope())), [])

    def test_optical_fixture_valid(self) -> None:
        self.assertEqual(list(self.validator.iter_errors(_optical_envelope())), [])

    def test_missing_privacy_assertions_fails(self) -> None:
        envelope = _base_envelope()
        del envelope["privacy_assertions"]
        self.assertTrue(list(self.validator.iter_errors(envelope)))

    def test_missing_required_base_field_fails(self) -> None:
        for field in ["candidate_commit", "artifact_sha256", "platform", "source_digest", "evidence_kind"]:
            envelope = _base_envelope()
            del envelope[field]
            self.assertTrue(list(self.validator.iter_errors(envelope)), f"missing {field} should fail")

    def test_null_without_reason_fails(self) -> None:
        for field in NULLABLE_FIELDS:
            envelope = _base_envelope()
            del envelope["not_applicable_reasons"][field]
            self.assertTrue(list(self.validator.iter_errors(envelope)), f"null {field} without reason should fail")

    def test_non_null_with_reason_fails(self) -> None:
        # A field that has a value must not carry a contradictory N/A reason.
        envelope = _route_envelope()
        envelope["not_applicable_reasons"]["route_decision_digest"] = "still applicable"
        self.assertTrue(list(self.validator.iter_errors(envelope)))

    def test_route_digest_null_fails(self) -> None:
        envelope = _route_envelope()
        envelope["route_decision_digest"] = None
        envelope["not_applicable_reasons"]["route_decision_digest"] = "route evidence pending"
        self.assertTrue(list(self.validator.iter_errors(envelope)))

    def test_route_capability_digest_null_fails(self) -> None:
        envelope = _route_envelope()
        envelope["capability_digest"] = None
        envelope["not_applicable_reasons"]["capability_digest"] = "route evidence pending"
        self.assertTrue(list(self.validator.iter_errors(envelope)))

    def test_optical_digest_null_fails(self) -> None:
        for digest in OPTICAL_DIGESTS:
            envelope = _optical_envelope()
            envelope[digest] = None
            envelope["not_applicable_reasons"][digest] = "optical evidence pending"
            self.assertTrue(list(self.validator.iter_errors(envelope)), f"null {digest} should fail")

    def test_privacy_false_combinations_fail(self) -> None:
        # Any combination where secret/raw/absolute-path assertion is false must fail.
        false_combos = [
            {"no_secret": False},
            {"no_raw_command_output": False},
            {"no_absolute_paths": False},
            {"no_complete_prompt_or_response": False},
            {"no_source_code": False},
            {"redaction_verified": False},
            {"no_secret": False, "no_raw_command_output": False},
            {"no_absolute_paths": False, "no_source_code": False, "redaction_verified": False},
        ]
        for combo in false_combos:
            envelope = _base_envelope()
            envelope["privacy_assertions"].update(combo)
            self.assertTrue(list(self.validator.iter_errors(envelope)), f"false combo {combo} should fail")

    def test_additional_properties_fail(self) -> None:
        envelope = _base_envelope()
        envelope["unexpected_top_level"] = "x"
        self.assertTrue(list(self.validator.iter_errors(envelope)))
        envelope = _base_envelope()
        envelope["platform"]["unexpected"] = "y"
        self.assertTrue(list(self.validator.iter_errors(envelope)))

    def test_unknown_status_and_tier_fail(self) -> None:
        envelope = _base_envelope()
        envelope["status"] = "passing"
        self.assertTrue(list(self.validator.iter_errors(envelope)))
        envelope = _base_envelope()
        envelope["capability_tier"] = "proxy"
        envelope["not_applicable_reasons"].pop("capability_tier", None)
        self.assertTrue(list(self.validator.iter_errors(envelope)))

    def test_non_pass_status_requires_reason_codes(self) -> None:
        for status in ("fail", "blocked", "unavailable", "partial"):
            envelope = _base_envelope()
            envelope["status"] = status
            envelope["reason_codes"] = []
            self.assertTrue(list(self.validator.iter_errors(envelope)), f"{status} with empty reason_codes must fail")
            envelope["reason_codes"] = ["evidence-missing"]
            self.assertEqual(list(self.validator.iter_errors(envelope)), [], f"{status} with a reason code must pass")

    def test_pass_may_keep_empty_reason_codes(self) -> None:
        # Frozen design example: reason_codes: [] is valid for a passing envelope.
        envelope = _base_envelope()
        envelope["reason_codes"] = []
        self.assertEqual(list(self.validator.iter_errors(envelope)), [])

    def test_whitespace_only_reason_codes_fail(self) -> None:
        envelope = _base_envelope()
        envelope["status"] = "fail"
        envelope["reason_codes"] = ["   "]
        self.assertTrue(list(self.validator.iter_errors(envelope)))
        envelope["reason_codes"] = ["\t\n "]
        self.assertTrue(list(self.validator.iter_errors(envelope)))

    def test_whitespace_only_na_reasons_fail(self) -> None:
        for field in NULLABLE_FIELDS:
            envelope = _base_envelope()
            envelope["not_applicable_reasons"][field] = "   "
            self.assertTrue(list(self.validator.iter_errors(envelope)), f"whitespace reason for {field} must fail")

    def test_na_reasons_reject_whitespace_positive(self) -> None:
        # A reason with a real non-whitespace character still passes.
        envelope = _base_envelope()
        envelope["not_applicable_reasons"]["agent_id"] = " generic aggregate envelope "
        self.assertEqual(list(self.validator.iter_errors(envelope)), [])


def rust_registry_agent_ids() -> set[str]:
    """Parse the 23 AgentAdapterDescriptor ids from the Rust registry source."""
    text = AGENT_RS.read_text(encoding="utf-8")
    block = re.search(r"pub const KNOWN_AGENT_ADAPTERS:.*?= &\[(.*?)\];", text, re.S)
    if not block:
        raise AssertionError(f"KNOWN_AGENT_ADAPTERS block not found in {AGENT_RS}")
    ids = set(re.findall(r'id:\s*"([a-z0-9-]+)"', block.group(1)))
    if len(ids) != 23:
        raise AssertionError(f"expected 23 AgentAdapterDescriptor ids, parsed {len(ids)}: {sorted(ids)}")
    return ids


def cc_switch_fixture_app_types() -> set[str]:
    """Read-only anchor: distinct app_type across providers/proxy_config.

    Queries the committed cc-switch fixture DB directly (mode=ro). The
    generator is deliberately NOT executed: outside --check it prints the
    secret marker, which must never reach assertions or logs. Only app_type
    values are read; settings_config content is never inspected.
    """
    connection = sqlite3.connect(f"file:{CC_SWITCH_DB}?mode=ro", uri=True)
    try:
        rows = connection.execute(
            "select distinct app_type from providers"
            " union select app_type from proxy_config"
        ).fetchall()
    finally:
        connection.close()
    return {row[0] for row in rows}


def _to_agent_id(name: str) -> str:
    """'Claude Code' -> 'claude-code'; 'OpenCode' -> 'opencode'."""
    return name.strip().lower().replace(" ", "-")


def target_spec_terms_agent_ids() -> set[str]:
    """Parse the target spec terms table Agent row (术语与命名规范 §2).

    Local section/table parse: locate the '## 2. 术语与命名规范' section, take
    the '| Agent |' row cell, split on '、', extract leading ASCII names, and
    convert each to a matrix agent_id. Refuses whole-file scanning with a
    hardcoded expected set (no self-verification).
    """
    text = TARGET_SPEC_PATH.read_text(encoding="utf-8")
    section = re.search(r"##\s*2\.\s*术语与命名规范\n(.*?)(?=\n##\s|\Z)", text, re.S)
    if not section:
        raise AssertionError("target spec §2 术语与命名规范 section not found")
    row = re.search(r"^\|\s*Agent\s*\|\s*(.+?)\s*\|", section.group(1), re.M)
    if not row:
        raise AssertionError("target spec terms table Agent row not found")
    cell = row.group(1)
    ids: set[str] = set()
    for token in cell.split("、"):
        name = re.match(r"[A-Za-z][A-Za-z ]*", token.strip())
        if name:
            ids.add(_to_agent_id(name.group(0)))
    return ids


def unified_product_chain_agent_ids() -> set[str]:
    """Parse the UNIFIED_RELEASE_DESIGN §5 product-chain line into agent ids.

    Local section parse: locate '## 5. 总体架构', take the line that starts with
    'OpenCode' (the product chain), split on ' / ', and convert each ASCII
    agent name to a matrix agent_id.
    """
    text = UNIFIED_DESIGN_PATH.read_text(encoding="utf-8")
    section = re.search(r"##\s*5\.\s*总体架构\n(.*?)(?=\n##\s|\Z)", text, re.S)
    if not section:
        raise AssertionError("UNIFIED_RELEASE_DESIGN §5 section not found")
    chain = re.search(r"^\s*OpenCode[^\n]*$", section.group(1), re.M)
    if not chain:
        raise AssertionError("UNIFIED_RELEASE_DESIGN §5 product chain line not found")
    ids: set[str] = set()
    for token in chain.group(0).split("/"):
        name = re.match(r"[A-Za-z][A-Za-z ]*", token.strip())
        if name:
            ids.add(_to_agent_id(name.group(0)))
    return ids


def agent_transport_configurable_adapter_count() -> int:
    """Count can_configure=true descriptors in agent_transport.rs ADAPTERS."""
    text = AGENT_TRANSPORT_RS.read_text(encoding="utf-8")
    block = re.search(r"const ADAPTERS:.*?= &\[(.*?)\];", text, re.S)
    if not block:
        raise AssertionError(f"ADAPTERS block not found in {AGENT_TRANSPORT_RS}")
    return len(re.findall(r"can_configure:\s*true", block.group(1)))


def envelope_matrix_binding_errors(envelope: dict, matrix: dict) -> list[str]:
    """Fail-closed envelope<->matrix binding contract (cross-artifact helper).

    JSON Schema cannot import the matrix fixture, so consumers
    (PR-7/PR-9/C8C/N5/N9) run this helper and block on any error. Rules are
    frozen in the schema description and matrix envelope_binding_rule:
      1. agent_id registered in the matrix -> a non-null capability_tier MUST
         equal that agent's official_capability;
      2. an unregistered agent_id MUST NOT declare capability_tier=route.
    Returns an empty list when the envelope complies.
    """
    errors: list[str] = []
    agent_id = envelope.get("agent_id")
    tier = envelope.get("capability_tier")
    if agent_id is None or tier is None:
        return errors  # N/A envelope; binding does not apply
    agents = {a["agent_id"]: a for a in matrix["agents"]}
    if agent_id in agents:
        expected = agents[agent_id]["official_capability"]
        if tier != expected:
            errors.append(
                f"agent_id={agent_id} capability_tier={tier} != matrix official_capability={expected}"
            )
    elif tier == "route":
        errors.append(f"agent_id={agent_id} not registered in matrix; capability_tier=route is forbidden")
    return errors


def timestamp_order_errors(envelope: dict) -> list[str]:
    """RFC3339 finished_at >= started_at, enforced by the downstream aggregator.

    Deliberately NOT a JSON Schema cross-field comparison (schemas cannot
    compare two timestamps); consumers parse both as RFC3339 and block when
    finished_at precedes started_at.
    """
    started = datetime.fromisoformat(envelope["started_at"].replace("Z", "+00:00"))
    finished = datetime.fromisoformat(envelope["finished_at"].replace("Z", "+00:00"))
    if finished < started:
        return ["finished_at precedes started_at (RFC3339); consumer must block"]
    return []


class AgentCapabilityMatrixTests(unittest.TestCase):
    """Contract requirement 3/4/5/6: union completeness, tiers, vocabulary, boundaries."""

    @classmethod
    def setUpClass(cls) -> None:
        _, cls.matrix = _load()

    def test_rust_registry_has_23_ids(self) -> None:
        self.assertEqual(len(rust_registry_agent_ids()), 23)

    def test_matrix_covers_all_23_rust_registry_agents(self) -> None:
        registry_ids = rust_registry_agent_ids()
        matrix_ids = {agent["agent_id"] for agent in self.matrix["agents"]}
        self.assertEqual(registry_ids, matrix_ids)

    def test_adapter_counts_match_rust_source(self) -> None:
        # The matrix must state the approved verified Route count (11) and the
        # code-declared configurable adapter count parsed from agent_transport.rs
        # (15) as separate facts — never collapsed into one number.
        rust = self.matrix["source_contracts"]["rust_registry_23_agents"]
        self.assertEqual(rust["approved_verified_adapter_count"], 11)
        self.assertEqual(rust["code_declared_configurable_adapters"], 15)
        parsed = agent_transport_configurable_adapter_count()
        self.assertEqual(parsed, 15)
        self.assertEqual(rust["code_declared_configurable_adapters"], parsed)
        self.assertNotEqual(
            rust["approved_verified_adapter_count"],
            rust["code_declared_configurable_adapters"],
            "approved-verified and code-declared counts must stay distinct facts",
        )

    def test_hermes_config_reference_is_l246(self) -> None:
        # agent.rs declares HERMES_CONFIG at L246 (home_config! macro); the
        # matrix must not drift back to the stale L249 reference.
        hermes = next(a for a in self.matrix["agents"] if a["agent_id"] == "hermes")
        self.assertIn("HERMES_CONFIG (L246)", hermes["configuration"]["official_source"])
        self.assertNotIn("HERMES_CONFIG (L249)", hermes["configuration"]["official_source"])

    def test_envelope_binding_rule_is_frozen_in_matrix(self) -> None:
        rule = self.matrix["envelope_binding_rule"]
        self.assertIn("official_capability", rule["rule"])
        self.assertIn("route", rule["rule"])
        self.assertEqual(set(rule["enforced_by_consumers"]), {"PR-7", "PR-9", "C8C", "N5", "N9"})

    def test_agent_ids_are_unique(self) -> None:
        ids = [agent["agent_id"] for agent in self.matrix["agents"]]
        self.assertEqual(len(ids), len(set(ids)))

    def test_four_source_union_complete(self) -> None:
        # Real-source anchors, not matrix self-declaration: each of the four
        # sources is parsed/queried from its own artifact and must resolve into
        # the matrix (single-source agents are never lost).
        matrix_ids = {agent["agent_id"] for agent in self.matrix["agents"]}
        rust_ids = rust_registry_agent_ids()
        self.assertEqual(rust_ids, matrix_ids)  # Rust 23 registry = exact matrix coverage
        for app_type in cc_switch_fixture_app_types():
            self.assertIn(self.matrix["cc_switch_aliases"][app_type], matrix_ids, app_type)
        self.assertTrue(target_spec_terms_agent_ids() <= matrix_ids)
        self.assertTrue(unified_product_chain_agent_ids() <= matrix_ids)

    def test_cc_switch_app_types_exact_from_fixture(self) -> None:
        # The matrix's declared app_types must equal the committed fixture's
        # actual distinct app_type rows (read-only), never a hand-written twin.
        fixture_app_types = cc_switch_fixture_app_types()
        declared = set(self.matrix["source_contracts"]["cc_switch_fixture_app_types"]["app_types"])
        self.assertEqual(fixture_app_types, declared)
        self.assertEqual(fixture_app_types, {"claude", "codex", "gemini"})

    def test_target_spec_terms_exact(self) -> None:
        # The matrix's target_spec_v2_1 agents must equal the terms table Agent
        # row converted to ids — not a hardcoded echo of the matrix itself.
        parsed = target_spec_terms_agent_ids()
        declared = set(self.matrix["source_contracts"]["target_spec_v2_1"]["agents"])
        self.assertEqual(parsed, declared)
        self.assertEqual(parsed, {"opencode", "claude-code", "codex", "cline", "cursor"})

    def test_unified_product_chain_exact(self) -> None:
        # This round's direct requirements must equal the UNIFIED §5 product
        # chain line converted to ids — an independent design-source anchor.
        parsed = unified_product_chain_agent_ids()
        declared = set(self.matrix["source_contracts"]["this_round_direct_requirements"]["agents"])
        self.assertEqual(parsed, declared)
        self.assertEqual(parsed, {"opencode", "claude-code", "codex", "cursor", "cline", "openclaw"})

    def test_cc_switch_aliases_are_complete(self) -> None:
        self.assertEqual(
            self.matrix["cc_switch_aliases"],
            {"claude": "claude-code", "codex": "codex", "gemini": "gemini-cli"},
        )
        fixture_app_types = cc_switch_fixture_app_types()
        self.assertEqual(fixture_app_types, {"claude", "codex", "gemini"})
        self.assertEqual(set(self.matrix["cc_switch_aliases"].keys()), fixture_app_types)

    def test_release_tier_counts_match_approved_design(self) -> None:
        summary = self.matrix["release_tier_summary"]
        self.assertEqual(summary["route"], 11)
        self.assertEqual(summary["detection_only"], 12)
        self.assertEqual(summary["mcp_hook"], 0)
        self.assertEqual(summary["total"], 23)
        actual = {
            "route": sum(1 for a in self.matrix["agents"] if a["official_capability"] == "route"),
            "detection_only": sum(1 for a in self.matrix["agents"] if a["official_capability"] == "detection_only"),
            "mcp_hook": sum(1 for a in self.matrix["agents"] if a["official_capability"] == "mcp_hook"),
        }
        self.assertEqual(actual, {"route": 11, "detection_only": 12, "mcp_hook": 0})

    def test_approved_11_route_set_is_exact(self) -> None:
        approved_route = {
            "claude-code", "codex", "cline", "goose", "kilo-code",
            "opencode", "openclaw", "gemini-cli", "roo-code", "continue", "aider",
        }
        route_agents = {a["agent_id"] for a in self.matrix["agents"] if a["official_capability"] == "route"}
        self.assertEqual(route_agents, approved_route)

    def test_tiers_and_write_policies_are_legal(self) -> None:
        for agent in self.matrix["agents"]:
            self.assertIn(agent["official_capability"], TIERS, agent["agent_id"])
            self.assertIn(agent["declared_integration_kind"], {"route", "mcp", "hook"}, agent["agent_id"])
            self.assertIn("current_code_caveat", agent, agent["agent_id"])
            for aspect in ASPECT_NAMES:
                self.assertIn(aspect, agent, f"{agent['agent_id']} missing {aspect}")
                for field in ASPECT_FIELDS:
                    self.assertIn(field, agent[aspect], f"{agent['agent_id']}.{aspect} missing {field}")
                self.assertIn(agent[aspect]["write_policy"], WRITE_POLICIES, f"{agent['agent_id']}.{aspect}")

    def test_detection_only_agents_never_claim_managed_write(self) -> None:
        for agent in self.matrix["agents"]:
            if agent["official_capability"] == "detection_only":
                for aspect in ("configuration", "rollback"):
                    self.assertEqual(
                        agent[aspect]["write_policy"],
                        "unsupported",
                        f"{agent['agent_id']}.{aspect} must be unsupported in detection_only tier",
                    )

    def test_route_agents_claim_user_selected_managed_write(self) -> None:
        for agent in self.matrix["agents"]:
            if agent["official_capability"] == "route":
                self.assertEqual(
                    agent["configuration"]["write_policy"],
                    "user_selected_managed",
                    f"{agent['agent_id']}.configuration",
                )
                self.assertEqual(
                    agent["rollback"]["write_policy"],
                    "user_selected_managed",
                    f"{agent['agent_id']}.rollback",
                )

    def test_ui_status_uses_only_frozen_10_words(self) -> None:
        frozen = set(self.matrix["status_vocabulary"]["frozen_ui_status_words_10"])
        self.assertEqual(len(frozen), 10)
        for agent in self.matrix["agents"]:
            for aspect in ASPECT_NAMES:
                self.assertIn(agent[aspect]["ui_status"], frozen, f"{agent['agent_id']}.{aspect}")

    def test_asset_list_words_are_exact_4(self) -> None:
        self.assertEqual(
            set(self.matrix["status_vocabulary"]["frozen_asset_list_words_4"]),
            {"已安装", "未安装", "已配置", "未配置"},
        )

    def test_internal_codes_registered_as_technical_only(self) -> None:
        codes = set(self.matrix["status_vocabulary"]["technical_only_internal_codes"])
        for internal in ("detected", "configured", "connected", "verified", "healthy", "active", "supported"):
            self.assertIn(internal, codes)
        for frozen_word in self.matrix["status_vocabulary"]["frozen_ui_status_words_10"]:
            self.assertNotIn(frozen_word, codes, "frozen UI words must not be internal codes")

    def test_read_only_controls_are_always_read_only(self) -> None:
        controls = {c["control_id"]: c for c in self.matrix["read_only_controls"]}
        self.assertEqual(set(controls), {"omp", "cc-switch"})
        for control_id, control in controls.items():
            self.assertEqual(control["write_policy"], "always_read_only", control_id)
            self.assertEqual(control["ui_status"], "工具能力已接入", control_id)

    def test_omp_and_cc_switch_not_masquerading_as_agents(self) -> None:
        matrix_ids = {agent["agent_id"] for agent in self.matrix["agents"]}
        self.assertNotIn("omp", matrix_ids)
        self.assertNotIn("cc-switch", matrix_ids)

    def test_vscode_copilot_chat_requires_extension_evidence(self) -> None:
        agent = next(a for a in self.matrix["agents"] if a["agent_id"] == "vscode-copilot-chat")
        discovery = agent["discovery"]
        self.assertIn("extension", discovery["official_source"].lower())
        self.assertIn("extension", discovery["config_path"].lower())
        self.assertIn("extension evidence", agent["current_code_caveat"])
        # Shared settings.json alone must be insufficient.
        self.assertIn("不足", agent["current_code_caveat"])
        self.assertEqual(agent["official_capability"], "detection_only")

    def test_cortex_connections_toml_is_metadata_fingerprint_only(self) -> None:
        agent = next(a for a in self.matrix["agents"] if a["agent_id"] == "cortex-code")
        joined = " ".join(
            [agent["current_code_caveat"], agent["discovery"]["config_path"], agent["discovery"]["field_ownership"]]
        )
        self.assertIn("connections.toml", joined)
        self.assertIn("metadata fingerprint", joined)
        self.assertIn("不读", joined)

    def test_every_agent_has_provenance_and_platforms(self) -> None:
        for agent in self.matrix["agents"]:
            self.assertIn("provenance", agent, agent["agent_id"])
            self.assertTrue(agent["platforms"], agent["agent_id"])
            for platform in agent["platforms"]:
                self.assertIn(platform, {"macos", "windows", "linux"})

    def test_caveat_never_masquerades_code_path_as_verified(self) -> None:
        # Agents outside the approved 11 Route set must carry an explicit caveat
        # naming the release-tier mismatch rather than claiming production routing.
        route_agents = {a["agent_id"] for a in self.matrix["agents"] if a["official_capability"] == "route"}
        for agent in self.matrix["agents"]:
            if agent["agent_id"] in route_agents:
                self.assertIn("无差异", agent["current_code_caveat"], agent["agent_id"])
            else:
                self.assertIn("detection_only", agent["current_code_caveat"], agent["agent_id"])

    def test_hermes_openhands_pi_mistral_vibe_route_caveats(self) -> None:
        for agent_id in ("hermes", "openhands", "pi", "mistral-vibe"):
            agent = next(a for a in self.matrix["agents"] if a["agent_id"] == agent_id)
            self.assertEqual(agent["official_capability"], "detection_only", agent_id)
            self.assertEqual(agent["declared_integration_kind"], "route", agent_id)
            self.assertIn("不在批准 11 verified Route 集", agent["current_code_caveat"], agent_id)


class CrossArtifactContractTests(unittest.TestCase):
    """CF-3 cross-artifact fail-closed contracts (envelope <-> matrix binding,
    RFC3339 timestamp ordering).

    JSON Schema alone CANNOT validate these (a schema cannot import the matrix
    fixture or compare two timestamp fields), so the frozen consumption rules —
    declared in the schema description, the matrix envelope_binding_rule, and
    the CF-3 report — are exercised here via executable helpers that consumers
    (PR-7/PR-9/C8C/N5/N9) must run and block on. This is explicitly a
    cross-artifact contract test, NOT a pure JSON Schema validation claim.
    """

    @classmethod
    def setUpClass(cls) -> None:
        _, cls.matrix = _load()

    def test_codex_route_binding_passes(self) -> None:
        # codex is a registered route agent: envelope must pass the binding.
        envelope = _route_envelope()  # agent_id=codex, capability_tier=route
        self.assertEqual(envelope_matrix_binding_errors(envelope, self.matrix), [])
        self.assertEqual(list(Draft202012Validator(_load()[0]).iter_errors(envelope)), [])

    def test_hermes_route_binding_rejected(self) -> None:
        # hermes is registered with official_capability=detection_only; a route
        # tier on its envelope must be blocked by the consumer helper.
        envelope = _route_envelope()
        envelope["agent_id"] = "hermes"
        errors = envelope_matrix_binding_errors(envelope, self.matrix)
        self.assertTrue(errors)
        self.assertIn("hermes", errors[0])
        self.assertIn("detection_only", errors[0])

    def test_unknown_route_binding_rejected(self) -> None:
        # An agent_id absent from the matrix must never declare capability_tier=route.
        envelope = _route_envelope()
        envelope["agent_id"] = "not-a-registered-agent"
        errors = envelope_matrix_binding_errors(envelope, self.matrix)
        self.assertTrue(errors)
        self.assertIn("not-a-registered-agent", errors[0])
        self.assertIn("route", errors[0])

    def test_timestamp_finished_before_started_rejected(self) -> None:
        envelope = _base_envelope()
        envelope["started_at"] = "2026-08-12T02:00:00Z"
        envelope["finished_at"] = "2026-08-12T01:00:00Z"
        errors = timestamp_order_errors(envelope)
        self.assertTrue(errors)
        self.assertIn("finished_at", errors[0])

    def test_timestamp_legal_order_passes(self) -> None:
        self.assertEqual(timestamp_order_errors(_base_envelope()), [])

    def test_timestamp_ordering_declared_as_aggregator_contract(self) -> None:
        # The schema/report freeze the rule as downstream-enforced; the schema
        # itself must NOT attempt a cross-field comparison (declared, not
        # implemented, in the schema).
        schema, _ = _load()
        self.assertIn("finished_at >= started_at", schema["description"])
        self.assertIn("RFC3339", schema["description"])


if __name__ == "__main__":
    unittest.main()
