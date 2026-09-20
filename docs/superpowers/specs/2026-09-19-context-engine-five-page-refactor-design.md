# d-token 统一上下文引擎与五页应用重构设计

日期：2026-09-19

修订：2026-09-20，扩大 Jev 为完整实时上下文决策层

状态：方向 A、扩大后的 Jev 定位与书面规格已获用户批准；实施计划已生成，等待执行方式确认

类型：产品与系统架构规格

关联产品真理源：`docs/PRD.md`

关联执行计划：`docs/PLAN.md`

## 1. 摘要

d-token 将从按技术模块堆叠能力的桌面控制面，收敛为面向 AI Coding 的本地上下文优化控制面。用户通过一个应用完成编码工具接入、上下文优化、代码图谱使用、策略调整、运行分析、诊断和回退。

本设计确定以下产品和架构方向：

1. 一级页面收敛为五个：配置、高级配置、分析日志、诊断、设置。
2. 删除 Optical 图片转文本压缩产品线；普通多模态输入继续按上游协议透传。
3. 无 Jev Key 时，本地 Rust 引擎提供完整基础能力，不下载额外大模型，不依赖 Python sidecar。
4. 有 Jev Key 时，Jev 作为完整实时上下文决策层，参与工具输出在线裁剪、增量历史选择、会话级 compaction、恢复与检索排序；压缩执行、保护、预算、恢复存储和校验仍由本地引擎负责。
5. 模型路由与上下文压缩分开授权、分开配置、分开计量。启用 Jev 判断不授权自动更换主模型。
6. 建立唯一上下文编排链路，普通 HTTP、MCP 和桌面入口只承担协议适配，不复制压缩业务逻辑。
7. 采用渐进迁移：先建立可观测基线和新信息架构，再迁移本地引擎、Jev shadow 与 Optical 退出，最后删除兼容层。
8. 代码、规则、Skill、MCP、工具和模型路由复用同一套版本化 Decision Workflow 基础，但分别授权、校准、发布和关闭。

该规格定义目标状态和迁移边界。它不声明当前代码已经实现这些能力，也不替代后续逐文件实施计划。

## 2. 用户目标与成功标准

### 2.1 用户目标

普通 AI Coding 用户不需要理解 Headroom、RTK、CCR、ProviderProfile、capability registry 等内部术语，即可：

- 连接已有编码工具并知道连接是否真正生效。
- 在没有 Jev Key 时使用可靠的本地上下文优化。
- 在有 Jev Key 时选择是否启用语义判断增强。
- 在工具结果进入上下文前实时去除可恢复的噪声，并在长会话中保留仍然有效的历史。
- 查看一次请求执行了什么、为什么执行、去了哪个上游、能否恢复。
- 在出现问题时完成诊断、修复、复检和必要回退。
- 使用代码图谱和智能学习，而不让它们阻塞核心请求链路。

### 2.2 成功标准

产品成功不能只用压缩率判断。至少同时满足：

1. 五页结构覆盖安装、连接、配置、分析、诊断和回退任务，没有重复维护同一开关。
2. 无 Jev Key、Jev 不可用、代码图谱未启用时，核心路由仍然工作。
3. 受保护内容、工具调用依赖、签名或加密状态、未知协议块不会被任意改写。
4. 每次真实请求都能关联运行身份、实际路由、优化回执和 Event Ledger 证据。
5. 压缩失败可以安全保留原请求；请求已经向上游提交后不得自动重复发送原文。
6. Optical 旧配置无法重新激活已经删除的生产能力，历史记录仍可解释。
7. Jev active 只有在 shadow、replay、人工复核和预设质量门槛通过后才允许开启。
8. 实时压缩同时覆盖工具结果、增量历史和上下文压力 compaction，且每条链路可以独立停用和降级。

## 3. 范围与非目标

### 3.1 本次范围

- 五个一级页面及其信息职责。
- 统一上下文引擎的领域边界与请求生命周期。
- 本地确定性模式和 Jev 增强模式。
- 工具结果进入上下文前的在线裁剪、历史新 span 的 sticky 判断和会话级 compaction。
- 原文归档、恢复引用、代码/规则/Skill 相关性选择。
- 决策问题的版本、静态检查、预算和失败语义。
- 模型路由的独立授权边界。
- 代码图谱与智能学习在新信息架构中的位置。
- Optical 生产能力、配置、页面和依赖的退出方案。
- 统一状态、操作回执、优化回执和分析证据。
- 渐进迁移与发布闸门。

### 3.2 非目标

- 本轮不把 Figma 设计导入列为交付内容。
- 不将 fast-jev-compaction、Winnow、Jev Codex Router、LitJev、LLMLingua 或外部 Headroom 作为运行时依赖。
- 不在首版默认下载或打包本地大模型。
- 不默认启用工具风险拦截、完成声明拦截或长期记忆写入；这些 workflow 只预留统一接口，必须独立评审和授权。
- 不承诺未经 d-token 工作负载实验验证的压缩率、成本降低或准确率。
- 不把所有历史 Headroom 能力继续定义为必须追平的功能清单。
- 不在应用中建立独立 benchmark、fixture、smoke test 或 testing center 一级页面。
- 不借重构修改用户凭据、扩大监听地址或放宽现有安全边界。

## 4. 设计原则

### 4.1 本地基础始终完整

Jev、代码图谱和智能学习均为增强能力。它们缺失、未授权、超时或失败时，不应让普通请求无法通过 d-token。

### 4.2 决策与执行分离

DecisionProvider 只输出结构化建议。保护规则、动作许可、预算、恢复准备、实际变换和 post-validation 由本地引擎控制。

### 4.3 先保护，再压缩

请求首先识别协议不变量、指令、开放工具调用、缓存边界和高风险精确文本。只有进入候选集合的内容才允许判断和变换。

### 4.4 可恢复不等于可随意删除

恢复引用只有在当前 Agent 的恢复工具可达、原文已安全保存、权限和 TTL 有效时才成立。即使可恢复，仍需评估模型是否会在正确时机取回原文。

### 4.5 可测量不等于已归因

本地 token 估算、Provider usage、缓存读写、Jev 成本和恢复请求分别记录。单次请求的估算差不能直接展示为已确认节省金额。

### 4.6 页面服务于任务

内部模块可以保留技术名称，一级页面只按用户任务组织。状态卡、动效和说明不能取代主要任务入口。

### 4.7 把判断移出请求临界路径

工具输出裁剪可以在结果回传前执行，增量历史判断优先在工具调用结束后或两轮请求之间执行。Provider 请求发送阶段只重放已验证的 sticky decision，避免对同一历史重复调用 Jev。必须同步判断时使用明确的数量、延迟和费用上限；达到期限时保留原文并继续请求。

## 5. 产品信息架构

### 5.1 配置

配置是默认首页，包含“压缩引擎”和“代码图谱”两个页签。

#### 压缩引擎页签

按顺序呈现：

1. 当前实际状态：服务是否运行、模式是什么、最近是否观察到真实请求。
2. 已发现的编码工具：选择目标后执行一次配置事务。
3. 每个工具分别显示待配置、配置完成、已观察到请求、失败或证据不足。
4. 引擎模式：本地智能压缩或 Jev 实时压缩。Jev 模式展示 Key 状态、连接状态、允许发送的内容范围、预算和 shadow/active 状态。
5. Jev 实时能力：工具输出裁剪、增量历史优化、会话 compaction、代码上下文排序；每项显示真实运行和降级状态。
6. 最近一次有效请求及“查看效果”入口，跳转到分析日志的对应记录。

配置写入成功与运行时请求生效必须分开显示。用户完成一次点击配置即授权该批已选 Agent 的受管写入；该流程沿用 PRD P0.0.10 的事务、备份、验证和回退合同。

#### 代码图谱页签

围绕单个当前项目组织：

1. 选择项目。
2. 检查组件是否可用。
3. 建立或更新索引。
4. 为选定 Agent 配置受限查询入口。
5. 打开图谱或查看索引状态。

索引失败或过期时明确显示来源与时间。代码图谱不可用不影响压缩引擎工作。

### 5.2 高级配置

高级配置只承载普通用户无需频繁访问的优化行为：

- 压缩策略和上下文预算。
- 智能学习和项目说明。
- Jev 决策策略和 shadow/active 控制。
- 工具输出阈值、增量判断、sticky decision 生命周期和会话 compaction 阈值。
- Jev Workflow：代码/文件排序、规则/Skill 选择、完成验证、记忆候选和诊断分级。
- 模型路由候选及授权。
- 原文恢复、决策缓存、TTL 和配额。

每项配置明确作用域：全局默认或项目覆盖。编辑草稿、已保存配置和运行时实际配置分开显示。

### 5.3 分析日志

包含“概览”“请求记录”“变更与恢复”三个页签。

#### 概览

展示少量可以解释的指标：

- 请求量和成功率。
- 本地估算的输入变化。
- Provider 返回的 usage 字段。
- Jev 调用次数、延迟和可用成本。
- 工具输出、增量历史、会话 compaction 分别产生的保留、裁剪、恢复和降级结果。
- 压缩跳过、降级和恢复次数。
- 有明确证据时的净成本估算。

#### 请求记录

按时间、Agent、项目、模式、路由和结果筛选。请求详情解释：

- 实际目标 Provider 和模型。
- 候选内容、实际变换和跳过原因。
- Jev 是否参与、采用哪个问题集版本、返回何种判断。
- 判断发生在工具返回、轮次间、请求发送或 compaction 中的哪个阶段。
- sticky decision 是首次生成还是直接重放。
- 本地校验是否接受或否决决策。
- 恢复引用是否创建、是否使用、是否过期。
- usage 来源和估算方法。

普通日志禁止保存 API key、Authorization、cookie、完整 prompt/response、源代码和大段原始命令输出。

#### 变更与恢复

承接现有 Changes 能力。配置、学习、诊断修复和迁移事务继续保留摘要、diff、backup、授权、verify 和 rollback evidence。导航合并不能削弱事务校验。

### 5.4 诊断

诊断将结果分为：需处理、可选未配置、正常、证据不足。

- 没有 Jev Key 是正常本地模式，不是错误。
- 没有代码图谱是可选能力未配置，不拉低核心服务健康度。
- 修复动作必须关联 operation_id。
- 修复完成后自动复检并关联原始问题。
- 首次未扫描时展示检查入口，不展示假定健康状态。

### 5.5 设置

设置只包含应用级偏好和管理：主题、语言、动效、启动行为、存储与隐私、更新和版本。压缩策略、Jev 模式和模型路由不在设置中维护第二套入口。

## 6. 目标系统架构

```text
OpenCode / Claude Code / Codex / Cursor / Cline / other Agent
  → loopback protocol adapter
  → dtoken-runtime-pipeline
  → dtoken-context-engine
      ├─ dtoken-compress
      ├─ dtoken-decision / workflow registry
      ├─ dtoken-context-store
      └─ dtoken-router
  → configured upstream
  → redacted Event Ledger / analytics / doctor / app projection
```

### 6.1 `dtoken-runtime-pipeline`

保留现有类型状态生命周期：

```text
ContextEnvelope
  → PreparedRequest
  → TransformArtifact
  → RecoveryPrepareReceipt
  → ReadyToSend
```

该 crate 定义请求在哪些条件下可以进入下一阶段，不承担所有具体压缩算法或网络客户端实现。

### 6.2 `dtoken-context-engine`

作为唯一领域编排器，负责：

- 识别保护块和候选块。
- 在工具返回、轮次间、请求发送和 compaction 四个时机调度对应 workflow。
- 调用内容类型 codec。
- 按策略调用 DecisionProvider。
- 执行预算和收益检查。
- 在需要时先准备恢复数据。
- 应用变换并执行 post-validation。
- 生成 ReadyToSend 与优化回执。

它可以作为新 crate 建立，也可以先在 `dtoken-core-api` 内部以模块落地；无论物理位置如何，依赖方向和唯一编排责任保持不变。

### 6.3 `dtoken-compress`

负责：

- 内容分类。
- 候选生成。
- 日志、搜索、构建、测试、JSON、配置、代码围栏等专用 codec。
- 重复内容处理。
- 受保护内容规则。
- 工具调用配对与协议校验。
- 变换后的结构和语义不变量校验。

首版不对任意源代码、补丁、精确报错执行无边界 token 删除。

### 6.4 `dtoken-decision`

定义统一的 DecisionProvider、StateBuilder、QuestionSet、DecisionPolicy 和 WorkflowRegistry，以及三类 provider 实现：

1. `LocalDecisionProvider`：确定性规则、结构和有限词面证据。
2. `JevDecisionProvider`：TypeSafe Jev 托管 API，可选启用。
3. `ExperimentalCompatibleProvider`：未来 LitJev 风格 HTTP 端点，仅实验配置可见。

每个 Decision Workflow 绑定自己的触发时机、StateBuilder、问题集、动作集合、阈值、超时、fallback、shadow 状态和 kill switch。工具输出裁剪、历史淘汰、代码排序、模型路由和完成验证不得共用一个固定阈值。

DecisionProvider 的最小输入包括：workflow_id、任务摘要、候选描述、允许发送的候选正文、保护信息、预算和问题集版本。

最小输出包括：

- candidate_id。
- 建议动作。
- Choice/Score 分布或 Noul 概率。
- 置信度是否可用。
- 决策 provider 和模型身份。
- workflow_id、workflow_version 和 policy_version。
- 问题集版本。
- 理由码。
- 延迟、usage 或失败原因。

### 6.5 `dtoken-context-store`

保存：

- 原文恢复对象。
- 决策缓存。
- sticky keep/omit decision、源内容 hash、精确 offset 和连续性边界。
- 工具输出 archive 及其权限、TTL 和恢复状态。
- replay 所需的脱敏候选和结果。
- 项目、会话、策略版本之间的关联。

原文对象与普通日志隔离，具有项目/会话权限、TTL、配额和删除机制。

### 6.6 `dtoken-router`

负责候选模型的能力过滤和最终路由执行。DecisionProvider 可以在用户批准的候选集合中给出建议，但不能绕过协议、工具调用、上下文窗口和结构化输出能力检查。

### 6.7 `dtoken-core-api` 与 Tauri

`dtoken-core-api` 收敛到协议、HTTP/流式 transport、公开 API 和 UI 数据投影。Tauri commands 负责桌面桥接、平台权限、凭据库和系统集成，不重新实现领域决策。

## 7. 请求生命周期

### 7.1 输入保护

适配器将请求解析为 `ContextEnvelope`，保留原始协议身份、项目、Agent、会话、Provider 和模型来源。

引擎优先标记：

- system/developer 指令。
- 当前用户问题。
- 尚未闭合的工具调用与对应结果。
- 签名、加密或未知状态块。
- 已缓存历史前缀。
- 精确格式、结构化输出或协议约束。

这些内容默认不进入语义删除候选。

### 7.2 候选生成

本地引擎从新进入 live zone 的大型工具结果开始，优先寻找：

- 重复日志和重复命中。
- 测试通过明细中的重复结构。
- 构建输出中的冗余进度和相同警告。
- 大型 JSON 中可安全折叠的重复字段。
- 已被更新结果明确取代的历史工具结果。

历史缓存区域只有在上下文压力达到独立阈值时才重新评估，避免每轮改写稳定前缀。

### 7.3 决策

本地模式根据内容类型、大小、重复性、工具依赖和保护规则决定候选动作。Jev 模式只对本地已经确认可处理的候选进行结构化判断。

允许动作包括：

- keep_full。
- keep_structured。
- truncate_with_recovery。
- drop_duplicate。
- defer。

高风险内容和证据不足内容默认 keep_full 或 defer。

### 7.4 恢复准备

需要恢复的变换必须在修改请求前完成原文存储，并验证：

- 恢复工具对当前 Agent 可用。
- 恢复引用绑定当前项目和会话。
- TTL 和配额满足请求生命周期。
- 恢复标记符合上游协议。

准备失败则缩小变换或保留原文。

### 7.5 执行与校验

本地执行器应用动作后检查：

- 工具调用 ID 与对应关系。
- JSON/协议结构。
- 保护块完整性。
- 关键路径、错误位置和失败项。
- 变换是否达到最小收益。
- 恢复引用是否仍可用。

只有通过校验的请求才能成为 ReadyToSend。

### 7.6 上游提交

上游提交前失败可以安全使用原请求。上游已经接收请求后，不自动补发原文，避免重复执行工具、重复修改和重复计费。

### 7.7 四个实时执行时机

#### 工具结果回传前

对超过内容类型阈值的 Bash、构建、测试、搜索和其他大结果分块。错误、诊断、结果汇总、路径、首尾、源代码、diff、结构化文档和未完整评分的块强制保留。原文先进入受控 archive；Jev 失败或 archive 不可用时原样返回。

#### 两轮请求之间

对新进入会话的闭合 tool call/result、teammate report 和大型历史字段做增量判断。决定与源 hash、offset、策略版本和连续性边界一起保存。后续请求只重放 sticky decision，不重新判断旧内容。

#### Provider 请求发送前

执行协议校验、保护规则、sticky decision 重放和最终预算检查。该阶段默认不发起大量新的 Jev 调用；同步缺失判断达到期限后保留原文。

#### 上下文压力触发时

对超出宿主保留窗口的历史执行会话级 compaction。用户与 assistant 文本默认保持原文；tool call/result 成对选择保留全文、保留调用并截断结果、或成对淘汰。收益不足、状态无法装入、响应不完整或 Jev 不可用时回到宿主原生 compaction 或安全保留路径。

## 8. 无 Jev Key 的本地模式

本地模式默认启用，并满足：

- 不下载模型权重。
- 不启动 Python sidecar。
- 不向新的第三方发送上下文。
- 不要求用户理解或配置 DecisionProvider。

本地引擎使用：

1. 内容类型识别。
2. 工具调用依赖分析。
3. 命令专用确定性 codec。
4. 重复与包含关系检测。
5. 项目索引提供的路径、符号和新鲜度证据。
6. 本地关键词和符号匹配作为排序信号。
7. 恢复可达性和收益门槛。

词面相关性只能帮助排序，不能单独授权删除源代码或精确错误内容。

## 9. 有 Jev Key 的增强模式

### 9.1 Jev 的职责

Jev 是实时上下文决策层，负责把模糊的相关性、保留价值、优先级和验证问题转换成 Choice、Score、Noul 结果。首批压缩 workflow 包括：

| Workflow | 触发时机 | 判断目标 | 可执行动作 |
|---|---|---|---|
| `tool_output_retention` | 工具结果回传前 | 输出块是否仍需进入主模型上下文 | 保留、局部裁剪、原样降级 |
| `arrival_span_retention` | 工具结束或轮次间 | 新增历史 span 是否值得后续重复发送 | sticky keep、sticky omit、defer |
| `session_compaction` | 宿主 compaction 或压力阈值 | 旧 tool call/result 是否仍需完整存在 | 保留全文、截断结果、成对淘汰 |
| `context_ranking` | 检索或图谱查询后 | 文件、符号、代码片段和搜索结果的优先级 | 排序、top-k、abstain |
| `rule_skill_selection` | 新任务开始或意图改变 | 哪些项目规则、Skill、MCP 或命令需要加载 | 选择、拒绝、abstain |

架构预留但不随首批压缩自动开启的 workflow 包括：

- `memory_write_qualification`：候选事实是否值得进入长期项目记忆。
- `completion_verification`：Agent 的完成声明是否具有测试或结果证据。
- `diagnostic_priority`：Doctor 和日志问题的影响等级及处理顺序。
- `tool_action_review`：工具调用是否与当前用户目标一致；它不是操作系统安全边界。
- `model_effort_routing`：在用户批准的候选集合中选择模型层级和推理强度。

这些 workflow 共用 provider、凭据、预算和可观测性基础，但不共用动作权限和发布状态。

Jev 不负责：

- 任意摘要生成。
- 修改协议结构。
- 改写用户凭据。
- 绕过本地保护与预算。
- 从未批准模型中自行选择路由。
- 把 schema-valid 判断宣传为一定正确。

### 9.2 数据最小化

默认只发送完成当前 workflow 判断所需的最小状态：当前任务、候选描述、必要候选正文、稳定约束和有限历史证据。不同 workflow 分别声明会发送的字段。用户可选择更严格的数据范围；如果授权范围不足以判断正文语义，引擎使用本地策略，不伪装为语义判断成功。

### 9.3 问题集管理

每套 Jev 问题具有：

- 稳定标识和版本。
- 适用内容类型。
- criteria 文本。
- 允许动作映射。
- 校准数据版本。
- 风险等级。
- 触发时机和允许读取的数据范围。
- 超时、abstain、fallback 和 kill switch。

发送前执行离线 lint，拒绝：单选项 Choice、单层级 Score、复合问题、条件问题、裸标签、缺少逃生选项、上下文超限和无法映射的输出。

### 9.4 批处理和预算

同一 workflow 中共享 state 的独立问题可批量发送，但受总 token、问题数量、延迟和费用预算约束。不同 workflow 不为节省一次请求而混合不相关 state。缓存键至少绑定项目/会话范围、候选内容摘要、任务版本、Jev 模型、workflow 版本和问题集版本。

### 9.5 失败与降级

Key 缺失、凭据错误、超时、限流、响应不完整、解析失败、低置信度或覆盖不完整都生成明确 reason code，并回到该 workflow 的保守动作：上下文 workflow 保留原文，排序 workflow abstain，路由 workflow 保留原模型，验证 workflow 只记录 unknown。

### 9.6 Shadow 到 Active

Jev 上线分三步：

1. Shadow：记录判断，不修改生产请求。
2. Guarded active：只对已验证内容类型和低风险动作生效。
3. Expanded active：在新内容类型或新 workflow 单独通过门槛后扩展。

每个 workflow 独立推进这三个状态并保留 kill switch。工具输出 active 不代表历史 compaction、模型路由或完成验证已经获得授权。

## 10. 模型路由边界

模型路由位于高级配置，默认关闭。启用后：

1. 用户先批准候选 Provider、模型和最大成本范围。
2. 本地能力过滤先排除协议、工具、上下文窗口或结构化输出不兼容项。
3. Jev 或本地策略只在剩余集合中建议路由层级。
4. 低置信度、网络失败和能力未知时保持原模型或使用用户指定的保守回退。
5. 未完成工具循环遵循既定模型和协议约束，避免中途破坏状态。

路由回执记录原模型、实际模型、候选集合、决策来源、问题版本和回退原因。模型路由收益与上下文压缩收益分别核算。

## 11. 代码图谱与智能学习

### 11.1 代码图谱

代码图谱按项目建立索引，提供符号、路径、关系和必要片段。它向上下文引擎提供相关性证据，但不进入每次请求的同步索引流程。

图谱减少的检索量与压缩减少的请求输入分别记账。索引过期、失败或未安装时，引擎独立工作。

### 11.2 智能学习

智能学习是后台流程：

```text
选择项目与数据来源
  → 本地增量扫描
  → 提取结构、构建命令、测试入口和稳定约定
  → 生成带来源的建议
  → 用户查看变更
  → 应用到受管区域
  → 保存回执和回退点
```

无 Jev 时使用结构事实和模板建议；有 Jev 时可以筛选冲突或相关候选。Jev 不承担任意长文生成。学习不得阻塞请求热路径，也不得未经授权改写整份 AGENTS.md、CLAUDE.md 或全局说明。

## 12. Optical 退出设计

Optical 退出覆盖完整产品链路：

1. 删除正式导航、页面、引导、帮助和 Doctor 检查。
2. 新配置不再接受 Optical 启用字段。
3. 旧配置读取器识别相关字段并生成迁移回执。
4. 回退旧配置时拒绝重新启用 Optical，并给出版本化错误。
5. 删除生产 renderer、PNG patcher、专属 capability/eval gate 和仅为该能力存在的依赖。
6. 将通用保护 IR、秘密分类、工具依赖和发送前校验从 Optical 耦合中提取。
7. 历史 Event Ledger、备份和导出保持只读解释能力。
8. 清理活动 PRD、PLAN、状态、发布要求、文案、打包资产和测试。
9. 普通图片和多模态内容继续按上游支持能力透传。

Optical 迁移必须在删除配置类型前先提供读取与转换路径，避免旧用户启动失败。

## 13. 数据合同

### 13.1 状态快照

统一状态快照至少包含：

- revision。
- captured_at。
- state。
- source。
- freshness。
- reason_codes。
- scope。

前端将技术证据翻译为用户文案，不由各页面单独推断服务健康。

### 13.2 操作回执

用于配置、学习、修复和迁移：

- operation_id。
- operation_kind。
- requested_scope。
- stages。
- per_target_result。
- backup_reference。
- verify_result。
- rollback_reference。
- final_status。

### 13.3 优化回执

演进现有 `OptimizationReceipt`，保留并扩展：

- request_id、session_id、project_id、agent_id。
- 原目标与实际 Provider/model。
- provider/model 是否变化。
- engine_mode、workflow_id、workflow_version、policy_version、question_set_version。
- decision_stage、source_hash、source_offset 和 sticky decision 是否重放。
- 候选、采用动作、否决动作和原因码。
- 输入覆盖是否完整、保护规则是否覆盖决策、archive 和 recovery reference 状态。
- 本地估算 token、Provider usage 和估算来源。
- Jev usage、延迟和成本字段。
- validation、recovery 和降级结果。
- claimable 与 estimated 的收益区分。

普通 UI 投影使用脱敏字段；需要原文恢复时通过独立受控存储访问。

## 14. 安全与隐私

- 所有服务默认只绑定 `127.0.0.1`。
- Jev Key 只进入操作系统凭据库，配置保存不透明引用。
- UI 明确区分“保存 Key”和“授权发送哪些上下文”。
- 每个 Decision Workflow 分别声明数据范围；启用工具输出裁剪不自动授权发送源代码、完整历史或长期记忆候选。
- 日志、导出和错误消息不包含凭据、Authorization、cookie、完整 prompt/response 或源代码。
- 决策缓存和恢复对象按项目、会话、用户权限隔离。
- Jev 请求遵循内容范围、预算和 retention 设置。
- 自定义 Jev-compatible endpoint 不自动继承官方信任等级。
- 外部开源仓库不进入构建依赖，除非另行完成来源、许可证、NOTICE、安全和维护审查。

## 15. 可观测性与评估

### 15.1 Replay corpus

从脱敏的真实请求结构和受控 fixture 建立覆盖以下任务的 replay corpus：

- 构建错误定位。
- 跨文件代码修改。
- 精确字符串替换。
- 代码搜索和多轮工具调用。
- 长测试输出。
- 大型 Bash、搜索、构建和安装输出在进入上下文前的在线裁剪。
- 新 span sticky 决策在后续轮次的一致重放。
- 多次 compaction 后原始 tool call/result 的成对完整性。
- 历史约束保留。
- 恢复引用调用。
- 代码、规则和 Skill 排序的 abstain 与错误候选。

### 15.2 比较组

同一任务至少比较：

1. 原文基线。
2. 本地模式。
3. Jev 工具输出 shadow/active。
4. Jev sticky history shadow/active。
5. Jev session compaction 候选。
6. 宿主原生 compaction 对照。

控制主模型、输入、预算和缓存条件。随机任务执行多次。

### 15.3 指标

- 任务完成率与测试结果。
- 关键事实和约束保留率。
- 工具调用有效性。
- 本地估算输入变化。
- Provider usage 与缓存字段。
- Jev 附加延迟和成本。
- 各 workflow 的调用次数、缓存命中、sticky replay、超时和失败率。
- 请求临界路径等待时间与轮次间后台判断时间。
- 恢复次数、恢复成功率和恢复后任务结果。
- regret：被删除内容随后是否需要取回，以及未能及时取回造成的失败。
- 校准误差和人工盲标一致性。

阈值在查看 active 结果前确定，不根据结果事后调整完成标准。

## 16. 渐进迁移阶段

### D0：决策冻结

统一 PRD、PLAN、实现状态、页面职责、Optical 退出和两种引擎模式。退出条件是活动文档没有相互冲突的产品合同。

### D1：可观测基线

统一状态、操作和优化回执，建立当前行为 replay corpus。退出条件是单个真实请求可以解释输入、动作、路由、usage 和恢复状态。

### D2：五页迁移

建立新导航和页签，旧 route 暂时映射到新职责。退出条件是配置、真实请求、分析、诊断和回退可以连续完成。

### D3：本地引擎

建立唯一 context engine，收敛确定性 codec、保护、恢复和 post-validation。退出条件是无 Key 模式通过真实 OpenCode 路由，异常时安全保留原文。

### D4：Jev Shadow

先实现共享 provider、workflow registry、问题 lint、预算、决策缓存和 shadow evidence，再分三个独立 lane 验证：

1. 工具输出块级裁剪 shadow。
2. 新历史 span sticky keep/omit shadow。
3. 会话级 compaction shadow。

退出条件是没有生产删除，每个 lane 分别完成 replay、校准、人工复核、超时和费用评估；一个 lane 通过不能替代其他 lane 的证据。

### D5：Optical 退出

完成旧配置迁移、历史只读投影和生产依赖删除。退出条件是新旧配置都不能激活 Optical，普通多模态透传没有回归。

### D6：Jev Active

按风险和执行时机逐步启用：先启用具备本地 archive 的工具输出 guarded active，再启用轮次间 sticky history，最后评审会话级 compaction。代码/规则/Skill 排序作为独立 workflow 进入 shadow；模型路由、完成验证和记忆写入仍保持关闭。所有 workflow 保留独立 kill switch 和本地降级。退出条件是任务质量、附加延迟、净成本、缓存行为和恢复指标同时满足各自门槛。

### D7：收尾与候选验收

删除旧 route、旧 PageId 和临时兼容层，完成文档、打包和发布证据。退出条件是 macOS/Windows、三协议和真实 Agent 验收绑定同一 candidate commit。

## 17. 备选方案与拒绝理由

### 17.1 一次性硬切

一次删除旧页面、Optical 和旧压缩路径，再整体接入新引擎。该方案最终结构直接，但当前工作树横跨 UI、配置和 runtime，且 Optical 影响类型、Doctor、回退、Agent bridge 和文档。一次硬切会把数据迁移、协议回归、页面状态和新引擎风险合并，因此拒绝。

### 17.2 默认本地大模型

把 LitJev、LLMLingua 或其他模型随 App 打包。当前缺少跨平台资源、质量、量化、包体和冷启动证据，且 LitJev 主要验证配置远超普通桌面默认条件，因此拒绝作为首版默认模式。保留未来显式下载的实验适配器边界。

### 17.3 多个外部 sidecar 串联

同时运行 Headroom、RTK、Winnow、LitJev 等外部服务会破坏 Rust-owned runtime、故障语义和统一证据链。社区项目只提供行为参考或评测基线，因此拒绝。

### 17.4 Jev 同时负责压缩和模型路由

该设计会让开启语义判断隐式改变主模型，违反显式路由和可追溯原则。两种能力必须分开授权，因此拒绝。

## 18. 研究来源与固定样本

研究样本固定在 `.ai-bridge/research/upstreams/2026-09-19/`，不属于产品依赖：

| 项目 | Commit | License | 采用边界 |
|---|---|---|---|
| [awesome-jev](https://github.com/fatwang2/awesome-jev) | `f036095ba8203001d40be955f3486cd3e0d0a81b` | MIT | 项目发现 |
| [awesome-jev-typesafe](https://github.com/valentynkit/awesome-jev-typesafe) | `bfe207b05b6a8ede9f7b76fcdc113f393415e933` | MIT | 实时 compaction、agent workflow 补充索引 |
| [fast-jev-compaction](https://github.com/tamaratran/fast-jev-compaction) | `e3f262a7f4d42bd8dd32ced30d26176f7cb545b0` | MIT | 工具配对、批量判断参考 |
| [Yoshi](https://github.com/compozy/yoshi) | `55c719718e5039f2276fbad804211682ae012f1e` | MIT | Claude/Codex 代理、sticky span 决策和实测正负结果 |
| [save-token-jev](https://github.com/IAmUnbounded/save-token-jev-clean) | `a7007354a8d3747f06ff82130561edb2822a17df` | MIT | Claude、Codex、OpenCode 与 transcript compaction 适配 |
| [jev-pruner](https://github.com/tamaratran/jev-pruner) | `47d017c34eab7690b95f075ce6f4839247c5dc0a` | MIT | 工具输出进入上下文前的实时裁剪、archive 和恢复 |
| [pi-jev-compaction](https://github.com/vava-nessa/pi-jev-compaction) | `4fcb116ef575ee69e6dcc5201771028bb91b00b0` | MIT | 宿主 compaction hook、原文保留和显式 fallback |
| [Jev Codex Router](https://github.com/0xNatoshi/jev-codex-router) | `8292b519659280884627a962c826ac7721136a64` | MIT | shadow、kill switch、路由分层参考 |
| [Winnow](https://github.com/GhalebDweikat/winnow) | `51d80b945c74c8384bc47fa817179f668289afd8` | MIT | replay、regret、恢复评估参考 |
| Jevkit | `4bda844160cca340897a72d4fa2763dd7b826a82` | MIT | 问题 lint 参考 |
| LitJev | `f21216c9fe5afe7fa52ff7064a402ee57fdbddd3` | Apache-2.0 | 实验性 compatible provider 研究 |
| [RTK](https://github.com/rtk-ai/rtk) | `0924356b4caba4989607227b7c8824d3d8098719` | Apache-2.0 | 命令专用 codec 行为参考 |
| [LLMLingua](https://github.com/microsoft/LLMLingua) | `5a4c78ae18ab17a98cf997e8259354e546081d64` | MIT | 离线评测基线 |

Jev 的协议和概率解释以 [TypeSafe AI Primitives](https://docs.typesafe.ai/primitives) 与 [Confidence](https://docs.typesafe.ai/confidence) 为依据。Headroom 使用仓库已有的 `v0.36.5` provenance 和 delta ledger；本轮未取得新的完整 checkout，不将 partial clone 视为证据。

## 19. 已知不确定项

- Jev 在 d-token 编码任务上的阈值、延迟和净收益尚未实测。
- 工具输出、sticky history 和 session compaction 的最佳执行阈值可能完全不同，不能从社区默认值继承。
- Yoshi 的公开结果同时包含有效裁剪和零收益高延迟案例，d-token 是否能通过轮次间判断消除热路径延迟需要实测。
- 本地规则对深层语义的上限需要通过 replay 数据确认。
- 现有 Optical 类型中哪些属于通用保护 IR，需要实施前按调用图确认。
- `dtoken-context-engine` 最终采用新 crate 还是 core-api 内部模块，需要在依赖图与最小迁移切片中决定。
- Windows 真实桌面、凭据库、进程和安装迁移仍需候选构建验收。
- 恢复工具在不同 Agent 中的可达性和主动使用率需要分别测试。

这些不确定项不会改变五页产品结构、本地基础、Jev 可选、路由独立授权和 Optical 退出等已确认方向。

## 20. 书面规格复核清单

- [x] 目标、用户和成功标准明确。
- [x] 五页职责没有重复配置入口。
- [x] 本地模式与 Jev 模式职责清楚。
- [x] Jev 覆盖工具输出实时裁剪、sticky history、session compaction 和上下文排序。
- [x] 每个 Decision Workflow 具有独立状态、阈值、fallback 和 kill switch。
- [x] 模型路由具有独立授权边界。
- [x] 请求生命周期包含保护、恢复和发送前校验。
- [x] Optical 退出包含配置、运行时、历史和多模态兼容。
- [x] 社区项目被限制为研究样本，不成为隐式依赖。
- [x] 失败和降级语义明确。
- [x] 评估覆盖任务质量、regret、延迟和净成本。
- [x] 未把尚未实测的数据写成产品承诺。
- [x] 迁移采用可回退阶段，不要求一次性硬切。
- [x] 用户完成书面规格复核。
