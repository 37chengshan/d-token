import type { AgentUpstreamBinding, ConnectionSetupSnapshot, UpstreamCandidate } from "./connectionSetup";

export type InstallPhase =
  | "idle"
  | "discovering"
  | "already-connected"
  | "ready"
  | "needs-credential"
  | "review"
  | "applying-runtime"
  | "applying-agent-configs"
  | "verifying"
  | "verified"
  | "partial"
  | "blocked"
  | "rolled-back";

export type AgentSelection = {
  candidateKey: string;
  model: string;
};

export type InstallMachineState = {
  phase: InstallPhase;
  snapshot: ConnectionSetupSnapshot | null;
  selectedAgentIds: string[];
  selections: Record<string, AgentSelection>;
  reviewDiscoveryRevision: string | null;
  message: string;
};

export type InstallMachineEvent =
  | { type: "DISCOVER_START" }
  | { type: "DISCOVERED"; snapshot: ConnectionSetupSnapshot }
  | { type: "DISCOVERY_FAILED"; message: string }
  | { type: "OPEN_REVIEW" }
  | { type: "TOGGLE_AGENT"; agentId: string }
  | { type: "SET_AGENT_SELECTION"; agentIds: string[] }
  | { type: "SET_PHASE"; phase: InstallPhase; message?: string }
  | { type: "RESET" };

export type InstallAgentBatchItem = {
  agentId: string;
  agentName: string;
  alreadyConnected: boolean;
  reviewDigest?: string;
};

export type InstallAgentReceipt = {
  agentId: string;
  agentName: string;
  state: "already-connected" | "applied" | "verified" | "failed" | "blocked" | "rolled-back" | "rollback-failed";
  transactionId?: string;
  summary: string;
};

export type InstallAgentBatchOperations = {
  apply: (item: InstallAgentBatchItem) => Promise<{ ok: boolean; transactionId?: string; summary: string }>;
  verify: (receipt: InstallAgentReceipt) => Promise<{ ok: boolean; summary: string }>;
  rollback: (receipt: InstallAgentReceipt) => Promise<{ ok: boolean; summary: string }>;
};

export type InstallAgentBatchResult = {
  ok: boolean;
  receipts: InstallAgentReceipt[];
  error: string;
};


export type InstallHandoffIntent = {
  agentIds: string[];
  discoveryRevision: string;
  source: "onboarding-start-setup";
  requestedAtMs: number;
};

let pendingInstallHandoff: InstallHandoffIntent | null = null;

function copyInstallHandoff(intent: InstallHandoffIntent): InstallHandoffIntent {
  return { ...intent, agentIds: [...intent.agentIds] };
}

export function requestInstallHandoff(intent: Omit<InstallHandoffIntent, "requestedAtMs"> & { requestedAtMs?: number }): InstallHandoffIntent {
  const agentIds = Array.from(new Set(intent.agentIds.filter((id) => id.trim().length > 0)));
  pendingInstallHandoff = {
    agentIds,
    discoveryRevision: intent.discoveryRevision,
    source: intent.source,
    requestedAtMs: intent.requestedAtMs ?? Date.now(),
  };
  return copyInstallHandoff(pendingInstallHandoff);
}

export function consumeInstallHandoff(): InstallHandoffIntent | null {
  const intent = pendingInstallHandoff;
  pendingInstallHandoff = null;
  return intent ? copyInstallHandoff(intent) : null;
}

export function hasPendingInstallHandoff(): boolean {
  return pendingInstallHandoff !== null;
}

export function clearInstallHandoff(_reason?: string): void {
  pendingInstallHandoff = null;
}

export const initialInstallMachineState: InstallMachineState = {
  phase: "idle",
  snapshot: null,
  selectedAgentIds: [],
  selections: {},
  reviewDiscoveryRevision: null,
  message: "",
};

let installMachineState = initialInstallMachineState;
const installMachineListeners = new Set<() => void>();

/**
 * The install transaction outlives the Install page. Keeping this store at
 * module scope prevents navigation from resetting an in-flight native write
 * to `idle` while the native transaction continues in the background.
 */
export function getInstallMachineState(): InstallMachineState {
  return installMachineState;
}

export function subscribeInstallMachine(listener: () => void): () => void {
  installMachineListeners.add(listener);
  return () => installMachineListeners.delete(listener);
}

export function dispatchInstallMachine(event: InstallMachineEvent): void {
  if (event.type === "TOGGLE_AGENT" || event.type === "SET_AGENT_SELECTION" || event.type === "RESET") {
    clearInstallHandoff("selection-changed");
  }
  const next = installMachineReducer(installMachineState, event);
  if (next === installMachineState) return;
  installMachineState = next;
  installMachineListeners.forEach((listener) => listener());
}

export function resetInstallMachineState(): void {
  clearInstallHandoff("reset");
  dispatchInstallMachine({ type: "RESET" });
}

export function installMachineReducer(state: InstallMachineState, event: InstallMachineEvent): InstallMachineState {
  switch (event.type) {
    case "DISCOVER_START":
      return state.phase === "idle" || state.phase === "blocked"
        ? { ...state, phase: "discovering", message: "" }
        : state;
    case "DISCOVERED": {
      const revisionChanged = state.snapshot !== null && state.snapshot.revision !== event.snapshot.revision;
      // First discovery defaults all actionable installed Agents into the
      // batch. Later refreshes preserve the user's explicit batch scope.
      const nextActionableIds = new Set(actionableBindings(event.snapshot).map((binding) => binding.agent_id));
      const selectedAgentIds = state.snapshot === null
        ? defaultSelectedAgentIds(event.snapshot)
        : state.selectedAgentIds.filter((id) => nextActionableIds.has(id));
      const selections = mergeSelections(event.snapshot, revisionChanged ? {} : state.selections);
      const nextPhase = deriveInstallPhase(event.snapshot, selections, selectedAgentIds);
      if (!revisionChanged && ["review", "applying-runtime", "applying-agent-configs", "verifying", "verified", "partial", "blocked", "rolled-back"].includes(state.phase)) {
        return { ...state, snapshot: event.snapshot, selectedAgentIds, selections };
      }
      return {
        ...state,
        snapshot: event.snapshot,
        selectedAgentIds,
        selections,
        phase: nextPhase,
        reviewDiscoveryRevision: null,
        message: revisionChanged && state.reviewDiscoveryRevision
          ? "source-config-changed"
          : "",
      };
    }
    case "TOGGLE_AGENT": {
      const actionable = state.snapshot ? actionableBindings(state.snapshot) : [];
      if (!actionable.some((binding) => binding.agent_id === event.agentId)) return state;
      const selectedAgentIds = state.selectedAgentIds.includes(event.agentId)
        ? state.selectedAgentIds.filter((id) => id !== event.agentId)
        : [...state.selectedAgentIds, event.agentId];
      return {
        ...state,
        selectedAgentIds,
        phase: state.snapshot ? deriveInstallPhase(state.snapshot, state.selections, selectedAgentIds) : state.phase,
        reviewDiscoveryRevision: null,
        message: "",
      };
    }
    case "SET_AGENT_SELECTION": {
      const actionableIds = new Set((state.snapshot ? actionableBindings(state.snapshot) : []).map((binding) => binding.agent_id));
      const selectedAgentIds = event.agentIds.filter((id) => actionableIds.has(id));
      return {
        ...state,
        selectedAgentIds,
        phase: state.snapshot ? deriveInstallPhase(state.snapshot, state.selections, selectedAgentIds) : state.phase,
        reviewDiscoveryRevision: null,
        message: "",
      };
    }
    case "DISCOVERY_FAILED":
      if (["applying-runtime", "applying-agent-configs", "verifying"].includes(state.phase)) return state;
      return { ...state, phase: "blocked", message: event.message, reviewDiscoveryRevision: null };
    case "OPEN_REVIEW":
      return state.snapshot && canReviewInstall(state.snapshot, state.selections, state.selectedAgentIds)
        ? { ...state, phase: "review", reviewDiscoveryRevision: state.snapshot.revision, message: "" }
        : state;
    case "SET_PHASE":
      return { ...state, phase: event.phase, message: event.message ?? "" };
    case "RESET":
      return initialInstallMachineState;
  }
}

export function candidateKey(candidate: UpstreamCandidate, model: string) {
  return `${candidate.source}::${candidate.id}::${model}`;
}

export function bindingOptions(binding: AgentUpstreamBinding) {
  return binding.candidates.flatMap((candidate) => {
    const models = candidate.models.length > 0 ? candidate.models : [""];
    return models.map((model) => ({
      key: candidateKey(candidate, model),
      candidate,
      model,
    }));
  });
}

export function selectedCandidate(binding: AgentUpstreamBinding, selection?: AgentSelection) {
  if (!selection) return null;
  const resolved = binding.resolved_candidate;
  if (!resolved || candidateKey(resolved, selection.model) !== selection.candidateKey) return null;
  if (!resolved.models.includes(selection.model)
    && !(binding.agent_id === "codex" && resolved.protocol === "responses" && resolved.models.length === 0 && selection.model === "")) return null;
  return { key: selection.candidateKey, candidate: resolved, model: selection.model };
}

export type ResolvedAgentBinding = {
  candidate: UpstreamCandidate;
  /** The unique auto-resolved model; "" means request-model passthrough. */
  model: string;
};

export type InstallSelectedPreparation = {
  agentId: string;
  agentName: string;
  binding: AgentUpstreamBinding;
  ready: boolean;
  reason: "ready" | "already-connected" | "credential-required" | "ambiguous-route" | "model-unresolved" | "safe-adapter-unavailable";
};

export function requiresAgentRouteRepair(binding: AgentUpstreamBinding): boolean {
  return binding.follow?.transport_out_of_sync === true
    || binding.follow?.state === "transport_out_of_sync";
}

/**
 * The only model d-token may ever write is the uniquely auto-resolved one
 * (PRD P0.0.10.2/.5): exactly one model on the resolved candidate, or the
 * empty passthrough marker for the Codex Responses proxy. Every other shape
 * (multiple models, or none outside the passthrough) is ambiguous and fails
 * closed to null — callers must block, never guess or pick models[0].
 */
export function resolveAgentBinding(binding: AgentUpstreamBinding, selection?: AgentSelection): ResolvedAgentBinding | null {
  const candidate = binding.resolved_candidate;
  if (!candidate) return null;
  if (selection && !selectedCandidate(binding, selection)) return null;
  if (candidate.models.length === 1) return { candidate, model: candidate.models[0] };
  if (candidate.models.length === 0 && binding.agent_id === "codex" && candidate.protocol === "responses") {
    return { candidate, model: "" };
  }
  return null;
}

export function prepareSelectedInstallBindings(
  snapshot: ConnectionSetupSnapshot,
  selections: Record<string, AgentSelection>,
  selectedAgentIds = defaultSelectedAgentIds(snapshot),
): InstallSelectedPreparation[] {
  return selectedBindings(snapshot, selectedAgentIds).map((binding) => {
    const base = { agentId: binding.agent_id, agentName: binding.agent_name, binding };
    if (binding.already_connected && !requiresAgentRouteRepair(binding)) {
      return { ...base, ready: true, reason: "already-connected" as const };
    }
    if (binding.resolution === "needs-credential") return { ...base, ready: false, reason: "credential-required" as const };
    if (binding.resolution === "needs-choice") return { ...base, ready: false, reason: "ambiguous-route" as const };
    if (binding.resolution !== "auto-resolved") return { ...base, ready: false, reason: "safe-adapter-unavailable" as const };
    if (!resolveAgentBinding(binding, selections[binding.agent_id])) {
      return { ...base, ready: false, reason: "model-unresolved" as const };
    }
    return { ...base, ready: true, reason: "ready" as const };
  });
}

export function canReviewInstall(snapshot: ConnectionSetupSnapshot, selections: Record<string, AgentSelection>, selectedAgentIds = defaultSelectedAgentIds(snapshot)) {
  return prepareSelectedInstallBindings(snapshot, selections, selectedAgentIds).some((item) => item.ready);
}

export function deriveInstallPhase(snapshot: ConnectionSetupSnapshot, selections: Record<string, AgentSelection>, selectedAgentIds = defaultSelectedAgentIds(snapshot)): InstallPhase {
  const installed = snapshot.agent_bindings.filter((binding) => binding.installed);
  const actionable = selectedBindings(snapshot, selectedAgentIds);
  if (installed.length === 0) return "blocked";
  // Detection-only Agents still have explicit Configure/Clear controls in the
  // UI, but they cannot enter a route batch until native discovery proves a
  // verified transport adapter.
  if (actionable.length === 0) return "partial";
  const preparation = prepareSelectedInstallBindings(snapshot, selections, selectedAgentIds);
  if (preparation.every((item) => item.reason === "already-connected")) return "already-connected";
  if (preparation.some((item) => item.ready)) return "ready";
  if (preparation.some((item) => item.reason === "credential-required")) return "needs-credential";
  return "blocked";
}

/// Bindings that are eligible for transport-level connection. A historical
/// prompt/manual registry label must not override a verified native transport
/// adapter; only Agents without such an adapter stay suggestion-only.
function actionableBindings(snapshot: ConnectionSetupSnapshot) {
  return snapshot.agent_bindings.filter(canJoinInstallBatch);
}

export function canJoinInstallBatch(binding: AgentUpstreamBinding) {
  // MCP/Hook agents provide only tool-level integration, not route evidence.
  // They are not selectable for the batch route install plan.
  if (binding.integration_kind === "mcp" || binding.integration_kind === "hook") {
    return false;
  }
  return binding.installed
    && (binding.already_connected || binding.can_configure === true);
}

function selectedBindings(snapshot: ConnectionSetupSnapshot, selectedAgentIds: string[]) {
  const selected = new Set(selectedAgentIds);
  return actionableBindings(snapshot).filter((binding) => selected.has(binding.agent_id));
}

function defaultSelectedAgentIds(snapshot: ConnectionSetupSnapshot) {
  // The whole batch is automatic: every installed Agent with a verified write
  // action joins the plan. Resolution gaps (credential/ambiguity) then decide
  // whether the batch is ready, needs a credential, or is blocked.
  return actionableBindings(snapshot).map((binding) => binding.agent_id);
}

function mergeSelections(snapshot: ConnectionSetupSnapshot, current: Record<string, AgentSelection>) {
  const selections: Record<string, AgentSelection> = {};
  for (const binding of actionableBindings(snapshot)) {
    const existing = current[binding.agent_id];
    if (existing) {
      // Preserve a previous selection only while it still matches the current
      // unique auto-resolution; a changed source invalidates it.
      const resolved = resolveAgentBinding(binding, existing);
      if (resolved && resolved.model === existing.model) {
        selections[binding.agent_id] = existing;
        continue;
      }
    }
    const resolved = resolveAgentBinding(binding);
    if (!resolved) continue;
    selections[binding.agent_id] = {
      candidateKey: candidateKey(resolved.candidate, resolved.model),
      model: resolved.model,
    };
  }
  return selections;
}

export async function runInstallAgentBatch(
  items: InstallAgentBatchItem[],
  operations: InstallAgentBatchOperations,
): Promise<InstallAgentBatchResult> {
  const receipts: InstallAgentReceipt[] = [];
  let error = "";

  for (const item of items) {
    if (item.alreadyConnected) {
      receipts.push({
        agentId: item.agentId,
        agentName: item.agentName,
        state: "already-connected",
        summary: "Agent already points to d-token",
      });
      continue;
    }
    if (!item.reviewDigest) {
      error = `Missing reviewed Agent integration for ${item.agentName}`;
      receipts.push({ ...item, state: "failed", summary: error });
      continue;
    }

    try {
      const applied = await operations.apply(item);
      if (!applied.transactionId) {
        throw new Error(applied.summary || `Agent integration apply failed for ${item.agentName}`);
      }
      const receipt: InstallAgentReceipt = {
        agentId: item.agentId,
        agentName: item.agentName,
        state: "applied",
        transactionId: applied.transactionId,
        summary: applied.summary,
      };
      receipts.push(receipt);
      if (!applied.ok) {
        throw new Error(applied.summary || `Agent integration apply failed for ${item.agentName}`);
      }
      const verified = await operations.verify(receipt);
      if (!verified.ok) {
        receipt.state = "failed";
        receipt.summary = verified.summary || `Agent integration verification failed for ${item.agentName}`;
        error = receipt.summary;
        try {
          const restored = await operations.rollback(receipt);
          receipt.state = restored.ok ? "rolled-back" : "rollback-failed";
          receipt.summary = restored.summary || receipt.summary;
        } catch (reason) {
          receipt.state = "rollback-failed";
          receipt.summary = reason instanceof Error ? reason.message : String(reason);
        }
        continue;
      }
      receipt.state = "verified";
      receipt.summary = verified.summary;
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
      const current = receipts.find((receipt) => receipt.agentId === item.agentId);
      if (!current) receipts.push({ ...item, state: "failed", summary: error });
      else {
        current.state = "failed";
        current.summary = error;
        if (current.transactionId) {
          try {
            const restored = await operations.rollback(current);
            current.state = restored.ok ? "rolled-back" : "rollback-failed";
            current.summary = restored.summary || current.summary;
          } catch (rollbackReason) {
            current.state = "rollback-failed";
            current.summary = rollbackReason instanceof Error ? rollbackReason.message : String(rollbackReason);
          }
        }
      }
      continue;
    }
  }

  return error ? { ok: false, receipts, error } : { ok: true, receipts, error: "" };
}
