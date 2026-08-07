<p align="center">
  <img src="assets/marketing/hero-banner.png" width="100%" alt="d-token — Less context. Same intent. Full control." />
</p>

<div align="center">
  <img src="assets/marketing/app-icon.png" width="112" alt="d-token logo" />

# d-token

**Less context. Same intent. Full control.**

A local context control plane for AI coding agents. See where every token goes, keep routing explicit, compress what's redundant, and roll back anything that breaks.

English · [简体中文](README-cn.md)

[![CI](https://github.com/37chengshan/d-token/actions/workflows/ci.yml/badge.svg)](https://github.com/37chengshan/d-token/actions/workflows/ci.yml)
![Version](https://img.shields.io/badge/version-0.2.0--beta.1-ea580c)
![Status](https://img.shields.io/badge/status-beta-ea580c)
![Desktop](https://img.shields.io/badge/desktop-Tauri%202-24c8db?logo=tauri&logoColor=white)
![Core](https://img.shields.io/badge/core-Rust-b7410e?logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-Apache--2.0-2563eb)
</div>

---

> **Public unsigned prerelease, not a signed installer release.** `0.2.0-beta.1` is published as a GitHub public prerelease for **macOS ARM64** and **Windows x64** only after the tagged aggregate workflow succeeds on both platforms in the same run with matching SHA-256, SBOM, license, security, and provenance evidence. It is still unsigned: macOS is not Developer ID signed or notarized, Windows is not Authenticode-signed. An unsigned macOS build may show a security warning — that is expected. Verify the hashes, and never bypass security controls for packages from untrusted sources.

## Your agent talks a lot. It spends even more.

AI coding agents re-send the same files, logs, and tool output over and over. The token bill grows, but nobody can say exactly where it went, which provider handled it, or whether the "optimization" actually helped.

d-token sits locally between your agent and the provider you chose, and does four things:

1. **Connect** — discover and wire up your coding agents from one desktop app.
2. **Optimize** — compress redundant context before it leaves your machine.
3. **Route** — every request takes an explicit, explainable path to your configured upstream.
4. **Observe** — each request leaves a receipt: source agent, route, tokens, transformations, and recovery status.

Same workflow. Same tools. Fewer tokens. More visibility. And when a config change goes wrong, you get a way back instead of a mystery.

## Proof, not promises

A real `OpenCode → d-token → upstream` run on 2026-08-04:

| Signal | Value | Source |
|---|---|---|
| Tokens physically reduced on a single request | **33,705** | real routed flow (claimable = 0) |
| Routed requests persisted | 89 | runtime summary |
| Total physical token reduction | **279,270 (~4.0%)** | runtime summary |
| Verified configure & rollback path | OpenCode | same-candidate acceptance |

Measured numbers come from evidence. Estimated numbers are labeled as estimates. Full evidence and release gates live in [`docs/product/IMPLEMENTATION_STATUS.md`](docs/product/IMPLEMENTATION_STATUS.md).

## Who it's for

| You are… | d-token gives you… |
|---|---|
| A **heavy token user** burning context every session | One place to see what requests cost, what got compressed, and what actually helped |
| Running **cc-switch or multiple providers** | Explicit, observable routing — never a silent provider swap behind your back |
| **Cautious about touching config** | Every write with a preview, a redacted diff, a backup, a confirmation, and a verified rollback path |
| **New to agent configuration** | Agent auto-discovery and a guided connect flow instead of hand-editing config files |
| On **Windows or macOS** | One shared desktop app with the same information architecture on both platforms |

## Product tour

### 1. A local-first control plane

Inspect runtime state, supported Agent connections, route evidence, token measurements, and diagnostics in one desktop control plane. Availability is evidence-based: unavailable or partial surfaces stay labeled instead of being filled with example success data.

<p align="center">
  <img src="assets/marketing/01-local-first-control-plane.svg" width="100%" alt="d-token local-first control plane" />
</p>

### 2. Explicit routing, no silent switching

d-token records the selected route and upstream for supported flows. If a route is unavailable, it stays blocked or asks you — it never silently falls back to another provider.

<p align="center">
  <img src="assets/marketing/02-explicit-routing-pipeline.svg" width="100%" alt="d-token explicit routing pipeline" />
</p>

### 3. Observability and code intelligence, with explicit limits

Review redacted Event Ledger evidence, analytics, diagnostics, and supported code-intelligence state. Code Graph is experimental and needs an explicit verified component import. Visual Compression is preview-first; production Active transformation stays disabled.

<p align="center">
  <img src="assets/marketing/03-observability-and-code-intelligence.svg" width="100%" alt="d-token observability and code intelligence" />
</p>

> Estimated savings are guidance, not a billing statement. Preview-only, disabled, blocked, and unavailable capabilities are not production-ready features.

### 4. Every request leaves a receipt

Each routed request produces an Optimization Receipt: source agent, route, transformations, protected content, Local vs Provider tokens, recovery status, and skip-or-failure reasons. Measured and estimated values are never mixed.

<p align="center">
  <img src="assets/marketing/06-optimization-receipt.svg" width="100%" alt="d-token optimization receipt" />
</p>

### 5. Configure safely: preview, back up, verify, roll back

Supported configuration actions present a user-readable summary and redacted diff, create a backup, require confirmation, verify the result, and keep rollback evidence. Unsupported operations stay unavailable rather than writing guessed configuration.

<p align="center">
  <img src="assets/marketing/04-safe-config-and-rollback.svg" width="100%" alt="d-token safe configuration and rollback" />
</p>

> OpenCode currently has the strongest verified configure and rollback path. Other Agent and platform paths may be partial, disabled, or unavailable.

## What d-token refuses to do

The boundaries are as deliberate as the features:

- **No silent provider switching.** The route is always explicit, explainable, and under your control.
- **No fabricated savings.** Estimates are labeled; measured numbers come from evidence.
- **No guessed configuration writes.** If a change can't be previewed, backed up, and rolled back, it isn't made.
- **No example data posing as real status.** Unavailable, partial, and unverified surfaces are labeled as such.
- **No secrets in ordinary logs.** API keys, tokens, and full prompts/responses stay out of logs, exports, and commits.

## Getting started

### Run the desktop app

```bash
git clone https://github.com/37chengshan/d-token.git
cd d-token/code/apps/desktop
npm ci
npm run tauri:dev
```

`npm run tauri:dev` prepares the local runtime sidecar before starting the desktop app. Running the frontend by itself only previews the interface and cannot connect to local agents or services.

### Requirements

- Node.js 24 and npm
- Rust stable toolchain
- macOS ARM64 or Windows x64 desktop development environment
- Xcode Command Line Tools on macOS
- Tauri and WebView2 prerequisites on Windows

### Build a local candidate

```bash
cd code/apps/desktop
npm run tauri:build
```

A successful local build is not the same as an officially signed release.

### Current availability

`0.2.0-beta.1` is a **public unsigned prerelease** only when attached to the matching GitHub prerelease tag after the same-run macOS ARM64 and Windows x64 aggregate workflow succeeds. It is not a signed or notarized production installer. Preview targets are macOS ARM64 and Windows x64; production signing, notarization, and broader platform acceptance remain release blockers.

## Roadmap

| Version | Focus | Status |
|---|---|---|
| `0.2.0-beta.1` | Trusted connect & trusted compression: agent discovery, cc-switch scanning, preview → diff → backup → apply → verify → rollback, Optimization Receipts, Doctor | current candidate |
| `v0.3` | Context continuity: cross-agent Context Relay, Session Cache, Code Map/Signatures reuse, Recovery Pointer, cc-switch follow / lock / observe modes | planned |
| `v0.4` | Automated local control plane: multi-agent adaptation, low-risk auto-repair, project & team policies, reproducible real-task evidence, stable signing, auto-update | planned |

Context Relay (v0.3) is planned, not shipped — the current candidate does not yet reuse context across agents.

<p align="center">
  <img src="assets/marketing/05-context-relay.svg" width="100%" alt="d-token context relay across agents (planned for v0.3)" />
</p>

## Honest boundaries

- This is a public unsigned prerelease — not Developer ID signed, not notarized, not Authenticode-signed.
- OpenCode has the most complete verified connection and rollback flow. Claude Code and Codex integration paths exist, but same-candidate real-user acceptance is still in progress.
- Visual Compression is shadow-preview only; production Active transformation stays disabled. Code Graph is experimental and does not bundle its large upstream component.
- Some Headroom-derived and Agent-facing surfaces are metadata-only, preview-only, static compatibility declarations, partial, blocked, or unavailable. That is not "Headroom is done."
- Token reduction may be measured or estimated depending on available request evidence; estimates are not provider billing data.
- Supported configuration writes follow preview → redacted diff → backup → confirmation → apply → verify → rollback. Where that transaction is unavailable, d-token does not write configuration automatically.

Latest implementation facts and release blockers: [`docs/product/IMPLEMENTATION_STATUS.md`](docs/product/IMPLEMENTATION_STATUS.md).

## Privacy and local data

d-token keeps its settings, diagnostics, request metadata, and rollback records on your machine. It is designed not to store complete prompts, responses, source files, API keys, or authorization headers in ordinary logs.

Your selected model provider still receives the requests you choose to send through it. d-token does not make a third-party provider local or private by itself.

## Technology

Tauri 2 · React 18 · TypeScript · Vite · Rust · SQLite

## Development and documentation

- [Product contract](docs/PRD.md)
- [Product direction (v2.1)](docs/d-token_AI_product_target_spec_v2.md)
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
  Less context. Same intent. Full control.
</div>
