import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { Diagnostics } from "./Diagnostics";

vi.mock("./Doctor", () => ({
  Doctor: () => <div data-testid="doctor-view">Mocked Doctor View</div>,
}));

vi.mock("./Compatibility", () => ({
  Compatibility: () => <div data-testid="compatibility-view">Mocked Compatibility View</div>,
}));

describe("Diagnostics page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders both Doctor and Compatibility tabs", async () => {
    render(<Diagnostics />);

    expect(await screen.findByText("系统诊断与兼容性")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /系统与运行时诊断/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /兼容性与平台矩阵/ })).toBeInTheDocument();
    expect(screen.getByTestId("doctor-view")).toBeInTheDocument();
  });

  it("displays four contract categories banner", async () => {
    render(<Diagnostics />);

    expect(await screen.findByText("诊断健康分类规则契约")).toBeInTheDocument();
    expect(screen.getByText(/1\. 需处理/)).toBeInTheDocument();
    expect(screen.getByText(/2\. 可选未配置/)).toBeInTheDocument();
    expect(screen.getByText(/3\. 正常/)).toBeInTheDocument();
    expect(screen.getByText(/4\. 证据不足/)).toBeInTheDocument();
  });

  it("displays correlation ID banner when passed and allows clearing", async () => {
    const user = userEvent.setup();
    const handleClear = vi.fn();

    render(<Diagnostics correlationId="req_01j8k9m06" onClearCorrelationId={handleClear} />);

    expect(await screen.findByText("req_01j8k9m06")).toBeInTheDocument();
    expect(screen.getByText(/正在针对失败请求进行定向诊断分析/)).toBeInTheDocument();

    const clearBtn = screen.getByRole("button", { name: /清除过滤/ });
    await user.click(clearBtn);

    expect(handleClear).toHaveBeenCalled();
  });

  it("switches to Compatibility tab", async () => {
    const user = userEvent.setup();
    render(<Diagnostics />);

    const compatTab = screen.getByRole("tab", { name: /兼容性与平台矩阵/ });
    await user.click(compatTab);

    expect(await screen.findByTestId("compatibility-view")).toBeInTheDocument();
  });
});
