# d-token Living PRD

## P0 发布版产品决策：从当前版本到稳定发布

状态：2026-07-08 用户已确认的最新产品口径。本节优先级高于旧页面草案；旧技术链路、runtime 契约、测试契约继续作为实现约束保留。

### P0.1 产品形态

第一版 d-token 采用“普通用户友好 + 高级能力折叠”的混合型产品形态。

默认体验必须像一个能直接使用的本地桌面 App：用户打开后可以看到总体状态、启动服务、一键安装或配置本地 AI Agent、检查 cc-switch 与上游、查看用量分析、诊断问题、修复问题、查看日志、回退配置。

高级能力必须存在，但不能把普通用户直接推入工程语言。`runtime`、`route adapter`、`payload`、`Event Ledger`、`provider profile` 等概念只能在高级详情、开发日志或技术附录中出现。正式 UI 主文案应使用“本地服务”“连接路径”“模型服务商”“本地操作记录”“连接检查”“恢复配置”等用户能理解的表达。

### P0.2 发布目标

当前版本不按“内部 demo”标准验收，而按“可公开试用的稳定 Beta / 候选发布版”推进。发布前必须满足：

1. 所有主页面有完整 UI、完整中文文案、清晰空状态、加载状态、错误状态和成功状态。
2. 所有主流程功能可用，不允许只有静态展示或纯 mock 完成态。
3. 本地服务安装、启动、停止、状态检测、端口检测、Agent 检测、cc-switch 检测、配置预览、备份、应用、验证、日志、诊断、回退必须形成闭环。
4. 示例数据可以用于浏览器预览或开发预览，但必须明确标注“示例数据”；真实运行完成态必须来自 d-token 自有本地服务、Tauri command 或 d-token 自有本地记录。
5. UI 视觉目标是现代、干净、克制、可信、稳定，不能像开发调试面板，也不能像测试中心。
6. d-token 第一版只安装和管理 d-token 自有本地服务；对 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode、cc-switch 等第三方工具只做检测、读取配置、生成配置计划和受控配置写入，不自动安装第三方工具。

### P0.3 核心价值排序

第一优先级：稳定连接与一键配置。用户能把本地 AI Coding Agent 接入 d-token，并确认请求是否经过 d-token。

第二优先级：本地可控、安全、可回滚。任何真实写配置、改 hook、改 provider、改端口、改 cc-switch 连接方式之前，都必须展示人话摘要、详细 diff、备份位置、风险提示和恢复方式。

第三优先级：Token 节省与智能学习。压缩、学习、项目文档注入、全局 Agent 文档优化必须帮助用户减少重复上下文、提升 Agent 理解项目的能力，但不能默认暴露原始 prompt、原始代码、会话正文或密钥。

第四优先级：问题诊断与自动修复。Doctor 必须能检测已知问题，低风险问题可以自动修复，高风险问题必须预览变更并由用户确认。

### P0.4 Agent 支持策略

对外产品口径：d-token 支持多种 AI Coding Agent，只要该 Agent 能通过配置文件、环境变量、hook、代理端口或本地上游地址接入，就应进入 d-token 的统一接入模型。

第一版支持范围必须以“能力状态”展示，而不是简单宣传“全部已完成”。每个 Agent 至少有以下状态之一：

1. 已可一键配置。
2. 可检测，需用户确认配置。
3. 可生成提示词，由用户复制给自己的 Agent 执行配置。
4. 仅支持高级手动配置。
5. 暂未支持。

目标 Agent 包括但不限于：Claude Code、Claude Desktop、Codex、OpenCode、OpenClaw、Gemini CLI、Hermes、Cursor、Cline/Roo、Continue、Aider、Goose、OpenHands、Copilot CLI、Windsurf、Kilo Code、Google Antigravity、Mistral Vibe 等。

第一版必须重点验收的 Agent 是：Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode。CommandCode 暂按用户指定名称进入目标清单；实现前需要补齐具体命令名、配置路径、hook 方式和上游配置格式。

OpenCode 仍然可以作为当前真实验收链路的优先入口，但 UI 信息架构不能以 OpenCode 为唯一中心。所有 Agent 必须复用同一套“检测 → 配置 → 预览 → 备份 → 应用 → 验证 → 日志/诊断/回退”的产品模型。

### P0.5 主页面信息架构

第一版主页面压缩为 5 个主页面 + 底部设置入口：

1. 总体概览。
2. 安装与连接工作台。
3. 分析页面。
4. Doctor 问题诊断与修复。
5. 变更与回退。
6. 设置。

如果代码层仍保留 `Overview / Install / Advanced / Analytics / Doctor / Changes / LogsSecurity / Compatibility / Settings` 等旧页面名，UI 层必须按照本节重新组织中文显示名、页面职责和用户路径。旧页面可以作为实现组件来源，但不能继续以旧工程语义组织正式产品。

### P0.6 第一页：总体概览

总体概览是用户打开 d-token 后看到的第一面。它不承担复杂配置，只回答：

1. d-token 本地服务是否正在运行。
2. 已连接哪些 Agent。
3. 当前请求是否经过 d-token。
4. cc-switch 或自定义上游是否被检测到。
5. 今日/近 7 日估算节省了多少 token。
6. 最近是否有错误、断连、端口冲突、配置异常。
7. 用户下一步应该做什么。

空白状态必须是“空白状态 + 友好提示”，而不是冷错误。例如：

> d-token 还没有开始工作。启动本地服务后，你可以连接本机 AI 编码工具，并在这里看到连接状态、节省情况和最近问题。

主按钮建议为：

1. 启动本地服务。
2. 前往安装与连接。
3. 打开 Doctor。

总体概览必须标注数据来源：真实数据来自 d-token 本地服务；示例数据只用于预览；暂无数据说明服务未启动或还没有请求记录。

### P0.7 第二页：安装与连接工作台

安装与连接工作台是第一版最重要页面，承担“一键安装服务 + Agent 配置 + 智能学习 + 高级安装 + 日志”的核心任务。

该页面顶部使用上栏标签组织为 4 个区域：

1. 一键安装。
2. 智能学习。
3. 高级安装。
4. 日志。

#### P0.7.1 一键安装

一键安装区域负责把 d-token 从“未配置”推进到“真实可用”。核心流程：

```text
检测本地环境
  ↓
安装或启动 d-token 本地服务
  ↓
检测端口占用与可用端口
  ↓
检测 cc-switch 配置文件夹与配置文件
  ↓
列出 cc-switch 中所有配置文件 / profile / provider / upstream
  ↓
自动识别上游类型、协议、base URL、端口、是否需要路由、是否需要端口转发
  ↓
检测本地 Agent 与可配置方式
  ↓
为用户选择的 Agent 生成配置计划
  ↓
展示人话摘要 + 详细 diff + 备份位置
  ↓
用户确认后应用配置
  ↓
验证连接是否生效
  ↓
写入日志与回退点
```

cc-switch 检测要求：

1. 自动定位 cc-switch 常见配置目录。
2. 列出检测到的所有配置文件，不显示密钥原文。
3. 自动识别每个配置对应的上游类型：官方上游、第三方中转、OpenAI-compatible、Anthropic-compatible、本地 provider、未知上游。
4. 检查 cc-switch 是否已经运行、需要启动、是否存在端口冲突。
5. 检查当前 Agent 是否需要先转发到 d-token，再由 d-token 转发到 cc-switch。
6. 检查是否需要端口转发、协议转换或 chain bridge。
7. 对无法确认的配置显示“需要用户确认”，不能静默猜测。
8. d-token 对 cc-switch 默认只读：只读取用户配置文件、profile、provider、upstream、端口和运行状态，不自动安装 cc-switch，不静默改写 cc-switch 配置。
9. 如果某个 Agent 当前不是官方上游配置，而是通过 cc-switch 或用户自定义配置走第三方上游，d-token 必须读取 cc-switch 中对应用户配置文件，并据此配置 d-token 的上游与连接路径。
10. 用户在 cc-switch 中切换配置/profile 时，d-token 必须能自动感知或在刷新后同步切换对应上游，避免 d-token 仍然指向旧上游。
11. cc-switch 自动同步是关键功能：UI 必须明确显示“当前 cc-switch profile / 当前上游 / d-token 当前连接路径 / 是否已同步”。

Agent 检测要求：

1. 检测本机已安装 Agent。
2. 检测每个 Agent 的配置文件位置、hook 能力、环境变量、可用命令、版本、当前上游。
3. 显示“可一键配置 / 可生成配置建议 / 仅高级手动配置 / 暂不可配置”。
4. 配置前必须备份，配置后必须验证。
5. 不允许在 UI 或日志中显示完整 API Key、认证 token、cookie、完整 prompt、完整 response。

#### P0.7.2 智能学习

智能学习是 d-token 相比普通路由/配置工具的重要创新功能。它参考 Headroom 的项目理解、learn、prompt/document 注入思路，但必须形成 d-token 自己的产品形态。

智能学习目标：帮助 d-token 和用户的 Agent 更快理解项目、减少重复解释、自动生成或更新项目级/全局级 Agent 说明文档。

核心流程：

```text
全面扫描本机可能包含 Agent 会话或项目上下文的工作文件夹
  ↓
快速识别所有 Agent 的会话来源、项目目录、最近活跃度和可学习价值
  ↓
用快速、准确的排序算法按项目价值、活跃度、Agent 关联度、最近会话强度从高到低排序
  ↓
用户搜索、筛选、勾选需要重点智能识别的文件夹
  ↓
使用 Headroom-derived / d-token-owned 学习方式对候选目录做快速索引，对用户重点选择的文件夹做深度扫描
  ↓
生成项目理解、约定、架构摘要、常用命令、风险点、Agent 协作建议
  ↓
用户选择执行方式：直接委托 Claude/Codex 写入，或生成提示词交给自己的 Agent 执行
  ↓
直接写入项目或全局 Agent 文档的受控追加块 / 插入块，不重写、不删除、不改动文档原有内容
  ↓
完成后进入日志与回退记录
```

智能学习必须支持两种执行模式：

1. 直接委托模式：用户选择 Claude 或 Codex，由 d-token 注入提示词并委托其理解项目，随后只以受控追加块 / 插入块方式写入项目或全局文档。
2. 提示词模式：d-token 只生成高质量提示词，用户复制给自己的 Agent 执行配置或文档补充。
3. 无论哪种模式，智能学习都不允许重写整份文档，不允许删除用户原有内容，不允许改动无关段落。

智能学习进度条必须使用用户能理解的阶段：

1. 准备委托。
2. 正在识别文件夹。
3. 正在理解项目。
4. 正在生成建议。
5. 正在写入文档。
6. 正在验证结果。
7. 完成。

可写入的文档类型包括：

1. 项目级 `CLAUDE.md`。
2. 项目级 `AGENTS.md`。
3. 项目级 `GEMINI.md`。
4. 项目级 d-token 学习摘要。
5. 全局 Agent/Claude/Codex 说明文档。
6. d-token 自有项目策略文件。

智能学习安全边界：

1. 智能学习允许并且必须进行全面扫描：自动发现本机 Agent 会话目录、项目目录、最近活跃工作区和高价值上下文来源。
2. 全面扫描必须快速准确：先做轻量索引和置信度排序，再对高价值目录做深度识别，避免长时间无反馈。
3. UI 必须展示扫描进度、已识别目录数量、排序依据、可学习价值和用户可控的停止/跳过入口。
4. 扫描不等于上传：默认本地完成识别、摘要和索引，不默认上传完整代码、完整会话、密钥或认证文件。
5. 对 `.env`、密钥文件、认证目录、token、cookie、私密会话必须默认排除或脱敏。
6. 写入文档时可以直接写入，但只能插入或追加 d-token 管理的内容块，不允许删除、重排、覆盖、重写用户原有内容。
7. 每一次写入都必须生成备份和回退点，并写入变更与日志。

#### P0.7.3 高级安装

高级安装面向熟悉 Agent、上游、端口和路由的用户。它不能替代一键安装，但必须允许用户完全自定义：

1. Agent 使用哪个上游。
2. Agent 是否先经过 d-token。
3. d-token 是否再转发到 cc-switch。
4. cc-switch 使用哪个 profile/config。
5. d-token 本地服务端口。
6. cc-switch 端口。
7. provider 协议：OpenAI-compatible、Anthropic-compatible、本地 provider、未知/自定义。
8. 是否启用端口转发或 chain bridge。
9. fallback 策略。
10. 超时、重试、健康检查、日志级别。

高级安装必须自动显示检测到的端口、上游、配置文件和风险，但不能在用户未确认时自动修改。

#### P0.7.4 日志

安装与连接工作台内的日志上栏只展示与“安装、连接、配置、智能学习、高级安装”相关的过程日志，不替代全局日志系统。

日志必须包含：

1. 检测了什么。
2. 计划修改什么。
3. 备份到哪里。
4. 应用了什么配置。
5. 哪一步失败。
6. 失败原因和下一步建议。
7. 可复制给 Agent 的修复提示词。

日志必须脱敏，不显示完整密钥、token、cookie、认证头、完整 prompt、完整 response 或用户未授权展示的代码内容。

### P0.8 第三页：分析页面

分析页面负责展示 d-token 的价值结果，而不是工程指标堆砌。

核心信息：

1. 估算节省 token。
2. 请求数量。
3. 压缩前后对比。
4. 最近 7 日 / 30 日趋势。
5. Agent 使用分布。
6. provider / cc-switch / 上游分布。
7. 错误率与失败原因分布。
8. 高价值项目或高消耗项目。
9. 智能学习带来的项目文档覆盖情况。

当节省数据不是精确计费数据时，必须写“估算节省”，并说明基于本地请求记录、压缩记录和路由记录计算。

### P0.9 第四页：Doctor 问题诊断与修复

Doctor 是问题修复中心，不是单纯系统检查面板。

Doctor 必须检测已知问题，并给出：

1. 问题标题。
2. 严重程度。
3. 影响范围。
4. 可能原因。
5. 自动修复按钮或手动修复步骤。
6. 可复制给 Agent 的修复提示词。
7. 相关日志入口。
8. 相关回退点。

Doctor 支持低风险自动修复：

1. 重启 d-token 本地服务。
2. 重新检测端口。
3. 重新检测 Agent。
4. 刷新 cc-switch 状态。
5. 重新验证配置。
6. 清理无效临时状态。

Doctor 对高风险修复必须要求确认：

1. 修改 Agent 配置。
2. 修改 hook。
3. 修改 provider / upstream。
4. 修改 cc-switch profile。
5. 修改全局 Agent 文档。
6. 执行回退。

Doctor 还必须能生成“修复提示词”，让用户复制给 Claude/Codex/OpenCode 等 Agent 协助修复项目配置或解释错误。

### P0.10 第五页：变更与回退

变更与回退页面负责让用户知道 d-token 改过什么，并能安全恢复。

必须展示：

1. 变更时间。
2. 变更来源：一键安装、智能学习、高级安装、Doctor 修复、手动设置。
3. 影响对象：Agent 配置、cc-switch 配置、provider、端口、项目文档、全局文档、d-token 配置。
4. 人话摘要。
5. 详细 diff。
6. 备份位置。
7. 验证结果。
8. 一键恢复。
9. 恢复后的验证结果。

回退页面必须支持智能学习写入文档的回退，不能只支持 route/provider 配置回退。

### P0.11 设置页面

设置页面从底部按钮进入，负责偏好和安全，不作为高级配置大杂烩。

第一版设置包括：

1. 语言。
2. 主题。
3. 本地服务端口。
4. 数据保留时间。
5. 日志脱敏策略。
6. 智能学习全面扫描开关、扫描强度、索引频率和扫描范围管理。
7. 默认排除目录与敏感文件规则。
8. 自动启动 d-token 服务。
9. 更新通道。
10. 高级/开发模式开关。

### P0.12 文案总规范

所有页面必须通过三个问题验收：

1. 我现在在哪里。
2. 这个页面能帮我做什么。
3. 下一步我该点哪里。

禁用或减少以下面向普通用户的直接表达：

1. payload。
2. fixture。
3. benchmark。
4. smoke test。
5. runtime payload unavailable。
6. route adapter failed。
7. event ledger append failed。
8. headroom external dashboard。
9. fake binary。
10. test harness。

推荐替换为：

1. 本地服务暂时没有返回数据。
2. 这是示例数据，启动服务后会显示真实记录。
3. 连接检查未通过。
4. 请求没有进入 d-token。
5. 本地操作记录写入失败。
6. 配置已备份，可以恢复。
7. 需要你确认上游地址。
8. 端口被占用，请换一个端口或关闭占用程序。
9. 已生成修复提示词，可以复制给你的 Agent。

### P0.13 从现在到发布的实现计划

#### 阶段 1：PRD 与信息架构落地

1. 按本节重构 PRD。
2. 固定主页面结构：总体概览、安装与连接工作台、分析、Doctor、变更与回退、设置。
3. 明确第二页四个上栏：一键安装、智能学习、高级安装、日志。
4. 输出每页 UI 文案清单、空状态、错误状态、成功状态。

验收：产品语义清楚，不再以开发测试、benchmark、fixture 作为正式 UI 主流程。

#### 阶段 2：一键安装与 cc-switch / Agent 检测闭环

1. 实现 d-token 自有本地服务安装/启动/停止/状态检查。
2. 对第三方工具只做检测，不自动安装 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode 或 cc-switch。
3. 实现端口检测和端口冲突提示。
4. 实现 cc-switch 配置目录探测、配置文件列表、上游识别、端口识别、运行状态识别。
5. 实现 cc-switch 当前 profile/config 读取，并在用户切换 cc-switch 配置时自动同步 d-token 上游。
6. 实现 Agent 检测、配置能力分级、hook/配置文件/环境变量识别。
7. 实现配置计划、diff、备份、应用、验证、回退点写入。

验收：用户能在第二页完成 d-token 服务启动、cc-switch 只读检查、cc-switch 上游同步、Agent 检测和至少一种真实 Agent 配置闭环。

#### 阶段 3：智能学习可用

1. 实现 Agent 会话/项目工作目录全面扫描。
2. 实现快速索引、置信度评分、活跃度评分、会话价值评分和项目关联度排序。
3. 实现搜索、筛选、勾选重点文件夹，并允许用户停止、跳过或重新扫描。
4. 实现敏感文件排除、脱敏、扫描进度展示和本地索引缓存。
5. 实现学习结果生成。
6. 实现 Claude/Codex 委托模式。
7. 实现提示词模式。
8. 实现项目级和全局文档的直接写入，但只能追加或插入 d-token 管理块，不允许改动其他内容。
9. 实现所有写入的备份、回退和变更记录。
10. 实现进度条阶段：准备委托、识别文件夹、理解项目、生成建议、写入文档、验证结果、完成。

验收：系统能全面快速扫描真实项目与 Agent 会话目录，生成可用学习结果，并以受控追加/插入方式安全写入或生成提示词。

#### 阶段 4：分析、Doctor、日志、回退完善

1. 分析页接入真实请求、压缩、路由、错误、Agent 使用数据。
2. Doctor 接入已知问题库与自动修复动作。
3. Doctor 支持生成修复日志和 Agent 修复提示词。
4. 变更与回退支持所有配置变更和文档写入变更。
5. 全局日志脱敏、筛选、导出。

验收：服务异常、配置异常、端口冲突、cc-switch 异常、Agent 配置异常、智能学习写入异常均可诊断、解释、修复或回退。

#### 阶段 5：UI 完整度与稳定性

1. 全部页面中文文案重写。
2. 全部页面 loading / empty / error / success 状态补齐。
3. 所有危险操作加确认、diff、backup、rollback。
4. 所有真实数据标注来源。
5. 所有示例数据明确标注。
6. Windows/macOS 路径、端口、权限、编码、shell 差异测试。
7. 前端 build、后端测试、手工桌面验收、真实链路验收全部通过。

验收：所有功能可用、UI 完整、文案友好、功能稳定，可以进入公开试用发布。

## 0.0 当前最高优先级链路定义

d-token 的目标不是“外部 Headroom Dashboard 的套壳”，也不是“Desktop 直接读取一个已经运行的 Headroom 服务”。正确产品链路是：

```text
OpenCode / Claude Code / Codex / Cursor / Cline / other AI coding agent
↓
d-token local entry / managed wrap
↓
d-token 本地服务
= Headroom 能力移植、吸收、改造后的 d-token-owned runtime
= compression / CCR / memory / learn / output shaping / metrics / stats / history / dashboard primitives
= integrated RTK internal capability
↓
d-token route adapter
↓
官方上游 / 第三方中转上游 / cc-switch route interface / local provider
↓
d-token Event Ledger / Dashboard / Doctor / Logs / App
```

本链路具有最高解释权：

1. `D:\D-TOKEN\headroom` 是源码参考、能力迁移来源和兼容行为参考，不是 d-token App 的运行时数据源。
2. 已存在的外部 Headroom 服务，例如 `http://127.0.0.1:8787/dashboard`，只能作为迁移对照和 no-touch 只读参考；不能作为成品 App Dashboard 的依赖。
3. d-token 必须拥有自己的本地服务、端口、stats/history/metrics、Event Ledger、ProviderProfile、RouteProfile、InstanceProfile 和 Dashboard 数据模型。
4. Headroom Dashboard 的所有数据与页面能力要迁移到 d-token App，但数据应来自 d-token-owned runtime，而不是直接轮询外部 Headroom Dashboard。
5. OpenCode 是第一条真实测试入口：测试请求必须从 OpenCode 打到 d-token，再由 d-token 完成压缩、路由和上游转发；OpenCode 测试已获授权直接使用用户本机原本配置中的真实 provider/model，不强制使用官方免费模型，也不做隔离配置。
6. 正确测试链路是 `OpenCode -> d-token local service -> d-token Headroom-derived runtime -> d-token route adapter -> official/custom/cc-switch upstream`。
7. 任何实现如果把链路写成 `App -> external Headroom 8787 -> Dashboard data`，都是错误方向。
8. 开发期可用 external Headroom 做只读对照，但不得启动、停止、重启、改配置、占端口或依赖其作为产品主路径。
9. OpenCode 真实测试已获用户授权直接使用用户原本 OpenCode 配置和真实 provider/model；默认不隔离 HOME、不替换 fake config、不另造临时 OpenCode 配置。执行 OpenCode real routed / workload test 时，默认直接调用本机 `opencode` 和原配置，不需要再退回隔离测试。测试目标是证明真实 OpenCode 流量经过 d-token local service。
10. OpenCode 是当前阶段唯一允许使用真实用户 agent 配置进行测试的入口；该例外不得扩展到 Claude Code、Codex、Cursor、Cline、Roo、Continue、Windsurf、系统代理或 shell profile。
11. 若为了验证真实 routed path 需要给 OpenCode 配置增加 d-token provider/route，必须先备份原配置，只做最小变更，记录 backup 路径和 diff 摘要；禁止读取、打印、持久化或提交 OpenCode 密钥和会话凭据。
12. 开发测试不能产品化成正式 UI：真实测试、workload benchmark、smoke test、fixture/mock/fake-bin 结果属于开发脚本、CI gate、测试报告和内部诊断证据，不能作为最终用户主界面里的“测试卡片”或“测试中心”长期保留。正式 App UI 只面向用户任务：安装/启用 d-token 服务、选择 agent、配置 provider/route/cc-switch、查看运行状态、Doctor 诊断修复、Dashboard/Logs/Export。
13. 最终 App 必须兼容所有目标 agent；OpenCode 只是当前第一条真实验收入口，不代表产品只服务 OpenCode，也不能把 OpenCode 测试逻辑硬编码成最终信息架构。当前阶段的真实验收口径应是：安装/启动 d-token-owned 服务后，在正式安装/配置流程中选择 OpenCode，点击 OpenCode 的安装/配置/连接动作，随后由开发侧脚本或 CI 证明请求确实经过 d-token。
14. 任何 UI、安装流、Dashboard、Doctor 或测试入口设计前，必须先按产品经理视角判断用户任务：用户是在“安装/连接/配置/诊断/查看状态”，不是在“跑测试”。只有完成这个产品语义翻译后，才能落到代码实现。
15. 新增或重做 App 流程时，必须先参考成熟桌面控制面产品的应用逻辑，例如连接状态、配置入口、诊断分区、日志导出、备份回滚，而不是把开发脚本、benchmark 或 smoke test 直接搬进正式 UI。
16. 浏览器、桌面窗口或截图验收只能证明界面显示正常；不得用它替代真实功能接入证明。真实功能接入必须同时看到 d-token-owned local service、route adapter、Event Ledger、metrics/export 和 OpenCode routed flow 等证据。

### 0.0.1 当前实现契约

1. d-token 本地服务默认绑定 `127.0.0.1`，当前开发端口以 d-token 自有端口为准，不能占用或依赖外部 Headroom `8787`。
2. App Dashboard、Doctor、Logs、Analytics 读取 d-token 自己的 API / Tauri command / Event Ledger；禁止把外部 Headroom Dashboard JSON 当作产品数据源。
3. Headroom 迁移工作以能力迁移为准：先迁移 proxy/stat/history/dashboard primitive，再逐步迁移 compression、CCR、memory、learn、output shaping、integrated RTK 指标桥接。
4. Route adapter 必须同时面向官方上游、第三方 OpenAI-compatible / Anthropic-compatible 上游、cc-switch route interface、本地 provider。
5. 所有 route decision 必须写入 Event Ledger，至少包含 agent、route、provider、input protocol、output protocol、chain mode、fallback policy、latency、token in/out、compression result。
6. OpenCode 测试必须证明真实请求经过 d-token local service，不允许只证明 OpenCode CLI 自己能请求官方模型。
7. 测试报告必须包含功能测试、路由测试、压缩率、压缩耗时、端到端延迟、错误分类、日志脱敏、Dashboard 数据一致性。
8. 当前 d-token runtime 必须把结构化运行数据写入 d-token-owned persistence：`events.jsonl`、`route-decisions.jsonl`、`request-logs.jsonl`。这些文件只允许保存聚合/路由/计量/错误信息，不保存 raw prompt、raw response、CCR 原文或真实密钥。
9. 当前 d-token runtime 必须支持 Anthropic-shaped `/v1/messages` 输入显式桥接到已配置的 OpenAI-compatible / cc-switch-style provider；禁止在 route 缺失时自动回退到官方 Anthropic。
10. 当前 d-token runtime 必须提供 d-token-owned CCR metadata 层：`ccr-metadata.jsonl`、`/ccr/summary`、`/ccr/retrieve/{recovery_ref}`。默认只返回 recovery metadata，`raw_available=false`，不得保存或返回 raw prompt、raw response、CCR 原文。
11. 当前 d-token runtime 必须提供 d-token-owned memory/learn/output shaping 元数据层：`memory-records.jsonl`、`/memory/learn`、`/memory/summary`、`/output-shape`。只保存用户显式提交的摘要、项目、agent、verbosity 等元数据，必须剔除 raw prompt、raw response、messages、body、content。
12. 当前 OpenCode 验证以真实 routed path 为主：`OpenCode 原配置 -> d-token provider/baseURL -> d-token :18001 -> configured upstream`。除非用户另行要求，OpenCode routed test 不使用隔离 HOME、临时 OpenCode 配置、临时 provider 或 fake provider。早期 direct command / fake-bin 测试只能作为诊断辅助，不能作为真实接入完成证据。
13. 当前 UI 实现契约：安装页可以有 OpenCode 的正式安装/配置入口，但不得以“运行真实链路测试”“workload benchmark”“能力自检测试”作为面向最终用户的主操作。开发验收可以通过隐藏 dev mode、脚本、CI 或测试报告触发同一后端能力；正式 UI 文案必须转换为用户可理解的“连接 OpenCode”“配置路由”“验证配置”“查看诊断结果”。
14. 当前前端所有真实运行数据来源必须指向 d-token 自己的服务或 Tauri command 汇总结果；Fixture、browser preview、mock、外部 Headroom 只可作为开发预览/对照，不能成为“已接入真实能力”的判定依据。
15. 当前 OpenCode 安装/配置验收必须模拟真实用户路径：选择 OpenCode agent，应用经过备份的最小配置 diff，验证请求进入 d-token Event Ledger，再回到 Dashboard/Logs/Doctor 展示脱敏状态。不得把该验收设计成面向最终用户的“OpenCode 测试页”。

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 0. 当前基线决策：d-token App + Headroom-derived Runtime + Integrated RTK

### 0.1 核心判断

`headroomlabs-ai/headroom` 已经包含并管理 RTK：Headroom wrap 链路会确保 RTK binary、注入 RTK instructions、启动 proxy 与 agent，并通过 `rtk gain --format json` 汇总 RTK 指标。因此 d-token 不再把 RTK 与 Headroom 视为两个默认平级、默认独立安装的外部系统。

新的产品基线是：

```text
第一版：d-token = Headroom 的可视化增强版 + 路由增强版
第二版及以后：d-token 逐步吸收、改造、创新 Headroom 核心能力，形成 d-token 自有 runtime

Headroom-derived runtime = compression core + proxy / SDK / MCP + agent wrap / unwrap + integrated RTK
d-token App              = Windows/macOS 图形化安装器、控制面、Provider/Route/cc-switch 编排、Doctor、Dashboard、可观测与回滚层
RTK                      = runtime 内部命令执行层能力，不作为用户侧独立产品路径暴露
```

### 0.2 最终用户安装模型

最终用户不应被要求先手动安装 Headroom 或 RTK。目标体验是：

```text
下载 d-token App
  ↓
打开图形化安装向导
  ↓
选择 agent / provider / cc-switch / 压缩策略
  ↓
d-token 自动安装或启用内置 runtime
  ↓
启动服务并在 Dashboard 看到链路、压缩、路由、错误和成本
```

开发期仍允许连接外部 Headroom 做 no-touch 验证；但成品视角下，Headroom 能力应被打包、吸收、修改并作为 d-token runtime 的组成部分提供。

### 0.3 不再保留 Standalone RTK 产品路径

1. d-token 不提供用户侧 Standalone RTK 模式。
2. d-token 不默认单独安装 RTK。
3. d-token 不要求用户理解或选择 RTK。
4. RTK 只作为 Headroom-derived runtime 内部能力出现。
5. Doctor 可以检测 standalone RTK 冲突，但只是为了避免重复 hook / PATH 污染，不作为推荐路径。
6. d-token 不把 RTK 放进 proxy hot path 当作 tool-result compressor。

### 0.4 产品创新点

1. 把 Headroom 的命令行安装、wrap、proxy、dashboard、doctor、learn、RTK 状态、provider 配置，做成 Windows/macOS 可视化 App。
2. 用户可以图形化选择安装/接入哪些 agent：Claude Code、Codex、Cursor、Cline、OpenCode、Aider、Continue、Goose、OpenHands、Copilot CLI 等。
3. 用户可以图形化选择官方上游、自定义第三方 OpenAI-compatible / Anthropic-compatible 上游、cc-switch / chain bridge 路由。
4. 支持直接压缩后发到自定义 URL：官方 URL、第三方中转 URL、本地 provider URL。
5. 支持需要二次路由的场景：先压缩，再转发到 cc-switch，再由 cc-switch 路由到最终 provider。
6. 典型场景：Claude Code 中使用 OpenCode Go 订阅，需要 cc-switch 路由；d-token 负责先压缩、再转发一次到 cc-switch。
7. 支持 Claude Code/Codex 等 agent 的协议差异、provider 差异、路由差异可视化，避免请求误走官方上游。
8. 支持中文界面和中文日志场景，Windows 上 UTF-8、盘符路径、项目名连字符、`.cmd` shim 等必须作为一等兼容目标。
9. 第一版做可视化增强 + 路由增强；第二版开始逐步吸收和创新 compression/runtime 核心。

## 1. 项目一句话

`d-token` 是一个面向 AI Coding 的本地优先桌面控制面与运行时：在 Windows/macOS 上把 Headroom-derived compression runtime、integrated RTK、agent wrap、provider route、cc-switch / chain bridge、project compression policy、Doctor、Dashboard、backup/rollback 与端到端 token savings attribution 合并成一个用户下载即用、可安装、可观测、可回滚的图形化 App。

## 2. 用户目标

目标用户包括：

1. 使用 Claude Code、Codex、Cursor、Cline、OpenCode 等 AI Coding 工具的开发者。
2. 希望减少 token 浪费、压缩日志/文件/tool output/命令输出的重度 AI Coding 用户。
3. 同时使用官方 provider 与第三方中转 provider 的用户。
4. 需要 cc-switch 或多段路由的用户。
5. 想使用 OpenCode Go、第三方中转、自定义 URL，但又需要 Claude/Codex 等 agent 兼容的人。
6. 不想手动配置 Headroom/RTK/hook/base URL/proxy/dashboard 的普通用户。
7. 在 Windows 上使用 Claude/Codex/Headroom/RTK 时遇到编码、路径、端口、shim 问题的用户。

用户最终应能完成：

```text
打开 d-token 桌面 App
  ↓
选择傻瓜模式或高级模式
  ↓
选择 agent
  ↓
选择压缩能力与项目策略
  ↓
选择 provider：官方 / 第三方自定义 URL / cc-switch / chain bridge
  ↓
预览 diff / 风险 / 回滚点
  ↓
一键安装或启用内置 runtime
  ↓
启动服务
  ↓
在 Dashboard 看到请求、压缩、RTK、路由、成本、错误和恢复建议
```

## 3. 开发期 no-touch 与成品 managed install

### 3.1 两条线必须分开

```text
开发测试线：No-touch Dev Mode
  目标：保护开发者本机已经部署好、正在使用的 agents / Headroom / RTK / cc-switch
  原则：不调用真实工具、不读写真实配置、不改 hook、不占真实端口、不启动/停止真实服务

成品安装线：Production Managed Install
  目标：让最终用户一键安装 / 接入 / 配置 / 升级 / 卸载 d-token App 与内置 runtime
  原则：可以写配置、装工具、改 hook、启动服务，但必须有事务、备份、校验、回滚、可解释日志

开发验收线与成品 UI 线必须分离：开发者需要的真实测试按钮、benchmark、mock/fake/fixture 入口只能存在于开发脚本、CI、隐藏开发工具或测试报告中；用户下载后的正式 App 不展示测试语义，只展示安装、连接、配置、状态、诊断和修复。
```

### 3.2 开发期禁止行为

开发期默认禁止：

1. 重新安装 Headroom。
2. 重新安装 RTK。
3. 修改已有 agent hook。
4. 修改 shell profile。
5. 修改 RTK config。
6. 修改 Headroom config。
7. 修改 cc-switch config。
8. 占用真实 proxy 端口。
9. 启动、停止、重启真实服务。
10. 读取真实 prompt、代码、CCR 原文、tee 文件、session 原文。

开发期默认必须通过 fixture、fake binary、临时 HOME、临时 PATH、临时端口、mock provider、sandbox config 来验证 d-token 自身逻辑。

OpenCode Real Route Track 是当前用户明确授权的例外：为了验证真实链路，OpenCode 测试可以直接使用用户原本 OpenCode 配置、真实 provider/model 和真实 OpenCode 运行环境，不强制隔离 HOME，也不要求先造 fake config 或临时 OpenCode profile。该例外只适用于 OpenCode routed / streaming / workload test，不得扩展到 Claude Code、Codex、Cursor、Cline、外部 Headroom、RTK、cc-switch、系统代理或 shell profile。

OpenCode 真实测试的默认动作是“用现有真实配置跑通 d-token 链路”。只有在需要把 OpenCode provider/baseURL 指向 d-token 时，才允许对 OpenCode 配置做最小变更；变更前必须备份，报告中只记录 backup 路径和 diff 摘要，禁止读取、打印、保存或提交密钥、token、cookie、auth 文件、完整 prompt/response。

OpenCode 真实测试对应的产品动作不是“测试 OpenCode”，而是“在安装向导中接入 OpenCode”：启动/安装 d-token-owned 服务，选择 OpenCode agent，生成并预览配置 diff，备份 OpenCode 配置，应用最小配置变更，验证配置生效，回到 Dashboard 显示真实路由与压缩数据。测试脚本只负责在幕后证明该产品动作有效。

### 3.3 成品安装事务

所有真实写操作必须进入 InstallationTransaction：

```text
scan
  ↓
plan
  ↓
show diff
  ↓
backup
  ↓
confirm
  ↓
apply
  ↓
verify
  ↓
doctor
  ↓
commit rollback point
```

失败时必须：

```text
stop only d-token-owned new processes
  ↓
verify process identity before killing anything
  ↓
restore backups
  ↓
remove partial files
  ↓
report exact failure and recovery action
```

## 4. 产品分层与 Ownership

### 4.1 Headroom-derived runtime owns

d-token 将直接吸收和修改 Headroom 能力，形成 Headroom-derived runtime。该 runtime 负责：

1. Context compression pipeline。
2. ContentRouter / CacheAligner / SmartCrusher / CodeCompressor / Kompress text compressor。
3. CCR reversible compression 与 local retrieval。
4. Local proxy / SDK / MCP server。
5. Agent wrap / unwrap。
6. Cross-agent memory、per-project memory、SharedContext。
7. learn / verbosity / output shaper。
8. Metrics、stats、history、dashboard primitives。
9. Integrated RTK install / hook / wrap-side metrics bridge。
10. Windows/macOS 兼容性补丁：UTF-8、路径、shim、端口、进程身份。

### 4.2 Integrated RTK owns

RTK 作为 runtime 内部能力负责：

1. command rewrite。
2. command execution shim / hook。
3. command-specific filtering。
4. raw-output tee recovery。
5. command-output savings analytics。
6. missed savings opportunities。
7. session adoption statistics。

RTK 不作为用户可选的独立产品路径出现。

### 4.3 d-token App owns

1. Windows/macOS desktop app。
2. 图形化安装向导与工具选择。
3. 傻瓜模式与高级模式。
4. 内置 runtime 的安装、启用、升级、停止、卸载、回滚。
5. ProviderProfile / RouteProfile / InstanceProfile / ProjectProfile。
6. 官方 provider、第三方 provider、自定义 URL、cc-switch、chain bridge 路由。
7. Anthropic-compatible 与 OpenAI-compatible 协议注入、转换与审计。
8. ProjectCompressionPolicy 与推荐理由。
9. Dashboard、Doctor、Logs、Export、Backup、Rollback。
10. Event Ledger 与 layered savings attribution normalization。
11. Windows/macOS 兼容性诊断与修复提示。
12. 中文 / 英文 i18n。
13. 安全边界：本地优先、默认 127.0.0.1、密钥脱敏、无静默 fallback。

## 5. 最终产品形态

### 5.1 直接压缩到官方或第三方自定义 URL

```text
AI Coding Tool
  ↓
d-token managed agent wrap
  ↓
Integrated RTK command rewrite / shell output reduction
  ↓
d-token Headroom-derived compression runtime
  ↓
d-token route adapter
  ↓
Official Provider / Custom OpenAI-compatible URL / Custom Anthropic-compatible URL
  ↓
Final upstream provider
  ↓
d-token Event Ledger / Dashboard / Doctor / GUI
```

适用场景：

1. 官方 OpenAI / Anthropic。
2. 第三方 OpenAI-compatible 中转。
3. 第三方 Anthropic-compatible 中转。
4. 本地模型服务。
5. 用户自己提供的 base URL。

### 5.2 先压缩，再转发到 cc-switch

当用户在 Claude Code 等 Anthropic-shaped agent 中使用 OpenAI-format URL、OpenCode Go 订阅、cc-switch 或多上游中转时，d-token 必须支持二次转发：

```text
Claude Code / Anthropic-compatible client
  ↓
d-token local entry
  ↓
d-token Headroom-derived compression runtime
  ↓
d-token protocol/route adapter
  ↓
cc-switch OpenAI-compatible or Anthropic-compatible route
  ↓
Final upstream provider
```

要求：

1. 先压缩，再转发。
2. route decision 必须记录 input protocol、output protocol、target provider、cc-switch endpoint。
3. 不允许静默从第三方 provider fallback 到官方 Anthropic/OpenAI。
4. Provider 401 必须显示为 route/provider mismatch，而不是泛化为 token 错误。
5. Header stripping、Authorization 选择、base URL 选择必须可审计。
6. GUI 必须展示“当前请求实际会去哪里”。
7. 用户不需要理解底层两次转发，只需要在图形界面选择“使用 cc-switch 路由”。

### 5.3 未来自研替代路径

第二版开始，d-token 可以逐步将 Headroom-derived runtime 中的关键能力改造为 d-token-native runtime：

```text
Phase 1: package and modify Headroom-derived runtime
Phase 2: replace fragile CLI/config/dashboard pieces with d-token-native Rust/TS modules
Phase 3: replace selected compression/router/observability core with d-token-native implementation
Phase 4: keep compatibility with Headroom concepts, but user-facing runtime becomes d-token-owned
```

## 6. 模式设计

### 6.1 Fixture Mode：开发默认

完全读取 `support/fixtures`，不调用任何真实命令。

用于测试：parser、API response、Dashboard、EventLedger、LayeredSavings、ProjectPolicy、ProviderRoute、cc-switch route plan。

必须保证：

1. 不调用真实 `rtk`。
2. 不调用真实 `headroom`。
3. 不调用真实 cc-switch。
4. 不读真实 agent config。
5. 不改任何真实配置。
6. 只使用测试端口。

### 6.2 Fake-binary Mode：开发命令调用测试

通过 `support/fake-bin` 提供假的 `headroom` / `rtk` / `cc-switch`，测试 command runner、stdout/stderr、exit code、Windows `.cmd` shim、error boundary。

必须使用临时 HOME、临时 PATH、临时端口。

### 6.3 Shadow Mode：开发者主动只读检测

只读检测真实环境，例如 existing Headroom、RTK binary provenance、cc-switch health endpoint、provider health。

Shadow Mode 必须由开发者主动开启，不能作为默认行为。

### 6.4 External Mode：连接已有安装，仅用于开发/迁移

适合已经手动装好 Headroom / RTK / cc-switch 的开发者或早期用户。d-token 只做检测、状态展示、analytics import、Dashboard、Doctor 和 route/project 管理。

External Mode 是迁移辅助模式，不是最终用户主路径。

### 6.5 Managed Mode：成品默认路径

适合新用户或希望 d-token 托管工具生命周期的用户。

默认 Managed Mode 安装顺序：

1. 安装/启用 d-token desktop。
2. 安装/启用 d-token Headroom-derived runtime。
3. 通过 runtime 内部路径启用 integrated RTK。
4. 可选安装/连接 cc-switch。
5. 图形化配置 agent wrap。
6. 图形化配置 provider routes。
7. 图形化配置 project compression policy。
8. 生成 rollback point。

### 6.6 Built-in Fallback Mode

当完整 runtime 不可用时，d-token 可提供基础 CLI output preprocessor：ANSI 清理、空行压缩、重复行折叠、超长输出截断、error-only mode。

该模式只能作为 emergency fallback，不声称等价于完整 runtime。

## 7. Integrated RTK 适配范围

Standalone RTK 不作为用户路径保留。d-token 必须适配 integrated RTK：

1. runtime 内部 RTK binary 是否存在。
2. RTK binary 来源、版本、校验状态。
3. agent wrap 是否启用了 RTK instructions。
4. runtime metrics 中 RTK invocation / savings 是否可见。
5. wrap session 结束后的 RTK savings summary。
6. Agent instruction 文件中 d-token/runtime/RTK block 的 diff、backup、rollback。
7. Windows native / WSL / macOS 下 hook 支持等级。
8. 是否存在外部 standalone RTK 冲突、重复 PATH、重复 hook。
9. 冲突只做提示和修复建议，不把 standalone RTK 暴露成推荐选项。

## 8. Headroom-derived runtime 适配范围

必须保留、吸收并逐步改造：Library / SDK、Local proxy、Agent wrap / unwrap、MCP server、Content-aware compression pipeline、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、Kompress/text compressor、CCR reversible compression、local retrieval、Cross-agent memory、Per-project memory、SharedContext、learn、output shaper、verbosity steering、effort routing、durable savings ledger、metrics endpoints、Historical stats、JSON/CSV export、Cost tracking、Per-model/per-agent/per-project breakdown、Doctor/perf/benchmark/troubleshooting、integrated RTK。

开发期适配原则：不读真实 CCR 原文、不改真实 Headroom config、不占真实端口。  
成品适配原则：通过 d-token App 安装、启动、配置、更新和卸载，但必须可回滚。

## 8.1 Agent 覆盖要求

Agent matrix 以 d-token managed agent wrap 支持为主。

必须覆盖：

```text
Claude Code
Codex CLI
Cursor
Cline / Roo Code
OpenCode
OpenClaw
Aider
Continue
Goose
OpenHands
GitHub Copilot CLI
VS Code Copilot Chat
Gemini CLI
Windsurf
Kilo Code
Google Antigravity
Mistral Vibe
Pi
Hermes
Cortex Code
```

每个 agent 在 GUI、Doctor 与 CompatibilityMatrix 中至少展示：

1. d-token managed wrap 是否支持。
2. integrated RTK 是否生效。
3. Agent 配置方式：wrap、manual proxy、plugin、rules、instructions、library only、memory/learn only。
4. 官方 provider 配置状态。
5. 第三方 provider 配置状态。
6. 自定义 URL 配置状态。
7. cc-switch route 配置状态。
8. macOS 支持等级。
9. Windows native 支持等级。
10. Windows WSL 支持等级。
11. 限制说明与修复建议。

## 8.2 Provider / Route / Protocol 要求

Headroom-derived runtime 能通过 proxy / SDK / middleware 接入 OpenAI-compatible 与 Anthropic-compatible 流量，d-token 必须把协议级接入作为一等能力，而不是只按具体 agent 列表处理。

### Provider 类型

```ts
interface ProviderProfile {
  id: string;
  name: string;
  kind: 'official' | 'custom' | 'cc_switch' | 'local_chain';
  protocol: 'openai' | 'anthropic' | 'auto';
  base_url: string;
  api_key_ref?: string;
  models?: string[];
  timeout_ms: number;
  healthcheck?: string;
  notes?: string;
}
```

### Route 类型

```ts
interface RouteProfile {
  id: string;
  name: string;
  input_protocol: 'openai' | 'anthropic';
  output_protocol: 'openai' | 'anthropic' | 'same_as_input';
  agent_id?: string;
  project_id?: string;
  runtime_instance_id?: string;
  target_provider_id: string;
  cc_switch_provider_id?: string;
  chain_mode: 'direct' | 'compress_then_provider' | 'compress_then_cc_switch' | 'chain_bridge';
  compression_policy_id?: string;
  bind_host: '127.0.0.1';
  local_port: number;
  fallback_policy: 'disabled' | 'explicit_only';
}
```

### 路由要求

1. 官方 provider 与第三方 provider 必须显式区分。
2. Claude/Anthropic traffic 不得因为配置缺失自动发往 `api.anthropic.com`。
3. OpenAI-compatible provider 不得伪装成 official OpenAI，必须展示 base URL。
4. cc-switch 必须作为 ProviderProfile 或 ChainBridgeProfile 显式配置。
5. route decision 必须进入 Event Ledger。
6. Dashboard 必须显示当前 agent → compression → route → provider 的完整链路。
7. 所有 fallback 默认禁用；如支持 fallback，必须显式展示并要求用户确认。
8. 用户可以选择“压缩后直连自定义 URL”或“压缩后转发到 cc-switch”。

## 9. Headroom-derived 项目级压缩策略

### 9.1 ProjectProfile

```ts
interface ProjectProfile {
  id: string;
  name: string;
  root_path: string;
  language_stack: string[];
  agents: string[];
  default_instance_id?: string;
  compression_policy_id?: string;
  provider_route_id?: string;
  cc_switch_route_id?: string;
  created_at: string;
  updated_at: string;
}
```

### 9.2 ProjectCompressionPolicy

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
  enable_integrated_rtk: boolean;
  output_verbosity: 'auto' | 'terse' | 'normal' | 'detailed';
  holdout_ratio?: number;
  notes?: string;
}
```

### 9.3 CompressionRecommendationEngine

推荐等级必须基于：项目语言栈、最近请求 token 规模、命令输出噪音比例、runtime 历史 savings、integrated RTK 历史 savings、LLM 回答质量反馈、retrieve 调用频率、失败率与回退率、用户手动覆盖偏好、高风险任务标记、provider/cc-switch 延迟与错误率。

```text
safe:       默认推荐，适合新项目、高风险任务、未知 provider route
balanced:   常规编码任务推荐
aggressive: 大日志、大测试输出、大搜索结果推荐
max:        只对低风险探索/批量日志/RAG chunks 推荐，必须提示可能损失细节
```

## 10. 分层压缩与路由指标模型

```text
raw_cli_tokens
  → rtk_cleaned_tokens
  → runtime_input_tokens
  → runtime_final_tokens
  → provider_input_tokens

rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
runtime_savings = runtime_input_tokens - runtime_final_tokens
route_overhead = provider_input_tokens - runtime_final_tokens
end_to_end_savings = raw_total_context_tokens - final_llm_input_tokens
```

禁止把 `rtk_savings + runtime_savings` 直接相加当作总 savings，除非 denominator 和 baseline 已经统一。

### 10.1 Runtime 数据来源

1. RTK savings 来自 d-token runtime metrics 或 integrated RTK metrics。
2. Context savings 来自 d-token runtime stats/history/metrics。
3. d-token 按 project/agent/route/provider 拆分展示。
4. d-token 不直接轮询 standalone `rtk gain` 作为产品路径。

### 10.2 Provider / cc-switch 指标

Dashboard 必须展示：

1. route decision count。
2. provider request count。
3. provider 401/403/429/5xx。
4. cc-switch double-hop latency。
5. compression latency。
6. route adapter latency。
7. provider final latency。
8. compressed tokens before forwarding。
9. final provider input tokens。
10. fallback 是否发生；默认不允许静默 fallback。

## 11. Event Ledger

```ts
interface DtokenEvent {
  event_id: string;
  timestamp_ms: number;
  project_id?: string;
  agent_id?: string;
  instance_id?: string;
  command_id?: string;
  request_id?: string;
  stage: 'rtk_rewrite' | 'rtk_filter' | 'agent_wrap' | 'runtime_route' | 'runtime_compress' | 'runtime_retrieve' | 'dtoken_route_decision' | 'protocol_adapt' | 'cc_switch_forward' | 'provider_forward' | 'install_step' | 'config_patch';
  source: 'rtk' | 'runtime' | 'dtoken' | 'cc_switch' | 'provider';
  status: 'ok' | 'passthrough' | 'failed' | 'disabled' | 'skipped';
  input_tokens?: number;
  output_tokens?: number;
  saved_tokens?: number;
  baseline_kind?: 'raw_cli' | 'rtk_cleaned' | 'runtime_input' | 'runtime_final' | 'provider_input' | 'final_llm_input';
  input_protocol?: 'openai' | 'anthropic';
  output_protocol?: 'openai' | 'anthropic';
  provider_id?: string;
  route_id?: string;
  error_code?: string;
  metadata?: Record<string, unknown>;
}
```

开发期 Event Ledger 不保存 full prompt、full response、full file content、full command output。成品默认也不保存，除非用户显式开启本地调试模式。

## 12. Windows/macOS 图形化 App 要求

### 12.1 App 技术路线

```text
Tauri 2 + React + TypeScript + Vite
Rust orchestration core
Headroom-derived packaged runtime
Integrated RTK default runtime ability
Optional cc-switch adapter
SQLite + JSONL event ledger + local JSON/YAML config
```

### 12.2 GUI 必须取代的命令行/网页体验

正式 GUI 的信息架构参考成熟桌面控制面产品逻辑：主界面承载安装、连接、状态、配置和常用管理动作；诊断能力放在 Doctor/Troubleshoot/Logs；开发测试和 benchmark 不进入用户主流程。公开参考包括 Docker Desktop 的 Troubleshoot/日志/诊断分区、Tailscale Windows 的托盘状态与配置入口、OpenCode 的 provider connect/config 流程。d-token 吸收的是产品逻辑，不复制任何外部源码或界面。

实现 GUI 前必须先完成“产品语义翻译”：把开发侧动作翻译成用户能理解的产品动作。例如 `run_opencode_workload_benchmark.ps1` 在正式 UI 中不叫“运行 workload benchmark”，而对应“验证 OpenCode 配置是否生效”；能力 smoke test 在正式 UI 中不叫“能力自检测试”，而对应 Doctor 的“诊断连接/检查配置/查看修复建议”。如果一个动作无法翻译成用户任务，它只能留在 `support/scripts`、CI、隐藏开发工具或测试报告中。

GUI 必须覆盖并逐步取代普通用户对以下命令或页面的依赖：

1. agent wrap / unwrap。
2. local proxy start / stop / restart。
3. doctor。
4. dashboard。
5. learn / verbosity。
6. RTK savings 聚合查看。
7. provider/base URL 配置。
8. cc-switch route 配置。
9. backup / rollback / diff。

高级用户仍可使用 CLI，但 GUI 是默认主入口。

### 12.3 GUI 页面

必须包含：

1. Welcome / Setup Wizard。
2. 傻瓜模式 / 高级模式选择。
3. Agent Selection。
4. Provider Setup：官方 / 第三方自定义 URL / cc-switch。
5. Route Builder：agent → compression → provider / cc-switch。
6. Runtime Status。
7. Integrated RTK Status。
8. Project Policy。
9. Dashboard。
10. Doctor。
11. Logs。
12. Config Diff。
13. Backup / Rollback。
14. Compatibility Matrix。
15. Settings。
16. 中文 / English 切换。

### 12.4 正式 UI 与开发测试边界

正式 UI 必须遵守以下边界：

1. 不展示“测试 workload”“benchmark”“smoke test”“fixture test”“mock test”“fake binary test”等开发验收概念。
2. 不把 OpenCode 当前测试入口设计成长期独立测试页；OpenCode 应作为 Agent Selection / Install Wizard / Route Builder 中的一个 agent。
3. 安装页的主路径是：服务状态 → 选择 agent → 配置 provider/route → 预览 diff/backup → 应用 → 验证 → Doctor/Dashboard。
4. 对用户可见的验证文案应是“验证配置”“诊断连接”“检查路由是否生效”，不能是“跑真实测试”。
5. 所有真实功能仍必须可被自动化测试覆盖，但触发位置应在 `support/scripts`、CI、Tauri command-level harness、隐藏 dev mode 或测试报告中。
6. OpenCode 当前验收只允许验证“安装/启动服务后，点击 OpenCode 的安装/配置/连接动作是否生效”；该验收不能把最终 App 变成 OpenCode 专用测试工具。
7. Dashboard 和 Logs 展示真实运行数据与诊断结果，不展示完整测试 prompt/response，不保存或暴露密钥、token、cookie、auth 文件。
8. 多 agent 兼容是最终产品原则。Claude Code、Codex、Cursor、Cline、OpenCode 等都应共享同一套 agent 接入模型、路由模型、Doctor 模型和 Dashboard 模型。
9. 浏览器或桌面窗口人工验收只能证明 UI 显示、交互、状态切换和可读性正常，不能被写成“真实功能已接入”的证据。真实接入完成必须由 d-token-owned runtime/API/Tauri command、Event Ledger、metrics/export、OpenCode routed flow 等运行证据共同证明。
10. 正式 App 中的 OpenCode 入口只能表达为“连接 OpenCode”“配置 OpenCode”“应用配置”“验证配置”“查看运行状态/日志/诊断”，不能表达为“测试 OpenCode”“跑 benchmark”“跑 workload”。同一条真实验证链路可以由开发脚本复用，但不能变成用户主界面的测试产品。
11. 安装/连接流程必须以 d-token 自有服务为中心：启动或连接 d-token-owned local service，选择 agent，生成 route/provider 配置，备份并应用最小 diff，验证请求是否进入 d-token Event Ledger。不得把外部 Headroom、外部 RTK 或外部 cc-switch 服务当作 UI 数据源或完成态依据。
12. OpenCode 是第一条真实验收入口，不是产品信息架构中心。后续 Claude Code、Codex、Cursor、Cline 等 agent 必须复用同一套 agent 接入、配置 diff、备份回滚、路由验证、Doctor 和 Dashboard 模型。
13. 测试结果可以进入开发报告、CI artifact、`support/artifacts` 或隐藏 dev mode，但正式 UI 只展示用户能理解和能操作的产品状态：已连接、待配置、配置已备份、路由已生效、服务运行中、诊断通过、日志可查看。
14. 若后续需要在 UI 中触发“验证配置”，按钮背后的实现可以调用真实 OpenCode 或其他 agent，但 UI 必须只展示脱敏后的配置生效、路由命中、事件入账和诊断结果，不展示完整 prompt/response、密钥、cookie、auth 文件或开发 benchmark 细节。
15. UI 中不得为了“方便测试”增加长期可见的测试中心、测试标签页、测试按钮、测试报告卡片；开发人员需要的验证入口必须放在脚本、CI、Tauri command harness、隐藏 dev mode 或文档化报告中。
16. 安装页不承担测试中心职责；它只承担服务安装/启动、agent 选择、provider/route 配置、diff/backup/apply、验证配置、跳转 Doctor/Dashboard/Logs 的职责。
17. Dashboard 不展示“测试是否通过”，而展示真实运行事实：请求量、压缩收益、路由目标、provider/cc-switch 状态、错误类型、延迟、Event Ledger 入账状态。
18. Logs 不展示完整测试 prompt/response，不保存或暴露密钥、token、cookie、auth 文件；只展示脱敏后的运行日志、路由日志、配置变更日志、错误日志和导出入口。
19. Doctor 可以执行诊断动作，但文案必须面向修复和状态解释，例如“检查 OpenCode 配置”“检查 d-token 服务”“检查路由是否生效”“查看修复建议”，不得把开发 smoke test 原样暴露给普通用户。
20. 每次新增 UI 功能前必须检查：它是否服务普通用户日常任务；是否复用多 agent 模型；是否读取 d-token-owned 数据；是否泄露开发测试概念；是否能通过脚本/CI 独立验证真实能力。

## 13. Doctor 要求

### 13.1 开发期 Doctor

开发期 Doctor 必须支持 no-touch 检查：

1. 当前 profile 是否为 fixture/fake/shadow。
2. 是否误调用真实 binary。
3. 是否使用测试端口。
4. 是否使用临时 HOME。
5. 是否存在外部配置写入风险。
6. 是否误连接真实 Headroom / RTK / cc-switch。

### 13.2 成品 Doctor

成品 Doctor 必须检查：

1. Local proxy。
2. 端口占用与旧进程残留。
3. 进程身份校验：kill 前必须确认是 d-token-owned 或用户确认。
4. d-token runtime/proxy/MCP/stats/CCR summary。
5. Integrated RTK binary/version/hook/metrics。
6. 外部 standalone RTK 冲突检测。
7. Provider route。
8. cc-switch route。
9. Project policy。
10. Config backup/rollback。
11. Event ledger。
12. Installation transaction。
13. Windows UTF-8、盘符路径、`.cmd` shim。
14. macOS xattr/quarantine/keychain/launch agent。

## 14. 已知问题沉淀为产品要求

以下问题必须转化为 d-token 的 Doctor、GUI、路由和安装验收项。

### 14.1 Provider / Route / Dashboard UX

1. 第三方 provider 401：不得因路由缺失把请求静默发到官方 Anthropic/OpenAI；GUI 必须展示真实 target provider。
2. 重启后端口占用：必须检测旧进程、端口占用、PID 身份，不能盲目启动新服务。
3. Dashboard 看不到请求：必须提供 route chain probe，确认 agent 请求是否真的经过 d-token runtime。
4. Output token savings 提示误导：可选功能说明必须和错误提示视觉区分。
5. learn/verbosity 项目上下文不明确：GUI 必须告诉用户需要选择项目、当前目录或 all projects。
6. 仪表盘无中文：d-token GUI 必须支持中文。

### 14.2 Windows 功能性兼容

1. UTF-8 编码：读取 session、JSONL、配置、日志时必须显式 UTF-8，不能依赖 Windows 默认 GBK。
2. 盘符路径：必须正确处理 `D:\work\project`、`D--work-project` 等编码/解码格式。
3. 项目名连字符：不能仅靠 `-` 反推路径；应优先从 session `cwd` 或显式 project root 反查真实路径。
4. npm 安装 CLI shim：Windows CreateProcess 失败后必须尝试 `.cmd` / `.exe` / `.bat` shim，但要记录实际执行路径。

## 15. 安全、隐私与开源合规原则

1. 默认只监听 `127.0.0.1`。
2. API key 只存在本地。
3. 日志默认脱敏。
4. Authorization / Cookie / API key 永不明文展示。
5. 导出日志二次脱敏。
6. RTK/CLI 输出预处理层不得默认记录完整命令输出。
7. d-token 不自动开启 RTK telemetry。
8. d-token 导入 RTK analytics 时默认只读取聚合统计。
9. d-token 不上传 RTK tracking database、tee files、prompt、代码、命令输出到远端。
10. d-token 不默认读取 CCR 原文缓存。
11. 所有 managed binary 必须记录来源与校验信息。
12. 成品覆盖安装必须有 backup / rollback。
13. 开发期测试必须默认 no-touch。
14. 所有非 loopback 绑定必须拒绝或要求强认证与显式确认。
15. 所有 provider route 与 fallback 必须可见、可解释、可审计。
16. 不提供账号托管、账号共享、账号池、转售或代登录能力。
17. 不帮助隐藏真实使用来源，不绕过第三方平台条款。
18. 吸收 Headroom/RTK 代码必须遵守上游开源 license、保留必要版权声明、NOTICE、变更说明与依赖清单。
19. d-token 修改 Headroom-derived runtime 时必须记录 upstream base commit，方便安全审计、升级和回滚。

## 16. CompatibilityMatrix

必须维护 OS / arch / shell / agent / runtime mode / provider route / cc-switch mode 的兼容矩阵：

```ts
interface CompatibilityCase {
  id: string;
  os: 'macos' | 'windows' | 'linux';
  arch: 'x64' | 'arm64';
  shell: 'bash' | 'zsh' | 'powershell' | 'cmd' | 'wsl-bash';
  agent: string;
  runtime_mode: 'fixture' | 'external_headroom_migration' | 'managed_dtoken_runtime' | 'disabled';
  rtk_source: 'integrated' | 'conflicting_external_detected' | 'disabled';
  provider_route_mode: 'official' | 'custom_openai' | 'custom_anthropic' | 'cc_switch' | 'chain_bridge';
  expected_hook_support: 'full' | 'partial' | 'none';
  expected_proxy_support: 'full' | 'partial' | 'none';
  status: 'untested' | 'pass' | 'fail' | 'blocked';
  notes?: string;
}
```

Mac 与 Windows 都是首要目标平台。Windows 必须区分 native 与 WSL：native 模式通常只能获得部分 hook/规则能力，WSL 才能获得完整 shell hook 体验。

## 17. 目录结构

仓库根目录允许：

```text
README.md
AGENTS.md
CLAUDE.md
docs/
code/
support/
.github/workflows/
.gitignore
```

目录含义：

```text
README.md  面向 GitHub 访问者的项目入口介绍
docs/      所有正式文档，包含唯一 PRD、架构、技术决策、安全边界、审查报告、研究报告
code/      所有应用代码、crate、package、runtime、配置样例
support/   测试、脚本、开发辅助、CI 模板、fixture、fake-bin、临时杂项
```

## 18. 里程碑与执行计划

d-token 的阶段执行计划、任务拆解、风险与 CI gate 不再写入 PRD，统一存放于 `docs/PLAN.md`，按代码现状滚动维护。

- 当前活跃计划：`docs/PLAN.md`
- 历史路线图快照（M0–M6 原文）：`docs/archive/ROADMAP_M0_M6_HISTORICAL.md`
- 已交付里程碑证据：`docs/M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md`、`docs/M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md`、`docs/M3_GUI_CONTROL_PLANE_MVP_REPORT.md`、`docs/M1_M3_DELIVERY_AUDIT.md`

PRD 只保留产品真理源：目标、边界、技术契约与验收标准。计划文档不得与 PRD 验收标准冲突；若产品边界变化，先改 PRD，再同步 PLAN。

## 19. 验收标准

1. 仓库只有一个 PRD：`docs/PRD.md`。
2. 所有正式文档都在 `docs/`。
3. 所有正式代码都在 `code/`。
4. 所有测试、脚本、fixture、fake-bin 都在 `support/`。
5. 开发期默认 no-touch，不破坏开发者本机已有 agents/RTK/Headroom/cc-switch。
6. 成品支持覆盖安装，但必须有事务、备份、校验和回滚。
7. 用户只需下载安装 d-token App，不应被要求先安装 Headroom 或 RTK。
8. Headroom 功能吸收不能删项，第一版可以增强封装，后续必须支持创新替代。
9. Dashboard 迁移不能缩水，GUI 必须逐步取代普通用户对网页 dashboard 与命令行安装的依赖。
10. 默认 RTK 路径是 integrated RTK，不保留 standalone RTK 用户路径。
11. Dashboard 必须能区分 RTK 命令输出预处理收益、runtime 上下文压缩收益、端到端总收益、provider/cc-switch route overhead。
12. 用户必须能按项目自定义压缩等级，并查看推荐等级与推荐理由。
13. 用户必须能选择官方 provider、第三方自定义 URL、cc-switch 或 chain bridge。
14. 用户必须能图形化选择 agent 并安装/接入对应 wrap/config。
15. 所有 agent/runtime/provider/cc-switch/shell 配置修改必须支持 diff、backup、rollback。
16. CI 必须至少覆盖 structure check、cargo check、cargo test、no-touch smoke、desktop build。
17. M3 GUI 开发默认 fixture，不允许默认扫描或修改真实环境。
18. M4/M5 成品安装器允许覆盖安装，但必须可追踪、可解释、可恢复。
19. Windows UTF-8、盘符路径、项目名连字符、`.cmd` shim 必须进入兼容性测试。
20. 不允许请求因为配置缺失静默走官方 provider；route decision 必须可见、可解释、可审计。
21. d-token 吸收/修改 Headroom/RTK 代码必须保留开源合规记录、upstream base commit、NOTICE 与变更说明。
22. 正式 UI 不得出现 workload benchmark、smoke test、fake/fixture test、testing center 等开发测试产品化入口；这些能力只能存在于脚本、CI、隐藏开发模式和报告中。
23. 真实功能验收必须区分两类证据：桌面/浏览器验收负责证明界面正确，运行时验收负责证明 OpenCode 或其他 agent 的真实请求经过 d-token-owned service、route adapter、upstream provider，并写入 Event Ledger。
24. 安装页验收以产品流程为准：安装/启动 d-token 服务，选择 agent，配置 provider/route，预览 diff，备份，应用，验证配置，进入 Doctor/Dashboard/Logs；不得把“点击测试按钮跑 workload”作为最终用户验收路径。
25. OpenCode 验收必须证明“正式安装/配置流程里的 OpenCode 连接动作有效”，而不是证明“UI 里有 OpenCode 测试按钮”。测试实现可以复用脚本，但用户看到的是连接、配置、验证、诊断和运行状态。
26. 每个真实能力接入完成时，必须同时提供 UI 证据和运行时证据：UI 证据来自浏览器/桌面窗口检查；运行时证据来自 d-token-owned API/Tauri command、Event Ledger、metrics/export、OpenCode routed flow 或对应 agent flow。
27. 功能实现前必须先对照 PRD 和成熟应用逻辑完成产品判断；如果实现方案会把开发测试、fixture、mock、benchmark、smoke 直接暴露给用户，则验收不通过。
28. d-token 最终 UI 必须保持多 agent 控制面结构；任何只适配 OpenCode、只展示 OpenCode 测试、或把 OpenCode 作为唯一产品中心的实现都不满足 PRD。
