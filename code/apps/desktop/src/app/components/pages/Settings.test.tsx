import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
}));

vi.mock("../../lib/theme-context", async (importOriginal) => {
  const original = await importOriginal<typeof import("../../lib/theme-context")>();
  return {
    ...original,
    useTheme: () => ({ theme: "light", setTheme: vi.fn() }),
    useAppearance: () => ({
      theme: "light",
      resolvedTheme: "light",
      setTheme: vi.fn(),
      accent: "orange",
      setAccent: vi.fn(),
      chartAccent: "orange",
      setChartAccent: vi.fn(),
      loadState: "preview",
      saving: false,
      persistenceNotice: "",
      updateAppearance: vi.fn(),
    }),
  };
});

import { Settings } from "./Settings";

beforeEach(() => {
  native.invokeNative.mockReset();
});

afterEach(cleanup);

describe("Settings product hierarchy and honest behavior", () => {
  it("orders settings around user tasks instead of engineering categories", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    const { container } = render(<Settings />);
    await screen.findByText("服务未启动");

    const sectionOrder = Array.from(container.querySelectorAll("[data-settings-section]"))
      .map((node) => node.getAttribute("data-settings-section"));
    expect(sectionOrder).toEqual(["runtime", "privacy", "learning", "updates", "appearance", "about"]);
    expect(container.querySelectorAll(".settings-group")).toHaveLength(6);
    expect(native.invokeNative.mock.calls.some(([command]) => command === "get_app_settings")).toBe(true);
  });

  it("uses the native autostart status while local preferences remain editable", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      if (command === "get_autostart_status") {
        return { supported: true, state: "disabled", enabled: false, source: "macos-launch-agent", service_argument: "--dtoken-autostart-service", summary: "disabled" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByText("服务未启动");

    expect(screen.getByRole("switch", { name: "自动启动 d-token 服务" })).toBeEnabled();
    expect(screen.getByText("已关闭")).toBeVisible();
    expect(screen.getByRole("switch", { name: "智能学习全面扫描" })).toBeEnabled();
    expect(screen.getByRole("switch", { name: "自动检查更新" })).toBeEnabled();
  });

  it("confirms and applies autostart through the native transaction", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      if (command === "get_autostart_status") return { supported: true, state: "disabled", enabled: false, source: "macos-launch-agent", service_argument: "--dtoken-autostart-service", summary: "disabled" };
      if (command === "enable_autostart") return {
        ok: true,
        summary: "enabled",
        rollback_performed: false,
        status: { supported: true, state: "enabled", enabled: true, source: "macos-launch-agent", service_argument: "--dtoken-autostart-service", summary: "enabled" },
      };
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    const autostart = await screen.findByRole("switch", { name: "自动启动 d-token 服务" });
    await waitFor(() => expect(autostart).toBeEnabled());
    await user.click(autostart);
    expect(await screen.findByRole("alertdialog")).toHaveTextContent("启用登录后自动启动？");
    expect(native.invokeNative).not.toHaveBeenCalledWith("enable_autostart");
    await user.click(screen.getByRole("button", { name: "确认启用" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("enable_autostart"));
    await waitFor(() => expect(screen.getByRole("switch", { name: "自动启动 d-token 服务" })).toBeChecked());
  });

  it("does not expose removed flowing-background controls", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByText("外观");
    expect(screen.queryByText("背景颜色")).not.toBeInTheDocument();
    expect(screen.queryByText("背景移动速度")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "日出" })).not.toBeInTheDocument();
  });

  it("does not call save in browser preview", async () => {
    native.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    render(<Settings />);

    expect((await screen.findAllByText("浏览器预览")).length).toBeGreaterThanOrEqual(1);
    await new Promise((resolve) => window.setTimeout(resolve, 420));

    expect(native.invokeNative.mock.calls.some(([command]) => command === "save_app_settings")).toBe(false);
  });

  it("uses the real service lifecycle command for the primary runtime action", async () => {
    const user = userEvent.setup();
    let running = false;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return {
          state: running ? "running" : "stopped",
          managed: running,
          service_url: "http://127.0.0.1:18001",
          summary: running ? "running" : "stopped",
        };
      }
      if (command === "start_dtoken_service") {
        running = true;
        return { ok: true, summary: "started", logs: [], tools: [] };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await user.click(await screen.findByRole("button", { name: "启动服务" }));

    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("start_dtoken_service"));
    expect(screen.getByRole("status")).toHaveTextContent("d-token 本地服务已启动。");
    expect(screen.queryByText("started")).not.toBeInTheDocument();
  });
});
