# d-token App Functions And Pages

Status: Draft for app buildout  
Source of truth: `docs/PRD.md`

## 1. Product Positioning

d-token desktop is a local control plane for AI coding compression, routing, installation, diagnosis, and observability.

In plain terms: it should let a user see "which agent is using which route, how much token was saved, whether RTK/Headroom are healthy, what config will be changed, and how to roll back safely."

The app must support two very different situations:

1. **Developer/test mode**: fixture-first, no-touch, never modifies the user's real RTK, Headroom, agent config, shell profile, or running services.
2. **Production mode**: managed install/configuration is allowed, but only through explicit scan, diff, backup, apply, verify, doctor, and rollback steps.

## 2. Primary Users

### 2.1 Normal AI Coding User

Wants one button to set up local token savings and routing without understanding RTK, Headroom, hooks, proxies, or route files.

Needs:

1. Clear setup wizard.
2. Safe defaults.
3. Visible status.
4. Easy rollback.
5. Error messages that explain what to do next.

### 2.2 Power User

Already has RTK, Headroom, multiple agents, custom providers, or local chains.

Needs:

1. External read-only connect mode.
2. Per-agent compatibility status.
3. Provider and route control.
4. Logs, metrics, exports.
5. Precise config diff before any write.

## 3. Internal Build/Test Context

Developer/test mode is not a separate product target group. It is an internal safety context used while building d-token.

The app can expose this as an advanced or demo mode, but it should not shape the main user journey.

Internal build/test needs:

1. Fixture demo mode.
2. Fake binary mode.
3. Shadow detect mode behind explicit action.
4. No-touch Doctor.
5. CI/smoke parity with local UI.

## 4. App Navigation

Recommended first-level navigation:

1. Overview Dashboard
2. RTK Dashboard
3. Headroom Dashboard
4. Setup
5. Agents
6. Projects
7. Routes
8. Providers
9. Doctor
10. Logs
11. Changes
12. Backup & Rollback
13. Settings

For M3, the visible navigation can be smaller, but the information architecture should already reserve these destinations so the app does not need a redesign in M4.

## 5. Page Specifications

### 5.1 Overview Dashboard

Purpose: total daily home screen for the whole d-token system.

Shows:

1. Current runtime mode: Fixture, Fake Binary, Shadow, External, Managed.
2. Overall health: proxy, RTK, Headroom, agents, routes, providers.
3. Total layered savings: RTK savings, Headroom savings, end-to-end estimate.
4. Recent errors and warnings.
5. Active project and active agent.
6. Recent route decisions.
7. RTK summary with jump to RTK Dashboard.
8. Headroom summary with jump to Headroom Dashboard.
9. Quick actions: run Doctor, open Setup, view logs, export report.

M3:

1. Static fixture data.
2. No-touch safety banner.
3. Agent coverage summary.
4. Protocol injection summary.
5. Separate entry cards for RTK and Headroom.

M4/M5:

1. Live local API data.
2. Real status cards.
3. Interactive filters by project, agent, route, provider, and instance.

### 5.2 RTK Dashboard

Purpose: dedicated dashboard for command-output savings and RTK integration.

Shows:

1. RTK mode: fixture, fake, shadow, external, managed, or builtin fallback.
2. RTK binary/version status.
3. Agent hook/plugin/rules status.
4. Command-output savings over time.
5. Savings by command family: test, build, git, search, package manager, logs.
6. `rtk gain` summary.
7. `rtk discover` missed opportunities.
8. `rtk session` adoption statistics.
9. Raw-output tee recovery status.
10. Telemetry opt-in state.
11. Hook integrity and permission precedence.
12. Windows native versus WSL support notes.

M3:

1. Fixture/fake RTK cards.
2. No real RTK command by default.

M4/M5:

1. External read-only import.
2. Managed install/update/uninstall.
3. Hook integrity repair flow.

### 5.3 Headroom Dashboard

Purpose: dedicated dashboard for Headroom-compatible context compression and memory/retrieval features.

Shows:

1. Headroom mode: fixture, shadow, external, or managed.
2. Proxy/SDK/MCP status.
3. Health, stats, history, and metrics.
4. Context compression savings over time.
5. Savings by project, agent, model, and compression policy.
6. Current and recommended compression level.
7. CCR status and summary, without raw CCR content by default.
8. Memory, learn, output shaper, cache aligner, code compressor, JSON SmartCrusher, and text Kompress status.
9. Cost tracking and export controls.
10. Errors and fallback events.

M3:

1. Fixture Headroom cards.
2. No real Headroom probing by default.

M4/M5:

1. External read-only stats import.
2. Managed lifecycle and policy control.

### 5.4 Setup

Purpose: guide the user into the right mode.

Modes:

1. Fixture Demo: safe demo, no real tools touched.
2. Connect Existing: read-only connection to installed RTK/Headroom.
3. Managed Install: install or configure RTK/Headroom/d-token with backup and rollback.
4. Repair Existing Setup: re-run Doctor and propose config fixes.

Key flow:

1. Detect platform.
2. Ask whether the user already has RTK/Headroom.
3. Show a plan.
4. Show exact diff before writes.
5. Backup.
6. Apply.
7. Verify.
8. Save rollback point.

M3:

1. Demo cards only.
2. Shadow/external actions disabled or clearly marked as opt-in.

M4:

1. Real managed installer transaction.
2. Backup/rollback integration.

### 5.5 Agents

Purpose: show which AI coding tools are supported and how they connect.

Shows one row per agent:

1. Agent name.
2. RTK support status.
3. RTK integration method: hook, plugin, rules, instructions, planned.
4. Headroom support status.
5. Headroom integration method: wrap, manual proxy, library only, memory/learn only.
6. macOS support level.
7. Windows native support level.
8. Windows WSL support level.
9. Known limitations.
10. Setup action or docs action.

Must include:

1. Claude Code
2. VS Code Copilot Chat
3. GitHub Copilot CLI
4. Cursor
5. Gemini CLI
6. OpenCode
7. OpenClaw
8. Pi
9. Hermes
10. Cline / Roo Code
11. Windsurf
12. Codex CLI
13. Kilo Code
14. Google Antigravity
15. Mistral Vibe
16. Aider
17. Continue
18. Goose
19. OpenHands
20. Cortex Code

M3:

1. Compatibility matrix from fixtures.
2. No writes.

M4/M5:

1. Agent-specific setup and repair flows.
2. Hook integrity verification.

### 5.6 Projects

Purpose: manage project-level compression policy.

Shows:

1. Project root.
2. Detected language stack.
3. Active agents.
4. Default route.
5. RTK profile.
6. Headroom policy.
7. Recommended compression level.
8. User override.
9. Feature toggles: CCR, memory, learn, MCP, output shaper, cache aligner, code compressor, JSON SmartCrusher, text Kompress.
10. Holdout/eval ratio when enabled.

Core decision: each project should have a visible reason for the recommended compression level.

M3:

1. Fixture project policy card.
2. Read-only UI.

M4/M5:

1. Real project policy editing.
2. Diff-backed config writes.

### 5.7 Routes

Purpose: make routing explicit and auditable.

Shows:

1. Route id and name.
2. Input protocol: OpenAI-compatible or Anthropic-compatible.
3. Bound host and port.
4. Instance.
5. Target provider.
6. Chain mode: direct or chain bridge.
7. Compression policy.
8. Fallback provider, if explicitly configured.
9. Recent route decisions.
10. Errors by route.

Rules:

1. No silent provider guessing.
2. No silent fallback.
3. Every decision must be explainable.
4. Header stripping and protocol injection must be visible.

M3:

1. Protocol injection plan only.

M4/M5:

1. Route editor.
2. Managed config patching.
3. Live route decision log.

### 5.8 Providers

Purpose: configure official, compatible, custom, and local-chain providers.

Shows:

1. Provider kind.
2. Protocol.
3. Base URL.
4. Models.
5. Timeout.
6. Healthcheck status.
7. Credential reference, never the raw key.
8. Last error.

Credential rule: the app must never display raw API keys, Authorization headers, cookies, refresh tokens, session tokens, or auth file contents.

M3:

1. Fixture provider status.

M4/M5:

1. OS keychain integration.
2. Provider healthcheck.
3. Redacted config diff.

### 5.9 RTK

Purpose: manage RTK configuration and lifecycle details.

Shows:

1. RTK install method: fixture, fake, external, managed, builtin fallback.
2. Version.
3. Hook/integration status.
4. `rtk gain` summary.
5. `rtk discover` opportunities.
6. `rtk session` adoption stats.
7. Telemetry opt-in state.
8. Raw-output tee recovery status.
9. Permission precedence summary: Deny > Ask > Allow > Default.
10. Windows native and WSL differences.

This page is more detailed and action-oriented than the RTK Dashboard. The dashboard answers "what is happening"; this page answers "how is RTK configured and what can I change?"

### 5.10 Headroom

Purpose: manage Headroom-compatible configuration and lifecycle details.

Shows:

1. Proxy/SDK/MCP status.
2. Health, stats, history, metrics.
3. Compression level and recommended level.
4. CCR status.
5. Memory and learn status.
6. Output shaper status.
7. Cost and token savings breakdown.
8. Export controls.

Rules:

1. Do not read CCR raw content by default.
2. Show summaries and counts first.
3. External mode is read-only unless the user enters managed config flow.

This page is more detailed and action-oriented than the Headroom Dashboard. The dashboard answers "what is happening"; this page answers "how is Headroom configured and what can I change?"

### 5.11 Doctor

Purpose: explain what is healthy, risky, or broken.

Checks:

1. Runtime mode safety.
2. Whether fixture mode accidentally calls real tools.
3. Test port versus real port.
4. Temporary HOME/PATH in fake mode.
5. RTK availability.
6. Headroom availability.
7. Proxy binding host.
8. Provider route validity.
9. Credential storage.
10. Config write risk.
11. Backup/rollback readiness.
12. Platform requirements: WebView2, MSVC/MinGW, macOS command line tools, WSL shell hooks.

M3:

1. No-touch Doctor.

M4/M5:

1. Install Doctor.
2. Repair suggestions with diff preview.

### 5.12 Logs

Purpose: make behavior auditable without leaking sensitive data.

Views:

1. Request log.
2. Route decisions.
3. RTK events.
4. Headroom events.
5. Install events.
6. Config patch events.
7. Errors.

Default log fields:

1. request id
2. timestamp
3. agent
4. instance id
5. provider id
6. route id
7. model
8. token counts
9. latency
10. transforms
11. error type

Never default-log:

1. Full API keys.
2. Full cookies.
3. Full Authorization headers.
4. Large prompts.
5. Large code snippets.
6. Large tool outputs.

### 5.13 Changes

Purpose: show every proposed config change before it happens.

Shows:

1. Change summary.
2. Target file or setting.
3. Before/after diff.
4. Risk level.
5. Backup location.
6. Rollback action.
7. Doctor checks that will run after apply.

M3:

1. Plan-only patch preview.

M4/M5:

1. Apply transaction.
2. Verify and commit rollback point.

### 5.14 Backup & Rollback

Purpose: make managed install safe.

Shows:

1. Rollback points.
2. Files changed.
3. Tools installed.
4. Processes started.
5. Verification status.
6. Restore action.
7. Cleanup action.

Failure flow:

1. Stop new processes.
2. Restore backups.
3. Remove partial files.
4. Report exact failure.

### 5.15 Settings

Purpose: app-level preferences.

Settings:

1. Default mode.
2. Theme.
3. Data retention.
4. Export redaction level.
5. Debug logging, with double confirmation.
6. Update channel.
7. Local API port.
8. Advanced feature flags.

Debug logging must be visibly dangerous and off by default.

## 6. Core Workflows

### 5.1 First Run

1. User opens app.
2. App explains current safe mode.
3. User chooses Demo, Connect Existing, or Managed Install.
4. App runs Doctor.
5. App shows plan and next action.

### 5.2 Connect Existing Tools

1. User chooses Connect Existing.
2. App runs read-only checks.
3. App shows RTK/Headroom status.
4. App imports aggregate stats only.
5. App never modifies hooks or config unless user starts a change flow.

### 5.3 Managed Install

1. Scan environment.
2. Build install plan.
3. Show exact diff and tool provenance.
4. Backup.
5. Apply.
6. Verify.
7. Run Doctor.
8. Save rollback point.

### 5.4 Daily Use

1. User opens Dashboard.
2. Checks savings and health.
3. Filters by project or agent.
4. Opens a warning if something changed.
5. Exports redacted report if needed.

### 5.5 Repair

1. Doctor finds a problem.
2. App explains likely cause.
3. App proposes a repair plan.
4. User previews diff.
5. User applies or cancels.
6. App verifies and records rollback.

## 7. Milestone Build Plan

### M3 App Build Target

Goal: useful fixture-first app skeleton.

Must have:

1. Dashboard.
2. RTK Dashboard.
3. Headroom Dashboard.
4. Agents matrix.
5. Protocol injection panel.
6. Project policy preview.
7. Doctor preview.
8. Changes preview.
9. Backup/Rollback placeholder.
10. Fixture data only.

Should not have:

1. Real config writes.
2. Real service control.
3. Real hook modification.
4. Real Headroom probing by default.

### M4 App Build Target

Goal: production managed install.

Must have:

1. Setup wizard.
2. InstallationTransaction.
3. Config diff.
4. Backup.
5. Apply.
6. Verify.
7. Doctor.
8. Rollback point.

### M5 App Build Target

Goal: release hardening.

Must have:

1. Windows installer.
2. macOS bundle.
3. Signing/notarization path.
4. Update flow.
5. E2E compatibility tests.
6. Privacy/export/redaction audit.
7. User guide and troubleshooting.

## 8. Data Model Summary

The UI should be built around these visible objects:

1. ProjectProfile
2. ProjectCompressionPolicy
3. AgentCompatibility
4. ProviderProfile
5. RouteProfile
6. InstanceProfile
7. DtokenEvent
8. DoctorCheck
9. PatchPlan
10. BackupPoint
11. RollbackAction

The frontend should not treat RTK, Headroom, provider, route, and installer status as unrelated cards. They are separate views of one local control plane.

## 9. Open Product Questions

These are the most useful questions to answer before heavy UI implementation:

1. Should first-run default be "Demo first" or "Setup wizard first"?
2. Should d-token mainly look like a developer dashboard or a consumer installer app?
3. Which agent should be treated as the primary launch target for v1: Claude Code, Codex CLI, Cursor, or all equally?
4. Should Managed Install be a single guided wizard or separate install pages for RTK, Headroom, and d-token?
5. Should the app support a tray/menu-bar always-running mode in v1, or only a normal desktop window?
6. How much historical data should be retained by default: 7 days, 30 days, or user-controlled?
7. Should export be JSON/CSV only first, or also a human-readable diagnostics bundle?
8. Should protocol injection be presented to users as "OpenAI/Anthropic proxy" or with friendlier wording like "Connect any compatible AI tool"?

## 10. Recommended Decision For Now

For the next implementation pass, build M3 into a real navigable app with these pages first:

1. Overview Dashboard
2. RTK Dashboard
3. Headroom Dashboard
4. Setup
5. Agents
6. Projects
7. Routes
8. Providers
9. Doctor
10. Logs
11. Changes

Keep Backup & Rollback visible as a placeholder because it is central to the product promise, but defer real rollback actions until M4.
