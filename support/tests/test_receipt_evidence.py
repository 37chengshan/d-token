"""N1 receipt evidence producer contract tests (PR-1).

Exercises the frozen CandidateEvidenceEnvelope schema (CF-3) and the PR-1
receipt contract: Local/Provider/claimable separation, per-request cost
(费用/差额/币种/单位/价格来源/更新时间) complete only when token and price
evidence both exist, missing usage/price stays unavailable (never 0), and no
sensitive material anywhere in the artifact. No network, no dependency
install, no git operations.
"""

from __future__ import annotations

import json
import os
import tempfile
import unittest
from copy import deepcopy
from pathlib import Path
from unittest import mock

from jsonschema import Draft202012Validator

from support.scripts import run_receipt_evidence as producer

ROOT = Path(__file__).resolve().parents[1]
SCHEMA_PATH = ROOT / "schemas" / "candidate-evidence-envelope.schema.json"

SHA256 = "b" * 64
SHA1_40 = "a" * 40


def _load_schema() -> dict:
    return json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))


def _input_fixture() -> Path:
    """Writes the producer fixture receipts to a temp JSON file for collect
    mode tests; the temp file is cleaned up by the caller's context."""
    temporary = Path(tempfile.mkstemp(suffix=".json")[1])
    temporary.write_text(json.dumps(producer._fixture_receipts()), encoding="utf-8")
    return temporary


class ReceiptContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.validator = Draft202012Validator(_load_schema())
        self.fixture = producer._fixture_receipts()

    def test_fixture_receipts_validate(self) -> None:
        validated = producer.validate_receipts(self.fixture)
        self.assertEqual(len(validated), 2)

    def test_receipt_with_usage_and_price_has_complete_cost(self) -> None:
        receipt = producer.validate_receipts(self.fixture)[0]
        self.assertIsNotNone(receipt["request_cost"])
        cost = receipt["request_cost"]
        self.assertEqual(cost["original_cost_micros"], 2400)
        self.assertEqual(cost["optimized_cost_micros"], 2100)
        self.assertEqual(cost["cost_delta_micros"], 300)
        self.assertEqual(cost["currency"], "USD")
        self.assertEqual(cost["unit"], "per_1k_tokens")
        self.assertEqual(cost["price_source"], "settings")
        self.assertEqual(cost["price_updated_at_ms"], 1_700_000_000_000)
        # Usage alone never elevates claimable.
        self.assertIsNone(receipt["claimable_tokens_avoided"])

    def test_receipt_without_usage_or_price_stays_unavailable_never_zero(self) -> None:
        receipt = producer.validate_receipts(self.fixture)[1]
        self.assertIsNone(receipt["provider_input_tokens"])
        self.assertIsNone(receipt["provider_output_tokens"])
        self.assertIsNone(receipt["request_cost"])
        self.assertIsNone(receipt["claimable_tokens_avoided"])

    def test_partial_cost_is_rejected(self) -> None:
        receipt = deepcopy(self.fixture[0])
        receipt["request_cost"].pop("currency")
        with self.assertRaises(producer.ContractError):
            producer.validate_receipt(receipt, "receipts[0]")

    def test_zero_cost_with_complete_evidence_is_allowed(self) -> None:
        # A mathematically zero cost is legal when the token and price
        # evidence is complete (e.g. a free request); it is not forbidden
        # without justification.
        receipt = deepcopy(self.fixture[0])
        receipt["request_cost"]["original_cost_micros"] = 0
        receipt["request_cost"]["optimized_cost_micros"] = 0
        receipt["request_cost"]["cost_delta_micros"] = 0
        validated = producer.validate_receipt(receipt, "receipts[0]")
        self.assertEqual(validated["request_cost"]["original_cost_micros"], 0)

    def test_fabricated_zero_cost_without_token_evidence_is_rejected(self) -> None:
        # A receipt may never claim a cost (including a fabricated 0) when
        # there is no token usage evidence: missing usage must stay null.
        receipt = deepcopy(self.fixture[1])
        receipt["request_cost"] = {
            "original_cost_micros": 0,
            "optimized_cost_micros": 0,
            "cost_delta_micros": 0,
            "currency": "USD",
            "unit": "per_1k_tokens",
            "price_source": "settings",
            "price_updated_at_ms": 1_700_000_000_000,
        }
        with self.assertRaises(producer.ContractError):
            producer.validate_receipt(receipt, "receipts[0]")

    def test_claimable_must_be_positive_or_absent(self) -> None:
        receipt = deepcopy(self.fixture[0])
        receipt["claimable_tokens_avoided"] = 0
        with self.assertRaises(producer.ContractError):
            producer.validate_receipt(receipt, "receipts[0]")

    def test_missing_required_field_is_rejected(self) -> None:
        receipt = deepcopy(self.fixture[0])
        del receipt["transformations"]
        with self.assertRaises(producer.ContractError):
            producer.validate_receipt(receipt, "receipts[0]")

    def test_sensitive_material_is_rejected(self) -> None:
        receipt = deepcopy(self.fixture[0])
        receipt["skip_or_failure_reason"] = "Bearer abcdef0123456789"
        with self.assertRaises(producer.ContractError):
            producer.validate_receipt(receipt, "receipts[0]")

    def test_credential_shaped_key_is_rejected(self) -> None:
        receipt = deepcopy(self.fixture[0])
        receipt["api_key"] = "sk-test"
        with self.assertRaises(producer.ContractError):
            producer.validate_receipt(receipt, "receipts[0]")

    def test_absolute_path_is_rejected(self) -> None:
        receipt = deepcopy(self.fixture[0])
        receipt["skip_or_failure_reason"] = "config at /Users/cc/.dtoken/config.json"
        with self.assertRaises(producer.ContractError):
            producer.validate_receipt(receipt, "receipts[0]")

    def test_absolute_path_variants_are_rejected(self) -> None:
        for path in ("path /home/cc/.config", r"path C:\Users\cc\config", "path /private/etc/passwd"):
            receipt = deepcopy(self.fixture[0])
            receipt["skip_or_failure_reason"] = path
            with self.assertRaises(producer.ContractError):
                producer.validate_receipt(receipt, "receipts[0]")


class EnvelopeContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.validator = Draft202012Validator(_load_schema())

    def _build(self) -> dict:
        return producer.build_envelope(
            producer._fixture_receipts(),
            candidate_commit=SHA1_40,
            source_digest=SHA256,
            registry_digest=SHA256,
        )

    def test_envelope_is_schema_valid(self) -> None:
        envelope = self._build()
        errors = sorted(self.validator.iter_errors(envelope), key=lambda error: error.path)
        self.assertEqual(errors, [])

    def test_envelope_marks_n1_generic_with_privacy_assertions(self) -> None:
        envelope = self._build()
        self.assertEqual(envelope["node_id"], "N1")
        self.assertEqual(envelope["schema_version"], "1.0.0")
        self.assertEqual(envelope["evidence_kind"], "generic")
        self.assertEqual(envelope["status"], "pass")
        self.assertTrue(all(envelope["privacy_assertions"].values()))

    def test_envelope_artifact_sha256_commits_to_receipts(self) -> None:
        envelope = self._build()
        self.assertRegex(envelope["artifact_sha256"], r"^[0-9a-f]{64}$")
        # CF-3: the envelope never embeds receipts (additionalProperties=false);
        # artifact_sha256 binds the companion receipts payload instead.
        self.assertNotIn("receipts", envelope)
        payload = producer.build_receipt_payload(producer._fixture_receipts())
        self.assertEqual(
            envelope["artifact_sha256"], producer.sha256_bytes(producer.canonical_json_bytes(payload))
        )

    def test_envelope_contains_no_sensitive_material(self) -> None:
        envelope = self._build()
        producer._assert_no_sensitive_material(envelope, "envelope")
        encoded = json.dumps(envelope)
        for forbidden in ["Bearer ", "sk-", "api_key", "password", "/Users/", "/home/"]:
            self.assertNotIn(forbidden.lower(), encoded.lower())


class ProducerCliContractTests(unittest.TestCase):
    def test_dry_run_fixture_mode_succeeds(self) -> None:
        self.assertEqual(producer.main(["--mode", "fixture", "--dry-run"]), 0)

    def test_dry_run_rejects_official_final_candidate_output(self) -> None:
        # The CLI contract is: an attempt to write into the official
        # final-candidate directory is an error, caught by main and surfaced
        # as a non-zero exit code (never an unhandled exception), and the
        # official artifact is not created.
        target = producer.FINAL_CANDIDATE_DIR / SHA1_40 / f"N1-receipt-{SHA1_40}.json"
        self.assertEqual(producer.main(["--mode", "fixture", "--output", str(target)]), 1)
        self.assertFalse(target.exists())

    def test_collect_dry_run_writes_to_explicit_temp_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "N1-receipt.json"
            self.assertEqual(
                producer.main(
                    ["--mode", "collect", "--dry-run", "--output", str(out), "--input", str(_input_fixture())]
                ),
                0,
            )
            self.assertTrue(out.is_file())
            envelope = json.loads(out.read_text(encoding="utf-8"))
            # CF-3: the envelope never embeds receipts; its digest binds the
            # companion payload.
            self.assertNotIn("receipts", envelope)
            self.assertRegex(envelope["artifact_sha256"], r"^[0-9a-f]{64}$")

    def test_collect_non_dry_run_writes_official_with_non_placeholder_digests(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            official_root = Path(tmp) / "final-candidate"
            with mock.patch.object(producer, "FINAL_CANDIDATE_DIR", official_root):
                code = producer.main(
                    [
                        "--mode",
                        "collect",
                        "--candidate-commit",
                        SHA1_40,
                        "--source-digest",
                        SHA256,
                        "--registry-digest",
                        SHA256,
                        "--input",
                        str(_input_fixture()),
                    ]
                )
            self.assertEqual(code, 0)
            target = official_root / SHA1_40 / f"N1-receipt-{SHA1_40}.json"
            companion = official_root / SHA1_40 / f"N1-receipt-{SHA1_40}.companion.json"
            self.assertTrue(target.is_file())
            self.assertTrue(companion.is_file())
            envelope = json.loads(target.read_text(encoding="utf-8"))
            payload = json.loads(companion.read_text(encoding="utf-8"))
            self.assertEqual(
                envelope["artifact_sha256"], producer.sha256_bytes(producer.canonical_json_bytes(payload))
            )
            self.assertEqual(len(payload["receipts"]), 2)
            self.assertNotIn("receipts", envelope)

    def test_collect_non_dry_run_requires_non_placeholder_digests(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            official_root = Path(tmp) / "final-candidate"
            with mock.patch.object(producer, "FINAL_CANDIDATE_DIR", official_root):
                code = producer.main(
                    ["--mode", "collect", "--candidate-commit", "0" * 40, "--input", str(_input_fixture())]
                )
            self.assertEqual(code, 1)
            self.assertEqual(list(official_root.rglob("*.json")), [])

    def test_collect_non_dry_run_writes_0600(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            official_root = Path(tmp) / "final-candidate"
            with mock.patch.object(producer, "FINAL_CANDIDATE_DIR", official_root):
                code = producer.main(
                    [
                        "--mode",
                        "collect",
                        "--candidate-commit",
                        SHA1_40,
                        "--source-digest",
                        SHA256,
                        "--registry-digest",
                        SHA256,
                        "--input",
                        str(_input_fixture()),
                    ]
                )
            self.assertEqual(code, 0)
            for path in official_root.rglob("*.json"):
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)


if __name__ == "__main__":
    unittest.main()
