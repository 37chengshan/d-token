---
document_id: d-token-product-target-spec
document_version: 2.1
status: draft
authority: product-direction-ssot
audience:
  - ai-planning-agent
  - product-agent
  - architecture-agent
  - implementation-agent
  - review-agent
target_versions:
  - 0.0.1
  - 0.3
  - 0.4
updated_at: 2026-08-05
language: zh-CN
---

# d-token 产品预期目标与执行约束

> 文档类型：AI 可执行产品目标规格  
> 适用范围：产品规划、架构设计、PRD 拆分、实现计划、代码审查、发布验收  
> 权威级别：产品方向事实来源；不代表所有能力已经实现

---

## 0. 给后续 AI 的使用说明

本文件是 d-token 产品方向的高层事实来源。后续 AI 在生成 PRD、技术方案、任务拆分、实现计划或审查报告时，必须遵守以下规则：

1. 不得把本文件中的长期目标全部塞入同一个版本或同一个 PR。
2. 必须区分：
   - **MUST**：当前目标版本必须满足；
   - **SHOULD**：强烈建议满足，但可因阻塞项延期；
   - **MAY**：可选能力，不得阻塞核心发布。
3. 未明确标注完成状态的能力，不得假设已经实现。
4. 无真实运行证据时，不得把功能描述为“已完成”“可用”或“稳定”。
5. 所有实现计划必须包含：
   - 影响范围；
   - 依赖项；
   - 风险；
   - 测试方法；
   - 回滚路径；
   - 验收标准。
6. 不得为了扩展功能而破坏以下核心约束：
   - 不静默切换 Provider 或模型；
   - 不静默修改用户配置；
   - 不伪造 Token、成本、状态或成功结果；
   - 所有配置修改可恢复；
   - 所有优化可解释；
   - 优化失败时安全降级。
7. 当需求与本文件冲突时，应先指出冲突，再提出修订方案，不得默认覆盖本文件。
8. `Headroom`、`LeanCTX`、`CCR`、`Codec`、`Context Bus`、`MCP` 等属于内部技术概念。除开发者页面和技术文档外，正式用户界面应使用用户可理解的语言。

---

## 1. 核心结论

### 1.1 产品定义

d-token 是面向 AI 编码 Agent 的本地上下文控制面。

它负责：

- 自动发现本机已安装或已配置的 AI 编码 Agent；
- 扫描 Agent、项目级配置、用户级配置以及 cc-switch 配置；
- 感知用户通过 cc-switch 或其他方式进行的上游切换；
- 在用户授权策略允许时同步 d-token Runtime；
- 保持 Provider、模型和路由结果透明；
- 压缩重复或低价值上下文；
- 保护代码、错误、标识符和用户意图；
- 恢复被省略或压缩的信息；
- 复用跨 Agent 已处理的项目上下文；
- 为每次请求生成可验证的优化、路由和恢复证据；
- 在异常时提供诊断、修复和回滚。

### 1.2 一句话定位

> **让 Claude Code、Codex、OpenCode 等 AI 编码 Agent 少用 Token、不丢上下文，并始终保持路由透明、配置可恢复。**

英文定位：

> **A local context control plane for AI coding agents — less context, same intent, full control.**

### 1.3 最终用户认知

> **我不需要持续管理 d-token，但我随时可以确认它做了什么，也可以随时撤销。**

### 1.4 核心差异化

d-token 不通过 Provider 数量取胜。

d-token 的核心壁垒是：

1. 可信压缩；
2. 跨 Agent 上下文复用；
3. Agent 与 cc-switch 配置闭环；
4. 请求级证据；
5. 可诊断、可修复、可回滚。

---

## 2. 术语与命名规范

后续文档和代码设计应统一使用以下术语。

| 术语 | 定义 |
|---|---|
| Agent | Claude Code、Codex、OpenCode、Cline、Cursor 等 AI 编码工具 |
| cc-switch | 用户用于切换 Agent 上游、模型或 Provider 的外部配置工具 |
| Upstream | Agent 请求最终发送到的 Provider、网关或兼容 API |
| Provider | OpenAI、Anthropic 或 OpenAI-compatible 等服务提供方 |
| Runtime | d-token 本地请求处理与路由运行时 |
| Trusted Compression | 可验证、可降级、必要时可恢复的上下文压缩 |
| Optimization Receipt | 单次请求的优化、路由、Token 与恢复证据 |
| Context Relay | 跨 Agent 复用已处理上下文的机制 |
| Recovery Pointer | 获取被省略完整内容的稳定引用 |
| Recovery Manifest | 描述原始内容、变换、校验与恢复方式的记录 |
| Doctor | 面向用户的状态验证、诊断、修复和恢复入口 |
| Event Ledger | 记录配置、路由、压缩、恢复和修复事件的统一事实账本 |
| Local Token | d-token 使用本地 tokenizer 计算的 Token |
| Provider Token | 上游 Provider 返回的计费或使用量 Token |

命名要求：

- 正式文档统一使用 `cc-switch`，不要混用 `ccsw`、`CCSwitch` 等别名。
- 用户界面可使用“上游”，技术层可使用 `Upstream`。
- 不得把 Local Token 与 Provider Token 合并成同一含义不明的字段。

---

## 3. 产品目标层级

### 3.1 北极星目标

用户安装并授权一次后，可以继续正常使用自己的 AI 编码 Agent。d-token 在后台完成发现、配置同步、上下文优化、路由验证、证据记录和异常诊断，只有在需要用户决策或存在风险时才打扰用户。

### 3.2 主要用户价值

- 减少重复上下文；
- 降低输入 Token 与成本；
- 降低长会话上下文膨胀；
- 在多个 Agent 之间保持项目上下文连续；
- 避免压缩误伤关键内容；
- 明确知道请求实际走向；
- 避免配置失配和不可逆修改；
- 出现问题时快速定位并恢复。

### 3.3 产品成功条件

d-token 达到目标时，用户应能确认：

1. 当前 Agent 是否已经接入 d-token；
2. 当前使用哪个 Provider 和模型；
3. 本次请求是否执行了优化；
4. 哪些内容被压缩、保护或复用；
5. Local Token 与 Provider Token 分别是多少；
6. 是否存在恢复方式；
7. 配置修改是否可回滚；
8. 异常属于 Agent、配置、Runtime、压缩、路由还是上游；
9. 是否可以安全卸载并恢复原始配置。

---

## 4. 明确的产品边界

### 4.1 当前核心范围

d-token 聚焦于：

- AI 编码 Agent；
- 本地运行；
- Agent 自动发现；
- 配置扫描与事务化修改；
- cc-switch 状态感知；
- 显式 Provider / Model 路由；
- 可信上下文压缩；
- 上下文恢复；
- 跨 Agent Context Relay；
- Token、成本、延迟和路由证据；
- 用户级 Doctor；
- Windows 与 macOS 桌面体验。

### 4.2 非目标

以下能力不是当前核心目标，不得抢占核心版本资源：

- 追求最多 Provider 数量；
- 免费模型聚合市场；
- 通用聊天客户端；
- 图片、音频、视频生成；
- 云端账号系统；
- 商业模型市场；
- 移动端 PWA；
- 默认启用复杂自动模型选择；
- 未经授权自动修改配置；
- 静默跨 Provider 或跨模型切换；
- 为每个内部模块创建独立一级页面；
- 为了展示而填充模拟成功数据。

### 4.3 竞争定位

d-token 不是另一个功能庞杂的 AI Gateway。

正确定位是：

> **OmniRoute 等产品主要解决模型与 Provider 的连接和路由；d-token 主要解决编码 Agent 的上下文效率、连续性、可恢复性和可信控制。**

---

## 5. 不可违反的产品原则

### P-01：用户意图优先

**MUST**

- 不静默更换 Provider；
- 不静默更换模型；
- 不静默修改 Agent 配置；
- 自动行为必须来自用户明确授权的策略；
- 发生冲突时请求用户决策，不得强制覆盖。

### P-02：所有配置修改可恢复

**MUST**

每次写入配置必须包含：

1. 修改前快照；
2. 修改预览；
3. 文件级或字段级 Diff；
4. 原子或等价事务写入；
5. 写入后重新读取验证；
6. 失败自动回滚；
7. 手动一键恢复；
8. 只回滚 d-token 修改的字段；
9. 不覆盖用户后续无关修改。

### P-03：所有优化可解释

**MUST**

任何压缩、选择、缓存复用、Context Relay 或路由行为必须生成证据，至少包含：

- 请求来源 Agent；
- 用户目标 Provider 和模型；
- 实际 Provider 和模型；
- 处理阶段；
- 变换类型；
- 受保护内容类别；
- Local Token 前后值；
- Provider Token；
- 跳过或失败原因；
- 恢复状态；
- 数据来源和时间。

### P-04：失败时安全降级

**MUST**

- 压缩失败时发送原始请求；
- Relay 失败时执行正常读取；
- Receipt 写入失败不得阻断主请求；
- Analytics 失败不得改变请求结果；
- Doctor 检测失败不得伪造健康状态；
- 配置写入失败必须恢复原状态或明确进入可修复失败状态。

### P-05：证据优先于展示

**MUST**

- 无证据时显示 `unknown`、`unavailable`、`partial` 或 `not verified`；
- 不得使用示例数字替代真实状态；
- 不得用绿色状态掩盖未验证能力；
- 不得把估算值标记为 Provider 实际值。

### P-06：本地优先与最小暴露

**MUST**

- 默认不上传源代码；
- 默认不上传 Prompt；
- 默认不上传 API Key；
- Secret 不进入普通日志；
- 分享报告默认脱敏；
- Context Relay 不跨用户；
- Context Relay 默认不跨项目；
- Telemetry 可关闭；
- 数据保留周期可配置；
- 卸载可删除本地数据并恢复配置。

---

## 6. 核心用户闭环

```text
发现 Agent 与配置
  → 建立当前状态快照
  → 显示接入能力与风险
  → 用户授权
  → 预览配置变更
  → 安全应用并验证
  → 发起真实测试请求
  → 生成首份 Optimization Receipt
  → 持续感知 Agent / cc-switch 变化
  → 复用可用上下文
  → 执行可信压缩
  → 显式路由到用户选择的上游
  → 记录请求证据
  → Doctor 持续验证
  → 异常时修复或回滚
```

闭环目标：

- 用户完成首次授权后，无需频繁进入 d-token；
- d-token 仅在存在风险、冲突或需要选择时打扰用户；
- 所有后台自动行为均可在 Event Ledger 中追踪。

---

## 7. 核心能力要求

### 7.1 F-01 首次使用引导

#### 目标

用户首次打开 d-token 后，在一个连续流程中完成发现、授权、连接、测试和价值验证。

#### MUST

1. 扫描支持的 Agent 可执行文件和配置；
2. 扫描用户级与项目级配置；
3. 扫描 cc-switch 配置及当前选择；
4. 区分：
   - 可自动接入；
   - 仅可检测；
   - 不支持；
   - 状态未知；
5. 显示当前 Provider、模型、配置来源和最后更新时间；
6. 让用户选择需要接入的 Agent；
7. 写入前展示 Diff；
8. 创建恢复点；
9. 应用配置并重新读取验证；
10. 发起真实连通性测试；
11. 生成首份 Optimization Receipt；
12. 清晰说明如何恢复。

#### SHOULD

- 自动填写可以可靠发现的信息；
- 支持跳过部分 Agent；
- 首次失败时直接进入对应 Doctor 诊断；
- 首次流程结束后进入简洁 Overview。

#### 禁止

- 未经确认写入配置；
- 把无法识别的 Agent 标记为已接入；
- 使用模拟请求冒充真实连通性测试；
- 首屏展示大量内部技术术语。

#### 验收标准

- 用户无需手动寻找已支持 Agent 的配置路径；
- 至少一个支持的 Agent 可以完成真实 routed-flow；
- Provider 和模型在测试前后保持符合用户选择；
- 配置备份、写入验证和恢复路径均可验证；
- 首次流程结束时存在真实 Receipt。

---

### 7.2 F-02 可信压缩

#### 目标

在不明显降低真实编码任务质量的前提下，减少重复、冗余或低价值上下文。

#### MUST

- 内容类型识别；
- 重复内容消除；
- 日志、JSON、代码、Diff、搜索结果、构建输出的专用处理；
- 代码块保护；
- 标识符保护；
- URL 保护；
- 错误信息和尾部保护；
- Net-cost Gate；
- 负收益原样透传；
- 压缩后结构验证；
- 压缩跳过原因；
- Recovery Pointer 或 Recovery Manifest；
- 失败时原始请求降级；
- Local Token 与 Provider Token 分离统计。

#### SHOULD

- 提供安全、标准、增强三档策略；
- 对不同内容类型提供独立证据；
- 允许用户按 Agent、项目或内容类型禁用优化；
- 对可能影响质量的高风险变换默认关闭。

#### Optimization Receipt 最小字段

```yaml
request_id:
timestamp:
source_agent:
project_scope:
user_target_provider:
user_target_model:
actual_provider:
actual_model:
provider_or_model_changed:
transformations:
protected_content:
local_original_tokens:
local_optimized_tokens:
provider_input_tokens:
provider_output_tokens:
estimated_local_tokens_avoided:
recovery_status:
recovery_pointer:
validation_status:
skip_or_failure_reason:
```

#### Token 口径

- `local_original_tokens` 与 `local_optimized_tokens` 必须使用同一 tokenizer；
- `provider_input_tokens` 来自 Provider 时，必须单独保存；
- 不得直接使用不同 tokenizer 口径计算节省；
- 成本计算优先使用 Provider 返回的计费口径；
- Provider 未返回 Token 时，必须明确标记为估算。

#### 验收标准

- 默认安全模式下，压缩失败不影响请求完成；
- 任何公开节省率都有可复现输入和测试脚本；
- 真实任务正确率与未启用优化的基线可比较；
- 无法验证的质量状态不得显示为通过；
- 用户可查看单次请求的真实处理证据。

---

### 7.3 F-03 跨 Agent 上下文复用

#### 目标

让同一用户、同一项目中的多个 Agent 安全复用已经处理过的上下文，避免重复读取、重复解析和重复传输。

#### 允许复用

- 文件 Hash；
- 文件版本；
- 修改时间；
- 符号表；
- Signatures；
- Code Map；
- 已验证结论；
- 脱敏摘要；
- Recovery Pointer；
- 新鲜度与来源证据。

#### 默认禁止复用

- Secret；
- API Key；
- 凭据；
- 未脱敏敏感日志；
- 跨项目完整原文；
- 跨用户上下文；
- 来源不明内容；
- 无法验证新鲜度的内容。

#### MUST

- 项目级隔离；
- 用户级隔离；
- Agent 身份；
- Session 身份；
- 内容 Hash 校验；
- 文件变化后失效；
- 同 Agent 自命中防护；
- Relay 内容大小边界；
- Relay 不污染普通 Session Cache；
- Relay 失败自动执行正常读取；
- 每次命中生成来源和节省证据。

#### SHOULD

- 第一阶段只支持 `map`、`signatures`、脱敏摘要；
- 采用确定性、可测试的失效策略；
- 提供项目级禁用开关；
- 对敏感项目默认使用更严格策略。

#### 验收标准

- Agent A 处理文件后，Agent B 可复用相同版本的允许内容；
- 文件变化后旧 Relay 不再命中；
- 不发生跨项目数据污染；
- 不发生同 Agent 自命中循环；
- Relay 不可用时 Agent 仍能正常完成读取；
- Receipt 能区分“压缩节省”和“Relay 避免重复传输”。

---

### 7.4 F-04 Doctor 用户保障

#### 目标

Doctor 必须回答“d-token 是否真的在正确、安全地工作”，而不是只展示工程测试状态。

#### Doctor 检查域

1. Runtime；
2. Agent 发现；
3. Agent 接入；
4. cc-switch 状态；
5. 配置一致性；
6. Provider / Model 路由；
7. 压缩生效；
8. Recovery；
9. Secret 安全；
10. 配置残留；
11. 安装、升级与卸载；
12. 最近失败请求。

#### 每个检查项的最小字段

```yaml
check_id:
category:
status:
severity:
summary:
evidence_source:
verified_at:
failure_reason:
recommended_action:
action_available:
action_reversible:
post_action_verification:
```

#### 状态枚举

- `healthy`
- `warning`
- `failed`
- `unknown`
- `not_applicable`
- `not_verified`

#### MUST

- 每个状态有证据来源；
- 每个可修复问题提供直接动作；
- 修复前显示影响；
- 修复动作可撤销；
- 修复后重新验证；
- 区分配置、Runtime、压缩、路由和上游故障；
- 无证据时不能显示 `healthy`。

#### 验收标准

- 普通用户能够理解主要错误；
- 常见配置失配能被定位；
- 修复动作不会覆盖无关配置；
- 修复后状态有新的验证时间和证据；
- Doctor 可验证完整恢复或卸载路径。

---

### 7.5 F-05 Agent 与 cc-switch 配置闭环

#### 目标

用户只需要正常使用 Agent 和 cc-switch；d-token 负责感知变化、保持 Runtime 一致、记录证据并避免配置冲突。

#### 发现范围

- Agent 可执行文件；
- 用户级配置；
- 项目级配置；
- 环境变量；
- MCP 配置；
- cc-switch 配置；
- 当前 Provider；
- 当前模型；
- Base URL；
- 配置来源；
- 文件变更；
- d-token 管理状态。

#### 模式

##### 跟随模式

经用户授权后，d-token 跟随 cc-switch 当前上游选择。

##### 锁定模式

d-token 保持当前上游；外部变化仅提醒，不自动覆盖。

##### 观察模式

d-token 只检测和展示变化，不修改配置或 Runtime。

#### MUST

1. 检测配置变化；
2. 尽可能识别变更来源；
3. 更新只读状态；
4. 根据用户策略决定是否同步；
5. Provider 与模型保持显式；
6. 记录切换前后状态；
7. 冲突时请求用户选择；
8. 无法确认时不得自动覆盖；
9. 配置修改采用事务与恢复机制；
10. Runtime 状态与磁盘配置分别记录。

#### 验收标准

- 用户通过 cc-switch 切换后，d-token 能识别当前状态；
- 跟随模式下 Runtime 与授权选择保持一致；
- 锁定模式下外部变化不会静默覆盖 d-token；
- 观察模式下不发生任何写入；
- 冲突事件进入 Event Ledger；
- 用户可查看变更来源、时间和结果。

---

## 8. 系统处理流水线

所有请求必须收敛到统一流水线，不得形成多套相互竞争的压缩、缓存或路由链路。

```text
Agent Identification
  → User / Project / Session Scope
  → Config and cc-switch State Resolution
  → Context Relay Lookup
  → Query-aware Context Selection
  → Trusted Compression
  → Post-validation
  → Recovery Manifest
  → Explicit Provider / Model Resolution
  → Upstream Request
  → Provider Usage Collection
  → Optimization Receipt
  → Event Ledger
  → Doctor / Analytics Projection
```

### 流水线约束

**MUST**

- 同一内容只能由一个阶段执行实际变换；
- Context Relay 命中不得重复计为压缩收益；
- Query-aware Selection 必须保留恢复路径；
- Trusted Compression 不得删除用户明确要求保留的内容；
- Provider / Model 解析结果必须可追踪；
- 任一优化阶段失败都可以安全降级；
- 主请求结果不依赖 Analytics 成功；
- Event Ledger 是配置、路由、压缩和恢复事件的事实来源；
- UI 只展示有来源、时间和状态的投影数据。

---

## 9. 产品信息架构

一级页面应控制在用户可理解的范围内。

### 9.1 Overview

展示：

- Runtime 状态；
- 已发现和已接入 Agent；
- 当前 Provider 与模型；
- cc-switch 当前状态；
- 今日优化请求；
- 今日 Local Token 避免量；
- Provider Token；
- Context Relay 命中；
- 最近异常；
- Doctor 总体状态。

### 9.2 Install & Connections

展示：

- Agent 自动发现；
- 接入能力；
- 当前配置来源；
- 配置 Diff；
- 授权；
- 写入验证；
- 恢复点；
- cc-switch；
- 跟随、锁定、观察模式。

### 9.3 Context

整合展示：

- Session Context；
- Context Relay；
- Code Map；
- Signatures；
- Memory；
- Stale / Superseded Context；
- Recovery Pointer。

禁止为 Headroom、LeanCTX 等上游项目创建用户级独立页面。

### 9.4 Analytics

展示：

- 单次 Optimization Receipt；
- Local Token；
- Provider Token；
- 处理方式；
- 路由结果；
- Relay 命中；
- 跳过、失败与降级；
- 成本和延迟趋势。

### 9.5 Doctor

展示诊断、证据、风险、直接修复和修复后验证。

### 9.6 Changes & Recovery

展示：

- d-token 修改过的文件和字段；
- 修改时间；
- 修改来源；
- 配置 Diff；
- 恢复点；
- 回滚操作；
- 回滚验证。

### 9.7 Settings

仅保留用户需要的策略：

- 跟随 / 锁定 / 观察；
- 安全 / 标准 / 增强优化；
- 启动行为；
- 项目例外；
- 数据保留；
- 隐私；
- Telemetry；
- 自动更新；
- 高级调试。

---

## 10. 版本路线与优先级

### 10.1 `v0.0.1`：可信连接与可信压缩

#### P0 / MUST

- 首次使用引导；
- Agent 自动发现基础能力；
- cc-switch 配置扫描；
- 配置 Diff、备份、事务写入和恢复；
- OpenAI Chat；
- OpenAI Responses；
- Anthropic Messages；
- Headroom 核心可信压缩链路；
- Optimization Receipt；
- Local Token 与 Provider Token 分离；
- Doctor 验证 Agent、路由、压缩和恢复；
- OpenCode 与 Codex 至少各一个真实 routed-flow；
- macOS 和 Windows 候选安装包；
- 安装、升级、卸载和恢复路径；
- 正式 UI 不使用模拟成功数据。

#### P1 / SHOULD

- Claude Code 真实 routed-flow；
- cc-switch 观察模式；
- 安全 / 标准优化策略；
- 项目级禁用；
- 可导出的脱敏 Receipt。

#### 暂不进入本版本

- 完整跨 Agent Context Relay；
- 自动修复高风险配置；
- 团队策略；
- 云端同步；
- 大规模 Provider 扩展。

#### 发布门槛

- 至少两个 Agent 完成真实请求；
- 配置修改和恢复均通过真实文件验证；
- 不静默改变 Provider 或模型；
- 压缩失败时原始请求仍可完成；
- Doctor 能识别核心失配；
- 双平台构建来自同一候选提交；
- 所有发布状态有证据。

---

### 10.2 `v0.3`：上下文连续性

#### P0 / MUST

- Context Bus；
- Session Cache；
- 跨 Agent Context Relay；
- Map / Signatures 复用；
- Query-aware Retrieval；
- Recovery Pointer；
- Stale / Superseded Context；
- CCR Crash Recovery；
- 长会话恢复；
- cc-switch 实时感知；
- 跟随、锁定和观察模式。

#### 发布门槛

- 两个 Agent 可复用同一项目的允许上下文；
- 文件变化后旧上下文自动失效；
- 跨项目污染为零；
- Relay 失败安全回退；
- Receipt 区分压缩与复用收益；
- cc-switch 切换后状态保持一致或明确提示冲突。

---

### 10.3 `v0.4`：自动化本地上下文控制面

#### 目标

- 多 Agent 稳定适配；
- 配置变化持续感知；
- 常见低风险问题自动修复；
- 项目与团队策略；
- 隐私安全会话报告；
- 完整真实任务 Benchmark；
- 企业代理与证书；
- 私有网络适配；
- 稳定签名；
- 自动更新；
- 长期运行可靠性。

#### 发布门槛

- 用户日常很少需要手动进入 d-token；
- 常见 Agent 与 cc-switch 切换稳定；
- Doctor 可修复大部分低风险问题；
- Token 节省和任务质量有公开可复现证据；
- 桌面端达到长期维护和正式分发标准。

---

## 11. 统一验收指标

所有指标必须注明：

- 数据来源；
- 统计时间；
- 样本量；
- 计算口径；
- 是否为估算；
- 对应版本；
- 是否可复现。

### 11.1 首次使用

- Agent 发现成功率；
- 自动定位配置成功率；
- 首次接入成功率；
- 首次真实请求成功率；
- 首次 Receipt 生成率；
- 首次失败可恢复率。

### 11.2 配置可靠性

- 配置写入成功率；
- 写入后验证通过率；
- 一键恢复成功率；
- 外部变化识别准确率；
- cc-switch 状态同步正确率；
- 无关配置覆盖事件数；
- 不可恢复配置事件数。

### 11.3 压缩可信度

- 真实任务成功率；
- 与 Direct 基线相比的质量变化；
- Local Token 减少；
- Provider Token 减少；
- 压缩误伤率；
- 自动降级成功率；
- Recovery 成功率；
- Receipt 字段一致性；
- 无证据成功状态数。

### 11.4 Context Relay

- Relay 命中率；
- 避免的重复读取量；
- 避免的 Local Token；
- 文件变化失效准确率；
- 跨项目污染事件数；
- 自命中循环事件数；
- Relay 失败回退成功率。

### 11.5 Doctor

- 检查覆盖率；
- 诊断准确率；
- 一键修复成功率；
- 修复后验证通过率；
- `healthy` 状态证据完整率；
- 无证据绿色状态数。

---

## 12. 数据与状态语义

### 12.1 状态不得混用

以下状态含义必须明确：

- `detected`：发现了文件、进程或配置；
- `configured`：配置中存在 d-token 相关设置；
- `connected`：真实请求已成功经过 d-token；
- `verified`：连接、路由或能力具有当前证据；
- `healthy`：对应检查项当前通过；
- `active`：当前 Runtime 正在使用；
- `supported`：代码声明支持，不代表当前环境已验证。

不得仅凭配置文件存在就显示 `connected` 或 `healthy`。

### 12.2 时间与新鲜度

所有状态投影必须尽可能携带：

- `observed_at`；
- `verified_at`；
- `source`；
- `freshness`；
- `revision`。

过期状态必须降级为 `stale`、`unknown` 或 `not_verified`。

### 12.3 估算数据

估算数据必须：

- 明确标记 `estimated`；
- 说明计算方法；
- 不与 Provider 实际值合并；
- 不用于宣称真实账单节省，除非具有 Provider 计费证据。

---

## 13. 隐私与安全要求

### MUST

- 本地优先；
- 默认不上传代码、Prompt、日志原文和凭据；
- Secret 使用系统安全存储；
- 日志默认脱敏；
- Receipt 分享默认移除源代码和 Prompt；
- Context Relay 使用用户与项目作用域；
- 敏感内容不默认进入 Relay；
- Telemetry 可关闭；
- 用户可配置数据保留；
- 用户可清除本地数据；
- 卸载时可恢复 Agent 和 cc-switch 配置；
- 权限不足时明确失败，不绕过系统安全机制。

### SHOULD

- 提供隐私模式；
- 提供项目级数据隔离策略；
- 提供敏感内容检测；
- 提供数据目录说明与导出；
- 提供安全事件审计。

---

## 14. 实现与 PR 拆分约束

后续 AI 生成实施计划时必须遵守：

1. 每个 PR 只解决一个主要能力域；
2. 不得把首次引导、Runtime、Relay、Doctor 和发布全部塞进一个 PR；
3. 每个 PR 必须列出：
   - 目标；
   - 非目标；
   - 修改文件范围；
   - 数据结构；
   - 兼容性；
   - 测试；
   - 回滚；
   - 验收证据；
4. 涉及配置修改的 PR 必须提供真实配置 Fixture 和恢复测试；
5. 涉及 Token 的 PR 必须明确 tokenizer 与 Provider 口径；
6. 涉及 UI 的 PR 不得用 Mock 数据表达正式成功状态；
7. 涉及 Context Relay 的 PR 必须先证明隔离、失效和回退；
8. 涉及发布的 PR 必须使用同一候选提交生成双平台证据；
9. 未完成的能力必须在文档和 UI 中标记为 Preview、Beta、Experimental、Partial 或 Unavailable；
10. 不得因追求架构整洁而进行与当前目标无关的大规模重构。

推荐拆分：

```text
PR-01 产品状态模型与术语统一
PR-02 Agent / cc-switch 发现模型
PR-03 配置事务与恢复
PR-04 首次使用引导
PR-05 路由真实性与协议适配
PR-06 Trusted Compression Pipeline
PR-07 Optimization Receipt 与 Token 口径
PR-08 Doctor 用户保障
PR-09 Context Bus
PR-10 Cross-Agent Relay
PR-11 Query-aware Retrieval 与 Recovery
PR-12 双平台发布与证据
```

---

## 15. Definition of Done

一个能力只有同时满足以下条件，才能标记为完成：

1. 代码路径已实现；
2. 正式入口可达；
3. 不依赖 Mock 才能工作；
4. 真实环境或等价端到端测试通过；
5. 错误路径已处理；
6. 安全降级已验证；
7. 配置修改可回滚；
8. 状态和证据可在 UI 或日志中查看；
9. 文档已同步；
10. 许可证和来源已记录；
11. 未知限制已明确；
12. 对应版本门槛已满足。

以下情况不得标记为完成：

- 只有类型、接口或空实现；
- 只有单元测试，没有正式入口；
- 只有 UI，没有真实后端数据；
- 只有后端，没有用户可达路径；
- 依赖手动修改配置但文档未说明；
- 只在 macOS 或 Windows 单平台验证，却声称跨平台完成；
- 无法解释失败或恢复方式；
- 指标来自示例数据。

---

## 16. 后续 AI 输出模板

后续 AI 根据本文件提出方案时，建议使用以下结构：

```markdown
# 任务名称

## 1. 目标
说明本次只解决什么。

## 2. 对应要求
列出 F-xx、P-xx 和版本优先级。

## 3. 当前状态
区分已存在、部分存在、缺失和未知。

## 4. 非目标
明确本次不做什么。

## 5. 方案
架构、数据流、接口和 UI 行为。

## 6. 配置与数据影响
涉及的文件、字段、数据库和迁移。

## 7. 风险
安全、兼容性、性能、隐私和回滚风险。

## 8. 测试
单元、集成、E2E、真实 routed-flow 和故障注入。

## 9. 验收标准
可被明确判断通过或失败的条件。

## 10. 回滚
代码、配置和数据如何恢复。

## 11. 证据
需要生成的日志、Receipt、截图、构建产物或测试报告。
```

---

## 17. 未决决策与禁止假设

以下事项尚未由本文件最终确定。后续 AI 不得自行选择并当作既定事实；应在具体 PRD 或设计阶段给出方案、权衡和推荐值。

| 决策项 | 当前约束 |
|---|---|
| 首发支持的完整 Agent 清单 | `v0.0.1` 至少需要 OpenCode 与 Codex 真实 routed-flow；其他 Agent 需按证据确认 |
| cc-switch 支持范围 | 必须先识别实际配置格式、路径和变更行为，再决定写入与跟随策略 |
| 默认配置模式 | 跟随、锁定、观察三种模式均保留；默认值需结合首次使用风险评估决定 |
| 默认压缩策略 | 必须优先安全和可降级；不得直接默认启用高风险增强变换 |
| Context Relay 持久化方式 | 必须先满足项目隔离、新鲜度、容量边界和清理策略 |
| 数据保留周期 | 必须可配置；默认周期需经过隐私、磁盘占用和诊断价值权衡 |
| Telemetry 默认状态 | 必须可关闭；默认开启或关闭需在正式隐私策略中决定 |
| 自动修复范围 | 只允许低风险、可逆、可验证的修复进入自动流程 |
| Provider 回退策略 | 不得静默跨 Provider；具体允许的回退必须由用户策略定义 |
| 云端能力 | 当前不是核心运行依赖，任何引入都必须保持本地核心可独立工作 |

处理规则：

1. 未决项不得阻塞与其无关的基础能力；
2. 需要决定时，AI 应给出 2–3 个方案及权衡；
3. 推荐方案必须说明安全、兼容性、用户体验和回滚影响；
4. 决策完成后，应更新本文件或对应版本 PRD；
5. 不得通过临时代码默认值隐式替代产品决策。

---

## 18. 最终产品目标

d-token 最终应成为：

> **以 Headroom 可信压缩为核心、以 LeanCTX 跨 Agent 上下文复用为关键能力、以 Agent 与 cc-switch 自动配置闭环为使用基础的本地 AI 编码上下文控制面。**

最终能力闭环：

```text
自动发现并安全接入
+ 实时感知配置变化
+ 显式保持 Provider 与模型
+ 可信压缩
+ 跨 Agent 上下文复用
+ 丢失信息可恢复
+ 每次优化可解释
+ 所有配置可回滚
+ Doctor 持续保障
+ 用户专注开发
```
