"""Contract tests for the Agent x integration-method matrix producer (PR-11).

The matrix is *generated* from the authoritative Rust registries
(agent.rs KNOWN_AGENT_ADAPTERS, agent_transport.rs ADAPTERS, and the
cc-switch app_type mapping in upstream_discovery.rs) — never maintained as a
second catalog. These tests pin the pinned registry contract
(REQUIREMENTS.md AGENT-4):

  * 23 detection adapters in the backend registry;
  * 15 verified Route write surfaces (can_configure=can_clear=true);
  * 6 conditional MCP/Hook write surfaces (desktop-managed, bridge-verified);
  * 2 detection-only adapters (vscode-copilot-chat, antigravity);
  * OMP appears once as an explicit read-only observer;
  * cc-switch app types claude/codex/gemini/opencode/openclaw all map to a
    backend Agent adapter;
  * every Route row's can_configure/can_clear are true and no conditional or
    detection-only row ever claims route evidence.

The generator must fail closed on any drift. No real configs, no network.
"""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).parents[1] / "scripts" / "generate_agent_integration_matrix.py"
SPEC = importlib.util.spec_from_file_location("generate_agent_integration_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class AgentIntegrationMatrixContractTests(unittest.TestCase):
    def build(self) -> dict:
        return MODULE.build_matrix()

    def test_matrix_generates_the_pinned_registry_contract(self) -> None:
        matrix = self.build()
        self.assertTrue(matrix["generated"])
        counts = matrix["registry_counts"]
        self.assertEqual(counts["known_agent_adapters"], 23)
        self.assertEqual(counts["transport_adapters"], 23)
        self.assertEqual(counts["route_write"], 15)
        self.assertEqual(counts["conditional_write"], 6)
        self.assertEqual(counts["detection_only"], 2)
        self.assertEqual(counts["omp_read_only"], 1)
        self.assertEqual(len(matrix["rows"]), 24)

    def test_every_route_row_offers_a_verified_write_surface(self) -> None:
        for row in self.build()["rows"]:
            if row["method_kind"] != "Route":
                continue
            self.assertTrue(row["can_configure"], row["agent_id"])
            self.assertTrue(row["can_clear"], row["agent_id"])
            self.assertEqual(row["integration_kind"], "Route")
            self.assertTrue(row["provides_route_evidence"], row["agent_id"])
            self.assertTrue(row["modifies_model_provider"], row["agent_id"])

    def test_conditional_and_detection_rows_never_claim_route_evidence(self) -> None:
        for row in self.build()["rows"]:
            if row["method_kind"] == "Route":
                continue
            self.assertFalse(row["can_configure"], row["agent_id"])
            self.assertFalse(row["can_clear"], row["agent_id"])
            self.assertFalse(row["provides_route_evidence"], row["agent_id"])
            self.assertFalse(row["modifies_model_provider"], row["agent_id"])

    def test_detection_only_adapters_are_the_pinned_pair(self) -> None:
        detection = [
            row["agent_id"]
            for row in self.build()["rows"]
            if row["method_kind"] == "DetectionOnly"
        ]
        self.assertEqual(sorted(detection), ["antigravity", "vscode-copilot-chat"])

    def test_omp_is_generated_once_as_read_only_observer(self) -> None:
        omp = [row for row in self.build()["rows"] if row["agent_id"] == "omp"]
        self.assertEqual(len(omp), 1)
        self.assertEqual(omp[0]["method_kind"], "ReadOnly")
        self.assertFalse(omp[0]["can_configure"])
        self.assertFalse(omp[0]["can_clear"])

    def test_cc_switch_app_types_all_map_to_a_backend_adapter(self) -> None:
        matrix = self.build()
        self.assertEqual(
            matrix["cc_switch_app_types"],
            ["claude", "codex", "gemini", "openclaw", "opencode"],
        )
        row_by_id = {row["agent_id"]: row for row in matrix["rows"]}
        for app, expected_agent in {
            "claude": "claude-code",
            "codex": "codex",
            "gemini": "gemini-cli",
            "opencode": "opencode",
            "openclaw": "openclaw",
        }.items():
            mapped = [
                agent_id
                for agent_id, row in row_by_id.items()
                if app in (row.get("cc_switch_app_type") or [])
            ]
            self.assertEqual(len(mapped), 1, f"{app} maps to exactly one adapter")
            self.assertEqual(mapped[0], expected_agent, f"{app} backend mapping")

    def test_cli_writes_generated_matrix_and_returns_zero(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "matrix.json"
            code = MODULE.main(["--out", str(out)])
            self.assertEqual(code, 0)
            data = json.loads(out.read_text(encoding="utf-8"))
            self.assertTrue(data["generated"])
            self.assertEqual(len(data["rows"]), 24)

    def test_drift_fails_closed_without_writing(self) -> None:
        original = MODULE.EXPECTED_ROUTE_WRITE
        try:
            MODULE.EXPECTED_ROUTE_WRITE = 999
            with self.assertRaises(MODULE.RegistryParseError):
                self.build()
        finally:
            MODULE.EXPECTED_ROUTE_WRITE = original


if __name__ == "__main__":
    unittest.main()
