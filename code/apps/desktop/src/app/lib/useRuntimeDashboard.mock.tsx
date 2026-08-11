/**
 * DEV-only mock data layer for web-preview animation review.
 *
 * Architectural rules:
 * - This module is ONLY reachable from `main.tsx` behind `import.meta.env.DEV`.
 *   In a production build (`import.meta.env.PROD`) the import site compiles to
 *   `false` and Vite tree-shakes this module away — it never ships.
 * - The mock never claims to be real: `status` stays `"browser-preview"` and
 *   slice states stay `"preview"`-flavored so the honest "this is a preview"
 *   semantics of the product are preserved. Numbers are structural samples so
 *   the motion containers (stat cards, lists, heatmap) have content to render.
 * - It does not touch native, config, or any real backend. No writes.
 */
import { createElement, type ReactNode, useCallback, useMemo, useRef } from "react";
import {
  createRuntimeDashboardStore,
  RuntimeDetailContext,
  RuntimeEvidenceContext,
  type RuntimeDashboardState,
  type RuntimeDashboardStore,
} from "./useRuntimeDashboard";

function slice<T>(data: T, source: string): { data: T; source: string; state: "verified" | "partial"; freshness: string; collectedAt: string; error: string } {
  return { data, source, state: "verified", freshness: "preview", collectedAt: new Date().toISOString(), error: "" };
}

function buildMockState(): RuntimeDashboardState {
  const now = new Date().toISOString();
  const payload = {
    desktop_revision: "preview-0.0.1-mock",
    desktop_data_source: "browser-preview-mock",
    desktop_collected_at: now,
    source: "browser-preview-mock",
    stats: {
      requests: { total: 1284, failed: 3, succeeded: 1281 },
      compression: { total_tokens: 1544532, saved_tokens: 184128, avg_ratio: 0.119 },
    },
    providers: [
      { id: "opencode-free", name: "OpenCode Free", model: "deepseek-v4-flash-free", status: "healthy" },
      { id: "opencode-go", name: "OpenCode Go", model: "deepseek-v4-flash", status: "healthy" },
      { id: "claude", name: "Claude Code", model: "claude-sonnet-4", status: "warning" },
    ],
    routes: [
      { id: "r1", provider: "opencode-free", name: "openai-responses", active: true, status: "healthy" },
      { id: "r2", provider: "claude", name: "anthropic-messages", active: false, status: "warning" },
    ],
    runtime_active_config: {
      config: { routes: [{ id: "r1", provider: "opencode-free" }, { id: "r2", provider: "claude" }], fallback_disabled: true },
    },
    doctor: { ok: true, summary: "All systems nominal", checks: 18, passed: 17 },
  };

  const operational = {
    service: slice({ state: "running", pid_running: true, restart_required: false, service_url: "http://127.0.0.1:18001", manifest_stale: false }, "browser-preview"),
    dashboard: slice(payload, "browser-preview"),
    traffic: slice([
      { id: "t1", time: now, type: "route", agent: "opencode", msg: "Routed via openai-responses", msgEn: "Routed via openai-responses", status: "success" },
      { id: "t2", time: now, type: "compression", agent: "opencode", msg: "Saved 11254 tokens (12.1%)", msgEn: "Saved 11254 tokens (12.1%)", status: "success" },
      { id: "t3", time: now, type: "provider", agent: "claude", msg: "Upstream 200 OK", msgEn: "Upstream 200 OK", status: "success" },
      { id: "t4", time: now, type: "error", agent: "codex", msg: "Provider unreachable (retry)", msgEn: "Provider unreachable (retry)", status: "failed" },
      { id: "t5", time: now, type: "route", agent: "claude", msg: "Routed via anthropic-messages", msgEn: "Routed via anthropic-messages", status: "success" },
      { id: "t6", time: now, type: "compression", agent: "codex", msg: "Saved 8301 tokens (10.8%)", msgEn: "Saved 8301 tokens (10.8%)", status: "success" },
      { id: "t7", time: now, type: "route", agent: "opencode", msg: "Routed via openai-responses", msgEn: "Routed via openai-responses", status: "success" },
    ], "browser-preview"),
    routes: slice({
      providers: [{ id: "opencode-free", name: "OpenCode Free", status: "healthy" } as never],
      routes: [{ id: "r1", provider: "opencode-free", name: "openai-responses", active: true } as never],
      instances: [],
      decisions: [{ id: "d1", route: "r1", reason: "configured", time: now } as never],
    }, "browser-preview"),
    transactions: slice({ events: [], backups: [], changes: [] }, "browser-preview"),
    doctor: slice({ ok: true, summary: "All systems nominal", checks: 18, passed: 17 }, "browser-preview"),
    collectedAt: now,
  };

  return {
    payload,
    status: "browser-preview",
    updatedAt: new Date(),
    error: "",
    operational,
    refresh: async () => {},
    invalidate: async () => {},
  };
}

export function MockPreviewProvider({ children }: { children: ReactNode }) {
  const storeRef = useRef<RuntimeDashboardStore | null>(null);
  if (!storeRef.current) storeRef.current = createRuntimeDashboardStore(buildMockState());

  const activate = useCallback(() => () => {}, []);
  const detailValue = useMemo(
    () => ({
      payload: (storeRef.current?.getSnapshot?.() ?? buildMockState()).payload,
      status: "browser-preview" as const,
      updatedAt: new Date(),
      error: "",
      refresh: async () => {},
      invalidate: async () => {},
      activate,
    }),
    [activate],
  );

  return createElement(
    RuntimeEvidenceContext.Provider,
    { value: storeRef.current },
    createElement(RuntimeDetailContext.Provider, { value: detailValue }, children),
  );
}
