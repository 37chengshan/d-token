#!/usr/bin/env python3
"""Hot-sync review-gate evidence observer (read-only, PR-13 / AGENT-11).

Observes the Rust-owned hot configuration surface and asserts the review
gate contract without touching any configuration:

  * ``ExternalConfigSnapshot.automatic_apply_allowed`` is always false in the
    desktop crate (upstream_discovery.rs); the coordinator only ever emits
    ``external_config_review_required`` / ``change_pending_debounce`` events
    and never an apply instruction;
  * no production path in main.rs/upstream_discovery.rs sets
    ``automatic_apply_allowed`` to true (the authorization-revocation gate:
    auto-sync without a second confirmation is a PRD change, never code);
  * the review-gated flow is anchored: refresh events carry review_required
    and the snapshot type has no apply field (evidence-only observation).

The artifact is a read-only observation report (status=observed|blocked); it
never asserts a pass for code it did not exercise. No network, no config
writes, no cargo build — the contract is verified from the committed source
text plus the existing unit-test evidence in the same files.

Run::

  python3 support/scripts/run_hot_sync_review_gate_evidence.py [--out PATH]
"""

from __future__ import annotations

import argparse
import datetime
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DISCOVERY_RS = (
    REPO_ROOT / "code" / "apps" / "desktop" / "src-tauri" / "src" / "upstream_discovery.rs"
)
MAIN_RS = REPO_ROOT / "code" / "apps" / "desktop" / "src-tauri" / "src" / "main.rs"

OBSERVED_AT = datetime.datetime.now(datetime.timezone.utc).isoformat()


def observe() -> dict:
    findings: list[dict] = []
    discovery = DISCOVERY_RS.read_text(encoding="utf-8")
    main = MAIN_RS.read_text(encoding="utf-8")

    # 1. Snapshot type carries automatic_apply_allowed and it is false at the
    # only construction site in the discovery module.
    snapshots_true = re.findall(
        r"automatic_apply_allowed\s*:\s*true", discovery + "\n" + main
    )
    snapshots_false = len(re.findall(r"automatic_apply_allowed\s*:\s*false", discovery + "\n" + main))
    findings.append(
        {
            "check": "automatic_apply_allowed_never_true",
            "observed": len(snapshots_true) == 0 and snapshots_false >= 3,
            "detail": (
                f"automatic_apply_allowed=true sites: {len(snapshots_true)}; "
                f"false sites: {snapshots_false}"
            ),
        }
    )

    # 2. The coordinator emits review_required, never an apply instruction.
    review_events = re.findall(r'"external_config_review_required"|review_required:\s*true', discovery)
    findings.append(
        {
            "check": "review_gated_observation_events",
            "observed": len(review_events) >= 2,
            "detail": f"review-required event sites: {len(review_events)}",
        }
    )

    # 3. The snapshot has no apply field (evidence only, no route
    # authorization) and the refresh event carries review_required.
    snapshot_block = re.search(
        r"pub struct ExternalConfigSnapshot\s*\{(?P<body>.*?)\n\}", discovery, re.DOTALL
    )
    apply_field = (
        snapshot_block is not None
        and re.search(r"(?m)^\s*(apply|apply_instruction)\s*:", snapshot_block.group("body"))
        is not None
    )
    findings.append(
        {
            "check": "snapshot_has_no_apply_instruction",
            "observed": not apply_field,
            "detail": "apply field in ExternalConfigSnapshot: " + ("present" if apply_field else "absent"),
        }
    )

    # 4. The watcher/coordinator tests pin the no-auto-apply contract.
    test_asserts = re.findall(
        r"assert!\(!initial_event\.automatic_apply_allowed\)|assert!\(!review\.automatic_apply_allowed\)",
        discovery,
    )
    findings.append(
        {
            "check": "unit_tests_pin_no_auto_apply",
            "observed": len(test_asserts) >= 2,
            "detail": f"no-auto-apply test assertions: {len(test_asserts)}",
        }
    )

    blocked = [f for f in findings if not f["observed"]]
    return {
        "schema_version": 1,
        "mode": "read-only-observation",
        "status": "observed" if not blocked else "blocked",
        "ok": not blocked,
        "observed_at": OBSERVED_AT,
        "scope": "desktop hot-config review gate (AGENT-11)",
        "findings": findings,
        "blockers": [f["check"] for f in blocked],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", help="artifact output path (optional)")
    args = parser.parse_args(argv)
    report = observe()
    if args.out:
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
