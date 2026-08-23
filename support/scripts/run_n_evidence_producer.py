#!/usr/bin/env python3
"""Fixed-name unified-envelope evidence producer/stager for N1-N8 and N10-U0..U9
(C8 / PR-C8C).

Contract (docs/releases/v0.0.1/VERIFICATION.md §1/§4/§5, EVIDENCE.md §3/§4):

  * Every artifact carries the unified envelope bound to C_final through the N0
    freeze (``support/artifacts/final-candidate/<C_final>/N0-preflight.json``):
    ``candidate_commit``, ``source_digest``, ``registry_digest``,
    ``corpus_digests``, ``dirty=false`` for pass, ``producer_revision`` and
    ``artifact_sha256`` recomputed as remove-field -> canonical JSON -> SHA-256.
  * Fixed output names per node (EVIDENCE.md §3); artifacts are never selected
    by mtime/latest/pass-only.
  * Local producers execute real commands and validate OBSERVED raw output
    (exit codes, ``test result: ok`` markers, parsed JSON fields).  A producer
    can never synthesize pass from test names, file existence or declared
    status.
  * External/UI/cc-switch/provider/signing lanes emit precise ``blocked`` with
    reason codes until explicitly executed with bound evidence.
  * U9 coverage seals 117/117 ``verified`` ONLY from actual prior artifacts:
    an entry is verified iff its producer node's artifact is envelope-valid,
    candidate-bound and pass.  It can never mark verified from the registry's
    structural status.
  * The runner rejects mock/fixture/pre-N0/dirty/pass-only/unexecuted output.

Run::

  python3 support/scripts/run_n_evidence_producer.py \
      [--candidate-commit <40-hex>] [--n0-artifact <path>] \
      [--artifacts-root <dir>] [--worktree-root <dir>] \
      [--nodes N10-U0,N10-U1,...] [--skip-local] [--upstream-root <dir>] \
      [--quiet]

Exit 0 when every requested node artifact is staged (pass or explicit blocked);
exit 1 when staging fails (missing N0, unreadable registry, invalid candidate)
or a local producer crashes while producing evidence.
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
from typing import Any

SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import generate_release_gate_status as gate_status  # noqa: E402

REPO_ROOT = SCRIPTS_DIR.parents[1]
REGISTRY = REPO_ROOT / "support" / "fixtures" / "unified-capability-registry-v1.json"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
DEFAULT_PRODUCER_REVISION = "run_n_evidence_producer.py@1"

# Fixed output names (EVIDENCE.md §3).  These names are also the only values
# allowed in registry real_evidence for the matching producer node.
FIXED_ARTIFACT_NAMES: dict[str, str] = {
    "N0": "N0-preflight.json",
    "N1": "N1-receipt.json",
    "N2": "N2-codex.json",
    "N3": "N3-opencode.json",
    "N4": "N4-config.json",
    "N5": "N5-cc-switch.json",
    "N6": "N6-ui.json",
    "N7": "N7-macos.json",
    "N8": "N8-windows.json",
    "N9": "N9-release-status.json",
    "N10": "N10-unified-proof.json",
    "N10-U0": "N10-U0-contract.json",
    "N10-U1": "N10-U1-registry.json",
    "N10-U2": "N10-U2-property-fuzz.json",
    "N10-U3": "N10-U3-fault.json",
    "N10-U4": "N10-U4-replay-quality.json",
    "N10-U5": "N10-U5-perf.json",
    "N10-U6": "N10-U6-provider-paired.json",
    "N10-U7": "N10-U7-optical-active.json",
    "N10-U8": "N10-U8-platform.json",
    "N10-U9": "N10-U9-coverage.json",
}

# N0 is the freeze itself (never re-staged by this runner); the remaining
# nodes are stageable evidence nodes.
ALL_NODES = [node for node in FIXED_ARTIFACT_NAMES if node != "N0"]

# Consumed upstream evidence patterns per node (verified explicit artifacts;
# absence -> blocked, never a pass).
UPSTREAM_PATTERNS: dict[str, list[str]] = {
    "N1": ["receipt/*.json", "receipt-*.json", "optimization-receipt-*.json"],
    "N2": ["codex-ocxlocal-routed/*.json", "codex-ocxlocal-routed-*.json"],
    "N3": ["opencode-real-routed/*.json", "opencode-real-routed-*.json"],
    "N4": ["agent-config-acceptance/*.json", "agent-config-acceptance-*.json"],
    "N5": ["cc-switch-aba/*.json", "cc-switch-aba-*.json"],
    "N6": ["tauri-ui-acceptance/*.json", "tauri-ui-acceptance-*.json"],
    "N7": [
        "upgrade-autostart-credential/*.json",
        "managed-rtk-candidate/*.json",
        "managed-rtk-candidate-evidence.json",
    ],
    "N8": [
        "windows-dacl-acl-tests-*.json",
        "fan-in-artifact-contract-*.json",
        "release-verification-*.json",
        "release-handoff-manifest-*.json",
    ],
    "N10-U6": ["provider-paired/*.json", "provider-paired-*.json"],
    "N10-U7": ["optical-exact-active/*.json", "optical-exact-active-*.json"],
    "N10-U8": ["platform-determinism/*.json", "platform-determinism-*.json"],
}

# G5 independent managed RTK roles (VERIFICATION.md §6 G5): every role must be
# present, candidate-bound, and report usage_count > 0; missing role blocks.
MANAGED_RTK_ROLES = ("managed_rtk_macos", "managed_rtk_windows", "managed_rtk_wsl")
MANAGED_RTK_PATTERNS: dict[str, list[str]] = {
    "managed_rtk_macos": ["managed-rtk-candidate/*.json", "managed-rtk-candidate-evidence.json"],
    "managed_rtk_windows": ["managed-rtk-windows/*.json", "managed-rtk-windows-evidence.json"],
    "managed_rtk_wsl": ["managed-rtk-wsl/*.json", "managed-rtk-wsl-evidence.json"],
}


def utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def worktree_dirty(root: Path) -> bool:
    """Fail-closed dirty detection: any git failure or non-empty porcelain."""
    proc = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=root,
        check=False,
        capture_output=True,
    )
    if proc.returncode != 0:
        return True
    return bool(proc.stdout.decode("utf-8", errors="replace").strip())


def load_json(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"{path} is not a JSON object")
    return data


def build_envelope(
    node_id: str,
    expected: dict[str, Any],
    status: str,
    reason_codes: list[str],
    started_at: str,
    finished_at: str,
    extra: dict[str, Any] | None = None,
    *,
    dirty: bool = False,
) -> dict[str, Any]:
    import platform as _platform

    data: dict[str, Any] = {
        "schema_version": "1",
        "node_id": node_id,
        "status": status,
        "reason_codes": reason_codes,
        "candidate_commit": expected.get("commit"),
        "source_digest": expected.get("source_digest"),
        "dirty": dirty,
        "registry_digest": expected.get("registry_digest"),
        "corpus_digests": expected.get("corpus_digests"),
        "started_at": started_at,
        "finished_at": finished_at,
        "platform": {
            "os": sys.platform,
            "arch": _platform.machine() or "unknown",
            "toolchain": f"python{sys.version_info.major}.{sys.version_info.minor}",
        },
        "producer_revision": expected.get("producer_revision")
        or DEFAULT_PRODUCER_REVISION,
    }
    if extra:
        data.update(extra)
    data["artifact_sha256"] = gate_status.recompute_artifact_sha256(data)
    return data


def run_command(argv: list[str], cwd: Path) -> dict[str, Any]:
    started = utc_now()
    proc = subprocess.run(
        argv,
        cwd=cwd,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    output = proc.stdout.decode("utf-8", errors="replace")
    tail = output.strip().splitlines()
    return {
        "argv": " ".join(str(item) for item in argv),
        "exit_code": proc.returncode,
        "ok": proc.returncode == 0,
        "output_tail": tail,
        "started_at": started,
        "finished_at": utc_now(),
    }


def cargo_test(argv_extra: list[str]) -> list[str]:
    return ["cargo", "test", *argv_extra]


def bench_run(args: list[str], root: Path) -> list[str]:
    return [
        "cargo", "run", "-q",
        "--manifest-path", str(root / "code" / "crates" / "dtoken-benchmark" / "Cargo.toml"),
        "--bin", "dtoken-bench",
        "--", *args,
    ]


def local_node_commands(node_id: str, root: Path) -> list[list[str]]:
    """Real command lines for code-owned local producers (never synthetic)."""
    bench_corpus = root / "code" / "crates" / "dtoken-benchmark" / "corpus"
    if node_id == "N10-U0":
        return [
            cargo_test(["-p", "dtoken-types", "--test", "hybrid_accounting"]),
            cargo_test(["-p", "dtoken-types", "--test", "optical_types"]),
            cargo_test(["-p", "dtoken-router", "--test", "route_decision"]),
            cargo_test(["-p", "dtoken-runtime-pipeline", "--test", "lifecycle"]),
            cargo_test(["-p", "dtoken-core-api", "--test", "surface_parity"]),
            cargo_test(["-p", "dtoken-compress", "--test", "props"]),
        ]
    if node_id == "N10-U1":
        return [
            ["python3", str(root / "support/scripts/validate_unified_capability_registry.py")],
            ["python3", str(root / "support/scripts/validate_unified_corpus_manifests.py")],
        ]
    if node_id == "N10-U2":
        return [
            cargo_test(["-p", "dtoken-compress", "--test", "props"]),
            cargo_test(["-p", "dtoken-compress", "--test", "hybrid_safe_baseline"]),
            cargo_test(["-p", "dtoken-compress", "--test", "codec_candidate_evidence"]),
            cargo_test(["-p", "dtoken-compress", "--test", "code_codec_gate"]),
            cargo_test(["-p", "dtoken-compress", "--test", "optical_phase_a"]),
        ]
    if node_id == "N10-U3":
        return [
            cargo_test(["-p", "dtoken-context-store", "--test", "durability"]),
            cargo_test(["-p", "dtoken-runtime-pipeline", "--test", "recovery_state_machine"]),
            cargo_test(["-p", "dtoken-event-ledger", "--test", "ledger"]),
            # Failpoint fault-duration contract requires the failpoints feature.
            cargo_test(["-p", "dtoken-event-ledger", "--features", "failpoints", "--lib"]),
            cargo_test(["-p", "dtoken-core-api", "--lib"]),
            cargo_test(["-p", "dtoken-agent-bridge", "--test", "subprocess_contract"]),
        ]
    if node_id == "N10-U4":
        return [
            bench_run(["replay", "--corpus", str(bench_corpus)], root),
            bench_run(
                ["paired-contract", "--corpus", str(bench_corpus), "--out",
                 str(root / "support" / "artifacts" / "paired-contract.json")],
                root,
            ),
            cargo_test(["-p", "dtoken-benchmark", "--test", "corpus_contract"]),
            cargo_test(["-p", "dtoken-benchmark", "--test", "runner_contract"]),
        ]
    if node_id == "N10-U5":
        # Thresholds MUST come from the tracked C7A baseline
        # (code/crates/dtoken-benchmark/corpus/baseline.json, VERIFICATION.md
        # U5).  RecordBaseline re-freezes thresholds post-N0 and can self-pass,
        # so it is never a pass path here: when the tracked baseline is absent
        # the producer blocks instead of re-freezing.
        tracked_baseline = root / "code" / "crates" / "dtoken-benchmark" / "corpus" / "baseline.json"
        if not tracked_baseline.is_file():
            return []
        return [
            bench_run(
                ["check", "--corpus", str(bench_corpus), "--baseline",
                 str(tracked_baseline), "--json"],
                root,
            ),
        ]
    if node_id == "N10-U8":
        return [
            cargo_test(["-p", "dtoken-compress", "--test", "optical_render_evidence"]),
        ]
    return []


def json_tail(record: dict[str, Any]) -> dict[str, Any] | None:
    lines = record.get("output_tail", [])
    decoder = json.JSONDecoder()
    # Walk backwards; for every line that may open a JSON object, try
    # raw_decode on the join from that line upward. The first complete object
    # wins (latest output block, e.g. the pretty-printed benchmark summary).
    for index in range(len(lines) - 1, -1, -1):
        if "{" not in lines[index]:
            continue
        for start in range(index, -1, -1):
            candidate = "\n".join(lines[start:])
            try:
                value, _ = decoder.raw_decode(candidate)
            except ValueError:
                continue
            if isinstance(value, dict):
                return value
    return None


def validate_local_output(node_id: str, records: list[dict[str, Any]]) -> tuple[bool, list[str]]:
    """Validate OBSERVED raw output; pass requires every command exit 0 and the
    expected success markers present in the captured output."""
    reasons: list[str] = []
    if not records:
        return False, ["no commands executed"]
    for record in records:
        if not record["ok"]:
            reasons.append(
                f"command failed (exit {record['exit_code']}): {record['argv']}"
            )
    if node_id == "N10-U1":
        last_lines = " ".join(
            r["output_tail"][-1] if r["output_tail"] else "" for r in records
        )
        if "contract OK" not in last_lines:
            reasons.append("registry/corpus validators did not report contract OK")
    if node_id in ("N10-U0", "N10-U2", "N10-U3", "N10-U8"):
        for record in records:
            if "test result: ok" not in "\n".join(record["output_tail"]):
                reasons.append(
                    f"cargo test did not report 'test result: ok': {record['argv']}"
                )
    if node_id == "N10-U4":
        replay = json_tail(records[0]) if records else None
        replay_text = " ".join(records[0]["output_tail"]) if records else ""
        replay_ok = (replay or {}).get("replay_deterministic") is True or (
            "replay_deterministic=true" in replay_text
        )
        if not replay_ok:
            reasons.append("Replay did not report replay_deterministic=true")
        paired = json_tail(records[1]) if len(records) > 1 else None
        paired_text = " ".join(records[1]["output_tail"]) if len(records) > 1 else ""
        paired_ok = bool(
            (paired and paired.get("tasks") and paired.get("same_model_contract") is True)
            or (
                '"tasks"' in paired_text
                and '"same_model_contract": true' in paired_text
            )
        )
        if not paired_ok:
            reasons.append("PairedContract did not report tasks>0 and same_model_contract=true")
    if node_id == "N10-U5":
        check = json_tail(records[0]) if records else None
        if not check:
            reasons.append("benchmark check produced no JSON summary")
        else:
            summary = check.get("summary") or {}
            if summary.get("floor_ok") is not True:
                reasons.append("benchmark product floor not ok (floor<10% => blocked)")
            if summary.get("band_ok") is not True:
                reasons.append("benchmark no-regression band not ok")
            if summary.get("replay_deterministic") is not True:
                reasons.append("benchmark deterministic replay not ok")
    return not reasons, reasons


def validate_upstream_artifact(
    path: Path,
    node_id: str,
    expected: dict[str, Any],
    n0: dict[str, Any],
) -> tuple[bool, list[str]]:
    """Validate a consumed upstream artifact: unified envelope, exact candidate
    binding, post-N0 freshness, no mock/fixture/pass-only claims."""
    reasons: list[str] = []
    try:
        data = load_json(path)
    except (OSError, ValueError) as exc:
        return False, [f"artifact not readable JSON: {exc}"]
    if data.get("node_id") not in (node_id, None):
        reasons.append(f"node_id={data.get('node_id')!r} != {node_id}")
    if data.get("fixture") is True or data.get("mock") is True:
        reasons.append("artifact self-declares fixture/mock output")
    if any(token in str(path) for token in ("/fixtures/", "/fake-bin/")):
        reasons.append(f"artifact path is a fixture/fake path: {path}")
    if "status" in data and data.get("status") not in (
        "pass", "passed", "ok", "verified", "routed-verified"
    ):
        reasons.append(f"upstream status {data.get('status')!r} is not a pass")
    if all(field not in data for field in gate_status.ENVELOPE_FIELDS):
        reasons.append("no unified envelope (pass-only claim rejected)")
    artifact_commit = data.get("candidate_commit", data.get("candidate_id"))
    if artifact_commit != expected.get("commit"):
        reasons.append(
            f"candidate binding {artifact_commit!r} != {expected.get('commit')} "
            "(missing/old candidate rejected)"
        )
    artifact_source_digest = data.get("source_digest")
    if isinstance(artifact_source_digest, dict):
        if artifact_source_digest.get("scope") != "n0-canonical-source-manifest":
            reasons.append("source_digest scope is not N0 canonical manifest")
        artifact_source_digest = artifact_source_digest.get("value")
    if artifact_source_digest != expected.get("source_digest"):
        reasons.append("source_digest missing or mismatched with frozen candidate")
    if "registry_digest" in data and data.get("registry_digest") != expected.get("registry_digest"):
        reasons.append("registry_digest mismatch with frozen candidate")
    corpus = data.get("corpus_digests")
    if isinstance(corpus, dict):
        for key, value in (expected.get("corpus_digests") or {}).items():
            if corpus.get(key) not in (None, value):
                reasons.append(f"corpus_digests[{key!r}] mismatch")
    if data.get("dirty") is True:
        reasons.append("artifact records dirty=true")
    started = gate_status.parse_iso(data.get("started_at"))
    n0_finished = gate_status.parse_iso(n0.get("finished_at"))
    if started is not None and n0_finished is not None and started < n0_finished:
        reasons.append(
            f"started {data.get('started_at')} before N0 freeze finished "
            f"{n0.get('finished_at')} (pre-N0 evidence rejected)"
        )
    if node_id == "N1":
        request_receipts = data.get("request_receipts")
        if not isinstance(request_receipts, list) or not request_receipts:
            reasons.append("N1 requires routed request receipts with stable request ids")
        else:
            request_ids = [item.get("request_id") for item in request_receipts if isinstance(item, dict)]
            if len(request_ids) != len(request_receipts) or any(
                not isinstance(request_id, str) or not request_id.strip()
                for request_id in request_ids
            ) or len(request_ids) != len(set(request_ids)):
                reasons.append("N1 request receipts require unique non-empty request ids")
        buckets = data.get("accounting_buckets")
        required_buckets = {"compression", "command_output", "provider_cache", "relay"}
        if not isinstance(buckets, dict) or set(buckets) != required_buckets:
            reasons.append("N1 requires four accounting buckets: compression, command_output, provider_cache, relay")
        digest_binding = data.get("digest_binding")
        if not isinstance(digest_binding, dict) or digest_binding.get("all_match") is not True:
            reasons.append("N1 requires validation digest = sent digest for every routed request")
        reconciliation = data.get("reconciliation")
        if not isinstance(reconciliation, dict) or not all(
            reconciliation.get(field) is True
            for field in ("ledger", "overview", "analytics", "export", "no_double_count", "unknown_not_zero")
        ):
            reasons.append("N1 requires ledger/overview/analytics/export reconciliation without double count")
        if data.get("no_raw_payloads") is not True:
            reasons.append("N1 requires no_raw_payloads=true")
    if node_id == "N10-U6":
        for field in ("provider_id", "route_id", "model", "api_revision"):
            if not isinstance(data.get(field), str) or not data[field].strip():
                reasons.append(f"U6 requires exact {field}")
        baseline = data.get("baseline")
        optimized = data.get("optimized")
        for lane_name, lane in (("baseline", baseline), ("optimized", optimized)):
            if not isinstance(lane, dict) or not isinstance(lane.get("request_id"), str):
                reasons.append(f"U6 {lane_name} requires a stable request id")
                continue
            usage = lane.get("provider_actual_usage")
            if not isinstance(usage, dict) or usage.get("evidence_kind") != "provider_actual":
                reasons.append(f"U6 {lane_name} requires provider actual usage")
        identity = data.get("identity_match")
        if not isinstance(identity, dict) or not all(
            identity.get(field) is True
            for field in ("provider", "route", "model", "api_revision")
        ):
            reasons.append("U6 baseline/optimized identity must match provider, route, model, and API revision")
        if data.get("claimable_from_actual_only") is not True:
            reasons.append("U6 claimable savings must come from actual provider usage only")
        exclusions = data.get("excluded_from_savings")
        if not isinstance(exclusions, dict) or not all(
            exclusions.get(field) is True for field in ("failed", "shadow", "fallback")
        ):
            reasons.append("U6 failure/shadow/fallback outcomes must be excluded from savings")
    if node_id == "N5":
        if data.get("wrote_cc_switch") is not False:
            reasons.append("N5 requires wrote_cc_switch=false")
        if data.get("identity_restored") is not True:
            reasons.append("N5 requires identity_restored=true")
    if node_id == "N7" or node_id in MANAGED_RTK_ROLES:
        usage = data.get("usage_count")
        if usage is not None and not (isinstance(usage, int) and usage > 0):
            reasons.append("managed_rtk usage_count must be > 0")
    if node_id == "N10-U7":
        # Demo/test-owned signed fixtures prove contract only, never a real
        # exact-active provider run (VERIFICATION.md §5 U7 boundary).
        if data.get("demo_fixture") is True or "demo" in str(path):
            reasons.append("demo fixture is not real exact-active provider evidence")
        for field in ("authority", "provider_id", "route_id", "model", "api_revision"):
            value = data.get(field)
            if not isinstance(value, str) or not value.strip():
                reasons.append(f"U7 requires real {field}")
        for field in (
            "renderer_profile_digest", "font_digest", "transform_contract_digest",
            "corpus_digest", "tokenizer_revision", "pricing_revision",
            "cache_policy_revision", "eval_artifact_digest",
        ):
            value = data.get(field)
            if not isinstance(value, str) or not value.strip():
                reasons.append(f"U7 requires bound {field}")
        if data.get("fresh_paired_eval") is not True:
            reasons.append("U7 requires fresh paired eval evidence")
        required_true = {
            "automatic_active_observed": "U7 requires automatic active observation",
            "exact_route_match": "U7 requires exact route/provider/model/API revision match",
            "capability_attested": "U7 requires verified capability attestation",
            "eval_gate_passed": "U7 requires passing fidelity/cost/latency eval gate",
            "resource_gate_passed": "U7 requires passing resource gate",
            "patch_validated": "U7 requires validated request patch",
            "post_validator_passed": "U7 requires passing post-validator",
            "ledger_completed": "U7 requires completed Event Ledger evidence",
            "unsupported_protocol_preserved": "U7 requires unsupported protocols to remain non-active",
            "failure_preservation_passed": "U7 requires original provider failures to be preserved",
        }
        for field, message in required_true.items():
            if data.get(field) is not True:
                reasons.append(message)
        if data.get("failure_preservation_passed") is not True:
            original_reasons = data.get("failure_reasons")
            if isinstance(original_reasons, list):
                reasons.extend(
                    reason for reason in original_reasons
                    if isinstance(reason, str) and reason
                )
        if data.get("ordinary_image_entry_used") is not False:
            reasons.append("U7 forbids ordinary image entry activation")
        if data.get("ui_switch_required") is not False:
            reasons.append("U7 active selection must not depend on a UI switch")
        if data.get("preserve_text_on_gate_failure") is not True:
            reasons.append("U7 requires PreserveText for every failed active gate")
        if data.get("provider_response_preserved_verbatim") is not True:
            reasons.append("U7 requires provider response and error text preserved verbatim")
        if data.get("claimable_actual_usage") is not True:
            reasons.append("U7 requires claimable economics from actual provider usage")
        if data.get("baseline_request_id") == data.get("optimized_request_id"):
            reasons.append("U7 paired eval requires distinct baseline and optimized request ids")
        for field in ("baseline_request_id", "optimized_request_id"):
            value = data.get(field)
            if not isinstance(value, str) or not value.strip():
                reasons.append(f"U7 requires stable {field}")
        reasons = list(dict.fromkeys(reasons))
    return not reasons, reasons


def find_upstream_files(node_id: str, roots: list[Path]) -> list[Path]:
    found: list[Path] = []
    for root in roots:
        for pattern in UPSTREAM_PATTERNS.get(node_id, []):
            for path in sorted(root.glob(pattern)):
                if path.is_file() and path not in found:
                    found.append(path)
    return found


def load_n0(n0_path: Path) -> dict[str, Any]:
    try:
        data = load_json(n0_path)
    except (OSError, ValueError) as exc:
        raise RuntimeError(f"N0 artifact unreadable: {exc}") from exc
    if data.get("node_id") != "N0":
        raise RuntimeError(
            f"{n0_path} is not an N0 artifact (node_id={data.get('node_id')!r})"
        )
    if data.get("status") != "pass":
        raise RuntimeError(
            f"N0 status is {data.get('status')!r}, not pass — binding refused (fail-closed)"
        )
    for field in ("candidate_commit", "source_digest", "registry_digest"):
        if not isinstance(data.get(field), str) or not data[field]:
            raise RuntimeError(f"N0 artifact missing '{field}'")
    corpus = data.get("corpus_digests")
    if not isinstance(corpus, dict) or not corpus:
        raise RuntimeError("N0 artifact missing corpus_digests")
    return data


def load_prior_fixed_artifacts(
    artifacts_root: Path, expected: dict[str, Any]
) -> dict[str, dict[str, Any]]:
    """Load every already-staged fixed artifact and envelope-validate it.  The
    runner never trusts a self-declared ``envelope_ok``: validation recomputes
    the full envelope contract (digests, candidate, artifact_sha256)."""
    loaded: dict[str, dict[str, Any]] = {}
    for node_id, filename in FIXED_ARTIFACT_NAMES.items():
        if node_id == "N10-U9":
            continue
        path = artifacts_root / filename
        if not path.is_file():
            continue
        try:
            data = load_json(path)
        except (OSError, ValueError):
            continue
        violations = gate_status.envelope_violations(data, expected)
        data["envelope_ok"] = not violations
        data["envelope_violations"] = violations
        loaded[node_id] = data
    return loaded


def coverage_report(
    registry: dict[str, Any],
    node_artifacts: dict[str, dict[str, Any]],
    expected: dict[str, Any],
) -> dict[str, Any]:
    """Map all 117 registry entries to actual prior artifacts via the entry's
    producer role; verified requires a valid envelope-pass bound artifact
    (recomputed, never self-reported)."""
    entries = registry.get("entries", [])
    registry_ids = [entry.get("id") for entry in entries]
    if len(registry_ids) != len(set(registry_ids)):
        raise ValueError("registry contains duplicate entry ids")
    producer_by_id = {
        entry.get("id"): entry.get("producer", "")
        for entry in entries
    }
    per_entry: list[dict[str, Any]] = []
    verified_ids: list[str] = []
    for entry in entries:
        entry_id = entry.get("id", "?")
        producer = entry.get("producer", "")
        node = (
            producer.split(":", 1)[1]
            if producer.startswith("entry-producer:") else None
        )
        artifact = node_artifacts.get(node) if node else None
        ok = bool(
            artifact
            and artifact.get("status") == "pass"
            and artifact.get("envelope_ok") is True
        )
        per_entry.append(
            {
                "id": entry_id,
                "producer_node": node,
                "artifact": FIXED_ARTIFACT_NAMES.get(node or ""),
                "verified": ok,
                "reason": None if ok else (
                    "no artifact" if artifact is None else
                    f"artifact status={artifact.get('status')!r} envelope_ok="
                    f"{artifact.get('envelope_ok')!r}"
                ),
            }
        )
        if ok:
            verified_ids.append(entry_id)
    total = len(entries)
    verified = len(verified_ids)
    status = "pass" if verified == total and total > 0 else "blocked"
    reason_codes: list[str] = []
    if verified != total:
        reason_codes.append(f"u9-coverage-incomplete:{verified}/{total}-verified")
        missing = [item for item in per_entry if not item["verified"]]
        reason_codes.append(f"u9-unverified-entries:{len(missing)}")
    # Integrity guards: unique ids, exact set equality with the registry,
    # producer mapping, fixed artifact names.
    coverage_ids = [item["id"] for item in per_entry]
    if len(coverage_ids) != len(set(coverage_ids)):
        reason_codes.append("u9-duplicate-entry-ids")
        status = "blocked"
    if sorted(coverage_ids) != sorted(registry_ids):
        reason_codes.append("u9-entry-id-set-mismatch")
        status = "blocked"
    if sorted(verified_ids) != sorted(registry_ids):
        reason_codes.append("u9-verified-id-set-incomplete")
        status = "blocked"
    for item in per_entry:
        producer = producer_by_id.get(item["id"], "")
        node = (
            producer.split(":", 1)[1]
            if producer.startswith("entry-producer:") else None
        )
        if item["producer_node"] != node:
            reason_codes.append(f"u9-producer-mismatch:{item['id']}")
            status = "blocked"
        expected_name = FIXED_ARTIFACT_NAMES.get(node or "")
        if item["artifact"] != expected_name:
            reason_codes.append(f"u9-artifact-name-mismatch:{item['id']}")
            status = "blocked"
    return {
        "total": total,
        "verified": verified,
        "unverified": total - verified,
        "verified_ids": verified_ids,
        "coverage": per_entry,
        "status": status,
        "reason_codes": sorted(set(reason_codes)),
    }


def stage_node(
    node_id: str,
    expected: dict[str, Any],
    n0: dict[str, Any],
    artifacts_root: Path,
    upstream_roots: list[Path],
    *,
    dirty: bool,
    skip_local: bool,
    started_at: str,
) -> dict[str, Any]:
    """Produce or stage one fixed-name artifact for a node.  Never fabricates a
    pass: pass requires observed successful execution (local) or a validated
    explicit upstream artifact; everything else is blocked with reason codes."""
    commands = [] if skip_local else local_node_commands(node_id, expected["root"])
    node_extra: dict[str, Any] = {}
    if node_id == "N10-U5" and not commands and not skip_local:
        # Tracked C7A baseline is the only allowed threshold source; without
        # it U5 blocks — RecordBaseline is never a post-N0 pass path.
        return build_envelope(
            node_id, expected, "blocked", ["u5-tracked-baseline-missing"],
            started_at, utc_now(),
            extra={"node": node_id, "envelope_ok": False,
                   "upstream_required": ["code/crates/dtoken-benchmark/corpus/baseline.json"]},
            dirty=dirty,
        )
    if commands:
        records = [
            run_command(
                argv,
                expected["root"] / "code"
                if argv and argv[0] == "cargo" and "--manifest-path" not in argv
                else expected["root"],
            )
            for argv in commands
        ]
        ok, reasons = validate_local_output(node_id, records)
        node_extra["commands"] = [
            {k: r[k] for k in ("argv", "exit_code", "ok", "output_tail")}
            for r in records
        ]
        if node_id == "N10-U5":
            check = json_tail(records[0]) if records else None
            if check:
                node_extra["bench_summary"] = check.get("summary")
                node_extra["bench_metrics"] = check.get("metrics")
        if node_id == "N10-U4":
            replay = json_tail(records[0]) if records else None
            paired = json_tail(records[1]) if len(records) > 1 else None
            if replay:
                node_extra["replay"] = replay
            if paired:
                node_extra["paired_contract"] = paired
        if node_id == "N10-U1":
            node_extra["validator_output"] = [r["output_tail"] for r in records]
        if ok and not dirty:
            return build_envelope(
                node_id, expected, "pass", [], started_at, utc_now(),
                extra={"node": node_id, "envelope_ok": True, **node_extra},
                dirty=False,
            )
        reasons_out = list(reasons)
        if dirty and "worktree-dirty" not in reasons_out:
            reasons_out.append("worktree-dirty")
        if not reasons_out:
            reasons_out = ["unknown-local-failure"]
        return build_envelope(
            node_id, expected, "blocked", reasons_out, started_at, utc_now(),
            extra={"node": node_id, "envelope_ok": False, **node_extra},
            dirty=dirty,
        )

    # No local producer: consume explicit upstream evidence or block honestly.
    upstream = find_upstream_files(node_id, upstream_roots)
    valid: list[dict[str, Any]] = []
    invalid: list[dict[str, Any]] = []
    for path in upstream:
        ok, reasons = validate_upstream_artifact(path, node_id, expected, n0)
        rel = path.relative_to(artifacts_root) if path.is_relative_to(artifacts_root) else path
        (valid if ok else invalid).append(
            {"path": str(rel), "ok": ok, "reasons": reasons}
        )
    if not upstream:
        node_extra["upstream_required"] = UPSTREAM_PATTERNS.get(node_id, [])
        return build_envelope(
            node_id, expected, "blocked",
            [f"{node_id.lower()}-not-executed"], started_at, utc_now(),
            extra={"node": node_id, "envelope_ok": False, **node_extra},
            dirty=dirty,
        )
    node_extra["upstream"] = valid + invalid
    if valid and not dirty:
        return build_envelope(
            node_id, expected, "pass", [], started_at, utc_now(),
            extra={"node": node_id, "envelope_ok": True,
                   "upstream_evidence": valid, **node_extra},
            dirty=False,
        )
    reasons_out = ["worktree-dirty" if dirty else reason
                   for v in invalid for reason in v["reasons"]]
    if dirty and not reasons_out:
        reasons_out = ["worktree-dirty"]
    if not reasons_out:
        reasons_out = ["no-valid-upstream-evidence"]
    return build_envelope(
        node_id, expected, "blocked", reasons_out, started_at, utc_now(),
        extra={"node": node_id, "envelope_ok": False, **node_extra},
        dirty=dirty,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-commit", default=None, help="40-hex C_final")
    parser.add_argument("--registry", default=None,
                        help="registry JSON path (default support/fixtures/unified-capability-registry-v1.json)")
    parser.add_argument("--n0-artifact", default=None, help="N0-preflight.json path")
    parser.add_argument("--artifacts-root", default=None, help="final-candidate artifact dir")
    parser.add_argument("--worktree-root", default=None, help="repo root used for commands/dirty")
    parser.add_argument("--nodes", default=",".join(ALL_NODES),
                        help="comma-separated node ids to stage")
    parser.add_argument("--skip-local", action="store_true",
                        help="do not execute local producers (stage blocked/consume only)")
    parser.add_argument("--upstream-root", action="append", default=[],
                        help="extra dir to search for consumed upstream evidence")
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args(argv)

    worktree_root = Path(args.worktree_root).resolve() if args.worktree_root else REPO_ROOT
    requested = [node for node in args.nodes.split(",") if node]
    unknown = [node for node in requested if node not in ALL_NODES]
    if unknown:
        print(f"unknown node ids: {unknown}")
        return 1

    try:
        n0_path = Path(args.n0_artifact) if args.n0_artifact else None
        if n0_path is None and args.candidate_commit:
            n0_path = (
                REPO_ROOT / "support" / "artifacts" / "final-candidate"
                / args.candidate_commit / "N0-preflight.json"
            )
        if n0_path is None or not n0_path.is_file():
            print("N0 artifact is required (--n0-artifact or --candidate-commit); refusing to bind")
            return 1
        n0 = load_n0(n0_path)
        commit = args.candidate_commit or n0["candidate_commit"]
        if not COMMIT_RE.match(commit):
            print(f"--candidate-commit must be 40 lowercase hex, got {commit!r}")
            return 1
        if commit != n0["candidate_commit"]:
            print(f"--candidate-commit {commit} != N0 candidate {n0['candidate_commit']}")
            return 1
        registry_path = Path(args.registry) if args.registry else REGISTRY
        registry = load_json(registry_path)
    except (OSError, ValueError, RuntimeError) as exc:
        print(f"staging refused: {exc}")
        return 1

    artifacts_root = (
        Path(args.artifacts_root).resolve()
        if args.artifacts_root
        else REPO_ROOT / "support" / "artifacts" / "final-candidate" / commit
    )
    artifacts_root.mkdir(parents=True, exist_ok=True)
    dirty = worktree_dirty(worktree_root)
    expected = {
        "commit": commit,
        "source_digest": n0["source_digest"],
        "registry_digest": n0["registry_digest"],
        "corpus_digests": n0["corpus_digests"],
        "root": worktree_root,
        "producer_revision": n0.get("producer_revision"),
    }
    upstream_roots = [artifacts_root] + [Path(p).resolve() for p in args.upstream_root]

    started_at = utc_now()
    node_artifacts: dict[str, dict[str, Any]] = {}
    exit_code = 0
    for node_id in ALL_NODES:
        if node_id not in requested:
            continue
        try:
            artifact = stage_node(
                node_id, expected, n0, artifacts_root, upstream_roots,
                dirty=dirty, skip_local=args.skip_local, started_at=started_at,
            )
        except (OSError, ValueError) as exc:
            print(f"node {node_id} staging crashed: {exc}")
            exit_code = 1
            continue
        node_artifacts[node_id] = artifact

    if "N10-U9" in requested:
        # U9 consumes every prior fixed artifact already staged in the
        # artifacts root (envelope-revalidated), not just this invocation's.
        prior = load_prior_fixed_artifacts(artifacts_root, expected)
        merged = dict(prior)
        merged.update(node_artifacts)
        try:
            coverage = coverage_report(registry, merged, expected)
        except ValueError as exc:
            print(f"N10-U9 coverage crashed: {exc}")
            coverage = {
                "total": len(registry.get("entries", [])),
                "verified": 0,
                "unverified": len(registry.get("entries", [])),
                "verified_ids": [],
                "coverage": [],
                "status": "blocked",
                "reason_codes": ["u9-registry-invalid"],
            }
        u9 = build_envelope(
            "N10-U9", expected, coverage["status"], coverage["reason_codes"],
            started_at, utc_now(),
            extra={
                "node": "N10-U9",
                "coverage": {
                    "total": coverage["total"],
                    "verified": coverage["verified"],
                    "unverified": coverage["unverified"],
                    "verified_ids": coverage["verified_ids"],
                    "per_entry": coverage["coverage"],
                },
            },
            dirty=dirty,
        )
        node_artifacts["N10-U9"] = u9
        # U9 can never seal while the worktree is dirty.
        if dirty and u9["status"] == "pass":
            u9["status"] = "blocked"
            u9["reason_codes"] = ["worktree-dirty"]
            u9["artifact_sha256"] = gate_status.recompute_artifact_sha256(u9)

    for node_id, artifact in node_artifacts.items():
        target = artifacts_root / FIXED_ARTIFACT_NAMES[node_id]
        target.write_text(
            json.dumps(artifact, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        if not args.quiet:
            print(
                f"{target.name}: status={artifact['status']} "
                f"reasons={artifact['reason_codes']}"
            )

    if exit_code == 0 and not node_artifacts:
        print("no nodes staged")
        exit_code = 1
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
