# Five-Page Desktop Application Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Converge the desktop product into five coherent pages without losing the current visual style, animation quality, or evidence boundaries.

**Architecture:** Replace the current top-level route set with five stable PageId values and compose existing capabilities into task-oriented workbenches. Add one typed contextEngine client for native commands, one shared evidence-state component, deterministic legacy route migration, and focused page tests. Keep business logic in Rust/Tauri; React owns presentation and user task flow.

**Tech Stack:** React 19, TypeScript, Tauri invoke, Vitest/Testing Library, Playwright, existing CSS and animation system.

**Spec:** [Approved context engine design](../specs/2026-09-19-context-engine-five-page-refactor-design.md)

## Global Constraints

- Top-level pages are only configuration, advanced-configuration, analysis-logs, diagnostics, and settings.
- Configuration has two primary surfaces: Compression Engine and Code Graph.
- Advanced Configuration contains learning, workflow policy, routing, recovery, and guarded expert controls.
- Analysis & Logs combines live/history analytics, request decisions, changes, backups, logs, and exports.
- Diagnostics combines runtime, provider, Jev, agent integration, security, and repair evidence.
- Keep current theme, motion language, notification system, operation dialogs, and accessible reduced-motion behavior.
- A displayed state needs source, freshness, and action-result evidence or must say unavailable, partial, preview, or unknown.
- Never display or transport a credential value after the store action completes.

## Review Focus

- A new user can configure the engine, understand Local versus Jev, diagnose a failure, and inspect one request without navigating across unrelated pages.
- Old session storage and internal navigation targets land on the correct new page.
- Actions show prerequisites, effect, evidence, and recovery; disabled actions explain why.
- Dense engineering terms are secondary detail, not the primary label.
- Keyboard, focus, responsive layout, reduced motion, and loading/error/empty states are tested.

---

### Task 1: Replace the top-level route contract and migrate legacy state

**Files:**
- Modify: code/apps/desktop/src/app/lib/nav.ts
- Modify: code/apps/desktop/src/app/components/shell/AppShell.tsx
- Modify: code/apps/desktop/src/app/components/shell/Sidebar.tsx
- Modify: code/apps/desktop/src/app/components/shell/AnimatedNavIcon.tsx
- Modify: code/apps/desktop/src/app/components/shell/StatusBar.tsx
- Modify: code/apps/desktop/src/app/lib/i18n.ts
- Modify: code/apps/desktop/src/app/components/shell/AppShell.test.tsx
- Modify: code/apps/desktop/src/app/components/shell/StatusBar.test.tsx
- Create: code/apps/desktop/src/app/lib/nav.test.ts

- [ ] Add failing tests for the exact five PageId values and legacy migration table.

~~~ts
const LEGACY_PAGE_MAP = {
  overview: "configuration",
  install: "configuration",
  "codebase-memory": "configuration",
  visual: "configuration",
  advanced: "advanced-configuration",
  analytics: "analysis-logs",
  changes: "analysis-logs",
  logs: "analysis-logs",
  doctor: "diagnostics",
  compatibility: "diagnostics",
  settings: "settings",
} satisfies Record<string, PageId>;
~~~

- [ ] Run focused tests and confirm the old route set fails.

~~~bash
cd code/apps/desktop
npm run test:run -- src/app/lib/nav.test.ts src/app/components/shell/AppShell.test.tsx
~~~

- [ ] Replace PageId and NAV_ITEMS. Keep settings in the same visible placement contract the shell currently uses, but do not treat it as a sixth internal route.

- [ ] Add readLegacyPageId that migrates the stored value once and rewrites the session key to the new PageId.

- [ ] Update every StatusBar/notification/deep-link navigation target through a typed navigateTo(PageId) boundary.

- [ ] Update icons and localized labels while preserving current animation timings and reduced-motion branches.

- [ ] Run route/shell tests.

~~~bash
cd code/apps/desktop
npm run test:run -- src/app/lib/nav.test.ts src/app/components/shell
npx tsc --noEmit
~~~

- [ ] Audit and commit.

Commit: feat(desktop): converge shell navigation to five pages

### Task 2: Build the Configuration page around engine setup

**Files:**
- Create: code/apps/desktop/src/app/components/pages/Configuration.tsx
- Create: code/apps/desktop/src/app/components/pages/Configuration.test.tsx
- Create: code/apps/desktop/src/app/components/context-engine/EngineModeCard.tsx
- Create: code/apps/desktop/src/app/components/context-engine/WorkflowSummary.tsx
- Create: code/apps/desktop/src/app/lib/contextEngine.ts
- Modify: code/apps/desktop/src/app/components/pages/InstallWorkbench.tsx
- Modify: code/apps/desktop/src/app/components/pages/CodebaseMemory.tsx
- Modify: code/apps/desktop/src/app/components/shell/AppShell.tsx

- [ ] Add failing tests for the two tabs, Local default, Jev no-key state, credential store action, connection test, and Code Graph unavailable/ready states.

- [ ] Define typed native DTOs in contextEngine.ts. Keep UI types aligned with Tauri responses and make unknown freshness explicit.

~~~ts
export type EngineMode = "local" | "jev-shadow" | "jev-active";

export interface DecisionHealth {
  source: "runtime" | "event-ledger" | "unknown";
  freshnessMs: number | null;
  credentialState: "not-configured" | "available" | "locked" | "unknown";
  overall: "healthy" | "partial" | "unavailable" | "unknown";
  reasonCodes: string[];
}
~~~

- [ ] Implement Compression Engine as a single setup flow:
  1. choose Local or Jev;
  2. show what changes;
  3. satisfy credential prerequisite when needed;
  4. test the connection;
  5. preview and apply;
  6. show verification and rollback reference.

- [ ] Reuse the functional parts of InstallWorkbench as extracted panels instead of nesting its old top-level tabs.

- [ ] Reuse CodebaseMemory inside the Code Graph tab. Keep its experimental label, indexing freshness, package/source evidence, and repair actions.

- [ ] Never return the Jev key to React after storage. Clear the input immediately on successful native storage.

- [ ] Run tests.

~~~bash
cd code/apps/desktop
npm run test:run -- src/app/components/pages/Configuration.test.tsx
npx tsc --noEmit
~~~

- [ ] Audit and commit.

Commit: feat(desktop): add context engine configuration page

### Task 3: Build Advanced Configuration with independent workflow controls

**Files:**
- Create: code/apps/desktop/src/app/components/pages/AdvancedConfiguration.tsx
- Create: code/apps/desktop/src/app/components/pages/AdvancedConfiguration.test.tsx
- Create: code/apps/desktop/src/app/components/context-engine/WorkflowPolicyEditor.tsx
- Create: code/apps/desktop/src/app/components/context-engine/LearningPolicyPanel.tsx
- Modify: code/apps/desktop/src/app/lib/advanced/AdvancedWorkbench.tsx
- Modify: code/apps/desktop/src/app/components/shell/AppShell.tsx

- [ ] Add failing tests for independent workflow mode, threshold, timeout, sticky TTL, fallback, kill switch, and reserved-workflow absence.

- [ ] Present four groups: Smart Learning, Jev Workflows, Routing & Cache, Recovery & Retention. Put raw expert details behind a disclosure without creating more pages.

- [ ] Implement a preview/apply/verify/rollback editor using existing OperationDialog and config change records.

- [ ] Render the five executable workflows from typed runtime data, not a duplicated hard-coded list. Assert that reserved workflows are not returned by the editable command.

- [ ] Display activation readiness for Shadow-to-Active as evidence metrics and unmet gates. Do not offer Active when the backend rejects readiness.

- [ ] Add invalid-input and concurrent-change tests: out-of-range threshold, timeout beyond bounds, stale config revision, key removed between preview/apply, and rollback failure.

- [ ] Run tests.

~~~bash
cd code/apps/desktop
npm run test:run -- src/app/components/pages/AdvancedConfiguration.test.tsx
npx tsc --noEmit
~~~

- [ ] Audit and commit.

Commit: feat(desktop): add advanced context policy page

### Task 4: Merge analytics, logs, changes, and recovery evidence

**Files:**
- Create: code/apps/desktop/src/app/components/pages/AnalysisLogs.tsx
- Create: code/apps/desktop/src/app/components/pages/AnalysisLogs.test.tsx
- Create: code/apps/desktop/src/app/components/analysis/DecisionTimeline.tsx
- Create: code/apps/desktop/src/app/components/analysis/WorkflowMetrics.tsx
- Modify: code/apps/desktop/src/app/components/pages/Analytics.tsx
- Modify: code/apps/desktop/src/app/components/pages/Changes.tsx
- Modify: code/apps/desktop/src/app/components/pages/LogsSecurity.tsx
- Modify: code/apps/desktop/src/app/components/shell/AppShell.tsx

- [ ] Add failing tests for Overview, Requests, Changes, and Logs tabs plus empty/loading/partial/error states.

- [ ] Build the Overview from runtime analytics: tokens, claimable savings, local/Jev split, fallback rate, p95 decision latency, sticky hit rate, and recovery failures. Label estimated and provider-observed values separately.

- [ ] Build Requests as a redacted timeline showing request_id, workflow, source, reason, confidence band, validation, recovery presence, send status, and freshness. No prompt/body preview.

- [ ] Integrate Changes and backups as one task flow: inspect diff, verify outcome, open evidence, and roll back.

- [ ] Integrate security/log export with explicit redaction and data-source status. Keep raw command output unavailable.

- [ ] Add navigation tests from a failed request to Diagnostics with a typed filter/correlation ID.

- [ ] Run tests.

~~~bash
cd code/apps/desktop
npm run test:run -- src/app/components/pages/AnalysisLogs.test.tsx
npx tsc --noEmit
~~~

- [ ] Audit and commit.

Commit: feat(desktop): unify analysis logs and change evidence

### Task 5: Converge Diagnostics and Settings

**Files:**
- Create: code/apps/desktop/src/app/components/pages/Diagnostics.tsx
- Create: code/apps/desktop/src/app/components/pages/Diagnostics.test.tsx
- Modify: code/apps/desktop/src/app/lib/doctor/DoctorWorkbench.tsx
- Modify: code/apps/desktop/src/app/components/pages/Compatibility.tsx
- Modify: code/apps/desktop/src/app/components/pages/Settings.tsx
- Modify: code/apps/desktop/src/app/components/pages/Settings.test.tsx
- Modify: code/apps/desktop/src/app/components/shell/AppShell.tsx

- [ ] Add failing Diagnostics tests for runtime, provider, Jev, agent integration, Event Ledger, recovery, security, and repair-result evidence.

- [ ] Merge compatibility checks into Diagnostics. Each check shows status, source, freshness, consequence, and one safe next action.

- [ ] Keep repair actions transactional. Show preview and required authorization according to existing product contracts, then verification and rollback evidence.

- [ ] Simplify Settings to app behavior, appearance, language, update/channel, privacy/retention, and data/export/reset. Link engine behavior back to Configuration or Advanced Configuration rather than duplicating controls.

- [ ] Add tests proving Settings does not expose Optical, raw keys, duplicate routing controls, or developer fixture/smoke commands.

- [ ] Run tests.

~~~bash
cd code/apps/desktop
npm run test:run -- \
  src/app/components/pages/Diagnostics.test.tsx \
  src/app/components/pages/Settings.test.tsx
npx tsc --noEmit
~~~

- [ ] Audit and commit.

Commit: feat(desktop): converge diagnostics and settings

### Task 6: Validate end-to-end user flows and remove obsolete routes

**Files:**
- Modify: code/apps/desktop/e2e/desktop-control-plane.spec.ts
- Modify: code/apps/desktop/src/styles/shell.css
- Modify: code/apps/desktop/src/styles/theme.css
- Modify: code/apps/desktop/README.md
- Modify: docs/APP_FUNCTIONS_AND_PAGES.md
- Modify: docs/FRONTEND_CONSTRAINTS.md
- Modify: docs/PRD.md
- Modify: docs/product/IMPLEMENTATION_STATUS.md

- [ ] Add Playwright flows:
  - first run in Local mode;
  - store Jev key, test, and remain in Shadow;
  - edit one workflow, preview, apply, verify, rollback;
  - inspect a fallback request and open Diagnostics;
  - use Code Graph unavailable and ready states;
  - delete Jev key and confirm immediate Local mode;
  - migrate each legacy route value;
  - navigate entirely by keyboard.

- [ ] Add viewport coverage for compact laptop, standard desktop, and wide desktop. Assert no clipped primary action or horizontal page overflow.

- [ ] Add reduced-motion and focus-visible assertions. Verify dialogs restore focus to the invoking control.

- [ ] Delete obsolete top-level component imports and route branches only after all capabilities have an owned location or are explicitly retired.

- [ ] Run complete desktop verification.

~~~bash
cd code/apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
~~~

Expected: all checks pass; E2E sees exactly five top-level pages.

- [ ] Run an interactive Tauri development build and inspect the five page flows at all target viewports.

~~~bash
cd code/apps/desktop
npm run tauri:dev
~~~

Expected: native commands return real source/freshness evidence; no preview-only state is mistaken for runtime success.

- [ ] Audit and commit.

Commit: docs(desktop): align five-page product contract
