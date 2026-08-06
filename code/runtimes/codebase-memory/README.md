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
pin (immutable `v0.9.0`) was replaced only after the user confirmed real v0.9.0
usage and the upgrade decision; no hash/signature/platform check is bypassed.

`artifact-manifest.json` is the machine-readable source of archive and extracted
binary SHA-256 values for the supported Windows/macOS targets. Release
preparation must verify the archive digest and the Cosign blob bundle against
the pinned GitHub Actions workflow identity and OIDC issuer before extracting
the headless binary. The extracted `LICENSE` and
`THIRD_PARTY_NOTICES.md` must accompany every desktop bundle.

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
