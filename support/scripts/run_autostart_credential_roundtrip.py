#!/usr/bin/env python3
"""Autostart + OS credential store round-trip audit (PR-8, PLATFORM-1/CONFIG-4).

Runs a unique-isolated, enable/disable round trip against the platform
autostart mechanism and a unique-isolated set/get/delete round trip against
the OS credential store, always under an explicit caller-provided namespace:

  macOS : LaunchAgent (~/Library/LaunchAgents/<ns>.plist + launchctl
          bootstrap/bootout gui/<uid>) and Keychain
          (security add/find/delete-generic-password)
  win32 : HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run value and
          Credential Manager (cmdkey /add /list /delete)

Credentials never leave the process: the Keychain secret is generated in
memory, the store's own retrieval is compared in memory, and the
artifact/stdout only ever carry booleans and entry names — no secret
values, no file contents, no absolute paths, and no plaintext fallback file
is ever written (CONFIG-4). On Windows, Credential Manager exposes no CLI
retrieval, so the cmdkey round trip is presence-verified against the unique
target name (set -> present -> delete -> absent); the secret value is only
ever handed to cmdkey on the real Windows host.

Safety contract (defaults to no side effects)
---------------------------------------------
* Default mode is a pure dry-run: inputs are validated and the exact plan
  is printed; nothing is written to disk, no launchctl/security/reg/cmdkey
  command runs, and no artifact is written. Real execution requires the
  explicit `--execute` flag plus `--namespace`, `--candidate-commit` and
  `--source-digest`.
* Existing same-name entries are refused fail-closed BEFORE any action:
  if the namespace LaunchAgent plist / HKCU Run value / Keychain
  service+account / cmdkey target already exists, the run exits 2 with
  nothing touched. User-owned entries are never modified, read beyond a
  presence probe, or deleted.
* Every entry is derived from the unique namespace; real runs end with the
  entry removed again (exact restore of the empty pre-state). Post-create
  verification failure or delete/restore failure sets ok=false (artifact
  status=failed) and cleanup still runs.
* Platform gate: `--platform win32` executed on a non-Windows host writes a
  blocked artifact (status=blocked, reason=windows-host-unavailable) — real
  Windows evidence is never fabricated, and this host cannot vouch for it.
  Any other platform (e.g. linux) is blocked with platform-unsupported.

Artifact contract (PR-7 aggregator compatible)
----------------------------------------------
Top-level JSON object with exactly these keys: schema_version, status
(verified|failed|blocked), mode (always "executed"; dry-run/check-schema
never write an artifact), platform, namespace, candidate_commit (40-hex git
sha), source_digest (64-hex sha256 source digest of the candidate), ok (bool), autostart,
credential, pre_state, cleanup, errors. Written to
support/artifacts/upgrade-autostart-credential/autostart-credential-roundtrip-<ns>.json
by default. `--check-schema` validates an existing artifact against this
contract and exits without side effects.

Exit codes: 0 verified; 1 failed or blocked; 2 usage/validation error or
failed dry-run (nothing executed in either case).
"""

from __future__ import annotations

import argparse
import json
import os
import plistlib
import re
import secrets
import subprocess
import sys
from pathlib import Path
from typing import Any, Optional

SCHEMA_VERSION = "1.0"
STATUS_VERIFIED = "verified"
STATUS_FAILED = "failed"
STATUS_BLOCKED = "blocked"
MODE_EXECUTED = "executed"
ARTIFACT_DIR_NAME = "upgrade-autostart-credential"
DEFAULT_ARTIFACT_NAME = "autostart-credential-roundtrip-{namespace}.json"
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
        "autostart",
        "credential",
        "pre_state",
        "cleanup",
        "errors",
    }
)
ARTIFACT_STATUSES = frozenset({STATUS_VERIFIED, STATUS_FAILED, STATUS_BLOCKED})
OP_KEYS = frozenset({"ok", "verified"})
GET_KEYS = frozenset({"ok", "secret_matches"})
AUTOSTART_KEYS = frozenset({"provider", "entry", "enable", "disable", "round_trips"})
CREDENTIAL_KEYS = frozenset(
    {"provider", "entry", "set", "get", "delete", "round_trips", "plaintext_fallback"}
)
PRE_STATE_KEYS = frozenset({"entry_present"})
CLEANUP_KEYS = frozenset({"entry_removed", "restored_exact"})
REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ARTIFACT_DIR = REPO_ROOT / "support" / "artifacts" / ARTIFACT_DIR_NAME
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")
NS_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")
HKCU_RUN = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run"



class RoundtripError(Exception):
    """Base class for fail-closed round-trip failures."""


def _run(argv: list[str], **kwargs: Any) -> subprocess.CompletedProcess:
    """Thin subprocess wrapper; tests patch this to inject faults."""
    return subprocess.run(argv, **kwargs)


def _generate_secret() -> str:
    """In-memory secret; never logged, never written to disk."""
    return secrets.token_hex(16)


def resolve_platform(explicit: str) -> str:
    if explicit and explicit != "auto":
        return explicit
    if sys.platform == "darwin":
        return "darwin"
    if sys.platform == "win32":
        return "win32"
    return sys.platform


def launch_agent_label(namespace: str) -> str:
    return f"com.dtoken.audit.{namespace}"


def launch_agent_plist(namespace: str, launch_agents_dir: Path) -> Path:
    return launch_agents_dir / f"{namespace}.plist"


def keychain_service(namespace: str) -> str:
    return f"com.dtoken.audit.{namespace}"


def keychain_account(namespace: str) -> str:
    return f"com.dtoken.audit.{namespace}.acct"


def hkcu_value_name(namespace: str) -> str:
    return namespace


def cmdkey_target(namespace: str) -> str:
    return f"dtoken-audit-{namespace}"


def build_launch_agent_plist(namespace: str) -> bytes:
    """Minimal inert LaunchAgent: no RunAtLoad, no timers; /usr/bin/true."""
    payload = {
        "Label": launch_agent_label(namespace),
        "ProgramArguments": ["/usr/bin/true"],
        "RunAtLoad": False,
    }
    return plistlib.dumps(payload, fmt=plistlib.FMT_XML)


def validate_namespace(namespace: str) -> list[str]:
    problems: list[str] = []
    if not namespace:
        problems.append("--namespace is required")
        return problems
    if not NS_RE.fullmatch(namespace):
        problems.append("--namespace must match [A-Za-z0-9][A-Za-z0-9._-]{0,63}")
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
        prog="run_autostart_credential_roundtrip.py",
        description=(
            "Unique-isolated autostart + OS credential store round trip "
            "(LaunchAgent/HKCU Run enable-disable; Keychain/Credential "
            "Manager set-get-delete). Defaults to a no-side-effect dry-run; "
            "real execution requires --execute plus every explicit parameter."
        ),
    )
    parser.add_argument(
        "--namespace",
        metavar="NS",
        help="unique isolation namespace; every entry name derives from it (required)",
    )
    parser.add_argument(
        "--platform",
        default="auto",
        choices=("auto", "darwin", "win32"),
        help="target platform (default: auto-detect from host)",
    )
    parser.add_argument(
        "--launch-agents-dir",
        metavar="DIR",
        default="",
        help="LaunchAgents directory override (default: ~/Library/LaunchAgents)",
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
        help="run the real round trips (requires --namespace, --candidate-commit, --source-digest)",
    )
    return parser.parse_args(argv)


def default_artifact_path(namespace: str) -> Path:
    return DEFAULT_ARTIFACT_DIR / DEFAULT_ARTIFACT_NAME.format(namespace=namespace)


# --- darwin backends ---------------------------------------------------------


def darwin_plist_present(plist_path: Path) -> bool:
    return plist_path.exists() or plist_path.is_symlink()


def darwin_launchctl_loaded(namespace: str) -> bool:
    """True when the service is currently bootstrapped in the GUI domain."""
    uid = os.getuid()
    result = _run(
        ["launchctl", "print", f"gui/{uid}/{launch_agent_label(namespace)}"],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.returncode == 0


def darwin_enable_launch_agent(namespace: str, plist_path: Path) -> None:
    uid = os.getuid()
    result = _run(
        ["launchctl", "bootstrap", f"gui/{uid}", str(plist_path)],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RoundtripError(f"launchctl bootstrap failed: {result.stderr.strip()[:200]}")


def darwin_disable_launch_agent(namespace: str) -> None:
    uid = os.getuid()
    result = _run(
        ["launchctl", "bootout", f"gui/{uid}/{launch_agent_label(namespace)}"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RoundtripError(f"launchctl bootout failed: {result.stderr.strip()[:200]}")


def darwin_keychain_present(namespace: str) -> bool:
    result = _run(
        [
            "security", "find-generic-password",
            "-s", keychain_service(namespace),
            "-a", keychain_account(namespace),
            "-g",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.returncode == 0


def darwin_keychain_set(namespace: str, secret: str) -> None:
    result = _run(
        [
            "security", "add-generic-password",
            "-s", keychain_service(namespace),
            "-a", keychain_account(namespace),
            "-w", secret,
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RoundtripError(f"security add-generic-password failed: {result.stderr.strip()[:200]}")


def darwin_keychain_get(namespace: str) -> Optional[str]:
    """Return the stored secret or None when the entry is absent."""
    result = _run(
        [
            "security", "find-generic-password",
            "-s", keychain_service(namespace),
            "-a", keychain_account(namespace),
            "-w",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return None
    return result.stdout.rstrip("\n")


def darwin_keychain_delete(namespace: str) -> None:
    result = _run(
        [
            "security", "delete-generic-password",
            "-s", keychain_service(namespace),
            "-a", keychain_account(namespace),
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RoundtripError(f"security delete-generic-password failed: {result.stderr.strip()[:200]}")


# --- win32 backends ----------------------------------------------------------


def win32_run_present(namespace: str) -> bool:
    result = _run(
        ["reg", "query", HKCU_RUN, "/v", hkcu_value_name(namespace)],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.returncode == 0


def win32_enable_run(namespace: str) -> None:
    result = _run(
        [
            "reg", "add", HKCU_RUN,
            "/v", hkcu_value_name(namespace),
            "/t", "REG_SZ",
            "/d", f"dtoken-audit-marker-{namespace}",
            "/f",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RoundtripError(f"reg add failed: {result.stderr.strip()[:200]}")


def win32_disable_run(namespace: str) -> None:
    result = _run(
        ["reg", "delete", HKCU_RUN, "/v", hkcu_value_name(namespace), "/f"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RoundtripError(f"reg delete failed: {result.stderr.strip()[:200]}")


def win32_cmdkey_present(namespace: str) -> bool:
    result = _run(
        ["cmdkey", f"/list:{cmdkey_target(namespace)}"],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.returncode == 0


def win32_cmdkey_set(namespace: str, secret: str) -> None:
    result = _run(
        [
            "cmdkey",
            f"/add:{cmdkey_target(namespace)}",
            "/user:dtoken-audit-user",
            f"/pass:{secret}",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RoundtripError(f"cmdkey /add failed: {result.stderr.strip()[:200]}")


def win32_cmdkey_get(namespace: str) -> bool:
    """Credential Manager exposes no password retrieval on the CLI; the
    /list evidence proves the entry exists with the expected target name."""
    return win32_cmdkey_present(namespace)


def win32_cmdkey_delete(namespace: str) -> None:
    result = _run(
        ["cmdkey", f"/delete:{cmdkey_target(namespace)}"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RoundtripError(f"cmdkey /delete failed: {result.stderr.strip()[:200]}")


# --- generic orchestration ---------------------------------------------------


def run_autostart_roundtrip(
    platform: str,
    namespace: str,
    launch_agents_dir: Path,
    errors: list[str],
) -> dict[str, Any]:
    """Enable -> verify -> disable -> verify -> enable -> verify -> disable(finish).
    Returns the autostart evidence record; failures are recorded and cleaned up.
    """
    if platform == "darwin":
        provider = "LaunchAgent"
        entry = f"{namespace}.plist"
        plist_path = launch_agent_plist(namespace, launch_agents_dir)

        def present() -> bool:
            return darwin_plist_present(plist_path) or darwin_launchctl_loaded(namespace)

        def enable() -> None:
            plist_path.parent.mkdir(parents=True, exist_ok=True)
            tmp = plist_path.with_name(plist_path.name + ".tmp")
            tmp.write_bytes(build_launch_agent_plist(namespace))
            os.chmod(tmp, 0o644)
            os.replace(tmp, plist_path)
            darwin_enable_launch_agent(namespace, plist_path)

        def disable() -> None:
            try:
                darwin_disable_launch_agent(namespace)
            finally:
                if plist_path.exists() or plist_path.is_symlink():
                    plist_path.unlink()
    else:  # win32
        provider = "HKCU-Run"
        entry = f"HKCU\\...\\Run\\{hkcu_value_name(namespace)}"

        def present() -> bool:
            return win32_run_present(namespace)

        def enable() -> None:
            win32_enable_run(namespace)

        def disable() -> None:
            win32_disable_run(namespace)

    enable_ok: list[bool] = []
    disable_ok: list[bool] = []
    for cycle in range(2):
        try:
            enable()
            enabled = present()
            enable_ok.append(enabled)
            if not enabled:
                errors.append(
                    f"autostart enable verification failed (cycle {cycle + 1}): "
                    f"entry not present after enable"
                )
        except RoundtripError as error:
            enable_ok.append(False)
            errors.append(f"autostart enable failed (cycle {cycle + 1}): {error}")
        try:
            disable()
            disabled = not present()
            disable_ok.append(disabled)
            if not disabled:
                errors.append(
                    f"autostart disable verification failed (cycle {cycle + 1}): "
                    f"entry still present after disable"
                )
        except RoundtripError as error:
            disable_ok.append(False)
            errors.append(f"autostart disable failed (cycle {cycle + 1}): {error}")

    return {
        "provider": provider,
        "entry": entry,
        "enable": {"ok": all(enable_ok), "verified": all(enable_ok)},
        "disable": {"ok": all(disable_ok), "verified": all(disable_ok)},
        "round_trips": 2,
    }


def run_credential_roundtrip(
    platform: str,
    namespace: str,
    errors: list[str],
) -> dict[str, Any]:
    """set -> verify -> delete -> verify -> set -> verify -> delete(finish).
    Never prints or persists the secret; returns evidence booleans only.
    """
    if platform == "darwin":
        provider = "Keychain"
        entry = f"{keychain_service(namespace)}/{keychain_account(namespace)}"

        def set_entry(secret: str) -> None:
            darwin_keychain_set(namespace, secret)

        def get_entry() -> Optional[str]:
            return darwin_keychain_get(namespace)

        def delete_entry() -> None:
            darwin_keychain_delete(namespace)
    else:  # win32
        provider = "Credential-Manager"
        entry = cmdkey_target(namespace)

        def set_entry(secret: str) -> None:
            win32_cmdkey_set(namespace, secret)

        def get_entry() -> Optional[str]:
            return "present" if win32_cmdkey_get(namespace) else None

        def delete_entry() -> None:
            win32_cmdkey_delete(namespace)

    set_ok: list[bool] = []
    get_matches: list[bool] = []
    delete_ok: list[bool] = []
    for cycle in range(2):
        secret = _generate_secret()
        try:
            set_entry(secret)
            stored = get_entry()
            if platform == "win32":
                # Credential Manager exposes no CLI retrieval: the round
                # trip is presence-verified against the unique target name
                # (set -> present -> delete -> absent). The secret value is
                # only ever handed to cmdkey on the real Windows host.
                matches = stored is not None
            else:
                matches = stored is not None and stored == secret
            set_ok.append(stored is not None)
            get_matches.append(matches)
            if stored is None:
                errors.append(
                    f"credential set verification failed (cycle {cycle + 1}): "
                    f"entry not found after set"
                )
            elif not matches:
                errors.append(
                    f"credential get verification failed (cycle {cycle + 1}): "
                    f"stored secret does not match"
                )
        except RoundtripError as error:
            set_ok.append(False)
            get_matches.append(False)
            errors.append(f"credential set failed (cycle {cycle + 1}): {error}")
        try:
            delete_entry()
            gone = get_entry() is None
            delete_ok.append(gone)
            if not gone:
                errors.append(
                    f"credential delete verification failed (cycle {cycle + 1}): "
                    f"entry still present after delete"
                )
        except RoundtripError as error:
            delete_ok.append(False)
            errors.append(f"credential delete failed (cycle {cycle + 1}): {error}")

    return {
        "provider": provider,
        "entry": entry,
        "set": {"ok": all(set_ok), "verified": all(set_ok)},
        "get": {"ok": all(get_matches), "secret_matches": all(get_matches)},
        "delete": {"ok": all(delete_ok), "verified": all(delete_ok)},
        "round_trips": 2,
        "plaintext_fallback": False,
    }


def _bool_or_blocked_null(value: Any, status: Any, name: str, violations: list[str]) -> None:
    """Booleans are required for executed runs; blocked runs may record
    null for facts that were never probed (no false claims of evidence)."""
    if value is None and status == STATUS_BLOCKED:
        return
    if not isinstance(value, bool):
        violations.append(f"{name} must be a boolean"
                          + ("" if status == STATUS_BLOCKED else " (or null only for blocked artifacts)"))


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
    if data.get("platform") not in {"darwin", "win32", "linux"}:
        violations.append("platform must be darwin, win32 or linux")
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
    autostart = data.get("autostart")
    if not isinstance(autostart, dict) or set(autostart.keys()) != AUTOSTART_KEYS:
        violations.append(f"autostart keys must be exactly {sorted(AUTOSTART_KEYS)}")
    else:
        for op in ("enable", "disable"):
            record = autostart.get(op)
            if not isinstance(record, dict) or set(record.keys()) != OP_KEYS:
                violations.append(f"autostart.{op} keys must be exactly {sorted(OP_KEYS)}")
        if not isinstance(autostart.get("round_trips"), int):
            violations.append("autostart.round_trips must be an integer")
    credential = data.get("credential")
    if not isinstance(credential, dict) or set(credential.keys()) != CREDENTIAL_KEYS:
        violations.append(f"credential keys must be exactly {sorted(CREDENTIAL_KEYS)}")
    else:
        for op in ("set", "delete"):
            record = credential.get(op)
            if not isinstance(record, dict) or set(record.keys()) != OP_KEYS:
                violations.append(f"credential.{op} keys must be exactly {sorted(OP_KEYS)}")
        get_record = credential.get("get")
        if not isinstance(get_record, dict) or set(get_record.keys()) != GET_KEYS:
            violations.append(f"credential.get keys must be exactly {sorted(GET_KEYS)}")
        if credential.get("plaintext_fallback") is not False:
            violations.append("credential.plaintext_fallback must be false")
    pre_state = data.get("pre_state")
    if not isinstance(pre_state, dict) or set(pre_state.keys()) != PRE_STATE_KEYS:
        violations.append(f"pre_state keys must be exactly {sorted(PRE_STATE_KEYS)}")
    else:
        _bool_or_blocked_null(
            pre_state.get("entry_present"), data.get("status"),
            "pre_state.entry_present", violations,
        )
    cleanup = data.get("cleanup")
    if not isinstance(cleanup, dict) or set(cleanup.keys()) != CLEANUP_KEYS:
        violations.append(f"cleanup keys must be exactly {sorted(CLEANUP_KEYS)}")
    else:
        for name in ("entry_removed", "restored_exact"):
            _bool_or_blocked_null(
                cleanup.get(name), data.get("status"), f"cleanup.{name}", violations,
            )
    if not isinstance(data.get("errors"), list):
        violations.append("errors must be a list")
    return violations


def entry_present(platform: str, namespace: str, launch_agents_dir: Path) -> bool:
    """Presence probe used by pre-flight refusal and final restore verification.

    On darwin the probe covers all three persistence surfaces of the
    namespace entry: the LaunchAgent plist file, a still-bootstrapped
    same-label service (e.g. leftover from a crashed prior run), and the
    Keychain service+account pair.
    """
    if platform == "darwin":
        return (
            darwin_plist_present(launch_agent_plist(namespace, launch_agents_dir))
            or darwin_launchctl_loaded(namespace)
            or darwin_keychain_present(namespace)
        )
    return win32_run_present(namespace) or win32_cmdkey_present(namespace)


def plan_text(platform: str, namespace: str) -> str:
    if platform == "darwin":
        return (
            "LaunchAgent enable->disable x2 + Keychain set->get->delete x2 "
            f"for namespace '{namespace}'"
        )
    if platform == "win32":
        return (
            "HKCU Run enable->disable x2 + Credential Manager set->get->delete x2 "
            f"for namespace '{namespace}'"
        )
    return f"no round trips defined for platform '{platform}'"


def blocked_artifact(args: argparse.Namespace, platform: str, reason: str) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "status": STATUS_BLOCKED,
        "mode": MODE_EXECUTED,
        "platform": platform,
        "namespace": args.namespace,
        "candidate_commit": args.candidate_commit,
        "source_digest": args.source_digest,
        "ok": False,
        "autostart": {
            "provider": "", "entry": "", "enable": {"ok": False, "verified": False},
            "disable": {"ok": False, "verified": False}, "round_trips": 0,
        },
        "credential": {
            "provider": "", "entry": "", "set": {"ok": False, "verified": False},
            "get": {"ok": False, "secret_matches": False},
            "delete": {"ok": False, "verified": False},
            "round_trips": 0, "plaintext_fallback": False,
        },
        "pre_state": {"entry_present": None},
        "cleanup": {"entry_removed": None, "restored_exact": None},
        "errors": [reason],
    }


def main(argv: Optional[list[str]] = None) -> int:
    args = parse_arguments(argv)
    namespace = args.namespace or ""
    launch_agents_dir = (
        Path(args.launch_agents_dir) if args.launch_agents_dir
        else Path.home() / "Library" / "LaunchAgents"
    )

    if args.check_schema:
        artifact_path = (
            Path(args.artifact) if args.artifact else default_artifact_path(namespace or "default")
        )
        violations = check_artifact_schema(artifact_path)
        if violations:
            for violation in violations:
                print(f"schema violation: {violation}", file=sys.stderr)
            return 2
        print(f"artifact schema valid: {artifact_path.name}")
        return 0

    ns_problems = validate_namespace(namespace)
    binding_problems = validate_binding(args.candidate_commit, args.source_digest)
    platform = resolve_platform(args.platform)

    dry_run = args.dry_run or not args.execute
    if dry_run:
        if ns_problems or binding_problems:
            for problem in ns_problems + binding_problems:
                print(f"dry-run failed: {problem}", file=sys.stderr)
            return 2
        if platform not in {"darwin", "win32"}:
            print(
                f"dry-run blocked: no round trips defined for platform "
                f"'{platform}' (host {sys.platform}); nothing executed",
                file=sys.stderr,
            )
            return 2
        print(
            f"dry-run: inputs valid; nothing executed (would run: {plan_text(platform, namespace)})"
        )
        return 0

    if ns_problems or binding_problems:
        for problem in ns_problems + binding_problems:
            print(f"validation failed: {problem}", file=sys.stderr)
        return 2

    if platform not in {"darwin", "win32"}:
        artifact_path = (
            Path(args.artifact) if args.artifact else default_artifact_path(namespace)
        )
        reason = f"platform-unsupported: platform '{platform}' has no round-trip definition"
        write_artifact_atomic(artifact_path, blocked_artifact(args, platform, reason))
        print(f"blocked: {reason}; blocked artifact written: {artifact_path.name}", file=sys.stderr)
        return 1

    # Real Windows evidence requires a real Windows host; never fabricate it.
    if platform == "win32" and sys.platform != "win32":
        artifact_path = (
            Path(args.artifact) if args.artifact else default_artifact_path(namespace)
        )
        reason = (
            "windows-host-unavailable: --platform win32 executed on "
            f"{sys.platform}; real Windows evidence is external/blocked, "
            "not fabricated"
        )
        write_artifact_atomic(artifact_path, blocked_artifact(args, platform, reason))
        print(
            f"blocked: {reason}; blocked artifact written: {artifact_path.name}",
            file=sys.stderr,
        )
        return 1

    if platform == "darwin" and sys.platform != "darwin":
        artifact_path = (
            Path(args.artifact) if args.artifact else default_artifact_path(namespace)
        )
        reason = (
            f"platform-unsupported: --platform darwin executed on {sys.platform}"
        )
        write_artifact_atomic(artifact_path, blocked_artifact(args, platform, reason))
        print(
            f"blocked: {reason}; blocked artifact written: {artifact_path.name}",
            file=sys.stderr,
        )
        return 1

    # Pre-flight refusal: an existing same-name entry must never be touched.
    try:
        already_present = entry_present(platform, namespace, launch_agents_dir)
    except OSError as error:
        print(f"validation failed: cannot probe entry state: {error}", file=sys.stderr)
        return 2
    if already_present:
        print(
            "validation failed: refusing existing same-name entry "
            f"(fail-closed, nothing will be touched): namespace '{namespace}'",
            file=sys.stderr,
        )
        return 2

    errors: list[str] = []
    autostart = run_autostart_roundtrip(platform, namespace, launch_agents_dir, errors)
    credential = run_credential_roundtrip(platform, namespace, errors)

    # Exact restore verification: both stores must be empty again.
    try:
        final_present = entry_present(platform, namespace, launch_agents_dir)
    except OSError:
        final_present = True
        errors.append("final restore verification could not probe entry state")
    entry_removed = not final_present
    restored_exact = entry_removed and not errors
    if not entry_removed:
        errors.append("cleanup failed: namespace entry still present after round trip")
    if not restored_exact:
        errors.append("cleanup did not restore pre-state exactly")

    ok = (
        autostart["enable"]["ok"]
        and autostart["disable"]["ok"]
        and credential["set"]["ok"]
        and credential["get"]["ok"]
        and credential["delete"]["ok"]
        and entry_removed
        and not errors
    )
    status = STATUS_VERIFIED if ok else STATUS_FAILED
    artifact = {
        "schema_version": SCHEMA_VERSION,
        "status": status,
        "mode": MODE_EXECUTED,
        "platform": platform,
        "namespace": namespace,
        "candidate_commit": args.candidate_commit,
        "source_digest": args.source_digest,
        "ok": ok,
        "autostart": autostart,
        "credential": credential,
        "pre_state": {"entry_present": False},
        "cleanup": {"entry_removed": entry_removed, "restored_exact": restored_exact},
        "errors": errors,
    }
    artifact_path = (
        Path(args.artifact) if args.artifact else default_artifact_path(namespace)
    )
    write_artifact_atomic(artifact_path, artifact)
    print(
        f"autostart/credential round trip: status={status} ok={ok} "
        f"artifact={artifact_path.name}"
    )
    return 0 if status == STATUS_VERIFIED else 1


if __name__ == "__main__":
    sys.exit(main())
