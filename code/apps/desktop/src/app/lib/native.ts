import { performanceNow, recordPerformanceDuration } from "./performance";

// tauri-specta auto-generated bindings. Re-run `npm run tauri:dev` after
// changing #[specta::specta] commands in src-tauri/src/main.rs to regenerate.
// This file is gitignored until the first tauri:dev run; type-checking falls
// back to `unknown` when bindings are absent.
type SpectaBindings = {
  commands: Record<string, (...args: unknown[]) => Promise<unknown>>;
};
const spectaBindings: SpectaBindings | null = (() => {
  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    return require("./bindings") as SpectaBindings;
  } catch {
    return null;
  }
})();

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

export const spectaCommands = new Proxy(
  spectaBindings?.commands ?? {},
  {
    get(target, prop: string) {
      const fn = (target as Record<string, (...args: unknown[]) => Promise<unknown>>)[prop];
      if (typeof fn !== "function") {
        throw new Error(`specta command not found: ${prop}`);
      }
      return fn;
    },
  },
) as SpectaBindings extends { commands: infer C } ? C : never;

// Re-export the bindings module type for consumers that want the raw types.
export type SpectaIpcStatus = {
  available: boolean;
  contractRevision: string;
  runtime: string;
};
