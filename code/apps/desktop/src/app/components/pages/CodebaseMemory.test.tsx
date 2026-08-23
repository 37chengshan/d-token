import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationProvider } from "../../lib/notifications";
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
    expect(screen.getByText("实验性", { exact: true })).toBeVisible();
    expect(screen.getByText("正在读取代码图谱状态…")).toBeVisible();
    expect(screen.getByRole("button", { name: "刷新" })).toBeDisabled();

    resolveStatus?.(status());
    expect(await screen.findByRole("heading", { level: 3, name: "图谱组件" })).toBeVisible();
  });

  it("renders five independent evidence dimensions without collapsing them to green", async () => {
    native.invokeNative.mockResolvedValue(status());
    renderPage();

    await screen.findByRole("heading", { level: 1, name: "代码图谱（实验性）" });
    const expected = [
      ["图谱组件", "已验证"],
      ["Agent 连接", "未配置"],
      ["Agent 使用情况", "已验证"],
      ["图谱页面", "失败"],
      ["项目索引", "部分可用"],
    ];
    for (const [heading, stateLabel] of expected) {
      const row = screen.getByRole("heading", { level: 3, name: heading }).closest("[data-flat-row]");
      expect(row).toBeTruthy();
      expect(within(row as HTMLElement).getByText(stateLabel)).toBeVisible();
    }
    expect(screen.getByText(/连接完成不代表图谱页面或索引已经可用/)).toBeVisible();
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

    // The supported release is pinned by the supply-chain manifest/allowlist,
    // not by a hard-coded version number in the UI.
    expect(await screen.findByRole("heading", { level: 2, name: "导入当前受支持的图谱组件" })).toBeVisible();
    expect(screen.getByText(/按当前 manifest\/allowlist 校验来源、平台、架构和 SHA-256/)).toBeVisible();
    expect(screen.queryByText(/v0\.9\.1/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "打开当前官方下载页" }));
    expect(native.invokeNative).toHaveBeenCalledWith("open_codebase_memory_download_page");
    const directory = screen.getByRole("textbox", { name: "已解压组件的位置" });
    await user.clear(directory);
    await user.type(directory, "/custom/code-graph");
    await user.click(screen.getByRole("button", { name: "保存目录" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("set_codebase_memory_package_directory", {
      request: { directory: "/custom/code-graph" },
    }));
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
    const configure = screen.getByRole("button", { name: "查看并连接 Agent" });
    expect(configure).toBeDisabled();
    // The blocked reason must be truthful about the failed manifest/allowlist
    // check, not ask the user to import again when a component was already detected.
    expect(screen.queryByText("请先导入并通过校验图谱组件。")).not.toBeInTheDocument();
    expect(screen.getAllByText(/未通过当前 manifest\/allowlist 校验/).length).toBeGreaterThanOrEqual(1);
    const packageRow = screen.getByRole("heading", { level: 3, name: "图谱组件" }).closest("[data-flat-row]");
    expect(within(packageRow as HTMLElement).getByText("未受信任")).toBeVisible();
    expect(within(packageRow as HTMLElement).getByText(/未通过当前 manifest\/allowlist 校验/)).toBeVisible();
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
    const configure = screen.getByRole("button", { name: "查看并连接 Agent" });
    expect(configure).toBeDisabled();
    // The upstream is verified; the real blocker is the missing launcher, so
    // the page must not claim the component was never imported.
    expect(screen.queryByText("请先导入并通过校验图谱组件。")).not.toBeInTheDocument();
    expect(screen.getAllByText(/受限 launcher 未找到/).length).toBeGreaterThanOrEqual(1);
    const packageRow = screen.getByRole("heading", { level: 3, name: "图谱组件" }).closest("[data-flat-row]");
    expect(within(packageRow as HTMLElement).getByText("不可用")).toBeVisible();
  });

  it("fails closed when native status and project approval are unavailable", async () => {
    native.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    renderPage();

    expect(await screen.findByText("浏览器预览不读取或修改本机代码图谱状态。")).toBeVisible();
    expect(screen.getByRole("button", { name: "查看并连接 Agent" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "建立索引" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "打开图谱" })).not.toBeInTheDocument();
    expect(native.invokeNative).toHaveBeenCalledTimes(2);
    expect(native.invokeNative).toHaveBeenCalledWith("get_codebase_memory_status");
    expect(native.invokeNative).toHaveBeenCalledWith("get_hybrid_agent_integrations");
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

    await user.click(await screen.findByRole("button", { name: "查看并连接 Agent" }));
    expect(await screen.findByText("正在为 OpenCode 准备配置预览（1/2）…")).toBeVisible();
    expect(previewCalls).toBe(1);
    expect(screen.getByRole("button", { name: "查看并连接 Agent" })).toBeDisabled();

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
    await user.click(await screen.findByRole("button", { name: "查看并连接 Agent" }));
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

  it("fails closed and rolls back earlier Agent writes when a batch target fails verification", async () => {
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
        return transactionId === "tx-codex"
          ? { ok: false, state: "failed", error: "semantic mismatch" }
          : { ok: true, state: "verified", verified: true };
      }
      if (command === "rollback_codebase_memory_integration") return { ok: true, state: "rolled_back" };
      throw new Error(`unexpected command ${command}`);
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "查看并连接 Agent" }));
    await user.click(await screen.findByRole("button", { name: "确认并连接" }));
    await waitFor(() => expect(screen.getAllByText(/本批次已写入的配置已回退/).length).toBeGreaterThanOrEqual(1));
    const rollbackIds = native.invokeNative.mock.calls
      .filter(([command]) => command === "rollback_codebase_memory_integration")
      .map(([, args]) => ((args as { request: { transaction_id: string } }).request.transaction_id));
    expect(rollbackIds).toEqual(["tx-codex", "tx-opencode"]);
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

    await user.click(await screen.findByRole("button", { name: "查看并连接 Agent" }));
    await user.click(await screen.findByRole("button", { name: "确认并连接" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith(
      "rollback_codebase_memory_integration",
      { request: { transaction_id: "tx-opencode-recovery" } },
    ));
    expect(screen.getAllByText(/本批次已写入的配置已回退/).length).toBeGreaterThanOrEqual(1);
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
});
