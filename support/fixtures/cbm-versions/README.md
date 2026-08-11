# cbm-versions fixtures

Per-version acceptance contract for the Codebase Memory v2 manifest
(`code/runtimes/codebase-memory/artifact-manifest.json`).

The five official upstream releases (`v0.9.0`, `v0.8.1`, `0.8.0`, `0.7.0`,
`0.6.1`) plus the approved prerelease pin (`v0.9.1-rc.1`) are registered with
tag/commit/platform-matrix identity only. This directory is the landing point
for the per-version real-verification evidence produced post-N0 by
`support/scripts/run_cbm_version_acceptance.py`:

- `v0.9.1-rc.1`, `v0.9.0`, `v0.8.1` (official binaries available): probe plans
  for indexing / graph query / call chain / dependency / impact; real probe
  receipts bind to `C_final` evidence (N4/N8) and are never committed as fake
  binaries or synthetic passes.
- `0.8.0`, `0.7.0`, `0.6.1` (archived releases, assets=0): source-tag
  registration only, always fail closed (`incompatible`/`untrusted`), and never
  offered to the UI for configuration/indexing/graph.

Rules (same as `support/fixtures/README.md`):

1. No real prompts, code, command output, provider keys, or user paths.
2. No real `codebase-memory-mcp` binary is committed here; the no-touch
   producer never downloads or executes anything.
3. Fixture mode is the default for the harness tests; real execution requires
   the post-N0 gate and runs in a temporary project outside the repository.
