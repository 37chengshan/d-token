# Protocol normalization report

- Branch: `codex/release-protocol-normalization`
- Scope: shared closed desktop protocol adapter and migration of Advanced, Analytics snapshot/breakdown, Compatibility, LogsSecurity, Install presentation, and cc-switch protocol parsing.
- Wire contract: OpenAI UI/canonical id is `openai`, runtime/config wire id is always `open_ai`; Anthropic remains `anthropic`. UI labels are localized and never sent back as payload protocol values. Unknown values fail closed as `Unknown`/`未知` and do not serialize.
- Capability separation: `cc-switch` is treated as an external capability id and is not normalized as a protocol or `cc_switch` wire value.
- Changed artifacts: `code/apps/desktop/src/app/lib/protocol.ts`, protocol tests, target page/lib consumers, and their protocol-facing regression expectations.

## Verification

- `npm run test:run -- src/app/lib/protocol.test.ts src/app/lib/analytics/metrics.test.ts src/app/components/pages/Compatibility.test.tsx src/app/components/pages/Install.test.tsx src/app/components/pages/Analytics.test.tsx src/app/components/pages/LogsSecurity.test.tsx` — passed: 6 files, 55 tests.
- `npm run build` — passed: Vite production build and bundle budget check.
- `npx tsc --noEmit` — blocked by pre-existing unrelated `src/app/lib/learning/discovery.ts` errors: missing `AGENT_REGISTRY` export and implicit `agent` any. No diagnostics were reported in protocol or migrated target files.
- Native/Tauri and E2E verification were not run in this focused frontend worktree.
