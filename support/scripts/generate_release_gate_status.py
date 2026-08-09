#!/usr/bin/env python3
"""G0-G9 release gate status aggregator (fail-closed, explicit evidence only).

This aggregator maps the release gates defined in docs/PLAN.md §1C.2 and
docs/releases/v0.0.1/VERIFICATION.md §6 (G0-G9) to concrete evidence
artifacts.  It NEVER discovers evidence by scanning directories, mtime, or
"latest" files: every consumed artifact must be listed explicitly in an
evidence manifest (--evidence-manifest), and every N0-N10 artifact must carry
the unified evidence envelope (VERIFICATION.md §1) whose digests are bound to
the frozen candidate.

Fail-closed contract (do not weaken):
  1. --expected-commit, --expected-source-digest, --expected-registry-digest
     and --expected-corpus-digests are REQUIRED.  A gate NEVER passes on
     "script parses", metadata-only presence, an unsigned notice, or a
     status=pass stamp alone: each N0-N10 artifact is envelope-validated
     (fields, candidate binding, dirty=false, digest match, timestamps) and
     its artifact_sha256 is recomputed with the canonical rule and compared
     exactly.  Missing, stale, old-candidate, digest-mismatch, or pass-only
     evidence is rejected.
  2. G6 (Ops) requires the N0/N1/N6/N7/N8/N10-U5 artifacts plus the executed
     PowerShell release-verification report (legacy role
     "release-verification-*") with redaction/cleanup steps and the handoff
     manifest ok=true.  A host where PowerShell was NOT executed reports G6
     unavailable - a blocker, never a pass.
  3. G8 (Candidate) requires N0/N7/N8/N10 plus the fan-in evidence
     ("fan-in-artifact-contract-*") ok=true with same_commit and
     same_source_digest, binding both platforms to the same non-empty commit
     and source digest.  Missing or failed fan-in evidence => G8 blocked
     (双平台同 commit fan-in 证据缺失).
  4. G9 (Frontend completeness) opens only after G0-G8 and the N10-U9
     117/117 coverage seal are green.
  5. Default exit code is 1 whenever any gate is not `passed`.  --report-only
     writes the report and exits 0 for diagnostics; it never marks a release
     ready and must never be used as a release pass.

Usage:
  python3 support/scripts/generate_release_gate_status.py \
    --evidence-manifest final-candidate-evidence-manifest.json \
    --expected-commit <C_final> \
    --expected-source-digest <sha256> \
    --expected-registry-digest <sha256> \
    --expected-corpus-digests '{"benchmark": "<sha256>", ...}' \
    [--expected-version 0.0.1] [--expected-producer-revision <rev>] \
    [--output-dir support/artifacts] [--report-only] [--quiet]

Evidence manifest schema:
  {
    "schema_version": "1",
    "artifacts": {
      "N0": ["support/artifacts/final-candidate/<C_final>/N0-preflight.json"],
      "N1": [".../N1-receipt.json"],
      ...
      "N10-U0": [".../N10-U0-contract.json"],
      ...
      "release_verification": [".../release-verification-*.json"],
      "release_handoff_manifest": [".../release-handoff-manifest-*.json"],
      "windows_dacl_acl": [".../windows-dacl-acl-tests-*.json"],
      "managed_rtk": [".../managed-rtk-candidate/candidate-*.json"],
      "opencode_real_routed": [".../opencode-real-routed/real-routed-*.json"],
      "fan_in_artifact_contract": [".../fan-in-artifact-contract-*.json"]
    }
  }
Node roles (N0-N8, N10-U0..N10-U9) must be envelope artifacts.  Legacy roles
may be old-schema reports; they are validated with ok/status when the envelope
is absent and only feed the legacy portions of G3/G5/G6/G8.
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

# Gate ids and labels mirror docs/PLAN.md §1C.2 and VERIFICATION.md §6
# (G9 is the frontend-completeness gate that only opens after G0-G8 and the
# N10-U9 117/117 coverage seal are green).
GATES = [
    ("G0", "Truth"),
    ("G1", "Contracts"),
    ("G2", "Codec"),
    ("G3", "CCR/ACL"),
    ("G4", "Surfaces"),
    ("G5", "Agent/cc-switch/RTK"),
    ("G6", "Ops"),
    ("G7", "Real OpenCode"),
    ("G8", "Candidate"),
    ("G9", "Frontend completeness"),
]

# Envelope fields required on every N0-N10 machine artifact (VERIFICATION.md §1).
ENVELOPE_FIELDS = [
    "schema_version",
    "node_id",
    "status",
    "reason_codes",
    "candidate_commit",
    "source_digest",
    "dirty",
    "registry_digest",
    "corpus_digests",
    "started_at",
    "finished_at",
    "platform",
    "producer_revision",
    "artifact_sha256",
]

NODE_ROLES = {
    "N0",
    "N1",
    "N2",
    "N3",
    "N4",
    "N5",
    "N6",
    "N7",
    "N8",
    "N10-U0",
    "N10-U1",
    "N10-U2",
    "N10-U3",
    "N10-U4",
    "N10-U5",
    "N10-U6",
    "N10-U7",
    "N10-U8",
    "N10-U9",
}

LEGACY_ROLES = {
    # Executed PowerShell release-verification report (G6).
    "release_verification",
    # PowerShell release handoff manifest (G6).
    "release_handoff_manifest",
    # Windows MSVC DACL/ACL test report (G3, legacy).
    "windows_dacl_acl",
    # managed-RTK candidate receipt (G5, legacy).
    "managed_rtk",
    # real OpenCode routed report (G7, legacy; candidate_id must equal C_final).
    "opencode_real_routed",
    # Dual-platform same-commit/source-digest fan-in contract (G8).
    "fan_in_artifact_contract",
}

# Gate -> required node roles (VERIFICATION.md §6).  "U1-registry" is the
# capability-registry digest binding: it is satisfied only when the expected
# registry digest is provided (required) and every envelope matches it.
GATE_ROLES = {
    "G0": ["N0", "N10-U0", "U1-registry"],
    "G1": ["N10-U0", "N10-U3", "N4"],
    "G2": ["U1-registry", "N10-U2", "N10-U5"],
    "G3": ["N10-U3", "N1", "N8"],
    "G4": ["N10-U0", "N1", "N2", "N3"],
    "G5": ["N2", "N3", "N4", "N5", "N7", "N8", "N10-U2"],
    "G6": ["N0", "N1", "N6", "N7", "N8", "N10-U5"],
    "G7": ["N3"],
    "G8": ["N0", "N7", "N8", "N10"],
    "G9": ["N6", "N10-U9"],
}

N10_AGGREGATE_ROLES = [f"N10-U{index}" for index in range(10)]


def canonical_json_bytes(data: dict) -> bytes:
    """Canonical JSON: UTF-8, object keys sorted, no extra whitespace."""
    return json.dumps(
        data, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def recompute_artifact_sha256(data: dict) -> str:
    """artifact_sha256 rule: remove the field, canonical-JSON, SHA-256 hex."""
    body = {key: value for key, value in data.items() if key != "artifact_sha256"}
    return hashlib.sha256(canonical_json_bytes(body)).hexdigest()


def parse_iso(timestamp: Any) -> datetime.datetime | None:
    if not isinstance(timestamp, str):
        return None
    try:
        return datetime.datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
    except ValueError:
        return None


def report_green(report: dict | None) -> bool | None:
    """True when a legacy report is an executed pass under either schema it uses."""
    if not isinstance(report, dict):
        return None
    if report.get("ok") is True:
        return True
    status = report.get("status")
    if isinstance(status, str) and status.lower() in ("pass", "passed", "ok", "success"):
        return True
    return False


def step_ok(report: dict | None, step_id: str) -> bool | None:
    if not isinstance(report, dict):
        return None
    steps = report.get("steps")
    if not isinstance(steps, list):
        return None
    for step in steps:
        if isinstance(step, dict) and step.get("id") == step_id:
            return step.get("ok") is True
    return None


def envelope_violations(artifact: dict, expected: dict) -> list[str]:
    """Validate the unified evidence envelope; empty list means valid."""
    violations: list[str] = []
    for field in ENVELOPE_FIELDS:
        if field not in artifact:
            violations.append(f"missing envelope field {field!r}")
    if violations:
        return violations
    if artifact.get("status") != "pass":
        violations.append(
            f"status={artifact.get('status')!r} is not 'pass' (a status stamp alone "
            f"never passes without a valid envelope)"
        )
    if not isinstance(artifact.get("reason_codes"), list):
        violations.append("reason_codes must be a list")
    if artifact.get("dirty") is not False:
        violations.append(f"dirty={artifact.get('dirty')!r} must be false")
    if artifact.get("candidate_commit") != expected.get("commit"):
        violations.append(
            f"candidate_commit={artifact.get('candidate_commit')!r} != expected "
            f"{expected.get('commit')!r} (old-candidate evidence is rejected)"
        )
    if artifact.get("source_digest") != expected.get("source_digest"):
        violations.append("source_digest mismatch with frozen candidate")
    if artifact.get("registry_digest") != expected.get("registry_digest"):
        violations.append("registry_digest mismatch with frozen candidate")
    artifact_corpus = artifact.get("corpus_digests")
    if not isinstance(artifact_corpus, dict):
        violations.append("corpus_digests must be an object")
    else:
        for key, value in (expected.get("corpus_digests") or {}).items():
            if artifact_corpus.get(key) != value:
                violations.append(f"corpus_digests[{key!r}] mismatch with frozen candidate")
    started = parse_iso(artifact.get("started_at"))
    finished = parse_iso(artifact.get("finished_at"))
    if started is None or finished is None:
        violations.append("started_at/finished_at must be ISO-8601 timestamps")
    elif finished < started:
        violations.append("finished_at precedes started_at")
    platform = artifact.get("platform")
    if not isinstance(platform, dict) or not all(
        key in platform for key in ("os", "arch", "toolchain")
    ):
        violations.append("platform must include os/arch/toolchain")
    producer_revision = artifact.get("producer_revision")
    if not isinstance(producer_revision, str) or not producer_revision:
        violations.append("producer_revision must be a non-empty string")
    elif expected.get("producer_revision") and producer_revision != expected.get(
        "producer_revision"
    ):
        violations.append("producer_revision mismatch")
    declared = artifact.get("artifact_sha256")
    recomputed = recompute_artifact_sha256(artifact)
    if not isinstance(declared, str) or declared != recomputed:
        violations.append(
            "artifact_sha256 mismatch (must equal SHA-256 of canonical JSON without "
            "the artifact_sha256 field)"
        )
    if artifact.get("node_id") == "N0":
        if "version" not in artifact:
            violations.append("N0 artifact must carry version (v0.0.1 single source of truth)")
        elif expected.get("version") and artifact.get("version") != expected.get("version"):
            violations.append(
                f"version={artifact.get('version')!r} != expected {expected.get('version')!r}"
            )
    return violations


def role_verdicts(role: str, paths: list[str], expected: dict) -> list[dict]:
    """Validate every declared artifact path for one role."""
    verdicts: list[dict] = []
    for raw_path in paths:
        path = Path(raw_path)
        if not path.is_file():
            verdicts.append(
                {
                    "role": role,
                    "path": raw_path,
                    "ok": False,
                    "violations": ["artifact file is missing"],
                    "data": None,
                }
            )
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8-sig"))
            if not isinstance(data, dict):
                raise ValueError("artifact root is not an object")
        except (OSError, ValueError) as exc:
            verdicts.append(
                {
                    "role": role,
                    "path": raw_path,
                    "ok": False,
                    "violations": [f"artifact is not readable JSON: {exc}"],
                    "data": None,
                }
            )
            continue
        if role in NODE_ROLES:
            violations = envelope_violations(data, expected)
        else:
            # Legacy role: envelope-validate when present, else ok/status.
            if all(field in data for field in ENVELOPE_FIELDS):
                violations = envelope_violations(data, expected)
            else:
                violations = []
                if report_green(data) is not True:
                    violations.append("legacy report is not an executed pass (ok/status)")
        # Role-specific contract checks apply regardless of envelope presence.
        if role == "fan_in_artifact_contract":
            if data.get("same_commit") is not True:
                violations.append("fan-in same_commit is not true")
            if data.get("same_source_digest") is not True:
                violations.append("fan-in same_source_digest is not true")
        if role == "opencode_real_routed":
            if data.get("candidate_id") != expected.get("commit"):
                violations.append(
                    f"opencode candidate_id={data.get('candidate_id')!r} != "
                    f"C_final {expected.get('commit')!r} (old-candidate rejected)"
                )
        verdicts.append(
            {
                "role": role,
                "path": raw_path,
                "ok": not violations,
                "violations": violations,
                "data": data,
            }
        )
    return verdicts


def stale_violations(verdicts_by_role: dict[str, list[dict]]) -> dict[str, list[str]]:
    """Timestamp-based staleness: any node artifact started before the N0
    freeze finished is stale.  File mtime is NEVER consulted."""
    n0 = None
    for verdict in verdicts_by_role.get("N0", []):
        if verdict["ok"] and verdict["data"] is not None:
            n0 = verdict["data"]
            break
    if n0 is None:
        return {}
    n0_finished = parse_iso(n0.get("finished_at"))
    if n0_finished is None:
        return {}
    stale: dict[str, list[str]] = {}
    for role, verdicts in verdicts_by_role.items():
        if role == "N0" or role not in NODE_ROLES:
            continue
        for verdict in verdicts:
            if not verdict["ok"] or verdict["data"] is None:
                continue
            started = parse_iso(verdict["data"].get("started_at"))
            if started is not None and started < n0_finished:
                stale.setdefault(role, []).append(
                    f"{verdict['path']} started at {verdict['data'].get('started_at')} "
                    f"before N0 freeze finished at {n0.get('finished_at')} (stale evidence)"
                )
    return stale


def evaluate(manifest: dict, expected: dict) -> dict:
    """Aggregate G0-G9 from an explicit evidence manifest (fail-closed)."""
    manifest_errors: list[str] = []
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, dict):
        return {
            "release_ready": False,
            "gates": [],
            "blockers": [f"manifest has no artifacts object: {manifest!r}"],
            "evidence": {},
            "artifact_verdicts": {},
            "manifest_errors": manifest_errors,
        }
    unknown_roles = sorted(set(artifacts) - NODE_ROLES - LEGACY_ROLES)
    if unknown_roles:
        manifest_errors.append(f"unknown evidence roles in manifest: {unknown_roles}")

    verdicts_by_role: dict[str, list[dict]] = {}
    for role, paths in artifacts.items():
        if not isinstance(paths, list) or not all(isinstance(p, str) for p in paths):
            manifest_errors.append(f"role {role!r} must map to a list of artifact paths")
            continue
        verdicts_by_role[role] = role_verdicts(role, paths, expected)

    stale = stale_violations(verdicts_by_role)
    for role, messages in stale.items():
        for verdict in verdicts_by_role.get(role, []):
            if verdict["ok"] and verdict["data"] is not None:
                for message in messages:
                    if verdict["path"] in message:
                        verdict["ok"] = False
                        verdict["violations"].append(message)

    def role_ok(role: str) -> tuple[bool, list[str]]:
        if role == "U1-registry":
            # Registry digest binding: the expected digest is mandatory and
            # every envelope above already compared registry_digest exactly.
            return bool(expected.get("registry_digest")), []
        if role == "N10":
            verdicts: list[dict] = []
            for sub in N10_AGGREGATE_ROLES:
                verdicts.extend(verdicts_by_role.get(sub, []))
            failures = [v for v in verdicts if not v["ok"]]
            if not verdicts:
                return False, ["N10 unified compression proof has no declared artifacts"]
            return not failures, [f for v in failures for f in v["violations"]]
        verdicts = verdicts_by_role.get(role, [])
        if not verdicts:
            return False, [f"no evidence declared for role {role}"]
        failures = [v for v in verdicts if not v["ok"]]
        return not failures, [f for v in failures for f in v["violations"]]

    checks: list[dict] = []
    for gate_id, label in GATES:
        role_results = {role: role_ok(role) for role in GATE_ROLES[gate_id]}
        failed = [
            f"{role}: {'; '.join(reasons)}" for role, (ok, reasons) in role_results.items() if not ok
        ]
        evidence_paths = sorted(
            {
                str(Path(v["path"]).resolve())
                for role in GATE_ROLES[gate_id]
                for v in verdicts_by_role.get(role, [])
            }
        )
        if gate_id == "G6":
            legacy_ok, legacy_reasons = _g6_legacy(verdicts_by_role, expected)
            if legacy_ok is False:
                failed.append(f"release_verification: {'; '.join(legacy_reasons)}")
        if gate_id == "G8":
            legacy_ok, legacy_reasons = _g8_fan_in(verdicts_by_role, expected)
            if legacy_ok is False:
                failed.append(f"fan_in_artifact_contract: {'; '.join(legacy_reasons)}")
        if failed:
            state = "blocked"
            detail = "; ".join(failed)
        else:
            state = "passed"
            detail = f"all required evidence for {gate_id} {label} is envelope-valid and bound to C_final"
        if gate_id == "G6" and not verdicts_by_role.get("release_verification"):
            state = "unavailable"
            detail = (
                "PowerShell release-verification report NOT executed on this host "
                "(role 'release_verification' absent from the explicit evidence "
                "manifest); this is a blocker, not a pass"
            )
        checks.append(
            {
                "gate": gate_id,
                "state": state,
                "detail": detail,
                "evidence": evidence_paths,
            }
        )

    blockers = [
        f"{gate['gate']} ({dict(GATES)[gate['gate']]}): {gate['state']} — {gate['detail']}"
        for gate in checks
        if gate["state"] != "passed"
    ]
    if manifest_errors:
        blockers.extend(f"manifest error: {message}" for message in manifest_errors)
    release_ready = len(blockers) == 0
    return {
        "release_ready": release_ready,
        "gates": checks,
        "blockers": blockers,
        "evidence": {
            role: [str(Path(v["path"]).resolve()) for v in verdicts]
            for role, verdicts in sorted(verdicts_by_role.items())
        },
        "artifact_verdicts": verdicts_by_role,
        "manifest_errors": manifest_errors,
    }


def _g6_legacy(verdicts_by_role: dict[str, list[dict]], expected: dict) -> tuple[bool | None, list[str]]:
    """G6 legacy: executed PowerShell report with redaction/cleanup + handoff ok."""
    verdicts = verdicts_by_role.get("release_verification", [])
    if not verdicts:
        return None, ["release_verification role absent (PowerShell NOT executed on this host)"]
    if not all(v["ok"] for v in verdicts):
        return False, [f for v in verdicts if not v["ok"] for f in v["violations"]]
    report = verdicts[0]["data"]
    redaction = step_ok(report, "release_artifact_redaction_audit")
    cleanup = step_ok(report, "release_cleanup_audit")
    handoff_verdicts = verdicts_by_role.get("release_handoff_manifest", [])
    handoff_ok = bool(handoff_verdicts) and all(v["ok"] for v in handoff_verdicts)
    reasons: list[str] = []
    if redaction is not True:
        reasons.append(f"release_artifact_redaction_audit step={redaction}")
    if cleanup is not True:
        reasons.append(f"release_cleanup_audit step={cleanup}")
    if not handoff_ok:
        reasons.append("release-handoff-manifest missing or not ok=true")
    return (not reasons), reasons


def _g8_fan_in(verdicts_by_role: dict[str, list[dict]], expected: dict) -> tuple[bool | None, list[str]]:
    """G8 legacy fan-in: both platforms bound to the same commit/source digest."""
    verdicts = verdicts_by_role.get("fan_in_artifact_contract", [])
    if not verdicts:
        return False, [
            "双平台同 commit fan-in 证据缺失: fan_in_artifact_contract role absent "
            "from the explicit evidence manifest; Windows runner and dual-platform "
            "same-commit evidence not produced"
        ]
    if not all(v["ok"] for v in verdicts):
        return False, [f for v in verdicts if not v["ok"] for f in v["violations"]]
    return True, []


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Aggregate G0-G9 release gate status from an explicit evidence "
                    "manifest (fail-closed).",
    )
    parser.add_argument("--evidence-manifest", required=True,
                        help="JSON manifest listing every consumed artifact path explicitly")
    parser.add_argument("--expected-commit", required=True,
                        help="Frozen candidate commit C_final")
    parser.add_argument("--expected-source-digest", required=True,
                        help="Frozen candidate source digest")
    parser.add_argument("--expected-registry-digest", required=True,
                        help="Frozen 117 capability registry digest")
    parser.add_argument("--expected-corpus-digests", required=True,
                        help='JSON object of corpus revision digests, e.g. {"benchmark": "<sha256>"}')
    parser.add_argument("--expected-version", default=None,
                        help="Expected release version (defaults to package.json)")
    parser.add_argument("--expected-producer-revision", default=None,
                        help="Optional producer revision every envelope must match")
    parser.add_argument("--output-dir", default="support/artifacts",
                        help="Where to write the status report (default: support/artifacts)")
    parser.add_argument("--report-only", action="store_true",
                        help="Write the report and exit 0 even when gates are blocked/unavailable. "
                             "Diagnostics only — never a release pass.")
    parser.add_argument("--quiet", action="store_true", help="Suppress human-readable summary")
    args = parser.parse_args(argv)

    manifest_path = Path(args.evidence_manifest)
    if not manifest_path.is_file():
        print(f"Release gate check failed: evidence manifest not found: {manifest_path}")
        return 1
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        print(f"Release gate check failed: cannot read evidence manifest: {exc}")
        return 1
    if not isinstance(manifest, dict):
        print("Release gate check failed: evidence manifest must be a JSON object")
        return 1

    try:
        corpus_digests = json.loads(args.expected_corpus_digests)
    except ValueError as exc:
        print(f"Release gate check failed: --expected-corpus-digests is not valid JSON: {exc}")
        return 1
    if not isinstance(corpus_digests, dict):
        print("Release gate check failed: --expected-corpus-digests must be a JSON object")
        return 1

    expected = {
        "commit": args.expected_commit,
        "source_digest": args.expected_source_digest,
        "registry_digest": args.expected_registry_digest,
        "corpus_digests": corpus_digests,
        "version": args.expected_version,
        "producer_revision": args.expected_producer_revision,
    }

    result = evaluate(manifest, expected)

    report = {
        "ok": result["release_ready"],
        "mode": "release-gate-status",
        "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "host_platform": sys.platform,
        "release_ready": result["release_ready"],
        "release_ready_hint": "false unless EVERY G0-G9 gate is passed on envelope-valid, "
                              "C_final-bound evidence; PowerShell-unexecuted and "
                              "Windows-runner evidence are blockers, never passes",
        "candidate_commit": args.expected_commit,
        "source_digest": args.expected_source_digest,
        "registry_digest": args.expected_registry_digest,
        "corpus_digests": corpus_digests,
        "expected_version": args.expected_version,
        "manifest_path": str(manifest_path.resolve()),
        "blockers": result["blockers"],
        "gates": result["gates"],
        "evidence": result["evidence"],
        "artifact_verdicts": {
            role: [
                {
                    "path": v["path"],
                    "ok": v["ok"],
                    "violations": v["violations"],
                }
                for v in verdicts
            ]
            for role, verdicts in result["artifact_verdicts"].items()
        },
        "manifest_errors": result["manifest_errors"],
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
