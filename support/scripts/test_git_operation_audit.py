from __future__ import annotations

import argparse
import importlib.util
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("git_operation_audit.py")
SPEC = importlib.util.spec_from_file_location("git_operation_audit", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class GitOperationAuditTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.pending_file = self.root / "pending-commits.json"
        self.operation_file = self.root / "git-operations.json"

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def plan_args(self, **overrides: object) -> argparse.Namespace:
        values: dict[str, object] = {
            "phase": "N6",
            "title": "Pending commit ledger",
            "summary": "Record work without claiming a Git commit",
            "commit_message": "chore(support): validate pending commit ledger",
            "files": ["support/pending-commits.json", "support/scripts/git_operation_audit.py"],
            "tests": ["python3 -m unittest support.scripts.test_git_operation_audit"],
            "dependencies": [],
            "notes": ["No Git operation is executed"],
            "shared_files": ["support/pending-commits.json"],
            "blockers": [],
            "staging_mode": "mixed",
            "verification_status": "passed",
            "branch": None,
            "head_before": None,
            "status": "ready",
            "id": "optical-n6-ledger",
            "pending_file": self.pending_file,
        }
        values.update(overrides)
        return argparse.Namespace(**values)

    def test_pending_commit_plan_lifecycle(self) -> None:
        with patch.object(
            MODULE,
            "git_value",
            side_effect=lambda *args: (
                "codex/prd-macos-polish" if "branch" in args else "abc123"
            ),
        ):
            with redirect_stdout(StringIO()):
                MODULE.plan_commit(self.plan_args())

        payload = json.loads(self.pending_file.read_text(encoding="utf-8"))
        entry = payload["pending_commits"][0]
        self.assertEqual(entry["status"], "ready")
        self.assertEqual(entry["branch"], "codex/prd-macos-polish")
        self.assertEqual(entry["staging_mode"], "mixed")
        self.assertEqual(entry["verification"]["status"], "passed")
        self.assertIsNone(entry["commit_hash"])
        self.assertEqual(MODULE.validate_pending_payload(payload), [])

        with redirect_stdout(StringIO()):
            MODULE.mark_plan(
                argparse.Namespace(
                    id="optical-n6-ledger",
                    status="committed",
                    commit_hash="abcdef123456",
                    note="commit recorded",
                    blocker=None,
                    clear_blockers=False,
                    verification_status="passed",
                    pending_file=self.pending_file,
                )
            )
        updated = json.loads(self.pending_file.read_text(encoding="utf-8"))["pending_commits"][0]
        self.assertEqual(updated["status"], "committed")
        self.assertTrue(updated["submitted"])
        self.assertIsNotNone(updated["submitted_at"])
        self.assertEqual(updated["commit_hash"], "abcdef123456")

    def test_duplicate_id_is_rejected_without_overwriting_valid_json(self) -> None:
        with patch.object(MODULE, "git_value", return_value="branch"):
            with redirect_stdout(StringIO()):
                MODULE.plan_commit(self.plan_args(status="pending", verification_status="not-run", shared_files=[] , staging_mode="whole-file"))
            before = self.pending_file.read_text(encoding="utf-8")
            with self.assertRaises(SystemExit):
                with redirect_stdout(StringIO()):
                    MODULE.plan_commit(self.plan_args(status="pending", verification_status="not-run", shared_files=[], staging_mode="whole-file"))
        self.assertEqual(self.pending_file.read_text(encoding="utf-8"), before)
        json.loads(before)

    def test_validation_rejects_ready_blockers_and_missing_dependency(self) -> None:
        entry = {
            "id": "n1",
            "branch": "branch",
            "phase": "N1",
            "status": "ready",
            "title": "title",
            "summary": "summary",
            "commit_message": "test: commit",
            "files": ["a"],
            "dependencies": ["missing"],
            "notes": [],
            "staging_mode": "whole-file",
            "shared_files": [],
            "verification": {"status": "passed", "checks": []},
            "blockers": ["blocked"],
            "commit_hash": None,
        }
        errors = MODULE.validate_pending_payload({"schema_version": 1, "pending_commits": [entry]})
        self.assertTrue(any("ready plans cannot have blockers" in error for error in errors))
        self.assertTrue(any("unknown dependency" in error for error in errors))

    def test_blocked_status_requires_a_concrete_blocker(self) -> None:
        entry = {
            "id": "n1",
            "branch": "branch",
            "phase": "N1",
            "status": "blocked",
            "title": "title",
            "summary": "summary",
            "commit_message": "test: commit",
            "files": ["a"],
            "tests": [],
            "dependencies": [],
            "notes": [],
            "staging_mode": "whole-file",
            "shared_files": [],
            "verification": {"status": "blocked", "checks": []},
            "blockers": [],
            "submitted": False,
            "submitted_at": None,
            "commit_hash": None,
        }
        self.assertTrue(
            any(
                "blocked plans require at least one blocker" in error
                for error in MODULE.validate_pending_entry(entry)
            )
        )

    def test_committed_status_requires_hash(self) -> None:
        entry = {
            "id": "n1",
            "branch": "branch",
            "phase": "N1",
            "status": "committed",
            "title": "title",
            "summary": "summary",
            "commit_message": "test: commit",
            "files": ["a"],
            "dependencies": [],
            "notes": [],
            "staging_mode": "whole-file",
            "shared_files": [],
            "verification": {"status": "passed", "checks": []},
            "blockers": [],
            "commit_hash": None,
        }
        self.assertTrue(
            any("committed plans require" in error for error in MODULE.validate_pending_entry(entry))
        )

    def test_record_writes_a_valid_real_operation(self) -> None:
        with patch.object(MODULE, "git_value", side_effect=lambda *args: "codex/prd-macos-polish" if "branch" in args else "abc123"):
            with redirect_stdout(StringIO()):
                MODULE.record(
                    argparse.Namespace(
                        operation="commit",
                        summary="Record the operation before committing",
                        branch=None,
                        head_before=None,
                        remote=None,
                        file=self.operation_file,
                    )
                )
        payload = json.loads(self.operation_file.read_text(encoding="utf-8"))
        self.assertEqual(MODULE.validate_operation_payload(payload), [])
        self.assertEqual(payload["operations"][0]["branch"], "codex/prd-macos-polish")
        self.assertEqual(payload["operations"][0]["head_before"], "abc123")

    def test_operation_validation_rejects_invalid_timestamp_and_multiline_summary(self) -> None:
        entry = {
            "id": "bad",
            "timestamp": "not-a-time",
            "operation": "commit",
            "branch": "branch",
            "summary": "first\nsecond",
            "head_before": None,
            "remote": None,
        }
        errors = MODULE.validate_operation_entry(entry)
        self.assertTrue(any("timestamp must be ISO 8601" in error for error in errors))
        self.assertTrue(any("summary must be one non-empty line" in error for error in errors))

    def test_record_rejects_multiline_explicit_branch_without_writing(self) -> None:
        with self.assertRaises(SystemExit):
            with redirect_stdout(StringIO()):
                MODULE.record(
                    argparse.Namespace(
                        operation="commit",
                        summary="Valid summary",
                        branch="first\nsecond",
                        head_before=None,
                        remote=None,
                        file=self.operation_file,
                    )
                )
        self.assertFalse(self.operation_file.exists())

    def test_operation_and_pending_logs_remain_separate(self) -> None:
        self.assertEqual(MODULE.load_log(self.operation_file), {"schema_version": 1, "operations": []})
        self.assertEqual(
            MODULE.load_pending_log(self.pending_file),
            {"schema_version": MODULE.PENDING_SCHEMA_VERSION, "pending_commits": []},
        )


if __name__ == "__main__":
    unittest.main()
