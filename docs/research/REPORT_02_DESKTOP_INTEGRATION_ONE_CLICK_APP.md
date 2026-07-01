# 深度研究报告 02：RTK 与 Headroom 图形 App 集成扩展版

日期：2026-07-01  
适用项目：d-token  
目标：用户通过图形 App 一键安装、选择式安装、配置、诊断、调优、回滚 RTK 与 Headroom。

## 1. 执行摘要

d-token Desktop 不应只是“启动 RTK/Headroom 的按钮”，而应成为本地 AI Coding 压缩控制面。

核心控制链：

```text
System Scanner
  ↓
Tool Selection
  ↓
Install Transaction Planner
  ↓
Config Diff / Backup / Rollback
  ↓
Runtime Manager
  ↓
Project Policy Manager
  ↓
Doctor
  ↓
Dashboard
```

它必须解决 6 个用户问题：

1. 我本机是否具备安装条件？
2. 我该装 RTK、Headroom、还是都装？
3. 它会改哪些配置文件？
4. 出错能不能回滚？
5. 每个项目应该用什么压缩等级？
6. 我怎么知道每一层到底省了多少？

## 2. Desktop 总体架构

```text
Tauri 2 Desktop
  ├─ React/TypeScript UI
  ├─ Installer Wizard
  ├─ Tool Manager UI
  ├─ Project Policy UI
  ├─ Doctor UI
  ├─ Dashboard UI
  └─ Logs / Export UI

Rust Core API
  ├─ install planner
  ├─ process manager
  ├─ config patcher
  ├─ backup manager
  ├─ RTK adapter
  ├─ Headroom adapter
  ├─ project policy manager
  ├─ event ledger
  └─ doctor runner

External / Managed Tools
  ├─ RTK binary
  ├─ Headroom Python runtime
  ├─ Headroom proxy
  ├─ Headroom MCP server
  └─ d-token local proxy
```

## 3. 安装事务模型

### 3.1 为什么必须是 transaction

RTK 与 Headroom 都可能修改 agent 配置、shell 配置、MCP 配置、PATH、项目规则文件。安装失败后如果没有事务模型，用户的开发环境可能进入“半安装”状态。

因此安装必须是事务：

```text
Plan
  ↓
Preview Diff
  ↓
Backup
  ↓
Apply
  ↓
Validate
  ↓
Doctor
  ↓
Commit Rollback Point
```

失败时：

```text
Apply failed / Validate failed / Doctor failed
  ↓
Stop new processes
  ↓
Restore backups
  ↓
Remove partial files
  ↓
Report exact failure
```

### 3.2 InstallationTransaction

```ts
interface InstallationTransaction {
  id: string;
  created_at: string;
  mode: 'external' | 'managed' | 'mixed';
  selected_tools: ToolKind[];
  selected_agents: AgentKind[];
  steps: InstallStep[];
  patch_plans: ConfigPatchPlan[];
  backups: BackupRecord[];
  status: 'planned' | 'awaiting_confirm' | 'applying' | 'validating' | 'done' | 'failed' | 'rolled_back';
  rollback_point_id?: string;
}
```

### 3.3 InstallStep

```ts
interface InstallStep {
  id: string;
  kind:
    | 'detect_tool'
    | 'download_binary'
    | 'verify_binary'
    | 'install_runtime'
    | 'patch_config'
    | 'start_process'
    | 'run_doctor'
    | 'import_metrics';
  tool?: 'rtk' | 'headroom' | 'dtoken';
  status: 'pending' | 'running' | 'success' | 'failed' | 'skipped';
  message?: string;
  reversible: boolean;
}
```

## 4. 安装向导页面

### Step 1：System Scan

检测：

1. OS / arch。
2. shell。
3. PATH。
4. WebView2。
5. Xcode CLT。
6. WSL。
7. Python 版本。
8. pip/uv。
9. Homebrew。
10. existing RTK。
11. existing Headroom。
12. existing d-token processes。
13. port availability。
14. credential store availability。

输出：

```ts
interface SystemScanResult {
  os: 'windows' | 'macos';
  arch: 'x64' | 'arm64';
  shells: ShellDetection[];
  runtimes: RuntimeDetection[];
  tools: ToolDetection[];
  ports: PortCheck[];
  warnings: DiagnosticWarning[];
  blockers: DiagnosticBlocker[];
}
```

### Step 2：Select Tools

用户可自由组合：

| 模式 | d-token | RTK | Headroom | MCP | hooks | 适合场景 |
|---|---|---|---|---|---|---|
| Minimal | 是 | 否 | 否 | 否 | 否 | 只做 route/provider/dashboard |
| RTK only | 是 | 是 | 否 | 否 | 可选 | 只优化 CLI 输出 |
| Headroom only | 是 | 否 | 是 | 可选 | 可选 | 只优化 API 上下文 |
| Layered | 是 | 是 | 是 | 可选 | 是 | 完整分层压缩 |
| External | 是 | 已有 | 已有 | 已有 | 已有 | 高级用户 |
| Managed | 是 | d-token 管理 | d-token 管理 | d-token 管理 | d-token 管理 | 普通用户 |

### Step 3：Select Agents

每个 agent 显示：

1. RTK auto-rewrite 支持状态。
2. Headroom wrap/proxy 支持状态。
3. MCP 支持状态。
4. 需要修改的文件。
5. 是否需要重启。
6. 风险等级。
7. 回滚路径。

### Step 4：Provider Setup

配置 provider 时必须区分：

1. official provider。
2. custom OpenAI-compatible provider。
3. custom Anthropic-compatible provider。
4. chain bridge。
5. Headroom proxy upstream。
6. d-token route target。

避免“d-token route”和“Headroom provider fallback”同时生效导致链路不可解释。

### Step 5：Project Policy Setup

新项目默认：

```text
Headroom level: safe
RTK: enabled if installed
CCR: enabled
memory: off unless user enables
learn: off unless user enables
MCP: optional
output shaper: off by default
recommendation: pending until enough metrics
```

## 5. Managed / External 工具生命周期

### 5.1 External mode

用户已有 RTK/Headroom，d-token 只做：

1. detect。
2. status。
3. import metrics。
4. doctor。
5. dashboard。
6. read-only config summary。

不做：

1. install。
2. upgrade。
3. uninstall。
4. silent patch。
5. telemetry enable。

### 5.2 Managed mode

d-token 管理工具生命周期：

1. install。
2. verify。
3. configure。
4. start。
5. stop。
6. upgrade。
7. uninstall。
8. rollback。

必须记录 provenance：

```ts
interface ManagedToolProvenance {
  tool: 'rtk' | 'headroom';
  version: string;
  source: 'github_release' | 'homebrew' | 'pip' | 'bundled' | 'manual';
  download_url?: string;
  checksum?: string;
  installed_at: string;
  installed_path: string;
}
```

### 5.3 Version pinning and updates

建议：

1. 默认 pin 当前验证版本。
2. 提供 stable channel。
3. advanced 用户可选择 latest channel。
4. 升级前生成 rollback point。
5. 升级后运行 Doctor。
6. 升级失败自动回滚。

## 6. ConfigPatcher 深化设计

### 6.1 Patch 分类

| Patch 类型 | 示例 | 风险 |
|---|---|---|
| Agent settings | Claude settings, Cursor hooks | 高 |
| Agent rule file | AGENTS.md, CLAUDE.md, RTK.md | 中 |
| Shell profile | `.zshrc`, `.bashrc`, PATH | 高 |
| RTK config | `~/.config/rtk/config.toml` | 中 |
| Headroom config | proxy/MCP/wrap config | 中 |
| d-token config | local profiles | 低 |

### 6.2 Conflict resolution

冲突处理原则：

1. unknown format：只读，不写。
2. existing hook：展示 merge plan。
3. duplicate RTK block：不重复写入。
4. user-custom block：保留。
5. broken JSON/YAML：提示用户修复，不强行覆盖。
6. concurrent modification：hash mismatch 时重新生成 diff。

### 6.3 Rollback 规则

1. rollback 不依赖当前文件是否可解析。
2. rollback 用 backup copy 恢复。
3. rollback 后重新运行 Doctor。
4. rollback 也要写 audit log。
5. rollback 不删除用户安装的外部工具，除非 managed install 且用户选择 uninstall。

## 7. Headroom 项目级策略产品化

### 7.1 策略层级

```text
Global default policy
  ↓
Workspace policy
  ↓
Project policy
  ↓
Agent override
  ↓
Temporary session override
```

优先级：下层覆盖上层，但所有 override 都要在 UI 中可见。

### 7.2 推荐等级解释

推荐引擎必须输出：

```ts
interface CompressionRecommendation {
  project_id: string;
  recommended_level: 'safe' | 'balanced' | 'aggressive' | 'max';
  confidence: 'low' | 'medium' | 'high';
  reasons: string[];
  risks: string[];
  expected_savings_range: [number, number];
  suggested_toggles: Partial<ProjectCompressionPolicy>;
}
```

用户点击“应用推荐”前必须看到：

1. 变更项。
2. 预期收益。
3. 风险提示。
4. 回滚入口。

### 7.3 Quality feedback loop

收集本地信号：

1. retrieve rate。
2. compression failure rate。
3. user rollback count。
4. manual level override。
5. model answer retry。
6. tests passed/failed after compression。
7. RTK passthrough rate。
8. Headroom pass-through/no-op rate。

这些信号只用于本地推荐，默认不上传。

## 8. Dashboard 状态设计

每张卡片必须有 5 种状态：

```text
empty      没有数据
loading    正在读取
ready      正常
warning    可用但有风险
error      不可用或失败
```

关键卡片：

1. System Health。
2. Install Status。
3. RTK Status。
4. Headroom Status。
5. Project Policy。
6. Layered Savings。
7. RTK Gain。
8. Headroom Stats。
9. Config Patches。
10. Rollback Points。
11. Doctor Results。
12. Compatibility Warnings。

## 9. API 扩展

```text
GET  /install/scan
POST /install/plan
POST /install/apply
POST /install/rollback
GET  /install/transactions
GET  /tools
GET  /tools/:tool/status
GET  /tools/:tool/provenance
POST /tools/:tool/install
POST /tools/:tool/update
POST /tools/:tool/uninstall
GET  /rtk/status
GET  /rtk/gain
GET  /rtk/discover
GET  /rtk/session
GET  /headroom/status
GET  /headroom/proxy
GET  /headroom/mcp
GET  /headroom/stats
GET  /projects
POST /projects
GET  /projects/:id/policy
PUT  /projects/:id/policy
POST /projects/:id/recommend-compression
GET  /config/patches
POST /config/patches/plan
POST /config/patches/apply
GET  /config/rollback-points
POST /config/rollback
GET  /events
GET  /layered-savings
```

## 10. Windows/macOS 深化策略

### 10.1 macOS

1. Homebrew RTK detection。
2. pip/uv Headroom runtime detection。
3. Keychain for secrets。
4. quarantine/xattr warning for downloaded binaries。
5. signing/notarization before auto-update。
6. zsh/bash profile patch diff。
7. Apple Silicon vs Intel binary selection。

### 10.2 Windows

1. WebView2 first-run check。
2. Native Windows vs WSL split。
3. RTK native：filters work, auto-rewrite limited。
4. WSL：recommended for RTK hook。
5. PowerShell/cmd/Windows Terminal instructions separated。
6. Credential Manager for secrets。
7. PATH/shim `.exe/.cmd/.bat` support。
8. Python runtime isolation for Headroom。

## 11. 测试矩阵

| 测试类型 | 必须覆盖 |
|---|---|
| installer | clean install、external detected、managed install、failed install、rollback |
| config patch | JSON/YAML/TOML/shell profile、hash mismatch、backup restore |
| RTK | missing、wrong package、version、hook installed、telemetry disabled、gain parse |
| Headroom | missing CLI、npm SDK only、proxy running、MCP status、CCR summary |
| project policy | default safe、recommend balanced/aggressive、manual override、rollback |
| dashboard | empty/loading/ready/warning/error |
| platform | mac Intel、mac ARM、Windows native、Windows WSL |

## 12. 结论

d-token Desktop 的成败不在于 UI 漂亮，而在于安装和配置是否安全可靠。

最小可用路径：

```text
System Scan
  → Select Tools
  → Preview Diff
  → Backup
  → Apply
  → Doctor
  → Dashboard
  → Rollback if needed
```

只要这个闭环扎实，d-token 才能成为 RTK + Headroom 的真正产品化入口。
