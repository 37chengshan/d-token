#!/usr/bin/env python3
"""N6 Tauri UI acceptance evidence recorder (optional helper, PR-9).

Main performs the REAL Tauri UI acceptance with Computer (eight pages,
1180x780 / 920x640, light/dark, zh/en, Reduce Motion, native actions,
restart restore, sidebar S1-S4, Optical assertions) and records observations
in a JSON file.  This recorder VALIDATES that evidence and writes the
structured, candidate-bound artifact.

It NEVER launches the app, never takes screenshots, never runs the UI, and
never fabricates evidence: any missing or inconsistent required field is
rejected (exit 1) and nothing is written.  Screenshot entries must carry a
real path and timestamp; ``browser_preview_only`` must be false (browser
preview may only supplement native evidence, VERIFICATION.md section 4 N6).

Usage:
  python3 support/scripts/run_tauri_ui_acceptance.py \
    --evidence <recorded-observations.json> \
    --candidate-commit <hex> --candidate-source-digest <sha256> \
    [--artifacts-dir support/artifacts] [--validate-only]
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
# Single source of truth for the N6 field contract lives in the orchestrator.
from run_final_candidate_verification import (  # noqa: E402
    COMMIT_RE,
    DIGEST_RE,
    tauri_ui_validate,
    utc_stamp,
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Validate and record Main's real Computer Tauri UI acceptance evidence (N6).",
    )
    parser.add_argument("--evidence", required=True,
                        help="JSON file with Main-recorded UI observations (real Computer captures)")
    parser.add_argument("--candidate-commit", required=True, help="Candidate commit (40-64 hex)")
    parser.add_argument("--candidate-source-digest", required=True,
                        help="sha256 of the ASCII hex tree id of the candidate commit")
    parser.add_argument("--artifacts-dir", default=str(Path(SCRIPT_DIR).parents[1] / "artifacts"),
                        help="Artifacts directory (default support/artifacts)")
    parser.add_argument("--validate-only", action="store_true",
                        help="Print the validation result and write nothing")
    args = parser.parse_args(argv)

    if not COMMIT_RE.fullmatch(args.candidate_commit):
        print(json.dumps({"mode": "error", "error": "--candidate-commit must be 40-64 hex"}, indent=2))
        return 2
    if not DIGEST_RE.fullmatch(args.candidate_source_digest):
        print(json.dumps({"mode": "error", "error": "--candidate-source-digest must be 64 hex sha256"}, indent=2))
        return 2

    evidence_path = Path(args.evidence)
    try:
        artifact = json.loads(evidence_path.read_text(encoding="utf-8-sig"))
    except (OSError, ValueError) as exc:
        print(json.dumps({"mode": "error", "error": f"cannot read --evidence: {exc}"}, indent=2))
        return 2
    if not isinstance(artifact, dict):
        print(json.dumps({"mode": "error", "error": "--evidence must be a JSON object"}, indent=2))
        return 2

    reasons: list[str] = []
    if str(artifact.get("candidate_commit", "")).lower() != args.candidate_commit.lower():
        reasons.append("candidate_commit does not match --candidate-commit")
    if str(artifact.get("source_digest", "")).lower() != args.candidate_source_digest.lower():
        reasons.append("source_digest does not match --candidate-source-digest")
    ok, field_reasons = tauri_ui_validate(artifact)
    if not ok:
        reasons.extend(field_reasons)
    if not isinstance(artifact.get("recorded_by"), str) or not artifact["recorded_by"]:
        reasons.append("recorded_by must name the recording actor (Main / Computer)")

    if reasons:
        print(json.dumps({"mode": "record", "ok": False, "error": "evidence rejected (never fabricated)",
                          "reasons": reasons}, indent=2))
        return 1
    if args.validate_only:
        print(json.dumps({"mode": "validate", "ok": True,
                          "detail": "recorded evidence is complete and bound to the candidate"}, indent=2))
        return 0

    artifacts_dir = Path(args.artifacts_dir)
    out_dir = artifacts_dir / "tauri-ui-acceptance"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"tauri-ui-acceptance-{utc_stamp()}.json"
    out_path.write_text(json.dumps(artifact, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"mode": "record", "ok": True, "artifact": str(out_path),
                      "detail": "real Computer UI evidence recorded; the orchestrator N6 consumes this artifact"}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
