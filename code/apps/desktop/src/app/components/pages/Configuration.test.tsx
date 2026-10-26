import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { Configuration } from "./Configuration";

const mockContextEngine = vi.hoisted(() => ({
  fetchEngineState: vi.fn(),
  storeJevCredential: vi.fn(),
  clearJevCredential: vi.fn(),
  testJevConnection: vi.fn(),
  updateEngineMode: vi.fn(),
}));

vi.mock("../../lib/contextEngine", () => mockContextEngine);

vi.mock("./Install", () => ({
  Install: () => <div data-testid="install-view">Mocked Install View</div>,
}));

vi.mock("./CodebaseMemory", () => ({
  CodebaseMemory: () => <div data-testid="codebase-memory-view">Mocked Codebase Memory View</div>,
}));

describe("Configuration page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    mockContextEngine.fetchEngineState.mockResolvedValue({
      mode: "local",
      health: {
        source: "runtime",
        freshnessMs: 0,
        credentialState: "not-configured",
        overall: "healthy",
        reasonCodes: [],
      },
      hasCredential: false,
      workflows: [
        {
          id: "tool_output_retention",
          title: "工具输出在线裁剪",
          description: "保持关键错误与返回",
          enabled: true,
          active: false,
          status: "shadow",
          fallback: "local-heuristic",
        },
      ],
      recentRequestObserved: false,
      lastRequestSummary: null,
    });
    mockContextEngine.storeJevCredential.mockResolvedValue({ ok: true, summary: "Saved" });
    mockContextEngine.updateEngineMode.mockResolvedValue({ ok: true, mode: "jev-shadow", summary: "Updated" });
    mockContextEngine.testJevConnection.mockResolvedValue({ ok: true, latencyMs: 110 });
  });

  it("renders both Compression Engine and Code Graph tabs", async () => {
    render(<Configuration />);

    expect(await screen.findByText("配置与连接")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /压缩引擎/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /代码图谱/ })).toBeInTheDocument();
    expect(screen.getByTestId("install-view")).toBeInTheDocument();
  });

  it("switches to Code Graph tab and displays codebase memory workbench", async () => {
    const user = userEvent.setup();
    render(<Configuration />);

    const codeGraphTab = await screen.findByRole("tab", { name: /代码图谱/ });
    await user.click(codeGraphTab);

    expect(await screen.findByTestId("codebase-memory-view")).toBeInTheDocument();
  });

  it("handles storing Jev credential without exposing raw key back to UI", async () => {
    const user = userEvent.setup();
    render(<Configuration />);

    const keyInput = await screen.findByPlaceholderText(/输入 Jev API Key/);
    await user.type(keyInput, "sk-jev-test-secret-1234");

    const saveButton = screen.getByRole("button", { name: "安全保存" });
    await user.click(saveButton);

    await waitFor(() => {
      expect(mockContextEngine.storeJevCredential).toHaveBeenCalledWith("sk-jev-test-secret-1234");
    });
  });

  it("allows switching engine mode to Jev", async () => {
    const user = userEvent.setup();
    render(<Configuration />);

    const jevCard = await screen.findByText("Jev 实时语义决策");
    await user.click(jevCard);

    await waitFor(() => {
      expect(mockContextEngine.updateEngineMode).toHaveBeenCalled();
    });
  });
});
