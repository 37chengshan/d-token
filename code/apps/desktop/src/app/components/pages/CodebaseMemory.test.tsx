import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationProvider, useNotifications } from "../../lib/notifications";
import { CodebaseMemory } from "./CodebaseMemory";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
}));

const dimension = (state: string, detail: string) => ({
  state,
  reason_code: `${state}_reason`,
  detail,
  checked_at_ms: 1_700_000_000_000,
  source: "dtoken-desktop-control-plane",
  freshness: "live",
  warnings: [],
});

const ownedUiDimension = (state: string, detail: string) => ({
  ...dimension(state, detail),
  owner: "dtoken",
});

const status = (overrides: Record<string, unknown> = {}) => ({
  schema_version: 1,
  source: "dtoken-desktop-control-plane",
  freshness: "live",
  revision: "cbm-1",
  approved_project_root: "/approved/project",
  package_directory: "/components/codebase-memory",
  package_directory_custom: false,
  package: dimension("ready", "Signed local package verified"),
  integration: dimension("not_configured", "OpenCode MCP is not configured"),
  agent_session: dimension("ready", "Agent launcher evidence observed"),
  ui_session: dimension("failed", "Graph page is unavailable"),
  index: dimension("partial", "Index is stale"),
  ...overrides,
});

const agentDiscovery = {
  agents: [
    { id: "opencode", name: "OpenCode", installed: true, detected: true, integration_supported: true },
    { id: "codex", name: "Codex", installed: true, detected: true, integration_supported: true },
    { id: "claude-code", name: "Claude Code", installed: false, detected: false, integration_supported: true },
    { id: "cursor", name: "Cursor", installed: true, detected: true, integration_supported: false },
  ],
};

function renderPage() {
  return render(<NotificationProvider><CodebaseMemory /></NotificationProvider>);
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("CodebaseMemory", () => {
  beforeEach(() => {
    native.invokeNative.mockReset();
  });

  afterEach(cleanup);

  it("renders a responsive loading surface while native package verification is pending", async () => {
    let resolveStatus: ((value: ReturnType<typeof status>) => void) | undefined;
    native.invokeNative.mockReturnValue(new Promise((resolve) => {
      resolveStatus = resolve;
    }));
    renderPage();

    expect(await screen.findByRole("heading", { level: 1, name: "代码图谱（实验性）" })).toBeInTheDocument();
    expect(screen.getByText("正在读取代码图谱状态…")).toBeVisible();
    expect(screen.getByRole("button", { name: "刷新" })).toBeDisabled();

    resolveStatus?.(status());
    expect(await screen.findByRole("heading", { level: 2, name: "图谱组件" })).toBeVisible();
  });

  it("refreshes an in-progress index without native events and reports its terminal result once", async () => {
    vi.useFakeTimers();
    let reads = 0;
    let titles: string[] = [];
    function Probe() { titles = useNotifications().notifications.map((item) => item.title); return null; }
    try {
      native.invokeNative.mockImplementation(async (command: string) => {
        if (command === "get_codebase_memory_status") {
          reads += 1;
          return status({ index: dimension(reads === 1 ? "busy" : "failed", "Index result") });
        }
        return agentDiscovery;
      });
      await act(async () => { render(<NotificationProvider><CodebaseMemory /><Probe /></NotificationProvider>); });
      await act(async () => { vi.advanceTimersByTime(0); });
      await act(async () => { vi.advanceTimersByTime(1000); });
      const row = screen.getByRole("heading", { level: 3, name: "项目索引" }).closest("[data-flat-row]");
      expect(within(row as HTMLElement).getByText("失败")).toBeVisible();
      expect(titles.filter((title) => title === "索引任务已结束，请查看结果")).toHaveLength(1);
      const finalReads = reads;
      await act(async () => { vi.advanceTimersByTime(5000); });
      expect(reads).toBe(finalReads);
    } finally {
      cleanup();
      vi.useRealTimers();
    }
  });

  it("renders five independent evidence dimensions without collapsing them to green", async () => {
    native.invokeNative.mockResolvedValue(status());
    renderPage();

    await screen.findByRole("heading", { level: 1, name: "代码图谱（实验性）" });
    const expected = [
      ["图谱组件", "已验证"],
      ["编码工具连接", "未配置"],
      ["编码工具使用情况", "已验证"],
      ["图谱页面", "失败"],
      ["项目索引", "部分可用"],
    ];
    for (const [heading, stateLabel] of expected) {
      const row = heading === "图谱组件" ? screen.getByRole("region", { name: "图谱组件" }) : screen.getByRole("heading", { level: 3, name: heading }).closest("[data-flat-row]");
      expect(row).toBeTruthy();
      expect(await within(row as HTMLElement).findByText(stateLabel)).toBeVisible();
    }
    expect(screen.queryByText(/连接完成不代表图谱页面或索引已经可用/)).not.toBeInTheDocument();
  });

  it("restores every persisted Agent transaction as a batch rollback after reload", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status({
        integration: dimension("ready", "Two Agent integrations are verified"),
        active_integrations: [
          { transaction_id: "tx-opencode", agent_id: "opencode", state: "verified", rollback_id: "rb-opencode" },
          { transaction_id: "tx-codex", agent_id: "codex", state: "verified", rollback_id: "rb-codex" },
        ],
      });
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    expect(await screen.findByRole("button", { name: "批量回退本次配置" })).toBeVisible();
  });

  it("retries incomplete recovery without rolling back a verified connection", async () => {
    const user = userEvent.setup();
    let attempts = 0;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status({
        active_integrations: [
          { transaction_id: "tx-opencode", agent_id: "opencode", state: "verified", rollback_id: "rb-opencode" },
          ...(attempts < 2 ? [{ transaction_id: "tx-codex", agent_id: "codex", state: "recovery_required", rollback_id: "rb-codex" }] : []),
        ],
      });
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "rollback_codebase_memory_integration") {
        attempts += 1;
        return { ok: attempts > 1, state: attempts > 1 ? "rolled_back" : "recovery_required" };
      }
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();
    await user.click(await screen.findByRole("button", { name: "重试恢复未完成的配置" }));
    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByText("Codex")).toBeVisible();
    expect(within(dialog).queryByText("OpenCode")).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "确认回退" }));
    await waitFor(() => expect(attempts).toBe(1));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("恢复失败");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "确认回退" })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: "确认回退" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(screen.queryByRole("button", { name: "重试恢复未完成的配置" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "批量回退本次配置" })).toBeVisible();
    const calls = native.invokeNative.mock.calls.filter(([command]) => command === "rollback_codebase_memory_integration");
    expect(calls).toHaveLength(2);
    for (const [, args] of calls) expect(args).toEqual({ request: { transaction_id: "tx-codex" } });
  });

  it("guides a lightweight user import and saves an existing custom component directory", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_codebase_memory_status") return status({
        package: dimension("unavailable", "Download and extract the currently supported component"),
      });
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "open_codebase_memory_download_page") return { ok: true, summary: "Opened current official release" };
      if (command === "open_codebase_memory_package_directory") return { ok: true, summary: "Opened import directory" };
      if (command === "set_codebase_memory_package_directory") return { ok: true, summary: "Directory saved" };
      throw new Error(`unexpected command ${command} ${JSON.stringify(args)}`);
    });
    renderPage();

    // The user sees only the current supported component, never an internal
    // verification mechanism or a hard-coded version number.
    expect(await screen.findByRole("heading", { level: 2, name: "图谱组件" })).toBeVisible();
    expect(screen.getByText("未找到可用组件，可导入下载好的压缩包。")).toBeVisible();
    expect(screen.queryByText(/v0\.9\.1/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "官方下载" }));
    expect(native.invokeNative).toHaveBeenCalledWith("open_codebase_memory_download_page");
    await user.click(screen.getByText("位置与其他导入方式"));
    const directory = screen.getByRole("textbox", { name: "已解压组件的位置" });
    await user.clear(directory);
    await user.type(directory, "/custom/code-graph");
    await user.click(screen.getByRole("button", { name: "保存目录" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("set_codebase_memory_package_directory", {
      request: { directory: "/custom/code-graph" },
    }));
  });

  it("recognizes an installed component and keeps its path in optional details", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => command === "get_codebase_memory_status"
      ? status({ package_candidates: [{ path: "/installed/bin/codebase-memory-mcp", compatible: true, source: "installed", reason_code: "verified" }] })
      : agentDiscovery);
    renderPage();
    expect(await screen.findByText("已找到并验证本机组件。")).toBeVisible();
    expect(screen.getByText("/installed/bin/codebase-memory-mcp")).not.toBeVisible();
    expect(screen.getByLabelText("已解压组件的位置")).not.toBeVisible();
    await user.click(screen.getByText("位置与其他导入方式"));
    expect(screen.getByText("/installed/bin/codebase-memory-mcp")).toBeVisible();
    expect(native.invokeNative.mock.calls.map(([command]) => command)).not.toContain("set_codebase_memory_package_directory");
  });

  it("selects an archive without writing and imports it only after the shown confirmation", async () => {
    const user = userEvent.setup();
    let imported = false;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status({ package: dimension(imported ? "ready" : "unavailable", "Component state") });
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "select_codebase_memory_archive") return "/downloads/graph.tar.gz";
      if (command === "import_codebase_memory_archive") { imported = true; return { ok: true, state: "imported" }; }
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();
    await user.click(await screen.findByRole("button", { name: "导入压缩包" }));
    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByText("graph.tar.gz")).toBeVisible();
    expect(imported).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "校验并导入" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(await screen.findByText("已找到并验证本机组件。")).toBeVisible();
    expect(native.invokeNative).toHaveBeenCalledWith("import_codebase_memory_archive", { request: { archive_path: "/downloads/graph.tar.gz" } });
  });

  it("keeps the original component when an archive is rejected and explains how to retry", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "select_codebase_memory_archive") return "/downloads/wrong.zip";
      if (command === "import_codebase_memory_archive") return { ok: false, error: "archive_hash_mismatch" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();
    await user.click(await screen.findByRole("button", { name: "导入压缩包" }));
    const dialog = screen.getByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "校验并导入" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("原有组件未更改");
    expect(within(dialog).getByRole("button", { name: "校验并导入" })).toBeEnabled();
    await user.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(screen.getByText("已找到并验证本机组件。")).toBeVisible();
    expect(native.invokeNative.mock.calls.map(([command]) => command)).not.toContain("set_codebase_memory_package_directory");
  });

  it("does nothing when archive selection is cancelled", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "select_codebase_memory_archive") return null;
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();
    await user.click(await screen.findByRole("button", { name: "导入压缩包" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(native.invokeNative.mock.calls.map(([command]) => command)).not.toContain("import_codebase_memory_archive");
  });

  it("clears the old ready status if native activation cannot restore previous settings", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "select_codebase_memory_archive") return "/downloads/graph.tar.gz";
      if (command === "import_codebase_memory_archive") return { ok: false, state: "recovery_required" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();
    await user.click(await screen.findByRole("button", { name: "导入压缩包" }));
    const dialog = screen.getByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "校验并导入" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("原有设置未能完整恢复");
    expect(screen.queryByText("已找到并验证本机组件。")).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(screen.getByRole("button", { name: "查看并连接编码工具" })).toBeDisabled();
  });

  it("lets the user choose an extracted component directory and save the selected draft", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "select_codebase_memory_package_directory") return "/custom/chosen-code-graph";
      if (command === "set_codebase_memory_package_directory") return { ok: true, summary: "Directory saved" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByText("位置与其他导入方式"));
    await user.click(screen.getByRole("button", { name: "选择已解压目录" }));
    const directory = screen.getByRole("textbox", { name: "已解压组件的位置" });
    expect(directory).toHaveValue("/custom/chosen-code-graph");
    expect(native.invokeNative).not.toHaveBeenCalledWith("set_codebase_memory_package_directory", expect.anything());

    await user.click(screen.getByRole("button", { name: "保存目录" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("set_codebase_memory_package_directory", {
      request: { directory: "/custom/chosen-code-graph" },
    }));
  });

  it("keeps the current directory unchanged when the native picker is cancelled", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "select_codebase_memory_package_directory") return null;
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByText("位置与其他导入方式"));
    await user.click(screen.getByRole("button", { name: "选择已解压目录" }));
    expect(screen.getByRole("textbox", { name: "已解压组件的位置" })).toHaveValue("/components/codebase-memory");
    expect(screen.queryByRole("status", { name: /错误/i })).toBeNull();
    expect(native.invokeNative).not.toHaveBeenCalledWith("set_codebase_memory_package_directory", expect.anything());
  });

  it("requires fresh status when changing a component directory cannot restore old settings", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "set_codebase_memory_package_directory") return { ok: false, state: "recovery_required" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();
    await user.click(await screen.findByText("位置与其他导入方式"));
    await user.click(screen.getByRole("button", { name: "保存目录" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "查看并连接编码工具" })).toBeDisabled());
    expect(screen.queryByText("已找到并验证本机组件。")).not.toBeInTheDocument();
    expect(screen.getAllByText(/原有设置未能完整恢复/).length).toBeGreaterThan(0);
  });

  it("truthfully explains an untrusted imported component instead of asking the user to import again", async () => {
    native.invokeNative.mockResolvedValue(status({
      package: {
        ...dimension("untrusted", "SHA-256 does not match the current manifest/allowlist"),
        reason_code: "upstream_untrusted",
      },
    }));
    renderPage();

    await screen.findByRole("heading", { level: 1, name: "代码图谱（实验性）" });
    const configure = screen.getByRole("button", { name: "查看并连接编码工具" });
    expect(configure).toBeDisabled();
    // The blocked reason explains what the user can do without exposing the
    // internal verification mechanism or asking them to import blindly.
    expect(screen.queryByText("请先导入并通过校验图谱组件。")).not.toBeInTheDocument();
    expect(screen.getAllByText(/当前组件版本不兼容或文件不完整/).length).toBeGreaterThanOrEqual(1);
    const packageRow = screen.getByRole("region", { name: "图谱组件" });
    expect(within(packageRow as HTMLElement).getByText("组件校验未通过")).toBeVisible();
    expect(within(packageRow as HTMLElement).getByText(/当前组件版本不兼容或文件不完整/)).toBeVisible();
  });

  it("truthfully explains a missing launcher when the pinned upstream is verified instead of asking the user to import again", async () => {
    native.invokeNative.mockResolvedValue(status({
      package: {
        ...dimension("unavailable", "dtoken-codebase-memory-launcher was not found or did not satisfy identity verification"),
        reason_code: "launcher_unavailable",
      },
    }));
    renderPage();

    await screen.findByRole("heading", { level: 1, name: "代码图谱（实验性）" });
    const configure = screen.getByRole("button", { name: "查看并连接编码工具" });
    expect(configure).toBeDisabled();
    // The upstream is verified; the real blocker is the missing launcher, so
    // the page must not claim the component was never imported.
    expect(screen.queryByText("请先导入并通过校验图谱组件。")).not.toBeInTheDocument();
    expect(screen.getAllByText(/受限 launcher 未找到/).length).toBeGreaterThanOrEqual(1);
    const packageRow = screen.getByRole("region", { name: "图谱组件" });
    expect(within(packageRow as HTMLElement).getByText("不可用")).toBeVisible();
  });

  it("fails closed when native status and project approval are unavailable", async () => {
    native.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    renderPage();

    expect(await screen.findByText("浏览器预览不读取或修改本机代码图谱状态。")).toBeVisible();
    expect(screen.getByRole("button", { name: "查看并连接编码工具" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "建立索引" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "打开图谱" })).not.toBeInTheDocument();
    expect(native.invokeNative).toHaveBeenCalledTimes(2);
    expect(native.invokeNative).toHaveBeenCalledWith("get_codebase_memory_status");
    expect(native.invokeNative).toHaveBeenCalledWith("get_hybrid_agent_integrations");
  });

  it("lets users exclude tools, keeps that choice on rescan, and previews only selected tools", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "preview_codebase_memory_integration") {
        const agentId = (args?.request as { agent_id: string }).agent_id;
        return { ok: true, reviewed_digest: `review-${agentId}`, diff: `+ ${agentId}`, draft: { agent_id: agentId, scope: "project", project_root: "/approved/project", surface: "mcp" } };
      }
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();
    const codex = await screen.findByRole("checkbox", { name: "连接 Codex" });
    expect(codex).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "连接 Cursor" })).toBeDisabled();
    await user.click(codex);
    await user.click(screen.getByRole("button", { name: "重新扫描" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "查看并连接编码工具" })).toBeEnabled());
    expect(screen.getByRole("checkbox", { name: "连接 Codex" })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "查看并连接编码工具" }));
    await screen.findByRole("button", { name: "确认并连接" });
    const ids = native.invokeNative.mock.calls.filter(([command]) => command === "preview_codebase_memory_integration")
      .map(([, args]) => (args as { request: { agent_id: string } }).request.agent_id);
    expect(ids).toEqual(["opencode"]);
  });

  it("renders per-Agent progress before native work and avoids concurrent preview storms", async () => {
    const user = userEvent.setup();
    const first = deferred<Record<string, unknown>>();
    const second = deferred<Record<string, unknown>>();
    let previewCalls = 0;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "preview_codebase_memory_integration") {
        previewCalls += 1;
        return previewCalls === 1 ? first.promise : second.promise;
      }
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "查看并连接编码工具" }));
    expect(await screen.findByText("正在为 OpenCode 准备配置预览（1/2）…")).toBeVisible();
    expect(previewCalls).toBe(1);
    expect(screen.getByRole("button", { name: "查看并连接编码工具" })).toBeDisabled();

    await act(async () => first.resolve({
      ok: true,
      reviewed_digest: "review-opencode",
      diff: "+ opencode",
      draft: { agent_id: "opencode", scope: "project", project_root: "/approved/project", surface: "mcp" },
    }));
    expect(await screen.findByText("正在为 Codex 准备配置预览（2/2）…")).toBeVisible();
    expect(previewCalls).toBe(2);

    await act(async () => second.resolve({
      ok: true,
      reviewed_digest: "review-codex",
      diff: "+ codex",
      draft: { agent_id: "codex", scope: "project", project_root: "/approved/project", surface: "mcp" },
    }));
    expect((await screen.findAllByText("变更已准备。查看内容后确认继续。")).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("+ opencode")).toBeVisible();
    expect(screen.getByText("+ codex")).toBeVisible();
  });

  it("scans, applies, verifies, and reversely rolls back a multi-Agent MCP batch", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "preview_codebase_memory_integration") {
        const agentId = (args?.request as { agent_id: string }).agent_id;
        return {
          ok: true,
          summary: `Configure ${agentId}`,
          reviewed_digest: `review-${agentId}`,
          diff: `+ ${agentId} codebase-memory MCP managed entry`,
          backup_summary: "A rollback point will be created",
          draft: { agent_id: agentId, scope: "project", project_root: "/approved/project", surface: "mcp" },
        };
      }
      if (command === "apply_codebase_memory_integration") {
        const agentId = (args?.request as { draft: { agent_id: string } }).draft.agent_id;
        return { ok: true, state: "applied", transaction_id: `tx-${agentId}`, rollback_id: `rb-${agentId}` };
      }
      if (command === "verify_codebase_memory_integration") {
        return { ok: true, state: "verified", verified: true, summary: "Read-back verified" };
      }
      if (command === "rollback_codebase_memory_integration") {
        return { ok: true, state: "rolled_back", summary: "Exact rollback verified" };
      }
      throw new Error(`unexpected command ${command} ${JSON.stringify(args)}`);
    });
    renderPage();

    expect(await screen.findByText("Cursor")).toBeVisible();
    expect(screen.getByText("暂不能连接")).toBeVisible();
    await user.click(await screen.findByRole("button", { name: "查看并连接编码工具" }));
    expect(await screen.findByText("+ opencode codebase-memory MCP managed entry")).toBeVisible();
    expect(screen.getByText("+ codex codebase-memory MCP managed entry")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "确认并连接" }));

    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("verify_codebase_memory_integration", { request: { transaction_id: "tx-opencode" } });
      expect(native.invokeNative).toHaveBeenCalledWith("verify_codebase_memory_integration", { request: { transaction_id: "tx-codex" } });
    });
    await user.click(await screen.findByRole("button", { name: "批量回退本次配置" }));
    await user.click(screen.getByRole("button", { name: "确认回退" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("rollback_codebase_memory_integration", { request: { transaction_id: "tx-opencode" } }));
    const rollbackCalls = native.invokeNative.mock.calls.filter(([command]) => command === "rollback_codebase_memory_integration");
    expect(rollbackCalls.map(([, args]) => ((args as { request: { transaction_id: string } }).request.transaction_id))).toEqual(["tx-codex", "tx-opencode"]);
  });

  it.each(["tx-codex", "tx-opencode"])("restores only %s when verification fails and keeps other tools connected", async (failedId) => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "preview_codebase_memory_integration") {
        const agentId = (args?.request as { agent_id: string }).agent_id;
        return { ok: true, reviewed_digest: `review-${agentId}`, diff: `+ ${agentId}`, draft: { agent_id: agentId, scope: "project", project_root: "/approved/project", surface: "mcp" } };
      }
      if (command === "apply_codebase_memory_integration") {
        const agentId = (args?.request as { draft: { agent_id: string } }).draft.agent_id;
        return { ok: true, transaction_id: `tx-${agentId}`, rollback_id: `rb-${agentId}` };
      }
      if (command === "verify_codebase_memory_integration") {
        const transactionId = (args?.request as { transaction_id: string }).transaction_id;
        return transactionId === failedId
          ? { ok: false, state: "failed", error: "semantic mismatch" }
          : { ok: true, state: "verified", verified: true };
      }
      if (command === "rollback_codebase_memory_integration") return { ok: true, state: "rolled_back" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "查看并连接编码工具" }));
    await user.click(await screen.findByRole("button", { name: "确认并连接" }));
    await waitFor(() => expect(screen.getAllByText(/已完成 1\/2 个工具连接/).length).toBeGreaterThanOrEqual(1));
    const rollbackIds = native.invokeNative.mock.calls
      .filter(([command]) => command === "rollback_codebase_memory_integration")
      .map(([, args]) => ((args as { request: { transaction_id: string } }).request.transaction_id));
    expect(rollbackIds).toEqual([failedId]);
    expect(native.invokeNative.mock.calls.filter(([command]) => command === "apply_codebase_memory_integration")).toHaveLength(2);
  });

  it("retries rollback when a failed apply returns a recovery transaction", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "preview_codebase_memory_integration") {
        const agentId = (args?.request as { agent_id: string }).agent_id;
        return { ok: true, reviewed_digest: `review-${agentId}`, diff: `+ ${agentId}`, draft: { agent_id: agentId, scope: "project", project_root: "/approved/project", surface: "mcp" } };
      }
      if (command === "apply_codebase_memory_integration") {
        return {
          ok: false,
          state: "recovery_required",
          transaction_id: "tx-opencode-recovery",
          rollback_id: "rb-opencode-recovery",
          error: "state persistence failed",
        };
      }
      if (command === "rollback_codebase_memory_integration") return { ok: true, state: "rolled_back" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "查看并连接编码工具" }));
    await user.click(await screen.findByRole("button", { name: "确认并连接" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith(
      "rollback_codebase_memory_integration",
      { request: { transaction_id: "tx-opencode-recovery" } },
    ));
    await waitFor(() => expect(screen.getAllByText(/已完成 0\/2 个工具连接/).length).toBeGreaterThanOrEqual(1));
  });

  it("starts and opens the owned graph in one click without a caller-supplied URL", async () => {
    const user = userEvent.setup();
    let statusCalls = 0;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "get_codebase_memory_status") {
        statusCalls += 1;
        return status({
          ui_session: ownedUiDimension(statusCalls === 1 ? "stopped" : "ready", "d-token-owned graph host"),
          index: dimension("stopped", "No current index"),
        });
      }
      if (command === "start_codebase_memory_ui") return { ok: true, summary: "UI started" };
      if (command === "start_codebase_memory_index") return { ok: true, summary: "Index started" };
      if (command === "open_codebase_memory_graph") return { ok: true, summary: "Graph opened" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "启动并打开图谱" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("start_codebase_memory_ui", {
      request: { project_root: "/approved/project" },
    }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("open_codebase_memory_graph"));
    const automaticOpenCall = native.invokeNative.mock.calls.find(([command]) => command === "open_codebase_memory_graph");
    expect(automaticOpenCall).toHaveLength(1);
    await waitFor(() => expect(screen.getByRole("button", { name: "打开图谱" })).toBeEnabled());

    await user.click(screen.getByRole("button", { name: "建立索引" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("start_codebase_memory_index", {
      request: { project_root: "/approved/project", mode: "explicit" },
    }));
  });

  it("cancels an in-progress index and requires confirmation before stopping the UI host", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "get_codebase_memory_status") return status({
        ui_session: ownedUiDimension("ready", "d-token-owned graph host"),
        index: dimension("busy", "Indexing approved project"),
      });
      if (command === "cancel_codebase_memory_index") return { ok: true, summary: "Cancellation requested" };
      if (command === "stop_codebase_memory_ui") return { ok: true, summary: "UI stopped" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "取消索引" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("cancel_codebase_memory_index"));
    await user.click(screen.getByRole("button", { name: "停止页面服务" }));
    expect(native.invokeNative).not.toHaveBeenCalledWith("stop_codebase_memory_ui");
    await user.click(screen.getByRole("button", { name: "确认停止" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("stop_codebase_memory_ui"));
  });

  it("keeps destructive dialogs open after a failed action so the operator can read the reason and retry", async () => {
    const user = userEvent.setup();
    let deleteAttempts = 0;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "get_codebase_memory_status") return status({
        ui_session: ownedUiDimension("stopped", "d-token-owned graph host"),
        index: dimension("stopped", "No current index"),
      });
      if (command === "delete_codebase_memory_index") {
        deleteAttempts += 1;
        return deleteAttempts === 1
          ? { ok: false, error: "index is still in use" }
          : { ok: true, summary: "Index deleted" };
      }
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "删除索引" }));
    const dialog = await screen.findByRole("alertdialog", { name: "删除此项目的本机索引？" });
    await user.click(within(dialog).getByRole("button", { name: "确认删除索引" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("操作未完成");
    expect(within(dialog).getByRole("button", { name: "确认删除索引" })).toBeEnabled();

    await user.click(within(dialog).getByRole("button", { name: "确认删除索引" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog", { name: "删除此项目的本机索引？" })).toBeNull());
  });

  it("keeps the graph-host stop dialog open when the owned UI cannot be stopped", async () => {
    const user = userEvent.setup();
    let stopAttempts = 0;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      if (command === "get_codebase_memory_status") return status({
        ui_session: ownedUiDimension("ready", "d-token-owned graph host"),
        index: dimension("stopped", "No current index"),
      });
      if (command === "stop_codebase_memory_ui") {
        stopAttempts += 1;
        return stopAttempts === 1 ? { ok: false, error: "runtime identity changed" } : { ok: true, summary: "UI stopped" };
      }
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "停止页面服务" }));
    const dialog = await screen.findByRole("alertdialog", { name: "停止图谱页面服务" });
    await user.click(within(dialog).getByRole("button", { name: "确认停止" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("操作未完成");
    await user.click(within(dialog).getByRole("button", { name: "确认停止" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog", { name: "停止图谱页面服务" })).toBeNull());
  });

  it("reports a completed manual Agent scan while keeping the initial scan quiet", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_codebase_memory_status") return status();
      if (command === "get_hybrid_agent_integrations") return agentDiscovery;
      throw new Error(`unexpected command ${command}`);
    });
    const { container } = renderPage();
    await screen.findByRole("button", { name: "重新扫描" });
    expect(container.textContent).not.toContain("本机 Agent 扫描完成");

    await user.click(screen.getByRole("button", { name: "重新扫描" }));
    expect(await screen.findByText("找到 2 个可接入的编码工具"))
      .toBeVisible();
  });
});
