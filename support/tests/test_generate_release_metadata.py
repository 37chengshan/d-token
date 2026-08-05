from __future__ import annotations

import importlib.util
import os
import subprocess
import unittest
from pathlib import Path
from unittest import mock


SCRIPT_PATH = (
    Path(__file__).resolve().parents[1] / "scripts" / "generate_release_metadata.py"
)
MODULE_SPEC = importlib.util.spec_from_file_location(
    "generate_release_metadata", SCRIPT_PATH
)
assert MODULE_SPEC is not None and MODULE_SPEC.loader is not None
RELEASE = importlib.util.module_from_spec(MODULE_SPEC)
MODULE_SPEC.loader.exec_module(RELEASE)


class SourceRevisionCommitBoundTests(unittest.TestCase):
    def test_explicit_revision_wins(self) -> None:
        self.assertEqual(RELEASE.resolve_source_revision("abc123"), "abc123")

    def test_ci_sha_wins_without_git(self) -> None:
        with mock.patch.dict(os.environ, {"GITHUB_SHA": "c" * 40}):
            self.assertEqual(RELEASE.resolve_source_revision(None), "c" * 40)

    def test_default_resolves_to_checked_out_commit(self) -> None:
        head = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=RELEASE.ROOT, text=True
        ).strip()
        self.assertEqual(len(head), 40)
        with mock.patch.dict(os.environ):
            os.environ.pop("GITHUB_SHA", None)
            revision = RELEASE.resolve_source_revision(None)
        self.assertEqual(revision, head)
        self.assertNotEqual(revision, "local-uncommitted")
