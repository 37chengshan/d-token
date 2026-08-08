#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${DTOKEN_TEST_PORT:-38102}"
LOG="${TMPDIR:-/tmp}/dtoken-fake-bin-${PORT}.log"
TEST_HOME="$(mktemp -d "${TMPDIR:-/tmp}/dtoken-fake-home-${PORT}.XXXXXX")"
PID=""

# Register cleanup before the first fallible staging operation below, so
# `set -e` failures cannot leak TEST_HOME.
cleanup() {
  status=$?
  trap - EXIT
  if [[ -n "$PID" ]]; then
    kill "$PID" >/dev/null 2>&1 || true
    wait "$PID" >/dev/null 2>&1 || true
  fi
  if ((status != 0)); then
    echo "fake binary smoke failed; bounded proxy log follows" >&2
    sed -n '1,200p' "$LOG" >&2 || true
  fi
  rm -rf "$TEST_HOME" "$LOG" >/dev/null 2>&1 || true
  exit "$status"
}
trap cleanup EXIT

# Resolve the host cargo/rustup homes before HOME is overridden below, so
# `cargo run` keeps using the real toolchain instead of the isolated test
# home. Explicit CARGO_HOME/RUSTUP_HOME (standalone cargo, CI) are respected.
export CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-${HOME}/.rustup}"

# Stage the fake binaries inside the isolated test home and chmod the staged
# copies only; tracked support/fake-bin sources keep their committed modes.
FAKE_BIN="$TEST_HOME/fake-bin"
mkdir -p "$FAKE_BIN"
cp "$ROOT/support/fake-bin/rtk" "$ROOT/support/fake-bin/headroom" "$FAKE_BIN/"
chmod +x "$FAKE_BIN/rtk" "$FAKE_BIN/headroom"
# Fake scripts resolve fixtures relative to their own location.
ln -s "$ROOT/support/fixtures" "$TEST_HOME/fixtures"

export PATH="$FAKE_BIN:$PATH"
export HOME="$TEST_HOME"
export DTOKEN_TEST_HOME="$TEST_HOME"
export DTOKEN_FAKE_BIN="$FAKE_BIN"
export DTOKEN_PROFILE=fake-binary
export DTOKEN_FIXTURE_ROOT="$ROOT/support/fixtures"

cd "$ROOT/code"
cargo run -p dtoken-proxy -- --port "$PORT" --config configs/config.example.json >"$LOG" 2>&1 &
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
  echo "fake binary proxy did not become healthy on port ${PORT}" >&2
  exit 1
fi

curl -fsS --max-time 5 "http://127.0.0.1:${PORT}/rtk/status" | grep -q "0.0.0-fixture"
curl -fsS --max-time 5 "http://127.0.0.1:${PORT}/headroom/status" | grep -q "0.0.0-fixture"
curl -fsS --max-time 5 -X POST "http://127.0.0.1:${PORT}/doctor/run" | grep -q "temporary-home-for-command-tests"

echo "fake binary smoke tests passed on port ${PORT}"
