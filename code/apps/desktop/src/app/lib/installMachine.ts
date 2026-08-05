import type { AgentUpstreamBinding, ConnectionSetupSnapshot, UpstreamCandidate } from "./connectionSetup";

export type InstallPhase =
  | "idle"
  | "discovering"
  | "already-connected"
  | "ready"
  | "needs-choice"
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
  | { type: "SELECT_UPSTREAM"; agentId: string; selection: AgentSelection }
  | { type: "TOGGLE_AGENT"; agentId: string; selected: boolean }
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
      const defaults = defaultSelectedAgentIds(event.snapshot);
      // Preserve explicit opt-ins across same-revision polling. Defaults are a
      // safe subset; they must not redefine which discovered Agents are
      // selectable after the user has deliberately included one.
      const selectable = new Set(selectableBindings(event.snapshot).map((binding) => binding.agent_id));
      const preserved = state.selectedAgentIds.filter((agentId) => selectable.has(agentId));
      const selectedAgentIds = state.snapshot !== null && !revisionChanged ? preserved : defaults;
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
    case "SELECT_UPSTREAM": {
      const selections = { ...state.selections, [event.agentId]: event.selection };
      return {
        ...state,
        selections,
        phase: state.snapshot ? deriveInstallPhase(state.snapshot, selections, state.selectedAgentIds) : "blocked",
        reviewDiscoveryRevision: null,
      };
    }
    case "TOGGLE_AGENT": {
      if (!state.snapshot || !selectableBindings(state.snapshot).some((binding) => binding.agent_id === event.agentId)) return state;
      const selectedAgentIds = event.selected
        ? Array.from(new Set([...state.selectedAgentIds, event.agentId]))
        : state.selectedAgentIds.filter((agentId) => agentId !== event.agentId);
      return {
        ...state,
        selectedAgentIds,
        phase: deriveInstallPhase(state.snapshot, state.selections, selectedAgentIds),
        reviewDiscoveryRevision: null,
        message: selectedAgentIds.length === 0 ? "select-at-least-one-agent" : "",
      };
    }
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

export function canReviewInstall(snapshot: ConnectionSetupSnapshot, selections: Record<string, AgentSelection>, selectedAgentIds = defaultSelectedAgentIds(snapshot)) {
  const actionable = selectedBindings(snapshot, selectedAgentIds);
  if (actionable.length === 0) return false;
  return actionable.every((binding) => {
    if (binding.already_connected) return true;
    if (binding.resolution === "needs-credential") return false;
    if (binding.resolution === "auto-resolved") return Boolean(selections[binding.agent_id]);
    if (binding.resolution === "needs-choice") return Boolean(selectedCandidate(binding, selections[binding.agent_id]));
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
  if (actionable.some((binding) => binding.resolution === "needs-choice" && !selectedCandidate(binding, selections[binding.agent_id]))) {
    return "needs-choice";
  }
  return canReviewInstall(snapshot, selections, selectedAgentIds) ? "ready" : "blocked";
}

/// Bindings that are eligible for transport-level connection. A historical
/// prompt/manual registry label must not override a verified native transport
/// adapter; only Agents without such an adapter stay suggestion-only.
function actionableBindings(snapshot: ConnectionSetupSnapshot) {
  return snapshot.agent_bindings.filter(canJoinInstallBatch);
}

/// Selection means "include in this verified connection batch". Detection-only
/// rows remain visible with Configure/Clear controls but do not get a checkbox
/// that would be silently ignored.
function selectableBindings(snapshot: ConnectionSetupSnapshot) {
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
  // Pre-select agents that are ready for one-shot confirmation:
  //   - already connected
  //   - one-click + auto-resolved
  //   - confirm-required + auto-resolved (the plan is ready, just needs user ok)
  return actionableBindings(snapshot)
    .filter((binding) => binding.already_connected
      || ((binding.capability === "one-click"
            || binding.capability === "confirm-required"
            || binding.can_configure === true)
          && binding.resolution === "auto-resolved"))
    .map((binding) => binding.agent_id);
}

function mergeSelections(snapshot: ConnectionSetupSnapshot, current: Record<string, AgentSelection>) {
  const selections: Record<string, AgentSelection> = {};
  for (const binding of actionableBindings(snapshot)) {
    const existing = current[binding.agent_id];
    if (existing && selectedCandidate(binding, existing)) {
      selections[binding.agent_id] = existing;
      continue;
    }
    const candidate = binding.resolved_candidate;
    if (candidate && candidate.models.length > 0) {
      selections[binding.agent_id] = {
        candidateKey: candidateKey(candidate, candidate.models[0]),
        model: candidate.models[0],
      };
    }
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
