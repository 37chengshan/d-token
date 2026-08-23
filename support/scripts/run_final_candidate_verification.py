#!/usr/bin/env python3
"""PR-9 final-candidate verification DAG orchestration (N0 -> N1..N8 -> N9).

Contracts (docs/releases/v0.0.1/VERIFICATION.md, PR-9 slice):

* Acyclic DAG: N0 (candidate full quality gate, VERIFICATION.md section 1)
  runs first; N1..N8 are then settled in parallel (evidence consumption only);
  N9 aggregates (calls generate_release_gate_status.py in bound mode) and
  renders the three-conclusion judgment.
* Strong candidate binding: every consumed evidence artifact must carry the
  candidate commit AND source_digest (sha256 of the ASCII hex bytes of
  ``git rev-parse HEAD^{tree}``).  A changed HEAD/tree invalidates the old N0
  and every node artifact: stale artifacts are reported and never accepted
  (EVID-1).
* The orchestrator only EXECUTES N0.  N1..N8 are consume-only: it never runs
  verification, never launches the Tauri app, never touches cc-switch, and
  never fabricates UI evidence.  N5 (cc-switch A->B->A) and N6 (Tauri UI)
  consume artifacts recorded by Main via real Computer actions; N5 artifacts
  must prove ``wrote_cc_switch=false`` + ``identity_restored=true``, N6
  artifacts must carry real screenshots with timestamps and
  ``browser_preview_only=false``.
* N3 rejects the 2026-08-04 old G7 evidence (candidate_id=G7, no candidate
  binding): only artifacts bound to the current candidate with status pass
  count.
* N8 (Windows / G6 / G8) is an external node: absent evidence is reported
  ``external`` (honest unavailability, never impersonated); external/blocked
  keep the final conclusion ``blocked``.
* N9 preconditions: N0 verified, N1..N8 all settled, critical nodes
  (N1..N7) verified.  Conclusions are exactly one of:
  ``signed-release`` / ``unsigned-technical-preview`` / ``blocked``.
  ``unsigned-technical-preview`` MUST keep ``release_ready=false`` and
  ``distribution_ready_public=false``.
* Default mode is dry-run with zero side effects (no writes, no subprocess
  execution).  ``--execute`` is explicit; a dirty candidate worktree makes
  execute refuse to produce passes (dry-run still plans).

Usage:
  python3 support/scripts/run_final_candidate_verification.py \
    --candidate-worktree <path> [--execute] [--n0-spec-file <json>]
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

SCHEMA = "final-candidate-verification/node-artifact/v1"
COMMIT_RE = re.compile(r"[0-9a-fA-F]{40,64}")
DIGEST_RE = re.compile(r"[0-9a-fA-F]{64}")
EXPECTED_RELEASE_VERSION = "0.0.1"
CONCLUSIONS = ("signed-release", "unsigned-technical-preview", "blocked")
NODE_IDS = ("N1", "N2", "N3", "N4", "N5", "N6", "N7", "N8")
CRITICAL_NODES = ("N1", "N2", "N3", "N4", "N5", "N6", "N7")
ROOT = Path(__file__).resolve().parents[2]
SCRIPT_DIR = Path(__file__).resolve().parent
AGGREGATOR = SCRIPT_DIR / "generate_release_gate_status.py"
DEFAULT_N9_OUTPUT = ROOT / ".ai-bridge" / "release-iteration" / "final-candidate-evidence.md"
NODE_ARTIFACT_DIR = Path("final-candidate-verification") / "nodes"

EVIDENCE_TEMPLATE = """# 最终候选验证证据（PR-9 / N9）

- 生成时间：{{GENERATED_AT}}
- 模式：{{MODE}}
- 候选 commit：`{{CANDIDATE_COMMIT}}`
- source_digest：`{{SOURCE_DIGEST}}`
- worktree_dirty：{{WORKTREE_DIRTY}}
- **最终结论：{{CONCLUSION_LABEL}}**
- `release_ready`：{{RELEASE_READY}}
- `distribution_ready_public`：{{DISTRIBUTION_READY_PUBLIC}}
{{LABELS}}

## 1. N0 质量门（VERIFICATION.md §1）

{{N0_SUMMARY}}

## 2. N1–N8 节点证据（全部绑定同一候选）

{{NODE_TABLE}}

## 3. G0–G8 聚合（generate_release_gate_status.py，bound mode）

{{GATE_SUMMARY}}

## 4. 缺项 / 阻塞清单

{{MISSING_ITEMS}}

## 5. 判定规则

- 三种结论：正式签名发布（signed release）/ 未签名技术预览（unsigned technical preview）/ 不可发布（blocked）。
- 未签名技术预览必须保持 `release_ready=false`、`distribution_ready_public=false`；不得写成 G0–G8 全 passed 或 release-ready。
- 任何关键证据缺席或失败 → blocked；N8 external/blocked 保持最终 blocked。
- 旧候选（非 `{{CANDIDATE_COMMIT}}` / `{{SOURCE_DIGEST}}`）证据一律拒绝（EVID-1）。
"""

# --------------------------------------------------------------------------
# Node spec table — mirrors docs/releases/v0.0.1/VERIFICATION.md sections 3-4
# and the aggregator's PR-7 supplemental specs.  ``groups``: each group must
# have a bound, green artifact.  Kinds: executor (runs), consumer (consumes
# Main-produced evidence), manual-computer (consumes Main Computer evidence;
# NEVER executed here), external (runner/CI evidence; absence -> external).
# --------------------------------------------------------------------------
NODE_SPECS: dict[str, dict] = {
    "N1": {
        "label": "Receipt (PRODUCT-1..4, RUNTIME-1/2/5, CONTEXT-2, OPTICAL-4)",
        "kind": "consumer",
        "groups": [
            {"patterns": ["receipt/*.json", "receipt-*.json", "optimization-receipt-*.json"],
             "green": "ok_and_status", "ok_statuses": {"passed", "pass", "ok", "verified"}},
        ],
    },
    "N2": {
        "label": "Codex routed (AGENT-1, RUNTIME-6)",
        "kind": "consumer",
        "groups": [
            {"patterns": ["codex-ocxlocal-routed/*.json", "codex-ocxlocal-routed-*.json"],
             "green": "ok_and_status", "ok_statuses": {"routed-verified", "passed", "pass", "ok"}},
        ],
    },
    "N3": {
        "label": "OpenCode routed (AGENT-2, CONTEXT-2; old G7 rejected)",
        "kind": "consumer",
        "groups": [
            {"patterns": ["opencode-real-routed/real-routed-*.json", "opencode-real-routed-*.json"],
             "green": "opencode_routed"},
        ],
    },
    "N4": {
        "label": "Config acceptance (AGENT-4/6/7, CONFIG-1/2/3, SECURITY-6, PR-CBM2)",
        "kind": "consumer",
        "groups": [
            {"patterns": ["agent-config-acceptance/*.json", "agent-config-acceptance-*.json"],
             "green": "status_ok", "ok_statuses": {"verified", "ok", "passed", "pass"}},
        ],
    },
    "N5": {
        "label": "cc-switch A->B->A (AGENT-5, SECURITY-7) — Main Computer evidence only",
        "kind": "manual-computer",
        "groups": [
            {"patterns": ["cc-switch-aba/*.json", "cc-switch-aba-*.json"],
             "green": "cc_switch_aba"},
        ],
    },
    "N6": {
        "label": "Tauri UI acceptance (UI-1..7, PRODUCT-3/5/7, OPTICAL-1/6, EVID-5) — Main Computer evidence only",
        "kind": "manual-computer",
        "groups": [
            {"patterns": ["tauri-ui-acceptance/*.json", "tauri-ui-acceptance-*.json"],
             "green": "tauri_ui"},
        ],
    },
    "N7": {
        "label": "macOS (PLATFORM-1, CONFIG-4): upgrade/rollback + autostart/credential + managed RTK",
        "kind": "consumer",
        "groups": [
            {"patterns": ["upgrade-autostart-credential/upgrade-rollback-audit-*.json",
                          "upgrade-rollback-audit-*.json"],
             "green": "ok_and_status", "ok_statuses": {"verified", "passed", "pass", "ok"}},
            {"patterns": ["upgrade-autostart-credential/autostart-credential-roundtrip-*.json",
                          "autostart-credential-roundtrip-*.json"],
             "green": "ok_and_status", "ok_statuses": {"verified", "passed", "pass", "ok"}},
            {"patterns": ["managed-rtk-candidate/candidate-*.json"],
             "green": "status_pass"},
        ],
    },
    "N8": {
        "label": "Windows / G6 / G8 external (PLATFORM-2/3/5, OSS-4, PR-CBM2, PR-R2)",
        "kind": "external",
        "groups": [
            {"patterns": ["fan-in-artifact-contract-*.json"], "green": "fan_in"},
            {"patterns": ["windows-dacl-acl-tests-*.json"],
             "green": "ok_and_status", "ok_statuses": {"passed", "pass", "ok"}},
            {"patterns": ["release-verification-*.json"],
             "green": "ok_and_status", "ok_statuses": {"passed", "pass", "ok"}},
            {"patterns": ["release-handoff-manifest-*.json"],
             "green": "ok_and_status", "ok_statuses": {"passed", "pass", "ok"}},
        ],
    },
}

SIGNING_SPECS: dict[str, dict] = {
    "macos": {
        "label": "macOS Developer ID + notarization + Gatekeeper",
        "patterns": ["signing/macos-signing-*.json", "macos-signing-*.json"],
        "required": ["ok", "notarized", "gatekeeper"],
    },
    "windows": {
        "label": "Windows Authenticode signing",
        "patterns": ["signing/windows-signing-*.json", "windows-signing-*.json"],
        "required": ["ok", "authenticode"],
    },
}

# --------------------------------------------------------------------------
# N6 (Tauri UI) evidence requirements.  The optional recorder
# (run_tauri_ui_acceptance.py) owns the same constants; the orchestrator
# reuses them when the recorder is present and falls back to this copy
# otherwise (contract tests pin the two together).
# --------------------------------------------------------------------------
TUI_PAGES = {
    "overview", "install", "codebase-memory", "analytics",
    "optical", "doctor", "changes", "settings",
}
TUI_SIZES = {"1180x780", "920x640"}
TUI_THEMES = {"light", "dark"}
TUI_LOCALES = {"zh", "en"}
TUI_REQUIRED_BOOLS = (
    "reduce_motion_verified",
    "native_action_verified",
    "restart_restore_verified",
)
TUI_SIDEBAR_KEYS = ("S1", "S2", "S3", "S4")
TUI_OPTICAL_KEYS = (
    "no_production_active_toggle",
    "allow_active_false_prompt",
    "preview_readonly",
    "shadow_no_raw_payloads",
    "no_image_compression_entry",
    "active_off_by_default",
)


def _string_set(value, field: str, expected: set[str], reasons: list[str]) -> None:
    """Append a structured blocked reason unless ``value`` is a list of
    strings whose set equals ``expected``.

    Malformed JSON must never crash consumption: a non-list value, or a list
    holding unhashable/non-string entries, yields a blocked reason instead of
    raising TypeError from ``set(value)``."""
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        reasons.append(f"{field} must be a list of strings "
                       f"(malformed or unhashable entries are not evidence)")
        return
    if set(value) != expected:
        reasons.append(f"{field} must be exactly {sorted(expected)}")


def tauri_ui_validate(artifact: dict) -> tuple[bool, list[str]]:
    """Structural N6 predicate.  Never green on presence alone: every field
    must be present with the required value, and real screenshot evidence
    (path + timestamp) is mandatory.  Browser preview can only supplement.
    List-valued fields are checked for string elements before set comparison;
    unhashable/malformed JSON produces structured blocked reasons, never a
    TypeError."""
    reasons: list[str] = []
    if not isinstance(artifact, dict):
        return False, ["artifact is not a JSON object"]
    if artifact.get("ok") is not True:
        reasons.append("ok must be true")
    if str(artifact.get("status", "")).lower() not in ("verified", "passed", "pass"):
        reasons.append("status must be verified/passed")
    if artifact.get("browser_preview_only") is True:
        reasons.append("browser_preview_only must be false (native Computer evidence required)")
    _string_set(artifact.get("pages_tested"), "pages_tested", TUI_PAGES, reasons)
    window = artifact.get("window")
    if not isinstance(window, dict):
        reasons.append("window must be an object with sizes_tested")
    else:
        _string_set(window.get("sizes_tested"), "window.sizes_tested", TUI_SIZES, reasons)
    _string_set(artifact.get("themes_tested"), "themes_tested", TUI_THEMES, reasons)
    _string_set(artifact.get("locales_tested"), "locales_tested", TUI_LOCALES, reasons)
    for key in TUI_REQUIRED_BOOLS:
        if artifact.get(key) is not True:
            reasons.append(f"{key} must be true")
    sidebar = artifact.get("sidebar")
    if not isinstance(sidebar, dict) or any(sidebar.get(k) is not True for k in TUI_SIDEBAR_KEYS):
        reasons.append(f"sidebar {TUI_SIDEBAR_KEYS} must all be true")
    optical = artifact.get("optical")
    if not isinstance(optical, dict) or any(optical.get(k) is not True for k in TUI_OPTICAL_KEYS):
        reasons.append(f"optical {TUI_OPTICAL_KEYS} must all be true")
    shots = artifact.get("screenshots")
    if not isinstance(shots, list) or not shots:
        reasons.append("screenshots must be a non-empty list of real Computer captures")
    else:
        for i, shot in enumerate(shots):
            if not isinstance(shot, dict):
                reasons.append(f"screenshots[{i}] must be an object")
                continue
            if not isinstance(shot.get("path"), str) or not shot["path"]:
                reasons.append(f"screenshots[{i}].path must be a non-empty string")
            if not isinstance(shot.get("taken_at"), str) or not shot["taken_at"]:
                reasons.append(f"screenshots[{i}].taken_at must be a non-empty timestamp")
    return not reasons, reasons


# --------------------------------------------------------------------------
# N0 command set — docs/releases/v0.0.1/VERIFICATION.md section 1 (W5 gate).
# Data only: executed solely by --execute; dry-run/tests never run these.
# Main may substitute a JSON spec via --n0-spec-file.
# --------------------------------------------------------------------------
def default_n0_commands() -> list[dict]:
    code = str(ROOT / "code")
    desktop = str(ROOT / "code" / "apps" / "desktop")
    return [
        {"id": "structure",
         "description": "check_structure.py: v0.0.1 package completeness + active links (1.1)",
         "argv": ["python3", "support/scripts/check_structure.py"], "cwd": str(ROOT), "expected_rc": 0},
        {"id": "release-version",
         "description": f"check_release_version.py == {EXPECTED_RELEASE_VERSION} (1.2)",
         "argv": ["python3", "support/scripts/check_release_version.py", "--expected", EXPECTED_RELEASE_VERSION],
         "cwd": str(ROOT), "expected_rc": 0},
        {"id": "rust-fmt",
         "description": "cargo fmt --all -- --check (1.3)",
         "argv": ["cargo", "fmt", "--all", "--", "--check"], "cwd": code, "expected_rc": 0},
        {"id": "rust-clippy",
         "description": "cargo clippy --workspace --all-targets -- -D warnings (1.3)",
         "argv": ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
         "cwd": code, "expected_rc": 0},
        {"id": "rust-test",
         "description": "cargo test --workspace (1.3, 2026-08-07 baseline 936 pass/5 ignored)",
         "argv": ["cargo", "test", "--workspace"], "cwd": code, "expected_rc": 0},
        {"id": "frontend-tsc",
         "description": "npx tsc --noEmit (1.4)",
         "argv": ["npx", "tsc", "--noEmit"], "cwd": desktop, "expected_rc": 0},
        {"id": "frontend-vitest",
         "description": "npx vitest run (1.4, 2026-08-07 baseline 634 pass)",
         "argv": ["npx", "vitest", "run"], "cwd": desktop, "expected_rc": 0},
        {"id": "frontend-build",
         "description": "npm run build (1.4)",
         "argv": ["npm", "run", "build"], "cwd": desktop, "expected_rc": 0},
        {"id": "frontend-e2e",
         "description": "npx playwright test: Chromium+WebKit E2E (1.4, baseline 13 pass each)",
         "argv": ["npx", "playwright", "test"], "cwd": desktop, "expected_rc": 0},
        {"id": "no-touch-fixture",
         "description": "run_fixture_tests.sh (1.5)",
         "argv": ["bash", "support/scripts/run_fixture_tests.sh"], "cwd": str(ROOT), "expected_rc": 0},
        {"id": "no-touch-fake",
         "description": "run_fake_binary_tests.sh (1.5)",
         "argv": ["bash", "support/scripts/run_fake_binary_tests.sh"], "cwd": str(ROOT), "expected_rc": 0},
        {"id": "security-no-sk-secrets",
         "description": "no sk- secrets written to logs/config/exports (1.6)",
         "argv": ["bash", "-c",
                  "if grep -rInE --include='*.rs' --include='*.ts' --include='*.tsx' "
                  "--include='*.js' --include='*.json' --include='*.toml' "
                  "-E 'sk-[A-Za-z0-9]{20,}' code support/scripts --exclude-dir=target; then "
                  "echo 'sk- secret-shaped literal found' >&2; exit 1; else echo 'no sk- secrets'; fi"],
         "cwd": str(ROOT), "expected_rc": 0},
        {"id": "security-no-absolute-config-in-export",
         "description": "no absolute config paths enter exports (1.6)",
         "argv": ["bash", "-c",
                  "if grep -rInE --include='*.rs' --include='*.ts' --include='*.tsx' "
                  "-E '(export|Export|exports)[^;\\n]{0,120}(/Users/[A-Za-z0-9_]+|/home/[A-Za-z0-9_]+|C:\\\\Users)' "
                  "code --exclude-dir=target; then echo 'absolute config path in export' >&2; exit 1; "
                  "else echo 'no absolute config paths in exports'; fi"],
         "cwd": str(ROOT), "expected_rc": 0},
        {"id": "security-no-raw-payloads",
         "description": "no raw prompt/response/token literals in source (1.6)",
         "argv": ["bash", "-c",
                  "if grep -rInE --include='*.rs' --include='*.ts' --include='*.tsx' "
                  "-E 'raw_prompt|raw_response|raw prompt|raw response|Authorization: Bearer [A-Za-z0-9._~+/=-]{20,}' "
                  "code --exclude-dir=target; then echo 'raw payload/token literal found' >&2; exit 1; "
                  "else echo 'no raw payload literals'; fi"],
         "cwd": str(ROOT), "expected_rc": 0},
        {"id": "security-cc-switch-readonly",
         "description": "d-token never writes the cc-switch database (1.6)",
         "argv": ["bash", "-c",
                  "if grep -rInE --include='*.rs' --include='*.ts' --include='*.tsx' "
                  "-E 'cc-switch[^\"\\']*\\.db[^;\\n]*(remove|unlink|write|create|open\\([^)]*\\+)' code; then "
                  "echo 'cc-switch write path found' >&2; exit 1; else echo 'cc-switch read-only OK'; fi"],
         "cwd": str(ROOT), "expected_rc": 0},
        {"id": "security-no-chooser",
         "description": "no provider-chooser residue (1.6)",
         "argv": ["bash", "-c",
                  "if grep -rInEi 'chooser' code/apps code/crates/dtoken-proxy; then "
                  "echo 'chooser residue found' >&2; exit 1; else echo 'no chooser residue'; fi"],
         "cwd": str(ROOT), "expected_rc": 0},
        {"id": "loopback-bind-test",
         "description": "cargo test -p dtoken-proxy loopback_host_validation (1.7)",
         "argv": ["cargo", "test", "-p", "dtoken-proxy", "loopback_host_validation"],
         "cwd": code, "expected_rc": 0},
        {"id": "loopback-bind-grep",
         "description": "no non-loopback default bind (1.7, grep verbatim from VERIFICATION.md)",
         "argv": ["bash", "-c",
                  "if grep -rInE --include='*.rs' -E 'bind\\([^)]*\"0\\.0\\.0\\.0\"|bind\\([^)]*\"::\"' "
                  "code/crates code/apps/desktop/src-tauri; then "
                  "echo 'non-loopback default bind found' >&2; exit 1; "
                  "else echo 'loopback-only default bind OK'; fi"],
         "cwd": str(ROOT), "expected_rc": 0},
        {"id": "version-gate",
         "description": "no 0.2.0-beta.1 in active docs (1.8)",
         "argv": ["bash", "-c",
                  "if grep -rInE --include='*.md' --include='*.json' --include='*.toml' "
                  "'0\\.2\\.0-beta\\.1' docs code --exclude-dir=history --exclude-dir=archive "
                  "--exclude-dir=target; then echo 'stale 0.2.0-beta.1 reference' >&2; exit 1; "
                  "else echo 'version gate OK'; fi"],
         "cwd": str(ROOT), "expected_rc": 0},
        {"id": "oss-drift-pin-unique",
         "description": "OSS drift: repo pins unique and registered (1.9, OSS_MIGRATION.md D1-D5)",
         "argv": ["bash", "-c",
                  "python3 - <<'PY'\n"
                  "import pathlib, sys\n"
                  "root = pathlib.Path('.')\n"
                  "errs = []\n"
                  "notice_paths = [root/'docs/legal/THIRD_PARTY_NOTICES.md', root/'THIRD_PARTY_NOTICES.md']\n"
                  "def texts():\n"
                  "    for p in notice_paths:\n"
                  "        if p.is_file():\n"
                  "            yield p, p.read_text(errors='ignore')\n"
                  "all_notices = '\\n'.join(t for _, t in texts())\n"
                  "for name, needle in [('headroom', 'v0.34.0'), ('pxpipe', 'v0.12.0'),\n"
                  "                     ('lean-ctx', 'v3.9.17'), ('rtk', 'v0.45.0'),\n"
                  "                     ('cbm', 'v0.9.1-rc.1')]:\n"
                  "    if needle not in all_notices:\n"
                  "        errs.append(f'{name}: pin {needle} not registered in any NOTICE')\n"
                  "if errs:\n"
                  "    print('\\n'.join(errs), file=sys.stderr)\n"
                  "    sys.exit(1)\n"
                  "print('OSS drift pin checks OK')\n"
                  "PY"],
         "cwd": str(ROOT), "expected_rc": 0},
    ]


# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------
def utc_stamp() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def tree_digest(tree_id: str) -> str:
    """sha256 of the ASCII hex bytes of ``git rev-parse HEAD^{tree}``."""
    return hashlib.sha256(tree_id.encode("ascii")).hexdigest()


def load_json(path: Path | None) -> dict | None:
    if path is None or not path.is_file():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8-sig"))
        return data if isinstance(data, dict) else None
    except (OSError, ValueError):
        return None


def artifact_binding(artifact: dict | None) -> tuple[str | None, str | None]:
    """Extract (candidate_commit, source_digest); None on malformed values."""
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


def find_evidence_files(directory: Path, patterns: list[str]) -> list[Path]:
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
    return sorted(found, key=lambda p: p.stat().st_mtime, reverse=True)


def evidence_green(group: dict, artifact: dict | None, commit: str) -> bool:
    if not isinstance(artifact, dict):
        return False
    green = group.get("green")
    if green == "ok_and_status":
        return artifact.get("ok") is True and str(artifact.get("status", "")).lower() in group["ok_statuses"]
    if green == "status_ok":
        return str(artifact.get("status", "")).lower() in group["ok_statuses"]
    if green == "status_pass":
        return str(artifact.get("status", "")).lower() == "pass"
    if green == "cc_switch_aba":
        return (artifact.get("ok") is True
                and artifact.get("wrote_cc_switch") is False
                and artifact.get("identity_restored") is True)
    if green == "opencode_routed":
        # Old G7 (2026-08-04) carries candidate_id=G7 and NO candidate binding;
        # binding is enforced by the caller.  candidate_id presence mirrors
        # the aggregator's G7 semantics on top of the strong binding.
        return (str(artifact.get("status", "")).lower() in ("pass", "passed", "ok")
                and bool(artifact.get("candidate_id")))
    if green == "fan_in":
        return (artifact.get("ok") is True
                and artifact.get("same_commit") is True
                and artifact.get("same_source_digest") is True
                and str(artifact.get("expected_commit", "")).lower() == commit.lower())
    if green == "tauri_ui":
        ok, _ = tauri_ui_validate(artifact)
        return ok
    return False


def consume_node(node_id: str, artifacts_dir: Path, commit: str, digest: str) -> dict:
    """Settle one N1..N8 node by consuming bound evidence artifacts.

    Pure file consumption: never spawns a subprocess, never launches the UI,
    never touches cc-switch.  Artifacts whose binding differs from the current
    candidate are stale (EVID-1) and are reported but never accepted.
    """
    spec = NODE_SPECS[node_id]
    evidence: list[dict] = []
    stale: list[dict] = []
    missing: list[str] = []
    failed: list[str] = []
    any_files = False
    for gi, group in enumerate(spec["groups"]):
        files = find_evidence_files(artifacts_dir, group["patterns"])
        any_files = any_files or bool(files)
        bound: list[tuple[Path, dict]] = []
        for path in files:
            artifact = load_json(path)
            binding = artifact_binding(artifact)
            if (binding[0] or "").lower() == commit.lower() and (binding[1] or "").lower() == digest.lower():
                bound.append((path, artifact))
            else:
                stale.append({"group": gi, "path": str(path),
                              "candidate_commit": binding[0], "source_digest": binding[1]})
        if not bound:
            missing.append(f"group {gi} ({', '.join(group['patterns'])}) has no artifact bound to the candidate")
            continue
        path, artifact = bound[0]  # newest bound artifact
        ok = evidence_green(group, artifact, commit)
        evidence.append({"group": gi, "patterns": group["patterns"],
                         "path": str(path), "bound": True, "green": ok})
        if not ok:
            failed.append(f"group {gi} artifact {path.name} is not green")
    if evidence and not failed and not missing:
        status, ok = "verified", True
        detail = "all evidence groups green and bound to the candidate"
    elif spec["kind"] == "external" and not any_files:
        status, ok = "external", False
        detail = "external runner/CI evidence not obtainable on this host; explicitly external (never impersonated)"
    else:
        status, ok = "blocked", False
        bits = missing + failed
        detail = "; ".join(bits) if bits else "no bound evidence"
        if stale:
            detail += f" ({len(stale)} stale artifact(s) rejected — old candidate evidence is invalid, EVID-1)"
    return {
        "schema": SCHEMA, "node_id": node_id, "label": spec["label"], "kind": spec["kind"],
        "status": status, "ok": ok, "candidate_commit": commit, "source_digest": digest,
        "evidence": evidence, "stale": stale, "detail": detail,
        "generated_at": utc_stamp(),
    }


# --------------------------------------------------------------------------
# N0 execution (execute mode only)
# --------------------------------------------------------------------------
def load_n0_spec(path: Path | None) -> list[dict]:
    if path is None:
        return default_n0_commands()
    data = load_json(path)
    if not isinstance(data, dict) or not isinstance(data.get("commands"), list):
        raise ValueError(f"--n0-spec-file {path} must be a JSON object with a 'commands' list")
    return data["commands"]


def execute_n0(commands: list[dict], commit: str, digest: str, dirty: bool) -> dict:
    """Run the N0 quality gate.  Each command is executed with its cwd; a
    returncode different from expected_rc fails the gate."""
    results: list[dict] = []
    ok = True
    for cmd in commands:
        argv = list(cmd["argv"])
        cwd = cmd.get("cwd")
        if isinstance(cwd, str) and not Path(cwd).is_absolute():
            cwd = str(ROOT / cwd)
        started = datetime.datetime.now(datetime.timezone.utc)
        try:
            proc = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=3600)
            rc_ok = proc.returncode == int(cmd.get("expected_rc", 0))
            results.append({
                "id": cmd["id"], "description": cmd.get("description", ""),
                "argv": argv, "returncode": proc.returncode, "ok": rc_ok,
                "output_tail": (proc.stdout + proc.stderr).strip()[-2000:],
            })
        except (OSError, subprocess.TimeoutExpired) as exc:
            results.append({"id": cmd["id"], "description": cmd.get("description", ""),
                            "argv": argv, "returncode": None, "ok": False,
                            "output_tail": f"execution error: {exc}"})
        ok = ok and results[-1]["ok"]
    return {
        "schema": SCHEMA, "node_id": "N0", "label": "N0 candidate full quality gate (VERIFICATION.md §1)",
        "kind": "executor", "status": "verified" if ok else "failed", "ok": ok,
        "candidate_commit": commit, "source_digest": digest, "worktree_dirty": dirty,
        "commands": results, "detail": "all N0 gates passed" if ok else "one or more N0 gates failed",
        "generated_at": utc_stamp(),
    }


# --------------------------------------------------------------------------
# N9: aggregator call + signing + three-conclusion judgment
# --------------------------------------------------------------------------
def run_aggregator(artifacts_dir: Path, commit: str, digest: str) -> dict | None:
    argv = [sys.executable, str(AGGREGATOR),
            "--artifacts-dir", str(artifacts_dir),
            "--output-dir", str(artifacts_dir),
            "--expected-commit", commit,
            "--expected-source-digest", digest,
            "--report-only"]
    try:
        proc = subprocess.run(argv, capture_output=True, text=True, timeout=300)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {"error": f"aggregator invocation failed: {exc}"}
    if proc.returncode != 0:
        return {"error": f"aggregator exited {proc.returncode}: {(proc.stderr or proc.stdout)[-800:]}"}
    try:
        report = json.loads(proc.stdout)
    except ValueError:
        return {"error": "aggregator stdout is not the report JSON"}
    return report if isinstance(report, dict) else {"error": "aggregator report is not an object"}


def check_signing(artifacts_dir: Path, commit: str, digest: str) -> dict:
    result: dict[str, dict] = {}
    for platform, spec in SIGNING_SPECS.items():
        files = find_evidence_files(artifacts_dir, spec["patterns"])
        chosen: Path | None = None
        bound = False
        for path in files:
            artifact = load_json(path)
            binding = artifact_binding(artifact)
            if (binding[0] or "").lower() == commit.lower() and (binding[1] or "").lower() == digest.lower():
                chosen, bound = path, True
                break
        if not bound:
            result[platform] = {"ok": False, "state": "missing",
                                "detail": f"{spec['label']} evidence absent or not bound to the candidate",
                                "path": None}
            continue
        artifact = load_json(chosen)
        ok = all(artifact.get(key) is True for key in spec["required"])
        result[platform] = {"ok": ok, "state": "verified" if ok else "blocked",
                            "detail": f"evidence {chosen.name}",
                            "path": str(chosen)}
    return result


def n9_preconditions(n0_state: dict | None, node_states: dict) -> tuple[bool, list[str]]:
    reasons: list[str] = []
    if not (isinstance(n0_state, dict) and n0_state.get("status") == "verified" and n0_state.get("ok") is True):
        reasons.append("N0 quality gate is not verified")
    for nid in NODE_IDS:
        state = node_states.get(nid, {})
        if state.get("status") not in ("verified", "blocked", "external"):
            reasons.append(f"{nid} is not settled (status={state.get('status')!r})")
    for nid in CRITICAL_NODES:
        if node_states.get(nid, {}).get("status") != "verified":
            reasons.append(f"{nid} is a critical node and is not verified")
    return not reasons, reasons


def conclude(n0_state: dict | None, node_states: dict, gate: dict | None,
             signing: dict | None) -> dict:
    """Three-conclusion judgment (VERIFICATION.md §6), fail-closed."""
    missing: list[str] = []
    pre_ok, pre_reasons = n9_preconditions(n0_state, node_states)
    if not pre_ok:
        missing.extend(pre_reasons)
    if node_states.get("N8", {}).get("status") != "verified":
        missing.append("N8 external evidence not verified — external/blocked keeps the final conclusion blocked")
    if gate is None or "error" in (gate or {}):
        missing.append(f"G0-G8 aggregation unavailable: {(gate or {}).get('error', 'no report')}")
    elif not gate.get("release_ready"):
        blockers = gate.get("blockers") or []
        missing.append("G0-G8 aggregation is not release-ready: " + "; ".join(blockers[:8]))
    if missing:
        return {"conclusion": "blocked", "release_ready": False, "distribution_ready_public": False,
                "missing_items": missing, "labels": []}
    signing = signing or {"macos": {"ok": False}, "windows": {"ok": False}}
    if signing["macos"]["ok"] and signing["windows"]["ok"]:
        return {"conclusion": "signed-release", "release_ready": True, "distribution_ready_public": True,
                "missing_items": [], "labels": []}
    return {"conclusion": "unsigned-technical-preview", "release_ready": False,
            "distribution_ready_public": False,
            "labels": ["unsigned", "unnotarized"],
            "missing_items": [
                "macOS Developer ID + notarization + Gatekeeper evidence (external)",
                "Windows Authenticode signing evidence (external)",
            ]}


def render_evidence(template: str, ctx: dict) -> str:
    out = template
    for key, value in ctx.items():
        out = out.replace("{{" + key + "}}", str(value))
    return out


def evidence_context(n0_state: dict, node_states: dict, gate: dict | None,
                     signing: dict | None, conclusion: dict, commit: str,
                     digest: str, dirty: bool, mode: str, gate_path: str | None) -> dict:
    label = {"signed-release": "正式签名发布（signed release）",
             "unsigned-technical-preview": "未签名技术预览（unsigned technical preview，明确标注 unsigned/unnotarized）",
             "blocked": "不可发布（blocked）"}[conclusion["conclusion"]]
    labels = "\n".join(f"- label：`{lab}`" for lab in conclusion["labels"])
    n0_rows = []
    for cmd in n0_state.get("commands", []):
        state = "ok" if cmd.get("ok") else "failed"
        n0_rows.append(f"- `{cmd['id']}`: {state} — {cmd.get('description', '')}")
    n0_summary = "\n".join(n0_rows) or "- N0 未执行（plan）"
    node_rows = []
    for nid in NODE_IDS:
        st = node_states.get(nid, {})
        ev = st.get("evidence") or []
        paths = "; ".join(e["path"] for e in ev) or st.get("detail", "")
        node_rows.append(f"| {nid} | {st.get('label', '')} | {st.get('status', '?')} | {paths} |")
    node_table = "\n".join(node_rows)
    gate_summary = f"- `release_ready`: {gate.get('release_ready') if gate else 'n/a'}"
    if gate and "error" not in gate:
        gate_summary += "\n" + "\n".join(
            f"- {g['gate']}: {g['state']} — {g['detail']}" for g in gate.get("gates", []))
    elif gate:
        gate_summary += f"\n- error: {gate['error']}"
    missing = "\n".join(f"- {item}" for item in conclusion["missing_items"]) or "- 无"
    return {
        "GENERATED_AT": utc_stamp(), "MODE": mode,
        "CANDIDATE_COMMIT": commit, "SOURCE_DIGEST": digest,
        "WORKTREE_DIRTY": dirty, "CONCLUSION_LABEL": label,
        "RELEASE_READY": conclusion["release_ready"],
        "DISTRIBUTION_READY_PUBLIC": conclusion["distribution_ready_public"],
        "LABELS": labels, "N0_SUMMARY": n0_summary, "NODE_TABLE": node_table,
        "GATE_SUMMARY": gate_summary, "MISSING_ITEMS": missing,
        "AGGREGATOR_REPORT": gate_path or "n/a",
    }


def write_artifact(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


# --------------------------------------------------------------------------
# Entry points
# --------------------------------------------------------------------------
def resolve_candidate(worktree: str | None, commit_flag: str | None,
                      digest_flag: str | None) -> tuple[str, str, bool]:
    """Resolve the candidate binding.  With a worktree, read-only git commands
    provide HEAD and the tree digest; explicit flags must match.  Without a
    worktree both flags are required (caller asserts a clean checkout)."""
    if worktree:
        w = Path(worktree)
        if not (w / ".git").exists() and not (w / ".git").is_file():
            raise ValueError(f"--candidate-worktree {worktree} is not a git worktree")
        try:
            head = subprocess.run(["git", "-C", str(w), "rev-parse", "HEAD"],
                                  capture_output=True, text=True, check=True).stdout.strip()
            tree = subprocess.run(["git", "-C", str(w), "rev-parse", "HEAD^{tree}"],
                                  capture_output=True, text=True, check=True).stdout.strip()
            porcelain = subprocess.run(["git", "-C", str(w), "status", "--porcelain"],
                                       capture_output=True, text=True, check=True).stdout
        except (OSError, subprocess.CalledProcessError) as exc:
            raise ValueError(f"cannot resolve candidate from {worktree}: {exc}") from exc
        digest = tree_digest(tree)
        dirty = bool(porcelain.strip())
        if commit_flag is not None and commit_flag.lower() != head.lower():
            raise ValueError(f"--candidate-commit {commit_flag} does not match worktree HEAD {head}")
        if digest_flag is not None and digest_flag.lower() != digest.lower():
            raise ValueError(f"--candidate-source-digest {digest_flag} does not match computed {digest}")
        return head, digest, dirty
    if not commit_flag or not digest_flag:
        raise ValueError("provide --candidate-worktree or both --candidate-commit and --candidate-source-digest")
    if not COMMIT_RE.fullmatch(commit_flag):
        raise ValueError("--candidate-commit must be 40-64 hex")
    if not DIGEST_RE.fullmatch(digest_flag):
        raise ValueError("--candidate-source-digest must be 64 hex sha256")
    return commit_flag, digest_flag, False


def build_plan(commit: str, digest: str, dirty: bool, artifacts_dir: Path,
               commands: list[dict], n9_output: Path) -> dict:
    nodes = []
    would_be: dict[str, dict] = {}
    for nid in NODE_IDS:
        state = consume_node(nid, artifacts_dir, commit, digest)
        would_be[nid] = {"status": state["status"]}
        state["status"] = "planned"
        state["detail"] = f"plan only — would settle as {would_be[nid]['status']}: {state['detail']}"
        nodes.append(state)
    pre_ok, pre_reasons = n9_preconditions(
        {"status": "verified", "ok": True, "commands": commands},  # plan view: N0 would run
        would_be,
    )
    plan = {
        "mode": "plan",
        "candidate": {"commit": commit, "source_digest": digest,
                      "worktree_dirty": dirty,
                      "note": "dirty candidate: execute is refused; plan only, no pass generated"},
        "n0": {"node_id": "N0", "kind": "executor", "status": "planned",
               "commands": [{"id": c["id"], "description": c["description"],
                             "argv": c["argv"], "expected_rc": c.get("expected_rc", 0)}
                            for c in commands],
               "detail": "would execute the N0 quality gate (VERIFICATION.md §1)"},
        "nodes": nodes,
        "n9": {
            "preconditions_ok": pre_ok,
            "precondition_reasons": pre_reasons,
            "aggregator": {"script": str(AGGREGATOR), "bound_mode": True,
                           "expected_commit": commit, "expected_source_digest": digest,
                           "report_only": True},
            "template_output": str(n9_output),
            "conclusion_set": list(CONCLUSIONS),
        },
    }
    return plan


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="PR-9 final-candidate verification DAG orchestration (N0 -> N1..N8 -> N9).",
    )
    parser.add_argument("--candidate-worktree", help="Path to the clean candidate worktree (git HEAD + tree digest resolved here)")
    parser.add_argument("--candidate-commit", help="Candidate commit (40-64 hex); must match --candidate-worktree when both given")
    parser.add_argument("--candidate-source-digest", help="sha256 of the ASCII hex tree id; must match --candidate-worktree when both given")
    parser.add_argument("--artifacts-dir", default=str(ROOT / "support" / "artifacts"),
                        help="Directory scanned for node evidence (default support/artifacts)")
    parser.add_argument("--n0-spec-file", help="JSON file overriding the N0 command list (contract tests inject a fake gate)")
    parser.add_argument("--n9-template", help="Evidence markdown template (default: repo .ai-bridge template when present, else embedded)")
    parser.add_argument("--n9-output", default=str(DEFAULT_N9_OUTPUT),
                        help="Rendered final-candidate-evidence.md output (default .ai-bridge/release-iteration/final-candidate-evidence.md)")
    parser.add_argument("--expected-version", default=EXPECTED_RELEASE_VERSION,
                        help=f"Expected release version for the N0 version gate (default {EXPECTED_RELEASE_VERSION})")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--execute", action="store_true", help="Execute N0, settle N1..N8, run N9 (writes artifacts)")
    group.add_argument("--dry-run", action="store_true", help="Plan only, zero side effects (default)")
    parser.add_argument("--verbose", action="store_true", help="Print per-command N0 output in execute mode")
    args = parser.parse_args(argv)
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        commit, digest, dirty = resolve_candidate(args.candidate_worktree,
                                                  args.candidate_commit,
                                                  args.candidate_source_digest)
    except ValueError as exc:
        print(json.dumps({"mode": "error", "error": str(exc)}, indent=2))
        return 2

    artifacts_dir = Path(args.artifacts_dir)
    try:
        commands = load_n0_spec(Path(args.n0_spec_file) if args.n0_spec_file else None)
    except ValueError as exc:
        print(json.dumps({"mode": "error", "error": str(exc)}, indent=2))
        return 2

    n9_output = Path(args.n9_output)

    if not args.execute:
        plan = build_plan(commit, digest, dirty, artifacts_dir, commands, n9_output)
        print(json.dumps(plan, indent=2))
        return 0

    # ---- execute mode ---------------------------------------------------
    if dirty:
        print(json.dumps({"mode": "execute", "conclusion": "blocked",
                          "error": "candidate worktree is dirty — evidence cannot bind; "
                                   "clean the candidate or run dry-run (plan only, no pass generated)"},
                         indent=2))
        return 2

    stamp = utc_stamp()
    nodes_dir = artifacts_dir / NODE_ARTIFACT_DIR
    n0_state = execute_n0(commands, commit, digest, dirty)
    write_artifact(nodes_dir / f"N0-{stamp}.json", n0_state)
    if not n0_state["ok"]:
        print(json.dumps({
            "mode": "execute", "conclusion": "blocked", "release_ready": False,
            "distribution_ready_public": False,
            "n0_ok": False, "n0_status": n0_state["status"],
            "detail": "N0 quality gate failed — downstream nodes and N9 are not run (DAG: N0 first)",
            "n0": n0_state,
        }, indent=2))
        return 1

    node_states: dict[str, dict] = {}
    for nid in NODE_IDS:
        state = consume_node(nid, artifacts_dir, commit, digest)
        node_states[nid] = state
        write_artifact(nodes_dir / f"{nid}-{stamp}.json", state)

    gate = run_aggregator(artifacts_dir, commit, digest)
    signing = check_signing(artifacts_dir, commit, digest)
    conclusion = conclude(n0_state, node_states, gate, signing)

    gate_path = None
    if gate and "error" not in gate and gate.get("report_path"):
        gate_path = gate["report_path"]

    template_path = Path(args.n9_template) if args.n9_template else (
        DEFAULT_N9_OUTPUT if DEFAULT_N9_OUTPUT.is_file() else None)
    template = EVIDENCE_TEMPLATE
    if template_path is not None and template_path.is_file():
        template = template_path.read_text(encoding="utf-8")
    ctx = evidence_context(n0_state, node_states, gate, signing, conclusion,
                           commit, digest, dirty, "execute", gate_path)
    n9_output.parent.mkdir(parents=True, exist_ok=True)
    n9_output.write_text(render_evidence(template, ctx) + "\n", encoding="utf-8")

    n9_state = {
        "schema": SCHEMA, "node_id": "N9", "label": "final conclusion (three-conclusion judgment)",
        "kind": "conclusion",
        "status": "verified" if conclusion["conclusion"] != "blocked" else "blocked",
        "ok": conclusion["conclusion"] != "blocked",
        "conclusion": conclusion["conclusion"],
        "release_ready": conclusion["release_ready"],
        "distribution_ready_public": conclusion["distribution_ready_public"],
        "labels": conclusion["labels"],
        "missing_items": conclusion["missing_items"],
        "detail": (f"final conclusion {conclusion['conclusion']} "
                   f"(release_ready={conclusion['release_ready']}, "
                   f"distribution_ready_public={conclusion['distribution_ready_public']})"),
        "candidate_commit": commit, "source_digest": digest, "worktree_dirty": dirty,
        "node_states": {nid: node_states[nid]["status"] for nid in NODE_IDS},
        "n0_ok": n0_state["ok"],
        "gate_report": gate_path,
        "signing": signing,
        "evidence_output": str(n9_output),
        "generated_at": utc_stamp(),
    }
    write_artifact(nodes_dir / f"N9-{stamp}.json", n9_state)

    print(json.dumps({
        "mode": "execute",
        "conclusion": conclusion["conclusion"],
        "release_ready": conclusion["release_ready"],
        "distribution_ready_public": conclusion["distribution_ready_public"],
        "labels": conclusion["labels"],
        "missing_items": conclusion["missing_items"],
        "candidate_commit": commit, "source_digest": digest,
        "n0_ok": n0_state["ok"],
        "node_states": {nid: node_states[nid]["status"] for nid in NODE_IDS},
        "gate_report": gate_path,
        "signing": signing,
        "evidence_output": str(n9_output),
        "n9_artifact": str(nodes_dir / f"N9-{stamp}.json"),
    }, indent=2))
    return 0 if conclusion["conclusion"] != "blocked" else 1


if __name__ == "__main__":
    sys.exit(main())
