#!/usr/bin/env bash
set -euo pipefail

CALLER_UMASK="$(umask)"
umask 077

usage() {
  cat <<'EOF'
Usage:
  run_opencode_real_routed_test_macos.sh --model PROVIDER/MODEL \
    --upstream-base-url URL (--api-key-ref REF | --upstream-no-auth) \
    [--compression-workload] [--pure]
  run_opencode_real_routed_test_macos.sh --list-models [PROVIDER]
  run_opencode_real_routed_test_macos.sh --dry-run --model PROVIDER/MODEL \
    --upstream-base-url URL (--api-key-ref REF | --upstream-no-auth)

The selected PROVIDER/MODEL and upstream must be the user's existing route.
This script does not select a free/default model and never reads auth.json.

Normal OpenCode config discovery order:
  1. --config-path / DTOKEN_OPENCODE_CONFIG_PATH (explicit effective config)
  2. OPENCODE_CONFIG (the user's existing OpenCode override)
  3. ${XDG_CONFIG_HOME:-$HOME/.config}/opencode/opencode.json

The effective config must already exist and be strict UTF-8 JSON. JSONC,
comments, duplicate keys, symlinks, and an existing dtoken-local-route provider
fail closed. The real run creates a persistent 0600 sibling backup, applies an
atomic minimal provider addition, and restores the exact original bytes only
when the temporary config hash still matches. A concurrent edit is never
overwritten; the backup is retained for manual recovery.

Non-secret environment equivalents:
  DTOKEN_OPENCODE_MODEL          Existing OpenCode model (PROVIDER/MODEL)
  DTOKEN_UPSTREAM_BASE_URL       Matching OpenAI-compatible upstream base URL
  DTOKEN_UPSTREAM_API_KEY_REF    keychain://SERVICE/ACCOUNT or
                                 os-credential://SERVICE/ACCOUNT
  DTOKEN_UPSTREAM_NO_AUTH=1      Explicitly declare an auth-free upstream
  DTOKEN_OPENCODE_CONFIG_PATH    Explicit effective normal OpenCode config
  DTOKEN_TEST_PORT               Temporary loopback listener port (default 18101)

Safety and evidence contract:
  - The user's normal OpenCode config and state are used; HOME and XDG state are
    not replaced or isolated.
  - auth.json, literal credentials, full prompts, full responses, and the nonce
    are never copied into the redacted report.
  - Only a dtoken-local-route provider (and, when already present, one
    enabled_providers entry) is added temporarily. Existing provider/model data
    remains unchanged and the config is restored byte-for-byte.
- d-token binds 127.0.0.1 and obtains upstream auth only from an OS credential
  reference. A literal API key is never accepted.
- --pure disables external OpenCode plugins for d-token-owned route evidence;
  the normal user config and state remain authoritative and are still restored.
  - Success requires OpenCode response proof plus matching route, request,
    provider, model, CCR, and four distinct valid JSONL evidence files.
  - The nonce must not occur in any d-token runtime persistence file.
  - The persistent redacted report is written below ignored
    support/artifacts/opencode-real-routed/. The sibling config backup remains.
  - --dry-run performs read-only validation: it does not edit config, create a
    backup/report, start d-token/OpenCode, or contact an upstream.

Only OpenAI-compatible upstreams are supported. If the existing OpenCode route
uses another protocol, do not guess an adapter.
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

require_value() {
  [[ $# -ge 2 && -n "$2" ]] || die "$1 requires a value"
}

sha256_file() {
  python3 - "$1" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
digest = hashlib.sha256()
with path.open("rb") as handle:
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
        digest.update(chunk)
print(digest.hexdigest())
PY
}

redact_home_path() {
  python3 - "$1" "${HOME:-}" <<'PY'
import os
import sys

path, home = sys.argv[1:]
path = os.path.abspath(os.path.expanduser(path))
home = os.path.abspath(os.path.expanduser(home)) if home else ""
if home and (path == home or path.startswith(home + os.sep)):
    print("~" + path[len(home):])
else:
    print("<explicit-config>/" + os.path.basename(path))
PY
}

MODEL="${DTOKEN_OPENCODE_MODEL:-}"
PORT="${DTOKEN_TEST_PORT:-18101}"
UPSTREAM_BASE_URL="${DTOKEN_UPSTREAM_BASE_URL:-}"
UPSTREAM_API_KEY_REF="${DTOKEN_UPSTREAM_API_KEY_REF:-}"
UPSTREAM_NO_AUTH="${DTOKEN_UPSTREAM_NO_AUTH:-0}"
CONFIG_PATH_OVERRIDE="${DTOKEN_OPENCODE_CONFIG_PATH:-}"
DRY_RUN=0
LIST_MODELS=0
LIST_PROVIDER=""
COMPRESSION_WORKLOAD=0
OPENCODE_PURE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --model)
      require_value "$1" "${2:-}"
      MODEL="$2"
      shift 2
      ;;
    --upstream-base-url)
      require_value "$1" "${2:-}"
      UPSTREAM_BASE_URL="$2"
      shift 2
      ;;
    --api-key-ref)
      require_value "$1" "${2:-}"
      UPSTREAM_API_KEY_REF="$2"
      shift 2
      ;;
    --upstream-no-auth)
      UPSTREAM_NO_AUTH=1
      shift
      ;;
    --config-path)
      require_value "$1" "${2:-}"
      CONFIG_PATH_OVERRIDE="$2"
      shift 2
      ;;
    --port)
      require_value "$1" "${2:-}"
      PORT="$2"
      shift 2
      ;;
    --compression-workload)
      COMPRESSION_WORKLOAD=1
      shift
      ;;
    --pure)
      OPENCODE_PURE=1
      shift
      ;;
    --dry-run|--preflight-only)
      DRY_RUN=1
      shift
      ;;
    --list-models)
      LIST_MODELS=1
      if [[ $# -ge 2 && "$2" != -* ]]; then
        LIST_PROVIDER="$2"
        shift 2
      else
        shift
      fi
      ;;
    --)
      shift
      break
      ;;
    -*)
      die "unknown option: $1"
      ;;
    *)
      [[ -z "$MODEL" ]] || die "model was provided more than once"
      MODEL="$1"
      shift
      ;;
  esac
done

[[ $# -eq 0 ]] || die "unexpected positional arguments: $*"

run_opencode_clean() (
  umask "$CALLER_UMASK"
  unset OPENCODE_CONFIG_CONTENT
  unset ANTHROPIC_BASE_URL
  unset ANTHROPIC_API_URL
  unset CLAUDE_CODE_PROXY
  if [[ "$OPENCODE_PURE" -eq 1 ]]; then
    command opencode --pure "$@"
  else
    command opencode "$@"
  fi
)

classify_opencode_failure() {
  python3 - "$1" "$2" "$3" <<'PY'
import json
import pathlib
import re
import sys

stdout_path, stderr_path, status = sys.argv[1:]
stdout_text = pathlib.Path(stdout_path).read_text(encoding="utf-8", errors="replace")
stderr_text = pathlib.Path(stderr_path).read_text(encoding="utf-8", errors="replace")
raw_text = stdout_text + "\n" + stderr_text
if not raw_text.strip():
    print("empty_output")
    raise SystemExit(0)

# Fixed labels only: never echo an arbitrary upstream error name or message.
error_names = {
    "apicallerror": "upstream_api_error",
    "ai_apicallerror": "upstream_api_error",
    "contextoverflowerror": "context_overflow",
    "providermodelnotfounderror": "model_not_found",
    "modelnotfounderror": "model_not_found",
    "provideriniterror": "provider_init_failed",
    "unknownerror": "opencode_unknown_error",
}
for line in raw_text.splitlines():
    try:
        event = json.loads(line)
    except json.JSONDecodeError:
        continue
    if not isinstance(event, dict) or event.get("type") != "error":
        continue
    error = event.get("error")
    if isinstance(error, dict):
        data = error.get("data")
        if isinstance(data, dict) and data.get("statusCode") in (400, 401, 403, 404, 408, 413, 429, 500, 502, 503, 504):
            print(f"upstream_http_{data['statusCode']}")
            raise SystemExit(0)
        for value in (error, data):
            if not isinstance(value, dict):
                continue
            for key in ("name", "code", "type"):
                identifier = value.get(key)
                if isinstance(identifier, str) and identifier.lower() in error_names:
                    print(error_names[identifier.lower()])
                    raise SystemExit(0)
    print("error_event_unclassified")
    raise SystemExit(0)

if int(status) == 0:
    print("none")
    raise SystemExit(0)
text = raw_text.lower()
rules = (
    ("cli_argument_unsupported", (r"unknown argument", r"unrecognized option", r"unknown option")),
    ("context_overflow", (r"context.*(exceed|length|limit|overflow)", r"too many tokens")),
    ("upstream_rate_limited", (r"rate.limit", r"too many requests")),
    ("request_timeout", (r"timed out", r"timeout")),
    ("config_rejected", (r"invalid config", r"configuration.*invalid", r"zoderror")),
    ("missing_environment", (r"environment variable.*(missing|not set|undefined)", r"missing.*environment variable")),
    ("provider_not_found", (r"provider.*not found", r"unknown provider")),
    ("model_not_found", (r"model.*not found", r"unknown model")),
    ("local_connection_failed", (r"connection refused", r"failed to connect", r"econnrefused")),
    ("database_locked", (r"database.*locked", r"sqlite.*busy", r"sqlite_busy")),
    ("permission_denied", (r"permission denied", r"eacces", r"operation not permitted")),
    ("shell_unbound_variable", (r"unbound variable", r"bad substitution")),
    ("mcp_startup_failed", (r"\bmcp\b.*(failed|error|unavailable)", r"(failed|error).*mcp")),
    ("config_file_unavailable", (r"config.*no such file", r"failed to (read|load).*config")),
    ("upstream_auth_failed", (r"unauthorized", r"authentication", r"api key")),
    ("plugin_or_hook_failed", (r"plugin", r"hook")),
)
for code, patterns in rules:
    if any(re.search(pattern, text) for pattern in patterns):
        print(code)
        break
else:
    print(
        "opencode_runtime_error"
        f"_stdout_{len(stdout_text.encode('utf-8'))}_{len(stdout_text.splitlines())}"
        f"_stderr_{len(stderr_text.encode('utf-8'))}_{len(stderr_text.splitlines())}"
    )
PY
}

[[ "$(uname -s)" == "Darwin" ]] || die "this test must run on macOS/Darwin"
command -v python3 >/dev/null 2>&1 || die "python3 is required"

if [[ "$LIST_MODELS" -eq 1 ]]; then
  [[ "$DRY_RUN" -eq 0 ]] || die "--list-models and --dry-run cannot be combined"
  command -v opencode >/dev/null 2>&1 || die "opencode is required"
  if [[ -n "$LIST_PROVIDER" ]]; then
    run_opencode_clean models "$LIST_PROVIDER"
    exit $?
  fi
  run_opencode_clean models
  exit $?
fi

[[ -z "${OPENCODE_CONFIG_CONTENT+x}" ]] || die "OPENCODE_CONFIG_CONTENT is active; unset it so the normal user config is authoritative"
[[ -n "$MODEL" ]] || die "select an existing model with --model PROVIDER/MODEL; use --list-models to discover IDs"
[[ "$MODEL" == */* ]] || die "model must use the existing OpenCode PROVIDER/MODEL form"
[[ "$MODEL" != *$'\n'* && "$MODEL" != *$'\r'* ]] || die "model contains a control character"
SELECTED_PROVIDER="${MODEL%%/*}"
UPSTREAM_MODEL="${MODEL#*/}"
[[ "$SELECTED_PROVIDER" =~ ^[A-Za-z0-9._-]+$ ]] || die "provider ID contains unsupported characters"
[[ -n "$UPSTREAM_MODEL" ]] || die "model ID cannot be empty"
[[ "$UPSTREAM_MODEL" != *$'\n'* && "$UPSTREAM_MODEL" != *$'\r'* ]] || die "model ID contains a control character"
[[ "$SELECTED_PROVIDER" != "dtoken-local-route" ]] || die "select the original upstream model, not the temporary d-token provider"

[[ "$PORT" =~ ^[0-9]+$ ]] || die "port must be numeric"
(( PORT >= 1024 && PORT <= 65535 )) || die "port must be between 1024 and 65535"
[[ -n "$UPSTREAM_BASE_URL" ]] || die "--upstream-base-url is required; it cannot be safely inferred without reading user config"

python3 - "$UPSTREAM_BASE_URL" <<'PY'
import ipaddress
import sys
from urllib.parse import urlsplit

value = sys.argv[1]
parsed = urlsplit(value)
if parsed.scheme not in {"http", "https"} or not parsed.hostname:
    raise SystemExit("error: upstream base URL must be an absolute HTTP(S) URL")
if parsed.username or parsed.password or parsed.query or parsed.fragment:
    raise SystemExit("error: upstream base URL must not contain credentials, query, or fragment")
if parsed.scheme == "http":
    try:
        loopback = ipaddress.ip_address(parsed.hostname).is_loopback
    except ValueError:
        loopback = parsed.hostname.lower() == "localhost"
    if not loopback:
        raise SystemExit("error: non-loopback upstreams must use HTTPS")
PY

if [[ -n "$UPSTREAM_API_KEY_REF" && "$UPSTREAM_NO_AUTH" == "1" ]]; then
  die "choose either --api-key-ref or --upstream-no-auth, not both"
fi
if [[ -z "$UPSTREAM_API_KEY_REF" && "$UPSTREAM_NO_AUTH" != "1" ]]; then
  die "declare upstream authentication with --api-key-ref or --upstream-no-auth"
fi
if [[ -n "$UPSTREAM_API_KEY_REF" ]]; then
  python3 - "$UPSTREAM_API_KEY_REF" <<'PY'
import sys
from urllib.parse import urlsplit

parsed = urlsplit(sys.argv[1])
if parsed.scheme not in {"keychain", "os-credential"}:
    raise SystemExit("error: macOS credential reference must use keychain:// or os-credential://")
if not parsed.hostname or not parsed.path.strip("/"):
    raise SystemExit("error: credential reference must include SERVICE/ACCOUNT")
if parsed.username or parsed.password or parsed.query or parsed.fragment:
    raise SystemExit("error: credential reference must not contain userinfo, query, or fragment")
PY
  CREDENTIAL_MODE="os_store_reference"
else
  CREDENTIAL_MODE="explicit_none"
fi

if [[ -n "$CONFIG_PATH_OVERRIDE" ]]; then
  CONFIG_PATH_INPUT="$CONFIG_PATH_OVERRIDE"
elif [[ -n "${OPENCODE_CONFIG:-}" ]]; then
  CONFIG_PATH_INPUT="$OPENCODE_CONFIG"
else
  [[ -n "${HOME:-}" ]] || die "HOME is required to discover the normal OpenCode config"
  CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
  CONFIG_PATH_INPUT="$CONFIG_HOME/opencode/opencode.json"
  if [[ ! -e "$CONFIG_PATH_INPUT" && -e "$CONFIG_HOME/opencode/opencode.jsonc" ]]; then
    die "the discovered OpenCode config is JSONC; this transaction requires strict opencode.json and fails closed"
  fi
fi

CONFIG_PATH="$(python3 - "$CONFIG_PATH_INPUT" <<'PY'
import os
import sys

value = sys.argv[1]
if not value or "\x00" in value or "\n" in value or "\r" in value:
    raise SystemExit("error: config path is empty or contains a control character")
print(os.path.abspath(os.path.expanduser(value)))
PY
)"
[[ "$(basename "$CONFIG_PATH")" != "auth.json" ]] || die "auth.json is never a valid config transaction target"

# Strict, read-only compatibility validation. No config values are printed.
CONFIG_PREFLIGHT="$(python3 - "$CONFIG_PATH" "$PORT" "$UPSTREAM_MODEL" <<'PY'
import json
import os
import pathlib
import stat
import sys

path = pathlib.Path(sys.argv[1])
port = int(sys.argv[2])
requested_model = sys.argv[3]
if path.is_symlink():
    raise SystemExit("error: OpenCode config symlinks are rejected for conflict-safe atomic restore")
if not path.is_file():
    raise SystemExit("error: the normal OpenCode opencode.json does not exist as a regular file")

def reject_duplicates(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON object key")
        result[key] = value
    return result

try:
    raw = path.read_bytes()
    text = raw.decode("utf-8")
    value = json.loads(text, object_pairs_hook=reject_duplicates)
except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError):
    raise SystemExit("error: OpenCode config must be readable strict UTF-8 JSON with no duplicate keys")
if not isinstance(value, dict):
    raise SystemExit("error: OpenCode config root must be a JSON object")
providers = value.get("provider", {})
if not isinstance(providers, dict):
    raise SystemExit("error: OpenCode config provider must be an object")
provider_state = "absent"
existing = providers.get("dtoken-local-route")
desired = {
    "npm": "@ai-sdk/openai-compatible",
    "name": "d-token local route",
    "options": {
        "baseURL": f"http://127.0.0.1:{port}/v1",
        "apiKey": "{env:DTOKEN_LOCAL_LISTENER_TOKEN}",
    },
    "models": {
        requested_model: {"name": f"d-token routed {requested_model}"},
    },
}
if existing is not None:
    owned = (
        isinstance(existing, dict)
        and set(existing) == {"npm", "name", "options", "models"}
        and existing.get("npm") == "@ai-sdk/openai-compatible"
        and existing.get("name") == "d-token local route"
        and isinstance(existing.get("options"), dict)
        and set(existing["options"]) == {"baseURL", "apiKey"}
        and isinstance(existing["options"].get("baseURL"), str)
        and existing["options"].get("apiKey") == "{env:DTOKEN_LOCAL_LISTENER_TOKEN}"
        and isinstance(existing.get("models"), dict)
        and len(existing["models"]) == 1
    )
    if owned:
        model, definition = next(iter(existing["models"].items()))
        owned = (
            isinstance(definition, dict)
            and definition == {"name": f"d-token routed {model}"}
        )
    if not owned:
        raise SystemExit("error: existing dtoken-local-route provider is not provably d-token-owned")
    provider_state = "owned_matching" if existing == desired else "owned_existing"
for key in ("enabled_providers", "disabled_providers"):
    entry = value.get(key)
    if entry is not None and (not isinstance(entry, list) or not all(isinstance(item, str) for item in entry)):
        raise SystemExit(f"error: OpenCode config {key} must be an array of strings")
if "dtoken-local-route" in value.get("disabled_providers", []):
    raise SystemExit("error: dtoken-local-route is explicitly disabled; refusing an ambiguous temporary patch")
mode = stat.S_IMODE(path.stat().st_mode)
enabled_providers_append = (
    "enabled_providers" in value
    and "dtoken-local-route" not in value["enabled_providers"]
)
config_change_required = existing != desired or enabled_providers_append
print(json.dumps({
    "size": len(raw),
    "mode": format(mode, "04o"),
    "provider_state": provider_state,
    "enabled_providers_append": enabled_providers_append,
    "config_change_required": config_change_required,
}, separators=(",", ":")))
PY
)"
CONFIG_CHANGE_REQUIRED="$(python3 - "$CONFIG_PREFLIGHT" <<'PY'
import json
import sys
print("1" if json.loads(sys.argv[1])["config_change_required"] else "0")
PY
)"

# Binding a loopback socket does not contact an upstream and closes immediately.
python3 - "$PORT" <<'PY'
import socket
import sys

port = int(sys.argv[1])
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
    listener.bind(("127.0.0.1", port))
PY

CONFIG_PATH_REDACTED="$(redact_home_path "$CONFIG_PATH")"
if [[ "$DRY_RUN" -eq 1 ]]; then
  python3 - "$SELECTED_PROVIDER" "$MODEL" "$PORT" "$CREDENTIAL_MODE" "$CONFIG_PATH_REDACTED" "$CONFIG_PREFLIGHT" "$OPENCODE_PURE" "$COMPRESSION_WORKLOAD" <<'PY'
import json
import sys

provider, model, port, credential_mode, config_path, config_preflight, opencode_pure, compression_workload = sys.argv[1:]
preflight = json.loads(config_preflight)
print(json.dumps({
    "status": "preflight_ok",
    "mode": "dry_run",
    "selected_provider": provider,
    "selected_model": model,
    "listener": f"127.0.0.1:{port}",
    "listener_port_available": True,
    "upstream_url_validated_without_request": True,
    "credential_mode": credential_mode,
    "normal_user_config": config_path,
    "strict_json_valid": True,
    "config_mode": preflight["mode"],
    "provider_state": preflight["provider_state"],
    "config_change_required": preflight["config_change_required"],
    "config_mutated": False,
    "backup_created": False,
    "report_created": False,
    "opencode_executed": False,
    "external_request_made": False,
    "opencode_external_plugins_enabled": opencode_pure != "1",
    "ccr_private_store_enabled": compression_workload == "1",
    "ccr_key_cleanup_planned": compression_workload == "1",
    "real_run_checks_deferred": [
        "existing_model_catalog_match",
        "listener_health",
        "response_nonce",
        "route_request_provider_model_success",
        "owned_runtime_source",
        "four_distinct_jsonl_files",
        "ccr_metadata_only",
        "nonce_absent_from_dtoken_persistence",
        "exact_config_restore",
    ],
}, ensure_ascii=False, separators=(",", ":")))
PY
  exit 0
fi

for command in cargo curl opencode; do
  command -v "$command" >/dev/null 2>&1 || die "$command is required"
done
[[ -w "$CONFIG_PATH" && -w "$(dirname "$CONFIG_PATH")" ]] || die "the normal OpenCode config and its directory must be writable for the guarded transaction"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/dtoken-opencode-real.XXXXXX")"
OPENCODE_WORK_DIR="$ROOT"
if [[ "$COMPRESSION_WORKLOAD" -eq 1 ]]; then
  # Use public synthetic shell output without loading this repository's large
  # instruction tree. The real user config, HOME and session store stay intact.
  OPENCODE_WORK_DIR="$TEMP_ROOT/public-workload"
  mkdir -p "$OPENCODE_WORK_DIR"
fi
CONFIG_RUNTIME_PATH="$TEMP_ROOT/dtoken-config.json"
DATA_DIR="$TEMP_ROOT/runtime-data"
STDOUT_PATH="$TEMP_ROOT/opencode.stdout"
STDERR_PATH="$TEMP_ROOT/opencode.stderr"
PROXY_STDOUT="$TEMP_ROOT/dtoken.stdout"
PROXY_STDERR="$TEMP_ROOT/dtoken.stderr"
MODELS_PATH="$TEMP_ROOT/models.txt"
TRANSACTION_STATE_PATH="$TEMP_ROOT/config-transaction.json"
RESULT_STATE_PATH="$TEMP_ROOT/result.json"
REPORT_DIR="$ROOT/support/artifacts/opencode-real-routed"
REPORT_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_PATH="$REPORT_DIR/real-routed-${REPORT_STAMP}-$$.json"
REPORT_PATH_REDACTED="support/artifacts/opencode-real-routed/$(basename "$REPORT_PATH")"
BACKUP_PATH="${CONFIG_PATH}.dtoken-backup-${REPORT_STAMP}-$$"
BACKUP_PATH_REDACTED="$(redact_home_path "$BACKUP_PATH")"
PROXY_PID=""
ORIGINAL_CONFIG_HASH=""
TEMP_CONFIG_HASH=""
ORIGINAL_CONFIG_MODE=""
CONFIG_TRANSACTION_STARTED=0
BACKUP_CREATED=0
RESTORE_STATUS="not_required"
RUN_STATUS="started"
FAILURE_STAGE="preflight"
PERSISTED_COUNTS='{}'
CCR_ENTRIES=0
COMPRESSION_EVIDENCE='{}'
CCR_EVIDENCE='{}'
OPENCODE_FAILURE_CODE="not_run"
CCR_KEYCHAIN_ACCOUNT=""
CCR_KEYCHAIN_CLEANUP_STATUS="not_required"
if [[ "$COMPRESSION_WORKLOAD" -eq 1 ]]; then
  command -v security >/dev/null 2>&1 || die "macOS security CLI is required for scoped CCR key cleanup"
  CCR_KEYCHAIN_ACCOUNT="$(python3 - "$DATA_DIR" <<'PY'
import hashlib
import sys
identity = hashlib.sha256(sys.argv[1].encode("utf-8")).hexdigest()
print(f"runtime-{identity[:24]}")
PY
)"
  if security find-generic-password -s "d-token.ccr" -a "$CCR_KEYCHAIN_ACCOUNT" >/dev/null 2>&1; then
    die "the temporary CCR Keychain identity already exists; refusing to delete a pre-existing credential"
  fi
  CCR_KEYCHAIN_CLEANUP_STATUS="planned"
fi

write_redacted_report() {
  local exit_code="$1"
  local final_status="$RUN_STATUS"
  if [[ "$RESTORE_STATUS" == "conflict_preserved" || "$RESTORE_STATUS" == "concurrent_change_preserved" || "$RESTORE_STATUS" == "restore_failed" ]]; then
    final_status="failed_restore"
  elif [[ "$exit_code" -ne 0 && "$final_status" != "pass" ]]; then
    final_status="failed"
  fi
  mkdir -p "$REPORT_DIR"
  DTOKEN_OPENCODE_REAL_ROOT="$TEMP_ROOT" python3 - "$REPORT_PATH" "$final_status" "$exit_code" "$FAILURE_STAGE" \
    "$SELECTED_PROVIDER" "$MODEL" "$PORT" "$CREDENTIAL_MODE" \
    "$CONFIG_PATH_REDACTED" "$BACKUP_PATH_REDACTED" "$BACKUP_CREATED" \
    "$RESTORE_STATUS" "$REPORT_PATH_REDACTED" "$PERSISTED_COUNTS" "$CCR_ENTRIES" "$COMPRESSION_WORKLOAD" "$COMPRESSION_EVIDENCE" "$CCR_EVIDENCE" "$CONFIG_PREFLIGHT" "$OPENCODE_FAILURE_CODE" "$OPENCODE_PURE" "$CCR_KEYCHAIN_CLEANUP_STATUS" <<'PY'
import json
import os
import pathlib
import sys
import tempfile
from datetime import datetime, timezone

(
    report_path, status, exit_code, failure_stage, provider, model, port,
    credential_mode, config_path, backup_path, backup_created, restore_status,
    report_path_redacted, persisted_counts, ccr_entries, compression_workload,
    compression_evidence, ccr_evidence, config_preflight, opencode_failure_code,
    opencode_pure, ccr_keychain_cleanup_status,
) = sys.argv[1:]
candidate_id = os.environ.get("DTOKEN_RELEASE_CANDIDATE_ID") or None
root_path = pathlib.Path(os.environ["DTOKEN_OPENCODE_REAL_ROOT"])
health_path = root_path / "health.json"
health = json.loads(health_path.read_text(encoding="utf-8")) if health_path.exists() else {}
result_state_path = root_path / "result.json"
try:
    result_state = json.loads(result_state_path.read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError):
    result_state = {}
diagnostics = result_state.get("diagnostics") if isinstance(result_state, dict) else None
persisted_count_payload = json.loads(persisted_counts)
if status != "pass" and not diagnostics:
    # Read only the owned, metadata-only journals after the proxy has stopped.
    # Failure during bootstrap must still leave enough evidence to distinguish
    # an upstream rejection from a request that never reached d-token.
    journals = {}
    for name in ("events.jsonl", "route-decisions.jsonl", "request-logs.jsonl", "ccr-metadata.jsonl"):
        rows = []
        for path in (root_path / "runtime-data").rglob(name):
            try:
                for line in path.read_text(encoding="utf-8").splitlines():
                    try:
                        item = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    if isinstance(item, dict):
                        rows.append(item)
            except OSError:
                continue
        journals[name] = rows
        persisted_count_payload[name] = len(rows)
    requests = [item for item in journals["request-logs.jsonl"] if item.get("route_id") == "opencode-real-routed" and item.get("provider_id") == provider]
    diagnostics = {
        "source": "persisted_terminal_records",
        "matching_request_count": len(requests),
        "successful_request_count": sum(item.get("error") is None for item in requests),
        "failed_request_count": sum(item.get("error") is not None for item in requests),
        "raw_available": False,
    }
effective_ccr_entries = int(result_state.get("ccr_entries") or ccr_entries or 0)
ccr_evidence_payload = json.loads(ccr_evidence)
payload = {
    "schema_version": 2,
    "created_at": datetime.now(timezone.utc).isoformat(),
    "status": status,
    "exit_code": int(exit_code),
    "failure_stage": None if status == "pass" else failure_stage,
    "candidate_id": candidate_id,
    "runtime_identity": health.get("runtime_identity"),
    "service_url": f"http://127.0.0.1:{port}",
    "selected_provider": provider,
    "selected_model": model,
    "listener": f"127.0.0.1:{port}",
    "credential_mode": credential_mode,
    "normal_user_config": config_path,
    "normal_user_state_used": True,
    "workload_project": "temporary_public_fixture" if compression_workload == "1" else "current_repository",
    "opencode_external_plugins_enabled": opencode_pure != "1",
    "config_transaction": {
        "change_required": json.loads(config_preflight)["config_change_required"],
        "backup_created": backup_created == "1",
        "backup_path": backup_path if backup_created == "1" else None,
        "backup_mode": "0600" if backup_created == "1" else None,
        "provider_state_before": json.loads(config_preflight)["provider_state"],
        "temporary_changes": (
            ["create or replace provably d-token-owned provider.dtoken-local-route", "enabled_providers append when present"]
            if json.loads(config_preflight)["config_change_required"]
            else []
        ),
        "restore_status": restore_status,
        "original_bytes_restored": restore_status in {"restored_exact", "not_required_identical"},
        "conflicting_edit_overwritten": False,
    },
    "evidence": {
        "persisted_jsonl_counts": persisted_count_payload,
        "distinct_jsonl_files": 4 if status == "pass" else None,
        "route_request_provider_model_verified": status == "pass",
        "provider_forward_verified": status == "pass",
        "ccr_entries": effective_ccr_entries,
        "ccr_raw_available_before_delete": (
            ccr_evidence_payload.get("raw_available_before_delete")
            if status == "pass"
            else None
        ),
        "ccr_raw_returned": (
            ccr_evidence_payload.get("raw_returned")
            if status == "pass"
            else None
        ),
        "nonce_persisted_in_dtoken_runtime": False if status == "pass" else None,
        "opencode_failure_code": opencode_failure_code,
        "compression_workload": compression_workload == "1",
        "compression": json.loads(compression_evidence),
        "diagnostics": diagnostics,
    },
    "ledger": {
        "events": persisted_count_payload.get("events.jsonl"),
        "route_decisions": persisted_count_payload.get("route-decisions.jsonl"),
        "request_logs": persisted_count_payload.get("request-logs.jsonl"),
        "ccr_metadata": persisted_count_payload.get("ccr-metadata.jsonl"),
    },
    "metrics": {
        "compression": json.loads(compression_evidence),
        "route": {
            "route_id": "opencode-real-routed",
            "provider_id": provider,
            "model": model,
        },
    },
    "ccr": {
        "entries": effective_ccr_entries,
        **ccr_evidence_payload,
        "private_store_configured": compression_workload == "1",
        "keychain_cleanup_status": ccr_keychain_cleanup_status,
    },
    "privacy": {
        "auth_json_read": False,
        "literal_credentials_recorded": False,
        "upstream_url_recorded": False,
        "prompt_recorded": False,
        "response_recorded": False,
        "nonce_recorded": False,
    },
    "report_path": report_path_redacted,
}
destination = pathlib.Path(report_path)
destination.parent.mkdir(parents=True, exist_ok=True)
fd, temporary = tempfile.mkstemp(prefix=destination.name + ".", dir=destination.parent)
try:
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, separators=(",", ":"))
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, destination)
    directory_fd = os.open(destination.parent, os.O_RDONLY)
    try:
        os.fsync(directory_fd)
    finally:
        os.close(directory_fd)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
PY
}

restore_original_config() {
  python3 - "$CONFIG_PATH" "$BACKUP_PATH" "$ORIGINAL_CONFIG_HASH" "$TEMP_CONFIG_HASH" "$ORIGINAL_CONFIG_MODE" <<'PY'
import hashlib
import os
import pathlib
import sys
import tempfile

config_path, backup_path, original_hash, temporary_hash, original_mode = sys.argv[1:]
config = pathlib.Path(config_path)
backup = pathlib.Path(backup_path)

def digest(data):
    return hashlib.sha256(data).hexdigest()

try:
    backup_bytes = backup.read_bytes()
    current_bytes = config.read_bytes()
except OSError:
    raise SystemExit(3)
if digest(backup_bytes) != original_hash:
    raise SystemExit(3)
current_hash = digest(current_bytes)
if current_hash == original_hash:
    raise SystemExit(0)
if current_hash != temporary_hash:
    raise SystemExit(2)

fd, temporary = tempfile.mkstemp(prefix=config.name + ".dtoken-restore-", dir=config.parent)
try:
    os.fchmod(fd, int(original_mode, 8))
    with os.fdopen(fd, "wb") as handle:
        handle.write(backup_bytes)
        handle.flush()
        os.fsync(handle.fileno())
    # Recheck immediately before replace so a concurrent edit is preserved.
    try:
        if digest(config.read_bytes()) != temporary_hash:
            raise SystemExit(2)
    except OSError:
        raise SystemExit(3)
    os.replace(temporary, config)
    directory_fd = os.open(config.parent, os.O_RDONLY)
    try:
        os.fsync(directory_fd)
    finally:
        os.close(directory_fd)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)

try:
    restored = config.read_bytes()
except OSError:
    raise SystemExit(3)
raise SystemExit(0 if digest(restored) == original_hash else 3)
PY
}

cleanup() {
  local incoming_status="$?"
  local final_status="$incoming_status"
  local restore_code=0
  trap - EXIT INT TERM HUP
  set +e

  if [[ -n "$PROXY_PID" ]] && kill -0 "$PROXY_PID" 2>/dev/null; then
    kill "$PROXY_PID" 2>/dev/null
    wait "$PROXY_PID" 2>/dev/null
  fi

  if [[ -n "$CCR_KEYCHAIN_ACCOUNT" ]]; then
    if security delete-generic-password -s "d-token.ccr" -a "$CCR_KEYCHAIN_ACCOUNT" >/dev/null 2>&1; then
      if security find-generic-password -s "d-token.ccr" -a "$CCR_KEYCHAIN_ACCOUNT" >/dev/null 2>&1; then
        CCR_KEYCHAIN_CLEANUP_STATUS="cleanup_failed"
        final_status=76
      else
        CCR_KEYCHAIN_CLEANUP_STATUS="deleted"
      fi
    elif security find-generic-password -s "d-token.ccr" -a "$CCR_KEYCHAIN_ACCOUNT" >/dev/null 2>&1; then
      CCR_KEYCHAIN_CLEANUP_STATUS="cleanup_failed"
      final_status=76
    else
      CCR_KEYCHAIN_CLEANUP_STATUS="not_created"
    fi
  fi

  if [[ "$CONFIG_TRANSACTION_STARTED" -eq 1 ]]; then
    if [[ -z "$TEMP_CONFIG_HASH" && -s "$TRANSACTION_STATE_PATH" ]]; then
      TEMP_CONFIG_HASH="$(python3 - "$TRANSACTION_STATE_PATH" <<'PY'
import json
import pathlib
import sys
try:
    print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["temporary_hash"])
except Exception:
    raise SystemExit(1)
PY
)"
    fi
    if [[ -n "$TEMP_CONFIG_HASH" && "$BACKUP_CREATED" -eq 1 ]]; then
      restore_original_config
      restore_code=$?
      case "$restore_code" in
        0)
          if [[ "$(sha256_file "$CONFIG_PATH" 2>/dev/null)" == "$ORIGINAL_CONFIG_HASH" ]]; then
            RESTORE_STATUS="restored_exact"
          else
            RESTORE_STATUS="restore_failed"
            final_status=74
          fi
          ;;
        2)
          RESTORE_STATUS="conflict_preserved"
          final_status=75
          ;;
        *)
          RESTORE_STATUS="restore_failed"
          final_status=74
          ;;
      esac
    elif [[ "$BACKUP_CREATED" -eq 1 && "$(sha256_file "$CONFIG_PATH" 2>/dev/null)" == "$ORIGINAL_CONFIG_HASH" ]]; then
      RESTORE_STATUS="restored_exact"
    else
      RESTORE_STATUS="restore_failed"
      final_status=74
    fi
  elif [[ "$CONFIG_CHANGE_REQUIRED" -eq 0 && -n "$ORIGINAL_CONFIG_HASH" ]]; then
    if [[ "$(sha256_file "$CONFIG_PATH" 2>/dev/null)" == "$ORIGINAL_CONFIG_HASH" ]]; then
      RESTORE_STATUS="not_required_identical"
    else
      RESTORE_STATUS="concurrent_change_preserved"
      final_status=75
    fi
  fi

  write_redacted_report "$final_status"
  rm -rf "$TEMP_ROOT"
  exit "$final_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

FAILURE_STAGE="model_catalog"
if ! OPENCODE_CONFIG="$CONFIG_PATH" \
  run_opencode_clean models "$SELECTED_PROVIDER" >"$MODELS_PATH" 2>"$TEMP_ROOT/models.stderr"; then
  die "OpenCode could not list the selected provider; CLI output was deleted and not printed"
fi
python3 - "$MODELS_PATH" "$MODEL" <<'PY'
import json
import pathlib
import re
import sys

path, expected = sys.argv[1:]
ansi = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
models = {ansi.sub("", line).strip() for line in pathlib.Path(path).read_text(encoding="utf-8", errors="replace").splitlines()}
if expected not in models:
    raise SystemExit("error: selected model is not visible in the user's current OpenCode model catalog")
PY
rm -f "$MODELS_PATH" "$TEMP_ROOT/models.stderr"

ORIGINAL_CONFIG_HASH="$(sha256_file "$CONFIG_PATH")"
ORIGINAL_CONFIG_MODE="$(python3 - "$CONFIG_PATH" <<'PY'
import pathlib
import stat
import sys
print(format(stat.S_IMODE(pathlib.Path(sys.argv[1]).stat().st_mode), "04o"))
PY
)"
if [[ "$CONFIG_CHANGE_REQUIRED" -eq 1 ]]; then
FAILURE_STAGE="config_backup"
python3 - "$CONFIG_PATH" "$BACKUP_PATH" "$ORIGINAL_CONFIG_HASH" <<'PY'
import hashlib
import os
import pathlib
import sys

source_path, backup_path, expected_hash = sys.argv[1:]
source = pathlib.Path(source_path)
backup = pathlib.Path(backup_path)
data = source.read_bytes()
if hashlib.sha256(data).hexdigest() != expected_hash:
    raise SystemExit("error: OpenCode config changed before backup; retry after the other writer finishes")
flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
fd = os.open(backup, flags, 0o600)
try:
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, "wb") as handle:
        handle.write(data)
        handle.flush()
        os.fsync(handle.fileno())
except Exception:
    try:
        os.unlink(backup)
    except OSError:
        pass
    raise
directory_fd = os.open(backup.parent, os.O_RDONLY)
try:
    os.fsync(directory_fd)
finally:
    os.close(directory_fd)
if hashlib.sha256(backup.read_bytes()).hexdigest() != expected_hash:
    raise SystemExit("error: persistent OpenCode config backup verification failed")
PY
BACKUP_CREATED=1

FAILURE_STAGE="config_patch"
CONFIG_TRANSACTION_STARTED=1
python3 - "$CONFIG_PATH" "$TRANSACTION_STATE_PATH" "$ORIGINAL_CONFIG_HASH" "$ORIGINAL_CONFIG_MODE" "$PORT" "$SELECTED_PROVIDER" "$UPSTREAM_MODEL" <<'PY'
import hashlib
import json
import os
import pathlib
import sys
import tempfile

config_path, state_path, original_hash, original_mode, port, provider, model = sys.argv[1:]
config = pathlib.Path(config_path)
state = pathlib.Path(state_path)

def reject_duplicates(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON object key")
        result[key] = value
    return result

raw = config.read_bytes()
if hashlib.sha256(raw).hexdigest() != original_hash:
    raise SystemExit("error: OpenCode config changed after backup; temporary patch was not applied")
try:
    value = json.loads(raw.decode("utf-8"), object_pairs_hook=reject_duplicates)
except (UnicodeDecodeError, json.JSONDecodeError, ValueError):
    raise SystemExit("error: OpenCode config stopped being valid strict JSON before patch")
providers = value.setdefault("provider", {})
if not isinstance(providers, dict):
    raise SystemExit("error: OpenCode provider map changed before patch; temporary patch was not applied")
existing = providers.get("dtoken-local-route")
if existing is not None:
    owned = (
        isinstance(existing, dict)
        and set(existing) == {"npm", "name", "options", "models"}
        and existing.get("npm") == "@ai-sdk/openai-compatible"
        and existing.get("name") == "d-token local route"
        and isinstance(existing.get("options"), dict)
        and set(existing["options"]) == {"baseURL", "apiKey"}
        and isinstance(existing["options"].get("baseURL"), str)
        and existing["options"].get("apiKey") == "{env:DTOKEN_LOCAL_LISTENER_TOKEN}"
        and isinstance(existing.get("models"), dict)
        and len(existing["models"]) == 1
    )
    if owned:
        owned_model, definition = next(iter(existing["models"].items()))
        owned = (
            isinstance(definition, dict)
            and definition == {"name": f"d-token routed {owned_model}"}
        )
    if not owned:
        raise SystemExit("error: OpenCode provider map changed to an unowned dtoken-local-route before patch")
providers["dtoken-local-route"] = {
    "npm": "@ai-sdk/openai-compatible",
    "name": "d-token local route",
    "options": {
        "baseURL": f"http://127.0.0.1:{port}/v1",
        "apiKey": "{env:DTOKEN_LOCAL_LISTENER_TOKEN}",
    },
    "models": {
        model: {"name": f"d-token routed {model}"},
    },
}
enabled_appended = False
if "enabled_providers" in value and "dtoken-local-route" not in value["enabled_providers"]:
    value["enabled_providers"].append("dtoken-local-route")
    enabled_appended = True

temporary_bytes = (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
temporary_hash = hashlib.sha256(temporary_bytes).hexdigest()
state_payload = {
    "original_hash": original_hash,
    "temporary_hash": temporary_hash,
    "provider_preexisting": existing is not None,
    "enabled_providers_appended": enabled_appended,
}
state.write_text(json.dumps(state_payload, separators=(",", ":")), encoding="utf-8")
os.chmod(state, 0o600)
with state.open("rb") as handle:
    os.fsync(handle.fileno())

fd, temporary = tempfile.mkstemp(prefix=config.name + ".dtoken-patch-", dir=config.parent)
try:
    os.fchmod(fd, int(original_mode, 8))
    with os.fdopen(fd, "wb") as handle:
        handle.write(temporary_bytes)
        handle.flush()
        os.fsync(handle.fileno())
    if hashlib.sha256(config.read_bytes()).hexdigest() != original_hash:
        raise SystemExit("error: OpenCode config changed during patch; concurrent edit was preserved")
    os.replace(temporary, config)
    directory_fd = os.open(config.parent, os.O_RDONLY)
    try:
        os.fsync(directory_fd)
    finally:
        os.close(directory_fd)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
if hashlib.sha256(config.read_bytes()).hexdigest() != temporary_hash:
    raise SystemExit("error: temporary OpenCode config patch verification failed")
PY
TEMP_CONFIG_HASH="$(python3 - "$TRANSACTION_STATE_PATH" <<'PY'
import json
import pathlib
import sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["temporary_hash"])
PY
)"
else
  FAILURE_STAGE="config_verified"
  TEMP_CONFIG_HASH="$ORIGINAL_CONFIG_HASH"
  RESTORE_STATUS="not_required_identical"
fi

FAILURE_STAGE="runtime_config"
python3 - "$CONFIG_RUNTIME_PATH" "$PORT" "$SELECTED_PROVIDER" "$UPSTREAM_MODEL" "$UPSTREAM_BASE_URL" "$UPSTREAM_API_KEY_REF" <<'PY'
import json
import os
import pathlib
import sys

config_path, port, provider, model, base_url, credential_ref = sys.argv[1:]
route_id = "opencode-real-routed"
config = {
    "providers": [{
        "id": provider,
        "name": f"Existing OpenCode provider ({provider})",
        "kind": "custom",
        "protocol": "open_ai",
        "base_url": base_url,
        "api_key_ref": credential_ref or None,
        "models": [model],
        "timeout_ms": 120000,
        "healthcheck_path": None,
    }],
    "routes": [{
        "id": route_id,
        "name": "OpenCode Real Routed",
        "input_protocol": "open_ai",
        "instance_id": "opencode-real-instance",
        "target_provider_id": provider,
        "fallback_provider_id": None,
        "chain_mode": "direct",
        "compression_policy": "balanced",
        "bind_host": "127.0.0.1",
        "local_port": int(port),
    }],
    "instances": [{
        "id": "opencode-real-instance",
        "name": "OpenCode Real Instance",
        "agent": "OpenCode",
        "route_id": route_id,
        "local_port": int(port),
        "process_status": "stopped",
        "log_path": "./logs/opencode-real.jsonl",
    }],
}
path = pathlib.Path(config_path)
path.write_text(json.dumps(config, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
os.chmod(path, 0o600)
PY

FAILURE_STAGE="runtime_build"
(
  cd "$CODE_ROOT"
  cargo build -p dtoken-proxy >/dev/null
  cargo build -p dtoken-core-api --example ccr_candidate_probe >/dev/null
)

FAILURE_STAGE="runtime_start"
if [[ "$COMPRESSION_WORKLOAD" -eq 1 ]]; then
  DTOKEN_CCR_ENABLED=1 "$CODE_ROOT/target/debug/dtoken-proxy" \
    --host 127.0.0.1 \
    --port "$PORT" \
    --config "$CONFIG_RUNTIME_PATH" \
    --data-dir "$DATA_DIR" \
    >"$PROXY_STDOUT" 2>"$PROXY_STDERR" &
else
  "$CODE_ROOT/target/debug/dtoken-proxy" \
    --host 127.0.0.1 \
    --port "$PORT" \
    --config "$CONFIG_RUNTIME_PATH" \
    --data-dir "$DATA_DIR" \
    >"$PROXY_STDOUT" 2>"$PROXY_STDERR" &
fi
PROXY_PID=$!

for _ in $(seq 1 60); do
  if curl --fail --silent "http://127.0.0.1:${PORT}/health" >"$TEMP_ROOT/health.json"; then
    break
  fi
  sleep 0.5
done
[[ -s "$TEMP_ROOT/health.json" ]] || die "d-token proxy did not become healthy on the temporary loopback listener"

FAILURE_STAGE="opencode_run"
RUN_NONCE="$(python3 -c 'import secrets; print(secrets.token_hex(12))')"
LOCAL_LISTENER_TOKEN="$(python3 -c 'import secrets; print(secrets.token_urlsafe(24))')"
LOCAL_MODEL="dtoken-local-route/$UPSTREAM_MODEL"
OPENCODE_PROMPT="Return only this nonce: ${RUN_NONCE}"
OPENCODE_STATUS=0
OPENCODE_SESSION_ID=""
if [[ "$COMPRESSION_WORKLOAD" -eq 1 ]]; then
  FAILURE_STAGE="opencode_compression_bootstrap"
  BOOTSTRAP_STDOUT="$TEMP_ROOT/opencode-bootstrap.jsonl"
  BOOTSTRAP_STDERR="$TEMP_ROOT/opencode-bootstrap.stderr"
  REQUIRED_COMPLETED_TOOL_RESULTS=7
  BOOTSTRAP_PROMPT='Use the shell tool exactly seven times, in order, and do not skip or combine calls. Call 1: for alpha in alpha beta gamma delta epsilon zeta eta theta iota kappa; do for beta in alpha beta gamma delta epsilon zeta eta theta iota kappa; do echo public placeholder context alpha beta gamma delta; done; done. Calls 2 through 7: run echo public filler tool result once per call. After all seven shell calls finish, reply only READY.'
  set +e
  (
    cd "$OPENCODE_WORK_DIR"
    OPENCODE_CONFIG="$CONFIG_PATH" \
      DTOKEN_LOCAL_LISTENER_TOKEN="$LOCAL_LISTENER_TOKEN" \
      run_opencode_clean run --format json --auto --model "$LOCAL_MODEL" \
        --title "d-token compression verification" "$BOOTSTRAP_PROMPT"
  ) >"$BOOTSTRAP_STDOUT" 2>"$BOOTSTRAP_STDERR"
  OPENCODE_STATUS=$?
  set -e
  OPENCODE_FAILURE_CODE="$(classify_opencode_failure "$BOOTSTRAP_STDOUT" "$BOOTSTRAP_STDERR" "$OPENCODE_STATUS")"
  [[ "$OPENCODE_STATUS" -eq 0 ]] || die "OpenCode compression preparation failed: ${OPENCODE_FAILURE_CODE}"
  python3 - "$BOOTSTRAP_STDOUT" "$REQUIRED_COMPLETED_TOOL_RESULTS" <<'PY'
import json
import pathlib
import sys

path, required = sys.argv[1:]
required = int(required)
completed = 0
large_output = False
for line in pathlib.Path(path).read_text(encoding="utf-8", errors="replace").splitlines():
    try:
        event = json.loads(line)
    except json.JSONDecodeError:
        continue
    part = event.get("part") if isinstance(event, dict) else None
    if not isinstance(part, dict) or part.get("type") != "tool":
        continue
    state = part.get("state")
    if not isinstance(state, dict) or state.get("status") != "completed":
        continue
    completed += 1
    output = state.get("output")
    if isinstance(output, str) and len(output.encode("utf-8")) >= 4096:
        large_output = True
if completed < required:
    raise SystemExit(
        f"error: OpenCode completed only {completed} safe tool results; {required} are required to cross the protected-tail gate"
    )
if not large_output:
    raise SystemExit("error: OpenCode did not retain the large safe tool result required for compression")
PY
  OPENCODE_SESSION_ID="$(python3 - "$BOOTSTRAP_STDOUT" <<'PY'
import json
import pathlib
import sys

def find_session(value):
    if isinstance(value, dict):
        for key in ("sessionID", "sessionId", "session_id"):
            candidate = value.get(key)
            if isinstance(candidate, str) and candidate.strip():
                return candidate.strip()
        for nested in value.values():
            found = find_session(nested)
            if found:
                return found
    elif isinstance(value, list):
        for nested in value:
            found = find_session(nested)
            if found:
                return found
    return None

for line in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace").splitlines():
    try:
        found = find_session(json.loads(line))
    except json.JSONDecodeError:
        continue
    if found:
        print(found)
        raise SystemExit(0)
raise SystemExit(1)
PY
)" || die "OpenCode did not expose a session id for the compression workload"
fi

FAILURE_STAGE="opencode_run"
set +e
if [[ -n "$OPENCODE_SESSION_ID" ]]; then
  (
    cd "$OPENCODE_WORK_DIR"
    OPENCODE_CONFIG="$CONFIG_PATH" \
      DTOKEN_LOCAL_LISTENER_TOKEN="$LOCAL_LISTENER_TOKEN" \
      run_opencode_clean run --format json --session "$OPENCODE_SESSION_ID" \
        --model "$LOCAL_MODEL" --title "d-token routed verification" "$OPENCODE_PROMPT"
  ) >"$STDOUT_PATH" 2>"$STDERR_PATH"
else
  (
    cd "$OPENCODE_WORK_DIR"
    OPENCODE_CONFIG="$CONFIG_PATH" \
      DTOKEN_LOCAL_LISTENER_TOKEN="$LOCAL_LISTENER_TOKEN" \
      run_opencode_clean run --format json \
        --model "$LOCAL_MODEL" --title "d-token routed verification" "$OPENCODE_PROMPT"
  ) >"$STDOUT_PATH" 2>"$STDERR_PATH"
fi
OPENCODE_STATUS=$?
set -e

RESPONSE_NONCE_OK=0
if [[ "$OPENCODE_STATUS" -eq 0 ]] && python3 - "$STDOUT_PATH" "$RUN_NONCE" <<'PY'
import pathlib
import sys

text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
raise SystemExit(0 if sys.argv[2] in text else 1)
PY
then
  RESPONSE_NONCE_OK=1
fi
OPENCODE_FAILURE_CODE="$(classify_opencode_failure "$STDOUT_PATH" "$STDERR_PATH" "$OPENCODE_STATUS")"
rm -f "$STDOUT_PATH" "$STDERR_PATH"

FAILURE_STAGE="runtime_evidence"
curl --fail --silent "http://127.0.0.1:${PORT}/dashboard/payload" >"$TEMP_ROOT/dashboard.json"
curl --fail --silent "http://127.0.0.1:${PORT}/ccr/summary" >"$TEMP_ROOT/ccr.json"

python3 - "$TEMP_ROOT" "$PORT" "$SELECTED_PROVIDER" "$MODEL" "$UPSTREAM_MODEL" "$OPENCODE_STATUS" "$RESPONSE_NONCE_OK" "$RUN_NONCE" "$RESULT_STATE_PATH" "$COMPRESSION_WORKLOAD" "$OPENCODE_FAILURE_CODE" <<'PY'
import json
import os
import pathlib
import sys

root, port, provider, selected_model, upstream_model, opencode_status, nonce_ok, nonce, result_path, compression_workload, opencode_failure_code = sys.argv[1:]
root_path = pathlib.Path(root)
data_path = root_path / "runtime-data"
dashboard = json.loads((root_path / "dashboard.json").read_text(encoding="utf-8"))
ccr = json.loads((root_path / "ccr.json").read_text(encoding="utf-8"))

routes = dashboard.get("route_decisions") or []
requests = dashboard.get("request_logs") or []
route = next((item for item in routes if item.get("route_id") == "opencode-real-routed" and item.get("provider_id") == provider), None)
matching_requests = [
    item for item in requests
    if item.get("route_id") == "opencode-real-routed"
    and item.get("provider_id") == provider
    and item.get("model") == upstream_model
]
successful_requests = [
    item for item in matching_requests
    if item.get("agent") == "OpenCode" and item.get("error") is None
]
inventory_snapshots = []
for item in dashboard.get("events") or []:
    metadata = item.get("metadata")
    if not isinstance(metadata, dict):
        continue
    inventory = metadata.get("compression_inventory")
    if isinstance(inventory, dict) and inventory.get("raw_available") is False:
        inventory_snapshots.append(inventory)

request_summaries = []
for item in matching_requests[-24:]:
    accounting = item.get("context_accounting")
    if not isinstance(accounting, dict):
        accounting = {}
    request_summaries.append({
        "error": item.get("error"),
        "input_tokens_before": item.get("input_tokens_before"),
        "input_tokens_after": item.get("input_tokens_after"),
        "transforms_applied": item.get("transforms_applied") or [],
        "disposition": accounting.get("disposition"),
        "sent_payload": accounting.get("sent_payload"),
        "upstream_state": accounting.get("upstream_state"),
        "attribution_basis": accounting.get("attribution_basis"),
        "fallback_reason_code": accounting.get("fallback_reason_code"),
    })
diagnostics = {
    "matching_request_count": len(matching_requests),
    "successful_request_count": len(successful_requests),
    "request_summaries": request_summaries,
    "compression_inventories": inventory_snapshots[-8:],
    "raw_available": False,
}
destination = pathlib.Path(result_path)
destination.write_text(json.dumps({
    "persisted": {},
    "ccr_entries": int(ccr.get("entries") or 0),
    "compression": {},
    "diagnostics": diagnostics,
}, separators=(",", ":")), encoding="utf-8")
os.chmod(destination, 0o600)

if compression_workload == "1":
    request = next(
        (
            item for item in reversed(successful_requests)
            if item.get("transforms_applied")
            and (item.get("context_accounting") or {}).get("sent_payload") == "optimized"
        ),
        None,
    )
else:
    request = next(reversed(successful_requests), None)
if route is None:
    raise SystemExit(
        "dashboard did not record the selected d-token route/provider "
        f"(opencode_exit={opencode_status}, nonce_observed={nonce_ok}, "
        f"failure_code={opencode_failure_code})"
    )
if request is None:
    error_category = matching_requests[-1].get("error") if matching_requests else None
    if not isinstance(error_category, str) or not error_category:
        error_category = (
            "optimized_request_missing"
            if compression_workload == "1"
            else "successful_request_missing"
        )
    raise SystemExit(
        "dashboard did not record the required successful OpenCode request "
        f"(reason_code={error_category})"
    )
if dashboard.get("source") != "dtoken-owned-runtime":
    raise SystemExit("dashboard evidence is not from the d-token-owned runtime")
if int(opencode_status) != 0 or int(nonce_ok) != 1:
    raise SystemExit("OpenCode did not return the response nonce through d-token")
if ccr.get("source") != "dtoken-owned-runtime":
    raise SystemExit("CCR evidence is not from the d-token-owned runtime")
if ccr.get("raw_returned") is not False:
    raise SystemExit("CCR HTTP summary does not explicitly prove that raw content was withheld")
if compression_workload == "1":
    if ccr.get("private_store_configured") is not True:
        raise SystemExit("CCR private store is not configured for the compression workload")
    if int(ccr.get("private_entries") or 0) < 1:
        raise SystemExit("CCR private store does not contain an encrypted recovery entry")
    if ccr.get("raw_available") is not True:
        raise SystemExit("CCR private store does not report the encrypted original as available")
    if ccr.get("raw_access") != "in-process-authorized-only":
        raise SystemExit("CCR private store does not enforce the authorized in-process boundary")
else:
    if int(ccr.get("entries") or 0) < 1:
        raise SystemExit("CCR metadata-only evidence is missing")
    if ccr.get("private_store_configured") is not False:
        raise SystemExit("CCR private store was unexpectedly configured for the ordinary routed test")
    if int(ccr.get("private_entries") or 0) != 0 or ccr.get("raw_available") is not False:
        raise SystemExit("CCR ordinary routed evidence unexpectedly retained a private original")

compression = {}
if compression_workload == "1":
    before = request.get("input_tokens_before")
    after = request.get("input_tokens_after")
    transforms = request.get("transforms_applied") or []
    accounting = request.get("context_accounting") or {}
    estimated = accounting.get("estimated") or {}
    signed = estimated.get("signed_savings") or {}
    signed_input = signed.get("input_tokens")
    if not isinstance(before, int) or not isinstance(after, int) or before <= after:
        raise SystemExit("compression workload did not prove a strict input token reduction")
    if not isinstance(transforms, list) or not transforms:
        raise SystemExit("compression workload did not record an applied transform")
    if accounting.get("disposition") not in {"active", "semantic_active"}:
        raise SystemExit("compression workload accounting is not active")
    upstream_state = accounting.get("upstream_state")
    if accounting.get("sent_payload") != "optimized" or upstream_state not in {"completed", "stream_started"}:
        raise SystemExit("compression workload accounting does not prove optimized upstream completion")
    attribution_basis = accounting.get("attribution_basis")
    if not isinstance(signed_input, int):
        raise SystemExit("compression workload lacks signed estimated comparison")
    if upstream_state == "completed" and attribution_basis != "estimated_active":
        raise SystemExit("completed compression workload lacks estimated active attribution")
    if upstream_state == "stream_started" and attribution_basis != "none":
        raise SystemExit("streaming compression workload made an unsupported savings attribution")
    if not any(item.get("route_id") == "opencode-real-routed" and item.get("compression_policy") == "balanced" for item in routes):
        raise SystemExit("compression workload did not prove the balanced route decision")
    compression = {
        "policy": "balanced",
        "input_tokens_before": before,
        "input_tokens_after": after,
        "physical_delta": before - after,
        "reported_tokens_saved": request.get("tokens_saved"),
        "transforms_applied": transforms,
        "accounting_disposition": accounting.get("disposition"),
        "upstream_state": accounting.get("upstream_state"),
        "attribution_basis": accounting.get("attribution_basis"),
        "signed_input_savings": signed_input,
        "claimable": attribution_basis == "estimated_active" and signed_input is not None,
        "actual_provider_usage": accounting.get("provider_actual_sent", {}).get("measurement") is not None,
        "matching_request_count": len(matching_requests),
        "background_failed_request_count": sum(
            1 for item in matching_requests if item.get("error") is not None
        ),
    }

required_files = [
    "events.jsonl",
    "route-decisions.jsonl",
    "request-logs.jsonl",
    "ccr-metadata.jsonl",
]
paths = [data_path / name for name in required_files]
if len({str(path.resolve()) for path in paths}) != 4:
    raise SystemExit("runtime evidence paths are not four distinct JSONL files")
identities = []
counts = {}
parsed = {}
for path in paths:
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"missing or empty runtime evidence: {path.name}")
    identities.append((path.stat().st_dev, path.stat().st_ino))
    entries = []
    for line in path.read_text(encoding="utf-8", errors="strict").splitlines():
        if line.strip():
            entries.append(json.loads(line))
    if not entries:
        raise SystemExit(f"empty runtime evidence: {path.name}")
    counts[path.name] = len(entries)
    parsed[path.name] = entries
if len(set(identities)) != 4:
    raise SystemExit("runtime evidence files alias the same filesystem object")

if not any(item.get("route_id") == "opencode-real-routed" and item.get("provider_id") == provider for item in parsed["route-decisions.jsonl"]):
    raise SystemExit("route JSONL does not prove the selected route/provider")
selected_request_id = request.get("request_id")
if not isinstance(selected_request_id, str) or not selected_request_id.strip():
    raise SystemExit("selected request does not expose a stable request identity")
selected_recovery_ref = f"ccr:{selected_request_id}"
selected_request_persisted = next(
    (
        item for item in parsed["request-logs.jsonl"]
        if item.get("route_id") == "opencode-real-routed"
        and item.get("provider_id") == provider
        and item.get("model") == upstream_model
        and item.get("agent") == "OpenCode"
        and item.get("error") is None
        and (
            selected_request_id is None
            or item.get("request_id") == selected_request_id
        )
    ),
    None,
)
if selected_request_persisted is None:
    raise SystemExit("request JSONL does not prove request/provider/model success")
if compression_workload == "1" and (
    not selected_request_persisted.get("transforms_applied")
    or (selected_request_persisted.get("context_accounting") or {}).get("sent_payload")
    != "optimized"
):
    raise SystemExit("request JSONL does not prove the selected optimized payload")
selected_ccr_record = next(
    (
        item for item in reversed(parsed["ccr-metadata.jsonl"])
        if item.get("recovery_ref") == selected_recovery_ref
        and item.get("request_id") == selected_request_id
        and item.get("route_id") == "opencode-real-routed"
        and item.get("provider_id") == provider
    ),
    None,
)
if selected_ccr_record is None:
    raise SystemExit("CCR JSONL does not bind the selected request to its recovery reference")
if compression_workload == "1":
    if selected_ccr_record.get("raw_available") is not True:
        raise SystemExit("selected CCR lifecycle record does not retain its encrypted original")
    if selected_ccr_record.get("raw_storage") != "encrypted-private-store":
        raise SystemExit("selected CCR lifecycle record does not identify encrypted private storage")
    if selected_ccr_record.get("state") not in {
        "finalized",
        "send_failed_retained",
        "finalized_with_non_ok_terminal",
    }:
        raise SystemExit("selected CCR lifecycle record is not terminal and recoverable")
else:
    if selected_ccr_record.get("raw_available") is not False:
        raise SystemExit("ordinary routed CCR metadata unexpectedly exposes a private original")
provider_events = [
    item for item in parsed["events.jsonl"]
    if item.get("stage") == "provider_forward"
    and item.get("status") == "ok"
    and (item.get("metadata") or {}).get("provider_id") == provider
]
if not any(isinstance((item.get("metadata") or {}).get("upstream_status"), int) and 200 <= item["metadata"]["upstream_status"] < 300 for item in provider_events):
    raise SystemExit("event JSONL does not prove successful provider forwarding")

# Scan every d-token runtime persistence file, not only the four required files.
for path in data_path.rglob("*"):
    if path.is_file() and nonce.encode("utf-8") in path.read_bytes():
        raise SystemExit(f"nonce leaked into d-token runtime persistence: {path.name}")

result = {
    "persisted": counts,
    "ccr_entries": int(ccr["entries"]),
    "ccr": {
        "selected_recovery_ref": selected_recovery_ref,
        "selected_request_bound": True,
        "private_store_configured": ccr.get("private_store_configured") is True,
        "private_entries_before_delete": int(ccr.get("private_entries") or 0),
        "raw_available_before_delete": ccr.get("raw_available") is True,
        "raw_access": ccr.get("raw_access"),
        "raw_returned": ccr.get("raw_returned"),
        "retrieve_state": "pending" if compression_workload == "1" else "not_required",
        "delete_state": "pending" if compression_workload == "1" else "not_required",
        "repeat_delete_state": "pending" if compression_workload == "1" else "not_required",
        "post_delete_retrieve": "pending" if compression_workload == "1" else "not_required",
        "raw_available_after_delete": None,
    },
    "compression": compression,
    "diagnostics": diagnostics,
}
destination = pathlib.Path(result_path)
destination.write_text(json.dumps(result, separators=(",", ":")), encoding="utf-8")
os.chmod(destination, 0o600)
PY

PERSISTED_COUNTS="$(python3 - "$RESULT_STATE_PATH" <<'PY'
import json
import pathlib
import sys
print(json.dumps(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["persisted"], separators=(",", ":")))
PY
)"
CCR_ENTRIES="$(python3 - "$RESULT_STATE_PATH" <<'PY'
import json
import pathlib
import sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["ccr_entries"])
PY
)"
COMPRESSION_EVIDENCE="$(python3 - "$RESULT_STATE_PATH" <<'PY'
import json
import pathlib
import sys
print(json.dumps(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")).get("compression", {}), separators=(",", ":")))
PY
)"
CCR_EVIDENCE="$(python3 - "$RESULT_STATE_PATH" <<'PY'
import json
import pathlib
import sys
evidence = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")).get("ccr", {})
evidence.pop("selected_recovery_ref", None)
print(json.dumps(evidence, separators=(",", ":")))
PY
)"

if [[ "$OPENCODE_STATUS" -ne 0 ]]; then
  die "OpenCode exited with status ${OPENCODE_STATUS}; output was deleted and not printed"
fi

if [[ "$COMPRESSION_WORKLOAD" -eq 1 ]]; then
  FAILURE_STAGE="ccr_lifecycle_probe"
  SELECTED_RECOVERY_REF="$(python3 - "$RESULT_STATE_PATH" <<'PY'
import json
import pathlib
import sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["ccr"]["selected_recovery_ref"]
if not isinstance(value, str) or not value.startswith("ccr:"):
    raise SystemExit(1)
print(value)
PY
)"
  if [[ -n "$PROXY_PID" ]] && kill -0 "$PROXY_PID" 2>/dev/null; then
    kill "$PROXY_PID"
    wait "$PROXY_PID" 2>/dev/null || true
    PROXY_PID=""
  else
    die "d-token proxy stopped before the CCR lifecycle probe"
  fi
  CCR_PROBE_OUTPUT="$(
    DTOKEN_CCR_ENABLED=1 \
      "$CODE_ROOT/target/debug/examples/ccr_candidate_probe" \
      "$CONFIG_RUNTIME_PATH" "$DATA_DIR" "$SELECTED_RECOVERY_REF"
  )"
  CCR_EVIDENCE="$(python3 - "$RESULT_STATE_PATH" "$CCR_PROBE_OUTPUT" <<'PY'
import json
import pathlib
import sys

result_path, probe_text = sys.argv[1:]
result = json.loads(pathlib.Path(result_path).read_text(encoding="utf-8"))
probe = json.loads(probe_text)
expected = {
    "source": "dtoken-owned-runtime",
    "initialized": True,
    "raw_available_before_delete": True,
    "retrieve_state": "retrieved",
    "delete_state": "deleted",
    "repeat_delete_state": "already_deleted",
    "post_delete_retrieve": "recovery_unavailable",
    "raw_available_after_delete": False,
    "raw_returned": False,
    "capability_returned": False,
    "recovery_ref_returned": False,
}
for key, value in expected.items():
    if probe.get(key) != value:
        raise SystemExit(f"CCR lifecycle probe failed its redacted {key} contract")
if not isinstance(probe.get("retrieved_body_bytes"), int) or probe["retrieved_body_bytes"] <= 0:
    raise SystemExit("CCR lifecycle probe did not retrieve the exact stored request object")
evidence = result["ccr"]
evidence.pop("selected_recovery_ref", None)
evidence.update({
    "retrieve_state": probe["retrieve_state"],
    "retrieved_body_bytes": probe["retrieved_body_bytes"],
    "delete_state": probe["delete_state"],
    "repeat_delete_state": probe["repeat_delete_state"],
    "post_delete_retrieve": probe["post_delete_retrieve"],
    "raw_available_after_delete": probe["raw_available_after_delete"],
    "raw_returned": probe["raw_returned"],
    "capability_returned": probe["capability_returned"],
    "recovery_ref_returned": probe["recovery_ref_returned"],
})
print(json.dumps(evidence, separators=(",", ":")))
PY
)"
fi

FAILURE_STAGE="complete"
RUN_STATUS="pass"
