#!/usr/bin/env python3
"""Generate a fail-closed, candidate-bound local release audit manifest.

The tool executes an explicit JSON gate specification and binds the results to
the current Git HEAD/worktree, required test reports, and required macOS
artifacts. It stores hashes and status metadata only: command output, report
content, credentials, prompts, and responses are never copied into the
manifest.

Specification schema (version 1):

{
  "schema_version": 1,
  "gates": [
    {"id": "structure", "argv": ["python3", "support/scripts/check_structure.py"], "cwd": "."},
    {"id": "rust-workspace", "argv": ["cargo", "test", "--workspace"], "cwd": "code"},
    {"id": "vitest", "argv": ["npm", "run", "test:run"], "cwd": "code/apps/desktop"},
    {"id": "e2e", "argv": ["npm", "run", "test:e2e"], "cwd": "code/apps/desktop"},
    {"id": "tauri", "argv": ["npm", "run", "tauri:build"], "cwd": "code/apps/desktop"},
    {"id": "real-opencode", "argv": ["...", "--explicit-safe-args"], "cwd": "."}
  ],
  "reports": [
    {"kind": "vitest", "path": "...json", "expected_sha256": "..."},
    {"kind": "e2e", "path": "...json", "expected_sha256": "..."},
    {"kind": "tauri", "path": "...json", "expected_sha256": "..."},
    {"kind": "real-opencode", "path": "...json", "expected_sha256": "..."}
  ],
  "artifacts": [
    {"kind": "app", "path": "...app"},
    {"kind": "sidecar", "path": ".../dtoken-proxy"},
    {"kind": "dmg", "path": "...dmg"}
  ]
}

All evidence paths must remain inside the repository. Report hashes are
mandatory and must be captured by the caller at report creation time, so a
report modified before manifest generation fails closed.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Iterable

SCHEMA_VERSION = 1
REQUIRED_GATE_IDS = frozenset(
    {"structure", "rust-workspace", "vitest", "e2e", "tauri", "real-opencode"}
)
REQUIRED_REPORT_KINDS = frozenset({"vitest", "e2e", "tauri", "real-opencode"})
REQUIRED_ARTIFACT_KINDS = frozenset({"app", "sidecar", "dmg"})
PASS_LABELS = frozenset({"pass", "passed", "success", "succeeded", "ok"})
MAX_REPORT_BYTES = 64 * 1024 * 1024
MAX_COMMAND_ARGS = 128
MAX_ARG_CHARS = 2048
SENSITIVE_OPTION_NAMES = frozenset(
    {
        "--authorization",
        "--body",
        "--cookie",
        "--data",
        "--password",
        "--prompt",
        "--raw-prompt",
        "--request-body",
        "--response-body",
        "-d",
    }
)
SENSITIVE_INLINE = re.compile(
    r"(?i)(authorization|cookie|api[-_]?key|password|secret|token)=([^\s]+)"
)
BEARER = re.compile(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]+")
SECRET_TOKEN = re.compile(r"(?i)\b(?:sk|key|token)-[A-Za-z0-9._-]{8,}\b")
SPEC_KEYS = frozenset({"schema_version", "gates", "reports", "artifacts"})
GATE_KEYS = frozenset({"id", "argv", "cwd", "timeout_seconds"})
REPORT_KEYS = frozenset({"kind", "path", "expected_sha256"})
ARTIFACT_KEYS = frozenset({"kind", "path", "expected_sha256"})


class AuditError(RuntimeError):
    """A fail-closed audit input or evidence error."""


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")


def run_read_only_git(root: Path, *arguments: str) -> bytes:
    completed = subprocess.run(
        ["git", *arguments],
        cwd=root,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    if completed.returncode != 0:
        raise AuditError(f"read-only Git metadata command failed: {' '.join(arguments)}")
    return completed.stdout


def collect_git_provenance(root: Path, output: Path) -> dict[str, Any]:
    root = root.resolve()
    output_absolute = Path(os.path.abspath(output))
    if not is_within(output_absolute, root):
        raise AuditError("audit output path escapes the repository")
    output_relative = output_absolute.relative_to(root).as_posix()
    output_exclusion = f":(exclude){output_relative}"
    head = run_read_only_git(root, "rev-parse", "HEAD").decode("ascii").strip()
    if not re.fullmatch(r"[0-9a-fA-F]{40,64}", head):
        raise AuditError("Git HEAD is not a full hexadecimal object id")

    status = run_read_only_git(
        root,
        "-c",
        "core.quotepath=false",
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--",
        ".",
        output_exclusion,
    )
    tracked_diff = run_read_only_git(
        root, "diff", "--binary", "HEAD", "--", ".", output_exclusion
    )
    untracked = run_read_only_git(
        root,
        "-c",
        "core.quotepath=false",
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        ".",
        output_exclusion,
    )
    untracked_records: list[bytes] = []
    for raw_path in sorted(item for item in untracked.split(b"\0") if item):
        relative = Path(os.fsdecode(raw_path))
        full_path = Path(os.path.abspath(root / relative))
        if full_path == output_absolute:
            continue
        if not is_within(full_path, root):
            raise AuditError("untracked worktree path escapes the repository")
        if full_path.is_symlink():
            raise AuditError("untracked symlink cannot be candidate-bound safely")
        if not full_path.is_file():
            continue
        untracked_records.append(
            relative.as_posix().encode("utf-8", "surrogateescape")
            + b"\0"
            + sha256_file(full_path).encode("ascii")
        )

    status_entries = [item for item in status.split(b"\0") if item]
    material = b"tracked-diff\0" + tracked_diff + b"\0untracked\0" + b"\n".join(
        untracked_records
    )
    return {
        "head": head.lower(),
        "dirty": bool(status_entries),
        "status_entry_count": len(status_entries),
        "status_sha256": sha256_bytes(status),
        "worktree_diff_sha256": sha256_bytes(material),
        "worktree_diff_algorithm": "sha256-git-binary-diff-plus-untracked-v1",
        "tracked_diff_sha256": sha256_bytes(tracked_diff),
        "untracked_file_count": len(untracked_records),
        "untracked_manifest_sha256": sha256_bytes(b"\n".join(untracked_records)),
    }


def is_within(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def resolve_evidence_path(root: Path, raw_path: object) -> tuple[Path, str]:
    if not isinstance(raw_path, str) or not raw_path or "\x00" in raw_path:
        raise AuditError("evidence path must be a non-empty string")
    root = root.resolve()
    candidate = Path(os.path.abspath(root / raw_path))
    if not is_within(candidate, root):
        raise AuditError("evidence path escapes the repository")
    current = root
    for component in candidate.relative_to(root).parts:
        current = current / component
        if current.is_symlink():
            raise AuditError("evidence path contains a symlink")
    return candidate, candidate.relative_to(root).as_posix()


def reject_unknown_keys(entry: dict[str, Any], allowed: frozenset[str], label: str) -> None:
    unknown = set(entry) - allowed
    if unknown:
        raise AuditError(f"{label} contains unknown fields: {', '.join(sorted(unknown))}")


def redact_arg(value: str) -> str:
    value = SENSITIVE_INLINE.sub(lambda match: f"{match.group(1)}=[REDACTED]", value)
    value = BEARER.sub("Bearer [REDACTED]", value)
    return SECRET_TOKEN.sub("[REDACTED]", value)


def validate_gate(entry: object, root: Path) -> tuple[str, list[str], Path, int]:
    if not isinstance(entry, dict):
        raise AuditError("gate entry must be an object")
    reject_unknown_keys(entry, GATE_KEYS, "gate entry")
    gate_id = entry.get("id")
    argv = entry.get("argv")
    cwd_raw = entry.get("cwd", ".")
    timeout = entry.get("timeout_seconds", 3600)
    if not isinstance(gate_id, str) or not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,63}", gate_id):
        raise AuditError("gate id is invalid")
    if (
        not isinstance(argv, list)
        or not argv
        or len(argv) > MAX_COMMAND_ARGS
        or not all(isinstance(argument, str) and argument for argument in argv)
    ):
        raise AuditError(f"gate {gate_id} argv is invalid")
    if any("\x00" in argument or "\n" in argument or len(argument) > MAX_ARG_CHARS for argument in argv):
        raise AuditError(f"gate {gate_id} argv contains unsafe content")
    lowered = {argument.lower() for argument in argv}
    if any(
        argument == sensitive or argument.startswith(f"{sensitive}=")
        for argument in lowered
        for sensitive in SENSITIVE_OPTION_NAMES
    ):
        raise AuditError(
            f"gate {gate_id} command embeds a payload/credential option; use a redacted report instead"
        )
    if not isinstance(timeout, int) or timeout < 1 or timeout > 7200:
        raise AuditError(f"gate {gate_id} timeout is invalid")
    cwd, _ = resolve_evidence_path(root, cwd_raw)
    if not cwd.is_dir() or cwd.is_symlink():
        raise AuditError(f"gate {gate_id} working directory is unavailable")
    return gate_id, argv, cwd, timeout


def execute_gate(entry: object, root: Path) -> dict[str, Any]:
    gate_id, argv, cwd, timeout = validate_gate(entry, root)
    started = time.monotonic()
    exit_code: int | None = None
    reason: str | None = None
    try:
        completed = subprocess.run(
            argv,
            cwd=cwd,
            check=False,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=timeout,
        )
        exit_code = completed.returncode
        status = "pass" if exit_code == 0 else "failed"
        if exit_code != 0:
            reason = "nonzero_exit"
    except subprocess.TimeoutExpired:
        status = "failed"
        reason = "timeout"
    except OSError:
        status = "failed"
        reason = "command_unavailable"
    return {
        "id": gate_id,
        "command": [redact_arg(argument) for argument in argv],
        "cwd": cwd.relative_to(root.resolve()).as_posix() or ".",
        "exit_code": exit_code,
        "status": status,
        "reason": reason,
        "duration_ms": round((time.monotonic() - started) * 1000),
    }


def generic_report_passed(payload: Any) -> bool:
    if not isinstance(payload, dict):
        return False
    if payload.get("ok") is True or payload.get("success") is True:
        return True
    status = payload.get("status")
    if isinstance(status, str) and status.lower() in PASS_LABELS:
        return True
    stats = payload.get("stats")
    return (
        isinstance(stats, dict)
        and stats.get("unexpected") == 0
        and isinstance(stats.get("expected"), int)
        and stats["expected"] > 0
    )


def opencode_report_passed(payload: Any) -> bool:
    if not isinstance(payload, dict) or str(payload.get("status", "")).lower() != "pass":
        return False
    transaction = payload.get("config_transaction")
    privacy = payload.get("privacy")
    evidence = payload.get("evidence")
    if not isinstance(transaction, dict) or not isinstance(privacy, dict) or not isinstance(evidence, dict):
        return False
    privacy_flags = (
        "auth_json_read",
        "literal_credentials_recorded",
        "prompt_recorded",
        "response_recorded",
        "nonce_recorded",
    )
    return (
        transaction.get("restore_status") == "restored_exact"
        and transaction.get("original_bytes_restored") is True
        and evidence.get("route_request_provider_model_verified") is True
        and evidence.get("provider_forward_verified") is True
        and all(privacy.get(flag) is False for flag in privacy_flags)
    )


def inspect_report(entry: object, root: Path) -> dict[str, Any]:
    if not isinstance(entry, dict):
        raise AuditError("report entry must be an object")
    reject_unknown_keys(entry, REPORT_KEYS, "report entry")
    kind = entry.get("kind")
    expected = entry.get("expected_sha256")
    if not isinstance(kind, str) or kind not in REQUIRED_REPORT_KINDS:
        raise AuditError("report kind is invalid")
    try:
        path, relative = resolve_evidence_path(root, entry.get("path"))
    except AuditError as error:
        return {"kind": kind, "path": None, "status": "failed", "reason": str(error)}
    base = {"kind": kind, "path": relative}
    if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", expected):
        return {**base, "status": "failed", "reason": "expected_sha256_missing_or_invalid"}
    if not path.exists():
        return {**base, "status": "missing", "reason": "report_missing"}
    if path.is_symlink() or not path.is_file():
        return {**base, "status": "failed", "reason": "report_not_a_regular_file"}
    size = path.stat().st_size
    if size > MAX_REPORT_BYTES:
        return {**base, "status": "failed", "reason": "report_too_large", "bytes": size}
    actual = sha256_file(path)
    if actual != expected.lower():
        return {
            **base,
            "status": "failed",
            "reason": "report_sha256_mismatch",
            "sha256": actual,
            "expected_sha256": expected.lower(),
            "bytes": size,
        }
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return {**base, "status": "failed", "reason": "report_json_invalid", "sha256": actual, "bytes": size}
    passed = opencode_report_passed(payload) if kind == "real-opencode" else generic_report_passed(payload)
    return {
        **base,
        "status": "pass" if passed else "failed",
        "reason": None if passed else "report_does_not_prove_success",
        "sha256": actual,
        "bytes": size,
    }


def hash_artifact(path: Path) -> tuple[str, str, int, int]:
    if path.is_symlink():
        raise AuditError("artifact symlinks are rejected")
    if path.is_file():
        return sha256_file(path), "sha256-file-v1", path.stat().st_size, 1
    if not path.is_dir():
        raise AuditError("artifact is not a file or directory")
    digest = hashlib.sha256()
    total_bytes = 0
    file_count = 0
    for child in sorted(path.rglob("*"), key=lambda item: item.relative_to(path).as_posix()):
        if child.is_symlink():
            raise AuditError("artifact tree contains a symlink")
        if not child.is_file():
            continue
        relative = child.relative_to(path).as_posix().encode("utf-8")
        child_hash = sha256_file(child).encode("ascii")
        size = child.stat().st_size
        digest.update(relative + b"\0" + str(size).encode("ascii") + b"\0" + child_hash + b"\n")
        total_bytes += size
        file_count += 1
    if file_count == 0:
        raise AuditError("artifact directory contains no files")
    return digest.hexdigest(), "sha256-tree-path-size-content-v1", total_bytes, file_count


def inspect_artifact(entry: object, root: Path) -> dict[str, Any]:
    if not isinstance(entry, dict):
        raise AuditError("artifact entry must be an object")
    reject_unknown_keys(entry, ARTIFACT_KEYS, "artifact entry")
    kind = entry.get("kind")
    if not isinstance(kind, str) or kind not in REQUIRED_ARTIFACT_KINDS:
        raise AuditError("artifact kind is invalid")
    try:
        path, relative = resolve_evidence_path(root, entry.get("path"))
    except AuditError as error:
        return {"kind": kind, "path": None, "status": "failed", "reason": str(error)}
    if not path.exists():
        return {"kind": kind, "path": relative, "status": "missing", "reason": "artifact_missing"}
    try:
        digest, algorithm, total_bytes, file_count = hash_artifact(path)
    except (AuditError, OSError) as error:
        return {"kind": kind, "path": relative, "status": "failed", "reason": str(error)}
    expected = entry.get("expected_sha256")
    if expected is not None:
        if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", expected):
            return {"kind": kind, "path": relative, "status": "failed", "reason": "artifact_expected_sha256_invalid"}
        if digest != expected.lower():
            return {
                "kind": kind,
                "path": relative,
                "status": "failed",
                "reason": "artifact_sha256_mismatch",
                "sha256": digest,
                "expected_sha256": expected.lower(),
            }
    return {
        "kind": kind,
        "path": relative,
        "status": "pass",
        "reason": None,
        "sha256": digest,
        "hash_algorithm": algorithm,
        "bytes": total_bytes,
        "file_count": file_count,
    }


def unique_entries(entries: object, key: str, label: str) -> list[dict[str, Any]]:
    if not isinstance(entries, list):
        raise AuditError(f"{label} must be an array")
    result: list[dict[str, Any]] = []
    seen: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict) or not isinstance(entry.get(key), str):
            raise AuditError(f"{label} entry is invalid")
        identity = entry[key]
        if identity in seen:
            raise AuditError(f"duplicate {label} identity: {identity}")
        seen.add(identity)
        result.append(entry)
    return result


def build_manifest(root: Path, output: Path, spec: dict[str, Any]) -> dict[str, Any]:
    reject_unknown_keys(spec, SPEC_KEYS, "audit specification")
    if spec.get("schema_version") != SCHEMA_VERSION:
        raise AuditError("unsupported audit specification schema")
    root = root.resolve()
    provenance = collect_git_provenance(root, output)
    gates_spec = unique_entries(spec.get("gates"), "id", "gates")
    reports_spec = unique_entries(spec.get("reports"), "kind", "reports")
    artifacts_spec = unique_entries(spec.get("artifacts"), "kind", "artifacts")
    gates = [execute_gate(entry, root) for entry in gates_spec]
    reports = [inspect_report(entry, root) for entry in reports_spec]
    artifacts = [inspect_artifact(entry, root) for entry in artifacts_spec]

    gate_ids = {entry["id"] for entry in gates}
    report_kinds = {entry["kind"] for entry in reports}
    artifact_kinds = {entry["kind"] for entry in artifacts}
    blockers: list[str] = []
    failures: list[str] = []
    if provenance["dirty"]:
        blockers.append("git_worktree_dirty")
    blockers.extend(f"missing_gate:{item}" for item in sorted(REQUIRED_GATE_IDS - gate_ids))
    blockers.extend(f"missing_report:{item}" for item in sorted(REQUIRED_REPORT_KINDS - report_kinds))
    blockers.extend(f"missing_artifact:{item}" for item in sorted(REQUIRED_ARTIFACT_KINDS - artifact_kinds))
    for entry in gates:
        if entry["status"] != "pass":
            failures.append(f"gate_failed:{entry['id']}")
    for entry in reports:
        if entry["status"] == "missing":
            blockers.append(f"report_missing:{entry['kind']}")
        elif entry["status"] != "pass":
            failures.append(f"report_failed:{entry['kind']}")
    for entry in artifacts:
        if entry["status"] == "missing":
            blockers.append(f"artifact_missing:{entry['kind']}")
        elif entry["status"] != "pass":
            failures.append(f"artifact_failed:{entry['kind']}")

    platform_record = {
        "system": platform.system(),
        "release": platform.release(),
        "machine": platform.machine(),
        "python": platform.python_version(),
    }
    candidate_material = {
        "head": provenance["head"],
        "worktree_diff_sha256": provenance["worktree_diff_sha256"],
        "platform": platform_record,
    }
    overall = "failed" if failures else "blocked" if blockers else "pass"
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "d-token-candidate-audit-manifest",
        "generated_at": utc_now(),
        "candidate_id": sha256_bytes(canonical_json_bytes(candidate_material)),
        "overall_status": overall,
        "candidate": provenance,
        "platform": platform_record,
        "gates": gates,
        "reports": reports,
        "artifacts": artifacts,
        "blockers": sorted(set(blockers)),
        "failures": sorted(set(failures)),
        "privacy": {
            "command_output_recorded": False,
            "report_content_recorded": False,
            "credentials_recorded": False,
            "raw_prompt_or_response_recorded": False,
        },
    }


def atomic_write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    temporary = Path(temporary_name)
    try:
        if os.name != "nt":
            os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        if os.name != "nt":
            directory_fd = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory_fd)
            finally:
                os.close(directory_fd)
    finally:
        if temporary.exists():
            temporary.unlink()


def parse_args(argv: Iterable[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: Iterable[str] | None = None) -> int:
    arguments = parse_args(argv)
    root = arguments.root.resolve()
    output = arguments.output
    if not output.is_absolute():
        output = root / output
    try:
        output, _ = resolve_evidence_path(root, str(output))
        spec_path, _ = resolve_evidence_path(root, str(arguments.spec))
        spec = json.loads(spec_path.read_text(encoding="utf-8"))
        if not isinstance(spec, dict):
            raise AuditError("audit specification root must be an object")
        manifest = build_manifest(root, output, spec)
    except (AuditError, OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"candidate audit failed closed: {error}", file=sys.stderr)
        return 2
    atomic_write_json(output, manifest)
    print(json.dumps({"overall_status": manifest["overall_status"], "output": str(output)}))
    return 0 if manifest["overall_status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
