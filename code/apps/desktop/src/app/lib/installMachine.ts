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
  state: "already-connected" | "applied" | "verified" | "failed" | "rolled-back" | "rollback-failed";
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
  const next = installMachineReducer(installMachineState, event);
  if (next === installMachineState) return;
  installMachineState = next;
  installMachineListeners.forEach((listener) => listener());
}

export function resetInstallMachineState(): void {
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
      // The batch is automatic: every installed Agent with a verified write
      // action joins. There is no user opt-in, so selection always follows
      // discovery and never needs to be preserved across polling.
      const selectedAgentIds = defaultSelectedAgentIds(event.snapshot);
      const selections = mergeSelections(event.snapshot, revisionChanged ? {} : state.selections);
      const nextPhase = deriveInstallPhase(event.snapshot, selections, selectedAgentIds);
      if (!revisionChanged && ["review", "applying-runtime", "applying-agent-configs", "verifying", "verified"].includes(state.phase)) {
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
  return bindingOptions(binding).find((option) => option.key === selection.candidateKey) ?? null;
}

export type ResolvedAgentBinding = {
  candidate: UpstreamCandidate;
  /** The unique auto-resolved model; "" means request-model passthrough. */
  model: string;
};

/**
 * The only model d-token may ever write is the uniquely auto-resolved one
 * (PRD P0.0.10.2/.5): exactly one model on the resolved candidate, or the
 * empty passthrough marker for the Codex Responses proxy. Every other shape
 * (multiple models, or none outside the passthrough) is ambiguous and fails
 * closed to null — callers must block, never guess or pick models[0].
 */
export function resolveAgentBinding(binding: AgentUpstreamBinding, selection?: AgentSelection): ResolvedAgentBinding | null {
  const chosen = selection ? selectedCandidate(binding, selection) : null;
  const candidate = chosen?.candidate ?? binding.resolved_candidate;
  if (!candidate) return null;
  if (candidate.models.length === 1) return { candidate, model: candidate.models[0] };
  if (candidate.models.length === 0 && binding.agent_id === "codex" && candidate.protocol === "responses") {
    return { candidate, model: "" };
  }
  return null;
}

export function canReviewInstall(snapshot: ConnectionSetupSnapshot, selections: Record<string, AgentSelection>, selectedAgentIds = defaultSelectedAgentIds(snapshot)) {
  const actionable = selectedBindings(snapshot, selectedAgentIds);
  if (actionable.length === 0) return false;
  return actionable.every((binding) => {
    if (binding.already_connected) return true;
    if (binding.resolution === "needs-credential") return false;
    // Ambiguity is never resolvable by the user; the plan fails closed.
    if (binding.resolution === "needs-choice") return false;
    // A selection exists only when the resolved candidate exposes exactly one
    // model (or the Codex Responses passthrough marker). Multi-model or
    // unresolvable bindings have no selection and block the plan.
    if (binding.resolution === "auto-resolved") return resolveAgentBinding(binding, selections[binding.agent_id]) !== null;
    return false;
  });
}

export function deriveInstallPhase(snapshot: ConnectionSetupSnapshot, selections: Record<string, AgentSelection>, selectedAgentIds = defaultSelectedAgentIds(snapshot)): InstallPhase {
  const installed = snapshot.agent_bindings.filter((binding) => binding.installed);
  const actionable = selectedBindings(snapshot, selectedAgentIds);
  if (installed.length === 0) return "blocked";
  // Detection-only Agents still have explicit Configure/Clear controls in the
  // UI, but they cannot enter a route batch until native discovery proves a
  // verified transport adapter.
  if (actionable.length === 0) return "partial";
  if (actionable.every((binding) => binding.already_connected)) return "already-connected";
  if (actionable.some((binding) => binding.resolution === "needs-credential")) return "needs-credential";
  // A genuinely ambiguous upstream/model blocks the whole plan until the
  // source is fixed and rescanned; no chooser is offered.
  if (actionable.some((binding) => binding.resolution === "needs-choice")) return "blocked";
  return canReviewInstall(snapshot, selections, selectedAgentIds) ? "ready" : "blocked";
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
      break;
    }

    try {
      const applied = await operations.apply(item);
      if (!applied.ok || !applied.transactionId) {
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
      const verified = await operations.verify(receipt);
      if (!verified.ok) throw new Error(verified.summary || `Agent integration verification failed for ${item.agentName}`);
      receipt.state = "verified";
      receipt.summary = verified.summary;
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
      const current = receipts.find((receipt) => receipt.agentId === item.agentId);
      if (!current) receipts.push({ ...item, state: "failed", summary: error });
      break;
    }
  }

  if (!error) return { ok: true, receipts, error: "" };

  for (const receipt of [...receipts].reverse()) {
    if (!receipt.transactionId || !["applied", "verified"].includes(receipt.state)) continue;
    try {
      const restored = await operations.rollback(receipt);
      receipt.state = restored.ok ? "rolled-back" : "rollback-failed";
      receipt.summary = restored.summary;
    } catch (reason) {
      receipt.state = "rollback-failed";
      receipt.summary = reason instanceof Error ? reason.message : String(reason);
    }
  }
  return { ok: false, receipts, error };
}
