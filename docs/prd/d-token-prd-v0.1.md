# d-token PRD v0.1

状态：Draft  
创建日期：2026-07-01  
目标平台：Windows 10/11、macOS  
目标用户：Claude Code / Codex / OpenAI-compatible / Anthropic-compatible 工具重度用户；需要多账号、多实例、中转、压缩节省 token 的 AI 编程用户。

---

## 1. 一句话结论

d-token 要做成一个 **本地优先的 AI Coding 压缩路由桌面 App**：用户不再手动折腾 Web Dashboard、命令行安装、端口、环境变量和多个 provider，而是在一个 Windows/macOS 可视化界面里完成 Claude/Codex 上游选择、第三方上游配置、cc-switch 链式转发、压缩率观测、用量记录、多实例启停。

---

## 2. 背景与机会

现有工具暴露出三类机会：

### 2.1 Headroom 类工具的机会

Headroom 已证明“本地压缩层 + 代理 + MCP + learn”有价值，但当前体验仍偏 CLI/Web Dashboard，并且第三方 provider、Windows 路径、编码、端口、路由链路对齐等问题会让普通用户卡住。

我们不直接复制其实现，而是借鉴以下产品方向：

- 请求到达 LLM 之前做上下文压缩。
- 对工具输出、日志、文件、RAG 结果、历史消息进行内容感知压缩。
- 本地代理、SDK、MCP、可逆检索缓存并存。
- 记录 token savings 与请求链路。
- 从失败会话中学习并生成项目级规则。

### 2.2 Cockpit Tools 类工具的机会

Cockpit Tools 已证明“桌面 App + 多账号 + 多实例 + 配额监控 + sidecar”是更适合普通用户的形态。d-token 应借鉴它的桌面化与实例管理能力：

- 多平台账号状态聚合。
- Codex/其他 AI IDE 多实例并行。
- 本地 sidecar API。
- 一键切换、一键启动、一键停止。
- 配额、状态、日志可视化。
- 中文界面与多语言基础。

### 2.3 用户真实痛点

当前链路中用户遇到的问题集中在：

- Claude Code 请求被错误发到官方 Anthropic，第三方 token 报 401。
- 重启服务时端口被旧进程占用。
- Claude/Codex 实际工作正常，但 Dashboard 看不到日志，因为请求没走到压缩代理。
- Output token savings 提示像错误，误导用户。
- `--apply` 不告诉用户必须在项目目录运行。
- Windows 中文 JSONL、盘符路径、项目名连字符、CLI shim 都可能导致 learn/verbosity 无法跑通。

这些问题说明：d-token 的核心价值不是“再做一个压缩库”，而是把压缩、路由、实例、上游、日志、错误修复和 UX 收成一个闭环。

---

## 3. 产品目标

### 3.1 v0.1 目标

建立 d-token 的产品、技术与目录边界：

- 明确支持 Claude/Codex 官方上游。
- 明确支持 OpenAI-compatible / Anthropic-compatible 第三方上游。
- 明确支持 `Agent -> d-token -> cc-switch -> Provider` 的二次路由链路。
- 明确 Windows/macOS 桌面 App 是主入口，CLI 仅做开发与高级调试入口。
- 明确多实例模型，尤其是 Codex 官方登录实例与第三方/中转实例分离。
- 初始化仓库目录与配置样例。

### 3.2 v0.2 目标

做出可运行的本地代理原型：

- d-token proxy 能监听本地端口。
- 支持 Anthropic-compatible 与 OpenAI-compatible 的请求识别。
- 支持 provider profile 配置。
- 支持简单压缩前置 hook。
- 支持请求日志、压缩前后 token 估算、错误原因展示。

### 3.3 v0.3 目标

做出桌面 App MVP：

- Windows/macOS 可安装。
- 可视化配置 provider。
- 可视化启动/停止多个 d-token proxy 实例。
- 展示请求日志、压缩率、上游、端口、错误。
- 提供 Claude/Codex 环境变量引导。

### 3.4 v0.4 目标

联动 cockpit-tools/多实例生态：

- 读取或对接外部实例配置。
- 为 Codex 官方实例与第三方实例生成独立代理配置。
- 支持两个压缩服务同时运行，分别转发到官方上游与第三方/cc-switch 链路。

---

## 4. 用户画像

### Persona A：Claude Code 重度用户

需求：

- 使用官方 Anthropic，也可能使用第三方 Claude-compatible provider。
- 想节省 token，但不想手动排查代理链路。
- 想看到每次请求到底走了哪个上游。

成功标准：

- 在 App 里选择 Claude 官方或第三方 provider。
- App 自动生成 base URL、API key、环境变量提示。
- 每条请求都能在日志中看到：agent、route、provider、tokens before/after、status。

### Persona B：Codex/OpenAI 用户

需求：

- 官方登录一个实例。
- 第三方/中转 provider 使用另一个实例。
- 两个实例分别走不同压缩服务和不同上游。

成功标准：

- App 中有两个 Codex instance：`codex-official` 与 `codex-relay`。
- 每个 instance 有自己的端口、配置、日志、启停状态。
- 两个实例可并行运行，互不污染 auth/config。

### Persona C：cc-switch 用户

需求：

- 上游路由由 cc-switch 管。
- 但希望 token 在进入 cc-switch 前已经被压缩。

成功标准：

- 链路固定为：`Claude/Codex -> d-token local proxy -> cc-switch local endpoint -> provider`。
- d-token 不取代 cc-switch 的模型路由职责，只做压缩、日志、链路健康检查。

---

## 5. 核心功能需求

### 5.1 Provider 管理

必须支持五类 provider：

1. `anthropic_official`：Anthropic 官方 Claude API。
2. `openai_official`：OpenAI 官方 API / Codex 相关 OpenAI-compatible 调用。
3. `custom_anthropic`：第三方 Anthropic-compatible provider。
4. `custom_openai`：第三方 OpenAI-compatible provider。
5. `cc_switch`：二级路由目标；d-token 先压缩，再转发给 cc-switch。

Provider 字段：

- `id`
- `name`
- `kind`
- `base_url`
- `api_key_env`
- `api_key_storage_ref`
- `protocol`
- `models`
- `timeout_ms`
- `healthcheck_path`
- `notes`

### 5.2 路由管理

路由由 `route_profile` 管理：

- `agent`: `claude-code` / `codex` / `cline` / `opencode` / `custom`
- `input_protocol`: `anthropic` / `openai`
- `compression_policy`: `off` / `safe` / `balanced` / `aggressive`
- `target_provider_id`
- `fallback_provider_id`
- `chain`: `direct` / `via_cc_switch`
- `local_port`
- `bind_host`: 默认 `127.0.0.1`

核心规则：

- 第三方 provider 必须显式配置 `base_url`，不得静默回落官方上游。
- cc-switch 链路必须显式显示两段跳转：`d-token -> cc-switch -> final provider`。
- 每条请求必须记录最终 route decision。

### 5.3 压缩引擎

v0.1 先定义接口，v0.2 再实现：

- `compress_request(messages, context, policy) -> compressed_messages`
- `compress_tool_output(output, content_type, policy) -> compressed_output`
- `store_original(content) -> retrieval_id`
- `retrieve_original(retrieval_id) -> content`

压缩策略：

- `safe`：只压缩日志、重复输出、无关文件列表，尽量不改写用户意图。
- `balanced`：压缩工具输出、长文件、重复上下文，保留关键错误、路径、符号、接口。
- `aggressive`：面向超长上下文，允许更强摘要，但必须保留可逆检索 ID。

### 5.4 桌面 App

推荐技术方向：Tauri + React/TypeScript + Rust sidecar。

核心页面：

1. 总览 Dashboard：运行实例、请求数、token before/after、节省率、错误请求。
2. Provider 页面：官方上游、第三方上游、cc-switch endpoint、健康检查。
3. 实例页面：Claude、Codex 官方、Codex relay/third-party 实例。
4. 请求日志页面：时间、Agent、Model、Route、Status、Token、Error reason。
5. 学习/规则页面：失败会话挖掘、项目级规则建议、本地规则文件应用。
6. 设置页面：语言、端口范围、日志保留、密钥存储、开机自启。

### 5.5 Windows/macOS 兼容

必须内置以下处理：

- UTF-8 文件读取：所有 JSON/JSONL/配置文件明确使用 UTF-8。
- Windows 盘符路径：支持 `D:\work\project`、`D--work-project`、session cwd 反查。
- 项目名连字符：不得用简单 `-` 推断路径；优先从 session 文件 cwd 字段反查。
- CLI shim：调用外部 CLI 时 Windows 优先尝试 `.cmd` / `.exe` / `.bat`。
- 端口占用：启动前检查端口；可一键停止旧服务；避免 WinError 10048。

---

## 6. Codex 双实例方案

用户的 Codex 方案固定为：

- **官方登录实例**：用于官方 OpenAI/Codex 登录与官方配额。
- **中转/第三方实例**：用于 custom provider 或 cc-switch 链路。

对应 d-token 实例：

```text
codex-official-instance
  Codex official config/auth
  -> d-token proxy A : 127.0.0.1:18001
  -> openai_official

codex-relay-instance
  Codex relay config/base_url/api_key
  -> d-token proxy B : 127.0.0.1:18002
  -> custom_openai OR cc_switch
```

关键要求：

- 两个实例的配置目录隔离。
- 两个 d-token proxy 端口隔离。
- 两条日志流隔离。
- App 中必须清楚标注“官方实例”和“中转实例”。

---

## 7. 非功能需求

### 7.1 安全

- 密钥仅本地保存。
- 默认只绑定 `127.0.0.1`。
- 日志默认脱敏 API key、Authorization header、Cookie。
- 导出日志前二次脱敏。
- 工具仅面向合法、合规的本地开发用途。

### 7.2 可靠性

- 服务启动前做端口占用检测。
- provider 健康检查失败时清楚提示是 base_url、key、protocol 还是网络问题。
- 所有路由错误必须能在 UI 中定位到具体 route profile。

### 7.3 可观测性

每条请求记录：

- request_id
- timestamp
- agent
- instance_id
- input_protocol
- target_provider
- chain mode
- model
- status code
- latency
- tokens before
- tokens after
- savings ratio
- error category

### 7.4 易用性

- 首次启动提供向导。
- 中文作为第一语言。
- 不要求用户先会 CLI。
- 所有“复制环境变量/启动命令”都有一键复制。

---

## 8. 明确不做

v0.1 不做：

- 云端账号同步。
- 账号共享、转售或托管。
- 违反平台条款的自动化用途。
- 直接复制 headroom/cockpit-tools 的代码。

---

## 9. 里程碑

### M0：仓库初始化

- README。
- PRD。
- 架构文档。
- 配置样例。
- 初始目录。
- 结构检查脚本。

### M1：Proxy 内核

- 本地 HTTP proxy。
- Provider adapter。
- Route profile。
- 请求日志。
- 健康检查。

### M2：压缩 MVP

- safe/balanced policy。
- 工具输出压缩。
- 请求消息压缩。
- 本地原文缓存。

### M3：Desktop MVP

- Tauri shell。
- Provider 管理。
- Instance 管理。
- 日志/节省率 Dashboard。

### M4：多实例与 cc-switch 联动

- Codex 双实例。
- d-token 双代理。
- cc-switch bridge。
- cockpit-tools 兼容/导入层。

---

## 10. 成功指标

- 用户能在 5 分钟内完成 Claude 或 Codex 的代理配置。
- 第三方 provider 不再误发官方 Anthropic/OpenAI。
- 请求日志 100% 可见。
- Windows/macOS 下端口、路径、编码问题有明确处理。
- Codex 官方实例与中转实例可并行运行。
- 压缩后 token 节省率可观测，且能追溯原文。
