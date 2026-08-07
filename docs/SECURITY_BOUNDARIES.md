# Local-first 安全边界

状态：Accepted  
关联：`docs/PRD.md`

## 1. 安全原则

d-token 是本地开发工具，不是云端账号托管服务，也不是远程 relay 服务。

默认原则：

1. 默认只监听 `127.0.0.1`。
2. 默认不开放公网端口。
3. 默认不上传用户代码、日志、prompt。
4. 默认本地保存配置和统计。
5. 敏感字段默认脱敏。
6. 所有 provider/route/instance 行为必须可见、可解释、可审计。

## 2. 明确不做

1. 不做云端账号托管。
2. 不做账号共享、账号池、转售或代登录。
3. 不做违反平台条款的自动化用途。
4. 不偷偷读取或迁移用户已有 token。
5. 不把用户代码、日志、prompt 上传到 d-token 服务器。
6. 不做公网 relay 服务。
7. 不做静默 provider fallback。
8. 不隐藏上游错误。
9. 不默认持久保留完整请求正文。

## 3. 凭据处理

### 3.1 开发期

开发期可以使用：

1. 环境变量。
2. 本地 `.env.local`。
3. 本地 `providers.local.yaml`。

但这些文件必须 gitignored。

### 3.2 产品期

产品期优先使用：

1. Windows Credential Manager。
2. macOS Keychain。
3. Tauri Stronghold 或同等级本地密钥方案。

### 3.3 显示规则

Dashboard 中永远不明文展示：

1. API key。
2. Authorization header。
3. Cookie。
4. Refresh token。
5. Session token。
6. Local auth file content。

## 4. 日志规则

日志默认记录：

1. request id。
2. timestamp。
3. agent。
4. instance id。
5. provider id。
6. route id。
7. model。
8. token counts。
9. latency。
10. transform names。
11. error type。

日志默认不记录：

1. 完整 API key。
2. 完整 Cookie。
3. 完整 Authorization。
4. 大段用户代码。
5. 大段 prompt。
6. 大段 tool result。

正式产品不提供把完整 prompt、response、代码或 tool output 写入 ordinary debug log 的模式。调试只能增加结构化 stage/reason/size/digest 等脱敏字段；可逆原文仅允许进入明确启用的 CCR/private recovery store，并受独立权限、TTL、容量、审计和精确删除约束。

## 5. 网络边界

默认监听：

```text
127.0.0.1
```

默认不允许：

```text
0.0.0.0
公网 IP
局域网自动暴露
```

如果未来需要局域网模式，必须单独设计：

1. UI 显式警告。
2. 密码或 token 保护。
3. CORS 限制。
4. 访问日志。
5. 明确开关。

## 6. Route 安全

1. 每条 route 必须显式配置 target provider。
2. 不允许配置缺失时自动猜测 provider。
3. 不允许 silent fallback。
4. fallback 必须显示在 Dashboard。
5. route decision 必须进入 request log。
6. chain bridge 必须显示每段链路状态。

## 7. Sidecar 安全

1. sidecar 必须有唯一 instance id。
2. sidecar 必须有 healthcheck。
3. sidecar 必须有 log path。
4. sidecar 必须可停止、可重启。
5. sidecar 异常退出必须可见。
6. sidecar 不允许长期无记录后台残留。

### 7.1 Codebase Memory 额外边界

1. `codebase-memory-mcp` 不进入 d-token App bundle。App 只打开当前 manifest 指向的官方 release，由用户自行下载并解压到默认或自定义目录；d-token 不代为联网下载、安装或更新，也不接受可控下载 URL。只有当前 manifest/allowlist 绑定的官方来源、版本、平台、架构和 SHA-256 全部通过才可执行；目录内任意其他 binary 必须 fail closed。应用更新时更新 manifest/allowlist；非常老或协议/配置方式不兼容的版本仍然阻止。
2. Agent-owned MCP 会话与 d-token-owned index/UI host 必须是两类独立实例；d-token 不得接管 Agent 的 stdio、PID 或停止权限。
3. launcher 必须强制 `persistence=false`、项目 canonical root allowlist、私有 cache namespace、受限 tool allowlist，并拒绝跨项目查询、ADR write、artifact export 与任意路径访问。
4. d-token 只允许打开自己生成并复检过的短期授权 loopback graph URL；不得直接暴露 upstream 原生 HTTP UI。
5. 自动扫描 Agent 只读取白名单安装证据；批量配置仍必须逐 Agent diff/backup/confirm/verify/receipt，任一失败逆序回退，不得扩大为批量读取 auth、凭据或会话正文。

## 8. Doctor 与回退动作安全

1. Doctor 扫描只读取本地服务清单、PID/端口/可执行文件身份、loopback health、Agent 检测、活动配置、路由验证和备份元数据；不得发起 Provider 计费请求，不得读取或返回凭据、auth 文件、完整 prompt/response。
2. 低风险修复也必须先生成绑定 Doctor revision 的计划并由用户确认。当前直接写 executor 只开放身份完整验证的 d-token-owned 服务，以及已有 d-token 管理证据的 Claude Code/Codex/OpenCode Agent Integration；只读刷新、重新检测和其他专项模块必须逐项显式接 executor 后才能开放。
3. 停止或替换进程前必须同时验证 manifest schema、PID 存活与出生标识、manifest identity、可执行文件 hash/路径、监听端口所有权、runtime identity/version 和 instance nonce。任一证据缺失时保护性阻止；legacy manifest 不允许自动重启，不得自动停止外部或 unmanaged 进程。
4. Agent 配置、Hook、全局/项目文档必须复用 Agent Integration 的脱敏 Diff、逐 Agent backup/transaction、明确确认、写后验证和 rollback evidence；Provider/upstream、cc-switch profile 和任意首次 Agent 接入不属于 Doctor 自动修复。Doctor 不修改 cc-switch，也不把 Hook/Skill/MCP 存在等同于压缩生效。
5. 回退预览必须分别返回 `integrity_verified` 与 `currently_restorable`，并以当前 SHA-256 和 preview revision 绑定确认。损坏备份、未知/legacy scope、并发修改或事务 manifest 不完整时必须在写入前拒绝。
6. 执行回退前必须先为当前状态创建反向安全备份，并将其绑定到回退后的精确 hash 或 `missing` 状态，保证反向恢复点本身可再次预检和使用；恢复后至少验证目标文件精确 hash。涉及 managed runtime 时按需 restart/reload 并区分 `verified`、`runtime_offline` 和 `failed`，不得把“文件已恢复”直接宣称为“运行时已恢复”。
7. 多文件 Agent integration、Optical 和其他专项事务必须由来源 transaction manifest/executor 整笔回退；通用页面不得摊平为独立文件后绕过原子性、服务 reload 或专项复检。

## 9. Hybrid Context Optimization 安全

1. 当前轮、近期对话、system/developer、tool schema、tool id/arguments、开放工具状态、代码/diff、secret/exact 内容不得只存在于图像。
2. Optical active 默认关闭，只允许同协议 Anthropic closed historical low-risk tool result；未知模型、跨协议或身份不匹配必须 fail closed。
3. active authority 必须是 Ed25519 签名 Capability，精确绑定 provider、route、model、endpoint、renderer profile、font、eval artifact 和有效期。
4. renderer 必须执行 deadline、取消、并发、候选字符、页数、像素、PNG、编码后图片、outbound body 与内存预算。
5. patcher 只能消费 sealed approved target；post-transform validator 必须从实际图片 envelope/base64/PNG/digest 推导证据，并证明 approved path 外 JSON 完全不变。
6. shadow、passthrough 与 pre-send fallback 使用原始请求 bytes；upstream send commit 后禁止自动原文重试，避免双计费和重复工具副作用。
7. Ledger、日志、metrics、Doctor 与导出只记录脱敏元数据，不记录 prompt、tool result、PNG、编码图片或密钥材料。

## 10. 数据保留

默认保留：

1. 汇总指标。
2. token savings ledger。
3. route decision metadata。
4. error metadata。

默认不长期保留：

1. 完整 prompt。
2. 完整代码文件。
3. 完整 tool output。
4. 完整 response body。

产品不提供普通“完整保留”开关。需要可逆恢复时只允许使用默认关闭的 CCR/private recovery store：本地加密、明确存储位置、OS credential key、TTL/容量上限、项目/会话授权、访问审计、过期与一键精确删除；普通日志、Ledger、SQLite query index、Dashboard 与 export 永远不包含该原文。

## 11. Headroom-derived Pipeline 安全状态机

1. release 未显式设置 profile 时必须是 native/unavailable，禁止默认 fixture。
2. Agent/project/session/route/provider/model 只能来自解析证据；未知时记录 unknown reason，禁止填测试身份。
3. 未获得 durable CCR prepare receipt 时禁止注入 recovery marker/tool reference。
4. 未通过 protocol post-validation 的 artifact 不能进入 `ReadyToSend`。
5. cache stability、节省、成本与 capability 必须由后端证据计算，禁止调用方布尔自报。
6. upstream send commit/stream started 后禁止原文重试或切换 provider。
7. memory/learning 只接收允许的结构化脱敏结果；未授权会话和项目不可扫描、索引或跨 Agent 共享。
8. optional model/ONNX/embedding/telemetry 默认关闭，必须具备 hash/license/resource/privacy/quality evidence 才可启用。

## 12. Hybrid 与 Agent bridge 加固边界

1. `SecretClassifier` 是 inventory、risk、factsheet、renderer 与配置 diff 的唯一共享 secret 判定入口；provider key、JWT、PEM、cookie/session、Authorization、label-wrapped 与 Unicode/转义变体命中后整块保留文本，禁止进入 PNG、factsheet、普通 diff 或 Ledger。
2. renderer 临时像素、最终 PNG、base64、patched body 与 retained transport clone 共享同一请求级 in-flight byte permit；deadline 也为请求级共享，禁止各阶段独立预算叠加突破上限。
3. Agent integration transaction manifest 必须原子持久化并用 transaction ID、review digest、目标顺序和 planned-after hash 绑定 verify/rollback；跨进程锁失败或证据缺失时 fail closed。
4. `dtoken-agent-bridge` 在任何子进程执行前先验证 release 平台签名；随后才允许读取 metadata-only identity。Hook/MCP 输入使用有界读取，不读取 transcript/raw prompt，不落盘原始请求。
5. macOS 本机只证明当前平台代码与 dry-run；Windows 只有编译、PowerShell/workflow 合同证据时不得写成 Windows 实机通过。

## 13. 合规实现原则

1. d-token 只做用户本地开发环境中的压缩、路由、观测和配置管理。
2. d-token 不替用户规避第三方平台条款。
3. d-token 不提供账号托管或共享能力。
4. d-token 不帮助隐藏真实使用来源。
5. d-token 的 Dashboard 要透明展示 route 和 provider，而不是隐藏链路。
