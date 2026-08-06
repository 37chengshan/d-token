import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
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
    ...overrides,
  } as AgentUpstreamBinding;
}

const detectedSnapshot: ConnectionSetupSnapshot = {
  ...emptySnapshot,
  agent_bindings: [
    binding({ agent_id: "opencode", agent_name: "OpenCode", installed: true, current_config_source: "agent-config:opencode" }),
    binding({ agent_id: "codex", agent_name: "Codex", installed: true, resolution: "needs-credential", candidates: [{ id: "provider-1", name: "Provider", protocol: "openai", models: ["m"], source: "test", freshness: "unix:1", auth_mode: "credential", active: true, agent_ids: ["codex"], confidence: 90, reasons: [], state: "idle" }] }),
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
  agent_route_follow: { schema_version: 1, enabled: true, state: "enabled", authorized_at_ms: 1 },
};

function renderPage(props: Partial<Parameters<typeof OnboardingPage>[0]> = {}, lang: "zh" | "en" = "zh") {
  const defaults = {
    gateState: "pending" as const,
    onComplete: vi.fn().mockResolvedValue(true),
    onDismiss: vi.fn(),
    onStartSetup: vi.fn(),
    onOpenOverview: vi.fn(),
  };
  const merged = { ...defaults, ...props };
  return {
    user: userEvent.setup(),
    onComplete: merged.onComplete,
    onDismiss: merged.onDismiss,
    onStartSetup: merged.onStartSetup,
    onOpenOverview: merged.onOpenOverview,
    ...render(
      <I18nContext.Provider value={{ lang, setLang: vi.fn() }}>
        <OnboardingPage {...merged} />
      </I18nContext.Provider>,
    ),
  };
}

beforeEach(() => {
  mocks.connectionSetup.mockReturnValue({ snapshot: null, status: "idle", error: "", refresh: vi.fn().mockResolvedValue(null) });
  mocks.runtimeSlice.mockReturnValue(runningRuntime);
});

afterEach(() => {
  cleanup();
  mocks.invokeNative.mockReset();
});

describe("OnboardingPage first-run states", () => {
  it("shows discovery summary and blocked completion when nothing is configured yet", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: detectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    renderPage();

    expect(screen.getByRole("heading", { level: 1, name: "欢迎使用 d-token" })).toBeInTheDocument();
    expect(screen.getByText("尚未完成配置")).toBeInTheDocument();
    expect(screen.getByText("已检测到 2 个 Agent")).toBeInTheDocument();
    expect(screen.getByText("已检测到 1 个上游候选")).toBeInTheDocument();
    expect(screen.getByText("已检测到，待确认")).toBeInTheDocument();
    expect(screen.getByText("需要凭据")).toBeInTheDocument();

    const complete = screen.getByRole("button", { name: "完成首次配置" });
    expect(complete).toBeDisabled();
    expect(screen.getByRole("button", { name: "开始配置" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "稍后再说" })).toBeEnabled();
  });

  it("enables completion and reports connected state only from native connected evidence", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    renderPage();

    expect(screen.getByText("检测到已接入的 Agent 或连接路径（来自本机状态）。")).toBeInTheDocument();
    const complete = screen.getByRole("button", { name: "完成首次配置" });
    expect(complete).toBeEnabled();
  });

  it("never claims completion from mere detection (detected ≠ connected)", async () => {
    mocks.connectionSetup.mockReturnValue({
      snapshot: { ...emptySnapshot, agent_bindings: [binding({ agent_id: "opencode", agent_name: "OpenCode", installed: true })] },
      status: "ready",
      error: "",
      refresh: vi.fn().mockResolvedValue(null),
    });
    renderPage();

    expect(screen.getByText("尚未完成配置")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "完成首次配置" })).toBeDisabled();
    expect(screen.queryByText(/已接入/)).not.toBeInTheDocument();
  });

  it("reports discovery unavailable honestly instead of fabricating zeros", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: null, status: "unavailable", error: "connection_setup_unavailable", refresh: vi.fn().mockResolvedValue(null) });
    renderPage();

    expect(screen.getByText("状态不可用")).toBeInTheDocument();
    expect(screen.getByText("配置状态暂不可用")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "完成首次配置" })).toBeDisabled();
  });

  it("shows the browser-preview banner and blocks writes when native is unavailable", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: null, status: "unavailable", error: "", refresh: vi.fn().mockResolvedValue(null) });
    renderPage({ gateState: "unavailable" });

    expect(screen.getByText(/浏览器预览/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "完成首次配置" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "稍后再说" })).toBeEnabled();
  });

  it("shows the completed state with a back-to-overview action when re-entered after completion", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onOpenOverview } = renderPage({ gateState: "completed" });

    expect(screen.getByText(/首次配置已完成/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回总体概览" }));
    expect(onOpenOverview).toHaveBeenCalledTimes(1);
  });
});

describe("OnboardingPage actions", () => {
  it("calls onComplete from the completion button and keeps the wizard open on failure", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const failingComplete = vi.fn().mockResolvedValue(false);
    const { user, onDismiss, onStartSetup } = renderPage({ onComplete: failingComplete });

    await user.click(screen.getByRole("button", { name: "完成首次配置" }));
    expect(failingComplete).toHaveBeenCalledTimes(1);
    // The wizard stays open: dismiss and start-setup remain available.
    expect(onDismiss).toBeDefined();
    expect(onStartSetup).toBeDefined();
    expect(screen.getByRole("button", { name: "稍后再说" })).toBeInTheDocument();
  });

  it("calls onStartSetup and onDismiss from the action buttons", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onStartSetup, onDismiss } = renderPage();

    await user.click(screen.getByRole("button", { name: "开始配置" }));
    expect(onStartSetup).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "稍后再说" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});

describe("OnboardingPage i18n, keyboard, and reduced motion", () => {
  it("renders English copy when the app language is English", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: emptySnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    renderPage({}, "en");

    expect(screen.getByRole("heading", { level: 1, name: "Welcome to d-token" })).toBeInTheDocument();
    expect(screen.getByText("Setup is not complete yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Complete first-run setup" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start setup" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Maybe later" })).toBeInTheDocument();
  });

  it("activates the completion action with the keyboard", async () => {
    mocks.connectionSetup.mockReturnValue({ snapshot: connectedSnapshot, status: "ready", error: "", refresh: vi.fn().mockResolvedValue(null) });
    const { user, onComplete } = renderPage();

    const complete = screen.getByRole("button", { name: "完成首次配置" });
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
