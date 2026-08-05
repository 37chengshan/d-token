// d-token 智能学习数据模型：LearningContext、LearningIndexRecord、LightIndex、LearningScore、DelegationPackage。
// 关联计划：release-plan-2026-07/phase-03 §4 评分算法、§5 索引缓存、§6 学习结果结构、§9 委托流程。

export type LightIndex = {
  folderName: string;
  lastModifiedMs: number;
  fileCountBucket: "tiny" | "small" | "medium" | "large" | "huge";
  hasGit: boolean;
  projectMarkers: string[];
  hasAgentDoc: boolean;
  hasAgentSession: boolean;
  languageFingerprints: string[];
};

export type LearningScore = {
  total: number;
  recency: number;
  agentActivity: number;
  projectConfidence: number;
  docGap: number;
  complexity: number;
  userPreference: number;
  dtokenHistory: number;
  riskPenalty: number;
};

export type RiskFlag = "sensitive-files" | "too-large" | "permission-denied" | "unreadable-many" | "none";

export type DeepScanStatus = "not-started" | "queued" | "running" | "completed" | "failed" | "skipped";

export type LearningIndexRecord = {
  folderPath: string;
  folderHash: string;
  lastIndexedAt: string;
  lightIndex: LightIndex;
  score: LearningScore;
  explanations: string[];
  riskFlags: RiskFlag[];
  deepScanStatus: DeepScanStatus;
};

export type SourceSummary = {
  readme: boolean;
  docs: number;
  scripts: number;
  configs: number;
  agentDocs: string[];
  recentChanges: number;
};

export type LearningContext = {
  projectName: string;
  projectPath: string;
  summary: string;
  techStack: string[];
  architectureNotes: string[];
  importantFiles: string[];
  commonCommands: string[];
  conventions: string[];
  risks: string[];
  agentInstructions: string[];
  dtokenRecommendations: string[];
  openQuestions: string[];
  generatedAt: string;
  sourceSummary: SourceSummary;
};

export type DelegationPackage = {
  task: "generate-project-agent-context";
  allowedScope: string[];
  excludedScope: string[];
  projectSummary: LightIndex;
  selectedFilesSummary: SourceSummary;
  outputSchema: "LearningContext";
  safetyRules: string[];
};

/** 敏感文件/目录排除规则：深度扫描与索引都不得读取。 */
export const SENSITIVE_PATTERNS: readonly string[] = [
  ".env",
  ".env.local",
  ".env.production",
  "id_rsa",
  "id_ed25519",
  ".ssh",
  ".gnupg",
  "cookie",
  "token",
  "credentials",
  "secrets",
  "*.pem",
  "*.key",
  "*.p12",
  "*.keystore",
];

/** 依赖/构建产物目录排除。 */
export const SKIP_DIRS: readonly string[] = [
  "node_modules",
  ".git",
  "target",
  "dist",
  "build",
  ".next",
  ".venv",
  "venv",
  "__pycache__",
  ".cache",
  ".idea",
  ".vscode",
];

export function isSensitivePath(path: string): boolean {
  const lower = path.toLowerCase();
  return SENSITIVE_PATTERNS.some((p) => lower.includes(p.replace("*", "")));
}

export function isSkipDir(name: string): boolean {
  return SKIP_DIRS.includes(name);
}
