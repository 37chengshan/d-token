import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: (...args: unknown[]) => native.invoke(...args),
}));

import { Compatibility, compatibilityCatalogEvidence } from "./Compatibility";

beforeEach(() => {
  native.invoke.mockReset();
});

afterEach(cleanup);

describe("Compatibility evidence boundaries", () => {
  it("keeps browser preview empty instead of loading production fallback rows", async () => {
    native.invoke.mockRejectedValue(new Error("native_unavailable"));

    render(<Compatibility />);

    expect(await screen.findByText("浏览器预览不提供本机兼容性证据")).toBeInTheDocument();
    const summary = screen.getByTestId("compatibility-summary");
    expect(within(summary).getAllByText("—")).toHaveLength(4);
    expect(screen.queryByText("OpenCode")).not.toBeInTheDocument();
    expect(screen.queryByText("agent -> d-token route adapter")).not.toBeInTheDocument();
    expect(screen.queryByText("browser preview")).not.toBeInTheDocument();
    expect(screen.queryByText("open_ai")).not.toBeInTheDocument();
    expect(screen.getByText("暂无 Agent 能力目录")).toBeInTheDocument();
    expect(screen.getByText("暂无本机路由")).toBeInTheDocument();
    expect(screen.getByText("暂无本机项目策略")).toBeInTheDocument();
    expect(screen.getByText("暂无协议能力目录")).toBeInTheDocument();
  });

  it("preserves verified native empty arrays as explicit empty states", async () => {
    native.invoke.mockResolvedValue(verifiedPayload({
      agents: [],
      routes: [],
      projects: [],
      runtimeCompatibility: {
        source: "dtoken-local-service",
        agents: [],
        protocolInjection: [],
        chainStatus: null,
      },
    }));

    render(<Compatibility />);

    expect(await screen.findByText(/^本机实时数据/)).toBeInTheDocument();
    expect(screen.getByText("暂无 Agent 能力目录")).toBeInTheDocument();
    expect(screen.getByText("暂无本机路由")).toBeInTheDocument();
    expect(screen.getByText("暂无本机项目策略")).toBeInTheDocument();
    expect(screen.getByText("暂无协议能力目录")).toBeInTheDocument();
    expect(screen.getByText("暂无本机链路状态")).toBeInTheDocument();
    expect(screen.queryByText("OpenCode")).not.toBeInTheDocument();
  });

  it("labels static capability rows as an implementation catalog while keeping runtime rows local", async () => {
    native.invoke.mockResolvedValue(verifiedPayload({
      agents: [],
      routes: [{
        id: "route-main",
        name: "主路由",
        protocol: "open_ai",
        target_provider_id: "provider-main",
      }],
      projects: [{
        root: "/workspace/project",
        stack: "Rust / TypeScript",
        agents: ["OpenCode"],
        route: "route-main",
        level: "balanced",
        features: ["Doctor"],
      }],
      runtimeCompatibility: {
        source: "dtoken-local-service",
        agents: [{
          agent: "OpenCode",
          rtk_supported: true,
          rtk_method: "integrated runtime",
          rtk_transparent_rewrite: "full",
          headroom_supported: true,
          headroom_method: "d-token local route",
          macos_support: "full",
          windows_native_support: "partial",
          windows_wsl_support: "full",
        }],
        protocolInjection: [{
          protocol: "open_ai",
          compatible_clients: ["OpenCode"],
          injection_method: "local route adapter",
        }],
        chainStatus: {
          mode: "compress_then_provider",
          status: "configured",
          routes: [{ route_id: "route-main", name: "主路由", fallback: "disabled" }],
        },
      },
    }));

    render(<Compatibility />);

    expect(await screen.findByText(/^本机实时数据/)).toBeInTheDocument();
    expect(screen.getAllByText("OpenCode")).not.toHaveLength(0);
    expect(screen.getAllByText("OpenAI")).not.toHaveLength(0);
    expect(screen.getAllByText("主路由")).not.toHaveLength(0);
    expect(screen.getByText("provider-main")).toBeInTheDocument();
    expect(screen.getByText("/workspace/project")).toBeInTheDocument();
    expect(screen.getByText("compress_then_provider")).toBeInTheDocument();

    const catalogBadges = screen.getAllByText("实现能力目录 · 非本机验证");
    expect(catalogBadges).toHaveLength(2);
    for (const badge of catalogBadges) {
      expect(badge).toHaveAttribute("data-source", compatibilityCatalogEvidence.source);
      expect(badge).toHaveAttribute("data-evidence-kind", compatibilityCatalogEvidence.evidence_kind);
      expect(badge).toHaveAttribute("data-verified-on-this-machine", "false");
    }
  });
});

function verifiedPayload(overrides: Record<string, unknown>) {
  return {
    desktop_revision: "compatibility-test-revision",
    source: "dtoken-owned-runtime",
    desktop_data_source: "dtoken-local-service",
    evidence_state: "verified",
    freshness: "fresh",
    complete: true,
    partial: false,
    transaction_provenance: {
      source: "dtoken-owned-runtime",
      command: "get_desktop_data",
      schema_version: 1,
      generated_at: "2026-07-11T12:00:00Z",
    },
    ...overrides,
  };
}
