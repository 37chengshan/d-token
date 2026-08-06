// d-token 候选目录发现器：从常用项目目录、Git 仓库、Agent 会话路径发现候选。
// 关联计划：release-plan-2026-07/phase-03 §3 候选来源。
import { AGENT_REGISTRY } from "../agents/registry";

export type DiscoverySource = "git-repo" | "agent-session" | "common-project" | "editor-workspace" | "dtoken-history";

export type DiscoveryCandidate = {
  path: string;
  source: DiscoverySource;
  agentId?: string;
};

/** 常用项目根目录候选（Windows/macOS）。 */
export const COMMON_PROJECT_ROOTS: string[] = [
  "~/projects",
  "~/code",
  "~/work",
  "~/Developer",
  "~/Documents/projects",
  "D:/work",
  "D:/projects",
  "D:/code",
  "C:/Users/%USER%/projects",
];

/** 由 Agent descriptor 收集会话路径提示。 */
export function agentSessionHints(): { agentId: string; paths: string[] }[] {
  return AGENT_REGISTRY.map((agent) => ({ agentId: agent.id, paths: agent.sessionPathHints }));
}

/** 合并所有候选来源（去重），供轻量索引使用。 */
export function mergeCandidates(lists: DiscoveryCandidate[][]): DiscoveryCandidate[] {
  const seen = new Set<string>();
  const out: DiscoveryCandidate[] = [];
  for (const list of lists) {
    for (const c of list) {
      const key = c.path.toLowerCase();
      if (!seen.has(key)) {
        seen.add(key);
        out.push(c);
      }
    }
  }
  return out;
}

/** 从常用项目根构造候选列表（实际枚举由真实文件系统层完成）。 */
export function fromCommonRoots(): DiscoveryCandidate[] {
  return COMMON_PROJECT_ROOTS.map((path) => ({ path, source: "common-project" as const }));
}
