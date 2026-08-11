import { describe, expect, it } from "vitest";
import {
  dispatchInstallMachine,
  getInstallMachineState,
  initialInstallMachineState,
  installMachineReducer,
  resetInstallMachineState,
  resolveAgentBindings,
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
  has_resolution_gap: agentBinding.resolution === "needs-source-fix",
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

  it("auto-resolves the OpenCode catalog without a chooser and keeps no single selection", () => {
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1") });
    expect(state.phase).toBe("ready");
    // OpenCode is an intentional multi-provider catalog: the machine keeps no
    // single-model selection; route rows come from resolveAgentBindings.
    expect(state.selections.opencode).toBeUndefined();
    expect(resolveAgentBindings(snapshot("r1").agent_bindings[0])).toEqual([
      { candidate: expect.objectContaining({ id: "provider" }), model: "model-a" },
    ]);
  });

  it("blocks the whole plan when an installed Agent's upstream/model is ambiguous", () => {
    const first = candidate({ id: "one", active: false });
    const second = candidate({ id: "two", active: false, models: ["model-b"] });
    const ambiguous = binding({ candidates: [first, second], resolved_candidate: null, resolution: "needs-source-fix" });
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", ambiguous) });
    // The ambiguous Agent joins the automatic batch and fails closed; no
    // chooser is offered.
    expect(state.selectedAgentIds).toEqual(["opencode"]);
    expect(state.phase).toBe("blocked");
  });

  it("fails closed on a non-OpenCode resolved candidate with two models instead of locking the first model", () => {
    const multiModel = binding({
      agent_id: "cline",
      agent_name: "Cline",
      resolved_candidate: candidate({ id: "cline-provider", models: ["model-a", "model-b"], agent_ids: ["cline"] }),
    });
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", multiModel) });
    expect(state.selectedAgentIds).toEqual(["cline"]);
    expect(state.selections.cline).toBeUndefined();
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

  it("includes every safe OpenCode catalog candidate/model in stable order without resolved_candidate limits", () => {
    const free = candidate({
      id: "opencode-free",
      name: "OpenCode free",
      source: "opencode-provider-catalog-cache",
      models: ["deepseek-v4-flash-free", "north-mini-code-free"],
      auth_mode: "none",
    });
    const go = candidate({
      id: "opencode-go",
      name: "OpenCode Go",
      source: "opencode-provider-catalog-cache",
      models: ["go-model"],
      auth_mode: "unavailable",
      dtoken_credential_configured: true,
    });
    const noBaseUrl = candidate({
      id: "no-base",
      name: "No base URL",
      source: "opencode-provider-catalog-cache",
      models: ["ghost-model"],
      auth_mode: "none",
      base_url: null,
    });
    const openCode = binding({
      candidates: [go, free, noBaseUrl],
      // The resolved candidate (here the Go route) never narrows the catalog.
      resolved_candidate: go,
    });
    expect(resolveAgentBindings(openCode)).toEqual([
      { candidate: free, model: "deepseek-v4-flash-free" },
      { candidate: go, model: "go-model" },
      { candidate: free, model: "north-mini-code-free" },
    ]);
  });

  it("includes authenticated OpenCode candidates only when the d-token credential is configured", () => {
    const free = candidate({ id: "free", source: "opencode-provider-catalog-cache", models: ["free-model"], auth_mode: "none" });
    const configured = candidate({ id: "go", source: "opencode-provider-catalog-cache", models: ["go-model"], auth_mode: "unavailable", dtoken_credential_configured: true });
    const missing = candidate({ id: "paid", source: "opencode-provider-catalog-cache", models: ["paid-model"], auth_mode: "unavailable", dtoken_credential_configured: false });
    const absentFlag = candidate({ id: "legacy", source: "opencode-provider-catalog-cache", models: ["legacy-model"], auth_mode: "unavailable" });
    const openCode = binding({ candidates: [missing, configured, absentFlag, free] });
    expect(resolveAgentBindings(openCode).map((entry) => entry.model)).toEqual(["free-model", "go-model"]);
  });

  it("fails closed when the same OpenCode model maps to different candidate/endpoint", () => {
    const first = candidate({ id: "one", source: "opencode-provider-catalog-cache", models: ["shared-model"] });
    const second = candidate({
      id: "two",
      source: "opencode-provider-catalog-cache",
      models: ["shared-model"],
      base_url: "https://other.example.test/v1",
    });
    const openCode = binding({ candidates: [first, second] });
    expect(resolveAgentBindings(openCode)).toEqual([]);
  });

  it("dedupes the same OpenCode model repeated within one candidate", () => {
    const repeated = candidate({ id: "dup", source: "opencode-provider-catalog-cache", models: ["model-a", "model-a"] });
    expect(resolveAgentBindings(binding({ candidates: [repeated] }))).toEqual([
      { candidate: repeated, model: "model-a" },
    ]);
  });

  it("preserves the single non-OpenCode resolution including the Codex empty passthrough", () => {
    const cline = binding({
      agent_id: "cline",
      agent_name: "Cline",
      resolved_candidate: candidate({ id: "cl", agent_ids: ["cline"] }),
    });
    expect(resolveAgentBindings(cline)).toEqual([
      { candidate: expect.objectContaining({ id: "cl" }), model: "model-a" },
    ]);
    const codex = binding({
      agent_id: "codex",
      agent_name: "Codex",
      resolved_candidate: candidate({ id: "ocxlocal", protocol: "responses", models: [], agent_ids: ["codex"] }),
    });
    expect(resolveAgentBindings(codex)).toEqual([
      { candidate: expect.objectContaining({ id: "ocxlocal" }), model: "" },
    ]);
  });

  it("keeps OpenCode ready when a safe free route exists alongside an unconfigured credential candidate", () => {
    const free = candidate({
      id: "opencode-free",
      name: "OpenCode free",
      source: "opencode-provider-catalog-cache",
      models: ["deepseek-v4-flash-free"],
      auth_mode: "none",
    });
    const go = candidate({
      id: "opencode-go",
      name: "OpenCode Go",
      source: "opencode-provider-catalog-cache",
      models: ["go-model"],
      auth_mode: "unavailable",
      dtoken_credential_configured: false,
    });
    const mixed = binding({ candidates: [free, go], resolved_candidate: go, resolution: "needs-credential" });
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", mixed) });
    expect(state.phase).toBe("ready");
    expect(resolveAgentBindings(mixed).map((entry) => entry.model)).toEqual(["deepseek-v4-flash-free"]);
  });

  it("keeps the credential dialog when no OpenCode safe route exists", () => {
    const go = candidate({
      id: "opencode-go",
      name: "OpenCode Go",
      source: "opencode-provider-catalog-cache",
      models: ["go-model"],
      auth_mode: "unavailable",
      dtoken_credential_configured: false,
    });
    const onlyCredential = binding({ candidates: [go], resolved_candidate: go, resolution: "needs-credential" });
    const state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: snapshot("r1", onlyCredential) });
    expect(state.phase).toBe("needs-credential");
  });

  it("keeps credential gaps and already-connected state distinct", () => {
    // Credential-gated Agents are part of the automatic batch and surface the
    // credential requirement without any user opt-in. The candidate is
    // explicitly auth_mode "unavailable" with no configured d-token
    // credential, and it is the binding's only candidate — with no safe route
    // present, the credential requirement is preserved instead of degrading
    // to a ready OpenCode catalog.
    const credentialCandidate = candidate({ auth_mode: "unavailable", dtoken_credential_configured: false });
    const credential = installMachineReducer(initialInstallMachineState, {
      type: "DISCOVERED",
      snapshot: snapshot("r1", binding({ resolution: "needs-credential", candidates: [credentialCandidate], resolved_candidate: credentialCandidate })),
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

  it("auto-includes a credential-gated Agent and blocks readiness until the credential is added", () => {
    const current = snapshot("r1");
    current.agent_bindings.push(binding({
      agent_id: "second-agent",
      agent_name: "Second Agent",
      resolution: "needs-credential",
      resolved_candidate: candidate({ id: "second", auth_mode: "unavailable", agent_ids: ["second-agent"] }),
    }));
    let state = installMachineReducer(initialInstallMachineState, { type: "DISCOVERED", snapshot: current });
    expect(state.phase).toBe("needs-credential");
    expect(state.selectedAgentIds).toEqual(["opencode", "second-agent"]);
    state = installMachineReducer(state, { type: "DISCOVERED", snapshot: current });
    expect(state.phase).toBe("needs-credential");
    expect(state.selectedAgentIds).toEqual(["opencode", "second-agent"]);
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
