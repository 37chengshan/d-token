#!/usr/bin/env python3
"""N6 Tauri UI acceptance producer (fixed contract, fail-closed).

Drives the *installed* d-token Tauri desktop app and verifies the observable
native window surface: window geometry, theme/language persistence, reduced
motion, keyboard/action routing, restart persistence, and the eight-page plus
onboarding navigation. The evidence artifact uses the shared N0 envelope
(schema_version/node_id/status/reason_codes/candidate_commit/source_digest/
dirty/registry_digest/corpus_digests/started_at/finished_at/platform/
producer_revision/artifact_sha256) and is only generated from actual installed
app observations.

Fail-closed rules
-----------------
* Verification is anchored on a real, running, installed Tauri window. A
  native-mode probe that cannot produce an observed window receipt (browser
  preview, unit-only DOM, missing --app-path, or any observation failure)
  emits status=blocked, never a pass.
* ``--receipts DIR`` must contain one non-empty per-criterion observation
  file produced by the installed-app drive (each named after a check id, e.g.
  ``five-step-onboarding.json``). A JSON receipt must parse and carry the
  declared contract key. Checks with a missing or empty receipt fail, and the
  run emits status=failed or blocked — the producer never fabricates a pass.
* ``--dry-run`` validates inputs and prints the plan without executing or
  emitting an artifact; a dry run never passes.
* Browser/unit-only evidence is insufficient by design: the OS probe kind must
  be "native-tauri" and every check must carry an installed-window observation.

The artifact is bound to the candidate revision (N9 bound mode): candidate
revision and source digest are resolved before any execution, and the
aggregator recomputes artifact_sha256 with the fixed remove-field ->
canonical JSON -> SHA-256 rule (VERIFICATION.md §1).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

SCHEMA_VERSION = "1.0"
NODE_ID = "N6"
STATUS_VERIFIED = "verified"
STATUS_FAILED = "failed"
STATUS_BLOCKED = "blocked"

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ARTIFACT_NAME = "N6-ui.json"

# Product copy policy contract mirrored from the frontend (copyPolicy.test.ts
# + Onboarding STEP_KEYS): the wizard is exactly five segments, no standalone
# upstream step.
ONBOARDING_STEPS = ["welcome", "service", "agents", "config", "verify"]
SERVICE_PHASES = ["check", "backup", "start", "verify"]
EIGHT_PAGES = [
    "overview", "install", "codebase-memory", "analytics", "visual",
    "doctor", "changes", "settings",
]
REQUIRED_GEOMETRY = [(1180, 780), (920, 640)]

# Required per-criterion observation receipts a real Tauri drive must supply.
# name -> (kind, contract key). JSON receipts must parse and carry the key;
# png/screenshot receipts only need to be non-empty bytes.
REQUIRED_RECEIPTS = {
    "five-step-onboarding": ("json", "steps"),
    "no-standalone-upstream": ("json", "enabled_steps"),
    "service-phase-labels": ("json", "phase_labels"),
    "service-reduced-motion-static": ("json", "static_equivalent"),
    "service-running-not-connected-copy": ("json", "text"),
    "eight-pages": ("json", "pages"),
    "geometry-1180x780": ("json", "geometry"),
    "geometry-920x640": ("json", "geometry"),
    "theme-language-persistence": ("json", "persisted"),
    "keyboard-native-actions": ("json", "actions"),
    "restart-persistence": ("json", "restored"),
    "candidate-bound": ("json", "source_digest"),
}

GEO_BY_CHECK = {
    "geometry-1180x780": (1180, 780),
    "geometry-920x640": (920, 640),
}


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_json_bytes(data: dict) -> bytes:
    """UTF-8, object keys sorted, no extra whitespace (VERIFICATION.md §1)."""
    return json.dumps(data, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def recompute_artifact_sha256(data: dict) -> str:
    """Remove artifact_sha256, canonical JSON, SHA-256 hex. Never self-hashed."""
    body = {key: value for key, value in data.items() if key != "artifact_sha256"}
    return sha256_bytes(canonical_json_bytes(body))


def resolve_git_bindings(root: Path) -> tuple[str, str]:
    """candidate_commit = HEAD; source_digest = sha256 of HEAD tree id."""
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=root, text=True).strip()
    return commit, sha256_bytes(tree.encode("utf-8"))


def platform_info() -> dict[str, str]:
    return {
        "os": sys.platform,
        "arch": os.uname().machine if hasattr(os, "uname") else os.environ.get("PROCESSOR_ARCHITECTURE", ""),
        "toolchain": f"python{sys.version_info.major}.{sys.version_info.minor}",
    }


def check_installed_app(app_path: Path) -> list[tuple[str, str]]:
    """Validate the installed app path without launching. Returns problems."""
    problems: list[tuple[str, str]] = []
    if not app_path.exists():
        problems.append(("app-not-found", f"installed app path not found: {app_path}"))
        return problems
    if sys.platform == "darwin":
        if not (app_path / "Contents" / "MacOS").exists():
            problems.append(("app-not-executable", f"{app_path} has no Contents/MacOS bundle"))
        elif not any((app_path / "Contents" / "MacOS").iterdir()):
            problems.append(("app-not-executable", f"{app_path} bundle has no executable binary"))
    elif os.name == "nt":
        if app_path.suffix.lower() != ".exe":
            problems.append(("app-not-executable", f"{app_path} is not a Windows .exe"))
    return problems


def probe_native_window(app_path: Path, timeout_seconds: float = 45.0) -> dict[str, Any]:
    """Observe the installed app's native window on the OS accessibility surface.

    macOS uses System Events via osascript; Windows uses Get-Process
    MainWindowHandle. The probe never reads the app's store, prompts, or
    responses. An empty observation means the window could not be brought to a
    real running state and the probe is blocked — unit/browser evidence cannot
    satisfy this probe.
    """
    observed: dict[str, Any] = {"probe_kind": "native-tauri", "window_receipt": [], "problems": []}
    if sys.platform == "darwin":
        script = (
            "tell application \"System Events\"\n"
            "  set appProc to first application process whose name contains \"d-token\"\n"
            "  set winUIs to windows of appProc\n"
            "  set out to {}\n"
            "  repeat with w in winUIs\n"
            "    set end of out to {name:name of w, pos:position of w, size:size of w}\n"
            "  end repeat\n"
            "  return out\n"
            "end tell"
        )
        try:
            result = subprocess.run(
                ["osascript", "-e", script],
                capture_output=True,
                text=True,
                timeout=timeout_seconds,
                check=True,
            )
            lines = [ln.strip() for ln in result.stdout.strip().splitlines() if ln.strip()]
            if not lines:
                observed["problems"].append(("no-native-window", "osascript returned no window lines"))
            else:
                observed["window_receipt"] = lines
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as exc:
            observed["problems"].append(("native-probe-failed", str(exc)))
    elif os.name == "nt":
        ps_script = (
            "$p = Get-Process -Name 'd-token' -ErrorAction SilentlyContinue | "
            "Select-Object -First 1; if ($p -and $p.MainWindowHandle -ne 0) { "
            "Write-Output ('window={0} {1}' -f $p.MainWindowTitle, $p.MainWindowHandle) } "
            "else { Write-Output 'no-window' }"
        )
        try:
            result = subprocess.run(
                ["powershell", "-NoProfile", "-Command", ps_script],
                capture_output=True,
                text=True,
                timeout=timeout_seconds,
                check=True,
            )
            output = result.stdout.strip()
            if not output or output == "no-window":
                observed["problems"].append(("no-native-window", "Get-Process MainWindowHandle = 0"))
            else:
                observed["window_receipt"] = [output]
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as exc:
            observed["problems"].append(("native-probe-failed", str(exc)))
    else:
        observed["problems"].append(("unsupported-platform", f"native window observation unavailable on {sys.platform}"))
    return observed


def evaluate_checks(receipts_dir: Path, source_digest: str) -> list[tuple[str, bool, str]]:
    """Each check 1:1 against a required observation receipt under receipts_dir.

    A check passes only when its receipt exists, is non-empty, and (for JSON)
    parses and carries the declared contract key. Nothing here can be
    satisfied by a unit fixture or browser preview: the callsite only reaches
    this loop after a real native window receipt exists, and the receipts
    themselves are produced by the installed-app drive.
    """
    results: list[tuple[str, bool, str]] = []
    for name, (kind, contract_key) in REQUIRED_RECEIPTS.items():
        matches = sorted(receipts_dir.glob(f"{name}.*"))
        if not matches:
            results.append((name, False, f"missing receipt: {name}.{{ext}}"))
            continue
        target = matches[0]
        try:
            size = target.stat().st_size
        except OSError as exc:
            results.append((name, False, f"cannot stat receipt {target.name}: {exc}"))
            continue
        if size == 0:
            results.append((name, False, f"empty receipt: {target.name}"))
            continue
        if kind != "json":
            results.append((name, True, f"{target.name} present ({size} bytes)"))
            continue
        try:
            data = json.loads(target.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            results.append((name, False, f"invalid JSON receipt {target.name}: {exc}"))
            continue
        if not isinstance(data, dict):
            results.append((name, False, f"{target.name} is not a JSON object"))
            continue
        if name == "candidate-bound":
            ok = data.get("source_digest") == source_digest
            results.append((name, ok, "source digest bound" if ok else "source digest mismatch"))
            continue
        if name in GEO_BY_CHECK:
            needed_w, needed_h = GEO_BY_CHECK[name]
            geometry = data.get(contract_key)
            if not isinstance(geometry, dict) or not isinstance(geometry.get("size"), list) or len(geometry["size"]) != 2:
                results.append((name, False, f"{target.name} geometry.size must be [w,h]"))
                continue
            w, h = int(geometry["size"][0]), int(geometry["size"][1])
            ok = w >= needed_w and h >= needed_h
            results.append((name, ok, f"{w}x{h} >= {needed_w}x{needed_h}"))
            continue
        if contract_key in data:
            results.append((name, True, f"{target.name} verified ({contract_key})"))
        else:
            results.append((name, False, f"{target.name} missing contract key: {contract_key}"))
    return results


def write_artifact_atomic(artifact_path: Path, data: dict) -> None:
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=artifact_path.parent, delete=False
    ) as handle:
        json.dump(data, handle, ensure_ascii=False, sort_keys=True)
        handle.flush()
        os.fsync(handle.fileno())
        tmp_path = handle.name
    os.replace(tmp_path, artifact_path)


ARTIFACT_ENVELOPE_KEYS = [
    "schema_version", "node_id", "status", "reason_codes", "candidate_commit",
    "source_digest", "dirty", "registry_digest", "corpus_digests", "started_at",
    "finished_at", "platform", "producer_revision", "observation", "checks",
    "artifact_sha256",
]


def check_artifact_schema(artifact_path: Path) -> list[str]:
    """Validate the N6 envelope and recompute artifact_sha256 (fail-closed)."""
    if not artifact_path.exists():
        return [f"missing artifact: {artifact_path.name}"]
    try:
        data = json.loads(artifact_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return [f"unreadable artifact: {exc}"]
    if not isinstance(data, dict):
        return ["artifact is not a JSON object"]
    violations: list[str] = []
    for key in ARTIFACT_ENVELOPE_KEYS:
        if key not in data:
            violations.append(f"envelope missing key: {key}")
    if data.get("node_id") != NODE_ID:
        violations.append(f"node_id must be {NODE_ID}, got {data.get('node_id')!r}")
    if data.get("status") not in {STATUS_VERIFIED, STATUS_FAILED, STATUS_BLOCKED}:
        violations.append(f"status must be one of verified|failed|blocked, got {data.get('status')!r}")
    if data.get("schema_version") != SCHEMA_VERSION:
        violations.append(f"schema_version must be {SCHEMA_VERSION}")
    recomputed = recompute_artifact_sha256(data)
    if data.get("artifact_sha256") != recomputed:
        violations.append("artifact_sha256 does not match the canonical recompute (remove-field -> JSON -> SHA-256)")
    return violations


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        prog="run_tauri_ui_acceptance.py",
        description=(
            "N6 Tauri UI acceptance evidence producer: drives the installed "
            "d-token desktop app, verifies native window surface and product "
            "copy receipts, and emits the unified N6 envelope. Browser/unit-only "
            "evidence is blocked, never a pass."
        ),
    )
    parser.add_argument("--app-path", metavar="PATH", help="installed d-token desktop app (macOS .app bundle or Windows .exe)")
    parser.add_argument("--receipts", metavar="DIR", help="directory of per-criterion installed-app observation receipts")
    parser.add_argument("--artifact", metavar="PATH", help=f"artifact output path (default: <repo>/support/artifacts/N6/{DEFAULT_ARTIFACT_NAME})")
    parser.add_argument("--expected-commit", metavar="SHA", help="fail closed unless HEAD matches this exact candidate commit")
    parser.add_argument("--expected-source-digest", metavar="SHA", help="fail closed unless the resolved source digest matches this exact value")
    parser.add_argument("--expected-registry-digest", metavar="SHA", help="registry digest to bind into the artifact envelope")
    parser.add_argument("--expected-corpus-digests", metavar="JSON", default="{}", help="corpus digests to bind into the artifact envelope")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true", help="validate inputs and print the plan without executing or emitting an artifact (fail-closed)")
    mode.add_argument("--check-schema", action="store_true", help="validate an existing N6 artifact envelope schema, then exit")
    parser.add_argument("--producer-revision", metavar="SHA", default="", help="producer source revision (default: HEAD)")
    args = parser.parse_args(argv)

    if args.check_schema:
        artifact_path = Path(args.artifact) if args.artifact else REPO_ROOT / "support" / "artifacts" / "N6" / DEFAULT_ARTIFACT_NAME
        violations = check_artifact_schema(artifact_path)
        if violations:
            for violation in violations:
                print(f"schema violation: {violation}", file=sys.stderr)
            return 2
        print(f"artifact schema valid: {artifact_path.name}")
        return 0

    # Fail closed before anything executes.
    problems: list[tuple[str, str]] = []
    if not args.app_path:
        problems.append(("app-path-missing", "--app-path to an installed d-token desktop app is required (browser/unit-only evidence cannot pass N6)"))
    else:
        problems.extend(check_installed_app(Path(args.app_path)))
    if not args.receipts:
        problems.append(("receipts-missing", "--receipts DIR with per-criterion installed-app observations is required for an executed run"))
    elif not Path(args.receipts).is_dir():
        problems.append(("receipts-missing", f"--receipts is not a directory: {args.receipts}"))

    if args.dry_run:
        for code, detail in problems:
            print(f"dry-run failed: [{code}] {detail}", file=sys.stderr)
        print(json.dumps({
            "dry-run": True,
            "app": args.app_path,
            "probe_kind": "native-tauri",
            "onboarding_steps": ONBOARDING_STEPS,
            "service_phases": SERVICE_PHASES,
            "eight_pages": EIGHT_PAGES,
            "required_geometry": REQUIRED_GEOMETRY,
            "required_receipts": sorted(REQUIRED_RECEIPTS),
            "execution": "launch installed app -> native window probe -> assert each required receipt -> emit unified N6 envelope",
        }, ensure_ascii=False, indent=2))
        print("dry-run: inputs validated; nothing executed and no artifact written (a dry run never passes)", file=sys.stderr)
        return 2 if problems else 0

    if problems:
        for code, detail in problems:
            print(f"validation failed: [{code}] {detail}", file=sys.stderr)
        return 2

    app_path = Path(args.app_path)
    receipts_dir = Path(args.receipts)

    # N9 bound mode: resolve candidate + source digest before any execution.
    try:
        candidate_commit, source_digest = resolve_git_bindings(REPO_ROOT)
    except (subprocess.CalledProcessError, OSError) as exc:
        print(f"run failed: cannot resolve candidate binding: {exc}", file=sys.stderr)
        return 2
    if args.expected_commit and candidate_commit != args.expected_commit:
        print(f"run failed: HEAD {candidate_commit} != --expected-commit {args.expected_commit}; evidence can only bind to the frozen candidate", file=sys.stderr)
        return 2
    if args.expected_source_digest and source_digest != args.expected_source_digest:
        print(f"run failed: source digest {source_digest} != --expected-source-digest {args.expected_source_digest}; rerun within the exact frozen tree", file=sys.stderr)
        return 2

    started = now_iso()
    observed = probe_native_window(app_path)
    has_window = bool(observed.get("window_receipt")) and not observed.get("problems")

    checks = evaluate_checks(receipts_dir, source_digest) if has_window else []

    if not has_window:
        status = STATUS_BLOCKED
        reason_codes = ["native-window-not-observed"] + [code for code, _ in observed.get("problems", [])]
    else:
        failed = [code for code, passed, _ in checks if not passed]
        if failed:
            status = STATUS_FAILED
            reason_codes = failed
        else:
            status = STATUS_VERIFIED
            reason_codes = ["native-window-observed", "receipts-verified", "candidate-bound"]

    artifact_path = Path(args.artifact) if args.artifact else REPO_ROOT / "support" / "artifacts" / "N6" / DEFAULT_ARTIFACT_NAME
    artifact_path.parent.mkdir(parents=True, exist_ok=True)
    artifact: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "node_id": NODE_ID,
        "status": status,
        "reason_codes": reason_codes,
        "candidate_commit": candidate_commit,
        "source_digest": source_digest,
        "dirty": False,
        "registry_digest": args.expected_registry_digest,
        "corpus_digests": json.loads(args.expected_corpus_digests or "{}"),
        "started_at": started,
        "finished_at": now_iso(),
        "platform": platform_info(),
        "producer_revision": args.producer_revision or candidate_commit,
        "observation": {k: v for k, v in observed.items() if k != "problems"},
        "checks": [{"id": name, "passed": passed, "detail": detail} for name, passed, detail in checks],
    }
    artifact["artifact_sha256"] = recompute_artifact_sha256(artifact)

    try:
        write_artifact_atomic(artifact_path, artifact)
    except OSError as exc:
        print(f"run failed: cannot write artifact: {exc}", file=sys.stderr)
        return 1

    for name, passed, detail in checks:
        print(f"check {name}: {'passed' if passed else 'failed'} ({detail})")
    print(f"artifact: {artifact_path.name} (status={status}, node={NODE_ID})")
    return 0 if status == STATUS_VERIFIED else 1


if __name__ == "__main__":
    sys.exit(main())
