#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${DTOKEN_TEST_PORT:-38101}"
LOG="${TMPDIR:-/tmp}/dtoken-fixture-${PORT}.log"

export DTOKEN_PROFILE=fixture
export DTOKEN_FIXTURE_ROOT="$ROOT/support/fixtures"

cd "$ROOT"
python support/scripts/check_structure.py

cd "$ROOT/code"
cargo check --workspace
cargo test --workspace

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
curl -fsS "http://127.0.0.1:${PORT}/headroom/status" | grep -q "fixture"
curl -fsS "http://127.0.0.1:${PORT}/headroom/stats" | grep -q "fixture"
curl -fsS "http://127.0.0.1:${PORT}/projects/default-policy" | grep -q "safe"
curl -fsS -X POST "http://127.0.0.1:${PORT}/config/patches/plan" | grep -q "m1-mock-patch"

echo "fixture mode smoke tests passed on port ${PORT}"
