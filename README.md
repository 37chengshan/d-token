<div align="center">
  <img src="icons/logo.svg" width="112" alt="d-token logo" />

# d-token

**Save tokens, understand every request, and keep your AI coding workflow under control.**

A local desktop app for connecting AI coding agents, tracking token usage, previewing optimization strategies, and safely managing changes from one place.

English · [简体中文](README-cn.md)

[![CI](https://github.com/37chengshan/d-token/actions/workflows/ci.yml/badge.svg)](https://github.com/37chengshan/d-token/actions/workflows/ci.yml)
![Version](https://img.shields.io/badge/version-0.2.0--beta.1-ea580c)
![Status](https://img.shields.io/badge/status-beta-ea580c)
![Desktop](https://img.shields.io/badge/desktop-Tauri%202-24c8db?logo=tauri&logoColor=white)
![Core](https://img.shields.io/badge/core-Rust-b7410e?logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-Apache--2.0-2563eb)
</div>

---

> **Public unsigned prerelease, not a signed installer release.** `0.2.0-beta.1` may be published as a GitHub public prerelease for **macOS ARM64** and **Windows x64** only after the tagged aggregate release workflow succeeds for both platforms in the same run and uploads the matching SHA-256, SBOM, license, security, and provenance evidence. That public prerelease is still unsigned: macOS assets are not Developer ID signed or notarized, and Windows assets are not Authenticode-signed. An unsigned or ad-hoc macOS build may trigger an operating-system security warning. Treat that warning as expected, verify the repository and supplied hashes, and do not bypass security controls for packages from untrusted sources.

## Overview

AI coding tools can consume large amounts of context without making it clear where those tokens went, which route handled the request, or whether an optimization actually helped.

d-token brings those daily tasks into one desktop app. Connect supported agents, route requests through your chosen provider, review token reduction and errors, preview compression before applying it, and recover configuration changes when something goes wrong.

The goal is simple: **use fewer tokens without turning your workflow into a collection of scripts, config files, and guesswork.**

## Core capabilities

| Capability | What it gives you |
|---|---|
| **Token savings you can see** | Review processed requests, estimated or measured token reduction, compression ratios, trends, and errors in one place. |
| **One place for AI coding agents** | Detect and connect supported agents through a consistent desktop workflow instead of editing each configuration by hand. |
| **Clear request routing** | See which route and upstream handled a request, then adjust the path without silently switching providers. |
| **Preview before changing anything** | Use Shadow previews to inspect compression strategies and expected impact before enabling a configuration change. |
| **Codebase understanding with less context** | Build a project index and code graph so agents can focus on relevant files and relationships instead of repeatedly receiving unnecessary context. |
| **Safer local changes** | Preview differences, create backups, verify the result, and roll back supported configuration changes from the app. |
| **Local diagnostics** | Check service state, ports, agent connections, configuration health, and problems that need attention. |
| **Comfortable daily use** | Switch between light, dark, and system themes, and choose separate colors for the interface and analytics. |

## Product tour

### 1. Local-first control plane for AI coding

Use one local desktop control plane to inspect local runtime state, supported Agent connections, route evidence, token measurements, and diagnostics. Availability is evidence-based: unavailable or partial surfaces stay labeled instead of being filled with example success data.

<p align="center">
  <img src="assets/marketing/01-local-first-control-plane.svg" width="100%" alt="d-token local-first control plane" />
</p>

### 2. Explicit routing, without silent provider switching

d-token records the selected route and upstream for supported flows. It does not silently fall back to another provider when a route is unavailable; incomplete integrations remain blocked or require explicit user action.

<p align="center">
  <img src="assets/marketing/02-explicit-routing-pipeline.svg" width="100%" alt="d-token explicit routing pipeline" />
</p>

### 3. Observability and code intelligence with explicit limits

Review redacted Event Ledger evidence, analytics, diagnostics, and supported code-intelligence state. Code Graph is experimental and needs an explicit verified component import. Visual Compression is preview-first; its production Active transformation remains disabled.

<p align="center">
  <img src="assets/marketing/03-observability-and-code-intelligence.svg" width="100%" alt="d-token observability and code intelligence" />
</p>

> Estimated savings are guidance, not a billing statement. Metadata-only, preview-only, disabled, blocked, and unavailable capabilities are not equivalents of production-ready functionality.

### 4. Configure safely: review, back up, verify, and roll back

For supported configuration actions, d-token presents a user-readable summary and redacted diff, creates a backup, requires confirmation, verifies the result, and retains rollback evidence. Unsupported Agent, repair, route, or transform operations remain unavailable rather than writing guessed configuration.

<p align="center">
  <img src="assets/marketing/04-safe-config-and-rollback.svg" width="100%" alt="d-token safe configuration and rollback" />
</p>

> OpenCode currently has the strongest verified configure and rollback path. Other Agent and platform paths may be partial, disabled, or unavailable.

## How it fits into your workflow

```text
Your AI coding agent
        ↓
      d-token
  connect · optimize · route · observe
        ↓
Your configured model provider
```

You continue using your preferred coding agent. d-token runs locally between the agent and the provider you configured, then presents the request history, token data, route state, diagnostics, and supported recovery actions in the desktop app.

## Getting started

### Current availability

`0.2.0-beta.1` is a **public unsigned prerelease** only when it is attached to the matching GitHub prerelease tag after the same-run macOS ARM64 and Windows x64 aggregate workflow succeeds. It is still not a signed or notarized production installer release. Do not treat chat attachments, unofficial mirrors, or arbitrary unsigned local builds as official prerelease assets.

Preview target coverage is macOS ARM64 and Windows x64. macOS assets may show a security warning because the prerelease is unsigned or ad-hoc signed. Windows assets remain unsigned and do not claim Authenticode or SmartScreen reputation. Production signing, notarization, and broader platform acceptance remain release blockers.

### Requirements

- Node.js 24 and npm
- Rust stable toolchain
- macOS ARM64 or Windows x64 desktop development environment
- Xcode Command Line Tools on macOS
- Tauri and WebView2 prerequisites on Windows

### Run the desktop app

```bash
git clone https://github.com/37chengshan/d-token.git
cd d-token/code/apps/desktop
npm ci
npm run tauri:dev
```

`npm run tauri:dev` prepares the local runtime sidecar before starting the desktop app. Running the frontend by itself only previews the interface and cannot connect to local agents or services.

### Build a local candidate

```bash
cd code/apps/desktop
npm run tauri:build
```

A successful local build is not the same as an officially signed release.

## Beta notes

- This is a public unsigned prerelease, not a signed production release. It is not Developer ID signed, notarized, Gatekeeper-approved, or Authenticode-signed.
- OpenCode currently has the most complete verified connection and rollback flow. Claude Code and Codex integration paths exist, but full same-candidate real-user acceptance is still in progress.
- Visual Compression focuses on Shadow previews; production Active transformation remains disabled. Code Graph is experimental and does not bundle its large upstream component.
- Some Headroom-derived and Agent-facing surfaces are metadata-only, preview-only, static compatibility declarations, partial, blocked, or unavailable. They must not be read as full Headroom completion.
- Token reduction can be measured or estimated depending on available request evidence. Estimates are not provider billing data.
- Supported configuration writes follow preview → redacted diff → backup → confirmation → apply → verify → rollback. Where that transaction is not available, d-token should not write configuration automatically.

The latest implementation facts and release blockers are maintained in [`docs/product/IMPLEMENTATION_STATUS.md`](docs/product/IMPLEMENTATION_STATUS.md).

## Privacy and local data

d-token keeps its own settings, diagnostics, request metadata, and rollback records on your machine. It is designed not to store complete prompts, responses, source files, API keys, or authorization headers in ordinary logs.

Your selected model provider still receives the requests you choose to send through it. d-token does not make a third-party provider local or private by itself.

## Technology

Tauri 2 · React 18 · TypeScript · Vite · Rust · SQLite

## Development and documentation

- [Product contract](docs/PRD.md)
- [Current implementation status](docs/product/IMPLEMENTATION_STATUS.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Security boundaries](docs/SECURITY_BOUNDARIES.md)
- [Changelog](CHANGELOG.md)
- [Security policy](SECURITY.md)
- [Third-party notices](THIRD_PARTY_NOTICES.md)

Contributions, bug reports, and product feedback are welcome through GitHub Issues and Pull Requests. Please read [`AGENTS.md`](AGENTS.md) before making repository-wide changes.

## License

Licensed under the [Apache License 2.0](LICENSE). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for bundled and adapted third-party components.

---

<div align="center">
  <strong>d-token</strong><br />
  Save tokens. See what changed. Stay in control.
</div>
