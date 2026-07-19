import { performanceNow, recordPerformanceDuration } from "./performance";

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
