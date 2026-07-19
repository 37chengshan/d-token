import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VisualCompression } from "./VisualCompression";
import { NotificationProvider } from "../../lib/notifications";
import { NotificationBanner } from "../common/NotificationCenter";

const native = vi.hoisted(() => ({
  invokeNative: vi.fn(),
}));

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
}));

const observabilityOk = {
  ok: true,
  complete: true,
  partial: false,
  source: "dtoken-local-service",
  collected_at: 1_700_000_000_000,
  service_url: "http://127.0.0.1:18001",
  status: {
    ok: true,
    source: "dtoken-local-service",
    error: "",
    data: {
      enabled: false,
      configured_mode: "shadow_preview",
      effective_mode: "shadow_preview",
      allow_active: false,
      fail_closed: true,
      config_issue_count: 0,
      renderer_ready: true,
      patcher_ready: true,
      post_validator_ready: true,
    },
  },
  capabilities: {
    ok: true,
    source: "dtoken-local-service",
    error: "",
    data: {
      shadow_preview_supported: true,
      active_transform_enabled: false,
      deterministic_png_renderer: true,
      artifact_post_validation: true,
      capability_attestation_required: true,
    },
  },
  lifecycle: {
    ok: true,
    source: "dtoken-local-service",
    error: "",
    data: {
      configured_snapshots: 0,
      verified_rules: 0,
      revoked_snapshots: 0,
      superseded_snapshots: 0,
      trusted_authorities: 0,
      passing_eval_artifacts: 0,
    },
  },
  events: {
    ok: true,
    source: "dtoken-local-service",
    error: "",
    data: [
      {
        event_id: "evt-1",
        timestamp_ms: 1_700_000_000_000,
        stage: "optical_preview",
        status: "ok",
        metadata: {
          final_status: "validated_shadow_plan",
          mode: "shadow_preview",
          candidate_blocks: 2,
          estimated_images: 1,
          net_saved_tokens: 12,
        },
      },
    ],
  },
};

const editorOk = {
  ok: true,
  source: "dtoken-desktop-control-plane",
  error: "",
  service_running: true,
  active_config_exists: true,
  active_config_path: "/tmp/dtoken-config.json",
  route_count: 1,
  routes: [{ route_id: "route-a", label: "Route A" }],
  draft: {
    global: { enabled: false, default_mode: "shadow_preview" },
    routes: [
      {
        route_id: "route-a",
        enabled: false,
        mode: "off",
        allow_tool_result: false,
        allow_history: false,
        confirmed_experimental_lossy: false,
      },
    ],
  },
  issues: [],
  allow_active_forced_off: true,
};

const agentDiscoveryOk = {
  ok: true,
  source: "d-token-agent-integration",
  agents: [
    { id: "claude-code", name: "Claude Code", installed: true, surfaces: ["instructions", "hook_or_plugin", "skill", "mcp"], transport_state: "separate_route_prerequisite" },
    { id: "codex", name: "Codex", installed: true, surfaces: ["instructions", "hook_or_plugin", "skill", "mcp"], transport_state: "separate_route_prerequisite" },
    { id: "opencode", name: "OpenCode", installed: true, surfaces: ["instructions", "hook_or_plugin", "skill", "mcp"], transport_state: "separate_route_prerequisite" },
  ],
  bridge: { available: true, path: "/tmp/dtoken-agent-bridge", error: null },
  service: {
    state: "running",
    pid: 42,
    port: 18001,
    runtime_identity: "dtoken-runtime-local",
    runtime_version: "0.1.0",
    runtime_identity_ok: true,
    runtime_version_ok: true,
  },
  transport: { configured: true, route_count: 1, detail: "separate" },
  defaults: {
    install_instructions: true,
    install_hook_or_plugin: true,
    install_skill: true,
    install_mcp: false,
    scope: "global",
    project_root: null,
  },
};

function mockHappyPath(extra: Record<string, unknown> = {}) {
  native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "get_optical_observability") return observabilityOk;
    if (command === "get_optical_config_editor") return editorOk;
    if (command === "get_hybrid_agent_integrations") return agentDiscoveryOk;
    if (extra[command]) {
      const value = extra[command];
      return typeof value === "function" ? (value as (a?: Record<string, unknown>) => unknown)(args) : value;
    }
    throw new Error(`unexpected command ${command}`);
  });
}

describe("VisualCompression", () => {
  beforeEach(() => {
    native.invokeNative.mockReset();
    try {
      window.sessionStorage.removeItem("d-token.visual.workbench-tab");
      window.localStorage.removeItem("d-token.hybrid.agent-transaction");
    } catch {
      /* ignore */
    }
  });

  afterEach(() => {
    cleanup();
  });

  it("shows install-style workbench tabs and unavailable notice when native is missing", async () => {
    native.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    render(<NotificationProvider><NotificationBanner /><VisualCompression /></NotificationProvider>);
    await waitFor(() => {
      expect(
        screen.getByText(/还不能启用视觉压缩|Visual compression is not ready yet/i),
      ).toBeInTheDocument();
    });
    expect(screen.getByText("Beta")).toBeInTheDocument();
    expect(screen.getByText(/默认使用 Shadow 预览|Shadow preview is the default/i)).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /一键启用|One-Click Enable/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /高级配置|Advanced Setup/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /状态与预览|Status & Preview/i })).toBeInTheDocument();
    expect(screen.queryByText(/如何配置|How to configure/i)).toBeNull();
  });

  it("defaults to one-click enable with safe defaults, not a setup checklist", async () => {
    mockHappyPath();
    render(<VisualCompression />);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /一键启用 Shadow 预览|Enable shadow preview/i })).toBeInTheDocument();
    });
    expect(screen.getByText(/将应用的安全默认值|Safe defaults that will be applied/i)).toBeInTheDocument();
    expect(screen.getByText(/强制关闭 Active 生产改写|Production Active rewrite stays forced off/i)).toBeInTheDocument();
    expect(screen.queryByText(/如何配置|How to configure/i)).toBeNull();
    expect(screen.queryByText(/前置检查|prerequisites/i)).toBeNull();
    expect(screen.queryByRole("button", { name: /开启 Active|Enable active/i })).toBeNull();
    expect(screen.queryByText(/benchmark|workload|fixture test|\blab\b/i)).toBeNull();
    await waitFor(() => {
      expect(screen.getByText(/dtoken-runtime-local.*v0\.1\.0/)).toBeInTheDocument();
    });
  });

  it("one-click enable previews and applies safe draft with hash binding", async () => {
    const user = userEvent.setup();
    mockHappyPath({
      preview_optical_config_change: {
        ok: true,
        source: "dtoken-desktop-control-plane",
        target: "/tmp/dtoken-config.json",
        before_sha256: "beforehash",
        after_sha256: "afterhash",
        diff: "- allow_active: false\n+ enabled: true",
        risk_level: "medium",
        requires_explicit_confirmation: true,
        restart_required: true,
        allow_active: false,
      },
      apply_optical_config_change: {
        ok: true,
        source: "dtoken-desktop-control-plane",
        summary: "Optical shadow config applied with a durable rollback point",
        rollback_id: "rb-1",
        restarted: true,
        restart_summary: "Managed d-token service restarted to load optical config",
        allow_active: false,
      },
    });
    render(<NotificationProvider><NotificationBanner /><VisualCompression /></NotificationProvider>);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /一键启用 Shadow 预览|Enable shadow preview/i })).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: /一键启用 Shadow 预览|Enable shadow preview/i }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /确认并应用|Confirm and apply/i })).toBeInTheDocument();
    });
    expect(screen.getByText(/beforehash|before/i)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /确认并应用|Confirm and apply/i }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith(
        "apply_optical_config_change",
        expect.objectContaining({
          request: expect.objectContaining({
            reviewed_before_sha256: "beforehash",
            reviewed_after_sha256: "afterhash",
            draft: expect.objectContaining({
              global: expect.objectContaining({ enabled: true, default_mode: "shadow_preview" }),
              routes: expect.arrayContaining([
                expect.objectContaining({
                  route_id: "route-a",
                  enabled: true,
                  mode: "shadow_preview",
                  allow_tool_result: true,
                }),
              ]),
            }),
          }),
        }),
      );
    });
    expect((await screen.findAllByText(/视觉压缩配置已应用|Visual compression config applied/i)).length).toBeGreaterThan(0);
  });

  it("previews, applies, verifies, and exposes rollback for Agent integration", async () => {
    const user = userEvent.setup();
    mockHappyPath({
      preview_hybrid_agent_integration: {
        ok: true,
        state: "review",
        source: "d-token-agent-integration",
        plan: {
          schema_version: 1,
          scope: "global",
          project_root: null,
          review_digest: "review-1",
          targets: [{
            agent_ids: ["codex"],
            surface: "instructions",
            target_path: "/tmp/AGENTS.md",
            before_sha256: "before",
            after_sha256: "after",
            diff: "+ managed block",
            trust_required: false,
            new_session_required: true,
          }],
          transport: { surface: "transport", state: "separate_route_prerequisite", detail: "separate" },
          restart_required: false,
          new_session_required: true,
        },
      },
      apply_hybrid_agent_integration: {
        ok: true,
        state: "verified",
        summary: "Agent integration applied",
        receipt: {
          transaction_id: "tx-1",
          review_digest: "review-1",
          rollback_ids: ["rb-1"],
          created_directories: ["/tmp/.codex"],
          verified_targets: ["/tmp/AGENTS.md"],
          trust_required: true,
          new_session_required: true,
        },
      },
      verify_hybrid_agent_integration: { ok: true, state: "verified" },
      get_hybrid_agent_integration_transaction: {
        ok: true,
        state: "verified",
        transaction: {
          schema_version: 1,
          transaction_id: "tx-1",
          review_digest: "review-1",
          targets: [],
          rollback_ids: ["rb-1"],
          created_directories: ["/tmp/.codex"],
          state: "verified",
          applied_at_ms: 1,
        },
      },
    });
    render(<VisualCompression />);
    await waitFor(() => expect(screen.getByRole("button", { name: /预览接入变更|Preview integration changes/i })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: /预览接入变更|Preview integration changes/i }));
    await waitFor(() => expect(screen.getByRole("button", { name: /确认、应用并验证|Confirm, apply, and verify/i })).toBeInTheDocument());
    expect(screen.getByText("/tmp/AGENTS.md")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /确认、应用并验证|Confirm, apply, and verify/i }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith(
      "apply_hybrid_agent_integration",
      { request: { draft: expect.any(Object), reviewed_digest: "review-1" } },
    ));
    expect(native.invokeNative).toHaveBeenCalledWith(
      "verify_hybrid_agent_integration",
      { request: { transaction_id: "tx-1" } },
    );
    await waitFor(() => expect(screen.getByRole("button", { name: /回退本次 Agent 接入|Rollback this integration/i })).toBeInTheDocument());
  });

  it("advanced tab edits route policy and applies with confirmation", async () => {
    const user = userEvent.setup();
    mockHappyPath({
      preview_optical_config_change: {
        ok: true,
        source: "dtoken-desktop-control-plane",
        target: "/tmp/dtoken-config.json",
        before_sha256: "beforehash",
        after_sha256: "afterhash",
        diff: "+ route enabled",
        risk_level: "medium",
        requires_explicit_confirmation: true,
        restart_required: true,
        allow_active: false,
      },
      apply_optical_config_change: {
        ok: true,
        source: "dtoken-desktop-control-plane",
        summary: "Optical shadow config applied with a durable rollback point",
        rollback_id: "rb-2",
        restarted: true,
        restart_summary: "Managed d-token service restarted to load optical config",
        allow_active: false,
      },
    });
    render(<VisualCompression />);
    await waitFor(() => {
      expect(screen.getByRole("tab", { name: /高级配置|Advanced Setup/i })).toBeInTheDocument();
    });
    await user.click(screen.getByRole("tab", { name: /高级配置|Advanced Setup/i }));
    await waitFor(() => {
      expect(screen.getByText(/启用 Shadow 预览引擎|Enable shadow preview engine/i)).toBeInTheDocument();
    });
    const enableSwitch = screen.getByRole("switch", { name: /启用 Shadow 预览引擎|Enable shadow preview engine/i });
    await user.click(enableSwitch);
    await user.click(screen.getByRole("button", { name: /预览变更|Preview change/i }));
    await waitFor(() => {
      expect(screen.getByText(/应用视觉压缩配置|Apply visual compression config/i)).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: /确认并应用|Confirm and apply/i }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith(
        "apply_optical_config_change",
        expect.objectContaining({
          request: expect.objectContaining({
            draft: expect.objectContaining({
              global: expect.objectContaining({ enabled: true }),
            }),
          }),
        }),
      );
    });
  });

  it("restores a changed Agent transaction without showing it as verified", async () => {
    window.localStorage.setItem("d-token.hybrid.agent-transaction", "tx-changed");
    mockHappyPath({
      get_hybrid_agent_integration_transaction: {
        ok: true,
        state: "changed",
        transaction: {
          schema_version: 1,
          transaction_id: "tx-changed",
          review_digest: "review-changed",
          targets: [],
          rollback_ids: ["rb-changed"],
          created_directories: [],
          state: "changed",
          applied_at_ms: 1,
        },
      },
      verify_hybrid_agent_integration: { ok: false, state: "changed" },
    });
    render(<VisualCompression />);
    await waitFor(() => expect(screen.getByText("changed")).toBeInTheDocument());
    expect(native.invokeNative).toHaveBeenCalledWith(
      "verify_hybrid_agent_integration",
      { request: { transaction_id: "tx-changed" } },
    );
    expect(screen.queryByText(/^已验证$|^Verified$/)).toBeNull();
  });

  it("revalidates a persisted verified Agent transaction after restart", async () => {
    window.localStorage.setItem("d-token.hybrid.agent-transaction", "tx-stale");
    mockHappyPath({
      get_hybrid_agent_integration_transaction: {
        ok: true,
        state: "verified",
        transaction: {
          schema_version: 1,
          transaction_id: "tx-stale",
          review_digest: "review-stale",
          targets: [{
            agent_ids: ["codex"],
            surface: "instructions",
            target_path: "/tmp/AGENTS.md",
            before_sha256: "before",
            after_sha256: "after",
            diff: "+ managed block",
            trust_required: false,
            new_session_required: true,
          }],
          rollback_ids: ["rb-stale"],
          created_directories: [],
          state: "verified",
          applied_at_ms: 1,
          verified_at_ms: 2,
        },
      },
      verify_hybrid_agent_integration: { ok: false, state: "changed" },
    });
    render(<VisualCompression />);
    await waitFor(() => expect(screen.getByText("changed")).toBeInTheDocument());
    expect(screen.queryByText(/^已验证$|^Verified$/)).toBeNull();
  });

  it("clears a stale verified receipt when persisted transaction evidence is unavailable", async () => {
    window.localStorage.setItem("d-token.hybrid.agent-transaction", "tx-missing");
    mockHappyPath({
      get_hybrid_agent_integration_transaction: {
        ok: false,
        state: "unavailable",
        error: "transaction manifest missing",
      },
    });
    render(<VisualCompression />);
    await waitFor(() => expect(screen.getByText("failed")).toBeInTheDocument());
    expect(screen.getByText("所需的本机信息已不可用。请刷新后重试。")).toBeInTheDocument();
    expect(screen.queryByText(/^已验证$|^Verified$/)).toBeNull();
    expect(screen.queryByRole("button", { name: /回退本次 Agent 接入|Rollback this integration/i })).toBeNull();
  });

  it("shows verify command failures as failed rather than changed", async () => {
    const user = userEvent.setup();
    mockHappyPath({
      preview_hybrid_agent_integration: {
        ok: true,
        state: "review",
        plan: {
          schema_version: 1,
          scope: "global",
          project_root: null,
          review_digest: "review-failed",
          targets: [],
          rollback_ids: [],
          created_directories: [],
          restart_required: false,
          new_session_required: true,
          transport: { surface: "transport", state: "separate_route_prerequisite", detail: "separate" },
        },
      },
      apply_hybrid_agent_integration: {
        ok: true,
        state: "applied",
        summary: "Agent integration applied",
        receipt: {
          transaction_id: "tx-failed",
          review_digest: "review-failed",
          rollback_ids: ["rb-failed"],
          created_directories: [],
          verified_targets: [],
          trust_required: false,
          new_session_required: true,
        },
      },
      verify_hybrid_agent_integration: { ok: false, state: "failed", error: "target unreadable" },
      get_hybrid_agent_integration_transaction: {
        ok: true,
        state: "failed",
        transaction: {
          schema_version: 1,
          transaction_id: "tx-failed",
          review_digest: "review-failed",
          targets: [],
          rollback_ids: ["rb-failed"],
          created_directories: [],
          state: "failed",
          applied_at_ms: 1,
        },
      },
    });
    render(<VisualCompression />);
    await waitFor(() => expect(screen.getByRole("button", { name: /预览接入变更|Preview integration changes/i })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: /预览接入变更|Preview integration changes/i }));
    await user.click(await screen.findByRole("button", { name: /确认、应用并验证|Confirm, apply, and verify/i }));
    await waitFor(() => expect(screen.getByText("failed")).toBeInTheDocument());
    expect(screen.queryByText("changed")).toBeNull();
  });

  it("keeps last good evidence when a later refresh fails", async () => {
    const user = userEvent.setup();
    let observabilityCalls = 0;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_optical_config_editor") return editorOk;
      if (command === "get_optical_observability") {
        observabilityCalls += 1;
        if (observabilityCalls === 1) return observabilityOk;
        throw new Error("runtime blip");
      }
      throw new Error(`unexpected command ${command}`);
    });
    render(<VisualCompression />);
    await waitFor(() => {
      expect(screen.getByRole("tab", { name: /状态与预览|Status & Preview/i })).toBeInTheDocument();
    });
    await user.click(screen.getByRole("tab", { name: /状态与预览|Status & Preview/i }));
    await waitFor(() => {
      expect(screen.getAllByText(/Shadow 预览|Shadow preview/i).length).toBeGreaterThan(0);
    });
    await user.click(screen.getByRole("button", { name: /刷新|Refresh/i }));
    await waitFor(() => {
      expect(screen.getByText("请重试；如果问题仍然存在，请查看连接日志。")).toBeInTheDocument();
    });
    expect(screen.getByText(/仍显示上一份可用证据|keeping the last good evidence/i)).toBeInTheDocument();
    expect(screen.getAllByText(/Shadow 预览|Shadow preview/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/暂无视觉压缩状态|Visual compression status unavailable/i)).toBeNull();
  });

  it("runs shadow preview on status tab without echoing request body", async () => {
    const user = userEvent.setup();
    mockHappyPath({
      preview_optical_plan: {
        ok: true,
        source: "dtoken-local-service",
        error: "",
        data: {
          plan_id: "plan-1",
          mode: "shadow_preview",
          final_status: "validated_shadow_plan",
          counters: { candidate_blocks: 1, estimated_images: 0 },
          economics: { net_saved_tokens: 3 },
          gates: [{ passed: true, reason: "Eligible" }],
          warnings: [],
        },
      },
    });
    render(<VisualCompression />);
    await waitFor(() => {
      expect(screen.getByRole("tab", { name: /状态与预览|Status & Preview/i })).toBeInTheDocument();
    });
    await user.click(screen.getByRole("tab", { name: /状态与预览|Status & Preview/i }));
    await waitFor(() => expect(screen.getByRole("button", { name: /预览规划|Preview plan/i })).toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: /预览规划|Preview plan/i }));
    const panel = document.getElementById("visual-preview-panel");
    expect(panel).toBeTruthy();
    await user.click(within(panel as HTMLElement).getByRole("button", { name: /运行预览|Run preview/i }));
    await waitFor(() => {
      expect(screen.getAllByText(/已验证 shadow 规划|Validated shadow plan/i).length).toBeGreaterThan(0);
    });
    expect(screen.queryByText("raw-secret-should-not-appear")).toBeNull();
    expect(native.invokeNative).toHaveBeenCalledWith(
      "preview_optical_plan",
      expect.objectContaining({
        request: expect.objectContaining({
          endpoint_path: "/v1/messages",
          protocol: "anthropic",
        }),
      }),
    );
  });
});
