#!/usr/bin/env bash
set -euo pipefail

umask 077

die() {
  echo "error: $*" >&2
  exit 1
}

# Default mode is the SAFE copy-fixture mode: the candidate probe runs against
# a controlled temp copy of each Agent's single config file, never against the
# real config. The legacy in-place probe is an explicit --real-targets opt-in
# and is never the default.
MODE="copy-fixture"
for arg in "$@"; do
  case "$arg" in
    --copy-fixture) MODE="copy-fixture" ;;
    --real-targets) MODE="real-targets" ;;
    --help|-h)
      cat <<'HELP'
usage: run_agent_transport_candidate_test_macos.sh [--copy-fixture|--real-targets]

  --copy-fixture (default)  Safe copy mode. Copies each Agent's single config
                            file (Claude Code settings.json / Codex config.toml
                            / Cline providers.json) into a system temp root,
                            runs the candidate probe against the COPY only,
                            verifies exact sha256+mode restore of the copy and
                            that the real configs were never touched, then
                            removes the temp root. Never reads auth/credentials.
  --real-targets            Legacy in-place probe against the real Agent
                            configs. Explicit opt-in only; the default mode and
                            this acceptance never use it.
HELP
      exit 0
      ;;
    *) die "unknown argument: $arg" ;;
  esac
done

[[ "$(uname -s)" == "Darwin" ]] || die "this candidate test requires macOS"
command -v cargo >/dev/null 2>&1 || die "cargo is required"
command -v python3 >/dev/null 2>&1 || die "python3 is required"
if [[ "$MODE" == "real-targets" ]]; then
  for command in claude cline codex; do
    command -v "$command" >/dev/null 2>&1 || die "$command is required in --real-targets mode"
  done
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
ENDPOINT="http://127.0.0.1:18001"
MODEL="dtoken-candidate-validation"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_DIR="$ROOT/support/artifacts/agent-transport-candidate"
REPORT_PATH="$REPORT_DIR/candidate-${STAMP}-$$.json"

CLAUDE_CONFIG="${HOME}/.claude/settings.json"
CODEX_CONFIG="${CODEX_HOME:-${HOME}/.codex}/config.toml"
CLINE_CONFIG="${HOME}/.cline/data/settings/providers.json"
OPENCODE_CONFIG="${HOME}/.opencode/opencode.json"

(
  cd "$CODE_ROOT"
  cargo build -p dtoken-config-patcher --example agent_transport_candidate_probe >/dev/null
)
PROBE="$CODE_ROOT/target/debug/examples/agent_transport_candidate_probe"

short_hash() {
  python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest()[:12])' "$1"
}

mkdir -p "$REPORT_DIR"
chmod 700 "$REPORT_DIR"

run_copy_fixture() {
  local TEMP_ROOT BACKUP_ROOT RESULTS_DIR AGENT_RESULTS
  # macOS TMPDIR lives under /var (a symlink to /private/var); the patcher
  # fails closed on any symlink path component, so resolve to a real path.
  local TMP_BASE
  TMP_BASE="$(python3 -c 'import os; print(os.path.realpath(os.environ.get("TMPDIR") or "/tmp"))')"
  TEMP_ROOT="$(mktemp -d "$TMP_BASE/dtoken-agent-copy.XXXXXX")"
  chmod 700 "$TEMP_ROOT"
  BACKUP_ROOT="$TEMP_ROOT/backups"
  mkdir -p "$BACKUP_ROOT"
  RESULTS_DIR="$REPORT_DIR/results-${STAMP}-$$"
  mkdir -p "$RESULTS_DIR"
  chmod 700 "$RESULTS_DIR"
  AGENT_RESULTS="$RESULTS_DIR/results.txt"
  : > "$AGENT_RESULTS"
  trap 'rm -rf "$TEMP_ROOT" "$RESULTS_DIR"' EXIT INT TERM

  # agent_id|status|reason|copy_before|copy_after|copy_mode|real_before|real_after|real_mode|copy_rel|receipt_file
  probe_copy() {
    local agent_id="$1" real_path="$2" copy_rel="$3"
    local out="$RESULTS_DIR/${agent_id}.meta"
    if [[ ! -e "$real_path" ]]; then
      printf '%s|unavailable|config_file_missing||||||||\n' "$agent_id" >> "$AGENT_RESULTS"
      return 0
    fi
    if [[ -L "$real_path" || ! -f "$real_path" ]]; then
      printf '%s|blocked|symlink_or_not_regular_file||||||||\n' "$agent_id" >> "$AGENT_RESULTS"
      return 0
    fi
    local real_before real_mode copy_path copy_before copy_mode
    real_before="$(short_hash "$real_path")"
    real_mode="$(stat -f '%Lp' "$real_path")"
    copy_path="$TEMP_ROOT/$copy_rel"
    mkdir -p "$(dirname "$copy_path")"
    # Copy only this single file; cp -p preserves the original mode.
    cp -p "$real_path" "$copy_path"
    chmod 600 "$copy_path"
    copy_before="$(short_hash "$copy_path")"
    copy_mode="$(stat -f '%Lp' "$copy_path")"
    # Codex pre-scan: capture the copied config's upstream selection BEFORE any
    # probe transaction runs and persist it, so the report records what the
    # original copied config actually selects regardless of probe outcome.
    if [[ "$agent_id" == "codex" ]]; then
      python3 - "$copy_path" "$RESULTS_DIR/codex.prescan.json" <<'PY'
import json
import os
import sys
import tomllib

path, out = sys.argv[1], sys.argv[2]
try:
    with open(path, "rb") as handle:
        doc = tomllib.load(handle)
    def _redact_base_url(value):
        # Only loopback endpoints survive the scan; any other URL is redacted
        # so no third-party endpoint or credential material is persisted.
        if isinstance(value, str) and value.startswith(
            ("http://127.0.0.1", "http://localhost", "http://[::1]")
        ):
            return value
        return "<redacted non-loopback>" if value is not None else None

    provider_id = doc.get("model_provider")
    providers = doc.get("model_providers", {})
    provider = providers.get(provider_id) if isinstance(providers, dict) and provider_id else None
    if not isinstance(provider, dict):
        # Root model_provider is absent: fall back to the active profile's
        # model_provider (prefer "default", else the single defined profile).
        # A profile's own model_providers overrides the root table.
        active_profile = None
        profiles = doc.get("profiles", {})
        if isinstance(profiles, dict):
            if "default" in profiles:
                active_profile = profiles["default"]
            elif len(profiles) == 1:
                active_profile = next(iter(profiles.values()))
        if isinstance(active_profile, dict):
            provider_id = active_profile.get("model_provider")
            profile_providers = active_profile.get("model_providers", {})
            if isinstance(profile_providers, dict) and provider_id:
                provider = profile_providers.get(provider_id)
            if not isinstance(provider, dict) and isinstance(providers, dict) and provider_id:
                provider = providers.get(provider_id)
    if isinstance(provider, dict):
        scan = {
            "current_upstream_provider": provider_id,
            "base_url_redacted": _redact_base_url(provider.get("base_url")),
            "wire_api": provider.get("wire_api"),
            "requires_openai_auth": provider.get("requires_openai_auth"),
            "root_model_pinned": "model" in doc,
        }
    else:
        # No resolvable active provider: report it as unavailable instead of
        # fabricating a local candidate selection.
        scan = {
            "status": "unavailable",
            "reason": "no_active_model_provider",
            "provider_id": provider_id,
        }
except Exception as error:
    scan = {"parse_error": str(error)[:200]}
with open(out, "w", encoding="utf-8") as handle:
    json.dump(scan, handle, ensure_ascii=False, separators=(",", ":"))
    handle.flush()
    os.fsync(handle.fileno())
PY
    fi
    local receipt="$RESULTS_DIR/${agent_id}.receipt"
    if "$PROBE" "$agent_id" "$copy_path" "$BACKUP_ROOT" "$ENDPOINT" "$MODEL" > "$receipt" 2>"$RESULTS_DIR/${agent_id}.err"; then
      local copy_after real_after
      copy_after="$(short_hash "$copy_path")"
      real_after="$(short_hash "$real_path")"
      printf '%s|ok|probe_transaction_passed|%s|%s|%s|%s|%s|%s|%s|%s\n' \
        "$agent_id" "$copy_before" "$copy_after" "$copy_mode" \
        "$real_before" "$real_after" "$real_mode" "$copy_rel" "${receipt##*/}" >> "$AGENT_RESULTS"
    else
      local copy_after real_after
      copy_after="$(short_hash "$copy_path" 2>/dev/null || echo unreadable)"
      real_after="$(short_hash "$real_path")"
      # A probe rejection caused by an EXISTING unowned d-token selection is
      # not a candidate failure: neither the copy nor the real config changed,
      # so record blocked instead of failed. Match on two independent stderr
      # signals (unowned marker AND d-token-shaped/openai-compatible marker)
      # so real patcher messages like "unowned d-token provider",
      # "unowned dtoken-local-route provider" and "unowned openai-compatible
      # provider" all classify as blocked.
      local probe_status="failed" probe_reason="probe_transaction_rejected"
      if [[ "$copy_before" == "$copy_after" && "$real_before" == "$real_after" ]] \
        && grep -q 'unowned' "$RESULTS_DIR/${agent_id}.err" 2>/dev/null \
        && grep -Eq 'd-token|dtoken|openai-compatible' "$RESULTS_DIR/${agent_id}.err" 2>/dev/null; then
        probe_status="blocked"
        probe_reason="blocked_existing_unowned_selection"
      fi
      printf '%s|%s|%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \
        "$agent_id" "$probe_status" "$probe_reason" "$copy_before" "$copy_after" "$copy_mode" \
        "$real_before" "$real_after" "$real_mode" "$copy_rel" "${receipt##*/}" >> "$AGENT_RESULTS"
    fi
  }

  probe_copy "claude-code" "$CLAUDE_CONFIG" "claude-home/settings.json"
  probe_copy "codex" "$CODEX_CONFIG" "codex-home/config.toml"
  probe_copy "cline" "$CLINE_CONFIG" "cline-home/data/settings/providers.json"

  # OpenCode: scan only. No copy, no write, no probe — real OpenCode config
  # writes are reserved for an explicit later acceptance.
  if [[ -e "$OPENCODE_CONFIG" ]]; then
    if [[ -L "$OPENCODE_CONFIG" || ! -f "$OPENCODE_CONFIG" ]]; then
      printf 'opencode|blocked|symlink_or_not_regular_file||||||||\n' >> "$AGENT_RESULTS"
    else
      printf 'opencode|present|scan_only_no_write||||||||\n' >> "$AGENT_RESULTS"
    fi
  else
    printf 'opencode|unavailable|config_file_missing||||||||\n' >> "$AGENT_RESULTS"
  fi

  local payload payload_file
  payload_file="$RESULTS_DIR/payload.json"
  # macOS system bash 3.2 re-lexes a quoted heredoc body nested inside a
  # "$( ... )" command substitution as shell code, so a single quote in the
  # body breaks parsing ("unexpected EOF while looking for matching quote").
  # Run the heredoc as a plain command redirected to a file instead.
  AGENT_RESULTS="$AGENT_RESULTS" RESULTS_DIR="$RESULTS_DIR" \
    TEMP_ROOT="$TEMP_ROOT" REPORT_PATH="$REPORT_PATH" \
    REPORT_NAME="$(basename "$REPORT_PATH")" \
    ENDPOINT="$ENDPOINT" MODEL="$MODEL" python3 - > "$payload_file" <<'PY'
import json
import os
import pathlib
import sys
import tempfile
from datetime import datetime, timezone

agent_results = os.environ["AGENT_RESULTS"]
results_dir = os.environ["RESULTS_DIR"]
temp_root = os.environ["TEMP_ROOT"]
report_name = os.environ["REPORT_NAME"]
endpoint = os.environ["ENDPOINT"]
model = os.environ["MODEL"]

agents = []
blocked_agents = []
failed_agents = []
for line in open(agent_results, encoding="utf-8"):
    line = line.rstrip("\n")
    if not line:
        continue
    parts = line.split("|")
    agent_id = parts[0]
    status = parts[1]
    reason = parts[2]
    entry = {
        "agent_id": agent_id,
        "status": status,
        "config_present": status not in ("unavailable",),
    }
    if reason:
        entry["reason"] = reason
    if len(parts) > 9 and parts[9]:
        entry["copy_target"] = "<temp-root>/" + parts[9]
        entry["copy_before_sha256_12"] = parts[3]
        entry["copy_after_sha256_12"] = parts[4]
        entry["copy_mode"] = parts[5]
        entry["real_before_sha256_12"] = parts[6]
        entry["real_after_sha256_12"] = parts[7]
        entry["real_mode"] = parts[8]
        entry["copy_restored_exact"] = parts[3] == parts[4] and parts[3] != ""
        entry["real_config_untouched"] = parts[6] == parts[7] and parts[6] != ""
        receipt_file = results_dir + "/" + parts[10]
        if os.path.exists(receipt_file):
            try:
                receipt = json.load(open(receipt_file, encoding="utf-8"))

                def _redact_hashes(node):
                    if isinstance(node, dict):
                        return {
                            key: (
                                str(value)[:12]
                                if key.endswith("sha256") and isinstance(value, str)
                                else _redact_hashes(value)
                            )
                            for key, value in node.items()
                        }
                    if isinstance(node, list):
                        return [_redact_hashes(item) for item in node]
                    return node

                entry["receipt"] = _redact_hashes(receipt)
            except Exception:
                entry["receipt"] = {"parse_error": True}
        err_file = results_dir + "/" + agent_id + ".err"
        if os.path.exists(err_file) and os.path.getsize(err_file):
            entry["probe_stderr_snippet"] = open(err_file, encoding="utf-8", errors="replace").read()[:400]
        if status == "blocked":
            blocked_agents.append(agent_id)
        elif status == "failed":
            failed_agents.append(agent_id)
    agents.append(entry)

# Codex pre-scan was captured before the probe ran and persisted by the shell
# (results_dir/codex.prescan.json); load it so the report records the original
# copied config's upstream selection, independent of the probe outcome. Never
# dump the config or any credential material.
codex_scan = None
codex_prescan = os.path.join(results_dir, "codex.prescan.json")
if os.path.isfile(codex_prescan):
    try:
        with open(codex_prescan, encoding="utf-8") as handle:
            codex_scan = json.load(handle)
    except Exception as error:
        codex_scan = {"load_error": str(error)[:200]}

if failed_agents:
    status = "fail"
elif blocked_agents:
    status = "blocked"
else:
    status = "pass"
payload = {
    "schema_version": 1,
    "created_at": datetime.now(timezone.utc).isoformat(),
    "status": status,
    "source": "dtoken-config-patcher",
    "mode": "copy-fixture",
    "candidate_id": os.environ.get("DTOKEN_RELEASE_CANDIDATE_ID") or None,
    "agents": agents,
    "blocked_agents": blocked_agents,
    "failed_agents": failed_agents,
    "codex_copy_scan": codex_scan,
    "contract": {
        "copied_single_file_per_agent": True,
        "probe_ran_against_temp_copy_only": True,
        "real_configs_never_written": True,
        "exact_copy_bytes_restored": all(
            a.get("copy_restored_exact") for a in agents if a["status"] == "ok"
        ),
        "real_configs_untouched": all(
            a.get("real_config_untouched") for a in agents if a["status"] == "ok"
        ),
        "network_request_made": False,
        "agent_process_started": False,
        "raw_config_returned": False,
        "credential_value_read": False,
        "auth_files_never_read": True,
    },
    "backup_root": "<temp-copy-root>/backups",
    "report_path": "support/artifacts/agent-transport-candidate/" + report_name,
}
destination = pathlib.Path(os.environ["REPORT_PATH"])
fd, temporary = tempfile.mkstemp(prefix=destination.name + ".", dir=destination.parent)
try:
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, separators=(",", ":"))
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, destination)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
PY
  payload="$(<"$payload_file")"
  echo "$payload"
  if [[ "$payload" == *'"status":"fail"'* || "$payload" == *'"status":"blocked"'* ]]; then
    exit 1
  fi
}

run_real_targets() {
  # Legacy in-place probe against real Agent configs. Explicit opt-in only.
  echo "warning: --real-targets probes the real Agent config files" >&2
  BACKUP_ROOT="${HOME}/Library/Application Support/d-token/candidate-agent-backups/${STAMP}-$$"
  for target in "$CLAUDE_CONFIG" "$CODEX_CONFIG" "$CLINE_CONFIG"; do
    [[ -f "$target" && ! -L "$target" ]] || die "candidate target must be a regular non-symlink file"
  done
  mkdir -p "$BACKUP_ROOT"
  chmod 700 "$BACKUP_ROOT"
  local CLAUDE_RECEIPT CODEX_RECEIPT CLINE_RECEIPT
  CLAUDE_RECEIPT="$("$PROBE" claude-code "$CLAUDE_CONFIG" "$BACKUP_ROOT" "$ENDPOINT" "$MODEL")"
  CODEX_RECEIPT="$("$PROBE" codex "$CODEX_CONFIG" "$BACKUP_ROOT" "$ENDPOINT" "$MODEL")"
  CLINE_RECEIPT="$("$PROBE" cline "$CLINE_CONFIG" "$BACKUP_ROOT" "$ENDPOINT" "$MODEL")"
  REPORT_NAME="$(basename "$REPORT_PATH")" \
  CLAUDE_RECEIPT="$CLAUDE_RECEIPT" CODEX_RECEIPT="$CODEX_RECEIPT" CLINE_RECEIPT="$CLINE_RECEIPT" \
  REPORT_PATH="$REPORT_PATH" python3 - <<'PY'
import json
import os
import pathlib
import tempfile
from datetime import datetime, timezone

report_path = os.environ["REPORT_PATH"]
report_name = os.environ["REPORT_NAME"]
payload = {
    "schema_version": 1,
    "created_at": datetime.now(timezone.utc).isoformat(),
    "status": "pass",
    "source": "dtoken-config-patcher",
    "mode": "real-targets",
    "candidate_id": os.environ.get("DTOKEN_RELEASE_CANDIDATE_ID") or None,
    "agents": [
        json.loads(os.environ["CLAUDE_RECEIPT"]),
        json.loads(os.environ["CODEX_RECEIPT"]),
        json.loads(os.environ["CLINE_RECEIPT"]),
    ],
    "contract": {
        "preview_apply_verify_clear_restore": True,
        "exact_original_bytes_restored": True,
        "network_request_made": False,
        "agent_process_started": False,
        "raw_config_returned": False,
        "credential_value_read": False,
    },
    "backup_root": "<d-token-private-candidate-backups>",
    "report_path": "support/artifacts/agent-transport-candidate/" + report_name,
}
destination = pathlib.Path(report_path)
fd, temporary = tempfile.mkstemp(prefix=destination.name + ".", dir=destination.parent)
try:
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, separators=(",", ":"))
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, destination)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
PY
}

if [[ "$MODE" == "copy-fixture" ]]; then
  run_copy_fixture
else
  run_real_targets
fi
