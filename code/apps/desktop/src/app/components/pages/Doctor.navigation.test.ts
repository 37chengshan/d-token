import "@testing-library/jest-dom/vitest";
import { createElement } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
}));

import {
  buildDoctorChecks,
  DOCTOR_READINESS_TITLE,
  Doctor,
  doctorFailureSummary,
  doctorSourceFromPayload,
  hasCompressionEvidence,
  resolveActiveConfigEvidence,
  resolveDoctorDestination,
} from "./Doctor";

beforeEach(() => {
  native.invokeNative.mockReset();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("resolveDoctorDestination", () => {
  it("maps legacy Doctor targets to pages that the current shell renders", () => {
    expect(resolveDoctorDestination("advanced")).toBe("install");
    expect(resolveDoctorDestination("logs")).toBe("changes");
    expect(resolveDoctorDestination("changes")).toBe("changes");
  });

  it("uses readable Chinese copy for diagnostic failures and readiness", () => {
    expect(doctorFailureSummary(new Error("native_unavailable"))).toBe(
      "当前是浏览器预览，请在 d-token 桌面窗口中运行完整诊断。",
    );
    expect(doctorFailureSummary(new Error("boom"))).toBe("诊断失败：boom");
    expect(DOCTOR_READINESS_TITLE).toBe("产品就绪检查");
  });

  it("keeps preview, stale and partial Doctor sources free of healthy results", () => {
    const state = {
      agentOk: true,
      agentSummary: "Agent detected",
      runtimeOk: true,
      runtimeSummary: "runtime healthy",
      activeConfigOk: true,
      activeConfigSummary: "config loaded",
      verificationOk: true,
      verificationSummary: "verified",
      verificationResult: null,
    };

    for (const source of ["preview", "stale", "partial"] as const) {
      expect(buildDoctorChecks(null, source, state).every((check) => check.status !== "healthy")).toBe(true);
    }
  });

  it("requires complete d-token runtime identity before using native Doctor status", () => {
    const base = { source: "dtoken-owned-runtime", desktop_data_source: "dtoken-local-service" };

    expect(doctorSourceFromPayload(base)).toBe("native");
    expect(doctorSourceFromPayload({ ...base, data_status: "stale" })).toBe("stale");
    expect(doctorSourceFromPayload({ ...base, partial: true })).toBe("partial");
    expect(doctorSourceFromPayload({ source: "dtoken-owned-runtime" })).toBe("partial");
  });

  it("requires compression-specific request evidence", () => {
    expect(hasCompressionEvidence({ request_logs: [{ elapsed_us: 120 }] })).toBe(false);
    expect(hasCompressionEvidence({ request_logs: [{ compression_elapsed_us: 120 }] })).toBe(true);
    expect(hasCompressionEvidence({
      request_logs: [{
        input_tokens_before: 120,
        input_tokens_after: 90,
        transforms_applied: ["drop_redundant_context"],
      }],
    })).toBe(true);
    expect(hasCompressionEvidence({
      request_logs: [{
        input_tokens_before: 120,
        input_tokens_after: 120,
        transforms_applied: ["compression_error_fallback"],
      }],
    })).toBe(false);
  });

  it("accepts the owned runtime loaded/source contract for active configuration", () => {
    const evidence = resolveActiveConfigEvidence({
      active_config: {
        source: "dtoken-owned-runtime",
        config: {
          source: "desktop_active_config",
          loaded: true,
          route_count: 1,
          provider_count: 1,
          fallback_disabled: true,
        },
      },
    }, null);

    expect(evidence.ok).toBe(true);
    expect(evidence.summary).toContain("source=desktop_active_config");
    expect(evidence.summary).toContain("routes=1");
  });

  it("fails closed for incomplete or foreign active configuration evidence", () => {
    expect(resolveActiveConfigEvidence({
      active_config: {
        source: "external-runtime",
        config: {
          source: "desktop_active_config",
          loaded: true,
          route_count: 1,
          provider_count: 1,
          fallback_disabled: true,
        },
      },
    }, null).ok).toBe(false);
    expect(resolveActiveConfigEvidence(null, {
      source: "d-token-desktop",
      state: "loaded",
      route_count: 0,
      provider_count: 1,
      fallback_disabled: true,
    }).ok).toBe(false);
  });
});

describe("Doctor polling", () => {
  it("preserves a manual Doctor result when the next status poll completes", async () => {
    let poll: TimerHandler | undefined;
    vi.spyOn(window, "setInterval").mockImplementation((handler: TimerHandler) => {
      poll = handler;
      return 1;
    });
    vi.spyOn(window, "clearInterval").mockImplementation(() => {});

    const dashboardPayload = {
      source: "dtoken-owned-runtime",
      desktop_data_source: "dtoken-local-service",
      stats: { requests: { total: 1 } },
      route_decisions: [],
      request_logs: [],
      active_config: {
        source: "dtoken-owned-runtime",
        config: {
          source: "desktop_active_config",
          loaded: true,
          route_count: 1,
          provider_count: 1,
          fallback_disabled: true,
        },
      },
    };
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_dtoken_dashboard_payload") return dashboardPayload;
      if (command === "get_active_route_config_status") {
        return {
          source: "d-token-desktop",
          state: "loaded",
          route_count: 1,
          provider_count: 1,
          fallback_disabled: true,
        };
      }
      if (command === "verify_agent_route_config") {
        return { ok: true, passed: 1, total: 1, summary: "连接配置已验证", checks: [] };
      }
      if (command === "run_product_readiness_check") {
        return {
          ok: true,
          mode: "product-readiness-check",
          passed: 1,
          total: 1,
          summary: "本地能力已验证",
          privacy: "仅使用本地元数据。",
          external_services: "none",
          checks: [],
        };
      }
      if (command === "run_doctor") {
        return {
          ok: true,
          summary: "诊断完成",
          logs: [],
          tools: [{
            id: "opencode",
            label: "OpenCode",
            installed: true,
            version: "1.0.0",
            status: "available",
            detail: "已安装",
          }],
        };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(createElement(Doctor, { setPage: vi.fn() }));
    await screen.findByText("尚未运行完整诊断，暂不判断 Agent 连接状态。");

    fireEvent.click(screen.getByRole("button", { name: "运行全部检查" }));
    await screen.findAllByText(/OpenCode 已检测到：1\.0\.0/);

    expect(typeof poll).toBe("function");
    await act(async () => {
      if (typeof poll === "function") poll();
    });
    await waitFor(() => {
      expect(screen.getAllByText(/OpenCode 已检测到：1\.0\.0/).length).toBeGreaterThan(0);
    });
    expect(screen.queryByText("尚未运行完整诊断，暂不判断 Agent 连接状态。")).not.toBeInTheDocument();
  });
});
