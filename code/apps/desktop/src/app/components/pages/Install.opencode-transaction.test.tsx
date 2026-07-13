import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const invokeNative = vi.fn();
const refresh = vi.fn().mockResolvedValue(undefined);

vi.mock("../../lib/native", () => ({
  invokeNative: (...args: unknown[]) => invokeNative(...args),
  isTauriRuntime: () => true,
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => ({
    payload: {
      desktop_data_source: "dtoken-local-service",
      runtime_active_config: {
        config: {
          providers: [{ id: "deepseek", models: ["deepseek-chat"] }],
          routes: [{ id: "opencode-route", name: "OpenCode route" }],
        },
      },
      request_logs: [],
    },
    status: "runtime",
    updatedAt: new Date("2026-07-12T10:00:00Z"),
    error: "",
    refresh,
    operational: {
      service: { data: { ok: true, state: "running", managed: true, service_url: "http://127.0.0.1:18001", summary: "running" } },
    },
  }),
}));

import { Install } from "./Install";

afterEach(() => {
  cleanup();
  invokeNative.mockReset();
  refresh.mockClear();
});

describe("Install OpenCode guarded transaction", () => {
  it("binds the apply action to the reviewed revision and both hashes", async () => {
    invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_dtoken_service_status") {
        return { ok: true, state: "running", managed: true, service_url: "http://127.0.0.1:18001", summary: "running" };
      }
      if (command === "detect_tools") {
        return {
          ok: true,
          summary: "detected",
          logs: [],
          tools: [{ id: "opencode", label: "OpenCode", installed: true, version: "1.14.51", path: "/usr/local/bin/opencode", status: "installed", detail: "installed" }],
        };
      }
      if (command === "discover_upstream_candidates") {
        return {
          source: "dtoken-desktop-read-only-discovery",
          freshness: "unix:1",
          state: "loaded",
          candidates: [{ id: "deepseek", name: "DeepSeek", protocol: "open_ai", base_url: "https://example.test/v1", models: ["deepseek-chat"], source: "opencode-normal-config", freshness: "unix:1", auth_mode: "os-ref", active: true, state: "discovered" }],
          selection_required: true,
          writes_performed: false,
          privacy: "redacted",
        };
      }
      if (command === "preview_opencode_connection") {
        return {
          ok: true,
          state: "review_required",
          source: "dtoken-desktop-opencode-adapter",
          freshness: "fresh",
          target: "~/.config/opencode/opencode.json",
          model: "deepseek-chat",
          listener: "127.0.0.1:18001",
          transaction: "scan-plan-diff-backup-confirm-apply-verify-doctor-rollback",
          revision: "revision-1",
          before_sha256: "before-hash",
          after_sha256: "after-hash",
          diff: "+ provider.dtoken-local-route",
          summary: "Review the exact OpenCode provider change before applying it",
          privacy: "auth.json is never read",
        };
      }
      if (command === "apply_opencode_connection") {
        return {
          ok: true,
          state: "applied_verified",
          source: "dtoken-desktop-opencode-adapter",
          freshness: "fresh",
          target: "~/.config/opencode/opencode.json",
          model: "deepseek-chat",
          listener: "127.0.0.1:18001",
          revision: "revision-1",
          before_sha256: "before-hash",
          after_sha256: "after-hash",
          rollback_id: "rollback-1",
          verified: true,
          summary: "OpenCode provider applied and verified",
          privacy: "auth.json is never read",
        };
      }
      if (command === "run_product_readiness_check") return { ok: true, checks: [], privacy: "redacted" };
      if (command === "verify_agent_route_config") return { ok: true, passed: 1, total: 1, summary: "verified", checks: [], service_url: "http://127.0.0.1:18001" };
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);
    fireEvent.click(await screen.findByRole("button", { name: "检测工具" }));
    await screen.findByText("v1.14.51");
    fireEvent.change((await screen.findAllByRole("combobox"))[0], { target: { value: "opencode-normal-config::deepseek::deepseek-chat" } });
    fireEvent.click(screen.getByRole("button", { name: "连接 OpenCode" }));
    fireEvent.click(await screen.findByRole("button", { name: "生成真实配置预览" }));
    expect(await screen.findByText(/before before-hash/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认并应用" }));

    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("apply_opencode_connection", {
      request: {
        model: "deepseek-chat",
        listener_port: 18001,
        reviewed_revision: "revision-1",
        reviewed_before_sha256: "before-hash",
        reviewed_after_sha256: "after-hash",
      },
    }));
    expect((await screen.findAllByText(/OpenCode 连接配置已按审阅内容写入并验证/)).length).toBeGreaterThan(0);
  });
});
