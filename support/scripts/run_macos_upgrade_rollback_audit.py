#!/usr/bin/env python3
"""macOS DMG install -> upgrade -> rollback -> uninstall audit (PR-8, PLATFORM-1).

Audits a candidate DMG pair (install DMG, upgrade DMG) through a full
controlled lifecycle inside an explicit, caller-provided app directory:

    pre_state_snapshot -> install -> verify_installed -> upgrade ->
    verify_after_upgrade -> rollback -> verify_after_rollback ->
    uninstall -> verify_after_uninstall -> cleanup_restore

Evidence is hash/mode based only: every step records a bundle aggregate
SHA-256 digest (over the sorted per-file digests and mode bits), the
Contents/Info.plist SHA-256, the main executable SHA-256, the bundle
CFBundleShortVersionString, and the read-only LaunchServices registration
state queried from `lsregister -dump` for the bundle id. The artifact never
contains file contents and never contains absolute paths; DMG source files
are fingerprinted (SHA-256 + size) before anything is mounted.

Safety contract (defaults to no side effects)
---------------------------------------------
* The default mode is a pure dry-run: inputs are validated and the exact
  plan is printed, nothing is mounted, copied, or modified, and no artifact
  is written. Real execution requires the explicit `--execute` flag AND the
  explicit `--install-dmg`, `--upgrade-dmg`, `--app-dir`, `--app-name`,
  `--namespace`, `--candidate-commit` and `--source-digest` parameters.
* The target app bundle must NOT already exist in the app directory; an
  existing same-name bundle is refused fail-closed before any action
  (exit 2, nothing touched). The user's existing installs are therefore
  never at risk: point --app-dir at a dedicated scratch directory.
* Mount points and temp state are scoped to a unique namespace
  (`dtoken-upgrade-<namespace>-*` under the system temp root); every mount
  is detached in a finally block and the temp root is removed afterwards.
* On non-macOS hosts, `--execute` writes a blocked artifact (status=blocked,
  reason=platform-unsupported) instead of pretending a run happened. Windows
  real-machine evidence is out of scope of this script by design.
* Any post-create verification failure, failed delete/restore, or failed
  cleanup sets ok=false (artifact status=failed) and cleanup still runs.

Artifact contract (PR-7 aggregator compatible)
----------------------------------------------
Top-level JSON object with exactly these keys: schema_version, status
(verified|failed|blocked), mode (always "executed"; dry-run/check-schema
never write an artifact), platform, namespace, candidate_commit (40-hex git
sha of the candidate), source_digest (64-hex sha256 source digest of the candidate), ok (bool,
true only when status == verified), sources, steps, rollback, cleanup,
errors. Written to
support/artifacts/upgrade-autostart-credential/upgrade-rollback-audit-*.json
by default. `--check-schema` validates an existing artifact against this
contract and exits without side effects.

Exit codes: 0 verified; 1 failed or blocked; 2 usage/validation error or
failed dry-run (nothing executed in either case).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Optional

SCHEMA_VERSION = "1.0"
STATUS_VERIFIED = "verified"
STATUS_FAILED = "failed"
STATUS_BLOCKED = "blocked"
MODE_EXECUTED = "executed"
ARTIFACT_DIR_NAME = "upgrade-autostart-credential"
DEFAULT_ARTIFACT_NAME = "upgrade-rollback-audit-{namespace}.json"
ARTIFACT_KEYS = frozenset(
    {
        "schema_version",
        "status",
        "mode",
        "platform",
        "namespace",
        "candidate_commit",
        "source_digest",
        "ok",
        "sources",
        "steps",
        "rollback",
        "cleanup",
        "errors",
    }
)
ARTIFACT_STATUSES = frozenset({STATUS_VERIFIED, STATUS_FAILED, STATUS_BLOCKED})
STEP_KEYS = frozenset(
    {
        "step", "status", "app_version", "digest", "info_plist_sha256",
        "executable_sha256", "file_count", "launch_services", "error_class",
    }
)
STEP_STATUSES = frozenset({"ok", "failed"})
REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ARTIFACT_DIR = REPO_ROOT / "support" / "artifacts" / ARTIFACT_DIR_NAME
LSREGISTER = (
    "/System/Library/Frameworks/CoreServices.framework/Frameworks/"
    "LaunchServices.framework/Support/lsregister"
)
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")


class AuditError(Exception):
    """Base class for fail-closed audit failures."""


def _run(argv: list[str], **kwargs: Any) -> subprocess.CompletedProcess:
    """Thin subprocess wrapper; tests patch this to inject faults."""
    return subprocess.run(argv, **kwargs)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 16), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_namespace(namespace: str) -> list[str]:
    """Namespace must be safe for mount-point and artifact names."""
    problems: list[str] = []
    if not namespace:
        problems.append("--namespace is required")
        return problems
    if len(namespace) > 48:
        problems.append("--namespace must be at most 48 characters")
    if not re.fullmatch(r"[a-z0-9](?:[a-z0-9-]*[a-z0-9])?", namespace):
        problems.append("--namespace must match [a-z0-9][a-z0-9-]*")
    return problems


def validate_app_name(app_name: str) -> list[str]:
    problems: list[str] = []
    if not app_name:
        problems.append("--app-name is required")
    elif not app_name.endswith(".app"):
        problems.append("--app-name must end with '.app'")
    elif "/" in app_name or "\\" in app_name or app_name.startswith("."):
        problems.append("--app-name must be a bare bundle name")
    return problems


def validate_binding(commit: Optional[str], digest: Optional[str]) -> list[str]:
    problems: list[str] = []
    if not commit:
        problems.append("--candidate-commit is required in execute mode")
    elif not COMMIT_RE.fullmatch(commit):
        problems.append("--candidate-commit must be a 40-hex git sha")
    if not digest:
        problems.append("--source-digest is required in execute mode")
    elif not DIGEST_RE.fullmatch(digest):
        problems.append("--source-digest must be a 64-hex sha256")
    return problems


def parse_arguments(argv: Optional[list[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="run_macos_upgrade_rollback_audit.py",
        description=(
            "Audit a candidate DMG pair through install -> upgrade -> "
            "rollback -> uninstall with hash/mode/LaunchServices evidence. "
            "Defaults to a no-side-effect dry-run; real execution requires "
            "--execute plus every explicit parameter."
        ),
    )
    parser.add_argument("--install-dmg", metavar="PATH", help="candidate install DMG (required)")
    parser.add_argument("--upgrade-dmg", metavar="PATH", help="candidate upgrade DMG (required)")
    parser.add_argument("--app-dir", metavar="DIR", help="existing writable scratch app directory (required)")
    parser.add_argument("--app-name", metavar="NAME", help="bundle name, e.g. d-token.app (required)")
    parser.add_argument("--namespace", metavar="NS", help="unique isolation namespace, [a-z0-9-]+ (required)")
    parser.add_argument(
        "--bundle-id",
        metavar="ID",
        default="",
        help="bundle identifier used for the read-only LaunchServices query "
        "(default: read from the installed bundle's Info.plist)",
    )
    parser.add_argument(
        "--candidate-commit",
        metavar="SHA",
        default="",
        help="40-hex git sha of the candidate (aggregator binding; required in execute mode)",
    )
    parser.add_argument(
        "--source-digest",
        metavar="SHA256",
        default="",
        help="64-hex sha256 source digest of the candidate (aggregator binding; required in execute mode)",
    )
    parser.add_argument(
        "--artifact",
        metavar="PATH",
        help=(
            "artifact output path for --execute, or artifact to validate for "
            f"--check-schema (default: support/artifacts/{ARTIFACT_DIR_NAME}/"
            f"{DEFAULT_ARTIFACT_NAME})"
        ),
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--dry-run",
        action="store_true",
        help="validate inputs and print the plan without executing anything (fail-closed)",
    )
    mode.add_argument(
        "--check-schema",
        action="store_true",
        help="validate an existing artifact JSON against the evidence contract, then exit",
    )
    mode.add_argument(
        "--execute",
        action="store_true",
        help="run the real DMG install/upgrade/rollback/uninstall lifecycle (requires all explicit parameters)",
    )
    return parser.parse_args(argv)


def default_artifact_path(namespace: str) -> Path:
    return DEFAULT_ARTIFACT_DIR / DEFAULT_ARTIFACT_NAME.format(namespace=namespace)


def validate_inputs(args: argparse.Namespace) -> list[str]:
    """Fail-closed pre-run validation shared by --dry-run and --execute."""
    problems: list[str] = []
    problems.extend(validate_namespace(args.namespace or ""))
    problems.extend(validate_app_name(args.app_name or ""))
    if not args.install_dmg:
        problems.append("--install-dmg is required")
    if not args.upgrade_dmg:
        problems.append("--upgrade-dmg is required")
    if not args.app_dir:
        problems.append("--app-dir is required")
    if args.install_dmg:
        install = Path(args.install_dmg)
        if not install.is_file():
            problems.append(f"--install-dmg is not a regular file: {install.name}")
    if args.upgrade_dmg:
        upgrade = Path(args.upgrade_dmg)
        if not upgrade.is_file():
            problems.append(f"--upgrade-dmg is not a regular file: {upgrade.name}")
    if args.app_dir:
        app_dir = Path(args.app_dir)
        if not app_dir.is_dir():
            problems.append(f"--app-dir is not a directory: {app_dir.name}")
        elif not os.access(app_dir, os.W_OK):
            problems.append(f"--app-dir is not writable: {app_dir.name}")
        elif args.app_name:
            target = app_dir / args.app_name
            if target.exists() or target.is_symlink():
                problems.append(
                    f"refusing existing same-name target app (fail-closed, "
                    f"nothing will be touched): {args.app_name}"
                )
    return problems


def plan_steps(_args: argparse.Namespace) -> list[str]:
    return [
        "pre_state_snapshot",
        "install",
        "verify_installed",
        "upgrade",
        "verify_after_upgrade",
        "rollback",
        "verify_after_rollback",
        "uninstall",
        "verify_after_uninstall",
        "cleanup_restore",
    ]


def fingerprint_dmg(path: Path) -> dict[str, Any]:
    return {
        "name": path.name,
        "sha256": hash_file(path),
        "size": path.stat().st_size,
    }


def hdiutil_attach(dmg: Path, mountpoint: Path) -> str:
    """Attach a DMG read-only at mountpoint; returns the mountpoint path."""
    result = _run(
        [
            "hdiutil", "attach", "-nobrowse", "-readonly",
            "-mountpoint", str(mountpoint), str(dmg),
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise AuditError(f"hdiutil attach failed: {result.stderr.strip()[:200]}")
    return str(mountpoint)


def hdiutil_detach(mountpoint: Path) -> None:
    result = _run(
        ["hdiutil", "detach", str(mountpoint)],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise AuditError(f"hdiutil detach failed: {result.stderr.strip()[:200]}")


def find_bundle(mountpoint: Path, app_name: str) -> Path:
    bundle = mountpoint / app_name
    if not bundle.is_dir():
        candidates = sorted(
            p.name for p in mountpoint.iterdir()
            if p.is_dir() and p.name.endswith(".app")
        )
        if app_name not in candidates:
            raise AuditError(
                f"bundle {app_name} not found in DMG "
                f"(found: {', '.join(candidates) or 'none'})"
            )
    return bundle


def copy_bundle(source: Path, target: Path) -> None:
    shutil.copytree(source, target, symlinks=True)


def bundle_metadata(bundle: Path) -> tuple[dict[str, Any], str, str, str, str]:
    """Return (info dict, version, info_plist_sha256, executable_sha256, executable_name)."""
    info_plist = bundle / "Contents" / "Info.plist"
    if not info_plist.is_file():
        raise AuditError(f"Info.plist missing in bundle: {bundle.name}")
    with info_plist.open("rb") as handle:
        info = plistlib.load(handle)
    version = str(info.get("CFBundleShortVersionString", ""))
    executable_name = str(info.get("CFBundleExecutable", ""))
    executable = bundle / "Contents" / "MacOS" / executable_name if executable_name else None
    if executable is None or not executable.is_file():
        raise AuditError(f"main executable missing in bundle: {bundle.name}")
    return (
        info,
        version,
        sha256_bytes(info_plist.read_bytes()),
        hash_file(executable),
        executable_name,
    )


def hash_bundle(bundle: Path) -> tuple[str, int]:
    """Aggregate digest over sorted (relative path, sha256, mode) tuples.

    Never embeds file contents — only digests and mode bits.
    """
    entries: list[str] = []
    for path in sorted(bundle.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(bundle).as_posix()
        mode = path.stat().st_mode & 0o7777
        entries.append(f"{rel}:{hash_file(path)}:{mode:o}")
    aggregate = hashlib.sha256()
    for entry in entries:
        aggregate.update(entry.encode("utf-8", "surrogateescape"))
        aggregate.update(b"\0")
    return aggregate.hexdigest(), len(entries)


def lsregister_state(bundle_id: str, app_name: str) -> dict[str, Any]:
    """Read-only LaunchServices registration query for the bundle id."""
    result = _run(
        [LSREGISTER, "-dump"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return {"queryable": False, "registered_entries": None}
    lines = result.stdout.splitlines()
    id_matches = [line for line in lines if bundle_id in line]
    app_matches = [line for line in lines if app_name in line]
    return {
        "queryable": True,
        "registered_entries": len(id_matches),
        "app_name_mentions": len(app_matches),
    }


def write_artifact_atomic(artifact_path: Path, artifact: dict[str, Any]) -> None:
    artifact_path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(artifact, indent=2, ensure_ascii=False, sort_keys=True) + "\n"
    tmp = artifact_path.with_name(artifact_path.name + ".tmp")
    tmp.write_text(payload, encoding="utf-8")
    os.chmod(tmp, 0o600)
    os.replace(tmp, artifact_path)


def check_artifact_schema(artifact_path: Path) -> list[str]:
    """Validate an artifact against the evidence contract; fail-closed."""
    violations: list[str] = []
    try:
        data = json.loads(artifact_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        return [f"artifact unreadable or not JSON: {error}"]
    if not isinstance(data, dict):
        return ["artifact must be a JSON object"]
    if set(data.keys()) != ARTIFACT_KEYS:
        violations.append(
            f"keys must be exactly {sorted(ARTIFACT_KEYS)}, got {sorted(data.keys())}"
        )
    if data.get("schema_version") != SCHEMA_VERSION:
        violations.append(f"schema_version must be {SCHEMA_VERSION}")
    if data.get("status") not in ARTIFACT_STATUSES:
        violations.append(f"status must be one of {sorted(ARTIFACT_STATUSES)}")
    if data.get("mode") != MODE_EXECUTED:
        violations.append(f"mode must be {MODE_EXECUTED}")
    if not isinstance(data.get("platform"), str):
        violations.append("platform must be a string")
    if not isinstance(data.get("namespace"), str):
        violations.append("namespace must be a string")
    commit = data.get("candidate_commit")
    if not isinstance(commit, str) or not COMMIT_RE.fullmatch(commit):
        violations.append("candidate_commit must be a 40-hex git sha")
    digest = data.get("source_digest")
    if not isinstance(digest, str) or not DIGEST_RE.fullmatch(digest):
        violations.append("source_digest must be a 64-hex sha256")
    ok = data.get("ok")
    if not isinstance(ok, bool):
        violations.append("ok must be a boolean")
    elif ok != (data.get("status") == STATUS_VERIFIED):
        violations.append("ok must be true only when status == verified")
    if not isinstance(data.get("sources"), list):
        violations.append("sources must be a list")
    else:
        for source in data["sources"]:
            if not isinstance(source, dict) or set(source.keys()) != {"name", "sha256", "size"}:
                violations.append("sources entries must have exactly name/sha256/size")
    if not isinstance(data.get("steps"), list):
        violations.append("steps must be a list")
    elif data.get("status") != STATUS_BLOCKED and not data["steps"]:
        violations.append("steps must be a non-empty list")
    else:
        for step in data["steps"]:
            if not isinstance(step, dict) or set(step.keys()) != STEP_KEYS:
                violations.append(f"step keys must be exactly {sorted(STEP_KEYS)}")
            elif step.get("status") not in STEP_STATUSES:
                violations.append("step status must be ok or failed")
    rollback = data.get("rollback")
    if not isinstance(rollback, dict) or set(rollback.keys()) != {
        "matched_install_digest", "version_equal_install",
    }:
        violations.append(
            "rollback must have exactly matched_install_digest/version_equal_install"
        )
    else:
        for name in ("matched_install_digest", "version_equal_install"):
            _bool_or_blocked_null(
                rollback.get(name), data.get("status"), f"rollback.{name}", violations,
            )
    cleanup = data.get("cleanup")
    if not isinstance(cleanup, dict) or set(cleanup.keys()) != {
        "mounts_detached", "app_removed", "restored_exact", "temp_root_removed",
    }:
        violations.append(
            "cleanup must have exactly "
            "mounts_detached/app_removed/restored_exact/temp_root_removed"
        )
    else:
        for name in (
            "mounts_detached", "app_removed", "restored_exact", "temp_root_removed",
        ):
            _bool_or_blocked_null(
                cleanup.get(name), data.get("status"), f"cleanup.{name}", violations,
            )
    if not isinstance(data.get("errors"), list):
        violations.append("errors must be a list")
    return violations


def _bool_or_blocked_null(value: Any, status: Any, name: str, violations: list[str]) -> None:
    """Booleans are required for executed runs; blocked runs may record
    null for facts that were never probed (no false claims of evidence)."""
    if value is None and status == STATUS_BLOCKED:
        return
    if not isinstance(value, bool):
        violations.append(f"{name} must be a boolean"
                          + ("" if status == STATUS_BLOCKED else " (or null only for blocked artifacts)"))


def run_audit(args: argparse.Namespace) -> tuple[str, dict[str, Any]]:
    """Execute the real lifecycle; returns (status, artifact)."""
    namespace = args.namespace
    app_dir = Path(args.app_dir)
    app_name = args.app_name
    install_dmg = Path(args.install_dmg)
    upgrade_dmg = Path(args.upgrade_dmg)

    errors: list[str] = []
    steps: list[dict[str, Any]] = []
    mounted: list[Path] = []
    temp_root: Optional[Path] = None
    installed_bundle: Optional[Path] = None
    install_version = ""
    upgrade_version = ""
    install_aggregate = ""
    upgrade_aggregate = ""
    install_info_sha = ""
    install_exec_sha = ""
    rollback_matched = False
    rollback_version_equal = False
    bundle_id = args.bundle_id or ""
    install_sources: list[dict[str, Any]] = []

    def record_step(
        step: str,
        status: str,
        *,
        version: str = "",
        digest: str = "",
        info_sha: str = "",
        exec_sha: str = "",
        file_count: int = 0,
        launch_services: Optional[dict[str, Any]] = None,
        error_class: str = "",
    ) -> None:
        steps.append(
            {
                "step": step,
                "status": status,
                "app_version": version,
                "digest": digest,
                "info_plist_sha256": info_sha,
                "executable_sha256": exec_sha,
                "file_count": file_count,
                "launch_services": launch_services or {},
                "error_class": error_class,
            }
        )

    def cleanup() -> dict[str, Any]:
        mounts_detached = True
        for mountpoint in list(mounted):
            try:
                hdiutil_detach(mountpoint)
            except AuditError as error:
                mounts_detached = False
                errors.append(f"cleanup mount detach failed: {error}")
            finally:
                mounted.remove(mountpoint)
        app_removed = True
        if installed_bundle is not None and installed_bundle.exists():
            try:
                shutil.rmtree(installed_bundle)
            except OSError as error:
                app_removed = False
                errors.append(f"cleanup app removal failed: {error}")
        temp_removed = True
        if temp_root is not None and temp_root.exists():
            try:
                shutil.rmtree(temp_root)
            except OSError as error:
                temp_removed = False
                errors.append(f"cleanup temp root removal failed: {error}")
        restored_exact = (
            app_removed
            and mounts_detached
            and temp_removed
            and not (app_dir / app_name).exists()
        )
        return {
            "mounts_detached": mounts_detached,
            "app_removed": app_removed,
            "restored_exact": restored_exact,
            "temp_root_removed": temp_removed,
        }

    try:
        try:
            temp_root = Path(tempfile.mkdtemp(prefix=f"dtoken-upgrade-{namespace}-"))
            mount_install = temp_root / "mnt-install"
            mount_upgrade = temp_root / "mnt-upgrade"
            mount_install.mkdir()
            mount_upgrade.mkdir()
            install_sources = [
                fingerprint_dmg(install_dmg),
                fingerprint_dmg(upgrade_dmg),
            ]
        except Exception as error:
            errors.append(f"pre-run setup failed: {error}")
            record_step("setup", "failed", error_class=error.__class__.__name__)
            raise

        # pre_state_snapshot: the app-dir is known empty of the target bundle
        # (validation refused an existing same-name app). The bundle id is
        # resolved from the mounted install DMG's source bundle so the
        # LaunchServices query records a genuine pre-install baseline of
        # registrations for that bundle id (a user's real install elsewhere
        # or a leftover from a crashed prior run is visible, not conflated
        # with registrations caused by this run).
        try:
            hdiutil_attach(install_dmg, mount_install)
            mounted.append(mount_install)
            source_bundle = find_bundle(mount_install, app_name)
            if not bundle_id:
                source_info, _, _, _, _ = bundle_metadata(source_bundle)
                bundle_id = str(source_info.get("CFBundleIdentifier", ""))
            pre_state_ls = (
                lsregister_state(bundle_id, app_name) if bundle_id else {}
            )
            record_step("pre_state_snapshot", "ok", launch_services=pre_state_ls)
        except Exception as error:
            record_step(
                "pre_state_snapshot", "failed",
                error_class=error.__class__.__name__,
            )
            errors.append(f"pre_state_snapshot failed: {error}")
            raise

        # install
        try:
            installed_bundle = app_dir / app_name
            copy_bundle(source_bundle, installed_bundle)
            record_step("install", "ok")
        except Exception as error:
            record_step("install", "failed", error_class=error.__class__.__name__)
            errors.append(f"install failed: {error}")
            raise
        try:
            info, install_version, install_info_sha, install_exec_sha, _ = bundle_metadata(
                installed_bundle
            )
            install_aggregate, install_file_count = hash_bundle(installed_bundle)
            if not bundle_id:
                bundle_id = str(info.get("CFBundleIdentifier", ""))
            record_step(
                "verify_installed", "ok",
                version=install_version,
                digest=install_aggregate,
                info_sha=install_info_sha,
                exec_sha=install_exec_sha,
                file_count=install_file_count,
                launch_services=lsregister_state(bundle_id, app_name) if bundle_id else {},
            )
        except Exception as error:
            record_step("verify_installed", "failed", error_class=error.__class__.__name__)
            errors.append(f"verify_installed failed: {error}")
            raise

        # upgrade
        try:
            hdiutil_attach(upgrade_dmg, mount_upgrade)
            mounted.append(mount_upgrade)
            upgrade_source = find_bundle(mount_upgrade, app_name)
            assert installed_bundle is not None
            shutil.rmtree(installed_bundle)
            copy_bundle(upgrade_source, installed_bundle)
            record_step("upgrade", "ok")
        except Exception as error:
            record_step("upgrade", "failed", error_class=error.__class__.__name__)
            errors.append(f"upgrade failed: {error}")
            raise
        try:
            _, upgrade_version, upgrade_info_sha, upgrade_exec_sha, _ = bundle_metadata(
                installed_bundle
            )
            upgrade_aggregate, upgrade_file_count = hash_bundle(installed_bundle)
            if not bundle_id:
                info, _, _, _, _ = bundle_metadata(installed_bundle)
                bundle_id = str(info.get("CFBundleIdentifier", ""))
            record_step(
                "verify_after_upgrade", "ok",
                version=upgrade_version,
                digest=upgrade_aggregate,
                info_sha=upgrade_info_sha,
                exec_sha=upgrade_exec_sha,
                file_count=upgrade_file_count,
                launch_services=lsregister_state(bundle_id, app_name) if bundle_id else {},
            )
        except Exception as error:
            record_step("verify_after_upgrade", "failed", error_class=error.__class__.__name__)
            errors.append(f"verify_after_upgrade failed: {error}")
            raise

        # rollback: reinstall from the install DMG and verify it matches install evidence.
        try:
            assert installed_bundle is not None
            shutil.rmtree(installed_bundle)
            source_bundle = find_bundle(mount_install, app_name)
            copy_bundle(source_bundle, installed_bundle)
            record_step("rollback", "ok")
        except Exception as error:
            record_step("rollback", "failed", error_class=error.__class__.__name__)
            errors.append(f"rollback failed: {error}")
            raise
        try:
            _, rollback_version, rollback_info_sha, rollback_exec_sha, _ = bundle_metadata(
                installed_bundle
            )
            rollback_aggregate, rollback_file_count = hash_bundle(installed_bundle)
            rollback_matched = (
                rollback_aggregate == install_aggregate
                and rollback_info_sha == install_info_sha
                and rollback_exec_sha == install_exec_sha
            )
            rollback_version_equal = rollback_version == install_version
            record_step(
                "verify_after_rollback", "ok" if rollback_matched else "failed",
                version=rollback_version,
                digest=rollback_aggregate,
                info_sha=rollback_info_sha,
                exec_sha=rollback_exec_sha,
                file_count=rollback_file_count,
                launch_services=lsregister_state(bundle_id, app_name) if bundle_id else {},
                error_class="" if rollback_matched else "RollbackDigestMismatch",
            )
            if not rollback_matched:
                errors.append(
                    "RollbackDigestMismatch: rollback bundle digest does not "
                    "match install evidence"
                )
        except Exception as error:
            record_step("verify_after_rollback", "failed", error_class=error.__class__.__name__)
            errors.append(f"verify_after_rollback failed: {error}")
            raise

        # uninstall
        try:
            assert installed_bundle is not None
            shutil.rmtree(installed_bundle)
            installed_bundle = None
            record_step("uninstall", "ok")
        except OSError as error:
            record_step("uninstall", "failed", error_class=error.__class__.__name__)
            errors.append(f"uninstall failed: {error}")
            raise
        try:
            absent = not (app_dir / app_name).exists()
            record_step(
                "verify_after_uninstall",
                "ok" if absent else "failed",
                launch_services=lsregister_state(bundle_id, app_name) if bundle_id else {},
                error_class="" if absent else "AppStillPresent",
            )
            if not absent:
                errors.append("app bundle still present after uninstall")
        except Exception as error:
            record_step("verify_after_uninstall", "failed", error_class=error.__class__.__name__)
            errors.append(f"verify_after_uninstall failed: {error}")
            raise
    except Exception:
        # Every lifecycle block records its own failed step and re-raises;
        # this guard catches anything that escaped without a record so the
        # artifact never reports failure with an empty cause.
        if not any(step["status"] == "failed" for step in steps):
            errors.append("audit aborted before any step could record the failure")

    cleanup_evidence = cleanup()
    all_steps_ok = all(step["status"] == "ok" for step in steps)
    cleanup_exact = all(cleanup_evidence.values())
    ok = (
        all_steps_ok
        and rollback_matched
        and cleanup_exact
        and not errors
        and bool(steps)
    )
    record_step(
        "cleanup_restore",
        "ok" if cleanup_exact else "failed",
        launch_services=lsregister_state(bundle_id, app_name) if bundle_id else {},
        error_class="" if cleanup_exact else "CleanupFailed",
    )
    if not cleanup_exact:
        errors.append("cleanup did not restore pre-state exactly")

    status = STATUS_VERIFIED if ok else STATUS_FAILED
    artifact = {
        "schema_version": SCHEMA_VERSION,
        "status": status,
        "mode": MODE_EXECUTED,
        "platform": sys.platform,
        "namespace": namespace,
        "candidate_commit": args.candidate_commit,
        "source_digest": args.source_digest,
        "ok": ok,
        "sources": install_sources,
        "steps": steps,
        "rollback": {
            "matched_install_digest": rollback_matched,
            "version_equal_install": rollback_version_equal,
        },
        "cleanup": cleanup_evidence,
        "errors": errors,
    }
    return status, artifact


def blocked_artifact(args: argparse.Namespace, reason: str) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "status": STATUS_BLOCKED,
        "mode": MODE_EXECUTED,
        "platform": sys.platform,
        "namespace": args.namespace,
        "candidate_commit": args.candidate_commit,
        "source_digest": args.source_digest,
        "ok": False,
        "sources": [],
        "steps": [],
        "rollback": {"matched_install_digest": None, "version_equal_install": None},
        "cleanup": {
            "mounts_detached": None,
            "app_removed": None,
            "restored_exact": None,
            "temp_root_removed": None,
        },
        "errors": [reason],
    }


def main(argv: Optional[list[str]] = None) -> int:
    args = parse_arguments(argv)

    if args.check_schema:
        namespace = args.namespace or "default"
        artifact_path = (
            Path(args.artifact) if args.artifact else default_artifact_path(namespace)
        )
        violations = check_artifact_schema(artifact_path)
        if violations:
            for violation in violations:
                print(f"schema violation: {violation}", file=sys.stderr)
            return 2
        print(f"artifact schema valid: {artifact_path.name}")
        return 0

    problems = validate_inputs(args)
    binding_problems = validate_binding(args.candidate_commit, args.source_digest)
    if args.dry_run or not args.execute:
        if problems or binding_problems:
            for problem in problems + binding_problems:
                print(f"dry-run failed: {problem}", file=sys.stderr)
            return 2
        print(
            "dry-run: inputs valid; nothing executed (would run: "
            + " -> ".join(plan_steps(args))
            + f") for namespace '{args.namespace}'"
        )
        return 0

    if problems or binding_problems:
        for problem in problems + binding_problems:
            print(f"validation failed: {problem}", file=sys.stderr)
        return 2

    if sys.platform != "darwin":
        artifact_path = (
            Path(args.artifact) if args.artifact else default_artifact_path(args.namespace)
        )
        reason = (
            "platform-unsupported: --execute requires macOS (darwin), "
            f"host is {sys.platform}"
        )
        write_artifact_atomic(artifact_path, blocked_artifact(args, reason))
        print(
            f"blocked: {reason}; blocked artifact written: {artifact_path.name}",
            file=sys.stderr,
        )
        return 1

    try:
        status, artifact = run_audit(args)
    except Exception as error:  # pragma: no cover - defensive fail-closed
        print(f"audit aborted: {error}", file=sys.stderr)
        return 1
    artifact_path = (
        Path(args.artifact) if args.artifact else default_artifact_path(args.namespace)
    )
    write_artifact_atomic(artifact_path, artifact)
    print(
        f"macOS upgrade/rollback audit: status={status} ok={artifact['ok']} "
        f"artifact={artifact_path.name}"
    )
    return 0 if status == STATUS_VERIFIED else 1


if __name__ == "__main__":
    sys.exit(main())
