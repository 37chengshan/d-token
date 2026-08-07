<p align="center">
  <img src="assets/marketing/hero-banner.png" width="100%" alt="d-token — Less context. Same intent. Full control." />
</p>

<div align="center">
  <img src="assets/marketing/app-icon.png" width="112" alt="d-token logo" />

# d-token

A local context control plane for AI coding agents.

English · [简体中文](README-cn.md)

[![CI](https://github.com/37chengshan/d-token/actions/workflows/ci.yml/badge.svg)](https://github.com/37chengshan/d-token/actions/workflows/ci.yml)
![Version](https://img.shields.io/badge/version-0.2.0--beta.1-ea580c)
![Status](https://img.shields.io/badge/status-beta-ea580c)
![Desktop](https://img.shields.io/badge/desktop-Tauri%202-24c8db?logo=tauri&logoColor=white)
![Core](https://img.shields.io/badge/core-Rust-b7410e?logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-Apache--2.0-2563eb)
</div>

---

## Highlights

- 🧠 **Save what you can see** — a single real routed request reduced **33,705 tokens** (measured, 2026-08-04)
- 🛣️ **Explicit routing** — every request takes a clear path to your configured upstream; no silent provider swaps
- 🧾 **A receipt for every request** — source agent, route, transformations, Local vs Provider tokens, recovery status
- 🔒 **Safe configuration** — preview → redacted diff → backup → verify → roll back
- 🏠 **Local-first** — runs on your machine; secrets never enter ordinary logs
- 🩺 **Doctor for your setup** — diagnose, repair, and recover when something breaks

## What it is

d-token sits between your AI coding agent and the model provider you chose. It sees every request, compresses redundant context before it leaves your machine, keeps the route explicit, and makes configuration changes recoverable — instead of a pile of scripts and guesswork.

```
Your AI coding agent
        ↓
      d-token
  connect · optimize · route · observe
        ↓
Your configured model provider
```

## Quick start

```bash
git clone https://github.com/37chengshan/d-token.git
cd d-token/code/apps/desktop
npm ci
npm run tauri:dev
```

> [!NOTE]
> `0.2.0-beta.1` is an **unsigned public prerelease** for macOS ARM64 and Windows x64 — not a signed installer. An unsigned macOS build may trigger a system security warning; verify the source and supplied hashes, and don't bypass OS security controls. Details in [SECURITY.md](SECURITY.md).

## Why it exists

AI coding agents re-send the same files, logs, and tool output over and over. The token bill grows, but nobody can say exactly where it went, which provider handled it, or whether an "optimization" actually helped. d-token makes those three questions answerable.

## Roadmap

| Version | Focus | Status |
|---|---|---|
| `0.2.0-beta.1` | Trusted connect & trusted compression, Optimization Receipts, Doctor | current candidate |
| `v0.3` | Context continuity: cross-agent Context Relay, Session Cache, cc-switch follow / lock / observe | planned |
| `v0.4` | Automated local control plane: multi-agent adaptation, low-risk auto-repair, stable signing | planned |

## Privacy

d-token keeps settings, diagnostics, request metadata, and rollback records on your machine. It is designed not to store complete prompts, responses, source files, API keys, or authorization headers in ordinary logs. Your provider still receives the requests you choose to send through it.

## Documentation

- [Product contract](docs/PRD.md)
- [Product direction (v2.1)](docs/d-token_AI_product_target_spec_v2.md)
- [Implementation status & release gates](docs/product/IMPLEMENTATION_STATUS.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Security boundaries](docs/SECURITY_BOUNDARIES.md)
- [Changelog](CHANGELOG.md)

## Contributing

Bug reports and product feedback are welcome through GitHub Issues and Pull Requests. Please read [`AGENTS.md`](AGENTS.md) before making repository-wide changes.

## License

[Apache License 2.0](LICENSE). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for bundled and adapted third-party components.
