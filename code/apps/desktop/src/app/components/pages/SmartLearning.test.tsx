import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({ invokeNative: native.invokeNative }));
vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => ({ payload: null, status: "unavailable", refresh: vi.fn() }),
  useRuntimeDashboardDetail: () => ({ payload: null, status: "unavailable", refresh: vi.fn() }),
}));

import { learningInputFromOwnedRuntime, learningInputFromProjectMetadata, SmartLearning } from "./SmartLearning";

beforeEach(() => {
  native.invokeNative.mockReset();
  native.invokeNative.mockResolvedValue({ state: "missing", payload: {} });
});

afterEach(cleanup);

describe("SmartLearning", () => {
  it("requires a real discovered project before generating a recommendation", async () => {
    render(<SmartLearning />);
    expect(screen.getByText(/本地枚举受支持 Agent 的会话记录/)).toBeVisible();
    expect(screen.getByRole("button", { name: "理解选中项目" })).toBeDisabled();
  });

  it("starts native session discovery without a project path or runtime evidence", async () => {
    render(<SmartLearning />);

    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("start_learning_discovery", {
      request: { refresh: false, project_roots: [], skip_paths: [] },
    }));
  });

  it("does not present a session root as a project", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "start_learning_discovery") return {
        ok: true,
        job_id: "learning-1",
        state: "complete",
        phase: "complete",
        source: "d-token-learning-discovery",
        freshness: "unix:1",
        cache_state: "fresh",
        progress: { scanned_agents: 6, total_agents: 6, session_records: 13, parsed_projects: 0, candidate_count: 1, skipped: 0, risks: 0, current_stage: "complete", cancellable: false },
        sources: [{ id: "claude-code", label: "Claude Code", state: "scanned", roots_found: 1, session_count: 13, project_count: 0, skipped_count: 0, detail: "metadata only" }],
        candidates: [{ id: "claude-root", path: "/Users/test/.claude/projects", agent: "Claude Code", kind: "session-root", mtime: 1, activity: "observed", confidence: "directory-and-metadata-verified", value: "high", reasons: [], freshness: "unix:1", session_count: 13, project_count: 0, skipped_count: 0, write_eligible: false }],
        writes_performed: false,
        privacy: "metadata only",
      };
      if (command === "get_learning_delegation_capabilities") return { agents: [] };
      return { state: "missing", payload: {} };
    });

    render(<SmartLearning />);

    expect(await screen.findByText(/尚未从可识别会话中关联到真实项目/)).toBeVisible();
    expect(screen.queryByText("/Users/test/.claude/projects")).not.toBeInTheDocument();
    expect(screen.getByText(/13 条已枚举 · 0 条已关联 · 13 条未关联/)).toBeVisible();
    expect(screen.getByRole("button", { name: "理解选中项目" })).toBeDisabled();
  });

  it("renders partial discovery and unavailable persistence as terminal states", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "start_learning_discovery") return discoverySnapshot({
        state: "partial",
        phase: "partial",
        cache_state: "unavailable",
        progress: { ...discoverySnapshot().progress, current_stage: "partial", cancellable: false },
      });
      if (command === "get_learning_delegation_capabilities") return { agents: [] };
      return { state: "missing", payload: {} };
    });

    render(<SmartLearning />);

    expect(await screen.findByText("本机项目发现部分完成")).toBeVisible();
    expect(screen.getByText(/部分完成 · 缓存不可用，结果未持久化/)).toBeVisible();
    expect(screen.queryByText("等待本机发现")).not.toBeInTheDocument();
    expect(screen.queryByText("正在建立缓存")).not.toBeInTheDocument();
  });

  it("keeps large project results bounded to one page of radio rows", async () => {
    const candidates = Array.from({ length: 55 }, (_, index) => projectCandidate(index));
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "start_learning_discovery") return discoverySnapshot({
        candidates,
        progress: { ...discoverySnapshot().progress, parsed_projects: candidates.length, candidate_count: candidates.length },
      });
      if (command === "get_learning_delegation_capabilities") return { agents: [] };
      return { state: "missing", payload: {} };
    });

    render(<SmartLearning />);

    await screen.findByText("第 1 / 3 页 · 共 55 个项目");
    expect(screen.getAllByRole("radio")).toHaveLength(20);
    expect(screen.getByText("/Users/test/work/project-0")).toBeVisible();
    expect(screen.queryByText("/Users/test/work/project-20")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "下一页" }));

    expect(await screen.findByText("第 2 / 3 页 · 共 55 个项目")).toBeVisible();
    expect(screen.getAllByRole("radio")).toHaveLength(20);
    expect(screen.getByText("/Users/test/work/project-20")).toBeVisible();
    expect(screen.queryByText("/Users/test/work/project-0")).not.toBeInTheDocument();
  });

  it("auto-selects a linked project and generates a metadata-only recommendation", async () => {
    let resolvePreview: (value: Record<string, any>) => void = () => {};
    const previewResult = new Promise<Record<string, any>>((resolve) => { resolvePreview = resolve; });
    let previewAttempts = 0;
    native.invokeNative.mockImplementation(async (command: string, args?: Record<string, any>) => {
      if (command === "start_learning_discovery") return {
        ok: true,
        job_id: "learning-project",
        state: "complete",
        phase: "complete",
        source: "d-token-learning-discovery",
        freshness: "unix:1",
        cache_state: "fresh",
        progress: { scanned_agents: 6, total_agents: 6, session_records: 4, parsed_projects: 1, candidate_count: 2, skipped: 0, risks: 0, current_stage: "complete", cancellable: false },
        sources: [
          { id: "opencode", label: "OpenCode", state: "scanned", roots_found: 1, session_count: 4, project_count: 1, skipped_count: 0, detail: "metadata only" },
          { id: "command-code", label: "CommandCode", state: "unsupported", roots_found: 1, session_count: 63, project_count: 0, skipped_count: 63, detail: "unsupported" },
        ],
        candidates: [
          { id: "opencode-root", path: "/Users/test/.local/share/opencode", agent: "OpenCode", kind: "session-root", mtime: 1, activity: "observed", confidence: "directory-and-metadata-verified", value: "high", reasons: [], freshness: "unix:1", session_count: 4, project_count: 1, skipped_count: 0, write_eligible: false },
          { id: "project-1", path: "/Users/test/work/d-token", agent: "OpenCode", kind: "project", mtime: 1, activity: "active", confidence: "session-project-linked", value: "high", reasons: [], freshness: "unix:1", session_count: 4, project_count: 1, skipped_count: 0, write_eligible: true },
        ],
        writes_performed: false,
        privacy: "metadata only",
      };
      if (command === "recommend_project_policy") {
        expect(args?.request).toMatchObject({ project_id: "d-token", evidence_source: "local-session-project-metadata-only" });
        return { recommendation: { recommended_level: "balanced", reasons: [] } };
      }
      if (command === "analyze_learning_project") {
        expect(args?.request).toEqual({ project_path: "/Users/test/work/d-token" });
        return {
          ok: true,
          state: "complete",
          source: "dtoken-learning-core",
          freshness: "unix:1",
          index: {
            project_name: "d-token",
            project_path: "/Users/test/work/d-token",
            last_modified_unix: 1,
            file_count: 120,
            file_count_bucket: "small",
            has_git: true,
            project_markers: ["Cargo.toml"],
            agent_docs: [],
            language_fingerprints: ["Rust"],
            top_level_directories: ["crates"],
            score: 84,
            ranking_reasons: ["git-project"],
            risk_flags: [],
            source_summary: { readme: true, docs: 1, scripts: 0, configs: 1, agent_docs: [], sensitive_entries_skipped: 0, unreadable_entries: 0, truncated: false },
          },
          context: {
            project_name: "d-token",
            project_path: "/Users/test/work/d-token",
            summary: "d-token is a local Rust project.",
            tech_stack: ["Rust"],
            architecture_notes: ["Top-level `crates` directory is present"],
            important_files: ["Cargo.toml"],
            common_commands: ["cargo test --workspace"],
            conventions: [],
            risks: [],
            agent_instructions: [],
            dtoken_recommendations: [],
            open_questions: [],
            generated_at: "unix:1",
            source_summary: { readme: true, docs: 1, scripts: 0, configs: 1, agent_docs: [], sensitive_entries_skipped: 0, unreadable_entries: 0, truncated: false },
          },
          writes_performed: false,
          privacy: "bounded metadata",
        };
      }
      if (command === "preview_learning_managed_block") {
        previewAttempts += 1;
        if (previewAttempts > 1) throw new Error("preview bridge unavailable");
        return previewResult;
      }
      if (command === "apply_learning_managed_block") return {
        ok: true,
        state: "verified",
        source: "d-token-learning-control-plane",
        transaction_id: "tx-learning-1",
        rollback_id: "rollback-learning-1",
        target: "/Users/test/work/d-token/AGENTS.md",
        before_sha256: "before-hash",
        after_sha256: "after-hash",
        verified: true,
        summary: "Managed block written atomically and verified",
      };
      if (command === "save_app_settings") return { ok: true, summary: "Settings saved", logs: [], tools: [] };
      if (command === "get_learning_delegation_capabilities") return { agents: [] };
      return { state: "missing", payload: {} };
    });

    render(<SmartLearning />);

    expect(await screen.findByText("/Users/test/work/d-token")).toBeVisible();
    expect(screen.queryByText("/Users/test/.local/share/opencode")).not.toBeInTheDocument();
    expect(screen.getByText(/发现 1 个目录，尚未解析会话格式/)).toBeVisible();
    const button = screen.getByRole("button", { name: "理解选中项目" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("recommend_project_policy", expect.any(Object)));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("analyze_learning_project", { request: { project_path: "/Users/test/work/d-token" } }));
    expect(await screen.findByText(/cargo test --workspace/)).toBeVisible();
    expect(screen.getByText("84/100")).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "保存项目理解" }));
    expect(screen.getByRole("button", { name: "正在准备写入预览" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("正在准备受控写入差异预览");
    expect(native.invokeNative).toHaveBeenCalledWith("preview_learning_managed_block", {
      request: {
        target: "/Users/test/work/d-token/AGENTS.md",
        content: expect.stringContaining("# d-token 项目理解"),
        reviewed_before_sha256: null,
        reviewed_after_sha256: null,
      },
    });

    resolvePreview({
      ok: true,
      state: "review",
      source: "d-token-learning-control-plane",
      target: "/Users/test/work/d-token/AGENTS.md",
      before_sha256: "before-hash",
      after_sha256: "after-hash",
      diff: "+<!-- d-token:learning:start -->",
    });
    expect(await screen.findByRole("alertdialog", { name: "确认写入 d-token 管理区块" })).toBeVisible();
    expect(screen.getByText("+<!-- d-token:learning:start -->")).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "确认原子写入" }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("apply_learning_managed_block", {
      request: {
        target: "/Users/test/work/d-token/AGENTS.md",
        content: expect.stringContaining("# d-token 项目理解"),
        reviewed_before_sha256: "before-hash",
        reviewed_after_sha256: "after-hash",
      },
    }));
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("save_app_settings", expect.objectContaining({ section: "smart-learning" })));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("项目理解已持久化"));

    fireEvent.click(screen.getByRole("button", { name: "生成文档草稿" }));
    expect((screen.getByRole("textbox", { name: "项目理解文档草稿" }) as HTMLTextAreaElement).value).toContain("# d-token 项目理解");
    expect(screen.getByRole("status")).toHaveTextContent("文档草稿已生成并显示在下方");

    fireEvent.click(screen.getByRole("button", { name: "预览受控写入" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请重试；如果问题仍然存在，请查看连接日志。");
  });

  it("does not fabricate candidates when native discovery has no evidence", () => {
    render(<SmartLearning />);
    expect(screen.getByText("尚未从可识别会话中关联到真实项目")).toBeVisible();
    expect(screen.getByText("尚无真实分析结果")).toBeVisible();
    expect(screen.queryByText("D:/D-TOKEN/d-token")).not.toBeInTheDocument();
  });

  it("keeps document actions available for a persisted project understanding", async () => {
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "start_learning_discovery") return discoverySnapshot({
        candidates: [projectCandidate(0)],
        progress: { ...discoverySnapshot().progress, parsed_projects: 1, candidate_count: 1 },
      });
      if (command === "get_app_settings") return {
        state: "loaded",
        payload: persistedLearningState(),
      };
      if (command === "get_learning_delegation_capabilities") return { agents: [] };
      return { state: "missing", payload: {} };
    });

    render(<SmartLearning />);

    expect(await screen.findByText("Persisted project understanding.")).toBeVisible();
    expect(screen.getByRole("button", { name: "保存项目理解" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "生成文档草稿" }));
    expect((screen.getByRole("textbox", { name: "项目理解文档草稿" }) as HTMLTextAreaElement).value).toContain("project-0");
  });

  it("uses an honest zero aggregate for an owned runtime with no requests", () => {
    const input = learningInputFromOwnedRuntime({ source: "dtoken-owned-runtime", desktop_data_source: "dtoken-local-service", request_logs: [] }, "/tmp/real-project");
    expect(input).toMatchObject({ project_id: "real-project", avg_request_tokens_7d: 0, failure_rate_7d: 0, evidence_source: "dtoken-owned-runtime-empty-or-aggregate-request-logs" });
  });

  it("creates a conservative input only for a write-eligible project candidate", () => {
    const input = learningInputFromProjectMetadata({ id: "p", path: "/tmp/Real Project", agent: "Codex", kind: "project", activity: "active", confidence: "session-project-linked", value: "high", reasons: [], freshness: "unix:1", session_count: 3, project_count: 1, skipped_count: 0, write_eligible: true });
    expect(input).toMatchObject({ project_id: "real-project", avg_request_tokens_7d: 0, evidence_source: "local-session-project-metadata-only" });
    expect(learningInputFromProjectMetadata({ id: "root", path: "/tmp/sessions", agent: "Codex", kind: "session-root", activity: "observed", confidence: "directory-and-metadata-verified", value: "high", reasons: [], freshness: "unix:1", session_count: 3, project_count: 0, skipped_count: 0, write_eligible: false })).toBeNull();
  });
});

function projectCandidate(index: number) {
  return {
    id: `project-${index}`,
    path: `/Users/test/work/project-${index}`,
    agent: index % 2 === 0 ? "Codex" : "Claude Code",
    kind: "project",
    mtime: index,
    activity: "active",
    confidence: "session-project-linked",
    value: "high",
    reasons: [],
    freshness: "unix:1",
    session_count: index + 1,
    project_count: 1,
    skipped_count: 0,
    write_eligible: true,
  };
}

function discoverySnapshot(overrides: Record<string, any> = {}) {
  return {
    ok: true,
    job_id: "learning-test",
    state: "complete",
    phase: "complete",
    source: "d-token-learning-discovery",
    freshness: "unix:1",
    cache_state: "fresh",
    progress: { scanned_agents: 7, total_agents: 7, session_records: 928, parsed_projects: 0, candidate_count: 0, skipped: 0, risks: 0, current_stage: "complete", cancellable: false },
    sources: [],
    candidates: [],
    writes_performed: false,
    privacy: "metadata only",
    ...overrides,
  };
}

function persistedLearningState() {
  return {
    project: { root: "/Users/test/work/project-0", project_id: "project-0" },
    analysis: {
      ok: true,
      state: "complete",
      source: "dtoken-learning-core",
      freshness: "unix:1",
      index: { score: 72, file_count: 12 },
      context: {
        project_name: "project-0",
        project_path: "/Users/test/work/project-0",
        summary: "Persisted project understanding.",
        tech_stack: ["Rust"],
        architecture_notes: [],
        important_files: [],
        common_commands: ["cargo test"],
        conventions: [],
        risks: [],
        agent_instructions: [],
        dtoken_recommendations: [],
        open_questions: [],
        source_summary: { sensitive_entries_skipped: 0 },
      },
      writes_performed: false,
      privacy: "bounded metadata",
    },
    recommendation: { recommended_level: "safe", reasons: [] },
    mapping: {},
    source: "dtoken-project-policy",
    privacy: "metadata-only",
  };
}
