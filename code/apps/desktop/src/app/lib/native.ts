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

export async function invokeNative<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    throw new Error("native_unavailable");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}
