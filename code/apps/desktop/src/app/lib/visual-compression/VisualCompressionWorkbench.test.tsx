import React from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  applyOpticalConfigChange: vi.fn(),
  loadOpticalConfigEditor: vi.fn(),
  loadOpticalObservability: vi.fn(),
  previewOpticalConfigChange: vi.fn(),
  previewOpticalPlan: vi.fn(),
  restoreOpticalConfigBackup: vi.fn(),
  applyHybridAgentIntegration: vi.fn(),
  getHybridAgentIntegrationTransaction: vi.fn(),
  loadHybridAgentIntegrations: vi.fn(),
  previewHybridAgentIntegration: vi.fn(),
  rollbackHybridAgentIntegration: vi.fn(),
  startHybridService: vi.fn(),
  verifyHybridAgentIntegration: vi.fn(),
}));

vi.mock("../optical/client", () => ({
  applyOpticalConfigChange: (...args: unknown[]) => mocks.applyOpticalConfigChange(...args),
  loadOpticalConfigEditor: (...args: unknown[]) => mocks.loadOpticalConfigEditor(...args),
  loadOpticalObservability: (...args: unknown[]) => mocks.loadOpticalObservability(...args),
  previewOpticalConfigChange: (...args: unknown[]) => mocks.previewOpticalConfigChange(...args),
  previewOpticalPlan: (...args: unknown[]) => mocks.previewOpticalPlan(...args),
  restoreOpticalConfigBackup: (...args: unknown[]) => mocks.restoreOpticalConfigBackup(...args),
}));

vi.mock("../optical/agentIntegration", () => ({
  applyHybridAgentIntegration: (...args: unknown[]) => mocks.applyHybridAgentIntegration(...args),
  getHybridAgentIntegrationTransaction: (...args: unknown[]) => mocks.getHybridAgentIntegrationTransaction(...args),
  loadHybridAgentIntegrations: (...args: unknown[]) => mocks.loadHybridAgentIntegrations(...args),
  previewHybridAgentIntegration: (...args: unknown[]) => mocks.previewHybridAgentIntegration(...args),
  rollbackHybridAgentIntegration: (...args: unknown[]) => mocks.rollbackHybridAgentIntegration(...args),
  startHybridService: (...args: unknown[]) => mocks.startHybridService(...args),
  verifyHybridAgentIntegration: (...args: unknown[]) => mocks.verifyHybridAgentIntegration(...args),
}));

import { VisualCompression } from "./VisualCompressionWorkbench";

const editorReady = {
  ok: true,
  source: "runtime",
  error: "",
  service_running: true,
  service_state: "running",
  active_config_exists: true,
  active_config_path: "/tmp/active.json",
  route_count: 1,
  routes: [{ route_id: "route-a", label: "Route A" }],
  draft: {
    global: { enabled: true, default_mode: "shadow_preview" },
    routes: [
      { route_id: "route-a", enabled: true, mode: "shadow_preview", allow_tool_result: true, allow_history: false, confirmed_experimental_lossy: false },
    ],
  },
  issues: [],
  allow_active_forced_off: true,
};

const editorBlocked = { ...editorReady, ok: false, active_config_exists: false, error: "no config" };

const discovery = {
  ok: true,
  source: "runtime",
  agents: [],
  bridge: { available: false, path: null, error: "not built" },
  service: { state: "stopped", pid: null, port: 18001 },
  transport: { configured: false, route_count: 0, detail: "" },
  defaults: {
    install_instructions: true,
    install_hook_or_plugin: true,
    install_skill: true,
    install_mcp: false,
    scope: "global",
    project_root: null,
  },
};

function snapshot(statusEnabled: boolean, overrides: Record<string, unknown> = {}) {
  return {
    ok: true,
    complete: true,
    partial: false,
    source: "runtime",
    collected_at: 123,
    service_url: "http://127.0.0.1:18001",
    status: {
      ok: true,
      source: "runtime",
      error: "",
      data: {
        enabled: statusEnabled,
        configured_mode: "shadow_preview",
        effective_mode: statusEnabled ? "shadow_preview" : "off",
        default_mode: "off",
        allow_active: false,
        active_reachable: false,
        active_execution_observed: false,
        public_summary_only: true,
        fail_closed: true,
        renderer_ready: true,
        patcher_ready: false,
        post_validator_ready: false,
        signed_accounting_ready: false,
        config_issue_count: 0,
      },
    },
    capabilities: {
      ok: true,
      source: "runtime",
      error: "",
      data: {
        shadow_preview_supported: true,
        active_transform_enabled: false,
        max_mode: "shadow_preview",
        unknown_model_status: "allowed",
        unsupported_path_status: "rejected",
        public_summary_only: true,
        fail_closed: true,
        deterministic_png_renderer: true,
        artifact_post_validation: true,
        signed_multimodal_accounting: false,
        capability_attestation_required: false,
      },
    },
    lifecycle: {
      ok: true,
      source: "runtime",
      error: "",
      data: {
        configured_snapshots: 0,
        active_snapshots: 0,
        revoked_snapshots: 0,
        superseded_snapshots: 0,
        verified_rules: 0,
        rejected_rules: 0,
        trusted_authorities: 0,
        eval_artifacts: 0,
        passing_eval_artifacts: 0,
        attestation_required: false,
      },
    },
    events: { ok: true, source: "runtime", error: "", data: [] },
    ...overrides,
  };
}

const reviewOk = {
  ok: true,
  source: "runtime",
  error: "",
  target: "active config",
  before_sha256: "abc123",
  after_sha256: "def456",
  diff: "-enabled: false\n+enabled: true",
  risk_level: "low",
  requires_explicit_confirmation: true,
  restart_required: true,
  allow_active: false,
};

const previewPlan = {
  ok: true,
  source: "runtime",
  error: "",
  data: {
    plan_id: "plan-1",
    mode: "shadow_preview",
    final_status: "eligible",
    gates: [{ name: "UnsupportedPath", passed: false, reason: "path outside project" }],
    warnings: ["sample reduced"],
    counters: { candidate_blocks: 2, preserved_blocks: 1, skipped_blocks: 1, estimated_images: 0 },
    economics: { net_saved_tokens: 0, estimated_input_tokens: 100, estimated_vision_tokens: 0 },
  },
};

beforeEach(() => {
  window.localStorage.clear();
  window.sessionStorage.clear();
  mocks.loadOpticalObservability.mockResolvedValue(snapshot(false));
  mocks.loadOpticalConfigEditor.mockResolvedValue(editorReady);
  mocks.loadHybridAgentIntegrations.mockResolvedValue(discovery);
  mocks.previewOpticalConfigChange.mockResolvedValue(reviewOk);
  mocks.applyOpticalConfigChange.mockResolvedValue({
    ok: true, source: "runtime", error: "", summary: "applied",
    rollback_id: "rb-1", config_applied: true, config_restored: false,
    runtime_verified: true, restarted: true, restart_summary: "", allow_active: false,
  });
  mocks.previewOpticalPlan.mockResolvedValue(previewPlan);
  mocks.restoreOpticalConfigBackup.mockResolvedValue({
    ok: true, source: "runtime", error: "", summary: "restored", config_restored: true, allow_active: false,
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function renderWorkbench() {
  const user = userEvent.setup();
  render(<VisualCompression />);
  await waitFor(() => expect(mocks.loadOpticalConfigEditor).toHaveBeenCalled());
  await waitFor(() => expect(mocks.loadOpticalObservability).toHaveBeenCalled());
  return user;
}

describe("VisualCompressionWorkbench state derivation", () => {
  it("shows ready (not enabled) when config is enabled but runtime status is disabled", async () => {
    await renderWorkbench();
    // editor draft has global.enabled=true, but runtime status says enabled=false.
    expect(screen.getByText("可启用")).toBeTruthy();
    expect(screen.queryByText("已启用")).toBeNull();
    expect(screen.getByRole("button", { name: "一键启用 Shadow 预览" })).toBeTruthy();
  });

  it("shows enabled only when runtime status confirms it", async () => {
    mocks.loadOpticalObservability.mockResolvedValue(snapshot(true));
    await renderWorkbench();
    expect(screen.getByText("已启用")).toBeTruthy();
    expect(screen.getByRole("button", { name: "关闭视觉压缩" })).toBeTruthy();
  });

  it("shows blocked when editor prerequisites are missing", async () => {
    mocks.loadOpticalConfigEditor.mockResolvedValue(editorBlocked);
    await renderWorkbench();
    expect(screen.getByText("未就绪")).toBeTruthy();
    expect(screen.queryByText("已启用")).toBeNull();
  });
});

describe("VisualCompressionWorkbench apply failure states", () => {
  it("distinguishes written-but-unverified apply and keeps the rollback CTA", async () => {
    mocks.applyOpticalConfigChange.mockResolvedValue({
      ok: false, source: "runtime", error: "verify failed", summary: "verify failed",
      rollback_id: "rb-9", config_applied: true, runtime_verified: false, allow_active: false,
    });
    const user = await renderWorkbench();
    await user.click(screen.getByRole("button", { name: "一键启用 Shadow 预览" }));
    await screen.findByRole("button", { name: "确认并应用" });
    await user.click(screen.getByRole("button", { name: "确认并应用" }));
    expect(await screen.findByText(/配置已写盘，但服务尚未验证/)).toBeTruthy();
    // Rollback entry must remain available for the written-but-unverified change.
    expect(screen.getByRole("button", { name: "撤销本次配置" })).toBeTruthy();
    expect(mocks.applyOpticalConfigChange).toHaveBeenCalled();
  });

  it("shows a plain failure when nothing was written and offers no rollback", async () => {
    mocks.applyOpticalConfigChange.mockResolvedValue({
      ok: false, source: "runtime", error: "write failed", summary: "write failed",
      config_applied: false, allow_active: false,
    });
    const user = await renderWorkbench();
    await user.click(screen.getByRole("button", { name: "一键启用 Shadow 预览" }));
    await screen.findByRole("button", { name: "确认并应用" });
    await user.click(screen.getByRole("button", { name: "确认并应用" }));
    expect((await screen.findAllByText(/请重试/)).length).toBeGreaterThan(0);
    expect(screen.queryByText(/配置已写盘/)).toBeNull();
    expect(screen.queryByRole("button", { name: "撤销本次配置" })).toBeNull();
  });

  it("reports success without claiming runtime verification when runtime_verified=false", async () => {
    mocks.applyOpticalConfigChange.mockResolvedValue({
      ok: true, source: "runtime", error: "", summary: "applied",
      rollback_id: "rb-5", config_applied: true, runtime_verified: false, allow_active: false,
    });
    const user = await renderWorkbench();
    await user.click(screen.getByRole("button", { name: "一键启用 Shadow 预览" }));
    await screen.findByRole("button", { name: "确认并应用" });
    await user.click(screen.getByRole("button", { name: "确认并应用" }));
    expect(await screen.findByText(/配置已写盘，但服务尚未验证/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "撤销本次配置" })).toBeTruthy();
  });
});

describe("VisualCompressionWorkbench status gating", () => {
  async function openStatusTab() {
    const user = await renderWorkbench();
    await user.click(screen.getByRole("tab", { name: /状态与预览/ }));
    await screen.findByText("能力");
    return user;
  }

  it("gates the Active KPI detail by statusOk instead of asserting unobserved text", async () => {
    mocks.loadOpticalObservability.mockResolvedValue(
      snapshot(true, { status: { ok: false, source: "unavailable", error: "status down", data: null } }),
    );
    await openStatusTab();
    expect(screen.getAllByText("当前切片不可用").length).toBeGreaterThan(0);
    expect(screen.queryByText("尚无 Active 执行证据")).toBeNull();
  });

  it("does not silently fall back across slices when the capabilities slice failed", async () => {
    mocks.loadOpticalObservability.mockResolvedValue(
      snapshot(true, { capabilities: { ok: false, source: "unavailable", error: "cap down", data: null } }),
    );
    await openStatusTab();
    // status.renderer_ready is true, but capabilities failed — the renderer row must show "—".
    const row = screen.getByText("确定性渲染").closest("div");
    expect(row?.textContent).toContain("—");
    expect(row?.textContent).not.toContain("已启用");
  });

  it("keeps the renderer row honest when capabilities omit the flag even if status has it", async () => {
    mocks.loadOpticalObservability.mockResolvedValue(
      snapshot(true, {
        capabilities: {
          ok: true, source: "runtime", error: "",
          data: { shadow_preview_supported: true, active_transform_enabled: false },
        },
      }),
    );
    await openStatusTab();
    const row = screen.getByText("确定性渲染").closest("div");
    expect(row?.textContent).toContain("—");
    expect(row?.textContent).not.toContain("已启用");
  });
});

describe("VisualCompressionWorkbench preview", () => {
  async function openPreview() {
    const user = await renderWorkbench();
    await user.click(screen.getByRole("tab", { name: /状态与预览/ }));
    await screen.findByText("能力");
    await user.click(screen.getByRole("button", { name: /预览规划/ }));
    await screen.findByLabelText("预览请求 JSON");
    return user;
  }

  it("renders gates and warnings from the shadow preview result", async () => {
    const user = await openPreview();
    await user.click(screen.getByRole("button", { name: "运行预览" }));
    expect(await screen.findByText("UnsupportedPath")).toBeTruthy();
    expect(screen.getByText("未通过")).toBeTruthy();
    expect(screen.getByText("path outside project")).toBeTruthy();
    expect(screen.getByText("sample reduced")).toBeTruthy();
  });

  it("passes the selected route_id through to the preview request", async () => {
    const user = await openPreview();
    await user.selectOptions(screen.getByLabelText("Route"), "route-a");
    await user.click(screen.getByRole("button", { name: "运行预览" }));
    await screen.findByText("UnsupportedPath");
    expect(mocks.previewOpticalPlan).toHaveBeenCalledWith(expect.anything(), { route_id: "route-a" });
  });

  it("never echoes the pasted request body in the UI", async () => {
    const user = await openPreview();
    const textarea = screen.getByLabelText("预览请求 JSON");
    fireEvent.change(textarea, { target: { value: JSON.stringify({ messages: [{ content: "sk-test-secret-123" }] }) } });
    await user.click(screen.getByRole("button", { name: "运行预览" }));
    await screen.findByText("UnsupportedPath");
    // The marker may only live in the operator's own input textarea — the preview
    // result, notices, and notifications must never echo the pasted body back.
    const matches = screen.queryAllByText(/sk-test-secret-123/);
    expect(matches.length).toBe(1);
    expect(matches[0]).toBe(textarea);
    // The operator-provided JSON is the request body (contract), but it must not be rendered back.
    expect(mocks.previewOpticalPlan).toHaveBeenCalledWith(
      expect.objectContaining({ messages: expect.any(Array) }),
      expect.anything(),
    );
  });
});
