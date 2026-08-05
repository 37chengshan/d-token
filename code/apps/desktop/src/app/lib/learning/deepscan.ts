// d-token 深度扫描器：只对高价值/用户选择目录执行，读取 README/docs/脚本/配置摘要。
// 必须跳过 .env、密钥、认证文件、cookie、token、大型二进制、依赖目录、构建产物。
// 关联计划：release-plan-2026-07/phase-03 §2 深度扫描、§6 学习结果结构。
import type { LearningContext, SourceSummary } from "./types";
import { isSensitivePath, isSkipDir } from "./types";

export type DeepScanFile = {
  path: string;
  kind: "readme" | "docs" | "script" | "config" | "agent-doc" | "other";
  sizeBytes: number;
  /** 脱敏后的摘要，不含敏感原文。 */
  excerpt: string;
};

export type DeepScanInput = {
  projectName: string;
  projectPath: string;
  files: DeepScanFile[];
};

/** 过滤敏感/跳过文件。 */
export function filterSafeFiles(files: DeepScanFile[]): DeepScanFile[] {
  return files.filter((f) => {
    const base = f.path.split(/[/\\]/).pop() ?? f.path;
    if (isSkipDir(base)) return false;
    if (isSensitivePath(f.path)) return false;
    if (f.sizeBytes > 512 * 1024) return false; // 跳过大型二进制
    return true;
  });
}

export function buildSourceSummary(files: DeepScanFile[]): SourceSummary {
  return {
    readme: files.some((f) => f.kind === "readme"),
    docs: files.filter((f) => f.kind === "docs").length,
    scripts: files.filter((f) => f.kind === "script").length,
    configs: files.filter((f) => f.kind === "config").length,
    agentDocs: files.filter((f) => f.kind === "agent-doc").map((f) => f.path),
    recentChanges: 0,
  };
}

/** 由深度扫描文件构造 LearningContext（面向 Agent 使用，告诉 Agent 如何工作、不要碰什么）。 */
export function buildLearningContext(input: DeepScanInput): LearningContext {
  const safe = filterSafeFiles(input.files);
  const summary = buildSourceSummary(safe);
  const techStack: string[] = [];
  const commonCommands: string[] = [];
  for (const f of safe) {
    if (f.kind === "config") {
      if (/package\.json/i.test(f.path) && !techStack.includes("Node")) techStack.push("Node");
      if (/Cargo\.toml/i.test(f.path) && !techStack.includes("Rust")) techStack.push("Rust");
      if (/pyproject/i.test(f.path) && !techStack.includes("Python")) techStack.push("Python");
    }
    if (f.kind === "script") {
      const cmd = f.excerpt.split("\n").find((l) => /npm|cargo|python|go|pnpm|yarn/i.test(l));
      if (cmd && commonCommands.length < 8) commonCommands.push(cmd.trim().slice(0, 80));
    }
  }
  return {
    projectName: input.projectName,
    projectPath: input.projectPath,
    summary: `${input.projectName} 包含 ${summary.scripts} 个脚本、${summary.configs} 个配置、${summary.docs} 个文档。`,
    techStack,
    architectureNotes: [],
    importantFiles: safe.filter((f) => f.kind === "config" || f.kind === "readme").map((f) => f.path).slice(0, 12),
    commonCommands,
    conventions: ["遵循已有目录结构", "不要修改依赖目录与构建产物"],
    risks: ["请勿读取 .env、密钥、认证文件", "请勿上传完整 prompt 或源代码"],
    agentInstructions: [
      "使用 d-token 本地服务转发请求，避免直接暴露上游凭据。",
      "优先参考 README 与已有 Agent 文档理解项目。",
    ],
    dtokenRecommendations: [
      "为该项目启用 balanced 压缩策略。",
      "补充 CLAUDE.md / AGENTS.md 的项目说明，可由智能学习受控写入。",
    ],
    openQuestions: summary.readme ? [] : ["缺少 README，建议补充项目说明。"],
    generatedAt: new Date().toISOString(),
    sourceSummary: summary,
  };
}
