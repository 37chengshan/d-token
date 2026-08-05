// Central fixture (mock) data for the d-token console prototype.
// All data is local/simulated and labelled "Fixture Data" in the UI.

export type Health = "healthy" | "warning" | "error";
export type SupportLevel = "full" | "partial" | "none";

export const service = {
  running: true,
  version: "1.1.8",
  port: 8787,
  mode: "fixture" as "fixture" | "shadow" | "managed",
  bindHost: "127.0.0.1",
  activeRoute: "Claude Code → 压缩 → 官方 Anthropic",
  ports: [8787, 8123],
  doctor: "warning" as Health,
};

export const overviewStats = {
  savedToday: 1_284_500,
  savedWeek: 8_940_200,
  costSaved: 42.7,
  compressionRate: 0.68,
  activeAgents: 3,
  errorsToday: 2,
  layered: { rtk: 0.31, runtime: 0.29, route: 0.08 },
};

// 14-day savings trend (tokens saved, in thousands)
export const trend14 = [
  420, 510, 480, 610, 700, 560, 640, 720, 810, 690, 760, 880, 940, 1284,
].map((v, i) => ({ day: `D${i + 1}`, saved: v }));

// Activity heatmap: 12 months x 7 days intensity 0..4
export const heatmap = Array.from({ length: 12 }, (_, m) =>
  Array.from({ length: 7 }, (_, d) => {
    const boost = m >= 9 ? 2 : 0; // recent months busier
    return { m, d, level: Math.max(0, Math.min(4, Math.floor(Math.random() * 3) - 1 + boost)) };
  }),
).flat();

export const months = ["7月", "8月", "9月", "10月", "11月", "12月", "1月", "2月", "3月", "4月", "5月", "6月"];

export type Agent = {
  id: string;
  name: string;
  installed: boolean;
  binaryPath: string | null;
  version: string | null;
  rtk: SupportLevel;
  headroom: SupportLevel;
  configMethod: string;
  configMethodEn: string;
  platform: { macos: SupportLevel; winNative: SupportLevel; wsl: SupportLevel };
  configured: boolean;
};

export const agents: Agent[] = [
  {
    id: "claude-code", name: "Claude Code", installed: true,
    binaryPath: "/usr/local/bin/claude", version: "1.0.42",
    rtk: "full", headroom: "full", configMethod: "Hook + 环境变量", configMethodEn: "Hook + env vars",
    platform: { macos: "full", winNative: "partial", wsl: "full" }, configured: true,
  },
  {
    id: "codex", name: "Codex CLI", installed: true,
    binaryPath: "/usr/local/bin/codex", version: "0.8.1",
    rtk: "partial", headroom: "full", configMethod: "配置文件", configMethodEn: "Config file",
    platform: { macos: "full", winNative: "partial", wsl: "full" }, configured: false,
  },
  {
    id: "cline", name: "Cline", installed: true,
    binaryPath: "~/.vscode/extensions/cline", version: "3.2.0",
    rtk: "partial", headroom: "partial", configMethod: "环境变量", configMethodEn: "Env vars",
    platform: { macos: "full", winNative: "full", wsl: "full" }, configured: false,
  },
  {
    id: "opencode", name: "OpenCode", installed: false,
    binaryPath: null, version: null,
    rtk: "full", headroom: "full", configMethod: "配置文件 + 代理", configMethodEn: "Config file + proxy",
    platform: { macos: "full", winNative: "partial", wsl: "full" }, configured: false,
  },
];

export const ccSwitch = {
  detected: true,
  proxyEnabled: true,
  port: 8123,
  decision: "经 cc-switch 代理", // current auto decision
  decisionEn: "Route via cc-switch",
};

export type Provider = {
  id: string; name: string; kind: "official" | "custom" | "cc_switch" | "local_chain";
  protocol: "anthropic" | "openai" | "custom"; baseUrl: string; health: Health;
  store: string; keyMasked: string;
};

export const providers: Provider[] = [
  { id: "p1", name: "官方 Anthropic", kind: "official", protocol: "anthropic", baseUrl: "api.anthropic.com", health: "healthy", store: "keychain", keyMasked: "sk-ant-***…***a91" },
  { id: "p2", name: "自定义 OpenAI 兼容", kind: "custom", protocol: "openai", baseUrl: "127.0.0.1:8080", health: "warning", store: "keychain", keyMasked: "sk-***…***f42" },
  { id: "p3", name: "cc-switch 桥接", kind: "cc_switch", protocol: "anthropic", baseUrl: "127.0.0.1:8123", health: "healthy", store: "encrypted", keyMasked: "cc-***…***0d7" },
];

export type Route = {
  id: string; name: string; agent: string; mode: string; target: string; port: number; fallback: string; risk: "low" | "medium" | "high";
};

export const routes: Route[] = [
  { id: "r1", name: "Claude → 压缩 → 官方", agent: "Claude Code", mode: "compress_then_provider", target: "官方 Anthropic", port: 8787, fallback: "无静默回退", risk: "low" },
  { id: "r2", name: "Codex → cc-switch", agent: "Codex CLI", mode: "compress_then_cc_switch", target: "cc-switch 桥接", port: 8788, fallback: "提示用户", risk: "medium" },
];

export type PortRow = { id: string; host: string; port: number; kind: string; confidence: "confirmed" | "candidate" | "unknown"; owner: string; health: Health };
export const ports: PortRow[] = [
  { id: "1", host: "127.0.0.1", port: 8787, kind: "dtoken_runtime", confidence: "confirmed", owner: "d-token", health: "healthy" },
  { id: "2", host: "127.0.0.1", port: 8123, kind: "cc_switch", confidence: "confirmed", owner: "cc-switch", health: "healthy" },
  { id: "3", host: "127.0.0.1", port: 8080, kind: "custom_openai_provider", confidence: "candidate", owner: "unknown", health: "warning" },
];

export type Project = {
  id: string; name: string; root: string; stack: string; level: string; verbosity: string;
  recommended: string; confidence: number;
  flags: { ccr: boolean; memory: boolean; learn: boolean; output_shaper: boolean; cache_aligner: boolean };
};
export const projects: Project[] = [
  { id: "pr1", name: "web-dashboard", root: "~/code/web-dashboard", stack: "TypeScript / React", level: "balanced", verbosity: "summary", recommended: "balanced", confidence: 0.86, flags: { ccr: true, memory: true, learn: false, output_shaper: true, cache_aligner: true } },
  { id: "pr2", name: "rust-core", root: "~/code/rust-core", stack: "Rust", level: "aggressive", verbosity: "minimal", recommended: "aggressive", confidence: 0.91, flags: { ccr: true, memory: true, learn: true, output_shaper: true, cache_aligner: true } },
  { id: "pr3", name: "docs-site", root: "~/code/docs-site", stack: "Markdown / Astro", level: "safe", verbosity: "raw", recommended: "safe", confidence: 0.74, flags: { ccr: false, memory: false, learn: false, output_shaper: false, cache_aligner: true } },
];

export const outputLevels = [
  { key: "off", zh: "关闭", en: "Off" },
  { key: "safe", zh: "安全", en: "Safe" },
  { key: "balanced", zh: "均衡", en: "Balanced" },
  { key: "aggressive", zh: "激进", en: "Aggressive" },
  { key: "max", zh: "最大", en: "Max" },
];

export type DoctorCheck = { id: string; layer: "runtime" | "port" | "route" | "provider" | "security"; name: string; nameEn: string; status: Health; detail: string; detailEn: string; repair?: string };
export const doctorChecks: DoctorCheck[] = [
  { id: "d1", layer: "runtime", name: "运行时进程存活", nameEn: "Runtime process alive", status: "healthy", detail: "PID 4821 运行中", detailEn: "PID 4821 running" },
  { id: "d2", layer: "runtime", name: "压缩层 (RTK) 就绪", nameEn: "Compression layer ready", status: "healthy", detail: "RTK v0.9 已加载", detailEn: "RTK v0.9 loaded" },
  { id: "d3", layer: "port", name: "端口 8787 响应", nameEn: "Port 8787 responds", status: "healthy", detail: "延迟 3ms", detailEn: "3ms latency" },
  { id: "d4", layer: "port", name: "端口冲突检测", nameEn: "Port conflict scan", status: "warning", detail: "8080 疑似被占用", detailEn: "8080 possibly in use", repair: "advanced" },
  { id: "d5", layer: "route", name: "路由配置有效", nameEn: "Route config valid", status: "healthy", detail: "2 条路由已验证", detailEn: "2 routes validated" },
  { id: "d6", layer: "provider", name: "Provider 凭据有效", nameEn: "Provider credential valid", status: "warning", detail: "自定义 OpenAI 401", detailEn: "Custom OpenAI 401", repair: "logs" },
  { id: "d7", layer: "provider", name: "健康检查通过", nameEn: "Healthcheck passing", status: "healthy", detail: "官方 Anthropic 正常", detailEn: "Anthropic healthy" },
  { id: "d8", layer: "security", name: "密钥仅存于钥匙串", nameEn: "Keys in keychain only", status: "healthy", detail: "无明文暴露", detailEn: "No plaintext exposure" },
  { id: "d9", layer: "security", name: "日志脱敏已启用", nameEn: "Log redaction enabled", status: "healthy", detail: "高级别脱敏", detailEn: "High redaction" },
  { id: "d10", layer: "security", name: "仅回环绑定", nameEn: "Loopback-only binding", status: "healthy", detail: "127.0.0.1", detailEn: "127.0.0.1" },
  { id: "d11", layer: "route", name: "NO_PROXY 校验", nameEn: "NO_PROXY validated", status: "healthy", detail: "localhost,127.0.0.1", detailEn: "localhost,127.0.0.1" },
  { id: "d12", layer: "runtime", name: "本地代理监听", nameEn: "Local proxy listening", status: "healthy", detail: "已监听 8787", detailEn: "Listening on 8787" },
];

export type Change = { id: string; file: string; reason: string; reasonEn: string; risk: "low" | "medium" | "high"; before: string; after: string };
export const changes: Change[] = [
  { id: "c1", file: "~/.claude/settings.json", reason: "注入 d-token 代理端点", reasonEn: "Inject d-token proxy endpoint", risk: "low",
    before: `{\n  "env": {}\n}`, after: `{\n  "env": {\n    "ANTHROPIC_BASE_URL": "http://127.0.0.1:8787"\n  }\n}` },
  { id: "c2", file: "~/.codex/config.toml", reason: "启用压缩路由", reasonEn: "Enable compression route", risk: "medium",
    before: `base_url = "https://api.openai.com"`, after: `base_url = "http://127.0.0.1:8788"  # dtoken managed` },
];

export type Backup = { id: string; time: string; files: number; ports: number[]; procs: number; before: Health; after: Health };
export const backups: Backup[] = [
  { id: "b1", time: "2026-07-02 09:14", files: 3, ports: [8787], procs: 1, before: "warning", after: "healthy" },
  { id: "b2", time: "2026-07-01 17:42", files: 2, ports: [8787, 8788], procs: 1, before: "error", after: "healthy" },
  { id: "b3", time: "2026-06-30 11:08", files: 1, ports: [8123], procs: 0, before: "warning", after: "healthy" },
];

export type LogEntry = { id: string; time: string; type: "route" | "compression" | "error" | "provider"; agent: string; msg: string; msgEn: string };
export const logs: LogEntry[] = [
  { id: "l1", time: "09:41:22", type: "compression", agent: "Claude Code", msg: "上下文压缩 12.4k → 3.9k tokens", msgEn: "Context compressed 12.4k → 3.9k tokens" },
  { id: "l2", time: "09:41:20", type: "route", agent: "Claude Code", msg: "路由决策 → 官方 Anthropic", msgEn: "Route decision → official Anthropic" },
  { id: "l3", time: "09:40:05", type: "error", agent: "Codex CLI", msg: "Provider 401，未静默回退", msgEn: "Provider 401, no silent fallback" },
  { id: "l4", time: "09:38:51", type: "provider", agent: "Cline", msg: "健康检查通过 (3ms)", msgEn: "Healthcheck passed (3ms)" },
  { id: "l5", time: "09:37:10", type: "compression", agent: "Codex CLI", msg: "RTK 清理输出 8.1k → 2.2k tokens", msgEn: "RTK cleaned output 8.1k → 2.2k tokens" },
  { id: "l6", time: "09:36:44", type: "route", agent: "Cursor", msg: "命中前缀缓存，节省 4.2k tokens", msgEn: "Prefix cache hit, saved 4.2k tokens" },
  { id: "l7", time: "09:35:12", type: "compression", agent: "Claude Code", msg: "SmartCrusher 压缩 JSON ratio=0.35", msgEn: "SmartCrusher compressed JSON ratio=0.35" },
  { id: "l8", time: "09:34:58", type: "provider", agent: "Claude Code", msg: "转发至官方 Anthropic (ttfb 118ms)", msgEn: "Forwarded to official Anthropic (ttfb 118ms)" },
  { id: "l9", time: "09:33:31", type: "compression", agent: "Cline", msg: "CodeCompressor AST 压缩 6.4k → 4.6k", msgEn: "CodeCompressor AST 6.4k → 4.6k" },
  { id: "l10", time: "09:32:07", type: "route", agent: "Codex CLI", msg: "NO_PROXY 校验通过，直连", msgEn: "NO_PROXY validated, direct connect" },
  { id: "l11", time: "09:31:40", type: "provider", agent: "Cursor", msg: "健康检查 200 OK", msgEn: "Healthcheck 200 OK" },
  { id: "l12", time: "09:30:15", type: "compression", agent: "Claude Code", msg: "LogCompressor 折叠日志 ratio=0.41", msgEn: "LogCompressor folded logs ratio=0.41" },
];

export type Secret = { id: string; name: string; store: string; masked: string; status: "valid" | "expired" };
export const secrets: Secret[] = [
  { id: "s1", name: "ANTHROPIC_API_KEY", store: "keychain", masked: "sk-ant-***…***a91", status: "valid" },
  { id: "s2", name: "OPENAI_API_KEY", store: "keychain", masked: "sk-***…***f42", status: "expired" },
  { id: "s3", name: "CC_SWITCH_TOKEN", store: "encrypted", masked: "cc-***…***0d7", status: "valid" },
];

// ---- Headroom dashboard (migrated from headroom `dashboard` / /stats) ----
export const hr = {
  costSavedUsd: 2.45,
  tokens: { saved: 302_400, savedPercent: 58.2, proxySaved: 268_100, cliSaved: 34_300, ofWirePercent: 41.3 },
  output: { available: true, saved: 8_450, percent: 16.1, method: "MEASURED" as "MEASURED" | "ESTIMATED", requests: 2340, ciLow: 14.2, ciHigh: 18.3 },
  overheadMs: 45,
  ttfbMs: 120,
  throughput: {
    inputWall: 1200, inputP50: 1050,
    compP50: 450, compP95: 850,
    fwdP50: 2100, fwdP95: 3200,
    genP50: 1500, genP95: 2800,
  },
  agentTotals: { requests: 234, before: 45_000, after: 34_500, saved: 10_500, savingsPercent: 23.3 },
  agents: [
    { agent: "claude", label: "Claude Code", source: "X-Headroom-Project", requests: 150, before: 30_000, after: 22_500, saved: 7_500, savingsPercent: 25.0, share: 71.4, dot: "#f97316" },
    { agent: "cursor", label: "Cursor", source: "User-Agent", requests: 54, before: 10_000, after: 8_100, saved: 1_900, savingsPercent: 19.0, share: 18.1, dot: "#38bdf8" },
    { agent: "codex", label: "Codex CLI", source: "X-Headroom-Project", requests: 30, before: 5_000, after: 3_900, saved: 1_100, savingsPercent: 22.0, share: 10.5, dot: "#a78bfa" },
  ],
  savings: { compressionUsd: 2.10, cacheUsd: 0.35 },
  prefixCache: { reads: 450, writes: 230, hitRate: 65.2, busts: 12, providers: ["anthropic", "openai"] },
  pipeline: [
    { name: "SmartCrusher", kind: "JSON", requests: 820, tokens: 142_000, ratio: 0.35 },
    { name: "CodeCompressor", kind: "AST", requests: 610, tokens: 98_400, ratio: 0.28 },
    { name: "LogCompressor", kind: "logs", requests: 430, tokens: 61_700, ratio: 0.41 },
    { name: "Kompress-v2", kind: "text", requests: 290, tokens: 24_800, ratio: 0.19 },
  ],
  quotaWindows: [
    { label: "5 小时窗口", labelEn: "5-hour window", util: 42, resetIn: "2h13m" },
    { label: "7 天窗口", labelEn: "7-day window", util: 71, resetIn: "3d4h" },
  ],
  history: [
    { ts: "07-02 10:30", requests: 45, before: 12_000, after: 9_200, saved: 2_800, ratio: 0.23, cost: 0.045 },
    { ts: "07-02 09:30", requests: 62, before: 15_400, after: 11_100, saved: 4_300, ratio: 0.28, cost: 0.061 },
    { ts: "07-02 08:30", requests: 38, before: 9_800, after: 7_600, saved: 2_200, ratio: 0.22, cost: 0.033 },
    { ts: "07-01 18:30", requests: 71, before: 18_200, after: 12_900, saved: 5_300, ratio: 0.29, cost: 0.078 },
    { ts: "07-01 17:30", requests: 55, before: 13_600, after: 10_400, saved: 3_200, ratio: 0.24, cost: 0.049 },
  ],
  daily: [
    { d: "6-26", saved: 210 }, { d: "6-27", saved: 260 }, { d: "6-28", saved: 240 },
    { d: "6-29", saved: 320 }, { d: "6-30", saved: 300 }, { d: "7-01", saved: 380 }, { d: "7-02", saved: 302 },
  ],
};

export const ownership = [
  { id: "o1", labelKey: "ch.own.native", items: ["~/.claude/settings.json", "~/.codex/config.toml"] },
  { id: "o2", labelKey: "ch.own.dtoken", items: ["~/.dtoken/config.json", "~/.dtoken/events.db"] },
  { id: "o3", labelKey: "ch.own.managed", items: ["# >>> dtoken managed >>> 块 in settings.json"] },
  { id: "o4", labelKey: "ch.own.external", items: ["cc-switch providers.json（只读）"] },
];
