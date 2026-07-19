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
}));

import { dispatchInstallMachine, resetInstallMachineState } from "../../lib/installMachine";

import {
  assertNativeActionSucceeded,
  connectionDiscoveryCounts,
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
  selection_required: false,
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
    already_connected: false,
  }],
  sources: [],
  selection_required: false,
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
      selection_required: false,
      cc_switch_sync: null,
      writes_performed: false,
      privacy: "redacted",
    })).toEqual({ agents: 1, upstreams: 1, ccSwitchProfiles: 3 });
  });

  it("stores a required credential through the secure command without rendering it when native storage fails", async () => {
    const credential = "test-credential-never-rendered";
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "discover_upstream_candidates") return credentialRequiredSnapshot;
      if (command === "store_provider_credential") throw new Error(`keychain rejected ${credential}`);
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);

    fireEvent.click(await screen.findByRole("checkbox", { name: "加入本次连接: OpenCode" }));
    fireEvent.click(screen.getByRole("button", { name: "补充所需凭据" }));
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
});
