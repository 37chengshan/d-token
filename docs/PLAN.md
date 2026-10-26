# d-token 执行计划（Living Plan）

状态：ACTIVE — 完成发布前全部任务，实施已对齐的前端与后端收尾
更新时间：2026-09-07

维护规则：本文件是仓库唯一正式 Living Plan。产品边界以 `docs/PRD.md` 为准；当前代码事实以源码、当前测试和 `docs/product/IMPLEMENTATION_STATUS.md` 为准。恢复阶段（R0–R2）的 Agent 详细执行分解已归档至 `docs/archive/release/2026-08-14-recovery-agent-ledger/`，只作追溯，不替代本文件。

## 0. 当前结论

2026-09-07 功能优先修复（覆盖上轮候选结论）：

- 后续恢复复核：路由写入失败后，仅在重新读取并核对原配置字节摘要后恢复原托管服务。任一文件恢复或服务重启失败都显示恢复未完成；错误正文不再作为“已回退”的证据。零工具成功显示连接未完成，部分失败保留成功工具；自动刷新不抹掉本次终态。请求日志的限流/超时增加用户可执行说明。
- 已暂停视觉调整。Doctor 将明确未配置的 Optical / 未安装的 cc-switch 识别为待设置，而非未知数据；真实 Preview 已完成核心服务修复、启动和复检。缺失/损坏配置仍区分，未降低未知和过期结果的写入门槛。
- OpenCode 原先仅注册 provider，未设置默认请求模型；安装入口现统一使用受管 transport，将 provider、默认模型和私有的原选择记录作为一个事务。支持旧自有 provider 更新、幂等重试、清除恢复原默认值；配置/记录变动、同名外部 provider 和符号链接阻止覆盖。普通安装不再走旧的 provider-only command。
- 安装预检失败发生在停启服务和写路由前；混合批次只应用通过预检的工具。事务后发生失败时按本次实际回执精确恢复，不能用清除全部接入代替回退。transport 应用阶段已按工具隔离失败，后续 Hybrid 部分完成回执也会触发该工具恢复，其他成功工具保留；仍需真实 App 批量点击复验。
- 当前源码前端 506 项全量、类型检查通过；安装事务覆盖混合预检失败、应用失败后继续配置其他工具和部分写入恢复。Rust 配置 crate 全量、OpenCode 11 项定向及原生发现回归通过；当前模块严格 Clippy 通过。随后全工作区 1072 项通过、6 项忽略；最终安装定向 35 项、类型检查及独立 debug Preview 构建通过。旧 release DMG 不能代表这些修改。
- 真实免费 MiMo 再试 `real-routed-20260906T155332Z-38784.json` 仍为 6 个 HTTP 429；原配置字节已核对恢复。OpenCode 默认接入的最新真实 App 点击链尚未复验；桌面工具再次报告锁屏，已通知用户。Goal 保持 ACTIVE，不宣称全部功能可用。

用户最新指令：将 Goal 更新为“完成发布前所有任务”。目标已生效，范围包括刚完成对齐的前端实现、后端未闭合功能、错误恢复、真实请求验收、当前候选构建与发布准备。此前“仅对齐文档、暂停实现”的阶段结束；以已恢复前端为基础执行最新 FRONTEND_CONSTRAINTS，不重新解释为另一套视觉风格。

当前推进顺序：先把进行中的后端修改收敛为可编译、可恢复的状态；并行实施安装小纸片卡、诊断与分析页面、统一反馈和动效；完成所有页面的文案及结构整理；统一前后端接口与失败状态；执行全量检查、真实 Tauri 和免费 MiMo 压缩链路验收；最后按既有授权范围完成候选打包、平台及发布准备对账。外部限流、未验证平台和签名/公证证据分别记录，不能提前标记目标完成。

2026-09-06 第二轮前端对齐已记录并进入实现：安装每排 3–4 张两行小纸片卡，整卡可选且详情交互独立；全页面删去无用文案与嵌套；诊断参考左右布局，圆环采用通过项/总检查项；分析突出首屏同时保留下方可滚动明细；右下角两秒 Toast 与行内错误、进度、弹窗、通知中心配套；沿用基准质感并细化全套动效。

前置恢复阶段已完成：源码组合 `5572998` + `99a7289` 的七个文件构建出与安装包同名同字节的 46 个 JS/CSS；该恢复基准的 473 项前端测试、类型检查与构建通过。已保存恢复前文件和完整覆盖前目录，未修改已安装 App 或撤销后端改动。后续功能修改在此基础上进行，不再要求新构建与旧安装包资源相同。

本轮实施进度与剩余门槛：

- 当前最终验证（本轮后续收尾）：502 项前端、类型检查通过；双浏览器 26 项全量通过，另两项旧重复标签断言更新后复检通过。最新 release App/DMG 与安装启动审计通过，SHA-256 `226800cd111408c2bee156a4b12f82458bf173d88adec6c771fa6af307bb7746`。当前摘要和原生逐项范围见 `verification-summary.json` / `native-followup.json`；免费 MiMo 压缩、完整原生黄金路径和余下页面语言审查仍未闭合。

- 22:50 后续原生复核修复：恢复页接回 `get_config_change_records` 专用接口，避免 runtime 离线错误影响本机备份；显式完整性失败优先于旧 verified 字段，未知 scope 禁止操作，恢复弹窗在读取失效后立即锁定。分析无数据合并为单一空态，有数据时保留全部深层明细；图谱项目提示修正为本页输入位置。设置移除重复成功条和来源标签，外观每次保存按完成 revision 给 Toast，不再推动页面或重放旧保存通知。最终候选验证结果以下方当前摘要为准。

- Hybrid 已修正部分失败被 verify 升为成功、撤销失败的 capability 丢失记录、验证/配置竞争早退丢失恢复关联；17 项 crate 事务测试、10 项 native capability 测试及对应严格 Clippy 通过。
- 真实服务生命周期失败来自旧测试配置缺失 `log_path`。更新配置 fixture 并重建代理后，隔离状态下启动/停止、空配置启动、Doctor 启动修复和托管服务重载通过；cc-switch 重载使用隔离 route 事务，不能据此声称用户真实数据库切换已验收。
- 通知已改为右下角最多三条、两秒后退出，悬停/聚焦暂停；查看打开对应记录，关闭不删除历史。公共操作弹窗保留失败原因与重试。图谱每批只汇总一次结果，初次静默扫描不刷通知。
- 安装两行小纸片卡、完整工具目录、选择保留、独立详情弹窗、诊断通过项/总项、分析模型明细与真实流量比例已实现。首次引导为五步，明确“开始配置”携带当时选择的一次性内存授权；普通查看/离开/过期/扫描变化均不自动配置。整卡 button 嵌套已改为独立复选框与详情入口，原生辅助功能已识别两者；详情按用户最后指令改为弹窗，失败结果保留在弹窗中。
- 图谱官方 v0.10.8 来源、短私有运行目录、项目隔离、CLI 输出上限、超时与终止清理已补强；非终态/失败 JSON 不再判 ready。实际小项目索引 8 节点/8 边及受限 MCP 查询通过；Windows 仍只有固定清单证据。
- Rust 全工作区 1065 项通过、6 项忽略；严格 Clippy 与格式通过。前端阶段全量 495 项、类型检查、Chromium/WebKit 28 项通过；两项高危 npm 依赖已在现有版本范围内更新，npm/Cargo 漏洞扫描均为 0（Cargo 另有 18 条警告）。macOS release App/DMG、包内容、ad-hoc 签名与临时安装启动审计通过。最后诊断空态/布局纠偏后的 495 项全量前端、类型、28 项双浏览器与重打包审计也已通过。当前摘要见 `support/artifacts/release-current-20260906/verification-summary.json`；最后空态仅浏览器实看，最终 release 二进制的全部原生点击链尚未复验，不宣称 Goal 完成。
- 原生诊断拒绝混合产品模块报告的根因已修复：接受明确登记的配置同步检查项，未知模块继续拒绝；实际扫描显示 1/6 通过、5/6 完成。普通窗口改为左右布局；未扫描时只保留就近说明和扫描按钮，不再显示空圆环或三层重复状态。部分结果下保留查看工具设置等只读处理入口，写修复仍受完整证据限制。

- 用户最新两项纠偏已实施：浅色外层底纸改为 #e9e5dc、工作区 #f0ede5，内容纸面仍为 #fbf8f2；设置强调色和深色主题独立。删除概览、分析及其余工作台常驻的重复数据来源标签，只保留影响操作的错误与过期提示。
- 免费 MiMo 再次真实请求仍为 `upstream_http_429`：`support/artifacts/opencode-real-routed/real-routed-20260906T140243Z-80548.json`，6 个终态失败请求，原配置精确恢复、临时凭据引用清理完成。未获得真实压缩成功证据；不重复密集重试或换付费模型冒充通过。

当前不继续推进旧的“大型发布 DAG”。原因不是发布需求取消，而是核心用户流程仍存在已经被多轮报告指出但当前源码尚未收敛的冲突；继续同时推进 canonical / Optical / CBM / 117 / release evidence 会扩大状态面和返工成本。

当前进入产品恢复模式：

```text
先稳定首次使用
→ 再稳定一键配置
→ 再稳定日常运行与 cc-switch 跟随
→ 再做八页真实用户任务验收
→ 最后从当前事实重新生成发布 lane
```

旧计划已保存为：

`docs/archive/release/2026-08-12-PLAN-pre-product-recovery.md`

旧 `.ai-bridge/release-iteration` 蓝图、审计与 release DAG 已进入其 `archive/2026-08-12-pre-recovery/`，只用于追溯。

### 0.1 合同一致性门（先于业务修复）

恢复计划本身不能成为另一套真理源。任何 Agent 开始 R1/R2 前必须先确认以下入口没有互相冲突：`AGENTS.md`、`docs/PRD.md` P0.0.10、本文件、`docs/product/IMPLEMENTATION_STATUS.md` 顶部恢复标记、`.ai-bridge/current-plan.md`、`docs/releases/v0.0.1/README.md`。发现“再次确认 / Provider chooser / 自动纳入不可取消 / whole-batch rollback / 旧 release DAG 可执行”等旧口径时，先修活动文档再改业务代码。

R0 的完成门必须包含一次“活动合同扫描”：旧语义只允许出现在明确的 archive/history 或带 FROZEN 警告的历史工作集中，不能作为当前执行指令。

## 1. 当前最高优先级：三条 Golden Path

### GP-A 首次启动与首次配置

目标流程固定为五段：

```text
欢迎简介
→ 启动本地服务（真实进度/反馈）
→ 扫描已安装 Agent
→ 用户调整默认勾选并一次点击“配置”
→ 开始使用
```

当前源码（2026-09-06）：已改为五步；服务启动有真实结果反馈，后续步骤依赖前置状态。引导到安装的一次性配置授权与普通只读导航已分开，选择和扫描版本变化会取消待执行授权。当前候选原生完整首次流程仍在验收。

完成标准：

- 行为先由失败回归测试锁定。
- 五段流程当前源码成立。
- service running 与 configured / connected / verified 状态分离。
- Reduce Motion 有静态等价。
- 当前候选真实 Tauri 首次流程通过。

### GP-B Agent 一键配置

`docs/PRD.md` P0.0.10 是当前最高优先级自动配置合同：

- 已安装 Agent 默认勾选，用户可以取消；未安装不可勾选。
- 用户点击一次“配置”即构成本次明确授权。
- 后台执行 plan / redacted diff / backup / apply / readback verify / receipt。
- Provider/model/protocol/endpoint/cc-switch profile 不提供 chooser。
- 每个 Agent 是独立事务。

当前已确认缺口（2026-08-24 对账更新）：

per-Agent 独立事务已实现（commit `cd3b383`）：Agent 验证失败只回滚自身并继续其余 Agent。剩余语义为两条，均在本轮 R3 修复范围：

1. 共享 route 本身失败/被回滚时，`Install.tsx` 会级联回滚指向该 route 的已应用 Agent 配置（P0-6 悬空清理，shared-infra consistency 例外，非 P0.0.10.5 冲突）——该例外语义需要「A 成功 B 失败 A 保持成功」与「route 失败悬空清理」双场景回归测试锁定（R3.A1）。
2. 一键配置失败路径不 refetch discovery/runtime 快照，失败/回滚后 UI 长期状态可能与磁盘不一致（R3.A1）。

完成标准：

- Agent A 成功、Agent B 失败时 A 仍成功。
- B 可单独回退、显示原因、单独重试。
- 已配置且未变化时只验证，不重复写。
- 普通用户不再被第二层工程确认弹窗打断。
- Route / MCP-Hook / Detection-only 结果诚实分层。
- 隔离真实配置完成 apply → readback verify → failure → retry → exact restore 验收。

### GP-C 日常运行与 cc-switch 跟随

目标：首次授权后，d-token 持续只读观察 cc-switch 当前生效配置；变化后自动更新 d-token 自有 route/runtime，不再次要求用户确认。

当前源码（2026-09-06）：已支持并校验 schema 12/16，未知版本仍拒绝；自动应用能力由已有授权决定。隔离 route 事务的服务重载、失败回退与快照测试通过；用户真实 cc-switch A→B→A 切换尚未取得当前候选证据，不以隔离测试代替。

完成标准：

- `wrote_cc_switch=false` 永久成立。
- A→B→A 变化能自动更新 d-token route。
- 每次同步有 backup / verify / receipt / Event Ledger。
- 同步失败保留 last-known-good。
- cc-switch 实际 app type 不静默丢失。
- 本机只读 A→B→A 验收通过。

## 2. 第二优先级：Codex / OpenCodex 真实链路

在 GP-A/B/C 收敛之后闭合：

```text
Codex
→ d-token listener
→ ocxlocal / OpenCodex
→ 当前动态模型上游
```

要求：

- 支持真实顶层 `openai_base_url` + `model_catalog_json` 形态。
- 保持 Responses / dynamic model 语义。
- 不读取或输出用户 auth/token。
- 同一候选产生真实 route decision、runtime identity、request / ledger evidence。
- 验收后用户配置精确恢复。

没有真实 routed-flow 证据前不能把 Codex 标为已验收。

## 2.5 第三优先级：通用 Agent 压缩层（R2.E，用户 2026-08-13 确认）

产品合同：`docs/PRD.md` P0.0.11（Runtime Discovery & Adaptive Compatibility）。

目标：实现「一次配置，新增模型 / Provider / route 无需重新配置」，作为「通用 Agent 压缩层」完成门。

- Runtime Capability Registry（按 model identity 记录 context_window / protocol / tool_call / stream / compression strategy / fallback），小版本与能力变化重新评估；不是固定白名单。
- 未知模型禁止失败：推断协议 → 默认压缩策略 → 记录 unknown → 学习更新；route 无法唯一解析仍 fail-closed。
- 不写死模型列表；新模型不要求重新发布或重新配置。
- 与 R2.C（cc-switch 跟随）、R2.D（Codex 动态透传）衔接；压缩策略复用统一压缩框架。

完成标准：一次配置后新增模型 A / Provider B / route C，下一次真实请求自动识别 model → 匹配 upstream → 选择 compression → 生成含 identity/capability/strategy/route/ledger 的完整 receipt。

执行位置：R2.D 之后、R3 之前（R3 八页验收需依赖本层真实状态）。

## 3. 第三优先级：八页用户任务验收

固定八页：

1. 总体概览
2. 安装与连接
3. 代码图谱（实验性）
4. 分析
5. 视觉压缩（Beta）
6. Doctor
7. 变更与回退
8. 设置

每页必须回答：

- 用户为什么进入此页；
- 第一主动作是什么；
- 成功后发生什么；
- 失败后如何继续。

验收：

- 1180×780 与 920×640。
- 中文 / 英文。
- Reduce Motion。
- 真实 Tauri 点击链，而非仅浏览器 preview。
- 无空白死循环、无效按钮、关键横向滚动、工程术语主导普通流程。
- stale / partial / unavailable / unknown 状态诚实。

## 4. 暂停的发布 backlog

以下需求没有取消，只是当前 `FROZEN`：

- canonical pipeline / sealed receipt / SendCommitReceipt；
- compression / CCR / quality / performance；
- Optical exact active / paired eval；
- CBM 五版本与兼容矩阵；
- 117 capability proof；
- 双平台最终证据与 packaging；
- 完整 release candidate / gate aggregation。

恢复条件：GP-A、GP-B、GP-C、Codex/OpenCodex 真实 routed-flow（R2.D）与八页用户任务验收均无 Blocker/High。R2.D 不是 release backlog 项，必须在重新开放 C0–C8/CBM/Optical 等扩张工作前闭环。

恢复时不复用旧 49+12 节点，也不复用旧 44/65 或任何历史完成百分比。根据当时当前源码重新生成最多四条 lane：

1. Runtime / canonical / receipt correctness。
2. Compression / CCR / Optical / quality/perf。
3. Agent / CBM / platform capability closure。
4. Release candidate / packaging / evidence。

## 5. 实施纪律

### 5.1 一次只修一个纵向用户切片

核心用户状态流同时只允许一个实现切片进入修改阶段。只读调查可以并行，但不得创建第二套计划、第二套状态机或第二份事实账本。R0 必须先冻结本轮唯一可写实施工作区和受保护 dirty 路径；pre-recovery 旧 worktree 默认不可信，不得因为名称含 final/release 就直接续跑。

### 5.2 TDD 优先

同一问题再次出现时：

```text
先写失败回归测试
→ 运行并确认 FAIL
→ 最小实现
→ targeted verification
→ 真实行为验收
```

不能再次只写审计报告。

### 5.3 PASS 的唯一标准

必须同时满足：

1. 当前源码实现；
2. 回归测试存在；
3. 当前候选测试实际通过；
4. 需要真实 App/runtime/routed flow 的行为有真实证据。

类型、fixture、mock、静态页面、历史报告、Agent 自述都不能单独构成 PASS。

### 5.4 状态报告不用百分比

当前进度只报告：

```text
Golden Path gates: x/4 PASS
Active Blocker: n
Active High: n
Unknown requiring verification: n
Frozen backlog: n
```

活动 Blocker/High 保持不超过 12 项；与当前 Golden Path 无直接关系的新发现进入 FROZEN backlog，不立即扩大实施范围。

## 6. 当前执行顺序

### 6.0 2026-09-05 至 09-06 决策与阶段记录（追溯）

本节保留多轮决策的先后过程；“用户接手前端、停止修改”等中间阶段已被 §0 最新授权替代。当前执行范围与证据只以 §0 和 §7 为准。

2026-09-06 最新分工：用户接手前端重构，Codex 停止修改 `code/apps/desktop/src/` 页面、样式及前端交互，转向完成后端功能与真实验收。保留已存在的前端改动；停止前安装完整清单/紧凑卡片与概览状态去重的 110 项相关测试通过，尚未重新打包实看。当前后端先闭合 OpenCode 免费 MiMo 的历史工具输出压缩与账本证据，再解决图谱组件版本冲突和完整索引/查询链路，并核对 Agent 配置、cc-switch 跟随、Doctor 与回退的失败恢复。原生接口尽量保持兼容；新增字段与状态同步到后端 README，供用户前端对接。签名、公证及 Windows 实包仍按 §6.1 的单独范围执行，不把代码测试等同于发布完成。

后端进展：完整 23 项 `agent_catalog` 已新增，检测事务集合保持兼容；压缩引擎固定原因码已贯通请求/事件；模型容量不再凭次数推算，真实工具调用/完整流观测已接入私有持久化；Doctor 修复状态与手动同步后的运行时检查已修正。Hybrid 批量部分成功与图谱私有运行目录正在做独立复核，之后统一执行 Rust 全工作区检查。MiMo 压缩准备的两次真实请求均为上游 HTTP 429；最新脱敏报告 `support/artifacts/opencode-real-routed/real-routed-20260906T100144Z-83232.json` 记录 6 条失败请求、`restored_exact` 和验证凭据清理。已区分临时公共示例项目与正常用户配置，不以测试失败或基础路由成功代替实际压缩验收。

2026-09-06 最新 UI 授权：用户明确允许按其 Modern Editorial / Warm Minimalist 说明重构除总体概览外的全部页面。用户进一步明确整体小字、适度留白、高信息密度、删除无用文案；取消设置双栏说明与卡片包围。最新允许安装工具区使用局部三列微卡片，禁止泛化为全应用卡片套层；删除重复大标题/说明和导航外框，扫描与批量配置集中于单行工具栏，工具详情使用弹出层避免撑开网格。首页保持不动；非首页样式限定于 `data-workbench`，不得通过共用 Surface 或全局覆盖影响首页。按安装、图谱、分析、视觉压缩、Doctor、回退、设置逐页检查实际渲染、正常/错误/空态与操作入口，未完成整组原生验收前不标记 UI 收尾完成。

2026-09-06 图谱入口补全：按用户要求，先只读识别已安装组件；支持选择官方压缩包，校验后导入到 d-token 自有目录。保留已解压目录作为折叠的替代方式，不在主界面预填或突出测试路径。兼容性判断继续绑定当前版本与文件摘要，不因检测到文件就标记可运行。

2026-09-06 用户再次纠偏后的执行优先级：

1. 核心路由与压缩引擎：使用用户指定的免费 `opencode/mimo-v2.5-free` 完成真实请求、历史工具输出压缩、route decision / optimization receipt / Event Ledger 一致性验收；不再以页面回归代替引擎可用。
2. 沿用用户认可的原有视觉基础，按本节顶部最新授权进行非首页局部结构调整；安装页采用紧凑工具网格并删除重复标题，不扩大为全应用卡片重排。直接研究 GitHub 项目的实际演示、源码和许可证作为核对，再修正标题、表面和控件层次丢失；强调色仍严格服从设置，保持紧凑与动画。`sahaj-b/claudeAI-frontend-clone` commit `7f77562c633e370ab1ad811907507f18bfcbc4c9` 已查看演示与组件，仅作为视觉研究（未声明许可证，不复制代码/资产）；`viewer12/ClaudeDesignSystem` 和 `13point5/open-artifacts` 为补充参考，后者实看截图不作为 Claude 暖色基准。
3. 图谱按 P0.7.2.1 的可选组件任务完成产品流程；`/private/tmp/dtoken-cbm-release-validation/package` 仅为独立 Preview 测试状态的验证目录，不能充当正式安装路径或默认体验的交付证据。

MiMo 基础真实请求（2026-09-06）：`support/artifacts/opencode-real-routed/real-routed-20260906T073113Z-97154.json` 返回 pass，1 条请求与 route/provider/model 对应，四类独立 JSONL 共 2/1/1/1 条，正常 OpenCode 配置 `restored_exact`。使用正常配置和状态，但本次显式关闭外部插件；该次没有可压缩历史工具输出，压缩前后均 129741，不能证明实际压缩，后续带工具输出场景单独验证。

2026-09-06 本轮安装与图谱验证：安装页三列/两列布局、取消选择与数量同步已在真实 Preview 窗口观察；随后详情改为 Popover 以消除整行撑高，其最终原生视觉检查仍待完成。图谱发现、受限压缩包导入与状态恢复已实现；组件 crate 25 项、桌面相关 12 项、实际官方压缩包导入 1 项、前端 575 项、类型检查、严格 Clippy 与规范 Tauri Preview 构建通过。Chromium/WebKit 38 项通过（36 项主跑 + 文案断言修正后的 2 项重跑）。Mac 锁屏阻止最终原生选择文件/确认导入及新详情弹出层实看；已请求手动解锁。测试 App 使用新的空状态目录，不把旧验证目录当作真实安装证据。

2026-09-06 整体验收对账（当前未提交工作树；不作发布通过结论）：

| 范围 | 当前已获得证据 | 仍缺少的最终证据 |
| --- | --- | --- |
| 安装、引导与连接 | 真实 App 只读导航、启动/停止、逐项未就绪提示；事务/隔离自动回归 | 已确认模型的真实 OpenCode 请求及账本关联 |
| 主题、密度、动画与通知 | 真实 App 主题与紧凑列表、通知详情/键盘；Chromium/WebKit 回归 | 最终候选 Windows 实包同等检查 |
| 总体概览与分析 | 真实无请求/旧数据状态，导出与聚合回归 | 同候选真实模型请求的图表与费用来源核对 |
| Doctor | 真实只读扫描与导航；修复前置、完整性、回执回归 | 真实可修复状态下的预览、执行与复检全过程 |
| 变更与回退 | 真实离线备份读取、预览/取消；精确恢复和并发拒绝原生回归 | 与最终真实连接验收同一事务的恢复证据 |
| 图谱与视觉压缩 | 原生目录选择器/取消、状态与配置回归；保持实验性/Beta 边界 | 当前组件与具体项目的完整可用流程复证 |
| 日志导出 | 真实 App JSON/CSV 落盘并解析，测试服务正常停止 | 带真实模型请求的脱敏内容核对 |
| 发布包 | macOS ad-hoc debug Preview、Rust/前端/类型/浏览器检查 | 候选 commit 绑定、正式签名/公证、Windows 实机 |

真实 OpenCode 验收模型已由用户明确为免费 MiMo，本次按 `opencode/mimo-v2.5-free` 执行；既有受管测试连接通过当前脚本的形状检查后最小替换，并精确恢复，结果见上方真实请求报告。

用户要求补齐发布前功能、减少普通界面的专业术语，并修复点击后没有通知或结果的问题。本轮保留当前工作树已有安装、cc-switch、Responses 压缩和开屏修改，优先完成用户指出不可用的「安装与连接」页，再按用户操作补齐其余页面反馈并统一验证：

1. 安装与连接优先：修复复选框占据整列、按钮换行的问题；提高信息密度；提供启动服务、批量与单工具配置、逐项结果、重试和验证入口；辅助集成与技术信息按需展开。
2. 一次点击严格绑定本次工具范围；重新扫描保留取消选择；预检与单项写入失败不得拖垮无关工具；共享服务/路由失败仍执行必要补偿。首次无配置时可启动仅供设置的空服务，未配置路由的模型请求继续明确拒绝。
3. App 统一通知：错误和待处理结果需明确确认；查看详情打开对应记录；提供返回任务页面的入口；去重、键盘、中英、窄窗口均可用。
4. 八页主动作与文案：点击显示处理中，避免重复执行；区分成功、部分完成、失败和浏览器预览；每个失败给出下一步；图表无数据时统一空态，不重复空卡或伪造数字。图谱组件目录可通过原生选择器回填，再执行既有校验。
5. 复核核心功能修改，执行回归、类型、构建、Chromium/WebKit 与真实 macOS Tauri 验证；将事实同步至实现状态。浏览器 UI、真实桌面、本地服务与真实模型请求证据分别记录。
6. 2026-09-06 用户视觉纠偏：强调色与图表色严格来自设置中的对应色块；保留原有白色按钮文字、开屏路径绘制、图标弹入与导航动效。小字固定为 12–13px，正文 14px，不通过缩小 rem 字号换取密度。安装工具采用局部三列响应式网格，主操作单一；工具详情就近弹出，不推高相邻卡片。

2026-09-06 实际 App 验收发现并修正：引导页「查看连接状态」和验证页「进入安装与连接完成验证」误用了自动配置回调；现已改用只读导航，普通导航同时清除遗留的一次性自动配置标记。复现中 Codex 的 `config.toml`、`AGENTS.md`、`hooks.json` 已从本次备份恢复，并核对原始字节哈希与权限；测试服务已停止，测试生成的 active config 已恢复为不存在并私有留存审计。新测试状态目录重跑「查看连接状态」没有生成配置事务或 active config，三个原文件哈希/权限未变化。批量准备失败的工具现在计入「部分完成」，不能因剩余工具验证成功而显示全部成功。

普通回退列表的工具配置备份已接入登记路径校验与现有精确恢复执行器；原生回归覆盖精确恢复、反向备份、并发改动、损坏备份、未登记路径和符号链接拒绝（10 项通过）。回退页将旧记录、文件变化和备份损坏翻译为可操作说明，并保留查看当前配置入口；失败历史与未验证完整性不再显示成功。仍需继续收尾：真实 App 的其余页面与完整请求链路验收。签名、公证和 Windows 实包仍单独受发布门禁约束。

本地恢复记录改由独立 `get_config_change_records` 命令读取，不再因模型服务统计过期而锁住有效备份。2026-09-06 重建的 macOS Preview 实测：服务停止时仍显示 7 个当前恢复点，有效记录可打开确认预览并取消；当前文件已变化的记录继续拒绝覆盖。此轮只读预览没有执行真实配置恢复。独立读取原生测试、17 项回退页面测试及 Chromium/WebKit 36 项端到端检查通过；严格 desktop clippy 与规范 Tauri 预览打包通过。

已执行的真实桌面检查：独立标识的 `d-token Preview.app` 使用本次测试状态目录，服务启动/停止、即时禁用与结果、未就绪工具的可见待处理通知、通知详情定位与键盘返回、原生目录选择器打开/取消均已观察。新的紧凑安装列表在同一 1162×768 窗口可见约五个完整工具，上一轮布局仅两个。图谱逐工具选择已在原生 App 验证，逐工具失败恢复和继续处理通过自动回归；真实配置事务仍需完成最终验收；这些 UI 检查不代表真实 Agent 配置或模型链路全部完成。

基线（修改前）：结构检查通过，前端 55 文件 / 494 测试通过。当前真实已安装 App 能打开，但其产物版本与当前工作树尚未绑定，不能据此认定当前候选通过。签名、公证、Windows 实包与真实模型验收必须分别记录实际证据，不能被 UI 收尾代替。

R0–R2.E 已完成（证据见归档 `docs/archive/release/2026-08-14-recovery-agent-ledger/TASKS.md`）。2026-08-23 用户确认进入 R3 可用性收敛轮，范围在原 R3 八页验收基础上扩展为：

```text
R3.a 文档/账本与授权边界修复（本节下方 2026-08-23 决策记录）
→ R3.b 八页真实 Tauri 逐页验收跑批，产出 Blocker/High 清单并修复闭环
→ R3.c 全 Agent Configure 覆盖：detection-only Agent 按官方文档调研补真实 adapter
→ R3.d 全 Agent 真实 UI 配置→注入→更新→恢复验收（不调用模型）
→ R3.e 智能路由真实验证（route decision / Capability Registry 未知模型 / cc-switch A→B→A 本机只读）
→ R3.f Codex Responses wire 压缩实现 + 真配置事务真模型验证 + 精确恢复
→ R3.g OpenCode 免费模型压缩/节省真实性验证
→ R3.h 开屏动画（矢量实现，小猫=App logo 同一形象）与 Dock 图标黑边修复
→ R3.i Headroom 等上游 delta 扫描 + 评测门控吸收（与 R3.b–g 并行）
→ R4 根据当前事实重新规划发布 lane（终点线：止于可用，发布另议）
→ R5 最终候选
```

### 6.1 2026-08-23 用户决策记录

1. 验收标准：R3 八页真实用户任务验收 + 修复闭环；用户本人真实使用终审。
2. 平台范围：macOS 优先；Windows 保持 CI 合同层。
3. Agent 覆盖：全部已装 Agent 做真实 UI 点击级「配置→注入→更新→恢复」验收，不调用模型；真实模型调用默认只用 OpenCode 免费模型。
4. Codex 例外授权（更新 AGENTS.md 第 5 节）：为验证 Responses wire 压缩，允许以「真配置事务 + 精确恢复」模式临时将真实 Codex 指向 d-token 并发送少量真实模型请求；对齐 OpenCode 授权模式（受限持久备份、最小变更、验证后精确恢复、不读取/打印凭据）。
5. Agent 全覆盖口径：detection-only Agent 按其官方文档逐个调研，凡存在可验证本地配置点的全部实现真实 Configure/Clear adapter；调研发现架构上无法本地路由验证的，携带证据回到用户决策，不得显示假可配置状态。
6. 开屏动画：矢量动画代码实现（非嵌入视频），小猫形象与 App logo/图标同一来源；Reduce Motion 提供静态等价。
7. Headroom 吸收：本轮并行执行，评测门控的全量 delta（安全修复直接吸收；压缩算法/策略变更须过 codec 评测集再采纳；非压缩面变更只记录）；codebase-memory 等 pinned 上游同步检查。
8. 终点线：本轮止于可用性收敛；R4/R5 发布 lane（签名/公证/双平台）另行规划。

## 7. 当前基础验证

仓库级基础检查仍保留：

```bash
python3 support/scripts/check_structure.py

cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

恢复阶段每个切片先跑 targeted verification；只有进入候选冻结时才要求全量门全部绑定同一 candidate/source digest。

## 8. 文档权威与归档

当前权威顺序：

```text
用户最新明确要求
→ docs/PRD.md 最新高优先级条款
→ docs/PLAN.md
→ 当前源码与当前测试事实 / IMPLEMENTATION_STATUS
→ 当前专题文档
→ history/
→ archive/
```

归档文件不删除历史信息，也不代表需求取消；它们只是退出当前执行线。

`docs/releases/v0.0.1/` 当前保留原位置，因为其中存在尚未吸收的本地修改；在产品恢复门通过前，它不是当前执行入口，不得覆盖本 Living Plan。后续进入 R4/R5 时再根据当前事实统一重写或归档。
