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
  doctorFailureDetail,
  doctorFailureSummary,
  doctorSourceFromPayload,
  hasCompressionEvidence,
  localizeReadinessCheck,
  localizeVerificationCheck,
  readinessCountEvidence,
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
    expect(doctorFailureSummary(new Error("boom"))).toBe("诊断失败，请稍后重试或查看连接日志。");
    expect(doctorFailureSummary(new Error("native_unavailable"), "en")).toBe(
      "Run the full diagnosis in the d-token desktop window; the browser is preview-only.",
    );
    expect(doctorFailureSummary(new Error("boom"), "en")).toBe("Diagnosis failed. Try again or review connection logs.");
    expect(DOCTOR_READINESS_TITLE).toBe("产品就绪检查");
    expect(doctorFailureDetail(new Error("readiness unavailable"), "zh")).toBe("");
    expect(doctorFailureDetail(new Error("readiness unavailable"), "en")).toBe("readiness unavailable");
    expect(doctorFailureDetail(new Error("Authorization: Bearer secret"), "en")).toBe("");
    expect(doctorFailureDetail(new Error("读取诊断失败"), "zh")).toBe("读取诊断失败");
  });

  it("localizes native check copy by stable id without exposing raw backend prose", () => {
    expect(localizeReadinessCheck("install_connect", true, "zh")).toEqual({
      label: "安装与连接计划",
      summary: "安装与连接计划已验证。",
    });
    expect(localizeVerificationCheck("runtime_health", false, "zh")).toEqual({
      label: "本地服务健康状态",
      summary: "本地服务健康状态需要处理。",
    });
    expect(localizeReadinessCheck("future_backend_check", false, "en")).toEqual({
      label: "Product readiness check",
      summary: "Product readiness check has not passed.",
    });
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

  it("does not gate Doctor on optional Headroom or RTK references", () => {
    const state = {
      agentOk: true,
      agentSummary: "OpenCode detected",
      runtimeOk: true,
      runtimeSummary: "runtime healthy",
      activeConfigOk: true,
      activeConfigSummary: "config loaded",
      verificationOk: true,
      verificationSummary: "verified",
      verificationResult: null,
    };
    const result = {
      ok: true,
      summary: "Doctor check complete",
      logs: [],
      tools: [
        { id: "rtk", label: "RTK", installed: false, status: "missing", detail: "missing" },
        { id: "headroom", label: "Headroom", installed: false, status: "missing", detail: "missing" },
        { id: "opencode", label: "OpenCode", installed: true, status: "installed", detail: "detected" },
      ],
    };

    const checks = buildDoctorChecks(result, "native", state);
    expect(checks.find((check) => check.id === "tool-headroom")).toMatchObject({
      status: "healthy",
      repair: undefined,
    });
    expect(checks.find((check) => check.id === "tool-rtk")).toMatchObject({
      status: "healthy",
      repair: undefined,
    });
    expect(checks.find((check) => check.id === "tool-opencode")?.status).toBe("healthy");

    result.tools[2].installed = false;
    const missingAgentChecks = buildDoctorChecks(result, "native", state);
    expect(missingAgentChecks.find((check) => check.id === "tool-opencode")).toMatchObject({
      status: "error",
      repair: "advanced",
    });
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
    expect(evidence.summary).toContain("来源=desktop_active_config");
    expect(evidence.summary).toContain("路由=1");

    const englishEvidence = resolveActiveConfigEvidence({
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
    }, null, "en");
    expect(englishEvidence.summary).toContain("source=desktop_active_config");
    expect(englishEvidence.summary).toContain("routes=1");
  });

  it("rejects inconsistent readiness counts as incomplete evidence", () => {
    expect(readinessCountEvidence({ passed: 1, total: 1, checks: [{ id: "one" }] as never })).toEqual({
      passed: 1,
      total: 1,
      complete: true,
    });
    expect(readinessCountEvidence({ passed: 0, total: 0, checks: [] })).toEqual({
      passed: 0,
      total: 0,
      complete: false,
    });
    expect(readinessCountEvidence({ passed: 2, total: 1, checks: [{ id: "one" }] as never }).complete).toBe(false);
    expect(readinessCountEvidence({ passed: 1, total: 2, checks: [{ id: "one" }] as never }).complete).toBe(false);
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

    const missingCounts = resolveActiveConfigEvidence(null, {
      source: "d-token-desktop",
      state: "loaded",
      fallback_disabled: true,
    });
    expect(missingCounts.ok).toBe(false);
    expect(missingCounts.summary).toContain("路由=—");
    expect(missingCounts.summary).toContain("上游=—");
  });
});

describe("Doctor polling", () => {
  it("announces loading and failure instead of leaving the action without a result", async () => {
    native.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    render(createElement(Doctor, { setPage: vi.fn() }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("get_desktop_data");
    });

    let rejectReadiness: ((reason: Error) => void) | undefined;
    native.invokeNative.mockImplementation((command: string) => {
      if (command === "run_product_readiness_check") {
        return new Promise((_resolve, reject) => {
          rejectReadiness = reject;
        });
      }
      return Promise.reject(new Error("native_unavailable"));
    });

    await waitFor(() => expect(screen.getByRole("button", { name: "运行全部检查" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "运行全部检查" }));
    const runningStatus = screen.getByRole("status");
    expect(runningStatus).toHaveAttribute("aria-live", "polite");
    expect(runningStatus).toHaveTextContent("正在检查本地服务、Agent、路由与安全边界");
    expect(screen.getByRole("button", { name: "运行全部检查" })).toBeDisabled();

    await act(async () => {
      rejectReadiness?.(new Error("readiness unavailable"));
    });
    const failureAlert = await screen.findByRole("alert");
    expect(failureAlert).toHaveAttribute("aria-live", "assertive");
    expect(failureAlert).toHaveTextContent("诊断失败，请稍后重试或查看连接日志。");
    expect(failureAlert).not.toHaveTextContent("readiness unavailable");
    expect(screen.queryByText("技术详情")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "运行全部检查" })).toBeEnabled();
  });

  it("preserves a manual Doctor result after the app-wide snapshot refreshes", async () => {
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
      if (command === "get_desktop_data") return dashboardPayload;
      if (command === "get_dtoken_service_status") {
        return { state: "running", managed: true, service_url: "http://127.0.0.1:18001" };
      }
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
          checks: [{ id: "local-runtime", label: "本地服务", ok: true, summary: "已验证" }],
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
    expect(await screen.findByRole("status")).toHaveTextContent("检查完成：1/1 项通过。");
    expect(screen.queryByText("本地能力已验证")).not.toBeInTheDocument();

    await waitFor(() => expect(screen.getAllByText(/OpenCode 已检测到：1\.0\.0/).length).toBeGreaterThan(0));
    expect(screen.queryByText("尚未运行完整诊断，暂不判断 Agent 连接状态。")).not.toBeInTheDocument();
  });
});
