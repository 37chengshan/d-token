import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
    expect(sectionOrder).toEqual(["appearance", "runtime", "privacy", "learning", "updates", "modelPricing", "about"]);
    expect(container.querySelectorAll(".settings-group")).toHaveLength(7);
    // Colors lead appearance; pure display-only rows are not duplicated.
    expect(screen.getByText(/主题色|Accent/i)).toBeVisible();
    expect(screen.queryByText("诊断数据上传")).not.toBeInTheDocument();
    expect(screen.queryByText("本地连接保护")).not.toBeInTheDocument();
    expect(screen.queryByText("扫描范围")).not.toBeInTheDocument();
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

  it("loads saved model pricing rows from the local settings contract", async () => {
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_app_settings" && args?.section === "model-pricing") {
        return {
          state: "loaded",
          payload: {
            prices: [
              { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
              { model: "claude-sonnet-4-5", currency: "CNY", inputPerMillion: 18, outputPerMillion: 90, source: "user", updatedAt: "unix:2" },
            ],
          },
        };
      }
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByDisplayValue("gpt-4o");
    expect(screen.getByDisplayValue("claude-sonnet-4-5")).toBeInTheDocument();
    expect(screen.getByDisplayValue("2.5")).toBeInTheDocument();
    expect(screen.getByDisplayValue("10")).toBeInTheDocument();
    expect(screen.getByDisplayValue("CNY")).toBeInTheDocument();
  });

  it("shows the price-unknown empty state before any pricing is saved", async () => {
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_app_settings" && args?.section === "model-pricing") {
        return { state: "missing", payload: {} };
      }
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByText(/还没有价格表/);
    expect(screen.queryByText(/价格表为空（已保存的空态）/)).not.toBeInTheDocument();
  });

  it("saves added pricing rows through the native settings command", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByText("模型价格");
    await user.click(screen.getByRole("button", { name: "添加模型" }));
    await user.type(screen.getByLabelText("第 1 行模型标识"), "gpt-4o-mini");
    await user.type(screen.getByLabelText("第 1 行输入价格"), "0.6");
    await user.type(screen.getByLabelText("第 1 行输出价格"), "2.5");
    await user.click(screen.getByRole("button", { name: "保存价格表" }));

    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith(
      "save_app_settings",
      expect.objectContaining({
        section: "model-pricing",
        payload: expect.objectContaining({
          prices: [expect.objectContaining({ model: "gpt-4o-mini", currency: "USD", inputPerMillion: 0.6, outputPerMillion: 2.5 })],
          unit: "per_1m_tokens",
        }),
      }),
    ));
  });

  it("rejects invalid pricing rows without saving", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByText("模型价格");
    await user.click(screen.getByRole("button", { name: "添加模型" }));
    // Empty model identifier.
    await user.click(screen.getByRole("button", { name: "保存价格表" }));
    expect(screen.getByText(/模型标识不能为空/)).toBeInTheDocument();
    expect(native.invokeNative.mock.calls.some(([command]) => command === "save_app_settings")).toBe(false);

    // Negative price. userEvent cannot type "-1" into a jsdom number input,
    // so set the value directly through the native change event.
    await user.type(screen.getByLabelText("第 1 行模型标识"), "gpt-4o");
    fireEvent.change(screen.getByLabelText("第 1 行输入价格"), { target: { value: "-1" } });
    await user.click(screen.getByRole("button", { name: "保存价格表" }));
    expect(screen.getByText(/必须是非负有限数字/)).toBeInTheDocument();
    expect(native.invokeNative.mock.calls.some(([command]) => command === "save_app_settings")).toBe(false);
  });

  it("removes a pricing row before saving", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_app_settings" && args?.section === "model-pricing") {
        return {
          state: "loaded",
          payload: {
            prices: [
              { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
              { model: "claude-sonnet-4-5", currency: "USD", inputPerMillion: 3, outputPerMillion: 15, source: "user", updatedAt: "unix:2" },
            ],
          },
        };
      }
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByDisplayValue("gpt-4o");
    await user.click(screen.getByRole("button", { name: "删除第 2 行" }));
    expect(screen.queryByDisplayValue("claude-sonnet-4-5")).not.toBeInTheDocument();
    expect(screen.getByDisplayValue("gpt-4o")).toBeInTheDocument();
  });

  it("restores the previous pricing table when persistence fails", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_app_settings" && args?.section === "model-pricing") {
        return {
          state: "loaded",
          payload: { prices: [{ model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" }] },
        };
      }
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      if (command === "save_app_settings") return { ok: false, summary: "Settings save failed", logs: [], tools: [] };
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByDisplayValue("gpt-4o");
    const modelInput = screen.getByLabelText("第 1 行模型标识");
    await user.clear(modelInput);
    await user.type(modelInput, "claude-sonnet-4-5");
    await user.click(screen.getByRole("button", { name: "保存价格表" }));

    await waitFor(() => expect(screen.getByDisplayValue("gpt-4o")).toBeInTheDocument());
    expect(screen.getByText(/保存失败；已恢复为保存前的价格表/)).toBeInTheDocument();
  });

  it("clears the pricing table through restore default", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_app_settings" && args?.section === "model-pricing") {
        return {
          state: "loaded",
          payload: { prices: [{ model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" }] },
        };
      }
      if (command === "get_app_settings") return { state: "loaded", payload: {} };
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByDisplayValue("gpt-4o");
    await user.click(screen.getByRole("button", { name: "恢复默认（清空）" }));

    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith(
      "save_app_settings",
      expect.objectContaining({ section: "model-pricing", payload: expect.objectContaining({ prices: [] }) }),
    ));
  });

  it("shows the pricing table as unavailable in browser preview without saving", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_app_settings") throw new Error("native_unavailable");
      if (command === "get_dtoken_service_status") {
        return { state: "stopped", managed: false, service_url: "http://127.0.0.1:18001", summary: "stopped" };
      }
      return { ok: true, summary: "saved", logs: [], tools: [] };
    });

    render(<Settings />);
    await screen.findByText("价格表暂不可用");
    expect(screen.getByText(/浏览器预览不能读取或保存模型价格表/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存价格表" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "添加模型" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "恢复默认（清空）" })).toBeDisabled();
    expect(native.invokeNative.mock.calls.some(([command]) => command === "save_app_settings")).toBe(false);
  });
});
