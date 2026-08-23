# Changelog

All notable user-facing changes to d-token are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and release versions follow Semantic Versioning. Pre-release entries describe candidate scope. A public unsigned prerelease is valid only after the platform gates pass for its tagged commit in the same aggregate workflow run.

## [Unreleased]

No user-facing changes have been assigned beyond the `0.0.1` candidate.

## [0.0.1] — unsigned technical preview (2026-08-08)

Candidate version unified from `0.2.0-beta.1` to `0.0.1` (VERSION-0); candidate scope, publication eligibility, and known limitations are unchanged from the `0.2.0-beta.1` entry below.

## [0.2.0-beta.1] — unsigned technical preview (2026-07-26)

`0.2.0-beta.1` is eligible for publication as a GitHub public unsigned prerelease for local evaluation, not a signed, notarized, Authenticode-signed, or production-certified installer release. Publication requires the tagged aggregate workflow to complete same-run macOS ARM64 and Windows x64 gates plus SHA-256, SBOM, license, security, and provenance evidence. Both platforms may display operating-system security warnings.

### Added

- Eight-page Tauri desktop control plane: Overview, Install & Connect, Code Graph (experimental), Analytics, Visual Compression (Beta), Doctor, Changes & Rollback, and Settings.
- Rust-owned loopback runtime for OpenAI Chat Completions, OpenAI Responses, and Anthropic Messages-compatible local entry points.
- Explicit provider/route decisions, redacted Event Ledger evidence, analytics projections, diagnostics, and typed fail-closed errors.
- Preview → redacted diff → backup → confirmation → apply → verify → rollback configuration transactions for supported surfaces.
- OpenCode configuration and routed-flow validation, plus restricted Codebase Memory MCP import/launcher support.
- Local Smart Learning project analysis and managed document transactions.
- Visual Compression shadow preview and metadata-only observability. Production Active mutation remains disabled.
- Release metadata generation for CycloneDX SBOM, dependency license inventory, security scan summary, SHA-256 manifests, release notes, and build information.
- Reusable unsigned-preview candidate workflows for macOS ARM64 and Windows x64, plus one aggregate tagged workflow that creates a GitHub public prerelease only after both platform jobs succeed in the same run.

### Changed

- Unified JavaScript, Tauri, desktop crate, and all Rust workspace crate versions at `0.2.0-beta.1`.
- Moved Rust crate version ownership to `[workspace.package]` to prevent version drift.
- Clarified that the desktop installer does not bundle `headroom-0.31.0/`, local `.codex/` state, test artifacts, or the large `codebase-memory-mcp` upstream executable.
- Added stable project and third-party legal resources to desktop bundles.
- Updated README product status, preview target coverage, capability boundaries, safe configuration workflow, Beta limitations, and release evidence requirements.

### Security

- The public unsigned prerelease workflows never report Developer ID, notarization, Gatekeeper, Authenticode, or production-signing readiness; mandatory functional, packaging, provenance, SBOM, license, security, and SHA-256 gates remain fail closed.
- Code Graph upstream import remains pinned by platform, architecture, version, and SHA-256.
- Windows preview artifacts are publishable only after a real Windows runner completes installation, startup, package audit and uninstall validation; they remain explicitly unsigned.

### Known limitations

- The current local macOS candidate is an unsigned/ad-hoc technical preview and may trigger a macOS security warning; that is not equivalent to a Developer ID signed and notarized public release.
- macOS ARM64 and Windows x64 are unsigned preview targets. A future production release still requires Developer ID/notarization and Authenticode evidence.
- Visual Compression Active is unavailable in production; only shadow/preview behavior is exposed.
- OpenCode has the strongest real integration evidence. Claude Code and Codex still require same-candidate real configuration and routed-flow acceptance before equivalent claims; incomplete Headroom-derived capabilities remain metadata-only, preview-only, passthrough, partial, blocked, or unavailable.
- Doctor can execute only supported, evidence-backed repair paths; unsupported repairs remain locked or fail closed.
- Code Graph requires explicit import of the pinned upstream component and does not bundle its executable in the installer.

[Unreleased]: https://github.com/37chengshan/d-token/compare/v0.2.0-beta.1...HEAD
[0.0.1]: https://github.com/37chengshan/d-token/compare/v0.2.0-beta.1...v0.0.1
[0.2.0-beta.1]: https://github.com/37chengshan/d-token/releases/tag/v0.2.0-beta.1
