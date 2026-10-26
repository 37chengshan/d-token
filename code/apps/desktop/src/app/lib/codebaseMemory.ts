import { invokeNative } from "./native";

export type CodebaseMemoryDimensionState =
  | "unavailable"
  | "untrusted"
  | "not_configured"
  | "stopped"
  | "starting"
  | "ready"
  | "busy"
  | "partial"
  | "failed"
  | "unknown";

export type CodebaseMemoryDimension = {
  state: CodebaseMemoryDimensionState;
  reason_code: string;
  detail: string;
  checked_at_ms: number;
  owner?: "agent" | "dtoken" | "none" | "unknown" | string;
  source?: string;
  freshness?: "live" | "recent" | "stale" | "unknown" | string;
  warnings?: string[];
};

export type CodebaseMemoryStatus = {
  schema_version: number;
  source?: string;
  freshness?: string;
  revision?: string;
  approved_project_root?: string | null;
  package_directory?: string;
  package_directory_custom?: boolean;
  active_package_source?: string;
  package_candidates?: Array<{ path: string; compatible: boolean; source: string; reason_code: string; version?: string }>;
  package: CodebaseMemoryDimension;
  integration: CodebaseMemoryDimension;
  agent_session: CodebaseMemoryDimension;
  ui_session: CodebaseMemoryDimension;
  index: CodebaseMemoryDimension;
  active_integrations?: Array<{
    transaction_id: string;
    agent_id: CodebaseMemoryIntegrationDraft["agent_id"] | string;
    state: string;
    rollback_id?: string;
  }>;
};

export type CodebaseMemoryDetectedAgent = {
  id: string;
  name: string;
  installed: boolean;
  detected?: boolean;
  integration_supported?: boolean;
};

export type CodebaseMemoryIntegrationDraft = {
  agent_id: "opencode" | "codex" | "claude-code";
  scope: "project" | "global";
  project_root: string;
  surface: "mcp" | "hook";
};

export type CodebaseMemoryIntegrationPreview = {
  ok: boolean;
  source?: string;
  error?: string | null;
  summary?: string;
  reviewed_digest?: string;
  review_digest?: string;
  diff?: string;
  target?: string;
  backup_summary?: string;
  warnings?: string[];
  draft?: CodebaseMemoryIntegrationDraft;
};

export type CodebaseMemoryIntegrationReceipt = {
  ok: boolean;
  state?: string;
  source?: string;
  error?: string | null;
  summary?: string;
  transaction_id?: string;
  rollback_id?: string;
  verified?: boolean;
  new_session_required?: boolean;
};

export type CodebaseMemoryActionResult = {
  ok: boolean;
  state?: string;
  source?: string;
  error?: string | null;
  summary?: string;
};

export const CODEBASE_MEMORY_DEFAULTS = {
  agent_id: "opencode",
  scope: "project",
  surface: "mcp",
} as const;

export async function getCodebaseMemoryStatus(): Promise<CodebaseMemoryStatus> {
  return invokeNative<CodebaseMemoryStatus>("get_codebase_memory_status");
}

export async function detectCodebaseMemoryAgents(): Promise<CodebaseMemoryDetectedAgent[]> {
  const discovery = await invokeNative<{
    agents?: CodebaseMemoryDetectedAgent[];
  }>("get_hybrid_agent_integrations");
  // Discovery is intentionally broader than the current writer allowlist so
  // the page can account for every locally detected Agent and explicitly mark
  // unsupported targets instead of silently hiding them.
  return discovery.agents ?? [];
}

export async function setCodebaseMemoryPackageDirectory(
  directory?: string,
): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("set_codebase_memory_package_directory", {
    request: { directory: directory?.trim() || null },
  });
}

/** Opens the native folder picker. Selection remains a UI draft until Save validates and persists it. */
export async function selectCodebaseMemoryPackageDirectory(): Promise<string | null> {
  return invokeNative<string | null>("select_codebase_memory_package_directory");
}

export async function selectCodebaseMemoryArchive(): Promise<string | null> {
  return invokeNative<string | null>("select_codebase_memory_archive");
}

export async function importCodebaseMemoryArchive(archivePath: string): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("import_codebase_memory_archive", {
    request: { archive_path: archivePath },
  });
}

export async function openCodebaseMemoryPackageDirectory(): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("open_codebase_memory_package_directory");
}

export async function openCodebaseMemoryDownloadPage(): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("open_codebase_memory_download_page");
}

export async function previewCodebaseMemoryIntegration(
  projectRoot: string,
  selection: Pick<CodebaseMemoryIntegrationDraft, "agent_id" | "scope" | "surface"> = CODEBASE_MEMORY_DEFAULTS,
): Promise<CodebaseMemoryIntegrationPreview> {
  return invokeNative<CodebaseMemoryIntegrationPreview>("preview_codebase_memory_integration", {
    request: { ...selection, project_root: projectRoot },
  });
}

export async function applyCodebaseMemoryIntegration(
  draft: CodebaseMemoryIntegrationDraft,
  reviewedDigest: string,
): Promise<CodebaseMemoryIntegrationReceipt> {
  return invokeNative<CodebaseMemoryIntegrationReceipt>("apply_codebase_memory_integration", {
    request: { draft, reviewed_digest: reviewedDigest },
  });
}

export async function verifyCodebaseMemoryIntegration(
  transactionId: string,
): Promise<CodebaseMemoryIntegrationReceipt> {
  return invokeNative<CodebaseMemoryIntegrationReceipt>("verify_codebase_memory_integration", {
    request: { transaction_id: transactionId },
  });
}

export async function rollbackCodebaseMemoryIntegration(
  transactionId: string,
): Promise<CodebaseMemoryIntegrationReceipt> {
  return invokeNative<CodebaseMemoryIntegrationReceipt>("rollback_codebase_memory_integration", {
    request: { transaction_id: transactionId },
  });
}

export async function startCodebaseMemoryUi(
  projectRoot: string,
): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("start_codebase_memory_ui", {
    request: { project_root: projectRoot },
  });
}

export async function stopCodebaseMemoryUi(): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("stop_codebase_memory_ui");
}

export async function startCodebaseMemoryIndex(
  projectRoot: string,
): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("start_codebase_memory_index", {
    request: { project_root: projectRoot, mode: "explicit" },
  });
}

export async function cancelCodebaseMemoryIndex(): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("cancel_codebase_memory_index");
}

export async function deleteCodebaseMemoryIndex(
  projectRoot: string,
): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("delete_codebase_memory_index", {
    request: { project_root: projectRoot },
  });
}

/** Native derives and validates the short-lived loopback URL; callers never provide one. */
export async function openCodebaseMemoryGraph(): Promise<CodebaseMemoryActionResult> {
  return invokeNative<CodebaseMemoryActionResult>("open_codebase_memory_graph");
}

export function integrationDigest(preview: CodebaseMemoryIntegrationPreview | null): string {
  return preview?.reviewed_digest?.trim() || preview?.review_digest?.trim() || "";
}

export function integrationDraft(
  preview: CodebaseMemoryIntegrationPreview,
  projectRoot: string,
  selection: Pick<CodebaseMemoryIntegrationDraft, "agent_id" | "scope" | "surface"> = CODEBASE_MEMORY_DEFAULTS,
): CodebaseMemoryIntegrationDraft {
  return preview.draft ?? { ...selection, project_root: projectRoot };
}
