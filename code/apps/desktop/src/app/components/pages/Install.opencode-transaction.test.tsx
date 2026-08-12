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
        const candidate = { id: "deepseek", name: "DeepSeek", protocol: "open_ai", base_url: "https://example.test/v1", models: ["deepseek-chat"], source: "opencode-normal-config", freshness: "unix:1", auth_mode: "none", active: true, state: "discovered", agent_ids: ["opencode"], confidence: 98, reasons: ["current"] };
        return {
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
          has_resolution_gap: false,
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
        return { ok: true, summary: "service lifecycle complete", logs: [], tools: [] };
      }
      if (command === "apply_connection_route_plan") {
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
      if (command === "verify_agent_route_config") return { ok: true, passed: 1, total: 1, summary: "verified", checks: [], service_url: "http://127.0.0.1:18001" };
      if (command === "finalize_agent_auto_follow_authorization") {
        return { ok: true, state: "authorized" };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);
    expect(await screen.findByText("OpenCode")).toBeInTheDocument();
    expect(screen.queryByText("Cline")).not.toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "查看变更" }));
    expect(await screen.findByText(/before before-hash/)).toBeInTheDocument();
    expect(screen.getByText(/before route-befo/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认并连接" }));

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
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("apply_opencode_connection", {
      request: {
        model: "deepseek-chat",
        listener_port: 18001,
        reviewed_revision: "revision-1",
        reviewed_before_sha256: "before-hash",
        reviewed_after_sha256: "after-hash",
      },
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
  });
});
