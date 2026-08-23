# agent-config-acceptance fixtures

PR-6 config-acceptance harness (T-PR6, EXECUTION.md PR-6) isolated temp-root
fixture. These files simulate per-agent configuration files so the harness can
exercise configure -> reread -> clear -> exact restore, per-agent reverse
rollback, dual-hash/permission/backup/receipt/readback evidence, and the
no-secret/no-content/no-absolute-path privacy posture WITHOUT touching any real
user configuration.

## Rules

1. All files are synthetic. No real prompts, no real code, no real command
   output, no real provider keys, and no real user paths.
2. Secret-shaped markers are deliberate and synthetic. Any leak of a marker
   into harness stdout or a receipt fails the candidate contract
   (mirrors the cc-switch fixture rule in `support/fixtures/README.md`).
3. `agents/auth.json` is a decoy: the harness must NEVER read, hash, or print
   it. The harness only touches files explicitly listed in `patches/*.json`;
   any patch spec that targets an auth/credential/secret-named file is refused
   fail-closed at the detect step.
4. The harness only ever writes inside its own isolated temporary root (a
   byte+mode copy of the listed `agents/` files). The files under `agents/`
   themselves are only read for original hash/permission evidence.
5. Fixture and `--dry-run` runs are contract verification only. They do NOT
   claim the N4 verification node (node id `N4-preflight`, lane `fixture`,
   status `blocked` for dry runs). Real N4 runs use `--lane real` with the
   frozen candidate commit and are executed by the orchestrator after N0.

## Layout

- `agents/<agent>/<config>` — synthetic per-agent config files (copies of the
  byte content are what the harness configures; originals stay untouched).
- `agents/auth.json` — decoy auth-shaped file; must never be read or printed.
- `patches/<agent>.json` — declarative patch specs consumed by the harness
  (agent id, per-file marker replacement, redacted diff label).
