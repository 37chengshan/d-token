import { performanceNow, recordPerformanceDuration } from "./performance";

// tauri-specta auto-generated bindings. Re-run `npm run tauri:dev` after
// changing #[specta::specta] commands in src-tauri/src/main.rs to regenerate.
// This file is gitignored until the first tauri:dev run; type-checking falls
// back to `unknown` when bindings are absent.
//
// Loading strategy (P2 fix):
// - No bare `require()`: that pulls in Node globals and breaks Vite/tsc when
//   the gitignored `bindings.ts` is absent.
// - No static `import("./bindings")`: with `moduleResolution: "Node"` and
//   Rollup bundling, a static literal dynamic import errors on missing file.
// - Instead we use a non-literal specifier (`BINDINGS_MODULE` variable) so
//   tsc does not attempt module resolution (no TS2307) and Vite/Rollup leaves
//   the import as a runtime dynamic import (no build error). At runtime, a
//   404 or missing module is caught by the try/catch → `null` (fail-closed).
// - We deliberately avoid `import.meta.glob` to keep the surface generic
//   across Vite and vitest environments.
type SpectaBindings = {
  commands: Record<string, (...args: unknown[]) => Promise<unknown>>;
};

let spectaBindings: SpectaBindings | null = null;

/** Non-literal specifier so tsc/Rollup skip static resolution. */
const BINDINGS_MODULE = "./bindings";

/**
 * Lazily load the tauri-specta bindings. Returns `null` when the bindings
 * module is absent (e.g. `npm run dev` without a prior `tauri:dev`), which
 * makes `spectaCommands` fail closed via the Proxy below.
 *
 * This must be called inside the Tauri runtime only; in plain Vite dev the
 * dynamic import is expected to fail and return `null`.
 */
async function loadSpectaBindings(): Promise<SpectaBindings | null> {
  if (spectaBindings) {
    return spectaBindings;
  }
  try {
    // Non-literal specifier: tsc does not resolve the module (no TS2307).
    // Vite/Rollup leaves this as a runtime dynamic import (no build error).
    // At runtime in the Tauri WebView the module should exist; outside Tauri
    // the 404 is caught by the try/catch and returns null.
    const mod = (await import(BINDINGS_MODULE)) as SpectaBindings;
    spectaBindings = mod;
    return mod;
  } catch {
    return null;
  }
}

export type NativeToolCheck = {
  id: string;
  label: string;
  installed: boolean;
  version?: string | null;
  path?: string | null;
  status: string;
  detail: string;
};

export type NativeActionLog = {
  time: string;
  level: "info" | "warn" | "error" | string;
  message: string;
};

export type NativeActionResult = {
  ok: boolean;
  summary: string;
  logs: NativeActionLog[];
  tools: NativeToolCheck[];
};

export type AgentRouteValidationResult = {
  mode: "agent-route-validation";
  route: "none/direct-command";
  fallback: "disabled";
  results: Array<{
    model: string;
    ok: boolean;
    summary: string;
    logs: NativeActionLog[];
  }>;
};

export function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const RUNTIME_INVALIDATED_EVENT = "dtoken:runtime-invalidated";

const MUTATING_COMMANDS = new Set([
  "start_dtoken_service",
  "restart_dtoken_service",
  "stop_dtoken_service",
  "run_install_transaction",
  "apply_opencode_connection",
  "apply_learning_managed_block",
  "apply_route_profile_draft",
  "apply_optical_config_change",
  "rollback_optical_config_change",
  "apply_hybrid_agent_integration",
  "rollback_hybrid_agent_integration",
  "apply_connection_route_plan",
  "rollback_connection_route_plan",
  "finalize_agent_auto_follow_authorization",
  "apply_doctor_common_repair",
  "apply_doctor_repair",
  "apply_rollback",
  "rollback_route_profile",
  "apply_backup_action",
  "manage_backup",
  "save_route_profile_draft",
  "store_provider_credential",
  "delete_provider_credential",
  "save_app_settings",
  "enable_autostart",
  "disable_autostart",
  "apply_learning_plan",
  "rollback_learning_plan",
  "apply_codebase_memory_integration",
  "rollback_codebase_memory_integration",
  "start_codebase_memory_ui",
  "stop_codebase_memory_ui",
  "start_codebase_memory_index",
  "cancel_codebase_memory_index",
  "delete_codebase_memory_index",
]);

export async function invokeNative<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const startedAt = performanceNow();
  if (!isTauriRuntime()) {
    recordPerformanceDuration(
      "native_command",
      command,
      startedAt,
      "error",
      "native_unavailable",
    );
    throw new Error("native_unavailable");
  }
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const result = await invoke<T>(command, args);
    recordPerformanceDuration("native_command", command, startedAt, "ok");
    if (MUTATING_COMMANDS.has(command)) {
      window.dispatchEvent(new CustomEvent(RUNTIME_INVALIDATED_EVENT, { detail: { command } }));
    }
    return result;
  } catch (error) {
    recordPerformanceDuration(
      "native_command",
      command,
      startedAt,
      "error",
      "native_command_failed",
    );
    throw error;
  }
}

// ---------------------------------------------------------------------------
// tauri-specta type-safe IPC surface
// ---------------------------------------------------------------------------
//
// `spectaCommands` exposes the Rust commands registered in
// `specta_commands!` (src-tauri/src/main.rs) with full TypeScript types.
// When bindings are absent (e.g. running `npm run dev` without Tauri), the
// proxy throws `native_unavailable` so callers fall back to mock data.
//
// Usage:
//   import { spectaCommands } from "@/app/lib/native";
//   const status = await spectaCommands.getSpectaIpcStatus();
//
// Migration recipe (Rust → TS):
//   1. Add `#[specta::specta]` to the Rust command.
//   2. Register it in `specta_commands!`.
//   3. Re-run `npm run tauri:dev` to regenerate `bindings.ts`.
//   4. Call it via `spectaCommands.myCommand(...)`.

/**
 * Type-safe access to the tauri-specta command surface.
 *
 * Because the bindings module is gitignored and may be absent, each call
 * lazily loads the bindings via `loadSpectaBindings`. When the bindings are
 * missing the returned promise rejects with `native_unavailable`, which is the
 * same sentinel `invokeNative` uses for a non-Tauri environment. Callers can
 * therefore branch on `native_unavailable` to fall back to mock data.
 *
 * This surface is async (returns `Promise<unknown>` per command) to match the
 * tauri-specta generated bindings, which always return promises.
 */
export const spectaCommands = new Proxy(
  {} as Record<string, (...args: unknown[]) => Promise<unknown>>,
  {
    get(_target, prop: string) {
      return async (...args: unknown[]): Promise<unknown> => {
        const bindings = await loadSpectaBindings();
        if (!bindings) {
          throw new Error("native_unavailable");
        }
        const fn = bindings.commands[prop];
        if (typeof fn !== "function") {
          throw new Error(`specta command not found: ${prop}`);
        }
        return fn(...args);
      };
    },
  },
) as unknown as SpectaBindings extends { commands: infer C } ? C : never;

// Re-export the bindings module type for consumers that want the raw types.
export type SpectaIpcStatus = {
  available: boolean;
  contractRevision: string;
  runtime: string;
};
