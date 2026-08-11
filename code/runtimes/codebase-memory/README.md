# codebase-memory-mcp provenance boundary

This directory records the pinned, externally built headless artifact used by
the optional d-token Codebase Memory integration. The upstream C sources are
not copied into the d-token runtime and the upstream installer is never run.

- Upstream: `DeusData/codebase-memory-mcp`
- Release: immutable prerelease `v0.9.1-rc.1`
- Commit: `ee9833df2c72b750378273897ffec9410fc2c4f2`
- Published: `2026-07-30T03:15:15Z` (GitHub immutable pre-release)
- License: MIT
- Distribution: build/release preparation only; no runtime downloader or updater

### Version pin (2026-08-06)

The pin tracks the upstream's newest immutable release `v0.9.1-rc.1` (pre-release,
commit `ee9833df2c72b750378273897ffec9410fc2c4f2`, published `2026-07-30T03:15:15Z`).
Its platform archives and extracted binary SHA-256 values were measured from the
official GitHub release assets and match the manifest below. The UI labels the
component as a pre-release and the page requires exactly this pinned tag/commit;
any other archive or binary is rejected by the fail-closed verification and the
desktop page truthfully reports "untrusted (v0.9.1-rc.1 required)". The previous
pin (immutable `v0.9.0` / `b637e3330c96cfe452da623db068c241aaa3ec01`) is
superseded and retained only as the historical reproduction baseline for the
user-reported v0.9.0 import failure; it is not offered for new imports. The
replacement was made only after the user confirmed real v0.9.0 usage and the
upgrade decision; no hash/signature/platform check is bypassed.

`artifact-manifest.json` (schema v2) is the machine-readable source of archive and
extracted binary SHA-256 values for the supported Windows/macOS targets. Release
preparation must verify the archive digest and the Cosign blob bundle against
the pinned GitHub Actions workflow identity and OIDC issuer before extracting
the headless binary. The extracted `LICENSE` and
`THIRD_PARTY_NOTICES.md` must accompany every desktop bundle.

### Version registry (v2, 2026-08-10)

The v2 manifest addresses the five most recent official upstream releases plus
the approved prerelease pin, each with its own tag/commit/platform matrix:

| version | kind | assets | runtime |
|---|---|---|---|
| `v0.9.1-rc.1` | approved-prerelease pin | verified (4 desktop targets) | verified (user-approved pin) |
| `v0.9.0` | official | available (4 desktop targets) | blocked until N4/N8 real verification |
| `v0.8.1` | official | available (3 targets; **no windows-arm64**) | blocked until N4/N8 real verification |
| `0.8.0` | official | none (archived, assets=0) | blocked (source-tag only) |
| `0.7.0` | official | none (archived, assets=0) | blocked (source-tag only) |
| `0.6.1` | official | none (archived, assets=0) | blocked (source-tag only) |

Version identity is tag/commit/hash only — the upstream `server.json` version
field is never read (it lags behind actual releases). Versions without official
assets are registered at source-tag level and fail closed: never claimed
runtime-compatible, never offered to the UI for configuration/indexing/graph.
`v0.8.1` has no official windows-arm64 asset and that target is explicitly
`incompatible`. Per-version real verification (indexing/graph query/call
chain/dependency/impact) runs post-N0 through
`support/scripts/run_cbm_version_acceptance.py`; until that evidence binds,
affected versions remain blocked.

The universal macOS executable is a deterministic d-token `lipo -create`
derivation of the two independently verified upstream macOS binaries. Its
derived hash and input archive names are recorded separately; it is not
described as an independently upstream-signed asset. Release signing changes
the final Mach-O/PE bytes, so preparation verifies the pinned pre-sign hash,
while the release launcher verifies the embedded Apple Team ID or Windows
certificate thumbprint before execution.

The product never exposes the upstream web UI. Agent-owned MCP sessions run
through `dtoken-codebase-memory-launcher`; d-token-owned graph viewing uses a
separate authorized read-only loopback projection.
