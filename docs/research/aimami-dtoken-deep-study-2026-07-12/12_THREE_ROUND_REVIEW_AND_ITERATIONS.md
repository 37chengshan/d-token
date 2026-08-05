# 三轮同维度审核与优化记录

## 1. 审核方法

三轮审核均使用完全相同的十个维度，不采用“第一轮只看前端、第二轮只看后端”的拆分方式。

| 编号 | 审核维度 |
|---|---|
| D1 | 事实可追溯性与来源边界 |
| D2 | 产品定位、用户价值与范围控制 |
| D3 | 架构、模块边界与可维护性 |
| D4 | 安全、隐私与配置写入 |
| D5 | 可靠性、并发与数据完整性 |
| D6 | 前端 UI、动画与可访问性 |
| D7 | 跨平台、打包、签名与发布 |
| D8 | 测试、可观测性与证据质量 |
| D9 | 开源许可证、供应链与商标边界 |
| D10 | 建议的可执行性、优先级与 DoD |

审核对象包括：

- AiMaMi 固定 commit 的 README、前端入口、Tauri lib、Repository、Process、Proxy、models、Tauri 配置、capabilities、package/Cargo 配置和贡献规范。
- d-token 当前本地活跃工作树的前端、Tauri、OpenCode adapter、upstream discovery、ConfigPatcher、Router、Core API、Event Ledger、Optical Engine、PRD/PLAN/实现状态与法律文件。
- 当前实际测试结果，而不是仅使用历史报告。

## 2. 第一轮审核

### D1 事实可追溯性

**发现**：初始结论过于依赖项目说明，容易把 AiMaMi 宣称的功能与直接代码证据混为一谈；d-token 也可能使用已提交状态代替当前未提交工作树。

**优化**：

- 固定 AiMaMi commit。
- 列出直接检查的源码文件。
- d-token 明确以活跃工作树为准。
- 将“源码事实”“本地验证”“架构推断”分开。
- 不把安全 allowlist 阻止的命令写成通过。

### D2 产品定位

**发现**：最初容易把两者都归类为“AI Coding 桌面工具”，差异不够清晰。

**优化**：

- 将 AiMaMi 定位为 Codex 本机工作流管理器。
- 将 d-token 定位为多 Agent 本地运行时与控制面。
- 增加“应做/不应做”边界。
- 明确 d-token 不转向账号运营工具。

### D3 架构

**发现**：只指出 AiMaMi 大文件风险，但没有对 d-token 自身中心模块进行同等严格审查。

**优化**：

- 识别 d-token `main.rs`、ConfigPatcher 和 Optical Engine 的集中风险。
- 给出 commands/services/agents/platform/dto 目标目录。
- 定义 Agent Adapter SPI。
- 给出 ConfigPatcher 拆分方案。

### D4 安全与隐私

**发现**：初始文档提到权限风险，但缺少完整威胁模型和事务不变量。

**优化**：

- 增加 CSP、Shell、Process、updater、secret、PID reuse、route fallback、配置覆盖等威胁。
- 记录 OpenCode adapter 的完整安全链。
- 定义未确认不写、审查绑定、外部修改不覆盖、精确恢复和失败可解释五个不变量。

### D5 可靠性与数据完整性

**发现**：未充分覆盖刷新竞争、外部配置修改、多进程 JSONL 和第三方 schema 漂移。

**优化**：

- 增加服务、配置事务、路由状态机。
- 增加 optimistic concurrency 和 receipt-driven refresh。
- 识别 cc-switch SQLite schema 耦合。
- 增加 JSONL lock/rotation/SQLite projection 建议。

### D6 UI、动画与可访问性

**发现**：前端建议偏视觉，缺少危险动作节奏、Reduce Motion 和真实状态约束。

**优化**：

- 单独形成 UI 与动画两份文档。
- 增加 Shell/Surface/Action Surface 三层。
- 增加页面、侧栏、分段控制器、状态步骤、图表和背景动画规范。
- 明确动画不得先于后端 verify。

### D7 跨平台与发布

**发现**：初稿对 macOS/Windows 的发布证据不够具体。

**优化**：

- 列出 macOS app/DMG、签名、notarization、xattr、Keychain、sidecar hash。
- 列出 Windows NSIS、签名、UTF-8、非 ASCII 路径、`.cmd` shim、Credential Manager 和缩放。
- 强调同一候选 commit 证据。

### D8 测试与证据

**发现**：测试结论缺少当前命令结果和噪声说明。

**优化**：

- 记录前端 108 项测试通过。
- 记录 Vite build、Rust workspace tests 和 Clippy 通过。
- 记录缺少 `typecheck` 脚本。
- 记录 Radix Dialog forwardRef warning。
- 增加 unit/integration/E2E/routed Agent/platform acceptance 分层。

### D9 许可证与供应链

**发现**：只写“Apache-2.0 可借鉴”不够，容易忽略商标与实际适配范围。

**优化**：

- 核对本地 `THIRD_PARTY_NOTICES.md`。
- 写明固定 commit、许可证副本、适配文件和修改边界。
- 明确未复制品牌、图标、截图、人物、二维码和声音。

### D10 可执行性

**发现**：建议较多，但缺少实施顺序和完成标准。

**优化**：

- 建立 P0/P1/P2。
- 新增 Phase 0–8 路线图。
- 每阶段增加 DoD。
- 增加两周与四到八周计划。

## 3. 第二轮审核

第二轮再次使用同样十个维度，重点检查第一轮修订是否仍存在片面结论、无法落地或内部矛盾。

### D1 事实可追溯性

**发现**：部分 AiMaMi 结论仍可能被读成运行实测；d-token 测试计数可能快速过期。

**优化**：

- 使用“固定源码显示”“本地测试显示”措辞。
- 研究入口保留当前测试结果，但长期事实文档优先引用命令。
- 增加源码证据索引。

### D2 产品定位

**发现**：建议吸收账号/配额体验，可能使 d-token 产品边界再次扩张。

**优化**：

- 把账号卡片改为 Provider/Instance 状态表达。
- 明确不管理账号买卖、批量注册和账号池运营。
- MCP/Skills 仅做检测和兼容性入口，不接管完整生命周期。

### D3 架构

**发现**：仅拆目录仍可能只是文件搬家，未定义依赖方向。

**优化**：

- 增加 UI → domain store → generated DTO → command/API → service → domain crate → platform adapter 的单向依赖。
- 明确 domain crate 不依赖 Tauri。
- 明确 read model 与 write model 分离。
- 为 Agent Adapter 定义安全合同。

### D4 安全与隐私

**发现**：Updater、sidecar 和 Tauri capability 的供应链风险未与发布流程绑定。

**优化**：

- 增加签名 updater、sidecar hash、禁止未签名降级。
- Capability 按功能拆分并进入 CI 审计。
- 增加 release CSP 要求。

### D5 可靠性与数据完整性

**发现**：前端轮询和后端操作进度缺少统一模型。

**优化**：

- 建议 snapshot 轮询 + Tauri event 混合模式。
- 窗口隐藏暂停轮询，恢复立即刷新。
- operation receipt 驱动精确 invalidation。

### D6 UI、动画与可访问性

**发现**：页面建议较完整，但没有把性能债务和动画加载关系纳入。

**优化**：

- 增加 chunk budget。
- 明确页面转场不能掩盖加载延迟。
- 图表轮询更新不重播全动画。
- 增加焦点返回和 Dialog ref 要求。

### D7 跨平台与发布

**发现**：系统能力可能被排在核心发布之后，导致成品体验继续缺失。

**优化**：

- 将 updater 的安全设计提前纳入 P0/P2。
- 单实例在配置事务扩展前实施，减少多窗口冲突。
- 托盘动作仍必须通过后端身份验证。

### D8 测试与证据

**发现**：仅测试成功路径不足。

**优化**：

- 增加 adversarial、property、fuzz、concurrency、failure injection 和 long-running tests。
- 增加 real model fidelity/cost/latency eval。
- 明确测试 stderr 零 warning。

### D9 许可证与供应链

**发现**：需要区分代码适配、视觉原则借鉴和自有实现。

**优化**：

- 在优秀思路文档中给出外部借鉴 design note 模板。
- 明确 `Surface` 是独立实现、导航动画为自有实现。
- 延续固定 commit 和修改声明。

### D10 可执行性

**发现**：路线图中多个大任务仍可能并行过多。

**优化**：

- 明确先收口工作树，再拆 main，再做 Adapter SPI，再做类型统一。
- Agent 接入顺序固定为 OpenCode → Claude Code → Codex → Cursor/Cline。
- Optical 真 Gate 放在 runtime/ledger/平台发布稳定之后。

## 4. 第三轮审核

第三轮继续使用相同十维度，目标是检查最终内容能否直接指导开发，并消除“写得很详细但不能验收”的问题。

### D1 事实可追溯性

**发现**：研究文档需要在仓库中可导航，不能只存在 Word。

**优化**：

- 拆分为多个 Markdown 专题。
- README 添加完整目录。
- `docs/index.md` 增加研究入口。
- 每份文档引用实际代码路径和证据文件。

### D2 产品定位

**发现**：产品目标仍可能被解释为“超过 AiMaMi 的功能数”。

**优化**：

- 将成功标准改为首个 verified connection 时间、安全回退、多 Agent 实证、双平台签名和真实证据。
- 明确不是 AiMaMi 多 Agent 复制版。

### D3 架构

**发现**：重构顺序若不谨慎，会与当前大规模未提交工作互相干扰。

**优化**：

- 增加 Phase 0 当前工作树收口。
- 要求按模块拆提交。
- 架构迁移先行为等价，再增加功能。

### D4 安全与隐私

**发现**：Optical active 代码存在，容易被误写为生产可用。

**优化**：

- 所有总结和路线统一写明 active 默认关闭。
- 把真实 Provider fidelity/cost/latency 和 route-bound attestation 列为硬 Gate。
- 禁止 shadow 计入 realized savings。

### D5 可靠性与数据完整性

**发现**：单实例可能被误当成解决配置并发的完整方案。

**优化**：

- 明确单实例只减少 UI 冲突。
- 文件 hash、target ownership、lock 和 restore guard 仍必须存在。

### D6 UI、动画与可访问性

**发现**：设计借鉴可能过度追求“像 AiMaMi”。

**优化**：

- 评估标准改为任务步骤、安全边界、真实数据、跨平台、测试和许可证。
- 保留 d-token 六页 IA、品牌、状态语义和 Evidence-first 设计。

### D7 跨平台与发布

**发现**：先完成 macOS 可能导致 Windows 永久滞后。

**优化**：

- macOS-first 只表示执行顺序，不表示 Windows 验收降级。
- 每个 install/patch/rollback 设计同时写 Windows 路径、UTF-8 和权限要求。
- 最终 Beta Gate 要求双平台证据。

### D8 测试与证据

**发现**：历史测试结果容易在后续继续被复制。

**优化**：

- 路线图规定完成状态绑定当前候选 commit。
- 长期文档优先写验证命令，阶段报告再写计数。
- 禁止用 browser preview、历史报告或代码存在标记完成。

### D9 许可证与供应链

**发现**：研究目录本身需要指向唯一法律入口，避免出现多个互相冲突的声明。

**优化**：

- README 只引用 `docs/legal/THIRD_PARTY_NOTICES.md`。
- 不在研究目录创建第二份法律真理源。

### D10 可执行性

**发现**：需要最终核对文档结构和链接。

**优化**：

- 新增源码证据索引。
- README 按阅读路径分类。
- 使用目录树、Markdown 链接检查和 Git 状态检查。
- 最终输出明确哪些文件实际写入。

## 5. 三轮后的最终改进

最终研究体系从单一报告升级为：

```text
总览
产品与用户旅程
前端 UI
动画与交互
Tauri/Rust 后端
状态统一
框架与模块边界
算法
安全与配置事务
优秀思路
差距与缺陷
实施路线图
三轮审核
源码证据索引
```

## 6. 最终质量判断

三轮审核后的文档满足：

- 同一组维度重复审核三次。
- 每轮都记录发现和具体优化。
- 结论同时覆盖 AiMaMi 与 d-token 的代码，不只看文档。
- 区分事实、验证与推断。
- 优点和缺陷均有覆盖。
- 建议包含优先级、依赖顺序和 DoD。
- 安全、跨平台、许可证和 Optical 发布边界没有被弱化。

## 7. 后续维护规则

- AiMaMi 上游版本变化时，新建日期化研究或更新固定 commit，不静默覆盖证据。
- d-token 核心代码和发布状态变化后，优先更新当前事实文档，再校准本研究中的差距。
- 若借鉴范围扩大，更新唯一 `THIRD_PARTY_NOTICES.md`。
- 研究建议进入正式执行前，必须与 PRD 和 PLAN 对齐。