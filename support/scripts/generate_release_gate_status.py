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

PR-7 extension (docs/releases/v0.0.1/REQUIREMENTS.md EVID-1/EVID-3/EVID-6):

* Supplemental required evidence is aggregated in a separate top-level
  ``supplemental`` section (Codex routed acceptance, agent config acceptance,
  cc-switch A->B->A, macOS upgrade/rollback, autostart/credential round-trip).
  Any missing or non-green supplemental evidence blocks the release
  (exit 1).  Supplemental evidence NEVER feeds G0-G8: G7 still means "Real
  OpenCode" only, and the other gates keep their exact semantics.
* ``--expected-commit`` and ``--expected-source-digest`` enable bound mode:
  every supplemental artifact must carry a candidate commit (and, when the
  digest flag is given, a source digest) matching the expected values.  In
  bound mode the aggregator never accepts "the newest file" alone: it only
  considers artifacts bound to the expected candidate, so stale pass
  artifacts cannot leak through (EVID-1).  G8 additionally requires the
  fan-in report's ``expected_commit`` to match ``--expected-commit``.
  Without the flags the aggregator keeps the legacy "latest artifact" mode
  and records binding_mode=latest; N9 release mode MUST pass both flags.
* The source digest anchor is content-bearing and worktree-invariant:
  sha256 of the ASCII hex bytes of ``git rev-parse HEAD^{tree}`` (the
  candidate commit's tree object id).  Unlike the old ``git diff --binary
  HEAD -- .`` anchor — a constant at any clean checkout — the tree anchor
  discriminates candidate content and is reproducible at a clean C_final
  checkout even when evidence was collected from a dirty worktree.  The
  fan-in contract script independently computes the same tree digest for the
  expected commit, so G8 and ``--expected-source-digest`` share one anchor.

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
  4. Default exit code is 1 whenever any gate is not `passed` or any
     supplemental evidence is not `passed`.  `--report-only` writes the
     report and exits 0 for diagnostics; it never marks a release ready and
     must never be used as a release pass.

Usage:
  python3 support/scripts/generate_release_gate_status.py \
    [--artifacts-dir support/artifacts] [--output-dir support/artifacts] \
    [--expected-commit <C_final>] [--expected-source-digest <sha256>] \
    [--report-only] [--quiet]
"""
from __future__ import annotations

import argparse
import datetime
import json
import re
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

# Supplemental required evidence (PR-7).  Each spec lists artifact globs
# relative to the artifacts dir (top level and/or one subdir), a green
# predicate, and whether the artifact is expected to carry candidate binding.
# Missing evidence is always `blocked`, never `unavailable`: these artifacts
# are produced by Main / the candidate verification nodes, not by host
# capability.
_SUPPLEMENTAL_SPECS: list[dict] = [
    {
        "id": "codex",
        "label": "Codex routed acceptance (N2)",
        "patterns": ["codex-ocxlocal-routed/*.json", "codex-ocxlocal-routed-*.json"],
        "green": "ok_and_status",
        "ok_statuses": {"routed-verified", "passed", "pass", "ok"},
        "binding": True,
    },
    {
        "id": "config",
        "label": "Agent config acceptance (N4)",
        "patterns": ["agent-config-acceptance/*.json", "agent-config-acceptance-*.json"],
        "green": "status_ok",
        "ok_statuses": {"ok", "passed", "pass"},
        "binding": True,
    },
    {
        "id": "cc-switch",
        "label": "cc-switch A->B->A evidence (N5)",
        "patterns": ["cc-switch-aba/*.json", "cc-switch-aba-*.json"],
        "green": "cc_switch_aba",
        "ok_statuses": set(),
        "binding": True,
    },
    {
        "id": "upgrade-rollback",
        "label": "macOS upgrade/rollback audit (N7)",
        "patterns": ["upgrade-autostart-credential/upgrade-rollback-audit-*.json",
                     "upgrade-rollback-audit-*.json"],
        "green": "ok_and_status",
        "ok_statuses": {"verified", "passed", "pass", "ok"},
        "binding": True,
    },
    {
        "id": "autostart-credential",
        "label": "Autostart/credential round-trip (N7)",
        "patterns": ["upgrade-autostart-credential/autostart-credential-roundtrip-*.json",
                     "autostart-credential-roundtrip-*.json"],
        "green": "ok_and_status",
        "ok_statuses": {"verified", "passed", "pass", "ok"},
        "binding": True,
    },
]

COMMIT_RE = re.compile(r"[0-9a-fA-F]{40,64}")
DIGEST_RE = re.compile(r"[0-9a-fA-F]{64}")


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


# --- PR-7 supplemental evidence helpers -------------------------------------

def artifact_binding(artifact: dict | None) -> tuple[str | None, str | None]:
    """Extract (candidate_commit, source_digest) from an evidence artifact.

    Accepted shapes (anything else yields None for that field, which fails
    closed in bound mode):
      - top-level ``candidate_commit`` / ``source_digest``
      - nested ``candidate: {"commit": ..., "source_digest" | "digest": ...}``
      - ``source_digest`` as ``{"value": ...}``
    """
    if not isinstance(artifact, dict):
        return None, None
    commit = artifact.get("candidate_commit")
    digest = artifact.get("source_digest")
    candidate = artifact.get("candidate")
    if isinstance(candidate, dict):
        if not isinstance(commit, str):
            commit = candidate.get("commit")
        if not isinstance(digest, (str, dict)):
            digest = candidate.get("source_digest") or candidate.get("digest")
    if isinstance(digest, dict):
        digest = digest.get("value")
    if not isinstance(commit, str) or not COMMIT_RE.fullmatch(commit):
        commit = None
    if not isinstance(digest, str) or not DIGEST_RE.fullmatch(digest):
        digest = None
    return commit, digest


def evidence_green(spec: dict, artifact: dict | None) -> bool:
    """Green predicate per supplemental spec; never green on presence alone."""
    if not isinstance(artifact, dict):
        return False
    if spec["green"] == "cc_switch_aba":
        return (
            artifact.get("ok") is True
            and artifact.get("wrote_cc_switch") is False
            and artifact.get("identity_restored") is True
        )
    if spec["green"] == "status_ok":
        status = artifact.get("status")
        return isinstance(status, str) and status.lower() in spec["ok_statuses"]
    # ok_and_status: an executed pass requires ok=true AND a pass status.
    if artifact.get("ok") is not True:
        return False
    status = artifact.get("status")
    return isinstance(status, str) and status.lower() in spec["ok_statuses"]


def find_evidence_files(directory: Path, patterns: list[str]) -> list[Path]:
    """All evidence files matching any pattern (top level or one subdir)."""
    if directory is None or not directory.is_dir():
        return []
    found: list[Path] = []
    for pattern in patterns:
        if "/" in pattern:
            subdir, name = pattern.split("/", 1)
            base = directory / subdir
            if base.is_dir():
                found.extend(p for p in base.glob(name) if p.is_file())
        else:
            found.extend(p for p in directory.glob(pattern) if p.is_file())
    # Newest first, deterministic order for the evidence list.
    return sorted(found, key=lambda p: p.stat().st_mtime, reverse=True)


def bound_match(artifact: dict | None, expected_commit: str | None,
                expected_digest: str | None) -> bool:
    """True when the artifact binds to the expected candidate (bound mode).

    In bound mode a binding field that is absent counts as a mismatch: a
    pass artifact without a candidate commit must not leak through (EVID-1).
    """
    if expected_commit is None and expected_digest is None:
        return True
    commit, digest = artifact_binding(artifact)
    if expected_commit is not None:
        if commit is None or commit.lower() != expected_commit.lower():
            return False
    if expected_digest is not None:
        if digest is None or digest.lower() != expected_digest.lower():
            return False
    return True


def supplemental_checks(artifacts_dir: Path, expected_commit: str | None,
                        expected_digest: str | None) -> list[dict]:
    """Evaluate the PR-7 supplemental required evidence (fail-closed)."""
    checks: list[dict] = []
    for spec in _SUPPLEMENTAL_SPECS:
        files = find_evidence_files(artifacts_dir, spec["patterns"])
        if not files:
            checks.append({
                "id": spec["id"],
                "label": spec["label"],
                "state": "blocked",
                "detail": "required supplemental evidence is missing (no matching artifact)",
                "evidence": [],
                "binding": None,
            })
            continue
        # Bound mode: only artifacts bound to the expected candidate count;
        # stale/unbound artifacts are reported but never accepted.
        eligible = files
        if expected_commit is not None or expected_digest is not None:
            eligible = [
                path for path in files
                if bound_match(load_json(path), expected_commit, expected_digest)
            ]
        if not eligible:
            newest = load_json(files[0])
            commit, digest = artifact_binding(newest)
            checks.append({
                "id": spec["id"],
                "label": spec["label"],
                "state": "blocked",
                "detail": (
                    f"no artifact binds the expected candidate "
                    f"(expected_commit={expected_commit}, expected_source_digest={expected_digest}); "
                    f"newest artifact binding: commit={commit}, source_digest={digest} — "
                    "stale/old pass artifacts are rejected (EVID-1)"
                ),
                "evidence": [str(p) for p in files],
                "binding": {"commit": commit, "source_digest": digest},
            })
            continue
        chosen = eligible[0]
        artifact = load_json(chosen)
        green = evidence_green(spec, artifact)
        commit, digest = artifact_binding(artifact)
        checks.append({
            "id": spec["id"],
            "label": spec["label"],
            "state": "passed" if green else "blocked",
            "detail": (
                f"artifact {chosen.name} is an executed pass bound to the expected candidate"
                if green else
                f"latest bound artifact {chosen.name} is not green "
                f"(ok={artifact.get('ok') if isinstance(artifact, dict) else None}, "
                f"status={artifact.get('status') if isinstance(artifact, dict) else None}, "
                f"wrote_cc_switch={artifact.get('wrote_cc_switch') if isinstance(artifact, dict) else None}, "
                f"identity_restored={artifact.get('identity_restored') if isinstance(artifact, dict) else None})"
            ),
            "evidence": [str(p) for p in files],
            "binding": {"commit": commit, "source_digest": digest},
        })
    return checks


def evaluate(artifacts_dir: Path, expected_commit: str | None = None,
             expected_source_digest: str | None = None) -> dict:
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
    # Evidence may live under managed-rtk-candidate/ or, for the CI fan-in
    # merge (which flattens evidence into one dir), at the top level.
    managed_rtk_path = find("candidate-*.json", subdir="managed-rtk-candidate")
    if managed_rtk_path is None:
        managed_rtk_path = find("candidate-*.json")
    managed_rtk = load_json(managed_rtk_path)
    opencode_path = find("real-routed-*.json", subdir="opencode-real-routed")
    if opencode_path is None:
        opencode_path = find("real-routed-*.json")
    opencode = load_json(opencode_path)
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
    # PR-7 bound mode: the fan-in report must also be bound to the expected
    # commit.  G8 semantics are otherwise unchanged (same commit + same source
    # digest across both platforms is still required).
    fanin_commit_ok = True
    if expected_commit is not None and isinstance(fanin, dict):
        reported = fanin.get("expected_commit")
        fanin_commit_ok = isinstance(reported, str) and reported.lower() == expected_commit.lower()
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
    elif not fanin_commit_ok:
        add("G8", "blocked",
            f"fan-in expected_commit does not match --expected-commit {expected_commit}",
            evidence_list(fanin_path))
    else:
        add("G8", "passed",
            "fan-in report green: both platforms bound to the same commit and source digest",
            evidence_list(fanin_path))

    # --- PR-7 supplemental required evidence --------------------------------
    supplemental = supplemental_checks(artifacts_dir, expected_commit, expected_source_digest)

    blockers = [
        f"{gate['gate']} ({dict(GATES)[gate['gate']]}): {gate['state']} — {gate['detail']}"
        for gate in checks
        if gate["state"] != "passed"
    ]
    blockers.extend(
        f"supplemental[{item['id']}] ({item['label']}): {item['state']} — {item['detail']}"
        for item in supplemental
        if item["state"] != "passed"
    )
    release_ready = len(blockers) == 0
    return {
        "release_ready": release_ready,
        "gates": checks,
        "supplemental": supplemental,
        "binding": {
            "mode": "bound" if (expected_commit is not None or expected_source_digest is not None) else "latest",
            "expected_commit": expected_commit,
            "expected_source_digest": expected_source_digest,
        },
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
    parser.add_argument("--expected-commit",
                        help="Required candidate commit (40-64 hex). Bound mode: any evidence not "
                             "bound to this commit is blocked; the latest-file fallback is disabled.")
    parser.add_argument("--expected-source-digest",
                        help="Required source digest sha256 (64 hex). Bound mode: any evidence "
                             "without a matching source digest is blocked.")
    parser.add_argument("--report-only", action="store_true",
                        help="Write the report and exit 0 even when gates are blocked/unavailable. "
                             "Diagnostics only — never a release pass.")
    parser.add_argument("--quiet", action="store_true", help="Suppress human-readable summary")
    args = parser.parse_args(argv)

    if args.expected_commit is not None and not COMMIT_RE.fullmatch(args.expected_commit):
        parser.error("--expected-commit must be a 40-64 hex git commit")
    if args.expected_source_digest is not None and not DIGEST_RE.fullmatch(args.expected_source_digest):
        parser.error("--expected-source-digest must be a 64-hex sha256")

    artifacts_dir = Path(args.artifacts_dir)
    result = evaluate(artifacts_dir, args.expected_commit, args.expected_source_digest)

    report = {
        "ok": result["release_ready"],
        "mode": "release-gate-status",
        "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "host_platform": sys.platform,
        "release_ready": result["release_ready"],
        "release_ready_hint": "false unless EVERY G0-G8 gate is passed AND every supplemental "
                              "required evidence item is passed; PowerShell-unexecuted, "
                              "Windows-runner and Main-hosted evidence are blockers, never passes",
        "binding": result["binding"],
        "blockers": result["blockers"],
        "gates": result["gates"],
        "supplemental": result["supplemental"],
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
