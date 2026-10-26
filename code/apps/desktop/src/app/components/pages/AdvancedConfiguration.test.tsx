import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { AdvancedConfiguration } from "./AdvancedConfiguration";

const mockContextEngine = vi.hoisted(() => ({
  fetchWorkflowPolicies: vi.fn(),
  previewWorkflowPolicyChanges: vi.fn(),
  applyWorkflowPolicyChanges: vi.fn(),
  rollbackWorkflowPolicy: vi.fn(),
  validateWorkflowPolicy: vi.fn(),
}));

vi.mock("../../lib/contextEngine", async () => {
  const actual = await vi.importActual("../../lib/contextEngine");
  return {
    ...actual,
    fetchWorkflowPolicies: mockContextEngine.fetchWorkflowPolicies,
    previewWorkflowPolicyChanges: mockContextEngine.previewWorkflowPolicyChanges,
    applyWorkflowPolicyChanges: mockContextEngine.applyWorkflowPolicyChanges,
    rollbackWorkflowPolicy: mockContextEngine.rollbackWorkflowPolicy,
  };
});

vi.mock("./SmartLearning", () => ({
  SmartLearning: () => <div data-testid="smart-learning-view">Mocked Smart Learning</div>,
}));

describe("AdvancedConfiguration page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    mockContextEngine.fetchWorkflowPolicies.mockResolvedValue({
      revision: "rev-mock-1",
      workflows: [
        {
          id: "tool_output_retention",
          title: "工具输出在线裁剪",
          description: "保持关键错误与返回",
          enabled: true,
          mode: "shadow",
          thresholdTokens: 2000,
          timeoutMs: 450,
          stickyTtlSeconds: 3600,
          fallback: "local-heuristic",
          killSwitch: false,
          isReserved: false,
        },
        {
          id: "arrival_span_retention",
          title: "增量历史选择 (Sticky)",
          description: "多轮对话间缓存已验证的重要判定",
          enabled: true,
          mode: "shadow",
          thresholdTokens: 1000,
          timeoutMs: 300,
          stickyTtlSeconds: 7200,
          fallback: "local-heuristic",
          killSwitch: false,
          isReserved: false,
        },
      ],
      readiness: {
        canActivate: false,
        shadowReplayCount: 15,
        shadowReplayRequired: 50,
        agreementRate: 0.90,
        agreementRateRequired: 0.95,
        latencyP95Ms: 210,
        latencyP95MaxMs: 300,
        unmetGates: [
          "Shadow 对照回放样本量不足 (当前 15/50)",
          "决策一致性尚未达到 95% 门槛",
        ],
      },
    });

    mockContextEngine.previewWorkflowPolicyChanges.mockResolvedValue({
      ok: true,
      revision: "rev-mock-1",
      diff: "[tool_output_retention] mode: shadow, threshold: 2500",
      summary: "调整 2 项工作流策略",
      requiresConfirmation: false,
    });

    mockContextEngine.applyWorkflowPolicyChanges.mockResolvedValue({
      ok: true,
      summary: "上下文工作流策略已持久化",
      rollbackId: "rb-test-12345",
    });

    mockContextEngine.rollbackWorkflowPolicy.mockResolvedValue({
      ok: true,
      summary: "已成功回退到策略版本 rb-test-12345",
    });
  });

  it("renders all four policy groups in the top tabs", async () => {
    render(<AdvancedConfiguration />);

    expect(await screen.findByText("高级策略配置")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Jev 工作流/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /智能学习/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /路由与缓存/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /恢复与留存/ })).toBeInTheDocument();
  });

  it("renders workflow policy editor with readiness metrics and unmet gates", async () => {
    render(<AdvancedConfiguration />);

    expect(await screen.findByText("工具输出在线裁剪")).toBeInTheDocument();
    expect(screen.getByText("尚未达到准入门槛（受保卫拦截）")).toBeInTheDocument();
    expect(screen.getByText(/Shadow 对照回放样本量不足/)).toBeInTheDocument();
  });

  it("handles workflow parameter modification, preview and apply transaction", async () => {
    const user = userEvent.setup();
    render(<AdvancedConfiguration />);

    expect(await screen.findByText("工具输出在线裁剪")).toBeInTheDocument();

    const previewBtn = screen.getByRole("button", { name: "预览策略变更" });
    await user.click(previewBtn);

    expect(mockContextEngine.previewWorkflowPolicyChanges).toHaveBeenCalled();
    expect(await screen.findByText("确认应用工作流策略变更")).toBeInTheDocument();

    const confirmBtn = screen.getByRole("button", { name: "确认并写入" });
    await user.click(confirmBtn);

    await waitFor(() => {
      expect(mockContextEngine.applyWorkflowPolicyChanges).toHaveBeenCalled();
    });

    expect(await screen.findByText(/策略已成功应用/)).toBeInTheDocument();
  });

  it("allows switching tabs to Routing & Cache and toggling routing authorization", async () => {
    const user = userEvent.setup();
    render(<AdvancedConfiguration />);

    const routingTab = screen.getByRole("tab", { name: /路由与缓存/ });
    await user.click(routingTab);

    expect(await screen.findByText("模型路由独立授权")).toBeInTheDocument();
    const checkbox = screen.getByRole("checkbox");
    expect(checkbox).not.toBeChecked();

    await user.click(checkbox);
    expect(checkbox).toBeChecked();
    expect(screen.getByText("已单独授权")).toBeInTheDocument();
  });

  it("switches to Smart Learning tab and displays learning policy panel", async () => {
    const user = userEvent.setup();
    render(<AdvancedConfiguration />);

    const learningTab = screen.getByRole("tab", { name: /智能学习/ });
    await user.click(learningTab);

    expect(await screen.findByTestId("learning-policy-panel")).toBeInTheDocument();
    expect(screen.getByTestId("smart-learning-view")).toBeInTheDocument();
  });

  it("does not claim cache clearing succeeded when the native command is unavailable", async () => {
    const user = userEvent.setup();
    render(<AdvancedConfiguration initialTab="routing-cache" />);

    await user.click(await screen.findByRole("button", { name: "清空当前缓存" }));

    expect(await screen.findByText("尚无运行时清理命令，缓存未改变")).toBeInTheDocument();
  });
});
