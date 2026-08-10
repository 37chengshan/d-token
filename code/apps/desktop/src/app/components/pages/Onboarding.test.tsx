import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invokeNative: vi.fn(),
  connectionSetup: vi.fn(),
  runtimeSlice: vi.fn(),
}));

vi.mock("../../lib/native", () => ({
  invokeNative: (...args: unknown[]) => mocks.invokeNative(...args),
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
}));

vi.mock("../../lib/connectionSetup", () => ({
  useConnectionSetup: () => mocks.connectionSetup(),
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboardSelector: () => mocks.runtimeSlice(),
  shallowEqualSlice: (a: unknown) => a,
}));

import { I18nContext } from "../../lib/i18n";
import { OnboardingPage } from "./Onboarding";
import type { AgentUpstreamBinding, ConnectionSetupSnapshot } from "../../lib/connectionSetup";

const emptySnapshot: ConnectionSetupSnapshot = {
  source: "dtoken-desktop-read-only-discovery",
  freshness: "unix:1",
  revision: "setup-1",
  state: "loaded",
  candidates: [],
  agent_bindings: [],
  sources: [],
  selection_required: false,
  cc_switch_sync: null,
  writes_performed: false,
  privacy: "metadata only",
};

const runningRuntime = {
  payload: { desktop_service_url: "http://127.0.0.1:4242" },
  status: "runtime" as const,
  operational: {
    service: { data: { state: "running", pid_running: true, managed: true, service_url: "http://127.0.0.1:4242" } },
  },
  updatedAt: null,
};

function binding(overrides: Partial<AgentUpstreamBinding> & { agent_id: string; agent_name: string }): AgentUpstreamBinding {
  return {
    installed: true,
    current_config_source: "agent-config:test",
    candidates: [],
    resolution: "auto-resolved",
    confidence: 80,
    reasons: [],
    capability: "one-click",
    already_connected: false,
    can_configure: true,
    ...overrides,
  } as AgentUpstreamBinding;
}

const detectedSnapshot: ConnectionSetupSnapshot = {
  ...emptySnapshot,
  agent_bindings: [
    binding({ agent_id: "opencode", agent_name: "OpenCode", installed: true, current_config_source: "agent-config:opencode" }),
    binding({ agent_id: "codex", agent_name: "Codex", installed: true, resolution: "needs-credential", can_configure: false, candidates: [{ id: "provider-1", name: "Provider", protocol: "openai", models: ["m"], source: "test", freshness: "unix:1", auth_mode: "credential", active: true, agent_ids: ["codex"], confidence: 90, reasons: [], state: "idle" }] }),
  ],
  candidates: [
    { id: "provider-1", name: "Provider", protocol: "openai", base_url: "https://example.com", models: ["m"], source: "test", freshness: "unix:1", auth_mode: "credential", active: true, agent_ids: ["opencode", "codex"], confidence: 90, reasons: [], state: "idle" },
  ],
};

const connectedSnapshot: ConnectionSetupSnapshot = {
  ...emptySnapshot,
  agent_bindings: [
    binding({ agent_id: "opencode", agent_name: "OpenCode", installed: true, already_connected: true }),
  ],
  agent_route_follow: { schema_version: 1, enabled: true, state: "applied_verified", authorized_at_ms: 1 },
};

/** Both Agents are auto-resolvable; confirm-required stays eligible when its verified adapter can configure. */
const autoBatchSnapshot: ConnectionSetupSnapshot = {
  ...emptySnapshot,
  agent_bindings: [
    binding({
      agent_id: "opencode",
      agent_name: "OpenCode",
      installed: true,
      config_path: "/home/user/.opencode/config.json",
      resolution: "auto-resolved",
      resolved_candidate: {
        id: "provider-1", name: "Provider", protocol: "openai", base_url: "https://example.com",
        models: ["gpt-5"], source: "test", freshness: "unix:1", auth_mode: "credential",
        active: true, agent_ids: ["opencode"], confidence: 90, reasons: [], state: "idle",
      },
    }),
    binding({
      agent_id: "codex",
      agent_name: "Codex",
      installed: true,
      config_path: "/home/user/.codex/config.json",
      capability: "confirm-required",
      can_configure: true,
      resolution: "auto-resolved",
      resolved_candidate: {
        id: "provider-2", name: "Anthropic", protocol: "anthropic", base_url: "https://api.anthropic.com",
        models: ["claude-4"], source: "test", freshness: "unix:1", auth_mode: "credential",
        active: true, agent_ids: ["codex"], confidence: 90, reasons: [], state: "idle",
      },
    }),
  ],
  candidates: [
    {
      id: "provider-1", name: "Provider", protocol: "openai", base_url: "https://example.com",
      models: ["gpt-5"], source: "test", freshness: "unix:1", auth_mode: "credential",
      active: true, agent_ids: ["opencode"], confidence: 90, reasons: [], state: "idle",
    },
  ],
};

function renderPage(props: Partial<Parameters<typeof OnboardingPage>[0]> = {}, lang: "zh" | "en" = "zh") {
  const defaults = {
    gateState: "pending" as const,
    onComplete: vi.fn().mockResolvedValue(true),
    onDismiss: vi.fn(),
    onStartSetup: vi.fn(),
    onOpenOverview: vi.fn(),
    onStartService: vi.fn().mockResolvedValue({ ok: true, summary: "runtime started", refreshed: true }),
  };
  const merged = { ...defaults, ...props };
  return {
    user: userEvent.setup(),
    onComplete: merged.onComplete,
    onDismiss: merged.onDismiss,
    onStartSetup: merged.onStartSetup,
    onOpenOverview: merged.onOpenOverview,
    onStartService: merged.onStartService,
    ...render(
      <I18nContext.Provider value={{ lang, setLang: vi.fn() }}>
        <OnboardingPage {...merged} />
      </I18nContext.Provider>,
    ),
  };
}

/** Advances from the current step to a later step by clicking "下一步". */
async function advance(user: ReturnType<typeof userEvent.setup>, times: number) {
  for (let i = 0; i < times; i++) {
    await user.click(screen.getByRole("button", { name: "下一步" }));
  }
}

beforeEach(() => {
  mocks.connectionSetup.mockReturnValue({ snapshot: null, status: "idle", error: "", refresh: vi.fn().mockResolvedValue(null) });
  mocks.runtimeSlice.mockReturnValue(runningRuntime);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  mocks.invokeNative.mockReset();
  vi.restoreAllMocks();
});

describe("OnboardingPage first-run states", () => {
  it("opens on the welcome step explaining value and the two modes", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    renderPage();

    expect(screen.getByRole("heading", { level: 1, name: "欢迎使用 d-token" })).toBeInTheDocument();
    expect(screen.getByText(/本机防御性 AI Coding 控制面/)).toBeInTheDocument();
    expect(screen.getByText("模板向导")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始引导" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "直接进入工作台" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "稍后再说" })).toBeEnabled();
  });

  it("walks through automatic batch discovery, detection summary, and verification", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: autoBatchSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    expect(screen.getByText(/本地监听服务用于接收/)).toBeInTheDocument();
    expect(screen.getByText("服务运行中")).toBeInTheDocument();

    await advance(user, 1);
    expect(screen.getByText("已检测到 2 个 Agent")).toBeInTheDocument();
    // The Agent step is a read-only batch listing: no radio/select/checkbox.
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.getByText("OpenCode")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
    // Upstream/cc-switch discovery is folded into the Agent scan step (the
    // standalone upstream step was removed from the 5-step flow).
    expect(screen.getByText("已检测到 1 个上游候选")).toBeInTheDocument();

    await advance(user, 1);
    expect(screen.getByRole("heading", { name: "检测摘要" })).toBeInTheDocument();
    const summary = screen.getByRole("region", { name: "检测摘要" });
    expect(summary).toHaveTextContent("OpenCode");
    expect(summary).toHaveTextContent("Codex");
    expect(screen.getByRole("button", { name: "复制检测摘要" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始一键配置" })).toBeInTheDocument();

    await advance(user, 1);
    expect(screen.getByRole("button", { name: "完成引导" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "进入安装与连接完成验证" })).toBeEnabled();
  });

  it("derives positive verify evidence from the connected Agent snapshot and runtime", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);

    expect(within(screen.getByRole("group", { name: "检测结果" })).getByText("证据成立")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "接入声明" })).getByText("证据成立")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "验证证据" })).getByText("证据成立")).toBeInTheDocument();
    expect(screen.queryByText("连接已验证")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "完成引导" })).toBeEnabled();
  });

  it("does not treat enabled but non-verified route-follow state as verified", async () => {
    mocks.connectionSetup.mockReturnValue({
      snapshot: { ...connectedSnapshot, agent_route_follow: { schema_version: 1, enabled: true, state: "applied", authorized_at_ms: 1 } },
      status: "ready",
      error: "",
      refresh: vi.fn().mockResolvedValue(null),
    });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);

    const verified = screen.getByRole("group", { name: "验证证据" });
    expect(within(verified).getByText("未知")).toBeInTheDocument();
    expect(verified).toHaveAttribute("data-evidence-health", "muted");
  });

  it("shows unknown muted verify states when detection has no connection evidence", async () => {
    mocks.connectionSetup.mockReturnValue({
      snapshot: { ...emptySnapshot, agent_bindings: [binding({ agent_id: "opencode", agent_name: "OpenCode", installed: true })] },
      status: "ready",
      error: "",
      refresh: vi.fn().mockResolvedValue(null),
    });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);

    expect(within(screen.getByRole("group", { name: "检测结果" })).getByText("证据成立")).toBeInTheDocument();
    const connected = screen.getByRole("group", { name: "接入声明" });
    const verified = screen.getByRole("group", { name: "验证证据" });
    expect(within(connected).getByText("未知")).toBeInTheDocument();
    expect(within(verified).getByText("未知")).toBeInTheDocument();
    expect(connected).toHaveAttribute("data-evidence-health", "muted");
    expect(verified).toHaveAttribute("data-evidence-health", "muted");
    expect(screen.getByRole("button", { name: "完成引导" })).toBeEnabled();
    expect(screen.queryByText("连接已验证")).not.toBeInTheDocument();
  });

  it("reports discovery unavailable as unknown instead of fabricating evidence", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: null, status: "unavailable", error: "connection_setup_unavailable", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);

    for (const name of ["检测结果", "接入声明", "验证证据"]) {
      const evidence = screen.getByRole("group", { name });
      expect(within(evidence).getByText("未知")).toBeInTheDocument();
      expect(evidence).toHaveAttribute("data-evidence-health", "muted");
    }
    expect(screen.getByRole("button", { name: "完成引导" })).toBeEnabled();
  });

  it("shows the browser-preview banner and blocks writes when native is unavailable", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: null, status: "unavailable", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage({ gateState: "unavailable" });

    expect(screen.getByText(/浏览器预览/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);
    expect(screen.getByRole("button", { name: "完成引导" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "稍后再说" })).toBeEnabled();
  });


  it("shows negative evidence without green when no visible Agent is present", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);

    const detected = screen.getByRole("group", { name: "检测结果" });
    expect(within(detected).getByText("未通过")).toBeInTheDocument();
    expect(detected).toHaveAttribute("data-evidence-health", "warning");
    expect(within(screen.getByRole("group", { name: "接入声明" })).getByText("未知")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "验证证据" })).getByText("未知")).toBeInTheDocument();
  });
  it("re-entering a completed wizard starts at welcome and keeps the full flow browsable", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onOpenOverview } = renderPage({ gateState: "completed" });

    expect(screen.getByRole("heading", { level: 1, name: "欢迎使用 d-token" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始引导" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);
    expect(screen.getByRole("group", { name: "检测结果" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "完成引导" })).toBeEnabled();

    expect(screen.getByRole("button", { name: "返回总体概览" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回总体概览" }));
    expect(onOpenOverview).toHaveBeenCalledTimes(1);
  });
});

describe("OnboardingPage actions", () => {
  it("calls onComplete only from the final verify step", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const failingComplete = vi.fn().mockResolvedValue(false);
    const { user, onDismiss, onStartSetup } = renderPage({ onComplete: failingComplete });

    expect(screen.queryByRole("button", { name: "完成引导" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);
    await user.click(screen.getByRole("button", { name: "完成引导" }));
    expect(failingComplete).toHaveBeenCalledTimes(1);
    expect(onDismiss).toBeDefined();
    expect(onStartSetup).toBeDefined();
    expect(screen.getByRole("button", { name: "稍后再说" })).toBeInTheDocument();
  });

  it("hands the automatic batch to the one-click setup flow without an Agent id", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: autoBatchSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onStartSetup } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 2);
    await user.click(screen.getByRole("button", { name: "开始一键配置" }));

    expect(onStartSetup).toHaveBeenCalledWith();
  });

  it("auto-includes an auto-resolved confirm-required Agent with a verified write action", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: autoBatchSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onStartSetup } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 1);
    expect(screen.getAllByText("可自动纳入")).toHaveLength(2);
    expect(screen.queryByText("来源冲突")).not.toBeInTheDocument();

    await advance(user, 1);
    await user.click(screen.getByRole("button", { name: "开始一键配置" }));
    expect(onStartSetup).toHaveBeenCalledWith();
  });

  it("keeps an auto-resolved Agent without a verified write action blocked", async () => {
    const blockedSnapshot: ConnectionSetupSnapshot = {
      ...autoBatchSnapshot,
      agent_bindings: autoBatchSnapshot.agent_bindings.map((item) => item.agent_id === "codex"
        ? { ...item, can_configure: false }
        : item),
    };
    mocks.connectionSetup.mockReturnValue({ snapshot: blockedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onStartSetup } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 1);
    expect(screen.getByText("来源冲突")).toBeInTheDocument();
    await advance(user, 1);
    expect(screen.getByRole("button", { name: "开始一键配置" })).toBeDisabled();
    expect(onStartSetup).not.toHaveBeenCalled();
  });

  it("shows the automatic batch count and per-Agent current provider/model/protocol/endpoint/target", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: autoBatchSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 2);

    expect(screen.getByText("本次将自动配置 2 个 Agent")).toBeInTheDocument();
    expect(screen.getByText("OpenCode")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(screen.getAllByText("Provider / Model")).toHaveLength(2);
    expect(screen.getByText("Provider / gpt-5")).toBeInTheDocument();
    expect(screen.getByText("Anthropic / claude-4")).toBeInTheDocument();
    expect(screen.getByText("openai")).toBeInTheDocument();
    expect(screen.getByText("anthropic")).toBeInTheDocument();
    expect(screen.getByText("https://example.com")).toBeInTheDocument();
    expect(screen.getByText("https://api.anthropic.com")).toBeInTheDocument();
    expect(screen.getAllByText("目标路径")).toHaveLength(2);
    expect(screen.getByText("/home/user/.opencode/config.json")).toBeInTheDocument();
    expect(screen.getByText("/home/user/.codex/config.json")).toBeInTheDocument();
  });

  it("blocks the whole one-click batch when any Agent has a source conflict", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onStartSetup } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 1);
    expect(screen.getByText("可自动纳入")).toBeInTheDocument();
    expect(screen.getByText("来源冲突")).toBeInTheDocument();
    expect(screen.getByText("修复来源后重新扫描")).toBeInTheDocument();

    await advance(user, 1);
    expect(screen.getByRole("button", { name: "开始一键配置" })).toBeDisabled();
    expect(screen.getByText(/一键配置已阻断/)).toBeInTheDocument();
    expect(onStartSetup).not.toHaveBeenCalled();
  });

  it("notes next to one-click setup that the next screen shows a diff and asks for confirmation", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 2);

    expect(screen.getByRole("button", { name: "开始一键配置" })).toBeInTheDocument();
    expect(screen.getByText(/先展示配置 diff 与备份摘要/)).toBeInTheDocument();
    expect(screen.getByText(/不会静默/)).toBeInTheDocument();
  });

  it("enters Install & Connect from the verify step without an Agent id", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onStartSetup } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);

    await user.click(screen.getByRole("button", { name: "进入安装与连接完成验证" }));
    expect(onStartSetup).toHaveBeenCalledWith();
  });

  it("shows an explicit start-and-check button on the service step", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));

    const start = screen.getByRole("button", { name: "启动并检查本地服务" });
    expect(start).toBeInTheDocument();
    expect(start).toBeEnabled();
  });

  it("disables the start button and keeps the preview banner when native is unavailable", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: null, status: "unavailable", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onStartService } = renderPage({ gateState: "unavailable" });

    await user.click(screen.getByRole("button", { name: "开始引导" }));

    expect(screen.getByRole("button", { name: "启动并检查本地服务" })).toBeDisabled();
    expect(onStartService).not.toHaveBeenCalled();
    expect(screen.getByText(/浏览器预览/)).toBeInTheDocument();
  });

  it("runs the service action with a loading state while the native call is pending", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    let resolveStart!: (value: { ok: boolean; summary: string; refreshed: boolean }) => void;
    const pending = new Promise<{ ok: boolean; summary: string; refreshed: boolean }>((resolve) => { resolveStart = resolve; });
    const onStartService = vi.fn().mockReturnValue(pending);
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    const loading = screen.getByRole("button", { name: "正在启动并检查本地服务…" });
    expect(loading).toBeDisabled();

    resolveStart({ ok: true, summary: "runtime started", refreshed: true });
    await waitFor(() => expect(screen.getByRole("button", { name: "启动并检查本地服务" })).toBeEnabled());
    expect(onStartService).toHaveBeenCalledTimes(1);
  });

  it("shows success only when the native receipt and the refresh evidence are present", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const onStartService = vi.fn().mockResolvedValue({ ok: true, summary: "runtime started", refreshed: true });
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByText("服务已启动，尚未完成 Agent 接入")).toBeInTheDocument());
    expect(onStartService).toHaveBeenCalledTimes(1);
  });

  it("never shows success when the refresh evidence is missing after a native receipt", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const onStartService = vi.fn().mockResolvedValue({ ok: true, summary: "runtime started", refreshed: false });
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByText(/启动失败或状态刷新未确认/)).toBeInTheDocument());
    expect(screen.queryByText("服务已启动，尚未完成 Agent 接入")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
  });

  it("reports a failed service start and offers retry", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const onStartService = vi.fn().mockResolvedValueOnce({ ok: false, summary: "transaction failed", refreshed: false })
      .mockResolvedValueOnce({ ok: true, summary: "runtime started", refreshed: true });
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByText(/启动失败或状态刷新未确认/)).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
    expect(screen.queryByText("服务已启动，尚未完成 Agent 接入")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(screen.getByText("服务已启动，尚未完成 Agent 接入")).toBeInTheDocument());
    expect(onStartService).toHaveBeenCalledTimes(2);
  });

  it("keeps the welcome workbench jump unscoped", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onStartSetup } = renderPage();

    await user.click(screen.getByRole("button", { name: "直接进入工作台" }));

    expect(onStartSetup).toHaveBeenCalledWith();
  });

  it("calls onDismiss from the later action", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onDismiss } = renderPage();

    await user.click(screen.getByRole("button", { name: "稍后再说" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("copies the metadata-only detection summary reliably", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 2);

    await user.click(screen.getByRole("button", { name: "复制检测摘要" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining("d-token 检测摘要"));
    expect(screen.getByRole("button", { name: "已复制到剪贴板" })).toBeInTheDocument();
  });
});

describe("OnboardingPage i18n, keyboard, and reduced motion", () => {
  it("renders English copy when the app language is English", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    renderPage({}, "en");

    expect(screen.getByRole("heading", { level: 1, name: "Welcome to d-token" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start guided setup" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Go to workbench" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Maybe later" })).toBeInTheDocument();
  });

  it("activates the completion action with the keyboard on the verify step", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onComplete } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 3);

    const complete = screen.getByRole("button", { name: "完成引导" });
    complete.focus();
    expect(complete).toHaveFocus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(onComplete).toHaveBeenCalledTimes(1));
  });

  it("marks reduced motion on the wizard root", () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { container } = renderPage({ reducedMotion: true });

    expect(container.querySelector("[data-reduced-motion='true']")).toBeInTheDocument();
  });

  it("keeps the normal-motion path when reduced motion is not requested", () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { container } = renderPage({ reducedMotion: false });

    expect(container.querySelector("[data-reduced-motion='false']")).toBeInTheDocument();
  });
});

describe("OnboardingPage service order gate (UI-9)", () => {
  const stoppedRuntime = {
    payload: { desktop_service_url: "" },
    status: "idle" as const,
    operational: { service: { data: { state: "stopped", pid_running: false, managed: true } } },
    updatedAt: null,
  };

  it("locks stepper steps after service until the service transaction succeeds and refreshes", async () => {
    mocks.runtimeSlice.mockReturnValue(stoppedRuntime);
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    let resolveStart!: (value: { ok: boolean; summary: string; refreshed: boolean }) => void;
    const pending = new Promise<{ ok: boolean; summary: string; refreshed: boolean }>((resolve) => { resolveStart = resolve; });
    const onStartService = vi.fn().mockReturnValue(pending);
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));

    // Service stopped: welcome/service stay open, agents/config/verify locked,
    // and the Next button cannot leave the service step either.
    const stepper = screen.getByRole("list");
    expect(within(stepper).getByRole("button", { name: /欢迎/ })).toBeEnabled();
    expect(within(stepper).getByRole("button", { name: /本地服务/ })).toBeEnabled();
    expect(within(stepper).getByRole("button", { name: /Agent/ })).toBeDisabled();
    expect(within(stepper).getByRole("button", { name: /配置/ })).toBeDisabled();
    expect(within(stepper).getByRole("button", { name: /验证/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "下一步" })).toBeDisabled();
    expect(screen.getByText(/本地监听服务用于接收/)).toBeInTheDocument();

    // A locked click never navigates (no jump to config/verify).
    await user.click(within(stepper).getByRole("button", { name: /配置/ }));
    expect(screen.getByText(/本地监听服务用于接收/)).toBeInTheDocument();

    // Starting the service (receipt + refresh) unlocks the rest of the flow.
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));
    expect(screen.getByRole("button", { name: /正在启动并检查本地服务/ })).toBeDisabled();

    resolveStart({ ok: true, summary: "runtime started", refreshed: true });
    await waitFor(() => expect(within(stepper).getByRole("button", { name: /Agent/ })).toBeEnabled());
    expect(within(stepper).getByRole("button", { name: /配置/ })).toBeEnabled();
    expect(within(stepper).getByRole("button", { name: /验证/ })).toBeEnabled();
    expect(screen.getByRole("button", { name: "下一步" })).toBeEnabled();

    // The stepper now navigates to config as a normal 5-segment progression.
    await user.click(within(stepper).getByRole("button", { name: /配置/ }));
    expect(screen.getByRole("heading", { name: "检测摘要" })).toBeInTheDocument();
  });

  it("keeps future steps locked when the start failed (no refresh evidence)", async () => {
    mocks.runtimeSlice.mockReturnValue(stoppedRuntime);
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const onStartService = vi.fn().mockResolvedValue({ ok: false, summary: "transaction failed", refreshed: false });
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByText(/启动失败或状态刷新未确认/)).toBeInTheDocument());
    const stepper = screen.getByRole("list");
    expect(within(stepper).getByRole("button", { name: /Agent/ })).toBeDisabled();
    expect(within(stepper).getByRole("button", { name: /配置/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "下一步" })).toBeDisabled();
  });
});

describe("OnboardingPage service step progress (UI-8)", () => {
  const stoppedRuntime = {
    payload: { desktop_service_url: "" },
    status: "idle" as const,
    operational: { service: { data: { state: "stopped", pid_running: false, managed: true } } },
    updatedAt: null,
  };

  it("shows the four start phases with an indeterminate breathing marker while the transaction runs", async () => {
    mocks.runtimeSlice.mockReturnValue(stoppedRuntime);
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    let resolveStart!: (value: { ok: boolean; summary: string; refreshed: boolean }) => void;
    const pending = new Promise<{ ok: boolean; summary: string; refreshed: boolean }>((resolve) => { resolveStart = resolve; });
    const onStartService = vi.fn().mockReturnValue(pending);
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));

    // All four phase labels are visible before anything runs.
    const progress = screen.getByTestId("service-step-progress");
    for (const label of ["检查", "备份", "启动", "验证"]) {
      expect(within(progress).getByText(label)).toBeInTheDocument();
    }
    expect(progress.querySelector('[data-progress-indeterminate="false"]')).toBeTruthy();

    // Loading: indeterminate breathing marker, never a connected/verified claim.
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));
    await waitFor(() => expect(progress.querySelector('[data-progress-indeterminate="true"]')).toBeTruthy());
    const dot = document.querySelector('[data-progress-indeterminate-dot="true"]');
    expect(dot).not.toBeNull();
    expect(dot).toHaveClass("animate-pulse");
    expect(screen.getByRole("button", { name: /正在启动并检查本地服务/ })).toBeDisabled();
    expect(screen.queryByText(/已连接|已接入/)).not.toBeInTheDocument();

    // Success: all phases done, honest copy that running ≠ Agent connected.
    resolveStart({ ok: true, summary: "runtime started", refreshed: true });
    await waitFor(() => expect(progress.querySelector('[data-progress-done="true"]')).toBeTruthy());
    expect(screen.getByText("服务已启动，尚未完成 Agent 接入")).toBeInTheDocument();
    expect(screen.queryByText(/已连接/)).not.toBeInTheDocument();
  });

  it("keeps the progress static and readable under reduced motion", async () => {
    mocks.runtimeSlice.mockReturnValue(stoppedRuntime);
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    let resolveStart!: (value: { ok: boolean; summary: string; refreshed: boolean }) => void;
    const pending = new Promise<{ ok: boolean; summary: string; refreshed: boolean }>((resolve) => { resolveStart = resolve; });
    const onStartService = vi.fn().mockReturnValue(pending);
    const { user } = renderPage({ onStartService, reducedMotion: true });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    const progress = screen.getByTestId("service-step-progress");
    expect(progress.querySelector('[data-reduced-motion="true"]')).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));
    await waitFor(() => expect(progress.querySelector('[data-progress-indeterminate="true"]')).toBeTruthy());
    // Static equivalent: no pulse/transform animation, labels stay readable.
    const dot = document.querySelector('[data-progress-indeterminate-dot="true"]');
    expect(dot).not.toBeNull();
    expect(dot).not.toHaveClass("animate-pulse");
    for (const label of ["检查", "备份", "启动", "验证"]) {
      expect(within(progress).getByText(label)).toBeInTheDocument();
    }
    resolveStart({ ok: true, summary: "runtime started", refreshed: true });
  });

  it("clarifies that a running service is not a connected Agent (H-1)", async () => {
    mocks.runtimeSlice.mockReturnValue(runningRuntime);
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));

    expect(screen.getByText("服务运行中")).toBeInTheDocument();
    expect(screen.getByRole("note")).toHaveTextContent(/≠ 已连接/);
  });
});
