#!/usr/bin/env python3
"""Audit real Git operations and maintain separate pending commit plans."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_LOG = ROOT / "support" / "git-operations.json"
DEFAULT_PENDING_LOG = ROOT / "support" / "pending-commits.json"
PENDING_SCHEMA_VERSION = 2
ALLOWED_OPERATIONS = {
    "branch-create",
    "branch-switch",
    "branch-delete",
    "branch-rename",
    "branch-use",
    "commit",
    "merge",
    "rebase",
    "push",
}
PENDING_STATUSES = {"pending", "ready", "blocked", "committed", "superseded"}
STAGING_MODES = {"whole-file", "hunk-level", "mixed"}
VERIFICATION_STATUSES = {"not-run", "passed", "failed", "blocked"}


def git_value(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() if result.returncode == 0 else ""


def now_iso() -> str:
    return datetime.now().astimezone().isoformat(timespec="seconds")


def load_log(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"schema_version": 1, "operations": []}
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("schema_version") != 1 or not isinstance(payload.get("operations"), list):
        raise SystemExit(f"unsupported audit log schema: {path}")
    errors = validate_operation_payload(payload)
    if errors:
        raise SystemExit("invalid audit log:\n" + "\n".join(errors))
    return payload


def load_pending_log(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"schema_version": PENDING_SCHEMA_VERSION, "pending_commits": []}
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("schema_version") not in {1, PENDING_SCHEMA_VERSION} or not isinstance(
        payload.get("pending_commits"), list
    ):
        raise SystemExit(f"unsupported pending commit schema: {path}")
    payload["schema_version"] = PENDING_SCHEMA_VERSION
    return payload


def atomic_write(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def require_one_line(value: str, field: str) -> str:
    normalized = value.strip()
    if not normalized or "\n" in normalized or "\r" in normalized:
        raise SystemExit(f"{field} must be one non-empty line")
    return normalized


def normalize_string_list(values: list[str]) -> list[str]:
    return sorted({value.strip() for value in values if value.strip()})


def validate_operation_entry(entry: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    entry_id = str(entry.get("id", "")).strip() or "<unknown>"
    for field in ("id", "timestamp", "operation", "branch", "summary"):
        value = entry.get(field)
        if not isinstance(value, str) or not value.strip() or "\n" in value or "\r" in value:
            errors.append(f"{entry_id}: {field} must be one non-empty line")
    if entry.get("operation") not in ALLOWED_OPERATIONS:
        errors.append(f"{entry_id}: invalid operation")
    timestamp = entry.get("timestamp")
    if isinstance(timestamp, str):
        try:
            datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
        except ValueError:
            errors.append(f"{entry_id}: timestamp must be ISO 8601")
    for field in ("head_before", "remote"):
        value = entry.get(field)
        if value is not None and (
            not isinstance(value, str) or not value.strip() or "\n" in value or "\r" in value
        ):
            errors.append(f"{entry_id}: {field} must be null or one non-empty line")
    return errors


def validate_operation_payload(payload: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    entries = payload.get("operations", [])
    ids: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            errors.append("operations entries must be objects")
            continue
        entry_id = str(entry.get("id", ""))
        if entry_id in ids:
            errors.append(f"duplicate operation id: {entry_id}")
        ids.add(entry_id)
        errors.extend(validate_operation_entry(entry))
    return errors


def validate_pending_entry(entry: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    entry_id = str(entry.get("id", "")).strip() or "<unknown>"
    for field in ("id", "branch", "phase", "status", "title", "summary", "commit_message"):
        value = entry.get(field)
        if not isinstance(value, str) or not value.strip() or "\n" in value or "\r" in value:
            errors.append(f"{entry_id}: {field} must be one non-empty line")
    if entry.get("status") not in PENDING_STATUSES:
        errors.append(f"{entry_id}: invalid status")

    files = entry.get("files")
    if not isinstance(files, list) or not files or any(
        not isinstance(item, str) or not item.strip() for item in files
    ):
        errors.append(f"{entry_id}: files must be a non-empty string list")
        files = []
    elif files != sorted(set(files)):
        errors.append(f"{entry_id}: files must be unique and sorted")

    staging_mode = entry.get("staging_mode", "whole-file")
    if staging_mode not in STAGING_MODES:
        errors.append(f"{entry_id}: invalid staging_mode")
    shared_files = entry.get("shared_files", [])
    if not isinstance(shared_files, list) or any(item not in files for item in shared_files):
        errors.append(f"{entry_id}: shared_files must be a subset of files")
    if shared_files and staging_mode == "whole-file":
        errors.append(f"{entry_id}: shared files require hunk-level or mixed staging")

    dependencies = entry.get("dependencies", [])
    if (
        not isinstance(dependencies, list)
        or any(not isinstance(item, str) or not item.strip() for item in dependencies)
        or entry_id in dependencies
    ):
        errors.append(f"{entry_id}: dependencies must be a list without self-reference")

    verification = entry.get("verification", {"status": "not-run", "checks": []})
    if (
        not isinstance(verification, dict)
        or verification.get("status", "not-run") not in VERIFICATION_STATUSES
    ):
        errors.append(f"{entry_id}: invalid verification status")
    elif not isinstance(verification.get("checks"), list) or any(
        not isinstance(item, str) or not item.strip() for item in verification.get("checks", [])
    ):
        errors.append(f"{entry_id}: verification checks must be a string list")
    for field in ("tests", "notes"):
        values = entry.get(field, [])
        if not isinstance(values, list) or any(
            not isinstance(item, str) or not item.strip() for item in values
        ):
            errors.append(f"{entry_id}: {field} must be a string list")
    blockers = entry.get("blockers", [])
    if not isinstance(blockers, list) or any(
        not isinstance(item, str) or not item.strip() for item in blockers
    ):
        errors.append(f"{entry_id}: blockers must be a string list")
        blockers = []
    if entry.get("status") == "ready" and blockers:
        errors.append(f"{entry_id}: ready plans cannot have blockers")
    if entry.get("status") == "ready" and (
        not isinstance(verification, dict) or verification.get("status") != "passed"
    ):
        errors.append(f"{entry_id}: ready plans require passed verification")
    if entry.get("status") == "blocked" and not blockers:
        errors.append(f"{entry_id}: blocked plans require at least one blocker")
    submitted = entry.get("submitted", False)
    if not isinstance(submitted, bool):
        errors.append(f"{entry_id}: submitted must be a boolean")
    if entry.get("status") == "committed":
        if not submitted or not entry.get("commit_hash") or not entry.get("submitted_at"):
            errors.append(
                f"{entry_id}: committed plans require submitted=true, submitted_at and commit_hash"
            )
    elif submitted or entry.get("commit_hash") or entry.get("submitted_at"):
        errors.append(f"{entry_id}: uncommitted plans cannot have submission evidence")
    return errors


def validate_pending_payload(payload: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    entries = payload.get("pending_commits", [])
    ids: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            errors.append("pending_commits entries must be objects")
            continue
        entry_id = str(entry.get("id", ""))
        if entry_id in ids:
            errors.append(f"duplicate pending commit id: {entry_id}")
        ids.add(entry_id)
        errors.extend(validate_pending_entry(entry))
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        for dependency in entry.get("dependencies", []):
            if dependency not in ids:
                errors.append(f"{entry.get('id', '<unknown>')}: unknown dependency {dependency}")
    return errors


def record(args: argparse.Namespace) -> None:
    if args.operation not in ALLOWED_OPERATIONS:
        allowed = ", ".join(sorted(ALLOWED_OPERATIONS))
        raise SystemExit(f"unsupported operation {args.operation!r}; choose one of: {allowed}")
    summary = require_one_line(args.summary, "summary")
    payload = load_log(args.file.resolve())
    branch = args.branch or git_value("branch", "--show-current") or "detached"
    entry = {
        "id": str(uuid.uuid4()),
        "timestamp": now_iso(),
        "operation": args.operation,
        "branch": branch,
        "summary": summary,
        "head_before": args.head_before or git_value("rev-parse", "--short=12", "HEAD") or None,
        "remote": args.remote or None,
    }
    payload["operations"].append(entry)
    errors = validate_operation_payload(payload)
    if errors:
        raise SystemExit("invalid audit log:\n" + "\n".join(errors))
    atomic_write(args.file.resolve(), payload)
    print(json.dumps(entry, ensure_ascii=False, separators=(",", ":")))


def query(args: argparse.Namespace) -> None:
    entries = load_log(args.file.resolve())["operations"]
    if args.operation:
        entries = [entry for entry in entries if entry.get("operation") == args.operation]
    if args.branch:
        entries = [entry for entry in entries if entry.get("branch") == args.branch]
    entries = entries[-args.limit :]
    if args.json:
        print(json.dumps(entries, ensure_ascii=False, indent=2))
        return
    for entry in entries:
        remote = f" remote={entry['remote']}" if entry.get("remote") else ""
        print(
            f"{entry.get('timestamp', '-')} {entry.get('operation', '-')} "
            f"branch={entry.get('branch', '-')} head_before={entry.get('head_before', '-')}"
            f"{remote} — {entry.get('summary', '')}"
        )


def validate_log(args: argparse.Namespace) -> None:
    payload = load_log(args.file.resolve())
    result = {"valid": True, "errors": [], "entries": len(payload["operations"])}
    print(json.dumps(result, ensure_ascii=False, indent=2 if args.json else None))


def plan_commit(args: argparse.Namespace) -> None:
    files = normalize_string_list(args.files)
    if not files:
        raise SystemExit("at least one --file-path is required")
    shared_files = normalize_string_list(args.shared_files)
    payload = load_pending_log(args.pending_file.resolve())
    branch = args.branch or git_value("branch", "--show-current") or "detached"
    timestamp = now_iso()
    checks = [item.strip() for item in args.tests if item.strip()]
    entry = {
        "id": args.id or str(uuid.uuid4()),
        "created_at": timestamp,
        "updated_at": timestamp,
        "branch": branch,
        "phase": require_one_line(args.phase, "phase"),
        "status": args.status,
        "title": require_one_line(args.title, "title"),
        "summary": require_one_line(args.summary, "summary"),
        "commit_message": require_one_line(args.commit_message, "commit-message"),
        "head_before": args.head_before or git_value("rev-parse", "--short=12", "HEAD") or None,
        "files": files,
        "tests": checks,
        "dependencies": normalize_string_list(args.dependencies),
        "notes": [item.strip() for item in args.notes if item.strip()],
        "staging_mode": args.staging_mode,
        "shared_files": shared_files,
        "verification": {"status": args.verification_status, "checks": checks},
        "blockers": [item.strip() for item in args.blockers if item.strip()],
        "submitted": False,
        "submitted_at": None,
        "commit_hash": None,
    }
    payload["pending_commits"].append(entry)
    errors = validate_pending_payload(payload)
    if errors:
        raise SystemExit("invalid pending plan:\n" + "\n".join(errors))
    atomic_write(args.pending_file.resolve(), payload)
    print(json.dumps(entry, ensure_ascii=False, separators=(",", ":")))


def mark_plan(args: argparse.Namespace) -> None:
    payload = load_pending_log(args.pending_file.resolve())
    entry = next((item for item in payload["pending_commits"] if item.get("id") == args.id), None)
    if entry is None:
        raise SystemExit(f"pending commit id not found: {args.id}")
    entry["status"] = args.status
    entry["updated_at"] = now_iso()
    if args.status == "committed":
        if not args.commit_hash:
            raise SystemExit("committed status requires --commit-hash")
        entry["submitted"] = True
        entry["submitted_at"] = now_iso()
        entry["commit_hash"] = require_one_line(args.commit_hash, "commit-hash")
    else:
        entry["submitted"] = False
        entry["submitted_at"] = None
        entry["commit_hash"] = None
    if args.verification_status:
        entry.setdefault("verification", {})["status"] = args.verification_status
    if args.clear_blockers:
        entry["blockers"] = []
    if args.blocker:
        entry.setdefault("blockers", []).append(require_one_line(args.blocker, "blocker"))
    if args.note:
        entry.setdefault("notes", []).append(require_one_line(args.note, "note"))
    errors = validate_pending_payload(payload)
    if errors:
        raise SystemExit("invalid pending plan:\n" + "\n".join(errors))
    atomic_write(args.pending_file.resolve(), payload)
    print(json.dumps(entry, ensure_ascii=False, separators=(",", ":")))


def query_plans(args: argparse.Namespace) -> None:
    entries = load_pending_log(args.pending_file.resolve())["pending_commits"]
    if args.status:
        entries = [entry for entry in entries if entry.get("status") == args.status]
    if args.branch:
        entries = [entry for entry in entries if entry.get("branch") == args.branch]
    if args.phase:
        entries = [entry for entry in entries if entry.get("phase") == args.phase]
    entries = entries[-args.limit :]
    if args.json:
        print(json.dumps(entries, ensure_ascii=False, indent=2))
        return
    for entry in entries:
        print(
            f"{entry.get('updated_at', '-')} {entry.get('status', '-')} "
            f"phase={entry.get('phase', '-')} branch={entry.get('branch', '-')} "
            f"id={entry.get('id', '-')} — {entry.get('commit_message', '')}"
        )


def validate_plans(args: argparse.Namespace) -> None:
    payload = load_pending_log(args.pending_file.resolve())
    errors = validate_pending_payload(payload)
    result = {"valid": not errors, "errors": errors, "entries": len(payload["pending_commits"])}
    print(json.dumps(result, ensure_ascii=False, indent=2 if args.json else None))
    if errors:
        raise SystemExit(1)


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    root.add_argument("--file", type=Path, default=DEFAULT_LOG)
    root.add_argument("--pending-file", type=Path, default=DEFAULT_PENDING_LOG)
    subparsers = root.add_subparsers(dest="command", required=True)

    record_parser = subparsers.add_parser("record")
    record_parser.add_argument("--operation", required=True)
    record_parser.add_argument("--summary", required=True)
    record_parser.add_argument("--branch")
    record_parser.add_argument("--head-before")
    record_parser.add_argument("--remote")
    record_parser.set_defaults(func=record)

    query_parser = subparsers.add_parser("query")
    query_parser.add_argument("--operation")
    query_parser.add_argument("--branch")
    query_parser.add_argument("--limit", type=int, default=20)
    query_parser.add_argument("--json", action="store_true")
    query_parser.set_defaults(func=query)

    validate_log_parser = subparsers.add_parser("validate")
    validate_log_parser.add_argument("--json", action="store_true")
    validate_log_parser.set_defaults(func=validate_log)

    plan_parser = subparsers.add_parser("plan")
    plan_parser.add_argument("--id")
    plan_parser.add_argument("--phase", required=True)
    plan_parser.add_argument("--title", required=True)
    plan_parser.add_argument("--summary", required=True)
    plan_parser.add_argument("--commit-message", required=True)
    plan_parser.add_argument("--file-path", dest="files", action="append", default=[])
    plan_parser.add_argument("--test", dest="tests", action="append", default=[])
    plan_parser.add_argument("--dependency", dest="dependencies", action="append", default=[])
    plan_parser.add_argument("--note", dest="notes", action="append", default=[])
    plan_parser.add_argument("--shared-file", dest="shared_files", action="append", default=[])
    plan_parser.add_argument("--blocker", dest="blockers", action="append", default=[])
    plan_parser.add_argument("--staging-mode", choices=sorted(STAGING_MODES), default="whole-file")
    plan_parser.add_argument("--verification-status", choices=sorted(VERIFICATION_STATUSES), default="not-run")
    plan_parser.add_argument("--branch")
    plan_parser.add_argument("--head-before")
    plan_parser.add_argument("--status", choices=sorted(PENDING_STATUSES), default="pending")
    plan_parser.set_defaults(func=plan_commit)

    mark_parser = subparsers.add_parser("mark-plan")
    mark_parser.add_argument("--id", required=True)
    mark_parser.add_argument("--status", required=True, choices=sorted(PENDING_STATUSES))
    mark_parser.add_argument("--commit-hash")
    mark_parser.add_argument("--note")
    mark_parser.add_argument("--blocker")
    mark_parser.add_argument("--clear-blockers", action="store_true")
    mark_parser.add_argument("--verification-status", choices=sorted(VERIFICATION_STATUSES))
    mark_parser.set_defaults(func=mark_plan)

    pending_query = subparsers.add_parser("query-plans")
    pending_query.add_argument("--status", choices=sorted(PENDING_STATUSES))
    pending_query.add_argument("--branch")
    pending_query.add_argument("--phase")
    pending_query.add_argument("--limit", type=int, default=20)
    pending_query.add_argument("--json", action="store_true")
    pending_query.set_defaults(func=query_plans)

    validate_parser = subparsers.add_parser("validate-plans")
    validate_parser.add_argument("--json", action="store_true")
    validate_parser.set_defaults(func=validate_plans)

    return root


def main() -> None:
    args = parser().parse_args()
    if getattr(args, "limit", 1) < 1:
        raise SystemExit("limit must be at least 1")
    args.func(args)


if __name__ == "__main__":
    main()
