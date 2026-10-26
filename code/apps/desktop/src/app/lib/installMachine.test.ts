import { describe, expect, it } from "vitest";
import {
  clearInstallHandoff,
  consumeInstallHandoff,
  dispatchInstallMachine,
  getInstallMachineState,
  initialInstallMachineState,
  installMachineReducer,
  requestInstallHandoff,
  resetInstallMachineState,
  runInstallAgentBatch,
  subscribeInstallMachine,
  type InstallMachineState,
} from "./installMachine";
import type { AgentUpstreamBinding, ConnectionSetupSnapshot, UpstreamCandidate } from "./connectionSetup";

const candidate = (overrides: Partial<UpstreamCandidate> = {}): UpstreamCandidate => ({
  id: "provider",
  name: "Current provider",
  protocol: "open_ai",
  base_url: "https://example.test/v1",
  models: ["model-a"],
  source: "agent-config:opencode",
  freshness: "unix:1",
  auth_mode: "none",
  active: true,
  state: "discovered",
  agent_ids: ["opencode"],
  confidence: 98,
  reasons: ["current"],
  ...overrides,
});

const binding = (overrides: Partial<AgentUpstreamBinding> = {}): AgentUpstreamBinding => {
  const resolved = candidate();
  return {
    agent_id: "opencode",
    agent_name: "OpenCode",
    installed: true,
    binary_path: "/usr/local/bin/opencode",
    config_path: "~/.config/opencode/opencode.json",
    current_config_source: "agent-config:opencode",
    candidates: [resolved],
    resolved_candidate: resolved,
    resolution: "auto-resolved",
    confidence: 98,
    reasons: ["current"],
    capability: "one-click",
    can_configure: true,
    can_clear: false,
    transport_supported: true,
    transport_status: "configure_only",
    already_connected: false,
    ...overrides,
  };
};

const snapshot = (revision: string, agentBinding = binding()): ConnectionSetupSnapshot => ({
  source: "dtoken-desktop-read-only-discovery",
  freshness: "unix:1",
  revision,
  state: "loaded",
  candidates: agentBinding.candidates,
  agent_bindings: [agentBinding],
  sources: [],
  selection_required: agentBinding.resolution === "needs-choice",
  cc_switch_sync: null,
  writes_performed: false,
  privacy: "redacted",
});

describe("installMachine", () => {

  it("stores an in-memory install handoff for one consume and clears it on navigation/reset", () => {
    const handoff = requestInstallHandoff({
      agentIds: ["opencode", "codex", "opencode", ""],
      discoveryRevision: "handoff-r1",
      source: "onboarding-start-setup",
      requestedAtMs: 42,
    });

    expect(handoff).toEqual({
      agentIds: ["opencode", "codex"],
      discoveryRevision: "handoff-r1",
      source: "onboarding-start-setup",
      requestedAtMs: 42,
    });
    expect(consumeInstallHandoff()).toEqual(handoff);
    expect(consumeInstallHandoff()).toBeNull();

    requestInstallHandoff({ agentIds: ["opencode"], discoveryRevision: "handoff-r2", source: "onboarding-start-setup" });
    clearInstallHandoff("navigation");
    expect(consumeInstallHandoff()).toBeNull();

    requestInstallHandoff({ agentIds: ["codex"], discoveryRevision: "handoff-r3", source: "onboarding-start-setup" });
    resetInstallMachineState();
    expect(consumeInstallHandoff()).toBeNull();
  });

  it("keeps transaction progress when the page subscriber unmounts and remounts", () => {
    resetInstallMachineState();
    const firstPageUpdates: string[] = [];
    const unsubscribe = subscribeInstallMachine(() => firstPageUpdates.push(getInstallMachineState().phase));

    dispatchInstallMachine({ type: "SET_PHASE", phase: "applying-runtime" });
    unsubscribe();
    dispatchInstallMachine({ type: "SET_PHASE", phase: "verifying" });

    expect(firstPageUpdates).toEqual(["applying-runtime"]);
    expect(getInstallMachineState().phase).toBe("verifying");

    const remountedPageSnapshot = getInstallMachineState();
    expect(remountedPageSnapshot.phase).toBe("verifying");
    resetInstallMachineState();
  });

  it("auto-resolves one high-confidence upstream without requiring Advanced setup", () => {
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1") });
    expect(state.phase).toBe("ready");
    expect(state.selections.opencode.model).toBe("model-a");
  });

  it("blocks the whole plan when an installed Agent's upstream/model is ambiguous", () => {
    const first = candidate({ id: "one", active: false });
    const second = candidate({ id: "two", active: false, models: ["model-b"] });
    const ambiguous = binding({ candidates: [first, second], resolved_candidate: null, resolution: "needs-choice" });
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", ambiguous) });
    // The ambiguous Agent joins the automatic batch and fails closed; no
    // chooser is offered.
    expect(state.selectedAgentIds).toEqual(["opencode"]);
    expect(state.phase).toBe("blocked");
  });

  it("fails closed on a resolved candidate with two models instead of locking the first model", () => {
    const multiModel = binding({ resolved_candidate: candidate({ models: ["model-a", "model-b"] }) });
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", multiModel) });
    expect(state.selectedAgentIds).toEqual(["opencode"]);
    expect(state.selections.opencode).toBeUndefined();
    expect(state.phase).toBe("blocked");
  });

  it("keeps Codex Responses empty models ready with an empty passthrough selection", () => {
    const codex = binding({
      agent_id: "codex",
      agent_name: "Codex",
      resolved_candidate: candidate({ id: "ocxlocal", protocol: "responses", models: [], agent_ids: ["codex"] }),
    });
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", codex) });
    expect(state.phase).toBe("ready");
    expect(state.selections.codex?.model).toBe("");
    expect(state.selectedAgentIds).toEqual(["codex"]);
  });

  it("keeps credential gaps and already-connected state distinct", () => {
    // Credential-gated Agents are part of the automatic batch and surface the
    // credential requirement without any user opt-in.
    const credential = installMachineReducer(initialInstallMachineState, {
      type: "DISCOVERED",
      snapshot: snapshot("r1", binding({ resolution: "needs-credential", resolved_candidate: candidate({ auth_mode: "unavailable" }) })),
    });
    expect(credential.phase).toBe("needs-credential");
    expect(credential.selectedAgentIds).toEqual(["opencode"]);

    const connected = installMachineReducer(initialInstallMachineState, {
      type: "DISCOVERED",
      snapshot: snapshot("r1", binding({ resolution: "already-connected", already_connected: true, resolved_candidate: null })),
    });
    expect(connected.phase).toBe("already-connected");
  });

  it("auto-includes an auto-resolved confirm-required Agent together with one-click Agents", () => {
    const current = snapshot("r1");
    current.agent_bindings.push(binding({
      agent_id: "claude-code",
      agent_name: "Claude Code",
      capability: "confirm-required",
      resolution: "auto-resolved",
      resolved_candidate: candidate({ id: "claude", models: ["claude-sonnet"], agent_ids: ["claude-code"] }),
    }));
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: current });
    expect(state.phase).toBe("ready");
    expect(state.selectedAgentIds).toEqual(["opencode", "claude-code"]);
    expect(state.selections["claude-code"].model).toBe("claude-sonnet");
  });

  it("treats a verified transport adapter as actionable despite an older prompt-only label", () => {
    const cline = binding({
      agent_id: "cline",
      agent_name: "Cline",
      capability: "prompt-only",
      can_configure: true,
      can_clear: true,
      resolution: "auto-resolved",
      resolved_candidate: candidate({
        id: "cline-provider",
        source: "agent-config:cline-cli-v1",
        agent_ids: ["cline"],
      }),
    });

    const state = installMachineReducer(initialInstallMachineState, {
      type: "DISCOVERED",
      snapshot: snapshot("r1", cline),
    });

    expect(state.phase).toBe("ready");
    expect(state.selectedAgentIds).toEqual(["cline"]);
    expect(state.selections.cline.model).toBe("model-a");
  });

  it("auto-includes every installed Agent with a verified write action", () => {
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1") });
    expect(state.phase).toBe("ready");
    expect(state.selectedAgentIds).toEqual(["opencode"]);
    expect(state.snapshot?.agent_bindings).toHaveLength(1);
  });

  it("keeps a credential-gated Agent selected without blocking ready siblings", () => {
    const current = snapshot("r1");
    current.agent_bindings.push(binding({
      agent_id: "second-agent",
      agent_name: "Second Agent",
      resolution: "needs-credential",
      resolved_candidate: candidate({ id: "second", auth_mode: "unavailable", agent_ids: ["second-agent"] }),
    }));
    let state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: current });
    expect(state.phase).toBe("ready");
    expect(state.selectedAgentIds).toEqual(["opencode", "second-agent"]);
    state = installMachineReducer(state, { type: "DISCOVERED", snapshot: current });
    expect(state.phase).toBe("ready");
    expect(state.selectedAgentIds).toEqual(["opencode", "second-agent"]);
  });

  it("preserves explicit deselection across a refreshed discovery revision", () => {
    const current = snapshot("r1");
    current.agent_bindings.push(binding({
      agent_id: "codex",
      agent_name: "Codex",
      resolved_candidate: candidate({ id: "codex-provider", agent_ids: ["codex"] }),
    }));
    let state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: current });
    expect(state.selectedAgentIds).toEqual(["opencode", "codex"]);

    state = installMachineReducer(state, { type: "TOGGLE_AGENT", agentId: "codex" });
    expect(state.selectedAgentIds).toEqual(["opencode"]);

    const refreshed = { ...current, revision: "r2" };
    state = installMachineReducer(state, { type: "DISCOVERED", snapshot: refreshed });
    expect(state.selectedAgentIds).toEqual(["opencode"]);

    state = installMachineReducer(state, { type: "SET_AGENT_SELECTION", agentIds: [] });
    expect(state.selectedAgentIds).toEqual([]);
    expect(state.phase).toBe("partial");
  });

  it("keeps a non-one-click Agent in the already-connected state across refreshes", () => {
    const connectedBinding = binding({ capability: "confirm-required", resolution: "already-connected", already_connected: true, resolved_candidate: null });
    let state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", connectedBinding) });
    expect(state.phase).toBe("already-connected");
    state = installMachineReducer(state, { type: "DISCOVERED", snapshot: snapshot("r1", connectedBinding) });
    expect(state.phase).toBe("already-connected");
  });

  it("invalidates a reviewed plan when the discovery revision changes", () => {
    let state: InstallMachineState = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1") });
    state = installMachineReducer(state, { type: "OPEN_REVIEW" });
    expect(state.phase).toBe("review");
    state = installMachineReducer(state, { type: "DISCOVERED", snapshot: snapshot("r2") });
    expect(state.phase).toBe("ready");
    expect(state.reviewDiscoveryRevision).toBeNull();
    expect(state.message).toBe("source-config-changed");
  });

  it("does not erase an in-flight native transaction on a transient discovery failure", () => {
    const applying = installMachineReducer(initialInstallMachineState, {
      type: "SET_PHASE",
      phase: "applying-runtime",
    });
    const state = installMachineReducer(applying, {
      type: "DISCOVERY_FAILED",
      message: "connection_setup_unavailable",
    });

    expect(state).toBe(applying);
    expect(state.phase).toBe("applying-runtime");
  });

  it("applies and verifies every selected Agent with one receipt per Agent", async () => {
    const applied: string[] = [];
    const result = await runInstallAgentBatch([
      { agentId: "opencode", agentName: "OpenCode", alreadyConnected: false, reviewDigest: "digest-open" },
      { agentId: "codex", agentName: "Codex", alreadyConnected: false, reviewDigest: "digest-codex" },
      { agentId: "claude-code", agentName: "Claude Code", alreadyConnected: true },
    ], {
      apply: async (item) => {
        applied.push(item.agentId);
        return { ok: true, transactionId: `tx-${item.agentId}`, summary: "applied" };
      },
      verify: async () => ({ ok: true, summary: "verified" }),
      rollback: async () => ({ ok: true, summary: "restored" }),
    });

    expect(result.ok).toBe(true);
    expect(applied).toEqual(["opencode", "codex"]);
    expect(result.receipts).toEqual([
      expect.objectContaining({ agentId: "opencode", state: "verified", transactionId: "tx-opencode" }),
      expect.objectContaining({ agentId: "codex", state: "verified", transactionId: "tx-codex" }),
      expect.objectContaining({ agentId: "claude-code", state: "already-connected" }),
    ]);
  });

  it("rolls back only the Agent that fails verification and keeps later Agents independent", async () => {
    const rolledBack: string[] = [];
    const result = await runInstallAgentBatch([
      { agentId: "opencode", agentName: "OpenCode", alreadyConnected: false, reviewDigest: "digest-open" },
      { agentId: "codex", agentName: "Codex", alreadyConnected: false, reviewDigest: "digest-codex" },
      { agentId: "claude-code", agentName: "Claude Code", alreadyConnected: false, reviewDigest: "digest-claude" },
    ], {
      apply: async (item) => ({ ok: true, transactionId: `tx-${item.agentId}`, summary: "applied" }),
      verify: async (receipt) => ({ ok: receipt.agentId !== "codex", summary: receipt.agentId === "codex" ? "source hash changed" : "verified" }),
      rollback: async (receipt) => {
        rolledBack.push(receipt.agentId);
        return { ok: true, summary: "restored" };
      },
    });

    expect(result.ok).toBe(false);
    expect(result.error).toBe("source hash changed");
    expect(rolledBack).toEqual(["codex"]);
    expect(result.receipts).toEqual([
      expect.objectContaining({ agentId: "opencode", state: "verified" }),
      expect.objectContaining({ agentId: "codex", state: "rolled-back" }),
      expect.objectContaining({ agentId: "claude-code", state: "verified" }),
    ]);
  });

  it("recovers a partial apply receipt before continuing with the next Agent", async () => {
    const restored: string[] = [];
    const result = await runInstallAgentBatch([
      { agentId: "opencode", agentName: "OpenCode", alreadyConnected: false, reviewDigest: "open" },
      { agentId: "codex", agentName: "Codex", alreadyConnected: false, reviewDigest: "codex" },
    ], {
      apply: async (item) => ({ ok: item.agentId !== "opencode", transactionId: `tx-${item.agentId}`, summary: "partial apply" }),
      verify: async () => ({ ok: true, summary: "verified" }),
      rollback: async (receipt) => { restored.push(receipt.transactionId!); return { ok: true, summary: "restored" }; },
    });
    expect(result.ok).toBe(false);
    expect(restored).toEqual(["tx-opencode"]);
    expect(result.receipts.map((receipt) => receipt.state)).toEqual(["rolled-back", "verified"]);
  });
});
