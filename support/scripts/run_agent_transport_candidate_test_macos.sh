#!/usr/bin/env bash
set -euo pipefail

umask 077

die() {
  echo "error: $*" >&2
  exit 1
}

[[ "$(uname -s)" == "Darwin" ]] || die "this candidate test requires macOS"
for command in cargo claude cline codex python3; do
  command -v "$command" >/dev/null 2>&1 || die "$command is required"
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
CLAUDE_CONFIG="${HOME}/.claude/settings.json"
CODEX_CONFIG="${CODEX_HOME:-${HOME}/.codex}/config.toml"
CLINE_CONFIG="${HOME}/.cline/data/settings/providers.json"
ENDPOINT="http://127.0.0.1:18001"
MODEL="dtoken-candidate-validation"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
BACKUP_ROOT="${HOME}/Library/Application Support/d-token/candidate-agent-backups/${STAMP}-$$"
REPORT_DIR="$ROOT/support/artifacts/agent-transport-candidate"
REPORT_PATH="$REPORT_DIR/candidate-${STAMP}-$$.json"

for target in "$CLAUDE_CONFIG" "$CODEX_CONFIG" "$CLINE_CONFIG"; do
  [[ -f "$target" && ! -L "$target" ]] || die "candidate target must be a regular non-symlink file"
done
mkdir -p "$BACKUP_ROOT" "$REPORT_DIR"
chmod 700 "$BACKUP_ROOT" "$REPORT_DIR"

safe_version() {
  "$1" --version 2>&1 | python3 -c '
import re
import sys
text = sys.stdin.read()
match = re.search(r"\d+(?:\.\d+){1,3}(?:[-+][A-Za-z0-9.-]+)?", text)
print(match.group(0) if match else "version-unparsed")
'
}

CLAUDE_VERSION="$(safe_version claude)"
CODEX_VERSION="$(safe_version codex)"
CLINE_VERSION="$(safe_version cline)"

(
  cd "$CODE_ROOT"
  cargo build -p dtoken-config-patcher --example agent_transport_candidate_probe >/dev/null
)
PROBE="$CODE_ROOT/target/debug/examples/agent_transport_candidate_probe"

CLAUDE_RECEIPT="$(
  "$PROBE" claude-code "$CLAUDE_CONFIG" "$BACKUP_ROOT" "$ENDPOINT" "$MODEL"
)"
CODEX_RECEIPT="$(
  "$PROBE" codex "$CODEX_CONFIG" "$BACKUP_ROOT" "$ENDPOINT" "$MODEL"
)"
CLINE_RECEIPT="$(
  "$PROBE" cline "$CLINE_CONFIG" "$BACKUP_ROOT" "$ENDPOINT" "$MODEL"
)"

python3 - "$REPORT_PATH" "$CLAUDE_VERSION" "$CODEX_VERSION" "$CLINE_VERSION" "$CLAUDE_RECEIPT" "$CODEX_RECEIPT" "$CLINE_RECEIPT" <<'PY'
import json
import os
import pathlib
import sys
import tempfile
from datetime import datetime, timezone

(
    report_path,
    claude_version,
    codex_version,
    cline_version,
    claude_receipt,
    codex_receipt,
    cline_receipt,
) = sys.argv[1:]
payload = {
    "schema_version": 1,
    "created_at": datetime.now(timezone.utc).isoformat(),
    "status": "pass",
    "source": "dtoken-config-patcher",
    "candidate_id": os.environ.get("DTOKEN_RELEASE_CANDIDATE_ID") or None,
    "agents": [
        {"installed_version": claude_version, **json.loads(claude_receipt)},
        {"installed_version": codex_version, **json.loads(codex_receipt)},
        {"installed_version": cline_version, **json.loads(cline_receipt)},
    ],
    "contract": {
        "preview_apply_verify_clear_restore": True,
        "exact_original_bytes_restored": True,
        "network_request_made": False,
        "agent_process_started": False,
        "raw_config_returned": False,
        "credential_value_read": False,
    },
    "backup_root": "<d-token-private-candidate-backups>",
    "report_path": "support/artifacts/agent-transport-candidate/" + pathlib.Path(report_path).name,
}
destination = pathlib.Path(report_path)
fd, temporary = tempfile.mkstemp(prefix=destination.name + ".", dir=destination.parent)
try:
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, separators=(",", ":"))
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, destination)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
PY
