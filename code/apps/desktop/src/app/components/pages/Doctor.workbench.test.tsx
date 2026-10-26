import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
}));

import { Doctor } from "./Doctor";

const reportWithRepair = {
  ok: false,
  mode: "doctor-report-v2",
  revision: "doctor-rev-1",
  source: "dtoken-desktop-control-plane",
  freshness: "fresh",
  collected_at: "2026-07-15T10:00:00Z",
  summary: "1 issue needs attention",
  counts: { healthy: 3, repairable: 1, manual: 0, blocked: 0 },
  issues: [
    {
      id: "service-not-running",
      category: "runtime",
      severity: "error",
      state: "repairable",
      title: "d-token 本地服务未运行",
      impact: "Agent 请求当前无法经过 d-token。",
      cause: "未检测到由当前桌面应用管理的运行实例。",
      evidence: "127.0.0.1:18001 未返回已验证的 d-token runtime identity。",
      remediation: {
        kind: "restart_managed_service",
        label: "启动并验证本地服务",
        risk: "low",
        writes_config: false,
        requires_confirmation: true,
      },
    },
  ],
};

const healthyReport = {
  ...reportWithRepair,
  ok: true,
  revision: "doctor-rev-2",
  collected_at: "2026-07-15T10:01:00Z",
  summary: "All common checks passed",
  counts: { healthy: 4, repairable: 0, manual: 0, blocked: 0 },
  issues: [],
};

const v3Snapshot = {
  schema_version: 3,
  revision: "doctor-v3-rev-1",
  source: "dtoken-desktop-control-plane",
  state: "attention",
  collected_at_ms: Date.now(),
  ttl_ms: 60_000,
  coverage: { completed: 5, total: 5, agents_detected: 3, agents_selected: 3 },
  counts: { healthy: 2, repairable: 2, manual: 1, blocked: 0 },
  modules: [
    {
      module_id: "core",
      state: "repairable",
      severity: "error",
      summary: "d-token 本地服务需要重新验证。",
      reason_codes: ["runtime_identity_stale"],
    },
    {
      module_id: "agent",
      state: "repairable",
      severity: "warning",
      summary: "2 个 Agent 接入需要审阅。",
      reason_codes: ["agent_integration_review_required"],
      agents: [
        { id: "claude-code", name: "Claude Code", detected: true, supported: true, managed_scopes: ["global", "project"], managed_targets: [{ scope: "project", project_root: "/workspace/demo" }] },
        { id: "codex", name: "Codex", detected: true, supported: true, managed_scopes: ["global", "project"], managed_targets: [{ scope: "project", project_root: "/workspace/demo" }] },
        { id: "opencode", name: "OpenCode", detected: true, supported: true, managed_scopes: ["global", "project"], managed_targets: [{ scope: "project", project_root: "/workspace/demo" }] },
      ],
    },
    { module_id: "optical", state: "healthy", severity: "info", summary: "混合上下文优化保持安全 Shadow。", reason_codes: [] },
    { module_id: "platform", state: "healthy", severity: "info", summary: "平台检查已通过。", reason_codes: [] },
    { module_id: "recovery", state: "manual", severity: "warning", summary: "一个旧备份需要人工审阅。", reason_codes: ["legacy_backup"] },
  ],
};

const v3DomainSnapshot = {
  schema_version: 3,
  revision: "doctor-v3-domain-rev-1",
  source: "combined",
  overall_state: "attention",
  collected_at_ms: Date.now(),
  ttl_ms: 60_000,
  modules: [
    ["core_service_lifecycle", "healthy"],
    ["core_port_process_identity", "healthy"],
    ["core_route_verification", "healthy"],
    ["agent_discovery", "healthy"],
    ["agent_transport", "healthy"],
    ["agent_instructions", "attention"],
    ["agent_hook_plugin", "healthy"],
    ["agent_skill", "healthy"],
    ["agent_metadata_mcp", "healthy"],
    ["agent_cc_switch_sync", "healthy"],
    ["optical_observability", "healthy"],
    ["optical_shadow_policy", "healthy"],
    ["platform_autostart", "healthy"],
    ["platform_runtime_package_identity", "healthy"],
    ["recovery_rollback_evidence", "healthy"],
    ["recovery_specialized_transaction_rollback", "healthy"],
  ].map(([module_id, state]) => ({
    module_id,
    state,
    observed_at_ms: Date.now(),
    evidence: [],
  })),
};

const v3AgentPlan = {
  schema_version: 3,
  ok: true,
  plan_id: "doctor-plan-agent-1",
  snapshot_revision: "doctor-v3-rev-1",
  module_id: "agent_hybrid_reinject",
  selection: {
    agent_ids: ["claude-code", "codex", "opencode"],
    route_ids: [],
    scope: "project",
    project_root: "/workspace/demo",
    install_instructions: true,
    install_hook_or_plugin: true,
    install_skill: true,
    install_mcp: false,
  },
  risk: "medium",
  repair_class: "specialized_confirmation",
  confirmation_required: true,
  digest_algorithm: "fnv1a64_v1",
  created_at_ms: Date.now(),
  expires_at_ms: Date.now() + 60_000,
  review_digest: "review-digest-1",
  summary: "审阅 3 个 Agent 的受控接入变更。",
  agents: [
    {
      agent_id: "claude-code",
      name: "Claude Code",
      state: "ready",
      targets: [{ target_path: "/workspace/demo/CLAUDE.md", before_sha256: "before-claude", after_sha256: "after-claude", diff: "+ d-token managed instructions" }],
    },
    {
      agent_id: "codex",
      name: "Codex",
      state: "ready",
      targets: [{ target_path: "/workspace/demo/AGENTS.md", before_sha256: "before-codex", after_sha256: "after-codex", diff: "+ d-token managed instructions" }],
    },
    {
      agent_id: "opencode",
      name: "OpenCode",
      state: "ready",
      targets: [{ target_path: "/workspace/demo/opencode.json", before_sha256: "before-opencode", after_sha256: "after-opencode", diff: "+ d-token plugin entry" }],
    },
  ],
};

beforeEach(() => {
  native.invokeNative.mockReset();
  native.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_doctor_snapshot") return null;
    if (command === "run_doctor_scan") return reportWithRepair;
    if (command === "preview_doctor_common_repair") {
      return {
        ok: true,
        mode: "doctor-repair-plan",
        report_revision: "doctor-rev-1",
        issue_ids: ["service-not-running"],
        risk: "low",
        summary: "Restart the verified d-token-owned service and run Doctor again.",
      };
    }
    if (command === "apply_doctor_common_repair") {
      return {
        ok: true,
        mode: "doctor-action-receipt",
        operation_id: "repair-1",
        summary: "Repair completed and verified",
        report: healthyReport,
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });
});

afterEach(cleanup);

describe("Doctor common-problem workbench", () => {
  it("fails closed for a stale v2 compatibility snapshot", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_doctor_snapshot") return { ...reportWithRepair, freshness: "stale" };
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Doctor setPage={vi.fn()} />);

    expect(await screen.findByText("诊断结果已过期")).toBeVisible();
    expect(screen.getByText(/重新扫描当前本机状态/)).toBeVisible();
    expect(screen.queryByText(/Doctor v2|Doctor v3|fresh|统一、本地、可复验/)).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "选择 d-token 本地服务未运行" })).not.toBeInTheDocument();
    expect(native.invokeNative).toHaveBeenCalledTimes(1);
  });

  it("runs one user-triggered scan and renders evidence, impact, and a repair candidate", async () => {
    render(<Doctor setPage={vi.fn()} />);

    expect(await screen.findByText("检查连接是否正常")).toBeVisible();
    expect(screen.queryByText("0/—")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "扫描常规问题" }));

    expect(await screen.findByText("d-token 本地服务未运行")).toBeVisible();
    expect(screen.getByText("Agent 请求当前无法经过 d-token。")).toBeVisible();
    const issueRow = screen.getByText("d-token 本地服务未运行").closest(".grid");
    expect(issueRow).not.toBeNull();
    fireEvent.click(within(issueRow as HTMLElement).getByText("查看原因"));
    expect(screen.getByText(/127\.0\.0\.1:18001/)).toBeVisible();
    expect(native.invokeNative.mock.calls.filter(([command]) => command === "run_doctor_scan")).toHaveLength(1);
    expect(native.invokeNative.mock.calls.some(([command]) => command === "run_product_readiness_check")).toBe(false);
    expect(screen.getByRole("checkbox", { name: "选择 d-token 本地服务未运行" })).toBeChecked();
  });

  it("previews selected repairs, requires confirmation, and displays the post-repair verification", async () => {
    render(<Doctor setPage={vi.fn()} />);
    await screen.findByText("检查连接是否正常");
    fireEvent.click(screen.getByRole("button", { name: "扫描常规问题" }));
    await screen.findByText("d-token 本地服务未运行");

    fireEvent.click(screen.getByRole("button", { name: "修复所选问题" }));
    const dialog = await screen.findByRole("alertdialog", { name: "确认常规修复" });
    expect(within(dialog).getByText(/只会启动或重启身份已验证的 d-token 自有服务/)).toBeVisible();
    expect(native.invokeNative).not.toHaveBeenCalledWith("apply_doctor_common_repair", expect.anything());

    fireEvent.click(within(dialog).getByRole("button", { name: "确认修复并复检" }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("apply_doctor_common_repair", {
        request: {
          report_revision: "doctor-rev-1",
          issue_ids: ["service-not-running"],
          confirmed: true,
        },
      });
    });
    expect(await screen.findByText("常规问题已修复并通过复检")).toBeVisible();
    expect(screen.getByText("当前未发现需要处理的常规问题。")).toBeVisible();
  });

  it("renders the v3 scan phase, freshness, coverage, statistics, and module tasks", async () => {
    let resolveScan: (value: typeof v3Snapshot) => void = () => undefined;
    native.invokeNative.mockImplementation((command: string) => {
      if (command === "get_doctor_snapshot") return Promise.resolve(null);
      if (command === "run_doctor_scan") return new Promise((resolve) => { resolveScan = resolve; });
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Doctor setPage={vi.fn()} />);
    await screen.findByText("检查连接是否正常");
    fireEvent.click(screen.getByRole("button", { name: "扫描常规问题" }));

    expect(screen.getByRole("status")).toHaveTextContent("正在检查本地服务与工具连接");
    resolveScan(v3Snapshot);

    expect(await screen.findByText("2/5")).toBeVisible();
    expect(screen.getByText("检查范围")).toBeVisible();
    expect(screen.getByText("5/5")).toBeVisible();
    expect(screen.getByText("需要处理")).toBeVisible();
    expect(screen.getByText("工具配置")).toBeVisible();
    expect(screen.getByText("视觉压缩")).toBeVisible();
    expect(native.invokeNative.mock.calls.filter(([command]) => command === "run_doctor_scan")).toHaveLength(1);
  });

  it("accepts the native product snapshot with its registered sync check and shows all six results", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_doctor_snapshot") return null;
      if (command === "run_doctor_scan") return {
        ...v3Snapshot, collected_at_ms: Date.now(), state: "blocked",
        coverage: { completed: 5, total: 6 },
        counts: { healthy: 1, repairable: 1, manual: 2, blocked: 1 },
        modules: [
          { module_id: "core", state: "manual" },
          { module_id: "agent", state: "repairable" },
          { module_id: "agent_cc_switch_sync", state: "manual", summary: "连接需要同步" },
          { module_id: "optical", state: "unknown" },
          { module_id: "platform", state: "healthy" },
          { module_id: "recovery", state: "blocked" },
        ],
      };
      throw new Error("unexpected command");
    });
    render(<Doctor setPage={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "扫描常规问题" }));
    expect(await screen.findByText("1/6")).toBeVisible();
    expect(screen.getByText("配置同步")).toBeVisible();
    expect(screen.getByText("5/6")).toBeVisible();
    expect(screen.getByText(/修复保持锁定/)).toBeVisible();
    expect(screen.queryByText("Doctor 扫描失败")).not.toBeInTheDocument();
  });

  it("still rejects unregistered modules instead of silently discarding their results", async () => {
    native.invokeNative.mockImplementation(async (command: string) => command === "get_doctor_snapshot" ? null : ({
      ...v3Snapshot, collected_at_ms: Date.now(),
      modules: [...v3Snapshot.modules, { module_id: "unregistered-check", state: "healthy" }],
    }));
    render(<Doctor setPage={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "扫描常规问题" }));
    expect(await screen.findByText(/请重试；如果问题仍然存在/)).toBeVisible();
    expect(screen.queryByText("0/—")).not.toBeInTheDocument();
  });

  it("aggregates the Rust v3 domain snapshot into product modules and keeps evidence-only attention non-actionable", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_doctor_snapshot" || command === "run_doctor_scan") {
        return { ...v3DomainSnapshot, collected_at_ms: Date.now() };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Doctor setPage={vi.fn()} />);

    expect(await screen.findByText("15/16")).toBeVisible();
    expect(screen.getByText("检查范围")).toBeVisible();
    expect(screen.getByText("16/16")).toBeVisible();
    expect(screen.getByText("1 项需要处理，已优先排在右侧。")).toBeVisible();
    expect(screen.queryByRole("button", { name: "处理工具配置" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "自动扫描并修复" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("run_doctor_scan"));
    expect(native.invokeNative.mock.calls.some(([command]) => command === "preview_doctor_repair")).toBe(false);
    expect(native.invokeNative.mock.calls.some(([command]) => command === "apply_doctor_repair")).toBe(false);
  });

  it("reviews per-Agent diffs before confirming a v3 Agent repair and reports partial results", async () => {
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_doctor_snapshot") return v3Snapshot;
      if (command === "preview_doctor_repair") return v3AgentPlan;
      if (command === "apply_doctor_repair") {
        return {
          schema_version: 3,
          operation_id: "doctor-operation-agent-1",
          plan_id: "doctor-plan-agent-1",
          plan_review_digest: "review-digest-1",
          snapshot_revision: "doctor-v3-rev-1",
          module_id: "agent_hybrid_reinject",
          outcome: "applied",
          post_verification: "partial",
          started_at_ms: Date.now(),
          completed_at_ms: Date.now(),
          post_snapshot_revision: "doctor-v3-rev-2",
          agents: [
            { agent_id: "claude-code", name: "Claude Code", outcome: "verified", summary: "规则已验证" },
            { agent_id: "codex", name: "Codex", outcome: "verified", summary: "规则已验证" },
            { agent_id: "opencode", name: "OpenCode", outcome: "blocked", summary: "配置在审阅后发生变化" },
          ],
        };
      }
      throw new Error(`unexpected command: ${command} ${JSON.stringify(args)}`);
    });

    render(<Doctor setPage={vi.fn()} />);
    expect(await screen.findByText("2 个 Agent 接入需要审阅。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "处理工具配置" }));

    const dialog = await screen.findByRole("dialog", { name: "Agent 接入诊断与修复" });
    expect(within(dialog).getByRole("checkbox", { name: "选择 Claude Code" })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: "选择 Codex" })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: "选择 OpenCode" })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: "元数据诊断 MCP（默认关闭）" })).not.toBeChecked();

    fireEvent.change(within(dialog).getByLabelText("作用域"), { target: { value: "project" } });
    fireEvent.change(within(dialog).getByLabelText("项目绝对路径"), { target: { value: "/workspace/demo" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "生成修复计划" }));

    expect(await within(dialog).findByText("/workspace/demo/AGENTS.md")).toBeVisible();
    expect(within(dialog).getAllByText("+ d-token managed instructions")).toHaveLength(2);
    expect(native.invokeNative).toHaveBeenCalledWith("preview_doctor_repair", {
      request: {
        snapshot_revision: "doctor-v3-rev-1",
        module_id: "agent_hybrid_reinject",
        selection: {
          agent_ids: ["claude-code", "codex", "opencode"],
          route_ids: [],
          scope: "project",
          project_root: "/workspace/demo",
          install_instructions: true,
          install_hook_or_plugin: true,
          install_skill: true,
          install_mcp: false,
        },
      },
    });

    fireEvent.click(within(dialog).getByRole("button", { name: "确认、应用并验证" }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("apply_doctor_repair", {
        request: {
          plan_id: "doctor-plan-agent-1",
          review_digest: "review-digest-1",
          snapshot_revision: "doctor-v3-rev-1",
          module_id: "agent_hybrid_reinject",
          selection: v3AgentPlan.selection,
          confirmed: true,
        },
      });
    });
    expect(await within(dialog).findByText("部分完成")).toBeVisible();
    expect(within(dialog).getByText("Claude Code · 已验证")).toBeVisible();
    expect(within(dialog).getByText("OpenCode · 已阻止")).toBeVisible();
  });

  it("fails closed for stale or partial v3 snapshots", async () => {
    const partial = {
      ...v3Snapshot,
      state: "partial",
      collected_at_ms: Date.now() - 120_000,
      ttl_ms: 1_000,
    };
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_doctor_snapshot") return partial;
      if (command === "run_doctor_scan") return partial;
      throw new Error(`unexpected command: ${command}`);
    });

    const navigate = vi.fn();
    render(<Doctor setPage={navigate} />);
    expect((await screen.findAllByText("部分证据 · 已过期")).length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: "处理工具配置" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "查看工具设置" }));
    expect(navigate).toHaveBeenCalledWith("configuration");
    fireEvent.click(screen.getByRole("button", { name: "自动扫描并修复" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("run_doctor_scan"));
    expect(native.invokeNative.mock.calls.some(([command]) => command === "preview_doctor_repair")).toBe(false);
    expect(native.invokeNative.mock.calls.some(([command]) => command === "apply_doctor_repair")).toBe(false);
  });
});
