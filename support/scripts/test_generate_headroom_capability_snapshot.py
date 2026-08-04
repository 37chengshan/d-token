#!/usr/bin/env python3
"""Contract tests for native registry to release audit snapshot conversion."""

from __future__ import annotations

import copy
import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from generate_headroom_capability_snapshot import (
    ContractError,
    REQUIRED_CAPABILITY_IDS,
    generate_snapshot,
    main,
)


CANDIDATE_ID = "a" * 64


def native_capability(capability_id: str, *, state: str = "ready") -> dict[str, object]:
    verified = state in {"ready", "degraded", "implemented_disabled"}
    return {
        "id": capability_id,
        "state": state,
        "implementation_revision": f"impl-{capability_id}",
        "config_revision": None,
        "evidence_revision": f"native-proof-{capability_id}" if verified else None,
        "freshness": "recent" if verified else "unknown",
        "reason_codes": ["policy_shadow_only"]
        if capability_id == "optical_active_transform"
        else ["native_runtime_evidence"],
        "last_verified_at_ms": 1_760_000_000_000 if verified else None,
    }


def native_registry() -> dict[str, object]:
    capabilities = [native_capability(capability_id) for capability_id in REQUIRED_CAPABILITY_IDS]
    optical = next(item for item in capabilities if item["id"] == "optical_active_transform")
    optical["state"] = "implemented_disabled"
    capabilities.append(native_capability("primitive-extra"))
    return {
        "schema_version": 1,
        "revision": "headroom-backend-capabilities-v2",
        "collected_at_ms": 1_760_000_000_000,
        "capabilities": capabilities,
    }


class GenerateHeadroomCapabilitySnapshotTests(unittest.TestCase):
    def test_copies_native_truth_and_binds_candidate_revision(self) -> None:
        registry = native_registry()
        snapshot = generate_snapshot(registry, CANDIDATE_ID)

        self.assertEqual(snapshot["source"], "dtoken-owned-runtime")
        self.assertEqual(snapshot["revision"], CANDIDATE_ID)
        self.assertEqual(snapshot["source_registry_revision"], registry["revision"])
        self.assertEqual(snapshot["collected_at"], "2025-10-09T08:53:20.000Z")
        self.assertEqual(
            [item["id"] for item in snapshot["capabilities"]], list(REQUIRED_CAPABILITY_IDS)
        )

        source_by_id = {item["id"]: item for item in registry["capabilities"]}
        for item in snapshot["capabilities"]:
            source = source_by_id[item["id"]]
            self.assertEqual(item["state"], source["state"])
            self.assertEqual(item["freshness"], source["freshness"])
            self.assertEqual(item["reason_codes"], source["reason_codes"])
            self.assertEqual(item["implementation_revision"], source["implementation_revision"])
            self.assertEqual(item["source_evidence_revision"], source["evidence_revision"])
            self.assertEqual(item["evidence_revision"], CANDIDATE_ID)

        self.assertNotIn("primitive-extra", [item["id"] for item in snapshot["capabilities"]])

    def test_missing_aggregate_capability_fails(self) -> None:
        registry = native_registry()
        registry["capabilities"] = [
            item for item in registry["capabilities"] if item["id"] != "compression"
        ]
        with self.assertRaisesRegex(ContractError, "missing aggregate capability ids: compression"):
            generate_snapshot(registry, CANDIDATE_ID)

    def test_duplicate_capability_fails(self) -> None:
        registry = native_registry()
        registry["capabilities"].append(copy.deepcopy(registry["capabilities"][0]))
        with self.assertRaisesRegex(ContractError, "duplicate capability id"):
            generate_snapshot(registry, CANDIDATE_ID)

    def test_unknown_state_or_freshness_fails(self) -> None:
        for field, value in (("state", "complete"), ("freshness", "fresh")):
            with self.subTest(field=field):
                registry = native_registry()
                registry["capabilities"][0][field] = value
                with self.assertRaisesRegex(ContractError, f"{field} is unknown"):
                    generate_snapshot(registry, CANDIDATE_ID)

    def test_verified_state_requires_native_evidence(self) -> None:
        registry = native_registry()
        registry["capabilities"][0]["evidence_revision"] = None
        with self.assertRaisesRegex(ContractError, "evidence_revision is required"):
            generate_snapshot(registry, CANDIDATE_ID)

    def test_unknown_fields_and_invalid_candidate_fail(self) -> None:
        registry = native_registry()
        registry["unexpected"] = True
        with self.assertRaisesRegex(ContractError, "unknown fields"):
            generate_snapshot(registry, CANDIDATE_ID)

        with self.assertRaisesRegex(ContractError, "64 lowercase hexadecimal"):
            generate_snapshot(native_registry(), "A" * 64)

    def test_cli_writes_candidate_bound_wrapper(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            registry_path = root / "registry.json"
            snapshot_path = root / "snapshot.json"
            registry_path.write_text(json.dumps(native_registry()), encoding="utf-8")
            stdout = io.StringIO()
            with redirect_stdout(stdout):
                exit_code = main(
                    [
                        "--registry",
                        str(registry_path),
                        "--candidate-id",
                        CANDIDATE_ID,
                        "--output",
                        str(snapshot_path),
                    ]
                )
            self.assertEqual(exit_code, 0)
            self.assertTrue(json.loads(stdout.getvalue())["ok"])
            self.assertEqual(json.loads(snapshot_path.read_text(encoding="utf-8"))["revision"], CANDIDATE_ID)


if __name__ == "__main__":
    unittest.main()
