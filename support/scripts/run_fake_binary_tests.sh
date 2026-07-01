#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${DTOKEN_TEST_PORT:-38102}"
LOG="${TMPDIR:-/tmp}/dtoken-fake-bin-${PORT}.log"
TEST_HOME="$(mktemp -d)"

chmod +x "$ROOT/support/fake-bin/rtk" "$ROOT/support/fake-bin/headroom"

export PATH="$ROOT/support/fake-bin:$PATH"
export HOME="$TEST_HOME"
unset DTOKEN_PROFILE || true
export DTOKEN_FIXTURE_ROOT="$ROOT/support/fixtures"

cd "$ROOT/code"
cargo run -p dtoken-proxy -- --port "$PORT" --config configs/config.example.json >"$LOG" 2>&1 &
PID=$!
cleanup() {
  kill "$PID" >/dev/null 2>&1 || true
  rm -rf "$TEST_HOME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

sleep 2

curl -fsS "http://127.0.0.1:${PORT}/health" >/dev/null
curl -fsS "http://127.0.0.1:${PORT}/rtk/status" | grep -q "0.0.0-fixture"
curl -fsS "http://127.0.0.1:${PORT}/headroom/status" | grep -q "0.0.0-fixture"

echo "fake binary smoke tests passed on port ${PORT}"
