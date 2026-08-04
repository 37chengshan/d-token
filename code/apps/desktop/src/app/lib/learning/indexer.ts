// d-token 轻量索引器：只读取目录名、修改时间、文件数量区间、.git、项目标记、Agent 文档、语言指纹。
// 不得读取完整源代码、完整会话和敏感文件。缓存按修改时间增量刷新。
// 关联计划：release-plan-2026-07/phase-03 §2 轻量索引、§5 索引缓存。
import type { LightIndex, LearningIndexRecord } from "./types";
import { isSkipDir, isSensitivePath } from "./types";

export type DirStat = {
  name: string;
  lastModifiedMs: number;
  fileCount: number;
  entryNames: string[];
};

export const PROJECT_MARKERS = [
  "package.json",
  "Cargo.toml",
  "pyproject.toml",
  "go.mod",
  "pom.xml",
  "build.gradle",
  "tsconfig.json",
  "vite.config.ts",
  "tauri.conf.json",
  ".git",
];

export const AGENT_DOCS = ["CLAUDE.md", "AGENTS.md", "GEMINI.md", ".cursorrules", ".clinerules"];

export const LANG_FINGERPRINTS: { file: string; lang: string }[] = [
  { file: "package.json", lang: "TypeScript/JS" },
  { file: "Cargo.toml", lang: "Rust" },
  { file: "pyproject.toml", lang: "Python" },
  { file: "go.mod", lang: "Go" },
  { file: "pom.xml", lang: "Java" },
];

function fileCountBucket(n: number): LightIndex["fileCountBucket"] {
  if (n < 20) return "tiny";
  if (n < 200) return "small";
  if (n < 2000) return "medium";
  if (n < 20000) return "large";
  return "huge";
}

/** 由目录条目构造轻量索引（纯函数，不执行 IO）。 */
export function buildLightIndex(stat: DirStat): LightIndex {
  const names = stat.entryNames.filter((n) => !isSkipDir(n) && !isSensitivePath(n));
  const hasGit = names.includes(".git");
  const projectMarkers = PROJECT_MARKERS.filter((m) => names.includes(m));
  const hasAgentDoc = AGENT_DOCS.some((d) => names.includes(d));
  const languageFingerprints = LANG_FINGERPRINTS.filter((f) => names.includes(f.file)).map((f) => f.lang);
  return {
    folderName: stat.name,
    lastModifiedMs: stat.lastModifiedMs,
    fileCountBucket: fileCountBucket(stat.fileCount),
    hasGit,
    projectMarkers,
    hasAgentDoc,
    hasAgentSession: false,
    languageFingerprints,
  };
}

/** 简易目录指纹：用于缓存增量刷新判断。 */
export function folderHash(stat: DirStat): string {
  const sig = `${stat.name}|${stat.lastModifiedMs}|${stat.fileCount}`;
  let h = 0;
  for (let i = 0; i < sig.length; i++) {
    h = (h * 31 + sig.charCodeAt(i)) | 0;
  }
  return `h${(h >>> 0).toString(16)}`;
}

/** 增量刷新：若指纹与缓存一致则跳过，否则重建。 */
export function maybeRefreshCache(
  cache: LearningIndexRecord | undefined,
  stat: DirStat,
  nowMs: number,
  buildScore: (idx: LightIndex) => LearningIndexRecord["score"],
  explain: (score: LearningIndexRecord["score"]) => string[],
): LearningIndexRecord {
  const hash = folderHash(stat);
  if (cache && cache.folderHash === hash) {
    return { ...cache, lastIndexedAt: new Date(nowMs).toISOString() };
  }
  const lightIndex = buildLightIndex(stat);
  const score = buildScore(lightIndex);
  return {
    folderPath: stat.name,
    folderHash: hash,
    lastIndexedAt: new Date(nowMs).toISOString(),
    lightIndex,
    score,
    explanations: explain(score),
    riskFlags: ["none"],
    deepScanStatus: "not-started",
  };
}
