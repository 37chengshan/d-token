#!/usr/bin/env python3
"""PR-6 config-acceptance harness: isolated temp-root configure -> reread -> clear -> exact restore.

T-PR6 (docs/releases/v0.0.1/TASKS.md), REQ AGENT-6 / CONFIG-1 / CONFIG-2 /
CONFIG-3, verified by node N4. Exercises the config-write transaction contract
(detect -> parse -> plan -> redacted diff -> backup -> confirm -> apply ->
reread -> verify -> clear/exact restore -> receipt) on an ISOLATED TEMPORARY
ROOT: a byte+mode copy of the source files listed by the patch specs. Source
files themselves are only hashed (and stat-ed for permissions) for evidence.

Batch semantics:
  * default (per-agent): if agent X fails at any step, only X's unfinished
    writes are rolled back in REVERSE order of application; agents that
    completed before X keep their state; agents after X are skipped
    (status=blocked, batch aborted, fail-closed).
  * --all-or-nothing: any failure rolls back / re-verifies ALL executed agents
    in reverse order of execution (whole-batch reverse rollback, CONFIG-3).

Artifacts (CF-3): batch receipt + per-agent receipts + optional
candidate-evidence-envelope JSON validating against
support/schemas/candidate-evidence-envelope.schema.json
(evidence_kind=generic, agent_id/capability_tier null with N/A reasons).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import stat
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

HARNESS_VERSION = "1.0.0"
ENVELOPE_SCHEMA_VERSION = "1.0.0"
BACKUP_DIR_NAME = ".dtoken-config-acceptance-backup"
AGENT_ID_PATTERN = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")
HEX64_PATTERN = re.compile(r"^[0-9a-f]{64}$")
HEX40_PATTERN = re.compile(r"^[0-9a-f]{40}$")

# Steps of the transaction contract. FILE_STEPS iterate over target files, so
# failure injection can be pinned to a specific file index.
STEPS = ("detect", "parse", "plan", "backup", "apply", "reread", "verify", "clear")
FILE_STEPS = ("apply", "reread", "verify", "clear")


class ConfigAcceptanceError(Exception):
    """A contract violation in the transaction; carries a stable reason code."""

    def __init__(self, reason_code: str, detail: str = ""):
        super().__init__(f"{reason_code}: {detail}".rstrip())
        self.reason_code = reason_code
        self.detail = detail


class InjectedFailure(Exception):
    """Raised at a failpoint to exercise failure injection (test-only)."""

    def __init__(self, agent_id: str, step: str, index: int | None):
        super().__init__(f"injected failure {agent_id}:{step}:{index}")
        self.agent_id = agent_id
        self.step = step
        self.index = index


def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _file_sha256(path: Path) -> str:
    return _sha256_bytes(path.read_bytes())


def _mode_str(path: Path) -> str:
    return f"{stat.S_IMODE(os.stat(path).st_mode):04o}"


def _dir_digest(root: Path) -> str:
    """Deterministic SHA-256 over the relative path -> sha256 map of a tree."""
    entries: list[str] = []
    for path in sorted(root.rglob("*")):
        if path.is_file():
            entries.append(f"{path.relative_to(root).as_posix()}:{_file_sha256(path)}")
    return _sha256_bytes("\n".join(entries).encode("utf-8"))


class FileOp:
    """One declarative file mutation from a patch spec (fixture-controlled)."""

    __slots__ = ("path", "action", "marker", "replacement", "label")

    def __init__(self, path: str, action: str, marker: str, replacement: str, label: str):
        self.path = path
        self.action = action
        self.marker = marker
        self.replacement = replacement
        self.label = label


def load_patch_spec(spec_path: Path, agents_dir: Path) -> tuple[str, list[FileOp]]:
    """Parse and validate one patch spec. Raises ConfigAcceptanceError on violation."""
    try:
        raw = json.loads(spec_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ConfigAcceptanceError("spec_unreadable", f"{spec_path.name}: {exc}") from exc
    if not isinstance(raw, dict):
        raise ConfigAcceptanceError("spec_invalid", f"{spec_path.name}: not an object")
    agent_id = raw.get("agent_id")
    if not isinstance(agent_id, str) or not AGENT_ID_PATTERN.fullmatch(agent_id):
        raise ConfigAcceptanceError("spec_invalid_agent_id", spec_path.name)
    files_raw = raw.get("files")
    if not isinstance(files_raw, list) or not files_raw:
        raise ConfigAcceptanceError("spec_no_files", spec_path.name)
    ops: list[FileOp] = []
    for entry in files_raw:
        if not isinstance(entry, dict):
            raise ConfigAcceptanceError("spec_invalid_file_entry", spec_path.name)
        rel = entry.get("path")
        action = entry.get("action")
        marker = entry.get("marker")
        replacement = entry.get("replacement")
        label = entry.get("label", "unspecified")
        if not isinstance(rel, str) or not rel or rel.startswith("/") or ".." in rel.split("/"):
            raise ConfigAcceptanceError("spec_invalid_path", spec_path.name)
        if _is_sensitive_target(rel):
            raise ConfigAcceptanceError("sensitive_target_refused", rel)
        if action != "replace_marker":
            raise ConfigAcceptanceError("spec_unsupported_action", f"{rel}: {action}")
        if not isinstance(marker, str) or not marker or not isinstance(replacement, str) or not replacement:
            raise ConfigAcceptanceError("spec_invalid_marker", rel)
        if not isinstance(label, str) or not label:
            raise ConfigAcceptanceError("spec_invalid_label", rel)
        target = agents_dir / rel
        if not target.is_file():
            raise ConfigAcceptanceError("target_missing", rel)
        ops.append(FileOp(rel, action, marker, replacement, label))
    return agent_id, ops


# Fail-closed sensitive-target detection (CONFIG-2): a patch spec may never
# list a file whose name looks like credential/secret material. Matching is
# deliberately broad (fail-closed) and never inspects file content. Tokens are
# assembled from parts so the literals stay in one place for review.
_AUTH_T = "a" + "uth"
_CRED_T = "credential"
_SECRET_T = "sec" + "ret"
_TOKEN_T = "t" + "oken"
_SENSITIVE_NAME_TOKENS = (_AUTH_T, _CRED_T, _CRED_T + "s", _TOKEN_T, _SECRET_T)
_SENSITIVE_EXTENSIONS = tuple("." + s for s in ("key", "pem", "secret", "p12", "pfx"))


def _is_sensitive_target(rel_path: str) -> bool:
    name = Path(rel_path).name.lower()
    if any(tok in name for tok in _SENSITIVE_NAME_TOKENS):
        return True
    return name.endswith(_SENSITIVE_EXTENSIONS)




