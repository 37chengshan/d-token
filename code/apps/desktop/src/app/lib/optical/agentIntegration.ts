import { invokeNative, type NativeActionResult } from "../native";

export type HybridAgentId = string;
export type HybridAgentScope = "global" | "project";

export type HybridAgentIntegrationDraft = {
  agent_ids: HybridAgentId[];
  scope: HybridAgentScope;
  project_root: string | null;
  install_instructions: boolean;
  install_hook_or_plugin: boolean;
  install_skill: boolean;
  install_mcp: boolean;
};

export type HybridAgentTargetPlan = {
  agent_ids: string[];
  surface: "transport" | "instructions" | "hook_or_plugin" | "skill" | "mcp";
  target_path: string;
  before_sha256: string;
  after_sha256: string;
  diff: string;
  trust_required: boolean;
  new_session_required: boolean;
};

export type HybridAgentIntegrationPlan = {
  schema_version: number;
  scope: HybridAgentScope;
  project_root: string | null;
  review_digest: string;
  targets: HybridAgentTargetPlan[];
  rollback_ids: string[];
  created_directories: string[];
  restart_required: boolean;
  new_session_required: boolean;
  transport: { surface: "transport"; state: string; detail: string };
};

export type HybridAgentDiscovery = {
  ok: boolean;
  source: string;
  agents: Array<{
    id: HybridAgentId;
    name: string;
    installed: boolean;
    detected?: boolean;
    integration_supported?: boolean;
    install_capability?: string;
    session_capability?: string;
    evidence_count?: number;
    surfaces: string[];
    transport_state: string;
  }>;
  bridge: { available: boolean; path?: string | null; error?: string | null };
  service: {
    state?: string;
    pid?: number;
    port?: number;
    runtime_identity?: string;
    runtime_version?: string;
    runtime_identity_ok?: boolean;
    runtime_version_ok?: boolean;
  };
  transport: { configured: boolean; route_count: number; detail: string };
  defaults: {
    install_instructions: boolean;
    install_hook_or_plugin: boolean;
    install_skill: boolean;
    install_mcp: boolean;
    scope: HybridAgentScope;
    project_root?: string | null;
  };
};

export type HybridAgentPreview = {
  ok: boolean;
  state: string;
  source?: string;
  error?: string;
  plan?: HybridAgentIntegrationPlan;
  requires_explicit_confirmation?: boolean;
};

export type HybridAgentReceipt = {
  transaction_id: string;
  review_digest: string;
  rollback_ids: string[];
  created_directories: string[];
  verified_targets: string[];
  trust_required: boolean;
  new_session_required: boolean;
};

export type HybridAgentApply = {
  ok: boolean;
  state: string;
  source?: string;
  error?: string;
  summary?: string;
  receipt?: HybridAgentReceipt;
};

export type HybridAgentVerification = {
  ok: boolean;
  state: HybridAgentTransactionState;
  source?: string;
  error?: string;
};

export type HybridAgentTransactionState =
  | "applied"
  | "verified"
  | "partial"
  | "changed"
  | "rolled_back"
  | "failed";

export type HybridAgentTransaction = {
  schema_version: number;
  transaction_id: string;
  review_digest: string;
  rollback_ids: string[];
  created_directories: string[];
  targets: HybridAgentTargetPlan[];
  state: HybridAgentTransactionState;
  applied_at_ms: number;
  verified_at_ms?: number | null;
  rolled_back_at_ms?: number | null;
  failure_reason?: string | null;
};

export async function loadHybridAgentIntegrations(): Promise<HybridAgentDiscovery> {
  return invokeNative<HybridAgentDiscovery>("get_hybrid_agent_integrations");
}

export async function previewHybridAgentIntegration(
  draft: HybridAgentIntegrationDraft,
): Promise<HybridAgentPreview> {
  return invokeNative<HybridAgentPreview>("preview_hybrid_agent_integration", { request: draft });
}

export async function applyHybridAgentIntegration(
  draft: HybridAgentIntegrationDraft,
  reviewedDigest: string,
): Promise<HybridAgentApply> {
  return invokeNative<HybridAgentApply>("apply_hybrid_agent_integration", {
    request: { draft, reviewed_digest: reviewedDigest },
  });
}

export async function verifyHybridAgentIntegration(
  transactionId: string,
): Promise<HybridAgentVerification> {
  return invokeNative<HybridAgentVerification>("verify_hybrid_agent_integration", {
    request: { transaction_id: transactionId },
  });
}

export async function rollbackHybridAgentIntegration(
  transactionId: string,
): Promise<HybridAgentApply> {
  return invokeNative<HybridAgentApply>("rollback_hybrid_agent_integration", {
    request: { transaction_id: transactionId },
  });
}

export async function getHybridAgentIntegrationTransaction(
  transactionId: string,
): Promise<{ ok: boolean; state: string; transaction?: HybridAgentTransaction; error?: string }> {
  return invokeNative("get_hybrid_agent_integration_transaction", {
    request: { transaction_id: transactionId },
  });
}

export async function startHybridService(): Promise<NativeActionResult> {
  return invokeNative<NativeActionResult>("start_dtoken_service");
}
