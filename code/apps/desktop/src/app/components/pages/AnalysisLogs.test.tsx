import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { AnalysisLogs } from "./AnalysisLogs";
import type { MetricsSummaryData } from "../analysis/WorkflowMetrics";
import type { DecisionTimelineItem } from "../analysis/DecisionTimeline";

vi.mock("./Changes", () => ({
  Changes: () => <div data-testid="changes-view">Mocked Changes View</div>,
}));

vi.mock("./LogsSecurity", () => ({
  LogsSecurity: () => <div data-testid="logs-security-view">Mocked Logs View</div>,
}));

describe("AnalysisLogs page", () => {
  const metrics: MetricsSummaryData = {
    totalRequests: 2,
    originalTokens: 1000,
    optimizedTokens: 700,
    tokensSaved: 300,
    reductionPercentage: 30,
    estimatedSavingsUsd: 0.12,
    providerConfirmedSavingsUsd: 0.1,
    localVsJevSplit: { localPercent: 50, jevPercent: 50 },
    fallbackRate: 0,
    p95LatencyLocalMs: 1,
    p95LatencyJevMs: 120,
    stickyHitRate: 50,
    recoveryFailures: 0,
    freshnessMs: 10,
    source: "test-runtime",
  };

  const timeline: DecisionTimelineItem[] = [
    {
      requestId: "req_01j8k9m01",
      timestamp: "2026-09-22T08:00:00Z",
      workflow: "tool_output_retention",
      agent: "OpenCode",
      model: "test-model",
      decisionEngine: "local",
      tokensBefore: 500,
      tokensAfter: 350,
      reductionPercentage: 30,
      reason: "redacted test evidence",
      confidenceBand: "high",
      validation: "accepted",
      recoveryPresent: true,
      sendStatus: "ok",
    },
    {
      requestId: "req_01j8k9m06",
      timestamp: "2026-09-22T08:01:00Z",
      workflow: "context_ranking",
      agent: "OpenCode",
      model: "test-model",
      decisionEngine: "local",
      tokensBefore: 500,
      tokensAfter: 500,
      reductionPercentage: 0,
      reason: "provider failure",
      confidenceBand: "n/a",
      validation: "bypassed",
      recoveryPresent: false,
      sendStatus: "failed",
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders all four tabs: Overview, Requests, Changes, Logs", async () => {
    render(<AnalysisLogs metricsData={metrics} timelineItems={[]} />);

    expect(await screen.findByText("分析与运行日志")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /运行概览/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /请求记录/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /变更与恢复/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /安全日志与导出/ })).toBeInTheDocument();
  });

  it("displays overview metrics with separate provider-confirmed and estimated savings", async () => {
    render(<AnalysisLogs metricsData={metrics} timelineItems={[]} />);

    expect(await screen.findByText("优化 Token 节省")).toBeInTheDocument();
    expect(screen.getByText("成本节省对照")).toBeInTheDocument();
    expect(screen.getByText("上游确认值")).toBeInTheDocument();
    expect(screen.getByText(/本地模型定价估算/)).toBeInTheDocument();
    expect(screen.getByText("决策引擎分流")).toBeInTheDocument();
  });

  it("displays redacted requests timeline and allows navigation to diagnostics on failure", async () => {
    const user = userEvent.setup();
    const handleDiagnostics = vi.fn();

    render(<AnalysisLogs metricsData={metrics} timelineItems={timeline} onNavigateToDiagnostics={handleDiagnostics} />);

    const requestsTab = screen.getByRole("tab", { name: /请求记录/ });
    await user.click(requestsTab);

    expect(await screen.findByText("脱敏保护已生效：此时间线仅展示结构化优化指标与决策理由，严禁记录或展示代码与 Prompt 明文。")).toBeInTheDocument();
    expect(screen.getByText("req_01j8k9m01")).toBeInTheDocument();

    const diagBtn = await screen.findByRole("button", { name: "前往诊断此请求" });
    await user.click(diagBtn);

    expect(handleDiagnostics).toHaveBeenCalledWith("req_01j8k9m06");
  });

  it("switches to Changes tab and displays changes workbench", async () => {
    const user = userEvent.setup();
    render(<AnalysisLogs />);

    const changesTab = screen.getByRole("tab", { name: /变更与恢复/ });
    await user.click(changesTab);

    expect(await screen.findByTestId("changes-view")).toBeInTheDocument();
  });

  it("switches to Logs tab and displays security logs workbench", async () => {
    const user = userEvent.setup();
    render(<AnalysisLogs />);

    const logsTab = screen.getByRole("tab", { name: /安全日志与导出/ });
    await user.click(logsTab);

    expect(await screen.findByTestId("logs-security-view")).toBeInTheDocument();
  });

  it("shows unknown evidence instead of sample metrics when runtime data is unavailable", async () => {
    render(<AnalysisLogs />);

    expect(await screen.findByText("等待请求级 token 证据")).toBeInTheDocument();
    expect(screen.getByText("等待证据")).toBeInTheDocument();
  });
});
