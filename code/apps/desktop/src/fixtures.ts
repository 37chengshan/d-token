/* Fallback data used only when the desktop backend is unavailable.
   The packaged Tauri app should prefer get_desktop_data from Rust. */

export interface HealthItem {
  key: string;
  label: string;
  state: "ok" | "warn" | "err";
}

export const health: HealthItem[] = [
  { key: "proxy", label: "Proxy", state: "ok" },
  { key: "rtk", label: "RTK", state: "ok" },
  { key: "headroom", label: "Headroom", state: "warn" },
  { key: "agents", label: "Agents", state: "ok" },
  { key: "routes", label: "Routes", state: "ok" },
  { key: "providers", label: "Providers", state: "ok" },
];

export const savings = {
  totalTokens: 4_820_000,
  rtkTokens: 2_910_000,
  headroomTokens: 1_910_000,
  costUsd: 186.4,
  deltaTotal: 12.4,
  deltaRtk: 9.8,
  deltaHeadroom: 15.2,
  deltaCost: 11.1,
};

/* 7-point spark series (0-100 normalized) */
export const rtkTrend = [42, 55, 48, 63, 70, 66, 78];
export const headroomTrend = [30, 38, 52, 49, 61, 68, 74];

export interface RouteDecision {
  id: string;
  agent: string;
  route: string;
  provider: string;
  model: string;
  saved: string;
  when: string;
}

export const recentDecisions: RouteDecision[] = [
  { id: "req_9f2a", agent: "Claude Code", route: "anthropic-main", provider: "Anthropic", model: "claude-sonnet-4", saved: "31%", when: "2m" },
  { id: "req_9f19", agent: "Cursor", route: "openai-main", provider: "OpenAI", model: "gpt-4.1", saved: "24%", when: "6m" },
  { id: "req_9ef0", agent: "Codex CLI", route: "openai-main", provider: "OpenAI", model: "gpt-4.1-mini", saved: "18%", when: "11m" },
  { id: "req_9ec7", agent: "Aider", route: "chain-local", provider: "Local Chain", model: "qwen2.5-coder", saved: "44%", when: "18m" },
];

export interface EventItem {
  level: "info" | "warn" | "err";
  time: string;
  msg: string;
}

export const recentEvents: EventItem[] = [
  { level: "warn", time: "18:41", msg: "Headroom proxy latency above 800ms threshold" },
  { level: "info", time: "18:39", msg: "RTK hook integrity verified for Claude Code" },
  { level: "err", time: "18:12", msg: "Provider healthcheck failed: local-chain (connection refused)" },
  { level: "info", time: "17:58", msg: "Route decision cache warmed for 4 agents" },
];

/* command family savings (RTK) */
export const commandFamilies = [
  { name: "test", pct: 68 },
  { name: "build", pct: 54 },
  { name: "git", pct: 41 },
  { name: "search", pct: 72 },
  { name: "package manager", pct: 38 },
  { name: "logs", pct: 61 },
];

export interface AgentRow {
  name: string;
  rtk: "ok" | "planned" | "partial";
  rtkMethod: string;
  headroom: "ok" | "planned" | "partial";
  headroomMethod: string;
  macos: "ok" | "partial" | "no";
  win: "ok" | "partial" | "no";
  wsl: "ok" | "partial" | "no";
  note: string;
}

export const agents: AgentRow[] = [
  { name: "Claude Code", rtk: "ok", rtkMethod: "hook", headroom: "ok", headroomMethod: "wrap", macos: "ok", win: "partial", wsl: "ok", note: "—" },
  { name: "VS Code Copilot Chat", rtk: "partial", rtkMethod: "rules", headroom: "partial", headroomMethod: "manual proxy", macos: "ok", win: "ok", wsl: "ok", note: "Chat only" },
  { name: "GitHub Copilot CLI", rtk: "ok", rtkMethod: "hook", headroom: "planned", headroomMethod: "planned", macos: "ok", win: "ok", wsl: "ok", note: "—" },
  { name: "Cursor", rtk: "ok", rtkMethod: "rules", headroom: "ok", headroomMethod: "manual proxy", macos: "ok", win: "ok", wsl: "partial", note: "—" },
  { name: "Gemini CLI", rtk: "ok", rtkMethod: "hook", headroom: "partial", headroomMethod: "library only", macos: "ok", win: "partial", wsl: "ok", note: "—" },
  { name: "OpenCode", rtk: "ok", rtkMethod: "plugin", headroom: "ok", headroomMethod: "wrap", macos: "ok", win: "partial", wsl: "ok", note: "—" },
  { name: "OpenClaw", rtk: "planned", rtkMethod: "planned", headroom: "planned", headroomMethod: "planned", macos: "partial", win: "no", wsl: "partial", note: "Early" },
  { name: "Pi", rtk: "partial", rtkMethod: "instructions", headroom: "planned", headroomMethod: "planned", macos: "ok", win: "partial", wsl: "ok", note: "—" },
  { name: "Hermes", rtk: "planned", rtkMethod: "planned", headroom: "partial", headroomMethod: "memory/learn only", macos: "partial", win: "no", wsl: "partial", note: "Early" },
  { name: "Cline / Roo Code", rtk: "ok", rtkMethod: "rules", headroom: "ok", headroomMethod: "manual proxy", macos: "ok", win: "ok", wsl: "ok", note: "—" },
  { name: "Windsurf", rtk: "partial", rtkMethod: "rules", headroom: "partial", headroomMethod: "manual proxy", macos: "ok", win: "ok", wsl: "partial", note: "—" },
  { name: "Codex CLI", rtk: "ok", rtkMethod: "hook", headroom: "ok", headroomMethod: "wrap", macos: "ok", win: "ok", wsl: "ok", note: "Primary target" },
  { name: "Kilo Code", rtk: "partial", rtkMethod: "plugin", headroom: "planned", headroomMethod: "planned", macos: "ok", win: "partial", wsl: "ok", note: "—" },
  { name: "Google Antigravity", rtk: "planned", rtkMethod: "planned", headroom: "planned", headroomMethod: "planned", macos: "partial", win: "partial", wsl: "no", note: "Early" },
  { name: "Mistral Vibe", rtk: "planned", rtkMethod: "planned", headroom: "planned", headroomMethod: "planned", macos: "partial", win: "no", wsl: "partial", note: "Early" },
  { name: "Aider", rtk: "ok", rtkMethod: "hook", headroom: "ok", headroomMethod: "wrap", macos: "ok", win: "ok", wsl: "ok", note: "—" },
  { name: "Continue", rtk: "ok", rtkMethod: "plugin", headroom: "partial", headroomMethod: "library only", macos: "ok", win: "ok", wsl: "ok", note: "—" },
  { name: "Goose", rtk: "partial", rtkMethod: "instructions", headroom: "partial", headroomMethod: "memory/learn only", macos: "ok", win: "partial", wsl: "ok", note: "—" },
  { name: "OpenHands", rtk: "planned", rtkMethod: "planned", headroom: "partial", headroomMethod: "manual proxy", macos: "ok", win: "partial", wsl: "ok", note: "—" },
  { name: "Cortex Code", rtk: "planned", rtkMethod: "planned", headroom: "planned", headroomMethod: "planned", macos: "partial", win: "no", wsl: "partial", note: "Early" },
];

export interface ProjectRow {
  root: string;
  stack: string;
  agents: string[];
  route: string;
  rtkProfile: string;
  policy: string;
  level: string;
  reason: string;
  features: string[];
}

export const projects: ProjectRow[] = [
  {
    root: "~/work/d-token",
    stack: "Rust · TypeScript",
    agents: ["Claude Code", "Codex CLI"],
    route: "anthropic-main",
    rtkProfile: "balanced",
    policy: "adaptive",
    level: "L2 · Balanced",
    reason: "Mixed code + long logs; L2 keeps diffs readable while saving on test output.",
    features: ["CCR", "memory", "output shaper", "code compressor"],
  },
  {
    root: "~/work/headroom-lab",
    stack: "Python",
    agents: ["Cursor", "Aider"],
    route: "openai-main",
    rtkProfile: "aggressive",
    policy: "aggressive",
    level: "L3 · Aggressive",
    reason: "Heavy retrieval + large notebooks; aggressive compression yields best savings.",
    features: ["CCR", "memory", "learn", "JSON SmartCrusher", "text Kompress"],
  },
  {
    root: "~/work/website",
    stack: "TypeScript · CSS",
    agents: ["Continue"],
    route: "chain-local",
    rtkProfile: "light",
    policy: "conservative",
    level: "L1 · Light",
    reason: "Small files, short sessions; light policy avoids over-compressing prompts.",
    features: ["output shaper", "cache aligner"],
  },
];

export interface RouteRow {
  id: string;
  name: string;
  protocol: "OpenAI-compatible" | "Anthropic-compatible";
  host: string;
  instance: string;
  provider: string;
  chain: "direct" | "chain bridge";
  policy: string;
  fallback: string;
  errors: number;
}

export const routes: RouteRow[] = [
  { id: "r1", name: "anthropic-main", protocol: "Anthropic-compatible", host: "127.0.0.1:8788", instance: "default", provider: "Anthropic", chain: "direct", policy: "adaptive", fallback: "—", errors: 0 },
  { id: "r2", name: "openai-main", protocol: "OpenAI-compatible", host: "127.0.0.1:8789", instance: "default", provider: "OpenAI", chain: "direct", policy: "adaptive", fallback: "openai-backup", errors: 1 },
  { id: "r3", name: "chain-local", protocol: "OpenAI-compatible", host: "127.0.0.1:8790", instance: "lab", provider: "Local Chain", chain: "chain bridge", policy: "aggressive", fallback: "—", errors: 2 },
];

export interface ProviderRow {
  name: string;
  kind: "official" | "compatible" | "custom" | "local-chain";
  protocol: string;
  baseUrl: string;
  models: string;
  timeout: string;
  health: "ok" | "warn" | "err";
  cred: string;
}

export const providers: ProviderRow[] = [
  { name: "Anthropic", kind: "official", protocol: "Anthropic", baseUrl: "https://api.anthropic.com", models: "claude-sonnet-4, claude-opus-4", timeout: "60s", health: "ok", cred: "keychain://anthropic" },
  { name: "OpenAI", kind: "official", protocol: "OpenAI", baseUrl: "https://api.openai.com/v1", models: "gpt-4.1, gpt-4.1-mini", timeout: "60s", health: "ok", cred: "keychain://openai" },
  { name: "OpenAI Backup", kind: "compatible", protocol: "OpenAI", baseUrl: "https://oai-proxy.internal", models: "gpt-4.1", timeout: "45s", health: "warn", cred: "keychain://oai-backup" },
  { name: "Local Chain", kind: "local-chain", protocol: "OpenAI", baseUrl: "http://127.0.0.1:11434/v1", models: "qwen2.5-coder, deepseek-coder", timeout: "120s", health: "err", cred: "none (local)" },
];

export interface DoctorRow {
  check: string;
  state: "ok" | "warn" | "err";
  detail: string;
}

export const doctorChecks: DoctorRow[] = [
  { check: "Runtime mode safety", state: "ok", detail: "Read-only fallback active; packaged desktop builds should read local tool state from Tauri." },
  { check: "External action isolation", state: "ok", detail: "No real tool calls detected from the current UI flow." },
  { check: "Test port vs real port", state: "ok", detail: "Using test ports 8788-8790 only." },
  { check: "Temp HOME / PATH (fake mode)", state: "ok", detail: "Sandbox HOME and PATH are isolated." },
  { check: "RTK availability", state: "ok", detail: "RTK test adapter responding (v0.9.2)." },
  { check: "Headroom availability", state: "warn", detail: "Proxy latency elevated (820ms)." },
  { check: "Proxy binding host", state: "ok", detail: "Bound to 127.0.0.1 (loopback only)." },
  { check: "Provider route validity", state: "warn", detail: "Route 'chain-local' target refused connection." },
  { check: "Credential storage", state: "ok", detail: "All credentials referenced via OS keychain." },
  { check: "Config write risk", state: "ok", detail: "No pending writes; plan-only mode." },
  { check: "Backup / rollback readiness", state: "ok", detail: "2 rollback points available." },
  { check: "Platform requirements", state: "err", detail: "WebView2 runtime not found on this Windows host." },
];

export interface LogRow {
  time: string;
  level: "info" | "warn" | "err";
  kind: "request" | "route" | "rtk" | "headroom" | "install" | "config" | "error";
  msg: string;
}

export const logs: LogRow[] = [
  { time: "18:41:22", level: "warn", kind: "headroom", msg: "headroom.proxy latency=820ms model=claude-sonnet-4 route=anthropic-main" },
  { time: "18:41:03", level: "info", kind: "request", msg: "req_9f2a agent=Claude Code route=anthropic-main tokens_in=8210 tokens_out=1204 latency=612ms" },
  { time: "18:40:51", level: "info", kind: "route", msg: "decision req_9f2a -> anthropic-main provider=Anthropic reason=protocol-match saved=31%" },
  { time: "18:40:12", level: "info", kind: "rtk", msg: "rtk.gain command=pytest saved_tokens=3120 family=test" },
  { time: "18:39:40", level: "info", kind: "rtk", msg: "rtk.hook integrity=verified agent=Claude Code precedence=Deny>Ask>Allow>Default" },
  { time: "18:12:05", level: "err", kind: "error", msg: "provider.healthcheck provider=local-chain error=connection_refused" },
  { time: "18:05:31", level: "info", kind: "config", msg: "config.patch plan_only=true target=~/.claude/settings.json changes=2" },
  { time: "17:58:10", level: "info", kind: "install", msg: "install.dryrun component=rtk plan=install steps=4" },
];

export interface ChangeRow {
  id: string;
  summary: string;
  target: string;
  risk: "low" | "medium" | "high";
  backup: string;
  diff: { type: "ctx" | "add" | "del"; text: string }[];
  postChecks: string[];
}

export const changes: ChangeRow[] = [
  {
    id: "c1",
    summary: "Add RTK hook to Claude Code settings",
    target: "~/.claude/settings.json",
    risk: "low",
    backup: "~/.dtoken/backups/2026-07-01/claude-settings.json",
    diff: [
      { type: "ctx", text: '  "hooks": {' },
      { type: "add", text: '+   "PreToolUse": "rtk wrap --agent claude-code",' },
      { type: "add", text: '+   "telemetry": false,' },
      { type: "ctx", text: '  }' },
    ],
    postChecks: ["RTK availability", "Hook integrity", "Runtime mode safety"],
  },
  {
    id: "c2",
    summary: "Register openai-main route for Cursor",
    target: "~/.dtoken/routes.toml",
    risk: "medium",
    backup: "~/.dtoken/backups/2026-07-01/routes.toml",
    diff: [
      { type: "ctx", text: "[route.openai-main]" },
      { type: "del", text: '- provider = "openai"' },
      { type: "add", text: '+ provider = "openai"' },
      { type: "add", text: '+ fallback = "openai-backup"' },
    ],
    postChecks: ["Provider route validity", "Proxy binding host"],
  },
];

export interface BackupPoint {
  id: string;
  label: string;
  when: string;
  files: number;
  tools: string[];
  verified: boolean;
}

export const backupPoints: BackupPoint[] = [
  { id: "b2", label: "Before RTK hook install", when: "2026-07-01 18:05", files: 2, tools: ["rtk"], verified: true },
  { id: "b1", label: "Initial managed setup", when: "2026-06-28 09:12", files: 5, tools: ["rtk", "headroom"], verified: true },
];

export function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return String(n);
}
