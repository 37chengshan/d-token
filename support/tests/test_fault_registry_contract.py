"""Contract test for the tracked fault registry (PR-C6B).

The registry is the single machine-readable index of durability crash
points: every id must be wired to a `maybe_crash("...")` literal in the
owner crate's source, otherwise the fault matrix cannot claim coverage of
that point.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
REGISTRY = REPO_ROOT / "support" / "fixtures" / "fault-registry-v1.json"
CRATES = REPO_ROOT / "code" / "crates"

TERMINAL_STATE_VOCABULARY = {
    "acked_records_survive_unacked_absent_or_complete",
    "previous_projection_intact_never_partial",
    "operation_aborts_before_ack",
}


class FaultRegistryContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = json.loads(REGISTRY.read_text(encoding="utf-8"))

    def test_registry_schema_is_complete(self) -> None:
        self.assertEqual(self.registry["schema_version"], 1)
        self.assertEqual(self.registry["id"], "fault-registry-v1")
        failpoints = self.registry["failpoints"]
        self.assertGreaterEqual(len(failpoints), 3, "fault matrix needs real crash points")

        required = {
            "id",
            "owner",
            "crash_point",
            "durability_mode",
            "terminal_state",
            "test",
            "test_kind",
        }
        ids = set()
        for failpoint in failpoints:
            self.assertEqual(
                set(failpoint), required, f"failpoint {failpoint.get('id')} missing fields"
            )
            self.assertTrue(failpoint["id"].startswith(("jsonl:", "context_store:")))
            self.assertNotIn(failpoint["id"], ids, "failpoint ids must be unique")
            ids.add(failpoint["id"])
            self.assertIn(
                failpoint["terminal_state"],
                TERMINAL_STATE_VOCABULARY,
                f"terminal state for {failpoint['id']} must use the vocabulary",
            )
        self.assertEqual(len(ids), len(failpoints))

    def test_every_registry_id_is_wired_in_owner_source(self) -> None:
        failpoints = self.registry["failpoints"]
        for failpoint in failpoints:
            owner_dir = CRATES / failpoint["owner"]
            self.assertTrue(owner_dir.is_dir(), f"owner crate {failpoint['owner']} missing")
            sources = list(owner_dir.rglob("*.rs"))
            literal = f'"{failpoint["id"]}"'
            self.assertTrue(
                any(literal in source.read_text(encoding="utf-8") for source in sources),
                f"failpoint {failpoint['id']} is not wired as a string literal in "
                f"{failpoint['owner']}",
            )

    def test_every_registry_test_name_exists_in_owner_tests(self) -> None:
        for failpoint in self.registry["failpoints"]:
            owner_dir = CRATES / failpoint["owner"]
            tests = [path.read_text(encoding="utf-8") for path in owner_dir.rglob("tests/*.rs")]
            self.assertTrue(
                any(f"fn {failpoint['test']}" in source for source in tests),
                f"test {failpoint['test']} for {failpoint['id']} must exist in "
                f"{failpoint['owner']}",
            )

    def test_crash_points_are_feature_gated_in_production(self) -> None:
        # The failpoint call sites must only exist under the `failpoints`
        # feature; a release build carries no injection points.
        for crate in ("dtoken-event-ledger", "dtoken-context-store"):
            cargo = (CRATES / crate / "Cargo.toml").read_text(encoding="utf-8")
            self.assertIn("failpoints", cargo, f"{crate} must declare the failpoints feature")
            lib = (CRATES / crate / "src" / "lib.rs").read_text(encoding="utf-8")
            self.assertIn("cfg(feature = \"failpoints\")", lib)


if __name__ == "__main__":
    unittest.main()
