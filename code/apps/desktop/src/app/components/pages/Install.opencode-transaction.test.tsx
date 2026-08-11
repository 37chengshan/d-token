import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

import { resetInstallMachineState } from "../../lib/installMachine";

import { Install } from "./Install";

beforeEach(() => {
  resetInstallMachineState();
});

afterEach(() => {
  cleanup();
  resetInstallMachineState();
  invokeNative.mockReset();
  refresh.mockClear();
});

describe("Install OpenCode guarded transaction", () => {
  it("applies one compound route + OpenCode transaction after the compound review and only then starts the runtime", async () => {
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
        const freeCandidate = { id: "opencode-free", name: "OpenCode free", protocol: "open_ai", base_url: "https://opencode.ai/zen/v1", models: ["deepseek-v4-flash-free"], source: "opencode-provider-catalog-cache", freshness: "unix:1", auth_mode: "none", active: false, state: "available", agent_ids: ["opencode"], confidence: 90, reasons: ["official free catalog"] };
        const goCandidate = { id: "opencode-go", name: "OpenCode Go", protocol: "open_ai", base_url: "https://opencode.go.example.test/v1", models: ["go-model"], source: "opencode-provider-catalog-cache", freshness: "unix:1", auth_mode: "unavailable", dtoken_credential_configured: true, active: false, state: "available", agent_ids: ["opencode"], confidence: 90, reasons: ["credential configured"] };
        return {
          source: "dtoken-desktop-read-only-discovery",
          freshness: "unix:1",
          revision: "setup-1",
          state: "loaded",
          candidates: [freeCandidate, goCandidate],
          agent_bindings: [{
            agent_id: "opencode",
            agent_name: "OpenCode",
            installed: true,
            binary_path: "/usr/local/bin/opencode",
            config_path: "~/.config/opencode/opencode.json",
            current_config_source: "opencode-normal-config",
            candidates: [freeCandidate, goCandidate],
            resolved_candidate: freeCandidate,
            resolution: "auto-resolved",
            confidence: 90,
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
      if (command === "preview_compound_route_opencode") {
        return {
          ok: true,
          state: "review_required",
          source: "dtoken-desktop-auto-route-plan",
          freshness: "fresh",
          compound_revision: "compound-revision-1",
          requires_explicit_confirmation: true,
          route: {
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
            summary: "Review the exact d-token route change inside the compound transaction",
          },
          opencode: {
            ok: true,
            state: "review_required",
            source: "dtoken-desktop-opencode-adapter",
            freshness: "fresh",
            target: "~/.config/opencode/opencode.json",
            models: ["deepseek-v4-flash-free", "go-model"],
            listener: "127.0.0.1:18001",
            transaction: "scan-plan-diff-backup-confirm-apply-verify-doctor-rollback",
            revision: "revision-1",
            before_sha256: "before-hash",
            after_sha256: "after-hash",
            diff: "+ provider.dtoken-local-route",
            summary: "Review the exact OpenCode provider change inside the compound transaction",
            privacy: "auth.json is never read",
          },
          summary: "Review the exact d-token route and OpenCode provider changes before applying them as one transaction",
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
      if (command === "apply_compound_route_opencode") {
        return {
          ok: true,
          state: "applied_verified",
          source: "dtoken-desktop-auto-route-plan",
          freshness: "fresh",
          compound_revision: "compound-revision-1",
          transaction_id: "compound-transaction-1",
          verified: true,
          agents: ["opencode"],
          route: {
            target: "~/.dtoken/active-config/dtoken-config.json",
            before_sha256: "route-before-hash",
            after_sha256: "route-after-hash",
            rollback_id: "route-rollback-1",
          },
          opencode: {
            target: "~/.config/opencode/opencode.json",
            revision: "revision-1",
            models: ["deepseek-v4-flash-free", "go-model"],
            listener: "127.0.0.1:18001",
            before_sha256: "before-hash",
            after_sha256: "after-hash",
            rollback_id: "rollback-1",
          },
          auto_follow: {
            enabled: false,
            state: "not_authorized_for_multiroute",
            agent_ids: [],
            skipped_agents: ["opencode"],
            skipped_candidates: 2,
            requires_runtime_verification: false,
          },
          summary: "Reviewed d-token route and OpenCode provider were applied as one verified transaction with per-file rollback points",
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
    // One compound preview covers the route and the OpenCode provider change;
    // the standalone preview commands must not be used for this batch.
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("preview_compound_route_opencode", {
      request: {
        plan: {
          discovery_revision: "setup-1",
          bindings: [{
            agent_id: "opencode",
            candidate_id: "opencode-free",
            candidate_source: "opencode-provider-catalog-cache",
            model: "deepseek-v4-flash-free",
          }, {
            agent_id: "opencode",
            candidate_id: "opencode-go",
            candidate_source: "opencode-provider-catalog-cache",
            model: "go-model",
          }],
        },
        opencode: { models: ["deepseek-v4-flash-free", "go-model"], listener_port: 18001 },
        enable_auto_follow: true,
      },
    }));
    expect(invokeNative).not.toHaveBeenCalledWith("preview_opencode_connection", expect.anything());
    expect(invokeNative).not.toHaveBeenCalledWith("preview_connection_route_plan", expect.anything());
    fireEvent.click(screen.getByRole("button", { name: "确认并连接" }));

    // Apply is one native compound transaction echoing the exact compound
    // revision and both hash pairs; the standalone apply commands must never
    // run for this batch.
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("apply_compound_route_opencode", {
      request: {
        plan: {
          discovery_revision: "setup-1",
          bindings: [{
            agent_id: "opencode",
            candidate_id: "opencode-free",
            candidate_source: "opencode-provider-catalog-cache",
            model: "deepseek-v4-flash-free",
          }, {
            agent_id: "opencode",
            candidate_id: "opencode-go",
            candidate_source: "opencode-provider-catalog-cache",
            model: "go-model",
          }],
        },
        opencode: { models: ["deepseek-v4-flash-free", "go-model"], listener_port: 18001 },
        reviewed_compound_revision: "compound-revision-1",
        reviewed_route_before_sha256: "route-before-hash",
        reviewed_route_after_sha256: "route-after-hash",
        reviewed_opencode_before_sha256: "before-hash",
        reviewed_opencode_after_sha256: "after-hash",
        enable_auto_follow: true,
      },
    }));
    // The runtime is started/reloaded only after the compound receipt is
    // verified: the compound apply must be ordered before run_install_transaction.
    const compoundApplyIndex = invokeNative.mock.calls.findIndex(([command]) => command === "apply_compound_route_opencode");
    const runtimeInstallIndex = invokeNative.mock.calls.findIndex(([command]) => command === "run_install_transaction");
    expect(compoundApplyIndex).toBeGreaterThanOrEqual(0);
    expect(runtimeInstallIndex).toBeGreaterThan(compoundApplyIndex);
    expect(invokeNative).not.toHaveBeenCalledWith("apply_connection_route_plan", expect.anything());
    expect(invokeNative).not.toHaveBeenCalledWith("apply_opencode_connection", expect.anything());

    // OpenCode multi-route follow is not authorized: the compound receipt
    // says so explicitly, so no follow authorization was staged and the
    // finalize command must never be called for this OpenCode-only receipt.
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
    expect(invokeNative).not.toHaveBeenCalledWith("finalize_agent_auto_follow_authorization", expect.anything());
    expect(await screen.findByText(/已确认配置生效、本地服务健康且路由可用/)).toBeInTheDocument();
  });

  it("never finalizes auto-follow when the OpenCode-only compound receipt says multi-route follow is not authorized", async () => {
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
        const freeCandidate = { id: "opencode-free", name: "OpenCode free", protocol: "open_ai", base_url: "https://opencode.ai/zen/v1", models: ["deepseek-v4-flash-free"], source: "opencode-provider-catalog-cache", freshness: "unix:1", auth_mode: "none", active: false, state: "available", agent_ids: ["opencode"], confidence: 90, reasons: ["official free catalog"] };
        const goCandidate = { id: "opencode-go", name: "OpenCode Go", protocol: "open_ai", base_url: "https://opencode.go.example.test/v1", models: ["go-model"], source: "opencode-provider-catalog-cache", freshness: "unix:1", auth_mode: "unavailable", dtoken_credential_configured: true, active: false, state: "available", agent_ids: ["opencode"], confidence: 90, reasons: ["credential configured"] };
        return {
          source: "dtoken-desktop-read-only-discovery",
          freshness: "unix:1",
          revision: "setup-1",
          state: "loaded",
          candidates: [freeCandidate, goCandidate],
          agent_bindings: [{
            agent_id: "opencode",
            agent_name: "OpenCode",
            installed: true,
            binary_path: "/usr/local/bin/opencode",
            config_path: "~/.config/opencode/opencode.json",
            current_config_source: "opencode-normal-config",
            candidates: [freeCandidate, goCandidate],
            resolved_candidate: freeCandidate,
            resolution: "auto-resolved",
            confidence: 90,
            reasons: ["current"],
            capability: "one-click",
            can_configure: true,
            can_clear: false,
            transport_supported: true,
            transport_status: "configure_only",
            already_connected: false,
          }],
          sources: [],
          cc_switch_sync: null,
          has_resolution_gap: false,
          writes_performed: false,
          privacy: "redacted",
        };
      }
      if (command === "preview_compound_route_opencode") {
        return {
          ok: true,
          state: "review_required",
          source: "dtoken-desktop-auto-route-plan",
          freshness: "fresh",
          compound_revision: "compound-revision-1",
          requires_explicit_confirmation: true,
          auto_follow: {
            enabled: false,
            state: "not_authorized_for_multiroute",
            agent_ids: [],
            skipped_agents: ["opencode"],
            skipped_candidates: 2,
            requires_runtime_verification: false,
          },
          route: {
            ok: true,
            state: "review_required",
            target: "~/.dtoken/active-config/dtoken-config.json",
            discovery_revision: "setup-1",
            before_sha256: "route-before-hash",
            after_sha256: "route-after-hash",
            diff: "+ automatic OpenCode route",
            agents: ["opencode"],
            summary: "Review the exact d-token route change inside the compound transaction",
          },
          opencode: {
            ok: true,
            state: "review_required",
            target: "~/.config/opencode/opencode.json",
            models: ["deepseek-v4-flash-free", "go-model"],
            listener: "127.0.0.1:18001",
            revision: "revision-1",
            before_sha256: "before-hash",
            after_sha256: "after-hash",
            diff: "+ provider.dtoken-local-route",
            summary: "Review the exact OpenCode provider change inside the compound transaction",
            privacy: "auth.json is never read",
          },
          summary: "Review the exact d-token route and OpenCode provider changes before applying them as one transaction",
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
      if (command === "apply_compound_route_opencode") {
        return {
          ok: true,
          state: "applied_verified",
          source: "dtoken-desktop-auto-route-plan",
          freshness: "fresh",
          compound_revision: "compound-revision-1",
          transaction_id: "compound-transaction-1",
          verified: true,
          agents: ["opencode"],
          route: {
            target: "~/.dtoken/active-config/dtoken-config.json",
            before_sha256: "route-before-hash",
            after_sha256: "route-after-hash",
            rollback_id: "route-rollback-1",
          },
          opencode: {
            target: "~/.config/opencode/opencode.json",
            revision: "revision-1",
            models: ["deepseek-v4-flash-free", "go-model"],
            listener: "127.0.0.1:18001",
            before_sha256: "before-hash",
            after_sha256: "after-hash",
            rollback_id: "rollback-1",
          },
          auto_follow: {
            enabled: false,
            state: "not_authorized_for_multiroute",
            agent_ids: [],
            skipped_agents: ["opencode"],
            skipped_candidates: 2,
            requires_runtime_verification: false,
          },
          summary: "Reviewed d-token route and OpenCode provider were applied as one verified transaction with per-file rollback points",
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
      if (command === "run_product_readiness_check") return { ok: true, checks: [], privacy: "redacted" };
      if (command === "verify_agent_route_config") return { ok: true, passed: 1, total: 1, summary: "verified", checks: [], service_url: "http://127.0.0.1:18001" };
      if (command === "finalize_agent_auto_follow_authorization") {
        return { ok: true, state: "authorized" };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Install />);
    expect(await screen.findByText("OpenCode")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "查看变更" }));
    // The native preview exposes the follow-exclusion projection before
    // confirmation: OpenCode multi-route follow is not authorized, so BOTH
    // the Free and Go candidates are skipped regardless of auth_mode.
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("preview_compound_route_opencode", {
      request: {
        plan: {
          discovery_revision: "setup-1",
          bindings: [{
            agent_id: "opencode",
            candidate_id: "opencode-free",
            candidate_source: "opencode-provider-catalog-cache",
            model: "deepseek-v4-flash-free",
          }, {
            agent_id: "opencode",
            candidate_id: "opencode-go",
            candidate_source: "opencode-provider-catalog-cache",
            model: "go-model",
          }],
        },
        opencode: { models: ["deepseek-v4-flash-free", "go-model"], listener_port: 18001 },
        enable_auto_follow: true,
      },
    }));
    fireEvent.click(screen.getByRole("button", { name: "确认并连接" }));

    // The compound apply still applies BOTH routes and the full OpenCode
    // model list; the receipt truthfully says OpenCode multi-route follow
    // is not authorized, so auto-follow is never staged or finalized.
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("apply_compound_route_opencode", {
      request: {
        plan: {
          discovery_revision: "setup-1",
          bindings: [{
            agent_id: "opencode",
            candidate_id: "opencode-free",
            candidate_source: "opencode-provider-catalog-cache",
            model: "deepseek-v4-flash-free",
          }, {
            agent_id: "opencode",
            candidate_id: "opencode-go",
            candidate_source: "opencode-provider-catalog-cache",
            model: "go-model",
          }],
        },
        opencode: { models: ["deepseek-v4-flash-free", "go-model"], listener_port: 18001 },
        reviewed_compound_revision: "compound-revision-1",
        reviewed_route_before_sha256: "route-before-hash",
        reviewed_route_after_sha256: "route-after-hash",
        reviewed_opencode_before_sha256: "before-hash",
        reviewed_opencode_after_sha256: "after-hash",
        enable_auto_follow: true,
      },
    }));
    // The receipt is not authorized for OpenCode multi-route and requires
    // no runtime verification, so the finalize command must never be called
    // for this OpenCode-only compound receipt.
    await waitFor(() => expect(invokeNative).toHaveBeenCalledWith("verify_hybrid_agent_integration", {
      request: { transaction_id: "agent-transaction-1" },
    }));
    expect(invokeNative).not.toHaveBeenCalledWith("finalize_agent_auto_follow_authorization", expect.anything());
    expect(await screen.findByText(/已确认配置生效、本地服务健康且路由可用/)).toBeInTheDocument();
  });
});
