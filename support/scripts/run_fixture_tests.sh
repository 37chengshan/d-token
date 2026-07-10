#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${DTOKEN_TEST_PORT:-38101}"
LOG="${TMPDIR:-/tmp}/dtoken-fixture-${PORT}.log"

export DTOKEN_PROFILE=fixture
export DTOKEN_FIXTURE_ROOT="$ROOT/support/fixtures"
PYTHON_BIN="${PYTHON_BIN:-python3}"
if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
  PYTHON_BIN="python"
fi

cd "$ROOT"
"$PYTHON_BIN" support/scripts/check_structure.py

cd "$ROOT/code"
cargo check --workspace
if command -v dlltool.exe >/dev/null 2>&1 || [[ "$(rustc -vV | grep host || true)" != *"windows-gnu"* ]]; then
  cargo test --workspace
else
  echo "Skipping cargo test: windows-gnu toolchain requires dlltool.exe, which is not installed."
fi

cargo run -p dtoken-proxy -- --port "$PORT" --config configs/config.example.json >"$LOG" 2>&1 &
PID=$!
cleanup() {
  kill "$PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT

sleep 2

curl -fsS "http://127.0.0.1:${PORT}/health" >/dev/null
curl -fsS "http://127.0.0.1:${PORT}/rtk/status" | grep -q "fixture"
curl -fsS "http://127.0.0.1:${PORT}/rtk/gain" | grep -q "fixture"
curl -fsS "http://127.0.0.1:${PORT}/rtk/discover" | grep -q "fixture"
curl -fsS "http://127.0.0.1:${PORT}/rtk/session" | grep -q "fixture"
curl -fsS "http://127.0.0.1:${PORT}/headroom/status" | grep -q "fixture"
curl -fsS "http://127.0.0.1:${PORT}/headroom/stats" | grep -q "fixture"
curl -fsS "http://127.0.0.1:${PORT}/stats-history" | grep -q "fixture"
curl -fsS -X POST "http://127.0.0.1:${PORT}/doctor/run" | grep -q "no-real-binary-default"
curl -fsS "http://127.0.0.1:${PORT}/projects/default-policy" | grep -q "safe"
curl -fsS -X POST "http://127.0.0.1:${PORT}/config/patches/plan" | grep -q "m1-mock-patch"
CHAT_PAYLOAD='{"model":"opencode/deepseek-v4-flash-free","messages":[{"role":"user","content":"repeat log line\nrepeat log line\ncompile warning warning"}]}'
CHAT_RESPONSE="$(curl -fsS -X POST "http://127.0.0.1:${PORT}/v1/chat/completions" -H "content-type: application/json" --data "$CHAT_PAYLOAD")"
echo "$CHAT_RESPONSE" | grep -q "d-token-ok"
echo "$CHAT_RESPONSE" | grep -q "dtoken-opencode-local"
echo "$CHAT_RESPONSE" | grep -q "saved_tokens"
curl -fsS "http://127.0.0.1:${PORT}/dashboard/payload" | grep -q "dtoken-owned-runtime"

echo "fixture mode smoke tests passed on port ${PORT}"
