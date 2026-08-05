#!/usr/bin/env python3
"""Cross-platform G0-G8 release gate status aggregator (fail-closed).

This aggregator maps the Headroom-migration release gates defined in
docs/PLAN.md §1C.2 (G0-G8) to concrete evidence artifacts under
support/artifacts/ and reports each gate as one of:

  passed      - the required evidence exists and is ok=true
  partial     - some evidence passed but platform/runner evidence is missing
  unavailable - required evidence could not be produced on this host
                (e.g. PowerShell gates on a macOS host without pwsh)
  blocked     - evidence exists but failed, or required evidence is absent

Fail-closed contract (do not weaken):
  1. A gate NEVER passes on "script parses", "metadata-only", or "unsigned"
     presence alone.  Every passed gate requires an executed report artifact
     with ok=true (or the equivalent status field).
  2. G6 (Ops) requires the PowerShell `release-verification-*.json` report and
     the `release-handoff-manifest-*.json`.  On a host where PowerShell could
     not run, G6 is `unavailable` and the aggregate `release_ready` is false.
  3. G8 (Candidate) requires `fan-in-artifact-contract-*.json` ok=true, which
     binds BOTH platforms to the same non-empty git commit and the same source
     digest.  Missing or failed fan-in evidence => G8 `blocked`.
  4. Default exit code is 1 whenever any gate is not `passed`.  `--report-only`
     writes the report and exits 0 for diagnostics; it never marks a release
     ready and must never be used as a release pass.

Usage:
  python3 support/scripts/generate_release_gate_status.py \
    [--artifacts-dir support/artifacts] [--output-dir support/artifacts] \
    [--report-only] [--quiet]
"""
from __future__ import annotations

import argparse
import datetime
import json
import sys
from pathlib import Path

# Gate ids and labels mirror docs/PLAN.md §1C.2 items 1-9 (G9 is out of scope:
# it is the frontend-completeness gate that only opens after G0-G8 are green).
GATES = [
    ("G0", "Truth"),
    ("G1", "Contracts"),
    ("G2", "Pipeline"),
    ("G3", "Stores"),
    ("G4", "Surfaces"),
    ("G5", "Agent/RTK"),
    ("G6", "Ops"),
    ("G7", "Real OpenCode"),
    ("G8", "Candidate"),
]


def latest_file(directory: Path, pattern: str) -> Path | None:
    """Newest file matching pattern (by mtime), or None."""
    if directory is None or not directory.is_dir():
        return None
    matches = sorted(
        (p for p in directory.glob(pattern) if p.is_file()),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )
    return matches[0] if matches else None


def load_json(path: Path | None) -> dict | None:
    if path is None or not path.is_file():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
        return data if isinstance(data, dict) else None
    except (OSError, ValueError):
        return None


def step_ok(report: dict | None, step_id: str) -> bool | None:
    """True/False when the report has the step; None when the report/step is absent."""
    if not isinstance(report, dict):
        return None
    steps = report.get("steps")
    if not isinstance(steps, list):
        return None
    for step in steps:
        if isinstance(step, dict) and step.get("id") == step_id:
            return step.get("ok") is True
    return None


def report_ok(report: dict | None) -> bool | None:
    if not isinstance(report, dict):
        return None
    value = report.get("ok")
    return value is True if isinstance(value, bool) else None


def report_green(report: dict | None) -> bool | None:
    """True when a report is an executed pass under either schema it uses.

    Some evidence reports stamp `ok: true` (fan-in, verification, audits);
    managed-RTK candidate receipts stamp `status: pass` with no `ok` field.
    Absent report or any non-green status => False.
    """
    if not isinstance(report, dict):
        return None
    if report.get("ok") is True:
        return True
    status = report.get("status")
    if isinstance(status, str) and status.lower() in ("pass", "passed", "ok", "success"):
        return True
    return False


def all_steps_ok(report: dict | None, step_ids: list[str]) -> bool | None:
    """True only when every listed step is present AND ok; None when the report is absent."""
    if not isinstance(report, dict):
        return None
    results = [step_ok(report, step_id) for step_id in step_ids]
    if any(result is None for result in results):
        return None
    return all(results)


def evaluate(artifacts_dir: Path) -> dict:
    checks: list[dict] = []

    def add(gate_id: str, state: str, detail: str, evidence: list[str]) -> None:
        checks.append(
            {
                "gate": gate_id,
                "state": state,
                "detail": detail,
                "evidence": evidence,
            }
        )

    def find(pattern: str, subdir: str = "") -> Path | None:
        base = artifacts_dir
        if subdir:
            base = artifacts_dir / subdir
        return latest_file(base, pattern)

    # --- Evidence sources ---------------------------------------------------
    verification_path = find("release-verification-*.json")
    verification = load_json(verification_path)
    handoff_path = find("release-handoff-manifest-*.json")
    handoff = load_json(handoff_path)
    dacl_path = find("windows-dacl-acl-tests-*.json")
    dacl = load_json(dacl_path)
    managed_rtk_path = find("candidate-*.json", subdir="managed-rtk-candidate")
    managed_rtk = load_json(managed_rtk_path)
    opencode_path = find("real-routed-*.json", subdir="opencode-real-routed")
    opencode = load_json(opencode_path)
    fanin_path = find("fan-in-artifact-contract-*.json")
    fanin = load_json(fanin_path)

    def evidence_list(*paths: Path | None) -> list[str]:
        return [str(p) for p in paths if p is not None]

    # --- G0 Truth: no fixture defaults; candidate provenance bound ----------
    # Evidence: executed release-verification report with the release-gate
    # contract and candidate-provenance steps green.  Absent report => the
    # PowerShell-driven Truth evidence was not produced on this host.
    g0_contract = step_ok(verification, "release_gate_contract")
    g0_provenance = step_ok(verification, "release_candidate_provenance")
    if verification is None:
        add("G0", "unavailable",
            "release-verification report absent; PowerShell Truth gate not executed on this host (never counts scripts-parse as Truth)",
            evidence_list(verification_path))
    elif report_ok(verification) is False:
        add("G0", "blocked",
            "release-verification report exists but ok=false", evidence_list(verification_path))
    elif g0_contract is True and g0_provenance is True:
        add("G0", "passed",
            "release-gate contract and candidate provenance steps green in an executed report",
            evidence_list(verification_path))
    else:
        add("G0", "blocked",
            f"release-gate contract step={g0_contract}, candidate provenance step={g0_provenance}; both must be ok=true",
            evidence_list(verification_path))

    # --- G1 Contracts --------------------------------------------------------
    g1 = all_steps_ok(verification, ["release_gate_contract", "release_candidate_provenance"])
    if verification is None:
        add("G1", "unavailable",
            "release-verification report absent; Contracts evidence not executed on this host",
            evidence_list(verification_path))
    elif g1 is True:
        add("G1", "passed", "contract regression and dynamic provenance tests green",
            evidence_list(verification_path))
    else:
        add("G1", "blocked", "one or more contract/provenance steps missing or failed",
            evidence_list(verification_path))

    # --- G2 Pipeline ---------------------------------------------------------
    g2 = all_steps_ok(verification, ["codec_candidate_evidence", "release_performance_gate"])
    if verification is None:
        add("G2", "unavailable",
            "release-verification report absent; codec/performance pipeline gates not executed on this host",
            evidence_list(verification_path))
    elif g2 is True:
        add("G2", "passed",
            "production codec probe and performance/stability gate green in executed report",
            evidence_list(verification_path))
    elif g2 is False:
        add("G2", "blocked", "codec evidence or performance gate step failed",
            evidence_list(verification_path))
    else:
        add("G2", "blocked", "codec evidence or performance gate step missing from report",
            evidence_list(verification_path))

    # --- G3 Stores -----------------------------------------------------------
    g3_workspace = step_ok(verification, "rust_workspace_tests")
    if dacl is not None and report_ok(dacl) is False:
        add("G3", "blocked",
            "windows-dacl-acl-tests report exists but ok=false", evidence_list(dacl_path))
    elif verification is None:
        add("G3", "unavailable",
            "no executed release-verification report and no Windows DACL evidence on this host",
            evidence_list(verification_path, dacl_path))
    elif g3_workspace is True and report_ok(dacl) is True:
        add("G3", "passed",
            "workspace store tests green and real Windows MSVC DACL/ACL tests green",
            evidence_list(verification_path, dacl_path))
    elif g3_workspace is True and dacl is None:
        add("G3", "partial",
            "workspace store tests green but Windows MSVC DACL 实测缺失 (no windows-dacl-acl-tests report on this host)",
            evidence_list(verification_path, dacl_path))
    else:
        add("G3", "blocked",
            f"workspace store tests step={g3_workspace}, DACL report={None if dacl is None else report_ok(dacl)}",
            evidence_list(verification_path, dacl_path))

    # --- G4 Surfaces ---------------------------------------------------------
    g4 = all_steps_ok(verification, [
        "frontend_vitest", "frontend_typecheck", "desktop_build", "rust_workspace_tests",
    ])
    if verification is None:
        add("G4", "unavailable",
            "release-verification report absent; surface parity steps not executed on this host",
            evidence_list(verification_path))
    elif g4 is True:
        add("G4", "passed", "frontend/desktop/workspace surface steps green",
            evidence_list(verification_path))
    elif g4 is False:
        add("G4", "blocked", "one or more surface steps failed",
            evidence_list(verification_path))
    else:
        add("G4", "blocked", "one or more surface steps missing from report",
            evidence_list(verification_path))

    # --- G5 Agent/RTK --------------------------------------------------------
    # macOS managed-RTK receipts stamp status=pass (no ok field); the helper
    # accepts both schemas.  A green macOS probe is only PARTIAL until the
    # fan-in report proves Windows/WSL runner test receipts exist.
    mac_rtk_ok = report_green(managed_rtk)
    win_rtk_receipts_ok = (fanin is not None and report_green(fanin) is True)
    if mac_rtk_ok is True and win_rtk_receipts_ok:
        add("G5", "passed",
            "macOS managed RTK lifecycle probe green and fan-in proves Windows test receipts",
            evidence_list(managed_rtk_path, fanin_path))
    elif mac_rtk_ok is True:
        add("G5", "partial",
            "macOS managed RTK probe green; Windows/WSL runner receipts not yet proven on this host (no fan-in evidence)",
            evidence_list(managed_rtk_path, fanin_path))
    elif managed_rtk is None and fanin is None:
        add("G5", "unavailable",
            "no managed RTK or fan-in evidence on this host", evidence_list(managed_rtk_path, fanin_path))
    elif managed_rtk is not None and mac_rtk_ok is False:
        add("G5", "blocked", "latest managed RTK receipt is not a pass", evidence_list(managed_rtk_path))
    else:
        add("G5", "blocked",
            f"managed RTK evidence absent (probe={managed_rtk_path})", evidence_list(managed_rtk_path))

    # --- G6 Ops --------------------------------------------------------------
    # PowerShell gates: verification / gate contract / provenance / performance /
    # codec / capability / redaction / cleanup / distribution / handoff / signing.
    # None of them may be replaced by "scripts parse" or metadata-only presence.
    g6_redaction = step_ok(verification, "release_artifact_redaction_audit")
    g6_cleanup = step_ok(verification, "release_cleanup_audit")
    if verification is None:
        add("G6", "unavailable",
            "PowerShell Ops gates (verification/handoff/redaction/cleanup/distribution/signing) were NOT executed on this macOS host; no release-verification report.  This is a blocker, not a pass.",
            evidence_list(verification_path, handoff_path))
    elif report_ok(verification) is False:
        add("G6", "blocked",
            "release-verification report exists but ok=false", evidence_list(verification_path))
    elif (report_ok(verification) is True and g6_redaction is True and g6_cleanup is True
          and handoff is not None and report_ok(handoff) is True):
        add("G6", "passed",
            "release verification ok, redaction/cleanup steps green, handoff manifest ok=true",
            evidence_list(verification_path, handoff_path))
    else:
        add("G6", "blocked",
            f"report ok={report_ok(verification)}, redaction={g6_redaction}, cleanup={g6_cleanup}, handoff ok={None if handoff is None else report_ok(handoff)}",
            evidence_list(verification_path, handoff_path))

    # --- G7 Real OpenCode ----------------------------------------------------
    opencode_status = None
    if isinstance(opencode, dict):
        opencode_status = opencode.get("status") or opencode.get("result")
        if opencode.get("candidate_id"):
            opencode_status = opencode.get("status") or opencode_status
    g7_ok = (isinstance(opencode, dict)
             and opencode.get("candidate_id")
             and (opencode.get("status") in ("pass", "passed", "ok")
                  or opencode.get("gate", {}).get("ok") is True))
    if g7_ok:
        add("G7", "passed",
            "latest real OpenCode routed report bound to a candidate with status pass",
            evidence_list(opencode_path))
    elif opencode is None:
        add("G7", "unavailable", "no opencode-real-routed evidence on this host",
            evidence_list(opencode_path))
    else:
        add("G7", "blocked",
            f"latest real OpenCode report is not a pass (status={opencode.get('status')!r})",
            evidence_list(opencode_path))

    # --- G8 Candidate --------------------------------------------------------
    if fanin is None:
        add("G8", "blocked",
            "双平台同 commit fan-in 证据缺失: no fan-in-artifact-contract report on this host; Windows runner and dual-platform same-commit evidence not yet produced",
            evidence_list(fanin_path))
    elif report_ok(fanin) is not True:
        add("G8", "blocked",
            "fan-in report exists but ok=false (commit/source-digest/platform binding failed)",
            evidence_list(fanin_path))
    elif (fanin.get("same_commit") is not True
          or fanin.get("same_source_digest") is not True):
        add("G8", "blocked",
            "fan-in ok but same_commit/same_source_digest not both true", evidence_list(fanin_path))
    else:
        add("G8", "passed",
            "fan-in report green: both platforms bound to the same commit and source digest",
            evidence_list(fanin_path))

    blockers = [
        f"{gate['gate']} ({dict(GATES)[gate['gate']]}): {gate['state']} — {gate['detail']}"
        for gate in checks
        if gate["state"] != "passed"
    ]
    release_ready = len(blockers) == 0
    return {
        "release_ready": release_ready,
        "gates": checks,
        "blockers": blockers,
        "evidence": {
            "release_verification": str(verification_path) if verification_path else None,
            "release_handoff_manifest": str(handoff_path) if handoff_path else None,
            "windows_dacl_acl": str(dacl_path) if dacl_path else None,
            "managed_rtk": str(managed_rtk_path) if managed_rtk_path else None,
            "opencode_real_routed": str(opencode_path) if opencode_path else None,
            "fan_in_artifact_contract": str(fanin_path) if fanin_path else None,
        },
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Aggregate G0-G8 release gate status from evidence artifacts (fail-closed).",
    )
    parser.add_argument("--artifacts-dir", default="support/artifacts",
                        help="Directory scanned for evidence (default: support/artifacts)")
    parser.add_argument("--output-dir", default="support/artifacts",
                        help="Where to write the status report (default: same as artifacts-dir)")
    parser.add_argument("--report-only", action="store_true",
                        help="Write the report and exit 0 even when gates are blocked/unavailable. "
                             "Diagnostics only — never a release pass.")
    parser.add_argument("--quiet", action="store_true", help="Suppress human-readable summary")
    args = parser.parse_args(argv)

    artifacts_dir = Path(args.artifacts_dir)
    result = evaluate(artifacts_dir)

    report = {
        "ok": result["release_ready"],
        "mode": "release-gate-status",
        "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "host_platform": sys.platform,
        "release_ready": result["release_ready"],
        "release_ready_hint": "false unless EVERY G0-G8 gate is passed; PowerShell-unexecuted and "
                              "Windows-runner evidence are blockers, never passes",
        "blockers": result["blockers"],
        "gates": result["gates"],
        "evidence": result["evidence"],
    }

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    report_path = output_dir / f"release-gate-status-{stamp}.json"
    report["report_path"] = str(report_path)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    if not args.quiet:
        print(json.dumps(report, indent=2))

    if not result["release_ready"] and not args.report_only:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
