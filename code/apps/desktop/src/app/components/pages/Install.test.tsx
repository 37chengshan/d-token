import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invokeNative: vi.fn(),
  refreshRuntime: vi.fn(),
  runtime: {
    payload: null as Record<string, unknown> | null,
    status: "browser-preview",
    updatedAt: null as Date | null,
    error: "",
    operational: { service: { data: null as Record<string, unknown> | null } },
  },
}));

vi.mock("../../lib/native", () => ({
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
  invokeNative: (...args: unknown[]) => mocks.invokeNative(...args),
  isTauriRuntime: vi.fn().mockReturnValue(false),
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => ({
    ...mocks.runtime,
    refresh: mocks.refreshRuntime,
  }),
  useRuntimeDashboardSelector: (selector: (state: unknown) => unknown) =>
    selector({ ...mocks.runtime, refresh: mocks.refreshRuntime }),
  shallowEqualSlice: () => false,
}));

import { dispatchInstallMachine, resetInstallMachineState } from "../../lib/installMachine";

import {
  assertNativeActionSucceeded,
  connectionDiscoveryCounts,
  buildConnectionRoutePlanRequest,
  desktopInstallRoot,
  deriveServicePresentation,
  Install,
  presentConnectionPrivacy,
  presentRouteVerificationSummary,
  presentServiceInstance,
} from "./Install";

const connectedSnapshot = {
  source: "dtoken-desktop-read-only-discovery",
  freshness: "unix:1",
  revision: "restart-1",
  state: "loaded",
  candidates: [],
  agent_bindings: [{
    agent_id: "opencode",
    agent_name: "OpenCode",
    installed: true,
    current_config_source: "agent-config:opencode",
    candidates: [],
    resolved_candidate: null,
    resolution: "already-connected" as const,
    confidence: 100,
    reasons: ["connected"],
    capability: "one-click" as const,
    already_connected: true,
  }],
  sources: [],
  has_resolution_gap: false,
  cc_switch_sync: null,
  writes_performed: false as const,
  privacy: "redacted",
};

const credentialRequiredSnapshot = {
  source: "dtoken-desktop-read-only-discovery",
  freshness: "unix:2",
  revision: "credential-1",
  state: "loaded",
  candidates: [{
    id: "provider-needing-key",
    name: "Provider needing key",
    protocol: "open_ai",
    base_url: "https://example.test/v1",
    models: ["model-a"],
    source: "agent-config:opencode",
    freshness: "unix:2",
    auth_mode: "required",
    active: true,
    state: "discovered",
    agent_ids: ["opencode"],
    confidence: 98,
    reasons: ["credential missing"],
  }],
  agent_bindings: [{
    agent_id: "opencode",
    agent_name: "OpenCode",
    installed: true,
    current_config_source: "agent-config:opencode",
    candidates: [{
      id: "provider-needing-key",
      name: "Provider needing key",
      protocol: "open_ai",
      base_url: "https://example.test/v1",
      models: ["model-a"],
      source: "agent-config:opencode",
      freshness: "unix:2",
      auth_mode: "required",
      active: true,
      state: "discovered",
      agent_ids: ["opencode"],
      confidence: 98,
      reasons: ["credential missing"],
    }],
    resolved_candidate: {
      id: "provider-needing-key",
      name: "Provider needing key",
      protocol: "open_ai",
      base_url: "https://example.test/v1",
      models: ["model-a"],
      source: "agent-config:opencode",
      freshness: "unix:2",
      auth_mode: "required",
      active: true,
      state: "discovered",
      agent_ids: ["opencode"],
      confidence: 98,
      reasons: ["credential missing"],
    },
    resolution: "needs-credential" as const,
    confidence: 98,
    reasons: ["credential missing"],
    capability: "one-click" as const,
    can_configure: true,
    can_clear: false,
    transport_supported: true,
    transport_status: "configure_only" as const,
    already_connected: false,
  }],
  sources: [],
  has_resolution_gap: false,
  cc_switch_sync: null,
  writes_performed: false as const,
  privacy: "redacted",
};

beforeEach(() => {
  resetInstallMachineState();
  mocks.invokeNative.mockRejectedValue(new Error("native_unavailable"));
  mocks.refreshRuntime.mockResolvedValue(undefined);
  mocks.runtime.payload = null;
  mocks.runtime.status = "browser-preview";
  mocks.runtime.updatedAt = null;
  mocks.runtime.error = "";
  mocks.runtime.operational = { service: { data: null } };
});

afterEach(() => {
  cleanup();
  resetInstallMachineState();
  mocks.invokeNative.mockReset();
  mocks.refreshRuntime.mockReset();
});


describe("Install", () => {
  it("stops a configuration flow when a native transaction reports failure", () => {
    expect(() =>
      assertNativeActionSucceeded({
        ok: false,
        summary: "Provider route would loop back into d-token",
        logs: [],
        tools: [],
      }),
    ).toThrow("Provider route would loop back into d-token");
  });

  it("never presents an offline service as running while an install is in progress", () => {
    expect(deriveServicePresentation(false, true)).toEqual({
      isOnline: false,
      status: "warning",
      phase: "installing",
    });
    expect(deriveServicePresentation(false, false)).toEqual({
      isOnline: false,
      status: "error",
      phase: "stopped",
    });
    expect(deriveServicePresentation(true, false)).toEqual({
      isOnline: true,
      status: "healthy",
      phase: "running",
    });
  });

  it("never presents a stale manifest PID or URL as the current service instance", () => {
    const staleStatus = {
      state: "stopped",
      managed: false,
      pid: 91930,
      pid_running: false,
      manifest_stale: true,
      service_url: "http://127.0.0.1:18001",
    };

    const detail = presentServiceInstance(
      staleStatus,
      false,
      "",
      null,
      "zh",
    );

    expect(detail).toBe("历史运行记录已失效，当前实例标识已隐藏");
    expect(detail).not.toContain("91930");
    expect(detail).not.toContain("18001");
  });

  it("hides stale runtime identifiers in the rendered service row", async () => {
    mocks.runtime.payload = { desktop_service_url: "http://127.0.0.1:18001" };
    mocks.runtime.status = "stale";
    mocks.runtime.operational = {
      service: {
        data: {
          state: "stopped",
          managed: false,
          pid: 91930,
          pid_running: false,
          manifest_stale: true,
          service_url: "http://127.0.0.1:18001",
        },
      },
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return connectedSnapshot;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);

    expect(await screen.findByText("历史运行记录已失效，当前实例标识已隐藏")).toBeInTheDocument();
    expect(screen.queryByText(/PID 91930/)).not.toBeInTheDocument();
    expect(screen.queryByText(/http:\/\/127\.0\.0\.1:18001/)).not.toBeInTheDocument();
    expect(screen.getByText("未运行")).toBeInTheDocument();
  });

  it("reviews a cc-switch route sync before applying and reloading the managed runtime", async () => {
    const ccSwitchSnapshot = {
      ...connectedSnapshot,
      revision: "cc-switch-1",
      external_config_snapshot: {
        fingerprint: "aggregate-fingerprint",
        cc_switch: {
          fingerprint: "cc-fingerprint",
          revision: "cc-switch-1",
          selection_state: "resolved",
        },
        automatic_apply_allowed: false as const,
      },
      cc_switch_sync: {
        detected: true,
        status: "out-of-sync",
        source_state: "loaded",
        upstream: "https://cc-switch.example/v1",
        source: "cc-switch-read-only",
        writes_performed: false as const,
      },
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return ccSwitchSnapshot;
      if (command === "preview_cc_switch_sync_plan") {
        return {
          ok: true,
          state: "review_required",
          source_fingerprint: "cc-fingerprint",
          target: "~/.dtoken/active-config/dtoken-config.json",
          before_sha256: "before-sync",
          after_sha256: "after-sync",
          diff: "- old route\n+ new route",
        };
      }
      if (command === "apply_cc_switch_sync_plan") {
        return { ok: true, verified: true, state: "applied_verified", summary: "route applied" };
      }
      if (command === "reload_managed_runtime_after_route_sync") {
        return { ok: true, state: "runtime_reloaded", summary: "reloaded" };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);
    fireEvent.click(await screen.findByRole("button", { name: "同步当前上游" }));
    expect(await screen.findByText("同步 cc-switch 当前上游")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认同步并重新验证" }));

    await waitFor(() => {
      expect(mocks.invokeNative).toHaveBeenCalledWith("apply_cc_switch_sync_plan", {
        request: expect.objectContaining({
          plan: {
            discovery_revision: "cc-switch-1",
            source_fingerprint: "cc-fingerprint",
          },
          enable_auto_follow: true,
        }),
      });
      expect(mocks.invokeNative).toHaveBeenCalledWith("reload_managed_runtime_after_route_sync");
    });
  });

  it("includes a verified prompt-only Agent adapter in the one-click transport transaction", async () => {
    const clineCandidate = {
      id: "cline-openai-compatible",
      name: "Cline current provider",
      protocol: "open_ai",
      base_url: "https://cline.example.test/v1",
      models: ["cline-model"],
      source: "agent-config:cline-cli-v1",
      freshness: "unix:3",
      auth_mode: "none",
      active: true,
      state: "discovered",
      agent_ids: ["cline"],
      confidence: 99,
      reasons: ["current"],
    };
    const clineSnapshot = {
      ...connectedSnapshot,
      revision: "cline-route-1",
      candidates: [clineCandidate],
      agent_bindings: [{
        agent_id: "cline",
        agent_name: "Cline",
        installed: true,
        config_path: "~/.cline/settings/providers.json",
        current_config_source: "agent-config:cline-cli-v1",
        candidates: [clineCandidate],
        resolved_candidate: clineCandidate,
        resolution: "auto-resolved" as const,
        confidence: 99,
        reasons: ["current"],
        capability: "prompt-only" as const,
        can_configure: true,
        can_clear: false,
        already_connected: false,
      }],
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return clineSnapshot;
      if (command === "preview_connection_route_plan") {
        return {
          ok: true,
          state: "review_required",
          target: "dtoken-config.json",
          discovery_revision: "cline-route-1",
          before_sha256: "route-before",
          after_sha256: "route-after",
          diff: "+ Cline route",
        };
      }
      if (command === "preview_managed_agent_transport") {
        return {
          ok: true,
          review: {
            agent_id: "cline",
            target_path: "~/.cline/settings/providers.json",
            revision: "transport-revision",
            before_sha256: "transport-before",
            after_sha256: "transport-after",
            changed: true,
            redacted_diff: "+ d-token local provider",
          },
        };
      }
      if (command === "apply_connection_route_plan") {
        return {
          ok: true,
          state: "applied_verified",
          verified: true,
          summary: "route applied",
          rollback_id: "route-rollback",
          before_sha256: "route-before",
          after_sha256: "route-after",
          auto_follow: { enabled: false, state: "pending_runtime_verification" },
        };
      }
      if (command === "run_install_transaction") {
        return { ok: true, summary: "runtime started", logs: [], tools: [] };
      }
      if (command === "apply_reviewed_managed_agent_transport") {
        return { changed: true, verification: { ok: true, detail: "configured" } };
      }
      if (command === "verify_agent_route_config") {
        return { ok: true, passed: 6, total: 6, summary: "verified", checks: [] };
      }
      if (command === "finalize_agent_auto_follow_authorization") {
        return { ok: true, state: "authorized" };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);
    fireEvent.click(await screen.findByRole("button", { name: "查看变更" }));
    expect(await screen.findByText(/传输配置/)).toBeInTheDocument();
    expect(screen.getByText("+ d-token local provider")).toBeInTheDocument();
    // The confirm dialog states the rollback boundary alongside target/diff/hash.
    expect(screen.getByText(/应用前会创建备份与回退点/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认并连接" }));

    await waitFor(() => expect(mocks.invokeNative).toHaveBeenCalledWith(
      "apply_reviewed_managed_agent_transport",
      expect.objectContaining({
        operation: "configure",
        reviewedRevision: "transport-revision",
      }),
    ));
    await waitFor(() => expect(mocks.invokeNative).toHaveBeenCalledWith(
      "finalize_agent_auto_follow_authorization",
      { request: { expected_route_sha256: "route-after" } },
    ));
  });

  it("renders only verified current instance identifiers", () => {
    expect(presentServiceInstance(
      { state: "running", managed: true, pid: 80124, pid_running: true, manifest_stale: false },
      true,
      "http://127.0.0.1:18001",
      null,
      "en",
    )).toBe("PID 80124 · http://127.0.0.1:18001");
  });

  it("does not mislabel an unmanaged live listener as a stale historical process", () => {
    expect(presentServiceInstance(
      { state: "unmanaged_running", managed: false, pid: null, pid_running: false, manifest_stale: false },
      true,
      "",
      null,
      "zh",
    )).toBe("本地监听已验证，托管实例标识不可用");
  });

  it("uses the platform app-data location instead of a Windows path on macOS", () => {
    expect(desktopInstallRoot("Mozilla/5.0 (Macintosh; Intel Mac OS X)")).toBe("~/Library/Application Support/d-token");
    expect(desktopInstallRoot("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("%LOCALAPPDATA%\\d-token");
  });

  it("fails closed in browser preview without inventing Agent or service state", async () => {
    render(<Install />);

    expect(await screen.findByRole("button", { name: "重新扫描" })).toBeEnabled();
    expect(screen.getByText("需要处理")).toBeInTheDocument();
    expect(screen.queryByText("/usr/local/bin/codex")).not.toBeInTheDocument();
    expect(screen.getByText("隐私边界：未读取或显示凭据、认证文件、提示词或响应原文。")).toBeInTheDocument();
  });

  it("renders real one-click controls only for Agents with a verified write action", async () => {
    const agents: Array<[string, string, boolean, boolean]> = [
      ["opencode", "OpenCode", true, true],
      ["claude-code", "Claude Code", true, false],
      ["codex", "Codex", true, false],
      ["cursor", "Cursor", false, false],
      ["gemini-cli", "Gemini CLI", false, false],
      ["cline", "Cline", true, false],
      ["openclaw", "OpenClaw", true, false],
    ];
    const capabilitySnapshot = {
      ...connectedSnapshot,
      revision: "agent-operation-matrix",
      agent_bindings: agents.map(([agent_id, agent_name, can_configure, can_clear]) => ({
        agent_id,
        agent_name,
        installed: true,
        current_config_source: `agent-config:${agent_id}`,
        candidates: [],
        resolved_candidate: null,
        resolution: "already-connected" as const,
        confidence: 100,
        reasons: ["connected"],
        capability: "one-click" as const,
        can_configure,
        can_clear,
        already_connected: true,
      })),
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return capabilitySnapshot;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);

    expect(await screen.findByText("OpenClaw")).toBeInTheDocument();
    // Only Agents with a real backend write action get rows with buttons.
    const configureButtons = screen.getAllByRole("button", { name: "一键配置" });
    const clearButtons = screen.getAllByRole("button", { name: "清除 d-token 配置" });
    expect(configureButtons).toHaveLength(5);
    expect(clearButtons).toHaveLength(5);
    expect(configureButtons.filter((button) => (button as HTMLButtonElement).disabled)).toHaveLength(0);
    expect(clearButtons.filter((button) => (button as HTMLButtonElement).disabled)).toHaveLength(4);
    // Detection-only Agents are counted as evidence, never given fake buttons.
    expect(screen.queryByText("Cursor")).not.toBeInTheDocument();
    expect(screen.queryByText("Gemini CLI")).not.toBeInTheDocument();
    expect(screen.getByText("7 个已检测")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "高级配置" })).not.toBeInTheDocument();
    expect(screen.queryByText(/手动配置|需要手动|手动完成/)).not.toBeInTheDocument();
    // No dead-end copy is rendered anywhere.
    expect(screen.queryByText(/暂未开放|自动写入未开放|等待安全适配/)).not.toBeInTheDocument();

    // Clear stays fail-closed without an ownership receipt: clicking a disabled
    // Clear button never claims success and never reports a version dead-end.
    expect(clearButtons[1]).toBeDisabled();
    fireEvent.click(clearButtons[1]);
    expect(screen.queryByText(/当前版本暂未开放可验证的自动清除/)).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("auto-includes every installed Agent and makes View changes the primary action", async () => {
    const clineBinding = {
      ...connectedSnapshot.agent_bindings[0],
      agent_id: "cline",
      agent_name: "Cline",
      config_path: "~/.cline/settings/providers.json",
      candidates: [{
        id: "cline-provider",
        name: "Cline provider",
        protocol: "open_ai",
        base_url: "https://cline.example.test/v1",
        models: ["cline-model"],
        source: "agent-config:cline",
        freshness: "unix:4",
        auth_mode: "none",
        active: true,
        state: "discovered",
        agent_ids: ["cline"],
        confidence: 99,
        reasons: ["current"],
      }],
      resolved_candidate: {
        id: "cline-provider",
        name: "Cline provider",
        protocol: "open_ai",
        base_url: "https://cline.example.test/v1",
        models: ["cline-model"],
        source: "agent-config:cline",
        freshness: "unix:4",
        auth_mode: "none",
        active: true,
        state: "discovered",
        agent_ids: ["cline"],
        confidence: 99,
        reasons: ["current"],
      },
      resolution: "auto-resolved" as const,
      capability: "one-click" as const,
      can_configure: true,
      can_clear: true,
      already_connected: false,
    };
    const openCodeBinding = {
      ...connectedSnapshot.agent_bindings[0],
      can_configure: true,
      can_clear: true,
    };
    const focusSnapshot = {
      ...connectedSnapshot,
      revision: "onboarding-focus",
      agent_bindings: [clineBinding, openCodeBinding],
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return focusSnapshot;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install initialAgentId="cline" />);

    // The batch is automatic: no per-Agent include checkbox is rendered and
    // the onboarding Agent hint never narrows the auto batch to one Agent.
    expect(await screen.findByRole("button", { name: "查看变更" })).toBeEnabled();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(screen.getByText("Cline")).toBeInTheDocument();
    expect(screen.getByText("OpenCode")).toBeInTheDocument();
  });

  it("marks an ambiguous upstream as needs-source-fix and fails closed without a chooser", async () => {
    const firstSourceCandidate = {
      id: "profile-one",
      name: "Profile one",
      protocol: "open_ai",
      base_url: "https://one.example.test/v1",
      models: ["model-a"],
      source: "agent-config:opencode",
      freshness: "unix:5",
      auth_mode: "none",
      active: true,
      state: "discovered",
      agent_ids: ["opencode"],
      confidence: 95,
      reasons: ["current"],
    };
    const secondSourceCandidate = {
      id: "profile-two",
      name: "Profile two",
      protocol: "open_ai",
      base_url: "https://two.example.test/v1",
      models: ["model-b"],
      source: "cc-switch:profile-two",
      freshness: "unix:5",
      auth_mode: "none",
      active: true,
      state: "discovered",
      agent_ids: ["opencode"],
      confidence: 92,
      reasons: ["active profile"],
    };
    const conflictBinding = {
      ...connectedSnapshot.agent_bindings[0],
      candidates: [firstSourceCandidate, secondSourceCandidate],
      resolved_candidate: null,
      resolution: "needs-source-fix" as const,
      confidence: 40,
      reasons: ["multiple sources claim the same Agent"],
      can_configure: true,
      can_clear: true,
      already_connected: false,
    };
    const conflictSnapshot = {
      ...connectedSnapshot,
      revision: "source-conflict-1",
      candidates: [firstSourceCandidate, secondSourceCandidate],
      agent_bindings: [conflictBinding],
      has_resolution_gap: true,
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return conflictSnapshot;
      throw new Error(`unexpected command: ${command}`);
    });

    const { container } = render(<Install />);

    // The conflict is surfaced as source-conflict copy: fix the source and
    // rescan. No chooser is offered for the ambiguous upstream.
    expect(await screen.findByText("来源冲突")).toBeInTheDocument();
    expect(screen.getByText(/无法唯一解析；请修复来源配置后重新扫描/)).toBeInTheDocument();

    // The whole plan fails closed: no review path exists while the source is
    // ambiguous; rescan is the only primary action.
    expect(screen.getByText("需要处理")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新扫描" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "查看变更" })).not.toBeInTheDocument();

    // No select or checkbox chooser is rendered on the page.
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(container.querySelector("select")).toBeNull();
  });

  it("rescans instead of redirecting a detection-only Agent to Advanced setup", async () => {
    const onOpenAdvanced = vi.fn();
    const detectionOnlySnapshot = {
      ...connectedSnapshot,
      revision: "detection-only-agent",
      candidates: [],
      agent_bindings: [{
        agent_id: "cursor",
        agent_name: "Cursor",
        installed: true,
        config_path: "~/.cursor/settings.json",
        current_config_source: "agent-config:cursor",
        candidates: [],
        resolved_candidate: null,
        resolution: "unsupported" as const,
        confidence: 0,
        reasons: ["schema-unverified"],
        capability: "prompt-only" as const,
        can_configure: false,
        can_clear: false,
        transport_supported: false,
        transport_status: "advanced_required" as const,
        already_connected: false,
      }],
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return detectionOnlySnapshot;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install onOpenAdvanced={onOpenAdvanced} />);

    const rescan = await screen.findByRole("button", { name: "重新扫描" });
    fireEvent.click(rescan);
    await waitFor(() => expect(mocks.invokeNative).toHaveBeenCalledWith("discover_upstream_candidates"));
    expect(onOpenAdvanced).not.toHaveBeenCalled();
  });

  it("shows when an Agent transport bypasses a followed d-token route", async () => {
    const driftedSnapshot = {
      ...connectedSnapshot,
      revision: "agent-follow-drift",
      agent_bindings: [{
        ...connectedSnapshot.agent_bindings[0],
        agent_id: "cline",
        agent_name: "Cline",
        config_path: "~/.cline/settings/providers.json",
        resolution: "auto-resolved" as const,
        can_configure: true,
        can_clear: true,
        already_connected: false,
        follow: {
          authorized: true,
          enabled: true,
          state: "transport_out_of_sync",
          transport_out_of_sync: true,
          next_action: "configure" as const,
          current_upstream: {
            provider_id: "cline-upstream",
            model: "cline-model",
            protocol: "open_ai",
            normalized_endpoint: "https://cline.example.test/v1",
            profile_scope: "agent-config:cline",
          },
          last_receipt: {
            schema_version: 1,
            state: "applied_verified",
            source_fingerprint: "source-b",
            changed_agents: ["cline"],
            route_before_sha256: "before",
            route_after_sha256: "after",
            rollback_id: "rollback-safe",
            runtime_state: "runtime_reloaded",
            occurred_at_ms: 43,
          },
        },
      }],
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return driftedSnapshot;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);

    expect(await screen.findByText("流量可能绕过 d-token")).toBeInTheDocument();
    expect(screen.getByText("下一步：重新配置此 Agent，使请求重新经过 d-token。")).toBeInTheDocument();
    expect(screen.getByText(/d-token 路由 · cline-upstream · cline-model · open_ai/)).toBeInTheDocument();
    expect(screen.getByText("receipt applied_verified · runtime_reloaded")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "一键配置" })).toBeEnabled();
  });

  it("counts only detected Agents and their real discovered upstreams", () => {
    expect(connectionDiscoveryCounts({
      source: "native",
      freshness: "unix:1",
      revision: "r1",
      state: "loaded",
      candidates: [
        { id: "live", name: "Live", protocol: "open_ai", base_url: "https://example.test/v1", models: ["m"], source: "agent-config:opencode", freshness: "unix:1", auth_mode: "none", active: true, state: "discovered", agent_ids: ["opencode"], confidence: 98, reasons: [] },
        { id: "catalog-only", name: "Catalog", protocol: "open_ai", base_url: "https://unused.test/v1", models: ["m"], source: "catalog", freshness: "unix:1", auth_mode: "none", active: false, state: "available", agent_ids: ["not-installed"], confidence: 80, reasons: [] },
      ],
      agent_bindings: [{ agent_id: "opencode", agent_name: "OpenCode", installed: true, current_config_source: "agent-config:opencode", candidates: [], resolution: "auto-resolved", confidence: 98, reasons: [], capability: "one-click", already_connected: false }],
      sources: [{ id: "cc-switch-sqlite", profile_count: 3 }],
      has_resolution_gap: false,
      cc_switch_sync: null,
      writes_performed: false,
      privacy: "redacted",
    })).toEqual({ agents: 1, upstreams: 1, ccSwitchProfiles: 3 });
  });

  it("keeps Codex Responses proxy model passthrough in the automatic route request", () => {
    const codexCandidate = {
      id: "codex-ocxlocal",
      name: "Codex ocxlocal",
      protocol: "responses",
      base_url: "http://127.0.0.1:10100/v1",
      models: [],
      source: "agent-config:codex",
      freshness: "unix:1",
      auth_mode: "none",
      active: true,
      state: "discovered",
      agent_ids: ["codex"],
      confidence: 100,
      reasons: [],
    };
    expect(buildConnectionRoutePlanRequest("r-codex", [{
      agent_id: "codex",
      agent_name: "Codex",
      installed: true,
      current_config_source: "agent-config:codex",
      candidates: [codexCandidate],
      resolved_candidate: codexCandidate,
      resolution: "auto-resolved",
      confidence: 100,
      reasons: [],
      capability: "one-click",
      already_connected: false,
    }], {})).toEqual({
      discovery_revision: "r-codex",
      bindings: [{
        agent_id: "codex",
        candidate_id: "codex-ocxlocal",
        candidate_source: "agent-config:codex",
        model: "",
      }],
    });
  });

  it("omits a multi-model resolved candidate from the route plan instead of locking models[0]", () => {
    const multi = {
      id: "multi",
      name: "Multi model",
      protocol: "open_ai",
      base_url: "https://example.test/v1",
      models: ["model-a", "model-b"],
      source: "agent-config:codex",
      freshness: "unix:1",
      auth_mode: "none",
      active: true,
      state: "discovered",
      agent_ids: ["codex"],
      confidence: 100,
      reasons: [],
    };
    const plan = buildConnectionRoutePlanRequest("r-multi", [{
      agent_id: "codex",
      agent_name: "Codex",
      installed: true,
      current_config_source: "agent-config:codex",
      candidates: [multi],
      resolved_candidate: multi,
      resolution: "auto-resolved",
      confidence: 100,
      reasons: [],
      capability: "one-click",
      already_connected: false,
    }], {});
    expect(plan.bindings).toEqual([]);
  });

  it("carries the single auto-resolved model into the route plan without a chooser", () => {
    const single = {
      id: "single",
      name: "Single",
      protocol: "open_ai",
      base_url: "https://example.test/v1",
      models: ["model-a"],
      source: "agent-config:codex",
      freshness: "unix:1",
      auth_mode: "none",
      active: true,
      state: "discovered",
      agent_ids: ["codex"],
      confidence: 100,
      reasons: [],
    };
    const plan = buildConnectionRoutePlanRequest("r-single", [{
      agent_id: "codex",
      agent_name: "Codex",
      installed: true,
      current_config_source: "agent-config:codex",
      candidates: [single],
      resolved_candidate: single,
      resolution: "auto-resolved",
      confidence: 100,
      reasons: [],
      capability: "one-click",
      already_connected: false,
    }], {});
    expect(plan.bindings).toEqual([{
      agent_id: "codex",
      candidate_id: "single",
      candidate_source: "agent-config:codex",
      model: "model-a",
    }]);
  });

  it("stores a required credential through the secure command without rendering it when native storage fails", async () => {
    const credential = "test-credential-never-rendered";
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return credentialRequiredSnapshot;
      if (command === "store_provider_credential") throw new Error(`keychain rejected ${credential}`);
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);

    fireEvent.click(await screen.findByRole("button", { name: "补充所需凭据" }));
    fireEvent.change(screen.getByLabelText("凭据"), { target: { value: credential } });
    fireEvent.click(screen.getByRole("button", { name: "安全保存并继续" }));

    await waitFor(() => expect(mocks.invokeNative).toHaveBeenCalledWith("store_provider_credential", {
      request: { provider_id: "provider-needing-key", credential },
    }));
    expect(await screen.findByRole("alert")).toHaveTextContent("未能安全保存凭据，请检查后重试。");
    expect(screen.queryByText(credential)).not.toBeInTheDocument();
    expect(screen.queryByText(`keychain rejected ${credential}`)).not.toBeInTheDocument();
    expect(screen.getByLabelText("凭据")).toHaveValue("");
  });

  it("offers the existing primary action to restart a stale managed runtime", async () => {
    mocks.runtime.status = "runtime";
    mocks.runtime.operational = {
      service: {
        data: {
          state: "restart_required",
          managed: true,
          restart_required: true,
          service_url: "http://127.0.0.1:18001",
        },
      },
    };
    mocks.invokeNative.mockReset();
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return connectedSnapshot;
      if (command === "restart_dtoken_service") {
        return { ok: true, summary: "restarted", logs: [], tools: [] };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);

    expect((await screen.findAllByText("需要重启")).length).toBeGreaterThan(0);
    fireEvent.click(await screen.findByRole("button", { name: "重启服务" }));
    await waitFor(() => expect(mocks.invokeNative).toHaveBeenCalledWith("restart_dtoken_service"));
  });

  it("closes the stop confirmation as soon as native stop succeeds without waiting for refresh", async () => {
    mocks.runtime.status = "runtime";
    mocks.runtime.operational = {
      service: {
        data: {
          state: "running",
          managed: true,
          pid: 80124,
          pid_running: true,
          manifest_stale: false,
          service_url: "http://127.0.0.1:18001",
        },
      },
    };
    mocks.refreshRuntime.mockImplementation(() => new Promise(() => {}));
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return connectedSnapshot;
      if (command === "stop_dtoken_service") {
        return { ok: true, summary: "The d-token local service stopped.", logs: [], tools: [] };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);
    fireEvent.click(await screen.findByRole("button", { name: "停止" }));
    fireEvent.click(screen.getByRole("button", { name: "确认停止" }));

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(screen.getByRole("status")).toHaveTextContent("d-token 本地服务已停止");
    expect(screen.queryByText("The d-token local service stopped.")).not.toBeInTheDocument();
  });

  it("keeps a failed stop confirmation open and actionable", async () => {
    mocks.runtime.status = "runtime";
    mocks.runtime.operational = {
      service: {
        data: {
          state: "running",
          managed: true,
          pid: 80124,
          pid_running: true,
          manifest_stale: false,
          service_url: "http://127.0.0.1:18001",
        },
      },
    };
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return connectedSnapshot;
      if (command === "stop_dtoken_service") throw new Error("native stop failed");
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);
    fireEvent.click(await screen.findByRole("button", { name: "停止" }));
    fireEvent.click(screen.getByRole("button", { name: "确认停止" }));

    expect((await screen.findAllByText("请重试；如果问题仍然存在，请查看连接日志。")).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "取消" })).toBeEnabled();
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });

  it("does not expose an English-only privacy sentence in the Chinese Install UI", () => {
    expect(presentConnectionPrivacy(
      "Normal strict JSON config only; auth.json, credentials, prompts, and responses are never read or returned",
      "zh",
    )).toBe("隐私边界：未读取或显示凭据、认证文件、提示词或响应原文。");
  });

  it("localizes native route verification summaries before rendering them", () => {
    const nativeSummary = "Agent route configuration is active and observable through d-token runtime";
    expect(presentRouteVerificationSummary({ ok: true, summary: nativeSummary }, "zh"))
      .toBe("配置、服务和路由证据一致。");
    expect(presentRouteVerificationSummary({ ok: false, summary: "Agent route configuration still needs attention" }, "zh"))
      .toBe("连接证据仍需处理。");
    expect(presentRouteVerificationSummary({ ok: true, summary: nativeSummary }, "en"))
      .toBe(nativeSummary);
  });

  it("restores in-flight install progress after the page unmounts and remounts", async () => {
    mocks.invokeNative.mockReset();
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return connectedSnapshot;
      throw new Error(`unexpected command: ${command}`);
    });
    dispatchInstallMachine({ type: "DISCOVERED", snapshot: connectedSnapshot });
    dispatchInstallMachine({ type: "SET_PHASE", phase: "applying-runtime" });

    const firstPage = render(<Install />);
    expect(await screen.findByText("正在启动服务")).toBeInTheDocument();
    firstPage.unmount();

    dispatchInstallMachine({ type: "SET_PHASE", phase: "verifying" });
    render(<Install />);
    expect(await screen.findByText("正在验证")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "正在处理" })).toBeDisabled();
  });

  it("passes nested review hashes from MCP preview to apply", async () => {
    const mcpBinding = {
      agent_id: "agent-mcp",
      agent_name: "Agent MCP",
      installed: true,
      mcp_config_path: "~/.config/agent-mcp/mcp.json",
      current_config_source: "agent-config:agent-mcp",
      candidates: [] as never[],
      resolved_candidate: null,
      resolution: "already-connected" as const,
      confidence: 100,
      reasons: ["connected"],
      capability: "one-click" as const,
      integration_kind: "mcp" as const,
      can_configure: true,
      can_clear: false,
      already_connected: false,
    };
    const mcpSnapshot = {
      ...connectedSnapshot,
      revision: "mcp-review-1",
      agent_bindings: [mcpBinding],
    };
    mocks.runtime.status = "runtime";
    mocks.runtime.operational = {
      service: {
        data: {
          state: "running",
          managed: true,
          pid: 90123,
          pid_running: true,
          manifest_stale: false,
          service_url: "http://127.0.0.1:18001",
        },
      },
    };
    mocks.invokeNative.mockReset();
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return mcpSnapshot;
      if (command === "preview_managed_agent_mcp_integration") {
        return {
          ok: true,
          state: "review",
          review: {
            before_sha256: "mcp-before-hash",
            after_sha256: "mcp-after-hash",
            revision: "mcp-revision",
          },
        };
      }
      if (command === "apply_managed_agent_mcp_integration") {
        return { ok: true, state: "applied", verification: { ok: true } };
      }
      throw new Error(`unexpected command: ${command}`);
    });
    vi.spyOn(window, "confirm").mockReturnValueOnce(true);

    render(<Install />);
    expect(await screen.findByText("Agent MCP")).toBeInTheDocument();
    // MCP/Hook integrations are labeled truthfully: tool capability only.
    expect(screen.getByText("工具能力，不改变模型上游")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "配置工具集成" }));

    await waitFor(() => {
      expect(mocks.invokeNative).toHaveBeenCalledWith(
        "apply_managed_agent_mcp_integration",
        expect.objectContaining({
          reviewed_before_sha256: "mcp-before-hash",
          reviewed_after_sha256: "mcp-after-hash",
        }),
      );
    });
    expect(screen.getByRole("status")).toHaveTextContent("Agent MCP MCP 工具已配置并验证。");
  });

  it("rejects MCP preview when review is missing required hashes", async () => {
    const mcpBinding = {
      agent_id: "agent-mcp",
      agent_name: "Agent MCP",
      installed: true,
      mcp_config_path: "~/.config/agent-mcp/mcp.json",
      current_config_source: "agent-config:agent-mcp",
      candidates: [] as never[],
      resolved_candidate: null,
      resolution: "already-connected" as const,
      confidence: 100,
      reasons: ["connected"],
      capability: "one-click" as const,
      integration_kind: "mcp" as const,
      can_configure: true,
      can_clear: false,
      already_connected: false,
    };
    const mcpSnapshot = {
      ...connectedSnapshot,
      revision: "mcp-review-2",
      agent_bindings: [mcpBinding],
    };
    mocks.runtime.status = "runtime";
    mocks.runtime.operational = {
      service: {
        data: {
          state: "running",
          managed: true,
          pid: 90124,
          pid_running: true,
          manifest_stale: false,
          service_url: "http://127.0.0.1:18001",
        },
      },
    };
    mocks.invokeNative.mockReset();
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return mcpSnapshot;
      if (command === "preview_managed_agent_mcp_integration") {
        return {
          ok: true,
          state: "review",
          review: {
            revision: "mcp-revision",
          },
        };
      }
      throw new Error(`unexpected command: ${command}`);
    });
    vi.spyOn(window, "confirm").mockReturnValueOnce(true);

    render(<Install />);
    expect(await screen.findByText("Agent MCP")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "配置工具集成" }));

    await waitFor(() => {
      expect(mocks.invokeNative).not.toHaveBeenCalledWith(
        "apply_managed_agent_mcp_integration",
        expect.anything(),
      );
    });
    expect(screen.getByRole("status")).toHaveTextContent("MCP 操作未完成");
  });
});
