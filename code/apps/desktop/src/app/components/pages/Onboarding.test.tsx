import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  connectionSetup: vi.fn(),
  runtimeSlice: vi.fn(),
  addNotification: vi.fn(),
}));

vi.mock("../../lib/connectionSetup", () => ({
  useConnectionSetup: () => mocks.connectionSetup(),
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboardSelector: () => mocks.runtimeSlice(),
  shallowEqualSlice: (a: unknown) => a,
}));

vi.mock("../../lib/notifications", () => ({
  createOperationRequestId: (scope: string) => `${scope}:test-request`,
  useNotifications: () => ({ addNotification: mocks.addNotification }),
}));

import { I18nContext } from "../../lib/i18n";
import { resetInstallMachineState } from "../../lib/installMachine";
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

const stoppedRuntime = {
  payload: null,
  status: "stopped" as const,
  operational: {
    service: { data: { state: "stopped", pid_running: false, managed: true, service_url: "" } },
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

const autoBatchSnapshot: ConnectionSetupSnapshot = {
  ...emptySnapshot,
  candidates: [
    {
      id: "provider-1",
      name: "Provider",
      protocol: "openai",
      base_url: "https://example.com",
      models: ["gpt-5"],
      source: "test",
      freshness: "unix:1",
      auth_mode: "credential",
      active: true,
      agent_ids: ["opencode"],
      confidence: 90,
      reasons: [],
      state: "idle",
    },
  ],
  agent_bindings: [
    binding({
      agent_id: "opencode",
      agent_name: "OpenCode",
      config_path: "/home/user/.opencode/config.json",
      resolved_candidate: {
        id: "provider-1",
        name: "Provider",
        protocol: "openai",
        base_url: "https://example.com",
        models: ["gpt-5"],
        source: "test",
        freshness: "unix:1",
        auth_mode: "credential",
        active: true,
        agent_ids: ["opencode"],
        confidence: 90,
        reasons: [],
        state: "idle",
      },
    }),
    binding({
      agent_id: "codex",
      agent_name: "Codex",
      config_path: "/home/user/.codex/config.json",
      resolved_candidate: {
        id: "provider-2",
        name: "Responses",
        protocol: "responses",
        base_url: "http://127.0.0.1:10100/v1",
        models: [],
        source: "test",
        freshness: "unix:1",
        auth_mode: "agent-managed",
        active: true,
        agent_ids: ["codex"],
        confidence: 90,
        reasons: [],
        state: "idle",
      },
    }),
  ],
};

const mixedSnapshot: ConnectionSetupSnapshot = {
  ...emptySnapshot,
  agent_bindings: [
    binding({ agent_id: "opencode", agent_name: "OpenCode", installed: true, already_connected: true }),
    binding({ agent_id: "cursor", agent_name: "Cursor", installed: true, integration_kind: "mcp", resolution: "unsupported", can_configure: true }),
    binding({ agent_id: "codex", agent_name: "Codex", installed: true, resolution: "needs-credential", can_configure: false }),
    binding({ agent_id: "aider", agent_name: "Aider", installed: true, resolution: "needs-choice", can_configure: true }),
  ],
};

const connectedSnapshot: ConnectionSetupSnapshot = {
  ...emptySnapshot,
  agent_bindings: [binding({ agent_id: "opencode", agent_name: "OpenCode", installed: true, already_connected: true })],
  agent_route_follow: { schema_version: 1, enabled: true, state: "applied_verified", authorized_at_ms: 1 },
};

function renderPage(props: Partial<Parameters<typeof OnboardingPage>[0]> = {}, lang: "zh" | "en" = "zh") {
  const defaults = {
    gateState: "pending" as const,
    onComplete: vi.fn().mockResolvedValue(true),
    onDismiss: vi.fn(),
    onStartSetup: vi.fn(),
    onOpenConnections: vi.fn(),
    onOpenOverview: vi.fn(),
    onStartService: vi.fn().mockResolvedValue({ ok: true, summary: "runtime started", refreshed: true }),
  };
  const merged = { ...defaults, ...props };
  return {
    user: userEvent.setup(),
    onComplete: merged.onComplete,
    onDismiss: merged.onDismiss,
    onStartSetup: merged.onStartSetup,
    onOpenConnections: merged.onOpenConnections,
    onOpenOverview: merged.onOpenOverview,
    onStartService: merged.onStartService,
    ...render(
      <I18nContext.Provider value={{ lang, setLang: vi.fn() }}>
        <OnboardingPage {...merged} />
      </I18nContext.Provider>,
    ),
  };
}

async function advance(user: ReturnType<typeof userEvent.setup>, times: number) {
  for (let i = 0; i < times; i++) {
    await user.click(screen.getByRole("button", { name: "下一步" }));
  }
}

beforeEach(() => {
  window.sessionStorage.clear();
  resetInstallMachineState();
  mocks.connectionSetup.mockReturnValue({ snapshot: autoBatchSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(autoBatchSnapshot) });
  mocks.runtimeSlice.mockReturnValue(runningRuntime);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  mocks.addNotification.mockReset();
  vi.restoreAllMocks();
});

describe("OnboardingPage five-step release flow", () => {
  it("renders exactly five steps and removes the standalone upstream step", async () => {
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));

    const stepper = screen.getByRole("list", { name: /第 2 步，共 5 步/ });
    expect(within(stepper).queryByRole("button", { name: /上游/ })).not.toBeInTheDocument();
    for (const name of [/欢迎/, /本地服务/, /编码工具/, /配置/, /验证/]) {
      expect(within(stepper).getByRole("button", { name })).toBeInTheDocument();
    }
  });

  it("locks later steps until the local service is actually running", async () => {
    mocks.runtimeSlice.mockReturnValue(stoppedRuntime);
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));

    expect(screen.getByRole("button", { name: /编码工具/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /配置/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /验证/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "下一步" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "启动并检查本地服务" })).toBeInTheDocument();
  });

  it("keeps configuration locked while discovery is still running", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: null, status: "discovering", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 1);

    expect(screen.getByRole("button", { name: /配置/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "下一步" })).toBeDisabled();
    expect(screen.getByText(/正在检查首次配置状态/)).toBeInTheDocument();
  });

  it("unlocks the final use step only after one-click configuration is explicitly started", async () => {
    const { user, onStartSetup, onComplete } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 2);

    expect(screen.getByRole("button", { name: /验证/ })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "完成引导" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "开始一键接入" }));
    expect(onStartSetup).toHaveBeenCalledTimes(1);
    expect(onComplete).not.toHaveBeenCalled();

    await advance(user, 1);
    expect(screen.getByRole("button", { name: "完成引导" })).toBeEnabled();
  });

  it("opens connection status without treating it as configuration authorization", async () => {
    const { user, onOpenConnections, onStartSetup, onStartService } = renderPage();

    await user.click(screen.getByRole("button", { name: "查看连接状态" }));

    expect(onOpenConnections).toHaveBeenCalledTimes(1);
    expect(onStartSetup).not.toHaveBeenCalled();
    expect(onStartService).not.toHaveBeenCalled();
  });
});

describe("OnboardingPage service feedback", () => {
  it("shows transaction stages and reports success only with native receipt plus refresh evidence", async () => {
    const { user, onStartService } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    expect(screen.getByText("启动本地服务")).toBeInTheDocument();
    expect(screen.getByText("确认运行状态")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByText("本地服务已启动并完成状态检查。")).toBeInTheDocument());
    expect(onStartService).toHaveBeenCalledTimes(1);
    expect(mocks.addNotification).toHaveBeenCalledWith(expect.objectContaining({
      dedupeKey: "onboarding-service-started",
      level: "success",
      receipt: expect.objectContaining({ requestId: "onboarding-service-start:test-request", targetId: "local-service" }),
    }));
    expect(mocks.addNotification.mock.calls[0][0]).not.toHaveProperty("actionPage");
  });

  it("treats a start receipt without refreshed state as partial rather than success", async () => {
    const onStartService = vi.fn().mockResolvedValue({ ok: true, summary: "refresh unavailable", refreshed: false });
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByText(/已收到启动结果，但状态还未确认/)).toBeInTheDocument());
    expect(screen.queryByText("本地服务已启动并完成状态检查。")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
    expect(mocks.addNotification).toHaveBeenCalledWith(expect.objectContaining({ level: "warning" }));
  });

  it("reports a failed service start and keeps later steps locked while runtime remains stopped", async () => {
    mocks.runtimeSlice.mockReturnValue(stoppedRuntime);
    const onStartService = vi.fn().mockResolvedValue({ ok: false, summary: "transaction failed", refreshed: false });
    const { user } = renderPage({ onStartService });

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await user.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByText(/启动未完成，请重试或打开安装与连接检查/)).toBeInTheDocument());
    expect(screen.getByRole("button", { name: /编码工具/ })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "完成引导" })).not.toBeInTheDocument();
  });

  it("renders service progress without a spinner when reduced motion is requested", async () => {
    const { user, container } = renderPage({ reducedMotion: true });

    await user.click(screen.getByRole("button", { name: "开始引导" }));

    expect(container.querySelector("[data-reduced-motion='true']")).toBeInTheDocument();
    expect(screen.getByText("启动本地服务")).toBeInTheDocument();
    expect(container.querySelector(".animate-spin")).toBeNull();
  });
});

describe("OnboardingPage Agent selection and evidence", () => {
  it("shows installed configurable Agents as selected checkboxes without provider/model controls", async () => {
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 1);

    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes).toHaveLength(2);
    expect(checkboxes[0]).toBeChecked();
    expect(checkboxes[1]).toBeChecked();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  });

  it("lets the user cancel a selected Agent and carries the selected count forward", async () => {
    const { user, onStartSetup } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 1);
    await user.click(screen.getAllByRole("checkbox")[1]);

    await advance(user, 1);
    expect(screen.getByText("本次将接入 1 个编码工具")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "开始一键接入" }));
    expect(onStartSetup).toHaveBeenCalledTimes(1);
  });

  it("keeps unsupported or credential-blocked Agents disabled and uses product wording", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: mixedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(mixedSnapshot) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 1);

    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes[0]).toBeChecked();
    expect(checkboxes[1]).toBeDisabled();
    expect(checkboxes[2]).toBeDisabled();
    expect(checkboxes[3]).toBeDisabled();
    expect(screen.getByText("已配置")).toBeInTheDocument();
    expect(screen.getByText("工具集成待配置")).toBeInTheDocument();
    expect(screen.getByText("需要 API Key")).toBeInTheDocument();
    expect(screen.getByText("配置失败")).toBeInTheDocument();
    expect(screen.queryByText(/来源冲突|等待安全适配|仅检测/)).not.toBeInTheDocument();
  });

  it("blocks the one-click button when every configurable Agent is deselected", async () => {
    const { user, onStartSetup } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 1);
    for (const checkbox of screen.getAllByRole("checkbox")) await user.click(checkbox);
    await advance(user, 1);

    expect(screen.getByText("请至少选择一个可接入的编码工具。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始一键接入" })).toBeDisabled();
    expect(onStartSetup).not.toHaveBeenCalled();
  });

  it("keeps detected, connected, and verified evidence separate on the final step", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(connectedSnapshot) });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 2);
    await user.click(screen.getByRole("button", { name: "开始一键接入" }));
    await advance(user, 1);

    expect(within(screen.getByRole("group", { name: "检测结果" })).getByText("证据成立")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "接入声明" })).getByText("证据成立")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "验证证据" })).getByText("证据成立")).toBeInTheDocument();
    expect(screen.queryByText("连接已验证")).not.toBeInTheDocument();
  });

  it("does not treat enabled but non-verified route-follow state as verified", async () => {
    mocks.connectionSetup.mockReturnValue({
      snapshot: { ...connectedSnapshot, agent_route_follow: { schema_version: 1, enabled: true, state: "applied", authorized_at_ms: 1 } },
      status: "ready",
      error: "",
      refresh: vi.fn().mockResolvedValue(connectedSnapshot),
    });
    const { user } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 2);
    await user.click(screen.getByRole("button", { name: "开始一键接入" }));
    await advance(user, 1);

    const verified = screen.getByRole("group", { name: "验证证据" });
    expect(within(verified).getByText("未知")).toBeInTheDocument();
    expect(verified).toHaveAttribute("data-evidence-health", "muted");
  });
});

describe("OnboardingPage completion and accessibility", () => {
  it("calls onComplete only from the final step", async () => {
    const { user, onComplete } = renderPage();

    expect(screen.queryByRole("button", { name: "完成引导" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "开始引导" }));
    await advance(user, 2);
    await user.click(screen.getByRole("button", { name: "开始一键接入" }));
    await advance(user, 1);
    await user.click(screen.getByRole("button", { name: "完成引导" }));

    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("renders English copy when the app language is English", () => {
    renderPage({}, "en");

    expect(screen.getByRole("heading", { level: 1, name: "Welcome to d-token" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start guided setup" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "View connection status" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Maybe later" })).toBeInTheDocument();
  });
});
