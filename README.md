# d-token

> Local-first token compression, routing, and observability control plane for AI coding workflows.

`d-token` is a desktop-oriented local control plane for AI coding tools. It combines command-output preprocessing, Headroom-compatible context compression, provider routing, project-level compression policy, and end-to-end token savings observability in one Windows/macOS app.

The project is currently in early M1 development. The repository already contains the living PRD, architecture documents, research reports, Rust workspace skeleton, and local API/proxy skeleton. Production installers and full RTK/Headroom integrations are planned but not complete yet.

## Why this exists

AI coding agents waste a large amount of context on noisy command output, long logs, repeated tool results, oversized file reads, and unstructured provider routing. Existing tools solve parts of the problem at different layers:

- `rtk-ai/rtk` works at the command execution layer: it rewrites shell commands and filters command output before it enters the agent context.
- `headroomlabs-ai/headroom` works at the LLM context layer: it compresses tool outputs, logs, files, RAG chunks, and conversation context before the request reaches the model.
- `d-token` is the local desktop control plane around those layers: installation, configuration, policy, routing, process control, diagnostics, metrics, rollback, and cross-layer attribution.

## Core idea

```text
AI Coding Tool
  ↓
Shell Command Execution
  ↓
RTK CLI Proxy / Hook / Plugin
  ↓
Cleaned command output enters agent context
  ↓
d-token Local Proxy
  ↓
Headroom-compatible Compression Core
  ↓
Route Decision
  ↓
Official Provider / Custom Provider / Chain Bridge
  ↓
Provider
```

`d-token` treats RTK and Headroom as complementary layers rather than substitutes:

```text
RTK       = Command Execution Layer / Shell Output Layer
Headroom  = API Gateway Layer / LLM Context Layer
d-token   = Desktop Orchestration / Routing / Policy / Observability Layer
```

## Planned capabilities

### 1. Desktop control plane

- Windows/macOS desktop app based on Tauri 2 + React + TypeScript.
- One-click setup wizard for d-token core, RTK, Headroom-compatible runtime, MCP, hooks, and provider configs.
- Local process manager for proxy/runtime/sidecar instances.
- Doctor page for environment, process, route, provider, RTK, Headroom, config, and rollback checks.

### 2. Layered token compression

- RTK command-output savings.
- Headroom-compatible context compression savings.
- End-to-end savings attribution without double-counting.
- Per-agent, per-project, per-route, per-provider, and per-instance breakdowns.

### 3. Provider routing

- Official providers.
- Custom OpenAI-compatible providers.
- Custom Anthropic-compatible providers.
- Local chain bridge mode.
- Explicit, auditable route decisions.
- No silent provider fallback by default.

### 4. Project-level compression policy

- `ProjectProfile`.
- `ProjectCompressionPolicy`.
- Recommended compression level.
- Manual override.
- Policy diff and rollback.
- Quality-risk warnings for aggressive/max compression.

### 5. Safe configuration patching

- Preview diff before any config or hook mutation.
- Backup before apply.
- Atomic install transaction where possible.
- Rollback points.
- Hash mismatch and conflict handling.
- No silent telemetry enablement.

### 6. Local-first security model

- Default bind address: `127.0.0.1`.
- API keys stay local.
- Prefer OS keychain for secrets.
- Logs are redacted by default.
- No upload of user code, prompts, RTK DB, tee files, command output, or Headroom CCR originals.
- CCR and raw-output recovery are summarized by default; full content requires explicit user action.

## Repository layout

```text
README.md                         Project overview
AGENTS.md                         Agent-facing project instructions
CLAUDE.md                         Claude-facing project instructions
docs/                             Formal docs, PRD, architecture, reviews, research reports
code/                             Application code, Rust crates, desktop app skeleton, runtime placeholders
support/                          Scripts, tests, CI helpers, repository checks
```

Key documents:

```text
docs/PRD.md                                           Single living PRD
docs/ARCHITECTURE.md                                  Runtime architecture
docs/TECH_STACK.md                                    Technology decisions
docs/SECURITY_BOUNDARIES.md                           Safety and privacy boundaries
docs/research/REPORT_01_RTK_HEADROOM_INTERNALS.md     RTK + Headroom internals
docs/research/REPORT_02_DESKTOP_INTEGRATION_ONE_CLICK_APP.md
docs/research/REPORT_03_FEATURES_AND_FIXES_IMPLEMENTATION.md
docs/reviews/PRD_MULTIDIMENSIONAL_GAP_REVIEW_2026-07-01.md
docs/reviews/RESEARCH_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-01.md
```

## Current implementation status

Current M1 skeleton includes:

```text
code/Cargo.toml                         Rust workspace
code/apps/desktop                       Tauri 2 + React/TypeScript desktop placeholder
code/crates/dtoken-types                Shared domain models
code/crates/dtoken-config               Local config loader
code/crates/dtoken-router               Provider and route decision layer
code/crates/dtoken-compress             Compression interface
code/crates/dtoken-core-api             Local API used by the desktop app
code/crates/dtoken-process              Local process manager
code/crates/dtoken-doctor               Doctor checks
code/crates/dtoken-proxy                Local proxy / core API entrypoint
code/packages/sdk                       TypeScript SDK / shared types placeholder
code/runtimes/headroom                  Headroom-compatible runtime placeholder
code/configs/config.example.json        Safe example config without real secrets
```

Planned near-term crates:

```text
dtoken-preprocessor          RTK adapter + built-in fallback preprocessor
dtoken-headroom-adapter      Headroom CLI/proxy/MCP/status adapter
dtoken-project-policy        Project-level compression policy and recommendation
dtoken-config-patcher        Diff / backup / rollback for config changes
dtoken-event-ledger          Unified event ledger for attribution and audit
dtoken-compatibility         Compatibility matrix for OS / shell / agent / mode combinations
```

## Development checks

From the repository root:

```bash
python support/scripts/check_structure.py
```

From the code workspace:

```bash
cd code
cargo check --workspace
cargo test --workspace
```

These checks are part of the minimum CI gate planned in the PRD.

## Roadmap

```text
M0  Documentation and repository structure convergence
M1  Core skeleton + RTK/Headroom adapter skeleton
M2  Desktop dashboard skeleton + installer wizard
M3  Provider / route / instance / RTK / Headroom integration
M4  Headroom-compatible parity + layered compression attribution
M5  Production hardening: installers, signing, updater, CI, compatibility tests
```

## Design rules

1. `docs/PRD.md` is the only PRD.
2. Do not create parallel PRD versions.
3. Do not silently modify agent, RTK, Headroom, or shell configs.
4. Do not silently enable telemetry.
5. Do not claim RTK-only, Headroom-only, or built-in preprocessing are equivalent.
6. Do not directly add RTK savings and Headroom savings without a unified baseline.
7. Do not store full command output, full prompt, full source files, or full API responses in default logs.
8. Prefer read-only detection before mutation.
9. Prefer external RTK/Headroom integration first; managed mode must be explicit and reversible.
10. Every user-visible route/fallback/compression decision must be explainable.

## License

License status is not finalized in this repository. If Apache-2.0 upstream code is reused, license notices must be preserved. Until licensing is finalized, treat the repository as a planning and early implementation workspace.