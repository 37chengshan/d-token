# 深度研究报告 02：如何把 RTK 与 Headroom 全面集成迁移到图形 App

日期：2026-07-01  
适用项目：d-token  
目标：用户只需点击即可安装、配置、诊断、调优 RTK 与 Headroom，并可按项目自定义 Headroom 压缩等级和推荐等级。

## 1. 执行摘要

图形 App 不应该只是“启动几个命令”的壳，而应该是一个本地开发环境控制台：

```text
Install Wizard
  ↓
Tool Manager
  ↓
Runtime Manager
  ↓
Agent Config Patcher
  ↓
Project Policy Manager
  ↓
Doctor
  ↓
Dashboard
```

d-token Desktop 的核心价值是把 RTK 与 Headroom 从“需要用户懂 CLI、懂 hook、懂 proxy、懂 config”的工具，变成“可视化选择、一键安装、自动诊断、可回滚、可观测”的产品。

## 2. 图形 App 总体架构

```text
Tauri Desktop App
  ├─ React/TypeScript UI
  ├─ Installer Wizard
  ├─ Dashboard
  ├─ Project Policy UI
  ├─ Doctor UI
  └─ Settings UI

Rust Core API
  ├─ /health
  ├─ /install/plan
  ├─ /install/apply
  ├─ /install/rollback
  ├─ /rtk/status
  ├─ /rtk/gain
  ├─ /headroom/status
  ├─ /headroom/policies
  ├─ /projects
  ├─ /project-policies
  ├─ /doctor/run
  └─ /export/*

Local Managers
  ├─ ToolManager
  ├─ ProcessManager
  ├─ ConfigPatcher
  ├─ BackupManager
  ├─ ProjectPolicyManager
  ├─ RecommendationEngine
  └─ MetricsIngestion
```

## 3. 一键安装向导设计

### 3.1 安装向导阶段

```text
Step 1: System Scan
Step 2: Select Tools
Step 3: Select Agents
Step 4: Provider Setup
Step 5: Project Policy Setup
Step 6: Show Diff
Step 7: Apply Install
Step 8: Run Doctor
Step 9: Enter Dashboard
```

### 3.2 Step 1：System Scan

检测：

1. OS：Windows / macOS。
2. CPU：x64 / arm64。
3. Shell：bash / zsh / fish / powershell / cmd / WSL bash。
4. PATH 中是否已有 `rtk`。
5. PATH 中是否已有 `headroom`。
6. Python 版本。
7. pip/uv 是否可用。
8. Homebrew 是否可用。
9. WebView2 是否存在。
10. Xcode Command Line Tools 是否存在。
11. Windows 是否启用 WSL。
12. 端口是否可用。
13. 旧 d-token/RTK/Headroom 进程是否残留。

输出：

```ts
interface SystemScanResult {
  os: 'windows' | 'macos';
  arch: 'x64' | 'arm64';
  shells: string[];
  has_wsl?: boolean;
  has_webview2?: boolean;
  has_xcode_clt?: boolean;
  rtk?: ToolDetection;
  headroom?: ToolDetection;
  python?: RuntimeDetection;
  ports: PortCheck[];
  warnings: DiagnosticWarning[];
}
```

### 3.3 Step 2：Select Tools

用户可选择：

1. d-token Desktop App。
2. d-token Core。
3. RTK。
4. Headroom-compatible runtime。
5. MCP server。
6. Agent hooks。
7. Provider profiles。
8. Project policies。

选择矩阵：

| 模式 | RTK | Headroom | d-token core | 适合用户 |
|---|---|---|---|---|
| Minimal | 否 | 否 | 是 | 只想做 provider/route/dashboard |
| RTK only | 是 | 否 | 是 | 只优化 CLI 输出 |
| Headroom only | 否 | 是 | 是 | 只优化 API 上下文 |
| Layered | 是 | 是 | 是 | 最完整方案 |
| External | 用户已有 | 用户已有 | 是 | 高级用户 |
| Managed | d-token 管理 | d-token 管理 | 是 | 普通用户 |

### 3.4 Step 3：Select Agents

用户选择要配置的 agent：

1. Claude Code。
2. Codex。
3. Cline / Roo Code。
4. OpenCode。
5. Cursor。
6. Windsurf。
7. Gemini CLI。
8. GitHub Copilot CLI。
9. Kilo Code。
10. Antigravity。

每个 agent 页面展示：

1. 支持状态。
2. RTK hook 模式：hook / plugin / instruction / unsupported。
3. Headroom wrap 模式：wrap / proxy config / manual config。
4. 是否需要重启 agent。
5. 即将修改的文件。
6. 可回滚路径。

## 4. Tool Manager 设计

### 4.1 ToolDetection

```ts
interface ToolDetection {
  tool: 'rtk' | 'headroom' | 'dtoken_core';
  found: boolean;
  version?: string;
  binary_path?: string;
  install_method?: 'homebrew' | 'cargo' | 'pip' | 'managed' | 'manual' | 'unknown';
  status: 'ok' | 'missing' | 'wrong_package' | 'outdated' | 'unsupported' | 'error';
  message: string;
}
```

### 4.2 RTK detection

优先检查：

```bash
rtk --version
rtk gain --all --format json
rtk init --show
rtk telemetry status
```

特殊检测：

1. crates.io 同名 `rtk` 错误包。
2. Native Windows hook limitation。
3. WSL 是否可用。
4. hook 是否安装。
5. tracking database 是否存在。
6. tee directory 是否存在。

### 4.3 Headroom detection

优先检查：

```bash
headroom --version
headroom doctor
headroom perf
headroom dashboard --help
headroom mcp status
```

特殊检测：

1. Python 版本是否满足。
2. 是否通过 pip 安装。
3. npm `headroom-ai` 是否只是 SDK，而不是 CLI。
4. proxy 是否已运行。
5. 当前 agent 是否已 wrap。
6. savings ledger 是否存在。
7. CCR cache 是否存在。
8. MCP server 是否可用。

## 5. ConfigPatcher 设计

### 5.1 为什么必须有 ConfigPatcher

RTK 与 Headroom 都可能修改 agent 配置文件。

如果 d-token 直接静默写文件，会产生严重风险：

1. 破坏用户原始配置。
2. 覆盖已有 hooks。
3. 引入无法恢复的问题。
4. 让用户不知道真实发生了什么。

因此必须设计 ConfigPatcher。

### 5.2 修改流程

```text
Read config
  ↓
Parse config
  ↓
Generate desired patch
  ↓
Show diff
  ↓
Create backup
  ↓
Apply patch
  ↓
Validate file
  ↓
Run Doctor
  ↓
Commit rollback point
```

### 5.3 数据模型

```ts
interface ConfigPatchPlan {
  id: string;
  target_file: string;
  tool: 'rtk' | 'headroom' | 'dtoken';
  agent: string;
  before_hash: string;
  after_preview: string;
  diff: string;
  risk_level: 'low' | 'medium' | 'high';
  requires_restart: boolean;
}

interface RollbackPoint {
  id: string;
  created_at: string;
  target_files: string[];
  backup_dir: string;
  reason: string;
  applied_patch_ids: string[];
}
```

## 6. Project Policy Manager

### 6.1 为什么要按项目管理 Headroom 压缩等级

不同项目对压缩风险的容忍度不同。

例如：

1. 小型脚本项目：可 aggressive。
2. 大型 Rust/TS monorepo：balanced。
3. 法务/合规/财务文本：safe。
4. 生产数据库迁移：safe。
5. 大量日志排查：aggressive。
6. RAG chunks：balanced/aggressive。
7. 需要精准代码审查：safe/balanced。

所以 Headroom 不能只有全局压缩等级，必须有 per-project policy。

### 6.2 ProjectProfile

```ts
interface ProjectProfile {
  id: string;
  name: string;
  root_path: string;
  language_stack: string[];
  agents: string[];
  default_instance_id?: string;
  headroom_policy_id?: string;
  rtk_profile_id?: string;
  provider_route_id?: string;
  created_at: string;
  updated_at: string;
}
```

### 6.3 ProjectCompressionPolicy

```ts
interface ProjectCompressionPolicy {
  id: string;
  project_id: string;
  level: 'off' | 'safe' | 'balanced' | 'aggressive' | 'max';
  recommended_level: 'safe' | 'balanced' | 'aggressive' | 'max';
  recommendation_confidence: 'low' | 'medium' | 'high';
  enable_ccr: boolean;
  enable_memory: boolean;
  enable_learn: boolean;
  enable_mcp: boolean;
  enable_output_shaper: boolean;
  enable_cache_aligner: boolean;
  enable_code_compressor: boolean;
  enable_json_smartcrusher: boolean;
  enable_text_kompress: boolean;
  output_verbosity: 'auto' | 'terse' | 'normal' | 'detailed';
  holdout_ratio?: number;
  notes?: string;
}
```

### 6.4 推荐等级引擎

推荐输入信号：

1. 项目语言栈。
2. 最近请求 token 规模。
3. 命令输出噪音比例。
4. RTK 历史 savings。
5. Headroom 历史 savings。
6. CCR retrieve 频率。
7. 用户手动回滚次数。
8. LLM 回答质量反馈。
9. 失败率。
10. 当前任务风险等级。

推荐等级：

| Level | 推荐场景 | 风险 |
|---|---|---|
| off | 调试压缩问题、对比实验 | 无压缩收益 |
| safe | 高风险任务、新项目、精确代码审查 | savings 较低 |
| balanced | 常规编码默认 | 平衡 |
| aggressive | 大日志、大测试、大搜索输出 | 可能损失细节 |
| max | 批量日志/RAG/低风险探索 | 必须强提醒 |

### 6.5 UI 设计

项目页必须展示：

1. 当前等级。
2. 推荐等级。
3. 推荐理由。
4. 预计 savings。
5. 风险提示。
6. 一键应用推荐。
7. 手动覆盖。
8. 最近 7/30 天效果。
9. RTK 与 Headroom 分层贡献。
10. 回滚按钮。

## 7. Dashboard 信息架构

### 7.1 首页

首页应该是全局控制台：

1. System health。
2. d-token core status。
3. RTK status。
4. Headroom status。
5. Active projects。
6. Active instances。
7. Active routes。
8. End-to-end savings。
9. Recent errors。
10. Doctor warnings。

### 7.2 安装页

安装页用于：

1. 一键安装。
2. 选择工具。
3. 选择 agents。
4. 配置 provider。
5. 显示 diff。
6. 运行 Doctor。
7. 回滚。

### 7.3 项目页

项目页用于：

1. 管理 ProjectProfile。
2. 管理 ProjectCompressionPolicy。
3. 查看推荐等级。
4. 查看 per-project savings。
5. 查看 RTK/Headroom 分层贡献。
6. 查看 route/provider/instance 绑定。

### 7.4 RTK 页

RTK 页用于：

1. binary/version 检测。
2. hook 状态。
3. telemetry 状态。
4. gain summary。
5. recent commands。
6. missed opportunities。
7. tee recovery。
8. Windows/WSL 提示。

### 7.5 Headroom 页

Headroom 页用于：

1. proxy 状态。
2. wrap 状态。
3. MCP 状态。
4. compression policy。
5. memory/learn。
6. CCR cache summary。
7. output shaping。
8. dashboard migration cards。

## 8. API 设计

M2/M3 应新增：

```text
GET  /install/scan
POST /install/plan
POST /install/apply
POST /install/rollback
GET  /tools
GET  /tools/rtk/status
GET  /tools/headroom/status
GET  /rtk/gain
GET  /rtk/discover
GET  /rtk/session
GET  /headroom/stats
GET  /headroom/policies
GET  /projects
POST /projects
GET  /projects/:id/policy
PUT  /projects/:id/policy
POST /projects/:id/recommend-compression
GET  /layered-savings
POST /doctor/run
GET  /config/patches
POST /config/patches/plan
POST /config/patches/apply
POST /config/rollback
```

## 9. 安装实现策略

### 9.1 macOS

优先顺序：

1. 检测已有 RTK / Headroom。
2. RTK 优先 Homebrew。
3. Headroom 优先 Python pip/uv 环境。
4. Managed mode 可下载 sidecar binary/runtime。
5. 修改 zsh/bash 配置必须显示 diff。
6. 使用 Keychain 保存敏感信息。
7. signing/notarization 后再做自动更新。

### 9.2 Windows

优先顺序：

1. 检测 WebView2。
2. 检测 native Windows vs WSL。
3. RTK 完整 hook 推荐 WSL。
4. Native Windows 提示显式 `rtk ...`。
5. Headroom Python runtime 检测。
6. PATH 修改必须显示 diff。
7. Credential Manager 保存敏感信息。
8. PowerShell/cmd/Windows Terminal 指引分开写。

## 10. 测试计划

### 10.1 Installer tests

1. clean install。
2. external RTK detected。
3. external Headroom detected。
4. wrong RTK package detected。
5. missing Python。
6. missing WebView2。
7. WSL present / absent。
8. config conflict。
9. rollback success。
10. failed install recovery。

### 10.2 Project policy tests

1. safe/balanced/aggressive/max policy serialization。
2. recommendation engine deterministic output。
3. manual override。
4. rollback。
5. project route binding。
6. per-project metrics attribution。

### 10.3 Dashboard tests

1. all Headroom cards render with mock data。
2. all RTK cards render with mock data。
3. layered savings does not double count。
4. empty state。
5. error state。
6. Windows limitation warning。
7. telemetry disabled state。

## 11. 结论

d-token Desktop 的正确方向不是简单把 RTK 和 Headroom 命令包一层 UI，而是做一个“本地 AI Coding 压缩控制面”。

它必须解决 5 个用户真实痛点：

1. 我怎么装？
2. 我该装哪个？
3. 我怎么知道有没有生效？
4. 我怎么按项目调压缩强度？
5. 出问题怎么回滚？

只要这 5 个问题解决，d-token 就不只是工具集合，而是一个真正可用的本地 AI Coding 基础设施产品。
