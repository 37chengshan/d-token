#!/usr/bin/env python3
"""N0 -> N1-N8/N10 -> N9 final candidate verification orchestrator.

Implements the v0.0.1 verification DAG (docs/releases/v0.0.1/VERIFICATION.md):

    N0 (freeze + preflight) -> N1..N8 + N10 (parallel evidence) -> N9 (final)

Fail-closed contract (do not weaken):
  * Every machine artifact must carry the unified evidence envelope
    (schema_version, node_id, status, reason_codes, candidate_commit,
    source_digest, dirty=false, registry_digest, corpus_digests, started_at,
    finished_at, platform, producer_revision, artifact_sha256).
  * artifact_sha256 is recomputed (remove field -> canonical JSON -> SHA-256)
    and compared exactly; producer self-reported values are never trusted.
  * Missing, stale (started before the N0 freeze finished), old-candidate
    (candidate_commit != C_final), digest-mismatch, dirty, or pass-only
    (status=pass without a valid envelope) evidence is REJECTED.
  * Evidence is selected exclusively from the canonical fixed-name artifact
    paths under the final-candidate artifacts root (or an explicit
    --evidence-manifest).  mtime/latest/directory scanning is never used.
  * N0 must pass before any N1-N8/N10 evidence is consumed; N9 runs last and
    emits exactly one of: signed | public-unsigned | blocked.
  * unsigned always means release_ready=false and distribution_ready_public=false.

Usage:
  python3 support/scripts/run_final_candidate_verification.py \
    --candidate-commit <C_final> \
    --source-digest <sha256> \
    --registry-digest <sha256> \
    --corpus-digests '{"benchmark": "<sha256>", ...}' \
    [--expected-version 0.0.1] [--producer-revision <rev>] \
    [--artifacts-root support/artifacts/final-candidate/<C_final>] \
    [--output-dir <artifacts-root>] [--evidence-report-out .ai-bridge/...] \
    [--legacy-evidence role=path ...] \
    [--dry-run] [--report-only] [--quiet]

--dry-run validates the DAG (acyclic, N0 first, N1-N8/N10 middle, N9 last),
the candidate/digest arguments, and prints the full execution plan without
reading or writing any artifact.
"""
from __future__ import annotations

import argparse
import datetime
import json
import re
import sys
from pathlib import Path
from typing import Any

SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import generate_release_gate_status as gate_status  # noqa: E402

REPO_ROOT = SCRIPTS_DIR.parents[1]
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")


def _version_re() -> re.Pattern:
    return re.compile(
        r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
        r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
    )


# Canonical final-candidate artifact filenames (fixed names under the artifacts
# root; EVIDENCE.md §3).  N10 is the unified compression proof chain U0-U9.
NODE_ARTIFACTS: dict[str, list[str]] = {
    "N0": ["N0-preflight.json", "N0-provenance.json"],
    "N1": ["N1-receipt.json"],
    "N2": ["N2-codex.json"],
    "N3": ["N3-opencode.json"],
    "N4": ["N4-config.json", "N4-cbm-versions.json"],
    "N5": ["N5-cc-switch.json"],
    "N6": ["N6-ui.json"],
    "N7": ["N7-macos.json"],
    "N8": ["N8-windows.json", "fan-in-artifact-contract.json"],
    "N10-U0": ["N10-U0-contract.json"],
    "N10-U1": ["N10-U1-registry.json"],
    "N10-U2": ["N10-U2-property-fuzz.json"],
    "N10-U3": ["N10-U3-fault.json"],
    "N10-U4": ["N10-U4-replay-quality.json"],
    "N10-U5": ["N10-U5-perf.json"],
    "N10-U6": ["N10-U6-provider-paired.json"],
    "N10-U7": ["N10-U7-optical-active.json"],
    "N10-U8": ["N10-U8-platform.json"],
    "N10-U9": ["N10-U9-coverage.json"],
}

NODE_ORDER: list[str] = [
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
]

# DAG edges: N0 freezes first; N1-N8/N10 run after N0; N9 aggregates last.
DAG_EDGES: list[tuple[str, str]] = (
    [("N0", node) for node in NODE_ORDER if node != "N0"]
    + [(node, "N9") for node in NODE_ORDER if node != "N0"]
)

FAN_OUT_NODES = [node for node in NODE_ORDER if node != "N0"]

# N10 gate-aggregate roles (VERIFICATION.md §6).
N10_SUBROLES = [f"N10-U{index}" for index in range(10)]


def dag_is_acyclic(edges: list[tuple[str, str]]) -> tuple[bool, list[str]]:
    """Kahn topological sort; returns (acyclic, node order)."""
    nodes = sorted({source for source, _ in edges} | {target for _, target in edges})
    dependents: dict[str, list[str]] = {node: [] for node in nodes}
    indegree: dict[str, int] = {node: 0 for node in nodes}
    for source, target in edges:
        if source not in dependents or target not in indegree:
            return False, [f"edge references unknown node: {source} -> {target}"]
        dependents[source].append(target)
        indegree[target] += 1
    queue = [node for node in nodes if indegree[node] == 0]
    order: list[str] = []
    while queue:
        queue.sort()
        node = queue.pop(0)
        order.append(node)
        for target in dependents[node]:
            indegree[target] -= 1
            if indegree[target] == 0:
                queue.append(target)
    if len(order) != len(nodes):
        cyclic = sorted(node for node in nodes if indegree[node] != 0)
        return False, [f"DAG contains a cycle involving: {cyclic}"]
    return True, order


def validate_dag() -> tuple[bool, str]:
    acyclic, order = dag_is_acyclic(DAG_EDGES)
    if not acyclic:
        return False, " -> ".join(order)
    if order[0] != "N0":
        return False, f"N0 must run first, got {order[0]}"
    middle = [node for node in order if node not in ("N0", "N9")]
    if set(middle) != set(FAN_OUT_NODES):
        return False, f"fan-out nodes must be N1-N8/N10, got {middle}"
    if order[-1] != "N9":
        return False, f"N9 must run last, got {order[-1]}"
    return True, " -> ".join(order)


def build_manifest(artifacts_root: Path, legacy_evidence: list[str]) -> dict:
    """Explicit path manifest: every role maps to exact artifact paths."""
    artifacts: dict[str, list[str]] = {}
    for role, filenames in NODE_ARTIFACTS.items():
        artifacts[role] = [
            str((artifacts_root / filename).resolve()) for filename in filenames
        ]
    for item in legacy_evidence:
        role, sep, raw_path = item.partition("=")
        if not sep or role not in gate_status.LEGACY_ROLES:
            raise ValueError(
                f"--legacy-evidence must be <role>=<path> with role in "
                f"{sorted(gate_status.LEGACY_ROLES)}; got {item!r}"
            )
        artifacts.setdefault(role, []).append(str(Path(raw_path).resolve()))
    # The dual-platform fan-in contract is part of N8 evidence AND the G8
    # fan-in role; both reference the same explicit path.
    fan_in_path = artifacts_root / "fan-in-artifact-contract.json"
    artifacts["fan_in_artifact_contract"] = [str(fan_in_path.resolve())]
    return {"schema_version": "1", "artifacts": artifacts}


def load_artifact(path: Path) -> dict | None:
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
        return data if isinstance(data, dict) else None
    except (OSError, ValueError):
        return None


def signing_state(verdicts_by_role: dict[str, list[dict]]) -> dict[str, Any]:
    """Read signing attestations from N7 (macOS) and N8 (Windows)."""
    def nested(data: dict | None, *keys: str) -> Any:
        current: Any = data
        for key in keys:
            if not isinstance(current, dict):
                return None
            current = current.get(key)
        return current

    n7 = next((v["data"] for v in verdicts_by_role.get("N7", []) if v["ok"]), None)
    n8 = next((v["data"] for v in verdicts_by_role.get("N8", []) if v["ok"]), None)
    return {
        "macos_developer_id": nested(n7, "signing", "developer_id") is True,
        "macos_notarized": nested(n7, "signing", "notarized") is True,
        "macos_gatekeeper": nested(n7, "signing", "gatekeeper") is True,
        "windows_authenticode": nested(n8, "signing", "authenticode") is True,
    }


def u9_coverage(verdicts_by_role: dict[str, list[dict]]) -> dict[str, Any]:
    u9 = next((v["data"] for v in verdicts_by_role.get("N10-U9", []) if v["ok"]), None)
    if not isinstance(u9, dict):
        return {"ok": False, "registry_total": None, "verified_total": None,
                "reason": "N10-U9 coverage artifact missing or invalid"}
    registry_total = u9.get("registry_total")
    verified_total = u9.get("verified_total")
    if not isinstance(registry_total, int) or not isinstance(verified_total, int):
        return {"ok": False, "registry_total": registry_total,
                "verified_total": verified_total,
                "reason": "N10-U9 must carry integer registry_total/verified_total"}
    ok = registry_total == 117 and verified_total == 117
    return {
        "ok": ok,
        "registry_total": registry_total,
        "verified_total": verified_total,
        "reason": None if ok else "117/117 coverage seal not reached",
    }


def decide_conclusion(
    node_verdicts: dict[str, dict],
    gate_result: dict,
    signing: dict[str, bool],
    coverage: dict[str, Any],
) -> dict[str, Any]:
    """N9 three-conclusion decision table (VERIFICATION.md §7)."""
    nodes_failed = [node for node, verdict in node_verdicts.items() if not verdict["ok"]]
    gates_failed = [g for g in gate_result["gates"] if g["state"] != "passed"]
    blockers = list(gate_result["blockers"])
    if nodes_failed:
        blockers.insert(
            0, f"N-node evidence failed: {', '.join(sorted(nodes_failed))}"
        )
    if not coverage["ok"]:
        blockers.append(f"U9 coverage: {coverage['reason']}")

    if nodes_failed or gates_failed or blockers:
        return {
            "conclusion": "blocked",
            "release_ready": False,
            "distribution_ready_public": False,
            "reason_codes": [
                code
                for code in (
                    "node-evidence-failed" if nodes_failed else None,
                    "gate-failed" if gates_failed else None,
                    "blockers" if blockers else None,
                )
                if code
            ],
            "blockers": blockers,
        }

    signing_ok = (
        signing["macos_developer_id"]
        and signing["macos_notarized"]
        and signing["macos_gatekeeper"]
        and signing["windows_authenticode"]
    )
    if signing_ok:
        return {
            "conclusion": "signed",
            "release_ready": True,
            "distribution_ready_public": True,
            "reason_codes": ["g0-g9-passed", "n0-n10-passed", "117-117-verified",
                             "macos-developer-id-notarized", "windows-authenticode"],
            "blockers": [],
        }
    return {
        "conclusion": "public-unsigned",
        "release_ready": False,
        "distribution_ready_public": False,
        "reason_codes": ["g0-g9-passed", "n0-n10-passed", "117-117-verified",
                         "unsigned-unnotarized",
                         "signing-credentials-missing"],
        "blockers": [
            "All functional/quality/platform evidence passed but signing "
            "credentials are missing: unsigned builds never count as "
            "release-ready (release_ready=false, distribution_ready_public=false)"
        ],
    }


def plan_text(candidate: str, artifacts_root: Path, legacy_evidence: list[str]) -> str:
    ok, order = validate_dag()
    lines = [
        "Final candidate verification plan (N0 -> N1-N8/N10 -> N9)",
        f"  candidate_commit (C_final): {candidate}",
        f"  artifacts_root: {artifacts_root}",
        f"  DAG acyclic and ordered: {ok}",
        f"  topological order: {order}",
        "  evidence envelope (VERIFICATION.md §1): schema_version, node_id, status,",
        "    reason_codes, candidate_commit, source_digest, dirty=false,",
        "    registry_digest, corpus_digests, started_at, finished_at, platform,",
        "    producer_revision, artifact_sha256 (recomputed, never trusted)",
        "  rejection rules: missing / stale (started before N0 freeze) /",
        "    old-candidate / digest-mismatch / dirty / pass-only evidence",
        "  evidence selection: explicit fixed-name paths only; no mtime/latest",
        "  N9 conclusions: signed | public-unsigned | blocked;",
        "    unsigned always release_ready=false",
        "  artifacts:",
    ]
    for node in NODE_ORDER:
        for filename in NODE_ARTIFACTS[node]:
            lines.append(f"    {node:<8} {(artifacts_root / filename).resolve()}")
    for item in legacy_evidence:
        lines.append(f"    legacy   {item}")
    lines.append("  gates (aggregator): " + ", ".join(g for g, _ in gate_status.GATES))
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="N0 -> N1-N8/N10 -> N9 final candidate verification orchestration "
                    "(fail-closed).",
    )
    parser.add_argument("--candidate-commit", required=True,
                        help="Frozen candidate commit C_final (40 hex chars)")
    parser.add_argument("--source-digest", required=True,
                        help="Frozen candidate source digest (64 hex chars)")
    parser.add_argument("--registry-digest", required=True,
                        help="Frozen 117 capability registry digest (64 hex chars)")
    parser.add_argument("--corpus-digests", required=True,
                        help='JSON object of corpus revision digests, e.g. {"benchmark": "<sha256>"}')
    parser.add_argument("--expected-version", default=None,
                        help="Expected release version (defaults to code/apps/desktop/package.json)")
    parser.add_argument("--producer-revision", default=None,
                        help="Optional producer revision every envelope must match")
    parser.add_argument("--artifacts-root", default=None,
                        help="Final-candidate artifacts directory "
                             "(default: support/artifacts/final-candidate/<C_final>)")
    parser.add_argument("--output-dir", default=None,
                        help="Where to write the N9 report (default: artifacts-root)")
    parser.add_argument("--evidence-report-out", default=None,
                        help="Where to render the final-candidate evidence report "
                             "(default: <repo>/.ai-bridge/release-iteration/final-candidate-evidence.md)")
    parser.add_argument("--legacy-evidence", action="append", default=[],
                        metavar="ROLE=PATH",
                        help="Extra explicit legacy evidence path (repeatable); "
                             "roles: " + ", ".join(sorted(gate_status.LEGACY_ROLES)))
    parser.add_argument("--dry-run", action="store_true",
                        help="Validate the DAG and arguments, print the plan, write nothing")
    parser.add_argument("--report-only", action="store_true",
                        help="Write the N9 report and exit 0 even when blocked")
    parser.add_argument("--quiet", action="store_true", help="Suppress the human-readable report")
    args = parser.parse_args(argv)

    failures: list[str] = []
    if not COMMIT_RE.fullmatch(args.candidate_commit):
        failures.append(f"--candidate-commit must be 40 hex chars, got {args.candidate_commit!r}")
    if not DIGEST_RE.fullmatch(args.source_digest):
        failures.append(f"--source-digest must be 64 hex chars, got {args.source_digest!r}")
    if not DIGEST_RE.fullmatch(args.registry_digest):
        failures.append(f"--registry-digest must be 64 hex chars, got {args.registry_digest!r}")
    try:
        corpus_digests = json.loads(args.corpus_digests)
    except ValueError as exc:
        failures.append(f"--corpus-digests is not valid JSON: {exc}")
        corpus_digests = {}
    if not isinstance(corpus_digests, dict):
        failures.append("--corpus-digests must be a JSON object")
        corpus_digests = {}
    for key, value in corpus_digests.items():
        if not isinstance(value, str) or not DIGEST_RE.fullmatch(value):
            failures.append(f"--corpus-digests[{key!r}] must be a 64 hex digest")

    expected_version = args.expected_version
    if expected_version is None:
        package_json = REPO_ROOT / "code" / "apps" / "desktop" / "package.json"
        try:
            expected_version = json.loads(package_json.read_text(encoding="utf-8")).get("version")
        except (OSError, ValueError):
            failures.append("cannot read code/apps/desktop/package.json for expected version")
    if not isinstance(expected_version, str) or not _version_re().fullmatch(expected_version):
        failures.append(f"--expected-version must be SemVer, got {expected_version!r}")

    artifacts_root = Path(args.artifacts_root) if args.artifacts_root else (
        REPO_ROOT / "support" / "artifacts" / "final-candidate" / args.candidate_commit
    )
    output_dir = Path(args.output_dir) if args.output_dir else artifacts_root
    evidence_report_out = Path(args.evidence_report_out) if args.evidence_report_out else (
        REPO_ROOT / ".ai-bridge" / "release-iteration" / "final-candidate-evidence.md"
    )

    try:
        manifest = build_manifest(artifacts_root, args.legacy_evidence)
    except ValueError as exc:
        failures.append(str(exc))
        manifest = {"schema_version": "1", "artifacts": {}}

    dag_ok, dag_description = validate_dag()
    if not dag_ok:
        failures.append(f"DAG validation failed: {dag_description}")

    if failures:
        print("Final candidate verification plan rejected:")
        for failure in failures:
            print(f"  - {failure}")
        return 2

    if args.dry_run:
        print(plan_text(args.candidate_commit, artifacts_root, args.legacy_evidence))
        print(f"DAG ok: {dag_description}")
        print("Dry run complete: no artifacts were read or written.")
        return 0

    started_at = datetime.datetime.now(datetime.timezone.utc)
    expected = {
        "commit": args.candidate_commit,
        "source_digest": args.source_digest,
        "registry_digest": args.registry_digest,
        "corpus_digests": corpus_digests,
        "version": expected_version,
        "producer_revision": args.producer_revision,
    }

    # Phase 1: N0 gate (freeze + preflight).  Nothing downstream may be
    # consumed when N0 is missing or failed.
    n0_verdicts = gate_status.role_verdicts(
        "N0", manifest["artifacts"].get("N0", []), expected
    )
    n0_ok = bool(n0_verdicts) and all(v["ok"] for v in n0_verdicts)

    # Phase 2: fan-out evidence (N1-N8/N10).  Validated independently so the
    # report shows every failure, but only consumed for gates when N0 passed.
    verdicts_by_role: dict[str, list[dict]] = {"N0": n0_verdicts}
    for role in FAN_OUT_NODES:
        verdicts_by_role[role] = gate_status.role_verdicts(
            role, manifest["artifacts"].get(role, []), expected
        )

    stale = gate_status.stale_violations(verdicts_by_role)
    for role, messages in stale.items():
        for verdict in verdicts_by_role.get(role, []):
            if verdict["ok"] and verdict["data"] is not None:
                for message in messages:
                    if verdict["path"] in message:
                        verdict["ok"] = False
                        verdict["violations"].append(message)

    node_verdicts: dict[str, dict] = {}
    for role in NODE_ORDER:
        verdicts = verdicts_by_role[role]
        node_verdicts[role] = {
            "ok": n0_ok and all(v["ok"] for v in verdicts) if role != "N0" else all(
                v["ok"] for v in verdicts
            ),
            "artifacts": [
                {"path": v["path"], "ok": v["ok"], "violations": v["violations"]}
                for v in verdicts
            ],
        }

    # Phase 3: N9 aggregation — gates, signing, coverage, three conclusions.
    gate_result = gate_status.evaluate(manifest, expected)
    signing = signing_state(verdicts_by_role)
    coverage = u9_coverage(verdicts_by_role)
    conclusion = decide_conclusion(node_verdicts, gate_result, signing, coverage)

    finished_at = datetime.datetime.now(datetime.timezone.utc)
    n9_report = {
        "schema_version": "1",
        "node_id": "N9",
        "status": "pass" if conclusion["release_ready"] else "failed",
        "conclusion": conclusion["conclusion"],
        "release_ready": conclusion["release_ready"],
        "distribution_ready_public": conclusion["distribution_ready_public"],
        "reason_codes": conclusion["reason_codes"],
        "candidate_commit": args.candidate_commit,
        "source_digest": args.source_digest,
        "registry_digest": args.registry_digest,
        "corpus_digests": corpus_digests,
        "version": expected_version,
        "dirty": False,
        "started_at": started_at.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "finished_at": finished_at.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platform": {
            "os": sys.platform,
            "arch": _platform_arch(),
            "toolchain": f"python{sys.version_info.major}.{sys.version_info.minor}",
        },
        "producer_revision": _producer_revision(),
        "n0_gate": {"ok": n0_ok, "verdicts": node_verdicts["N0"]},
        "node_verdicts": node_verdicts,
        "gates": gate_result["gates"],
        "blockers": conclusion["blockers"],
        "signing": signing,
        "coverage": {key: value for key, value in coverage.items() if key != "reason"},
        "manifest": manifest,
        "artifact_paths": {
            role: [v["path"] for v in verdicts]
            for role, verdicts in sorted(verdicts_by_role.items())
        },
    }
    n9_report["artifact_sha256"] = gate_status.recompute_artifact_sha256(n9_report)

    output_dir.mkdir(parents=True, exist_ok=True)
    n9_path = output_dir / "N9-release-status.json"
    n9_path.write_text(json.dumps(n9_report, indent=2) + "\n", encoding="utf-8")

    _render_evidence_report(
        evidence_report_out, n9_report, artifacts_root, legacy=args.legacy_evidence
    )

    if not args.quiet:
        print(json.dumps(n9_report, indent=2))

    if not conclusion["release_ready"] and not args.report_only:
        return 1
    return 0


def _platform_arch() -> str:
    import platform
    return platform.machine() or "unknown"


def _producer_revision() -> str:
    return "run_final_candidate_verification.py@1"


def _render_evidence_report(
    out_path: Path, n9_report: dict, artifacts_root: Path, legacy: list[str]
) -> None:
    template = SCRIPTS_DIR / "final-candidate-evidence.template.md"
    if not template.is_file():
        return
    text = template.read_text(encoding="utf-8")
    verdict_lines = []
    for node in NODE_ORDER:
        verdict = n9_report["node_verdicts"][node]
        verdict_lines.append(
            f"- {node}: {'ok' if verdict['ok'] else 'FAILED'}"
            + (
                ""
                if verdict["ok"]
                else " — " + "; ".join(
                    v["path"] + ": " + ", ".join(v["violations"])
                    for v in verdict["artifacts"]
                )
            )
        )
    gate_lines = [
        f"- {g['gate']} {g['state']}: {g['detail']}" for g in n9_report["gates"]
    ]
    rendered = (
        text.replace("{{C_FINAL}}", str(n9_report.get("candidate_commit")))
        .replace("{{SOURCE_DIGEST}}", str(n9_report.get("source_digest")))
        .replace("{{REGISTRY_DIGEST}}", str(n9_report.get("registry_digest")))
        .replace("{{CORPUS_DIGESTS}}", json.dumps(n9_report.get("corpus_digests"), indent=2))
        .replace("{{VERSION}}", str(n9_report.get("version")))
        .replace("{{CONCLUSION}}", str(n9_report.get("conclusion")))
        .replace("{{RELEASE_READY}}", str(n9_report.get("release_ready")))
        .replace(
            "{{DISTRIBUTION_READY_PUBLIC}}",
            str(n9_report.get("distribution_ready_public")),
        )
        .replace("{{NODE_VERDICTS}}", "\n".join(verdict_lines))
        .replace("{{GATES}}", "\n".join(gate_lines))
        .replace("{{N9_REPORT_PATH}}", str(n9_report.get("artifact_paths", {}).get("N9", "")))
        .replace("{{ARTIFACTS_ROOT}}", str(artifacts_root))
        .replace("{{TIMESTAMP}}", datetime.datetime.now(datetime.timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        ))
    )
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(rendered, encoding="utf-8")


if __name__ == "__main__":
    sys.exit(main())
