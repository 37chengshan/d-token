# Optical release iteration report — 2026-08-06

## Scope

This slice closes the desktop Visual Compression Beta optical control-plane semantics. It remains shadow/preview-only and does not modify the active Rust runtime boundary or enable production Active transforms.

## Delivered behavior

- Apply failures distinguish a write failure (`config_applied !== true`) from a configuration written to disk but not runtime-verified (`config_applied === true`); the latter keeps its rollback receipt and offers rollback.
- Runtime status is the sole source for the enabled badge and one-click state. The editor draft is not treated as runtime fact after a write that has not been verified.
- Active KPI detail is shown only when the status slice is available; unavailable status renders an unavailable detail instead of implying execution evidence.
- Capability rows are slice-scoped. A failed capability slice no longer silently falls back to status flags; missing capability fields remain unavailable.
- Preview renders gate names/pass-fail/reasons and warnings from the public summary. It accepts only an optional opaque `route_id`; `provider_id` and `model` remain null.
- Removed the obsolete `setupChecklist` helper and its test-only import/coverage because the current workbench uses runtime/editor evidence instead.
- Preview polling avoids hidden-window refreshes and suppresses an immediately duplicated focus refresh.

## Verification evidence

- `npm test -- --run src/app/lib/optical/client.test.ts src/app/lib/optical/configDraft.test.ts src/app/lib/visual-compression/VisualCompressionWorkbench.test.tsx` — PASS, 3 files / 21 tests.
- `npm run build` — PASS, Vite build and bundle budget check.
- `git diff --check` — PASS.
- `npx tsc --noEmit` — BLOCKED by an existing unrelated error in `src/app/lib/learning/discovery.ts`: it imports `AGENT_REGISTRY`, which is not exported by the intentionally presentation-only `src/app/lib/agents/registry.ts` (TS2305 plus the resulting implicit-any diagnostic). No optical file causes this diagnostic.

## Security and boundary checks

The optical config patch still forces `allow_active: false`, `public_summary_only: true`, and disables schema/system-static-slab route features. Preview request tests assert no credential, authorization, token, cookie, base64, image, raw prompt, or secret fields are introduced. Rust active execution boundaries were not changed in this slice.

## Remaining gates

Real-provider signed capability/fidelity evidence, Windows/MSVC evidence, signed release packaging, and real Agent routed-flow evidence remain external release blockers. This report does not mark the product release-ready.
