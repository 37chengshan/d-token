import { describe, expect, it } from "vitest";
import {
  candidateKey,
  dispatchInstallMachine,
  getInstallMachineState,
  initialInstallMachineState,
  installMachineReducer,
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

  it("requires a choice only for a genuinely ambiguous Agent", () => {
    const first = candidate({ id: "one", active: false });
    const second = candidate({ id: "two", active: false, models: ["model-b"] });
    const ambiguous = binding({ candidates: [first, second], resolved_candidate: null, resolution: "needs-choice" });
    let state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", ambiguous) });
    expect(state.phase).toBe("partial");
    state = installMachineReducer(state, { type: "TOGGLE_AGENT", agentId: "opencode", selected: true });
    expect(state.phase).toBe("needs-choice");
    state = installMachineReducer(state, {
      type: "SELECT_UPSTREAM",
      agentId: "opencode",
      selection: { candidateKey: candidateKey(second, "model-b"), model: "model-b" },
    });
    expect(state.phase).toBe("ready");
  });

  it("keeps credential gaps and already-connected state distinct", () => {
    let credential = installMachineReducer(initialInstallMachineState, {
      type: "DISCOVERED",
      snapshot: snapshot("r1", binding({ resolution: "needs-credential", resolved_candidate: candidate({ auth_mode: "unavailable" }) })),
    });
    // Credential-gated Agents are not selected by default; selecting one is
    // an explicit user action and then surfaces the credential requirement.
    expect(credential.phase).toBe("partial");
    credential = installMachineReducer(credential, { type: "TOGGLE_AGENT", agentId: "opencode", selected: true });
    expect(credential.phase).toBe("needs-credential");

    const connected = installMachineReducer(initialInstallMachineState, {
      type: "DISCOVERED",
      snapshot: snapshot("r1", binding({ resolution: "already-connected", already_connected: true, resolved_candidate: null })),
    });
    expect(connected.phase).toBe("already-connected");
  });

  it("pre-selects an auto-resolved confirm-required Agent together with one-click Agents", () => {
    const current = snapshot("r1");
    current.agent_bindings.push(binding({
      agent_id: "claude-code",
      agent_name: "Claude Code",
      capability: "confirm-required",
      resolution: "auto-resolved",
      resolved_candidate: candidate({ id: "claude", models: ["claude-sonnet", "claude-opus"], agent_ids: ["claude-code"] }),
    }));
    let state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: current });
    expect(state.phase).toBe("ready");
    // confirm-required + auto-resolved is pre-selected alongside one-click agents
    expect(state.selectedAgentIds).toEqual(["opencode", "claude-code"]);
    expect(state.selections["claude-code"].model).toBe("claude-sonnet");
    // unselecting confirm-required leaves the one-click agent still ready
    state = installMachineReducer(state, { type: "TOGGLE_AGENT", agentId: "claude-code", selected: false });
    expect(state.phase).toBe("ready");
    expect(state.selectedAgentIds).toEqual(["opencode"]);
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

  it("lets the user exclude a one-click Agent without losing discovered evidence", () => {
    let state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1") });
    state = installMachineReducer(state, { type: "TOGGLE_AGENT", agentId: "opencode", selected: false });
    expect(state.phase).toBe("partial");
    expect(state.selectedAgentIds).toEqual([]);
    expect(state.snapshot?.agent_bindings).toHaveLength(1);
  });

  it("recomputes readiness from the selected batch instead of all default one-click Agents", () => {
    const current = snapshot("r1");
    current.agent_bindings.push(binding({
      agent_id: "second-agent",
      agent_name: "Second Agent",
      resolution: "needs-credential",
      resolved_candidate: candidate({ id: "second", auth_mode: "unavailable", agent_ids: ["second-agent"] }),
    }));
    let state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: current });
    expect(state.phase).toBe("ready");
    expect(state.selectedAgentIds).toEqual(["opencode"]);
    state = installMachineReducer(state, { type: "TOGGLE_AGENT", agentId: "second-agent", selected: true });
    expect(state.phase).toBe("needs-credential");
    state = installMachineReducer(state, { type: "DISCOVERED", snapshot: current });
    expect(state.phase).toBe("needs-credential");
    expect(state.selectedAgentIds).toEqual(["opencode", "second-agent"]);
    state = installMachineReducer(state, { type: "TOGGLE_AGENT", agentId: "second-agent", selected: false });
    expect(state.phase).toBe("ready");
    expect(state.selectedAgentIds).toEqual(["opencode"]);
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

  it("rolls back every Agent already written when a later Agent fails verification", async () => {
    const rolledBack: string[] = [];
    const result = await runInstallAgentBatch([
      { agentId: "opencode", agentName: "OpenCode", alreadyConnected: false, reviewDigest: "digest-open" },
      { agentId: "codex", agentName: "Codex", alreadyConnected: false, reviewDigest: "digest-codex" },
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
    expect(rolledBack).toEqual(["codex", "opencode"]);
    expect(result.receipts).toEqual([
      expect.objectContaining({ agentId: "opencode", state: "rolled-back" }),
      expect.objectContaining({ agentId: "codex", state: "rolled-back" }),
    ]);
  });
});
