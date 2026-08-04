#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${DTOKEN_TEST_PORT:-38101}"
TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/dtoken-fixture-${PORT}.XXXXXX")"
LOG="$TEMP_ROOT/proxy.log"
CONFIG_PATH="$ROOT/code/configs/config.example.json"
DATA_DIR="$TEMP_ROOT/data"
PID=""

export DTOKEN_PROFILE=fixture
export DTOKEN_FIXTURE_ROOT="$ROOT/support/fixtures"
PYTHON_BIN="${PYTHON_BIN:-python3}"
if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
  PYTHON_BIN="python"
fi

cleanup() {
  status=$?
  trap - EXIT
  if [[ -n "$PID" ]]; then
    kill "$PID" >/dev/null 2>&1 || true
    wait "$PID" >/dev/null 2>&1 || true
  fi
  if ((status != 0)); then
    echo "fixture smoke failed; bounded proxy log follows" >&2
    sed -n '1,200p' "$LOG" >&2 || true
  fi
  rm -rf "$TEMP_ROOT"
  exit "$status"
}
trap cleanup EXIT

cd "$ROOT"
"$PYTHON_BIN" support/scripts/check_structure.py

cd "$ROOT/code"
cargo check --workspace
if command -v dlltool.exe >/dev/null 2>&1 || [[ "$(rustc -vV | grep host || true)" != *"windows-gnu"* ]]; then
  cargo test -p dtoken-config-patcher --test agent_bridge_fixture
  cargo test -p dtoken-agent-bridge --test subprocess_contract
  cargo test --workspace
else
  echo "Skipping cargo test: windows-gnu toolchain requires dlltool.exe, which is not installed."
fi

cargo run -p dtoken-proxy -- --port "$PORT" --config "$CONFIG_PATH" --data-dir "$DATA_DIR" >"$LOG" 2>&1 &
PID=$!

READY=false
for _attempt in $(seq 1 120); do
  if ! kill -0 "$PID" >/dev/null 2>&1; then
    break
  fi
  if curl -fsS --max-time 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    READY=true
    break
  fi
  sleep 0.5
done
if [[ "$READY" != "true" ]]; then
  echo "fixture proxy did not become healthy on port ${PORT}" >&2
  sed -n '1,200p' "$LOG" >&2 || true
  exit 1
fi

assert_contains() {
  local method="$1"
  local path="$2"
  local expected="$3"
  local label="$4"
  local response
  if [[ "$method" == "POST" ]]; then
    if ! response="$(curl -fsS --max-time 5 -X POST "http://127.0.0.1:${PORT}${path}")"; then
      echo "${label} request failed" >&2
      return 1
    fi
  else
    if ! response="$(curl -fsS --max-time 5 "http://127.0.0.1:${PORT}${path}")"; then
      echo "${label} request failed" >&2
      return 1
    fi
  fi
  if [[ "$response" != *"$expected"* ]]; then
    echo "${label} response did not contain the expected fixture marker" >&2
    return 1
  fi
}

curl -fsS --max-time 5 "http://127.0.0.1:${PORT}/health" >/dev/null
assert_contains GET "/rtk/status" "fixture" "rtk status"
assert_contains GET "/rtk/gain" "fixture" "rtk gain"
assert_contains GET "/rtk/discover" "fixture" "rtk discovery"
assert_contains GET "/rtk/session" "fixture" "rtk session"
assert_contains GET "/headroom/status" "dtoken-owned-runtime" "Headroom status"
assert_contains GET "/headroom/stats" "dtoken-owned-runtime" "Headroom stats"
assert_contains GET "/stats-history" "dtoken-owned-runtime" "stats history"
assert_contains POST "/doctor/run" "no-real-binary-default" "Doctor"
assert_contains GET "/projects" "dtoken-owned-runtime" "project registry"
assert_contains POST "/config/patches/plan" "desktop_transaction_executor_required" "config patch plan"
assert_contains GET "/dashboard/payload" "dtoken-owned-runtime" "dashboard"

echo "fixture mode no-touch smoke tests passed on port ${PORT}"
