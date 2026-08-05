# M3 GUI Control Plane MVP Report

Status: implemented as a fixture-first desktop dashboard skeleton.

## Scope

M3 adds a Tauri/Vite/React control-plane MVP that defaults to fixture mode and presents the core user-facing areas without scanning or modifying the real environment.

## Implemented

1. Vite + React + TypeScript desktop app under `code/apps/desktop`.
2. Tauri 2 configuration shell under `code/apps/desktop/src-tauri`.
3. Dashboard UI for runtime mode, RTK adapter, Headroom adapter, no-touch Doctor, layered savings, project policy, patch preview, and rollback entry.
4. Responsive layout with restrained operational styling.
5. Frontend build pipeline.
6. Agent coverage panel for the supported-agent union.
7. Protocol injection panel for OpenAI-compatible and Anthropic-compatible traffic.

## Verification

1. `npm install` completed with zero reported vulnerabilities.
2. `npm run build` passed.
3. `cargo check --workspace` passed.
4. Structure check passed.

## Multidimensional Review

Product: the first screen is the actual control plane, not a landing page.

Design: UI is operational and dashboard-focused, with stable cards, side navigation, status indicators, and dense but readable information.

Engineering: production Tauri bundling remains disabled until M4 managed installer work, avoiding premature installer complexity.

No-touch safety: visible UI defaults to Fixture Demo and marks Shadow Detect as disabled.

Risk: the dashboard currently uses static fixture-facing data in the frontend; wiring it to the local API is the next refinement before production use.

Platform/product review: M3 visibly separates Mac/Windows support notes and protocol-level injection from agent-specific status, so users are not forced to infer support from implementation details.
