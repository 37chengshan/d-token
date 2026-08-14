#!/usr/bin/env bash
# R2.D: real Codex routed-flow acceptance.
#
# Proves `Codex CLI -> d-token listener -> ocxlocal local proxy -> upstream`
# with the Responses protocol and dynamic model passthrough, WITHOUT touching
# the user's real ~/.codex configuration (AGENTS.md §5.6: Codex real-config
# writes are not authorized; the OpenCode authorization does not extend here).
# A temporary CODEX_HOME with a minimal config.toml points Codex at a
# temporary d-token listener; the listener forwards to the local ocxlocal
# proxy (127.0.0.1:10100) which owns dynamic model switching.
#
# Evidence: the nonce round-trip, the d-token dashboard/ledger request counts,
# and a structured report under support/artifacts/codex-real-routed/.
set -euo pipefail
umask 077

die() { echo "error: $*" >&2; exit 1; }
require_value() { [[ $# -ge 2 && -n "$2" ]] || die "$1 requires a value"; }

PORT="${DTOKEN_CODEX_TEST_PORT:-18201}"
UPSTREAM_BASE_URL="${DTOKEN_CODEX_UPSTREAM:-http://127.0.0.1:10100/v1}"
MODEL="${DTOKEN_CODEX_MODEL:-opencode-go/deepseek-v4-flash}"
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --port) require_value "$@"; PORT="$2"; shift 2;;
    --upstream) require_value "$@"; UPSTREAM_BASE_URL="$2"; shift 2;;
    --model) require_value "$@"; MODEL="$2"; shift 2;;
    --dry-run) DRY_RUN=1; shift;;
    *) die "unknown argument: $1";;
  esac
done

[[ "$PORT" =~ ^[0-9]+$ ]] || die "port must be numeric"
(( PORT >= 1024 && PORT <= 65535 )) || die "port must be between 1024 and 65535"
[[ "$MODEL" == */* ]] || die "model must be PROVIDER/MODEL form"
# P0-7: never write a credential-bearing upstream URL into the report; the
# upstream must be a plain http(s) URL without userinfo/query/fragment.
python3 - "$UPSTREAM_BASE_URL" <<'PY'
import sys
from urllib.parse import urlsplit
url = sys.argv[1]
parts = urlsplit(url)
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
) model=$MODEL port=$PORT; no request will be sent"
  exit 0
fi
command -v codex >/dev/null 2>&1 || die "codex CLI is required"
command -v cargo >/dev/null 2>&1 || die "cargo is required"
command -v curl >/dev/null 2>&1 || die "curl is required"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/dtoken-codex-real.XXXXXX")"
CODEX_HOME="$TEMP_ROOT/codex-home"
mkdir -p "$CODEX_HOME"
DATA_DIR="$TEMP_ROOT/runtime-data"
mkdir -p "$DATA_DIR"
CONFIG_TOML="$CODEX_HOME/config.toml"
STDOUT_PATH="$TEMP_ROOT/codex.stdout"
STDERR_PATH="$TEMP_ROOT/codex.stderr"
PROXY_STDOUT="$TEMP_ROOT/proxy.stdout"
PROXY_STDERR="$TEMP_ROOT/proxy.stderr"
REPORT_DIR="$ROOT/support/artifacts/codex-real-routed"
mkdir -p "$REPORT_DIR"
REPORT_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
REPORT_PATH="$REPORT_DIR/real-routed-${REPORT_STAMP}-$$.json"
PROXY_PID=""
RUN_STATUS="started"
FAILURE_STAGE="preflight"
NONCE=""
NONCE_OK=0
LEDGER_REQUESTS=0

cleanup() {
  [[ -z "$PROXY_PID" ]] || kill "$PROXY_PID" 2>/dev/null || true
  # P0-7: the report carries the redacted upstream (scheme://host/path only),
  # never userinfo/query/fragment.
  REDACTED_UPSTREAM="$(python3 - "$UPSTREAM_BASE_URL" <<'PY'
import sys
from urllib.parse import urlsplit
parts = urlsplit(sys.argv[1])
print(f"{parts.scheme}://{parts.netloc}{parts.path}")
PY
)"
  cat >"$REPORT_PATH" <<JSON
{
  "test": "codex-real-routed",
  "stage": "R2.D",
  "run_status": "$RUN_STATUS",
  "failure_stage": "$FAILURE_STAGE",
  "nonce_ok": $NONCE_OK,
  "ledger_requests_after": $LEDGER_REQUESTS,
  "model": "$MODEL",
  "listener_port": $PORT,
  "upstream_base_url": "$REDACTED_UPSTREAM",
  "isolated_codex_home": true,
  "user_config_touched": false,
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

# 2. Start the d-token listener on the temporary loopback port, pointing at
#    the local ocxlocal proxy.
FAILURE_STAGE="runtime_start"
cat >"$TEMP_ROOT/proxy-config.json" <<JSON
{
  "providers": [
    {
      "id": "ocxlocal",
      "name": "OpenCodex local",
      "kind": "custom",
      "protocol": "open_ai",
      "base_url": "$UPSTREAM_BASE_URL",
      "models": [],
      "timeout_ms": 60000,
      "wire_api": "open_ai_responses"
    }
  ],
  "routes": [
    {
      "id": "codex-route",
      "name": "Codex routed",
      "input_protocol": "open_ai",
      "instance_id": "codex",
      "target_provider_id": "ocxlocal",
      "chain_mode": "direct",
      "compression_policy": "off",
      "bind_host": "127.0.0.1",
      "local_port": $PORT
    }
  ],
  "instances": [
    {
      "id": "codex",
      "name": "Codex",
      "agent": "codex",
      "route_id": "codex-route",
      "local_port": $PORT,
      "process_status": "stopped",
      "log_path": ""
    }
  ]
}
JSON
"$CODE_ROOT/target/debug/dtoken-proxy" \
  --host 127.0.0.1 --port "$PORT" \
  --config "$TEMP_ROOT/proxy-config.json" \
  --data-dir "$DATA_DIR" >"$PROXY_STDOUT" 2>"$PROXY_STDERR" &
PROXY_PID=$!
for _ in $(seq 1 60); do
  if curl --silent --fail "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then break; fi
  sleep 0.5
done
curl --silent --fail "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1 || die "d-token listener did not become healthy"

# 3. Isolated Codex config: the d-token listener is the provider; the model
#    is passed through to the local proxy (no auth, Responses protocol).
FAILURE_STAGE="codex_config"
cat >"$CONFIG_TOML" <<TOML
model = "$MODEL"
model_provider = "dtoken"

[model_providers.dtoken]
name = "d-token local route"
base_url = "http://127.0.0.1:$PORT/v1"
wire_api = "responses"
requires_openai_auth = false
TOML

# 4. Real request: Codex exec returns a nonce we verify.
FAILURE_STAGE="codex_run"
NONCE="$(python3 -c 'import secrets; print(secrets.token_hex(12))')"
set +e
CODEX_HOME="$CODEX_HOME" codex exec --sandbox read-only "Return only this exact nonce: ${NONCE}" >"$STDOUT_PATH" 2>"$STDERR_PATH"
CODEX_STATUS=$?
set -e
if [[ $CODEX_STATUS -eq 0 ]]; then
  if grep -q "$NONCE" "$STDOUT_PATH"; then NONCE_OK=1; fi
fi

# 5. Runtime evidence: the ledger must show the routed request.
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

RUN_STATUS="pass"
echo "R2.D PASS: Codex -> d-token:${PORT} -> ocxlocal -> upstream; ledger requests=$LEDGER_REQUESTS"
echo "report: $REPORT_PATH"
