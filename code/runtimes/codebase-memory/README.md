# codebase-memory-mcp provenance boundary

This directory records the pinned, externally built headless artifact used by
the optional d-token Codebase Memory integration. The upstream C sources are
not copied into the d-token runtime and the upstream installer is never run.

- Upstream: `DeusData/codebase-memory-mcp`
- Release: immutable `v0.9.0`
- Commit: `b637e3330c96cfe452da623db068c241aaa3ec01`
- License: MIT
- Distribution: build/release preparation only; no runtime downloader or updater

### Version pin check (2026-08-06)

Checked against the upstream GitHub release list: the newest available release is
`v0.9.1-rc.1` (pre-release, commit `ee9833df2c72b750378273897ffec9410fc2c4f2`).
Per the pinned-stable + Sigstore attestation provenance mode, pre-releases are
explicitly blocked and are not imported: their platform archives and extracted
binary SHA-256 values do not match this manifest, so the fail-closed verification
rejects them and the desktop page truthfully reports "untrusted (v0.9.0 required)".
The pin remains on the immutable `v0.9.0` release; no hash/signature/platform check
is bypassed.

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
