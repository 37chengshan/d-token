import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const invokeNative = vi.fn();
const refresh = vi.fn().mockResolvedValue(undefined);

vi.mock("../../lib/native", () => ({
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
  invokeNative: (...args: unknown[]) => invokeNative(...args),
  isTauriRuntime: () => true,
}));

vi.mock("../../lib/useRuntimeDashboard", () => {
  const buildState = () => ({
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
  });
  return {
    useRuntimeDashboard: () => buildState(),
    useRuntimeDashboardSelector: (selector: (value: unknown) => unknown) => selector(buildState()),
    shallowEqualSlice: () => false,
  };
});

import { Install } from "./Install";
import { clearInstallHandoff, getInstallMachineState, dispatchInstallMachine, requestInstallHandoff, resetInstallMachineState } from "../../lib/installMachine";

afterEach(() => {
  cleanup();
  resetInstallMachineState();
  invokeNative.mockReset();
  refresh.mockClear();
});

describe("Install OpenCode guarded transaction", () => {
  it.each(["manual", "handoff", "cancelled", "stale", "expired", "preflight-failed", "mixed-preflight", "mixed-apply", "recovery-write-failed", "recovery-backup-failed", "recovery-service-failed", "recovery-drift", "single-apply-failed"])("binds %s configuration to the exact selected batch and reviewed hashes", async (entry) => {
    const recoveryCase = entry.startsWith("recovery-");
    let serviceStarts = 0;
    invokeNative.mockImplementation(async (command: string, args?: { id?: string; request?: { agent_id?: string } }) => {
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
        const candidate = { id: "deepseek", name: "DeepSeek", protocol: "open_ai", base_url: "https://example.test/v1", models: ["deepseek-chat"], source: "opencode-normal-config", freshness: "unix:1", auth_mode: "none", active: true, state: "discovered", agent_ids: ["opencode"], confidence: 98, reasons: ["current"] };
        const result = {
          source: "dtoken-desktop-read-only-discovery",
          freshness: "unix:1",
          revision: "setup-1",
          state: "loaded",
          candidates: [candidate],
          agent_bindings: [{
            agent_id: "opencode",
            agent_name: "OpenCode",
            installed: true,
            binary_path: "/usr/local/bin/opencode",
            config_path: "~/.config/opencode/opencode.json",
            current_config_source: "opencode-normal-config",
            candidates: [candidate],
            resolved_candidate: candidate,
            resolution: "auto-resolved",
            confidence: 98,
            reasons: ["current"],
            capability: "one-click",
            can_configure: true,
            can_clear: false,
            transport_supported: true,
            transport_status: "configure_only",
            already_connected: false,
          }, {
            agent_id: "cline",
            agent_name: "Cline",
            installed: false,
            current_config_source: "unknown",
            candidates: [],
            resolved_candidate: null,
            resolution: "unavailable",
            confidence: 0,
            reasons: ["not detected"],
            capability: "prompt-only",
            already_connected: false,
          }],
          sources: [],
          cc_switch_sync: null,
          selection_required: false,
          writes_performed: false,
          privacy: "redacted",
        };
        if (["mixed-preflight", "mixed-apply"].includes(entry)) {
          const ready = result.agent_bindings[0];
          if (!ready.binary_path) throw new Error("Ready binding fixture is missing its binary");
          result.agent_bindings.push({ ...ready, agent_id: "codex", agent_name: "Codex", config_path: "~/.codex/config.toml" });
        }
        return result;
      }
      if (command === "preview_managed_agent_transport") {
        if (entry === "preflight-failed" || (entry === "mixed-preflight" && args?.request?.agent_id === "opencode")) return { ok: false, summary: "Existing provider cannot be modified" };
        return { ok: true, review: { revision: "revision-1", before_sha256: "before-hash", after_sha256: "after-hash", changed: true } };
      }
      if (command === "preview_connection_route_plan") {
        return {
          ok: true,
          state: "review_required",
          source: "dtoken-desktop-auto-route-plan",
          freshness: "fresh",
          target: "~/.dtoken/active-config/dtoken-config.json",
          discovery_revision: "setup-1",
          before_sha256: "route-before-hash",
          after_sha256: "route-after-hash",
          diff: "+ automatic OpenCode route",
          agents: ["opencode"],
          summary: "Review the automatic route plan",
        };
      }
      if (command === "preview_hybrid_agent_integration") {
        return {
          ok: true,
          state: "review",
          plan: {
            review_digest: "agent-review-1",
            targets: [{
              target_path: "~/.config/opencode/plugins/dtoken-hybrid-context.js",
              before_sha256: "agent-before",
              after_sha256: "agent-after",
              diff: "+ d-token plugin",
            }],
          },
        };
      }
      if (command === "stop_dtoken_service" || command === "run_install_transaction") {
        if (command === "run_install_transaction") {
          serviceStarts += 1;
          if (entry === "recovery-service-failed" && serviceStarts > 1) return { ok: false, summary: "restart failed", logs: [], tools: [] };
        }
        return { ok: true, summary: "service lifecycle complete", logs: [], tools: [] };
      }
      if (command === "get_active_route_config_status") return { source: "d-token-desktop", state: "loaded", content_sha256: entry === "recovery-drift" ? "external-change" : "route-before-hash" };
      if (command === "manage_backup") return { ok: entry !== "recovery-backup-failed", summary: "restore result" };
      if (command === "rollback_connection_route_plan") return { ok: true, summary: "restored" };
      if (command === "apply_connection_route_plan") {
        if (entry === "recovery-write-failed") throw new Error("rollback required after write refusal");
        return {
          ok: true,
          state: "applied_verified",
          verified: true,
          summary: "automatic route applied",
          rollback_id: "route-rollback-1",
          before_sha256: "route-before-hash",
          after_sha256: "route-after-hash",
          auto_follow: {
            enabled: false,
            state: "pending_runtime_verification",
          },
        };
      }
      if (command === "apply_reviewed_managed_agent_transport") {
        if (["mixed-apply", "single-apply-failed"].includes(entry) && args?.request?.agent_id === "opencode") return { ok: false, state: "failed", summary: "Configuration changed after review" };
        return { changed: true, transaction: { changes: [{ backup: { rollback_id: "config-backup" } }, { backup: { rollback_id: "selection-backup" } }] }, verification: { ok: true, detail: "OpenCode default connection verified" } };
      }
      if (command === "apply_hybrid_agent_integration") {
        return {
          ok: true,
          state: "applied",
          summary: "Agent integration applied",
          receipt: { transaction_id: "agent-transaction-1" },
        };
      }
      if (command === "verify_hybrid_agent_integration") {
        return { ok: true, state: "verified", summary: "Agent integration verified" };
      }
      if (command === "rollback_hybrid_agent_integration") {
        return { ok: true, state: "rolled_back", summary: "Agent integration rolled back" };
      }
      if (command === "run_product_readiness_check") return { ok: true, checks: [], privacy: "redacted" };
      if (command === "verify_agent_route_config") return { ok: !recoveryCase, passed: 1, total: 1, summary: "verified", checks: [], service_url: "http://127.0.0.1:18001" };
      if (command === "finalize_agent_auto_follow_authorization") {
        return { ok: true, state: "authorized" };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    if (!["manual", "preflight-failed", "mixed-preflight", "mixed-apply"].includes(entry)) {
      const snapshot = await invokeNative("discover_upstream_candidates");
      dispatchInstallMachine({ type: "DISCOVERED", snapshot });
      requestInstallHandoff({ agentIds: ["opencode"], discoveryRevision: entry === "stale" ? "old-revision" : snapshot.revision,
        source: "onboarding-start-setup", requestedAtMs: entry === "expired" ? Date.now() - 61_000 : Date.now() });
      if (entry === "cancelled") clearInstallHandoff("navigation");
    }
    const mounted = render(<Install />);
    expect(await screen.findByText("OpenCode")).toBeInTheDocument();
    expect(screen.queryByText("Cline")).not.toBeInTheDocument();
    if (["cancelled", "stale", "expired"].includes(entry)) {
      if (entry !== "cancelled") await screen.findByText("连接信息已变化，请检查所选工具后重新点击配置。");
      expect(invokeNative.mock.calls.some(([command]) => String(command).startsWith("apply_"))).toBe(false);
      mounted.unmount();
      render(<Install />);
      await screen.findByRole("button", { name: "配置所选 (1)" });
      expect(invokeNative.mock.calls.some(([command]) => String(command).startsWith("apply_"))).toBe(false);
      return;
    }
    if (["manual", "preflight-failed", "mixed-preflight", "mixed-apply"].includes(entry)) fireEvent.click(await screen.findByRole("button", { name: ["mixed-preflight", "mixed-apply"].includes(entry) ? "配置所选 (2)" : "配置所选 (1)" }));
    expect(screen.queryByText("确认连接方案")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "确认并连接" })).not.toBeInTheDocument();

    if (entry === "single-apply-failed") {
      await screen.findByText("没有工具完成配置，请查看详情后重试。");
      expect(getInstallMachineState().phase).toBe("blocked");
      expect(invokeNative.mock.calls.some(([command]) => command === "apply_hybrid_agent_integration")).toBe(false);
      return;
    }
    if (recoveryCase) {
      await waitFor(() => expect(getInstallMachineState().phase).toBe(entry === "recovery-write-failed" ? "rolled-back" : "blocked"));
      expect(serviceStarts).toBe(["recovery-write-failed", "recovery-drift"].includes(entry) ? 1 : 2);
      if (entry !== "recovery-write-failed") {
        const restored = invokeNative.mock.calls.filter(([command]) => command === "manage_backup").map(([, args]) => args.id);
        expect(restored).toEqual(entry === "recovery-backup-failed" ? ["selection-backup"] : ["selection-backup", "config-backup"]);
        await screen.findByText("操作未完成，部分配置或服务尚未恢复。请运行诊断或查看变更与回退。");
      }
      return;
    }
    if (["mixed-preflight", "mixed-apply"].includes(entry)) {
      await waitFor(() => expect(invokeNative.mock.calls.some(([command, args]) => command === "apply_reviewed_managed_agent_transport" && args.request.agent_id === "codex")).toBe(true));
      expect(invokeNative.mock.calls.some(([command, args]) => command === "apply_reviewed_managed_agent_transport" && args.request.agent_id === "opencode")).toBe(entry === "mixed-apply");
      const routes = invokeNative.mock.calls.filter(([command]) => command === "apply_connection_route_plan");
      expect(routes).toHaveLength(1);
      expect(routes[0][1].request.plan.bindings.map((binding: { agent_id: string }) => binding.agent_id)).toEqual(entry === "mixed-apply" ? ["opencode", "codex"] : ["codex"]);
      await screen.findByText("部分工具未完成配置，请查看对应工具的详情后重试。");
      expect(invokeNative.mock.calls.some(([command]) => ["manage_backup", "rollback_connection_route_plan", "rollback_hybrid_agent_integration"].includes(String(command)))).toBe(false);
      return;
    }
    if (entry === "preflight-failed") {
      await screen.findByText("OpenCode 的配置预检未通过，请检查工具设置后重试。");
      expect(invokeNative.mock.calls.some(([command]) => String(command).startsWith("apply_") || ["stop_dtoken_service", "run_install_transaction"].includes(String(command)))).toBe(false);
      return;
    }
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("apply_connection_route_plan", {
      request: {
        plan: {
          discovery_revision: "setup-1",
          bindings: [{
            agent_id: "opencode",
            candidate_id: "deepseek",
            candidate_source: "opencode-normal-config",
            model: "deepseek-chat",
          }],
        },
        reviewed_before_sha256: "route-before-hash",
        reviewed_after_sha256: "route-after-hash",
        enable_auto_follow: true,
      },
    }));
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("finalize_agent_auto_follow_authorization", {
      request: {
        expected_route_sha256: "route-after-hash",
      },
    }));
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("apply_reviewed_managed_agent_transport", {
      request: { agent_id: "opencode", target_path: "~/.config/opencode/opencode.json", endpoint: "http://127.0.0.1:18001", enabled: true, model: "deepseek-chat" },
      operation: "configure",
      reviewedRevision: "revision-1", reviewedBeforeSha256: "before-hash", reviewedAfterSha256: "after-hash",
    }));
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("apply_hybrid_agent_integration", {
      request: {
        draft: {
          agent_ids: ["opencode"],
          scope: "global",
          project_root: null,
          install_instructions: true,
          install_hook_or_plugin: true,
          install_skill: true,
          install_mcp: false,
        },
        reviewed_digest: "agent-review-1",
      },
    }));
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("verify_hybrid_agent_integration", {
      request: { transaction_id: "agent-transaction-1" },
    }));
    expect(await screen.findByText(/已确认配置生效、本地服务健康且路由可用/)).toBeInTheDocument();
    if (entry === "handoff") {
      mounted.unmount();
      render(<Install />);
      await screen.findByText("OpenCode");
      expect(invokeNative.mock.calls.filter(([command]) => command === "apply_reviewed_managed_agent_transport")).toHaveLength(1);
    }
  });
});
