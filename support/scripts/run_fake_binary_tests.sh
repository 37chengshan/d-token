#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${DTOKEN_TEST_PORT:-38102}"
LOG="${TMPDIR:-/tmp}/dtoken-fake-bin-${PORT}.log"
TEST_HOME="$(mktemp -d)"
ORIGINAL_HOME="${HOME:-$TEST_HOME}"

chmod +x "$ROOT/support/fake-bin/rtk" "$ROOT/support/fake-bin/headroom"

export PATH="$ROOT/support/fake-bin:$PATH"
# no-touch 隔离：产品进程看到临时 HOME，但 rustup/cargo shim 仍需原工具链
# 配置，否则 cargo run 因找不到默认 toolchain 而失败（代理未启动 → 502）。
export HOME="$TEST_HOME"
export RUSTUP_HOME="${RUSTUP_HOME:-$ORIGINAL_HOME/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$ORIGINAL_HOME/.cargo}"
# 循环回环测试必须直连 127.0.0.1；用户 shell 的 http(s)_proxy 会劫持 curl，
# 在目标未监听时由外部代理返回 502，污染 no-touch 结果。
unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY all_proxy ALL_PROXY
export DTOKEN_TEST_HOME="$TEST_HOME"
export DTOKEN_FAKE_BIN="$ROOT/support/fake-bin"
export DTOKEN_PROFILE=fake-binary
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
curl -fsS -X POST "http://127.0.0.1:${PORT}/doctor/run" | grep -q "temporary-home-for-command-tests"

echo "fake binary smoke tests passed on port ${PORT}"
