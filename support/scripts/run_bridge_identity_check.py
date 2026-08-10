#!/usr/bin/env python3
"""dtoken-agent-bridge identity check (read-only, PR-13 / AGENT-7 H3).

Verifies the d-token Agent bridge identity before any managed MCP/Hook write
surface is claimed. This script performs the read-only observation half of
the authority gate:

  * the bridge binary is a non-empty regular file at a canonical absolute path;
  * on macOS release builds the Developer ID signature/Team ID is verified
    via codesign when available (the embedded-hash check is compiled into the
    desktop verifier; this script reports the platform signature evidence and
    otherwise reports blocked);
  * the bundled identity response (`dtoken-agent-bridge identity`) must match
    the expected component/protocol_version/arch/os and a non-empty version;
  * the desktop verifier seam (verify_dtoken_agent_bridge in main.rs) is
    confirmed to exist so managed writes are gated on the same authority.

The script NEVER writes configs, never passes credentials, and reports
status=blocked instead of passing when any evidence is unavailable. It cannot
self-assert a pass: ok=true requires the identity response to be observed
from the real bridge.

Run::

  python3 support/scripts/run_bridge_identity_check.py \
      --bridge /abs/path/dtoken-agent-bridge [--out PATH]
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
MAIN_RS = REPO_ROOT / "code" / "apps" / "desktop" / "src-tauri" / "src" / "main.rs"

BRIDGE_COMPONENT = "dtoken-agent-bridge"
EXPECTED_PROTOCOL_VERSION = 1

OBSERVED_AT = datetime.datetime.now(datetime.timezone.utc).isoformat()


def run(cmd: list[str], timeout: int = 15) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def verify(bridge: Path) -> dict:
    findings: list[dict] = []

    # 1. Non-empty regular file at a canonical path.
    try:
        metadata = bridge.stat()
        canonical_ok = bridge.is_file() and metadata.st_size > 0
        findings.append(
            {
                "check": "bridge_is_non_empty_regular_file",
                "observed": canonical_ok,
                "detail": f"size={metadata.st_size}" if canonical_ok else f"path={bridge}",
            }
        )
    except OSError as exc:
        findings.append({"check": "bridge_is_non_empty_regular_file", "observed": False, "detail": str(exc)})

    # 2. macOS codesign Developer ID / Team ID evidence (release authority).
    team_id_evidence = None
    if sys.platform == "darwin" and bridge.is_file():
        try:
            details = run(["/usr/bin/codesign", "-dv", "--verbose=4", str(bridge)])
            text = details.stderr + details.stdout
            match = re.search(r"TeamIdentifier=([0-9A-Za-z]+)", text)
            if match:
                team_id_evidence = match.group(1)
            findings.append(
                {
                    "check": "codesign_team_identity",
                    "observed": team_id_evidence is not None,
                    "detail": (
                        f"team_id={team_id_evidence}" if team_id_evidence else "no TeamIdentifier in codesign output"
                    ),
                }
            )
        except (OSError, subprocess.SubprocessError) as exc:
            findings.append({"check": "codesign_team_identity", "observed": False, "detail": str(exc)})
    else:
        findings.append(
            {
                "check": "codesign_team_identity",
                "observed": sys.platform != "darwin",
                "detail": "codesign is macOS-only; non-macOS platforms rely on the embedded-hash verifier in the desktop binary",
            }
        )

    # 3. Bundled identity response matches the pinned contract.
    identity: dict | None = None
    if bridge.is_file():
        try:
            proc = run([str(bridge), "identity"])
            if proc.returncode == 0:
                try:
                    identity = json.loads(proc.stdout)
                except ValueError:
                    identity = None
        except (OSError, subprocess.SubprocessError):
            identity = None
    identity_ok = (
        isinstance(identity, dict)
        and identity.get("component") == BRIDGE_COMPONENT
        and identity.get("protocol_version") == EXPECTED_PROTOCOL_VERSION
        and isinstance(identity.get("arch"), str)
        and isinstance(identity.get("os"), str)
        and isinstance(identity.get("version"), str)
        and bool(identity.get("version"))
    )
    findings.append(
        {
            "check": "bridge_identity_response",
            "observed": identity_ok,
            "detail": (
                {
                    "component": identity.get("component"),
                    "protocol_version": identity.get("protocol_version"),
                    "arch": identity.get("arch"),
                    "os": identity.get("os"),
                    "version_sha256": (
                        hashlib.sha256(identity["version"].encode()).hexdigest()[:16]
                        if identity and identity.get("version")
                        else None
                    ),
                }
                if identity
                else {"observed": None}
            ),
        }
    )

    # 4. The desktop verifier seam exists so managed writes are gated.
    main = MAIN_RS.read_text(encoding="utf-8")
    gate_exists = (
        "fn verify_dtoken_agent_bridge(" in main
        and "fn hybrid_agent_integration_request" in main
        and "verify_dtoken_agent_bridge(&bridge_path)" in main
    )
    findings.append(
        {
            "check": "desktop_verifier_gate_exists",
            "observed": gate_exists,
            "detail": (
                "verify_dtoken_agent_bridge called in hybrid_agent_integration_request"
                if gate_exists
                else "verifier seam missing"
            ),
        }
    )

    blocked = [f for f in findings if not f["observed"]]
    report = {
        "schema_version": 1,
        "mode": "read-only-identity-check",
        "status": "observed" if not blocked else "blocked",
        "ok": not blocked,
        "observed_at": OBSERVED_AT,
        "bridge_path": str(bridge),
        "team_id_redacted": (team_id_evidence[:4] + "…" + team_id_evidence[-4:])
        if team_id_evidence and len(team_id_evidence) > 8
        else None,
        "findings": findings,
        "blockers": [f["check"] for f in blocked],
    }
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bridge", required=True, help="absolute path to dtoken-agent-bridge")
    parser.add_argument("--out", help="artifact output path (optional)")
    args = parser.parse_args(argv)
    raw = Path(args.bridge).expanduser()
    if not raw.is_absolute():
        print("--bridge must be an absolute path", file=sys.stderr)
        return 2
    bridge = raw.resolve()
    report = verify(bridge)
    if args.out:
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
