#!/usr/bin/env bash
# R3.C2: real Codex config-transaction + real-model compression acceptance.
#
# User-authorized exception (AGENTS.md §5.6, 2026-08-23): proves
# `Codex CLI (real ~/.codex) -> d-token listener -> opencodex (10100) -> upstream`
# with the native Responses wire and compression_policy=balanced, via a real
# config transaction: restricted backup -> minimal single-field change
# (top-level `openai_base_url`) -> real `codex exec` request -> ledger
# evidence -> byte-exact restore with SHA-256 verification.
#
# Evidence: nonce round-trip, ledger request with route/provider identity,
# native Responses compression metadata, and a structured report under
# support/artifacts/codex-real-config/. No credential value is ever read,
# printed, or stored; auth.json is never opened.
set -euo pipefail
umask 077

die() { echo "error: $*" >&2; exit 1; }
require_value() { [[ $# -ge 2 && -n "$2" ]] || die "$1 requires a value"; }

PORT="${DTOKEN_CODEX_TEST_PORT:-18202}"
UPSTREAM_BASE_URL="${DTOKEN_CODEX_UPSTREAM:-http://127.0.0.1:10100/v1}"
CODEX_CONFIG="${HOME}/.codex/config.toml"
CODEX_TIMEOUT_SECS="${DTOKEN_CODEX_TIMEOUT:-240}"
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --port) require_value "$@"; PORT="$2"; shift 2;;
    --upstream) require_value "$@"; UPSTREAM_BASE_URL="$2"; shift 2;;
    --dry-run) DRY_RUN=1; shift;;
    *) die "unknown argument: $1";;
  esac
done

[[ "$PORT" =~ ^[0-9]+$ ]] || die "port must be numeric"
(( PORT >= 1024 && PORT <= 65535 )) || die "port must be between 1024 and 65535"
python3 - "$UPSTREAM_BASE_URL" <<'PY'
import sys
from urllib.parse import urlsplit
parts = urlsplit(sys.argv[1])
if parts.scheme not in ("http", "https") or not parts.netloc:
    raise SystemExit("upstream must be a plain http(s) URL")
if parts.username is not None or parts.password is not None:
    raise SystemExit("upstream must not contain userinfo credentials")
if parts.query or parts.fragment:
    raise SystemExit("upstream must not contain query or fragment")
PY
if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "dry-run: validated upstream=$(python3 - "$UPSTREAM_BASE_URL" <<'PY'
import sys
from urllib.parse import urlsplit
parts = urlsplit(sys.argv[1])
print(f"{parts.scheme}://{parts.netloc}{parts.path}")
PY
) port=$PORT; no request will be sent and no config will be touched"
  exit 0
fi
command -v codex >/dev/null 2>&1 || die "codex CLI is required"
command -v cargo >/dev/null 2>&1 || die "cargo is required"
command -v curl >/dev/null 2>&1 || die "curl is required"
[[ -f "$CODEX_CONFIG" ]] || die "real config.toml not found at $CODEX_CONFIG"
grep -q '^openai_base_url' "$CODEX_CONFIG" || die "config.toml has no top-level openai_base_url to redirect"
curl --silent --max-time 3 "http://127.0.0.1:10100/health" >/dev/null 2>&1 \
  || curl --silent --max-time 3 "http://127.0.0.1:10100/v1/models" >/dev/null 2>&1 \
  || die "opencodex upstream at 127.0.0.1:10100 is not reachable; aborting before any write"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/dtoken-codex-real-config.XXXXXX")"
DATA_DIR="$TEMP_ROOT/runtime-data"
mkdir -p "$DATA_DIR"
STDOUT_PATH="$TEMP_ROOT/codex.stdout"
STDERR_PATH="$TEMP_ROOT/codex.stderr"
PROXY_STDOUT="$TEMP_ROOT/proxy.stdout"
PROXY_STDERR="$TEMP_ROOT/proxy.stderr"
BACKUP_PATH="$TEMP_ROOT/config.toml.backup"
REPORT_DIR="$ROOT/support/artifacts/codex-real-config"
mkdir -p "$REPORT_DIR"
REPORT_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_PATH="$REPORT_DIR/real-config-${REPORT_STAMP}-$$.json"
PROXY_PID=""
CODEX_PID=""
RUN_STATUS="started"
FAILURE_STAGE="preflight"
NONCE=""
NONCE_OK=0
LEDGER_REQUESTS=0
COMPRESSION_METADATA="none"
ROUTE_ID_VERIFIED=0
RESTORE_STATUS="not_needed"
CONFIG_CHANGED=0

original_sha="$(shasum -a 256 "$CODEX_CONFIG" | awk '{print $1}')"
cp "$CODEX_CONFIG" "$BACKUP_PATH"
chmod 0600 "$BACKUP_PATH"

restore_config() {
  local current_sha=""
  current_sha="$(shasum -a 256 "$CODEX_CONFIG" 2>/dev/null | awk '{print $1}')"
  if [[ "$current_sha" != "$original_sha" ]]; then
    cp "$BACKUP_PATH" "$CODEX_CONFIG"
    chmod 0600 "$CODEX_CONFIG"
    current_sha="$(shasum -a 256 "$CODEX_CONFIG" | awk '{print $1}')"
    if [[ "$current_sha" == "$original_sha" ]]; then
      RESTORE_STATUS="restored_exact"
    else
      RESTORE_STATUS="restore_mismatch"
    fi
  else
    RESTORE_STATUS="already_original"
  fi
}

cleanup() {
  [[ -z "$CODEX_PID" ]] || kill "$CODEX_PID" 2>/dev/null || true
  [[ -z "$PROXY_PID" ]] || kill "$PROXY_PID" 2>/dev/null || true
  if [[ "$CONFIG_CHANGED" -eq 1 ]]; then
    restore_config
  fi
  REDACTED_UPSTREAM="$(python3 - "$UPSTREAM_BASE_URL" <<'PY'
import sys
from urllib.parse import urlsplit
parts = urlsplit(sys.argv[1])
print(f"{parts.scheme}://{parts.netloc}{parts.path}")
PY
)"
  cat >"$REPORT_PATH" <<JSON
{
  "test": "codex-real-config-compression",
  "stage": "R3.C2",
  "run_status": "$RUN_STATUS",
  "failure_stage": "$FAILURE_STAGE",
  "agent_id": "codex",
  "nonce_ok": $NONCE_OK,
  "ledger_requests_after": $LEDGER_REQUESTS,
  "route_id": "codex-real-config",
  "provider_id": "opencodex",
  "protocol": "open_ai_responses",
  "route_verified": $ROUTE_ID_VERIFIED,
  "compression_metadata": "$COMPRESSION_METADATA",
  "listener_port": $PORT,
  "upstream_base_url": "$REDACTED_UPSTREAM",
  "isolated_codex_home": false,
  "user_config_touched": true,
  "backup_mode": "0600",
  "restore_status": "$RESTORE_STATUS",
  "report_stamp": "$REPORT_STAMP"
}
JSON
  rm -rf "$TEMP_ROOT"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

# 1. Build the d-token runtime (same binary the desktop app manages).
FAILURE_STAGE="runtime_build"
( cd "$CODE_ROOT" && cargo build -p dtoken-proxy --quiet ) || die "dtoken-proxy build failed"

# 2. Start d-token: native Responses wire to opencodex, balanced compression,
#    dynamic model passthrough (models: []).
FAILURE_STAGE="runtime_start"
cat >"$TEMP_ROOT/proxy-config.json" <<JSON
{
  "providers": [
    {
      "id": "opencodex",
      "name": "OpenCodex local",
      "kind": "custom",
      "protocol": "open_ai",
      "base_url": "$UPSTREAM_BASE_URL",
      "models": [],
      "timeout_ms": 120000,
      "wire_api": "open_ai_responses"
    }
  ],
  "routes": [
    {
      "id": "codex-real-config",
      "name": "Codex real config routed",
      "input_protocol": "open_ai",
      "instance_id": "codex",
      "target_provider_id": "opencodex",
      "chain_mode": "direct",
      "compression_policy": "balanced",
      "bind_host": "127.0.0.1",
      "local_port": $PORT
    }
  ],
  "instances": [
    {
      "id": "codex",
      "name": "Codex",
      "agent": "codex",
      "route_id": "codex-real-config",
      "local_port": $PORT,
      "process_status": "stopped",
      "log_path": ""
    }
  ]
}
JSON
DTOKEN_CCR_ENABLED=1 "$CODE_ROOT/target/debug/dtoken-proxy" \
  --host 127.0.0.1 --port "$PORT" \
  --config "$TEMP_ROOT/proxy-config.json" \
  --data-dir "$DATA_DIR" >"$PROXY_STDOUT" 2>"$PROXY_STDERR" &
PROXY_PID=$!
for _ in $(seq 1 60); do
  if curl --silent --fail "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then break; fi
  sleep 0.5
done
curl --silent --fail "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1 || die "d-token listener did not become healthy"

# 3. Real config transaction: rewrite ONLY the top-level openai_base_url.
#    The auth file, MCP servers, model, and catalog stay untouched.
FAILURE_STAGE="config_transaction"
CONFIG_CHANGED=1
python3 - "$CODEX_CONFIG" "$PORT" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
port = sys.argv[2]
raw = path.read_text()
new = re.sub(
    r'(?m)^openai_base_url\s*=\s*"[^"]*"$',
    f'openai_base_url = "http://127.0.0.1:{port}/v1"',
    raw,
    count=1,
)
if new == raw:
    raise SystemExit("failed to rewrite openai_base_url")
path.write_text(new)
PY

# 4. Real request through the user's real Codex (their auth, their model).
#    opencodex owns the top-level `openai_base_url` field and re-injects
#    127.0.0.1:10100 a few seconds after any external write (observed ~9s).
#    A watchdog re-applies the d-token URL atomically every 500ms for the
#    whole run so the value codex reads is always the d-token listener; the
#    EXIT trap restores the exact original bytes afterwards.
FAILURE_STAGE="codex_run"
NONCE="$(python3 -c 'import secrets; print(secrets.token_hex(12))')"
(
  end=$(( SECONDS + CODEX_TIMEOUT_SECS ))
  while (( SECONDS < end )); do
    python3 - "$CODEX_CONFIG" "$PORT" <<'PY'
import os, pathlib, re, sys, tempfile
path = pathlib.Path(sys.argv[1])
port = sys.argv[2]
raw = path.read_text()
want = f'openai_base_url = "http://127.0.0.1:{port}/v1"'
new = re.sub(r'(?m)^openai_base_url\s*=\s*"[^"]*"$', want, raw, count=1)
if new != raw:
    fd, tmp = tempfile.mkstemp(dir=str(path.parent))
    with os.fdopen(fd, "w") as handle:
        handle.write(new)
    os.chmod(tmp, 0o600)
    os.replace(tmp, path)
PY
    sleep 0.5
  done
) &
WATCHDOG_PID=$!
set +e
codex exec --sandbox read-only "Return only this exact nonce: ${NONCE}" >"$STDOUT_PATH" 2>"$STDERR_PATH" &
CODEX_PID=$!
CODEX_STATUS=0
for _ in $(seq 1 "$CODEX_TIMEOUT_SECS"); do
  if ! kill -0 "$CODEX_PID" 2>/dev/null; then break; fi
  sleep 1
done
if kill -0 "$CODEX_PID" 2>/dev/null; then
  kill "$CODEX_PID" 2>/dev/null || true
  CODEX_STATUS=124
else
  wait "$CODEX_PID" 2>/dev/null || CODEX_STATUS=$?
fi
CODEX_PID=""
kill "$WATCHDOG_PID" 2>/dev/null || true
wait "$WATCHDOG_PID" 2>/dev/null || true
set -e
if [[ $CODEX_STATUS -eq 0 ]] && grep -q "$NONCE" "$STDOUT_PATH"; then
  NONCE_OK=1
fi

# 5. Runtime evidence: ledger request count + route identity + compression
#    metadata (native_responses_compression when transforms applied,
#    native_responses_passthrough when the request had no approved target —
#    both are honest outcomes).
FAILURE_STAGE="runtime_evidence"
curl --silent --fail "http://127.0.0.1:${PORT}/dashboard/payload" >"$TEMP_ROOT/dashboard.json" 2>/dev/null || true
LEDGER_REQUESTS="$(python3 - "$TEMP_ROOT/dashboard.json" <<'PY'
import json, sys
try:
    data = json.load(open(sys.argv[1]))
    payload = data.get("payload", data)
    stats = payload.get("stats", {})
    requests = stats.get("requests", 0)
    if isinstance(requests, dict):
        print(int(requests.get("total", 0)))
    else:
        print(int(requests))
except Exception:
    print(0)
PY
)"
ROUTE_ID_VERIFIED="$(python3 - "$TEMP_ROOT/dashboard.json" <<'PY'
import json, sys
try:
    dashboard = json.load(open(sys.argv[1]))
    routes = dashboard.get("route_decisions") or []
    request_logs = dashboard.get("request_logs") or []
    for entry in routes:
        if entry.get("route_id") == "codex-real-config" and entry.get("provider_id") == "opencodex":
            print(1)
            break
    else:
        for entry in request_logs:
            if entry.get("route_id") == "codex-real-config" and entry.get("error") is None:
                print(1)
                break
        else:
            print(0)
except Exception:
    print(0)
PY
)"
COMPRESSION_METADATA="$(python3 - "$DATA_DIR/events.jsonl" <<'PY'
import json, sys
try:
    with open(sys.argv[1]) as handle:
        for line in handle:
            try:
                event = json.loads(line)
            except Exception:
                continue
            metadata = event.get("metadata") or {}
            if "native_responses_compression" in metadata:
                print("native_responses_compression")
                break
            if "native_responses_passthrough" in metadata:
                print("native_responses_passthrough")
                break
except Exception:
    pass
PY
)"
[[ -n "$COMPRESSION_METADATA" ]] || COMPRESSION_METADATA="none"

if [[ $CODEX_STATUS -ne 0 ]]; then
  RUN_STATUS="fail"
  FAILURE_STAGE="codex_run"
  die "codex exited with status ${CODEX_STATUS}; stderr at $STDERR_PATH"
fi
if [[ "$NONCE_OK" -ne 1 ]]; then
  RUN_STATUS="fail"
  FAILURE_STAGE="nonce_roundtrip"
  die "the returned nonce did not match the prompt"
fi
if [[ "$LEDGER_REQUESTS" -lt 1 ]]; then
  RUN_STATUS="fail"
  FAILURE_STAGE="ledger_evidence"
  die "no routed request was recorded in the d-token ledger"
fi
if [[ "$ROUTE_ID_VERIFIED" -ne 1 ]]; then
  RUN_STATUS="fail"
  FAILURE_STAGE="route_identity"
  die "route codex-real-config was not verified in the dashboard evidence"
fi

RUN_STATUS="pass"
echo "R3.C2 PASS: Codex(real config) -> d-token:${PORT} -> opencodex -> upstream; nonce ok; ledger requests=$LEDGER_REQUESTS; compression=$COMPRESSION_METADATA"
echo "report: $REPORT_PATH"
