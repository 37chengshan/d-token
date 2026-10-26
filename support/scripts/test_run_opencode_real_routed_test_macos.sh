#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/support/scripts/run_opencode_real_routed_test_macos.sh"
TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/dtoken-opencode-dryrun-test.XXXXXX")"

cleanup() {
  rm -rf "$TEMP_ROOT"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ "$(uname -s)" == "Darwin" ]] || fail "this focused test requires macOS"
[[ -x "$SCRIPT" ]] || fail "real routed test script must be executable"
bash -n "$SCRIPT"
if rg -q 'OPENCODE_SESSION_ARGS' "$SCRIPT"; then
  fail "macOS Bash 3.2 must not expand an empty OpenCode session argument array under set -u"
fi
rg -q 'REQUIRED_COMPLETED_TOOL_RESULTS=7' "$SCRIPT" || fail "compression workload must cross the six completed-tool-pair protected tail"
rg -q 'ccr_candidate_probe' "$SCRIPT" || fail "compression workload must run the real MCP CCR lifecycle probe"
rg -q 'private_store_configured' "$SCRIPT" || fail "CCR evidence must distinguish private-store and metadata-only modes"
rg -q 'raw_available_before_delete' "$SCRIPT" || fail "CCR evidence must prove availability before authorized deletion"
rg -q 'repeat_delete_state' "$SCRIPT" || fail "CCR evidence must prove idempotent deletion"
if rg -q 'CCR aggregate-only evidence is missing or exposes raw content' "$SCRIPT"; then
  fail "private encrypted CCR storage must not be rejected as raw exposure"
fi
if rg -q 'for filler in' "$SCRIPT"; then
  fail "plain-text filler turns do not cross the completed-tool-pair protected tail"
fi
rg -q 'public placeholder context alpha beta gamma delta' "$SCRIPT" || fail "compression workload must use repeat-fold-safe natural text"
if rg -q 'PUBLIC_PLACEHOLDER_CONTEXT' "$SCRIPT"; then
  fail "all-uppercase underscore markers are exact anchors and cannot be used as a compression candidate"
fi

python3 - "$SCRIPT" "$TEMP_ROOT" <<'PY'
import json
import pathlib
import subprocess
import sys

script, root = map(pathlib.Path, sys.argv[1:])
source = script.read_text()
start = source.index("classify_opencode_failure() {")
end = source.index("\nPY\n}\n", start) + len("\nPY\n}\n")
classifier = source[start:end]
stdout = root / "classification.stdout"
stderr = root / "classification.stderr"
cases = [
    (json.dumps({"type": "error", "error": {"name": "APICallError", "data": {"statusCode": 429, "message": "private detail"}}}), "", 1, "upstream_http_429"),
    (json.dumps({"type": "error", "error": {"name": "SensitiveIdentifierError"}}), "", 1, "error_event_unclassified"),
    (json.dumps({"type": "error", "error": {"name": "ContextOverflowError"}}), "", 0, "context_overflow"),
    ("", "Unknown argument: synthetic-option", 1, "cli_argument_unsupported"),
    (json.dumps({"type": "text", "part": {"text": "private detail"}}), "", 0, "none"),
]
for output, error, status, expected in cases:
    stdout.write_text(output)
    stderr.write_text(error)
    result = subprocess.run(["bash", "-c", classifier + '\nclassify_opencode_failure "$@"', "classifier", str(stdout), str(stderr), str(status)], check=True, capture_output=True, text=True)
    assert result.stdout.strip() == expected, result.stdout
assert 'OPENCODE_FAILURE_CODE="$(classify_opencode_failure "$BOOTSTRAP_STDOUT" "$BOOTSTRAP_STDERR" "$OPENCODE_STATUS")"' in source
assert 'FAILURE_STAGE="opencode_compression_bootstrap"' in source

# A bootstrap failure reports journal counts, never provider messages or bodies.
health = root / "health.json"
health.write_text(json.dumps({"runtime_identity": "dtoken-runtime-local"}))
journals = root / "runtime-data" / "ledger"
journals.mkdir(parents=True)
for name in ("events.jsonl", "route-decisions.jsonl", "request-logs.jsonl", "ccr-metadata.jsonl"):
    (journals / name).write_text(json.dumps({"route_id": "opencode-real-routed", "provider_id": "test-provider", "error": "PRIVATE_UPSTREAM_MESSAGE", "body": "PRIVATE_REQUEST_BODY"}) + "\n")
report_start = source.index("write_redacted_report() {")
python_start = source.index("<<'PY'\n", report_start) + len("<<'PY'\n")
python_end = source.index("\nPY\n", python_start)
report = root / "failed-report.json"
args = [str(report), "failed", "1", "opencode_compression_bootstrap", "test-provider", "test-provider/model", "18101", "explicit_none", "<config>", "<backup>", "1", "restored_exact", "<report>", "{}", "0", "1", "{}", "{}", json.dumps({"config_change_required": True, "provider_state": "absent"}), "upstream_http_429", "1", "not_created"]
import os
result = subprocess.run([sys.executable, "-", *args], input=source[python_start:python_end], env={**os.environ, "DTOKEN_OPENCODE_REAL_ROOT": str(root)}, check=True, capture_output=True, text=True)
payload = json.loads(report.read_text())
assert payload["evidence"]["diagnostics"]["failed_request_count"] == 1
assert payload["ledger"]["request_logs"] == 1
assert payload["evidence"]["provider_forward_verified"] is False
assert "PRIVATE_" not in report.read_text() + result.stdout
PY

CALLER_UMASK="$(umask)"
mkdir -p "$TEMP_ROOT/umask-bin"
python3 - "$TEMP_ROOT/umask-bin/opencode" <<'PY'
import os
import pathlib
import sys

executable = pathlib.Path(sys.argv[1])
executable.write_text("#!/bin/sh\numask\n", encoding="utf-8")
os.chmod(executable, 0o700)
PY
OBSERVED_UMASK="$(PATH="$TEMP_ROOT/umask-bin:$PATH" "$SCRIPT" --list-models)"
python3 - "$CALLER_UMASK" "$OBSERVED_UMASK" <<'PY'
import sys
expected, observed = sys.argv[1:]
assert int(expected, 8) == int(observed, 8), (expected, observed)
PY

mkdir -p "$TEMP_ROOT/bin"
python3 - "$TEMP_ROOT/bin/opencode" "$TEMP_ROOT/opencode-called" <<'PY'
import os
import pathlib
import sys

executable, marker = map(pathlib.Path, sys.argv[1:])
executable.write_text(f"#!/bin/sh\ntouch '{marker}'\nexit 99\n", encoding="utf-8")
os.chmod(executable, 0o700)
PY

CONFIG_PATH="$TEMP_ROOT/opencode.json"
python3 - "$CONFIG_PATH" <<'PY'
import json
import pathlib
import sys

pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "provider": {"existing": {"name": "Existing provider"}},
    "enabled_providers": ["existing"],
}, separators=(",", ":")) + "\n", encoding="utf-8")
PY

PORT="$(python3 - <<'PY'
import socket
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
    listener.bind(("127.0.0.1", 0))
    print(listener.getsockname()[1])
PY
)"
BEFORE_HASH="$(shasum -a 256 "$CONFIG_PATH" | awk '{print $1}')"
PATH="$TEMP_ROOT/bin:$PATH" "$SCRIPT" \
  --dry-run \
  --pure \
  --config-path "$CONFIG_PATH" \
  --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 \
  --api-key-ref keychain://dtoken-test/existing \
  --port "$PORT" \
  >"$TEMP_ROOT/result.json"
AFTER_HASH="$(shasum -a 256 "$CONFIG_PATH" | awk '{print $1}')"

[[ "$BEFORE_HASH" == "$AFTER_HASH" ]] || fail "dry-run changed the OpenCode config"
[[ ! -e "$TEMP_ROOT/opencode-called" ]] || fail "dry-run executed OpenCode"
[[ "$(find "$TEMP_ROOT" -maxdepth 1 -name '*.dtoken-backup-*' | wc -l | tr -d ' ')" == "0" ]] || fail "dry-run created a backup"
python3 - "$TEMP_ROOT/result.json" <<'PY'
import json
import pathlib
import sys

result = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert result["status"] == "preflight_ok"
assert result["mode"] == "dry_run"
assert result["credential_mode"] == "os_store_reference"
assert result["strict_json_valid"] is True
assert result["provider_state"] == "absent"
assert result["config_change_required"] is True
assert result["config_mutated"] is False
assert result["backup_created"] is False
assert result["report_created"] is False
assert result["opencode_executed"] is False
assert result["external_request_made"] is False
assert result["opencode_external_plugins_enabled"] is False
assert result["ccr_private_store_enabled"] is False
assert result["ccr_key_cleanup_planned"] is False
assert result["normal_user_config"].startswith("<explicit-config>/")
PY

PATH="$TEMP_ROOT/bin:$PATH" "$SCRIPT" \
  --dry-run \
  --pure \
  --compression-workload \
  --config-path "$CONFIG_PATH" \
  --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 \
  --api-key-ref keychain://dtoken-test/existing \
  --port "$PORT" \
  >"$TEMP_ROOT/compression-preflight.json"
python3 - "$TEMP_ROOT/compression-preflight.json" <<'PY'
import json
import pathlib
import sys
result = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert result["status"] == "preflight_ok"
assert result["ccr_private_store_enabled"] is True
assert result["ccr_key_cleanup_planned"] is True
assert result["config_mutated"] is False
assert result["external_request_made"] is False
PY

OWNED_PATH="$TEMP_ROOT/owned.json"
python3 - "$OWNED_PATH" <<'PY'
import json
import pathlib
import sys
pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "provider": {
        "dtoken-local-route": {
            "npm": "@ai-sdk/openai-compatible",
            "name": "d-token local route",
            "options": {
                "baseURL": "http://127.0.0.1:18001/v1",
                "apiKey": "{env:DTOKEN_LOCAL_LISTENER_TOKEN}",
            },
            "models": {
                "existing-model": {"name": "d-token routed existing-model"},
            },
        },
    },
}, separators=(",", ":")) + "\n", encoding="utf-8")
PY
OWNED_HASH="$(shasum -a 256 "$OWNED_PATH" | awk '{print $1}')"
"$SCRIPT" --dry-run --config-path "$OWNED_PATH" --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 --upstream-no-auth --port "$PORT" \
  >"$TEMP_ROOT/owned-result.json"
[[ "$OWNED_HASH" == "$(shasum -a 256 "$OWNED_PATH" | awk '{print $1}')" ]] || fail "owned-provider dry-run changed config"
python3 - "$TEMP_ROOT/owned-result.json" <<'PY'
import json
import pathlib
import sys
result = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert result["status"] == "preflight_ok"
assert result["provider_state"] == "owned_existing"
assert result["config_change_required"] is True
assert result["config_mutated"] is False
PY

MATCHING_PATH="$TEMP_ROOT/matching.json"
python3 - "$MATCHING_PATH" "$PORT" <<'PY'
import json
import pathlib
import sys

path, port = sys.argv[1:]
pathlib.Path(path).write_text(json.dumps({
    "provider": {
        "dtoken-local-route": {
            "npm": "@ai-sdk/openai-compatible",
            "name": "d-token local route",
            "options": {
                "baseURL": f"http://127.0.0.1:{port}/v1",
                "apiKey": "{env:DTOKEN_LOCAL_LISTENER_TOKEN}",
            },
            "models": {
                "model-v1": {"name": "d-token routed model-v1"},
            },
        },
    },
}, separators=(",", ":")) + "\n", encoding="utf-8")
PY
MATCHING_HASH="$(shasum -a 256 "$MATCHING_PATH" | awk '{print $1}')"
"$SCRIPT" --dry-run --pure --config-path "$MATCHING_PATH" --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 --upstream-no-auth --port "$PORT" \
  >"$TEMP_ROOT/matching-result.json"
[[ "$MATCHING_HASH" == "$(shasum -a 256 "$MATCHING_PATH" | awk '{print $1}')" ]] || fail "matching-provider dry-run changed config"
python3 - "$TEMP_ROOT/matching-result.json" <<'PY'
import json
import pathlib
import sys
result = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert result["status"] == "preflight_ok"
assert result["provider_state"] == "owned_matching"
assert result["config_change_required"] is False
assert result["config_mutated"] is False
assert result["backup_created"] is False
PY

INVALID_PATH="$TEMP_ROOT/invalid.json"
python3 - "$INVALID_PATH" <<'PY'
import pathlib
import sys
pathlib.Path(sys.argv[1]).write_text('{"provider":{},}\n', encoding="utf-8")
PY
INVALID_HASH="$(shasum -a 256 "$INVALID_PATH" | awk '{print $1}')"
if "$SCRIPT" --dry-run --config-path "$INVALID_PATH" --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 --upstream-no-auth --port "$PORT" \
  >"$TEMP_ROOT/invalid.out" 2>"$TEMP_ROOT/invalid.err"; then
  fail "invalid JSON unexpectedly passed"
fi
[[ "$INVALID_HASH" == "$(shasum -a 256 "$INVALID_PATH" | awk '{print $1}')" ]] || fail "failed preflight changed invalid config"

CONFLICT_PATH="$TEMP_ROOT/conflict.json"
python3 - "$CONFLICT_PATH" <<'PY'
import json
import pathlib
import sys
pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "provider": {"dtoken-local-route": {}},
}) + "\n", encoding="utf-8")
PY
if "$SCRIPT" --dry-run --config-path "$CONFLICT_PATH" --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 --upstream-no-auth --port "$PORT" \
  >"$TEMP_ROOT/conflict.out" 2>"$TEMP_ROOT/conflict.err"; then
  fail "existing dtoken-local-route provider unexpectedly passed"
fi

rg -q "strict UTF-8 JSON" "$TEMP_ROOT/invalid.err" || fail "invalid JSON did not fail closed clearly"
rg -q "not provably d-token-owned" "$TEMP_ROOT/conflict.err" || fail "provider conflict did not fail closed clearly"
echo "PASS: OpenCode real routed dry-run is read-only and fail-closed"
