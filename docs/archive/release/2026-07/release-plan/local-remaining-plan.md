# d-token 剩余本地实施计划

## 0. 当前已完成的仓库内工作

本文件记录 ChatGPT 当前可直接在仓库中完成的 Phase 1-5 工作，以及剩下必须在本地真实环境完成的任务。

### 已完成：Phase 1 可落地部分

1. 用户可见主导航已从旧工程页收敛为 5 个主页面：总体概览、安装与连接、分析、Doctor、变更与回退。
2. `Advanced`、`LogsSecurity`、`Compatibility` 不再作为默认侧边栏一级入口，但代码页面仍保留，后续可迁移到“安装与连接”内部上栏。
3. App 名称从“d-token 控制台”调整为“d-token 本地连接中心”。
4. App tagline 改为“连接 AI Agent、同步上游、节省 token 并诊断问题”。
5. 安装页名称改为“安装与连接”。
6. 安装页副标题改为围绕“本地服务、cc-switch、AI Agent、配置变更预览”。
7. 安装页顶部新增 4 个工作区说明卡片：一键安装、智能学习、高级安装、日志。
8. 部分文案从 runtime/route 工程词改为“本地服务 / 连接路径 / 真实本地数据 / AI Agent”。
9. 顶部工具按钮的 title 从英文工程词调整为中文用户语言。
10. `docs/index.md` 已改成分类导航页。
11. 新增 `docs/DOCS_ORGANIZATION.md`，定义文档整理规范和后续迁移目标。
12. `docs/release-plan-2026-07/` 已完成实施规格、多轮审查和实施启动文档。

### 已完成：Phase 2-5 规划层可落地部分

1. Phase 2 已补齐 Agent 抽象模型、cc-switch 同步状态机、上游比较算法和安装事务算法。
2. Phase 3 已补齐智能学习扫描架构、评分公式、索引缓存、管理块写入协议和委托输入包。
3. Phase 4 已补齐分析数据模型、Doctor 问题模型、日志脱敏模型、回退点模型和共享运行状态模型。
4. Phase 5 已补齐页面完整度矩阵、测试矩阵、Release Blockers 和最终验收流程。
5. `multi-round-audit.md` 已记录多轮审查结论。
6. `implementation-kickoff.md` 已把 Phase 1-5 转成 Sprint 实施顺序。

## 1. 为什么还有本地任务

以下任务必须依赖真实本机环境，不能仅靠仓库静态编辑完成：

1. 读取真实 cc-switch 配置目录和 profile。
2. 观察用户切换 cc-switch 配置后的实际文件变化或运行状态变化。
3. 检测真实 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode 安装位置。
4. 验证不同 Agent 的真实配置文件格式和 hook 能力。
5. 启动真实 d-token 本地服务并确认端口、路由、日志和回退。
6. 对真实项目目录做智能学习扫描，评估性能和准确率。
7. 验证 Windows/macOS 路径、权限、编码、shell、端口占用差异。

因此，当前仓库已经具备开工规格，但仍需要本地实测补齐真实适配器和验收证据。

## 2. 剩余本地 Sprint 计划

### Sprint A：完成 Phase 1 UI 整合

目标：把旧页面内容真正合并到“安装与连接”内部上栏。

本地/代码任务：

1. 将旧 `Advanced` 页面里的高级配置区域迁到安装与连接页的“高级安装”上栏。
2. 将旧 `LogsSecurity` 里的安装/配置过程日志迁到安装与连接页的“日志”上栏。
3. 将旧 `Compatibility` 的 Agent 支持状态迁到安装与连接页或“支持的工具”局部模块。
4. 为“智能学习”上栏创建真实 UI 容器：扫描按钮、候选目录列表、排序说明、写入结果。
5. 为每个上栏补齐 loading、empty、ready、warning、error、success 状态。
6. 清理主路径中仍然出现的 runtime、payload、benchmark、fixture、route adapter 等工程词。

验收标准：

1. 侧边栏只显示 5 个主页面和设置。
2. 安装与连接页内能看到 4 个上栏，并且每个上栏不是空白。
3. 所有主页面都有空状态和错误状态。
4. 用户不需要理解 runtime/payload/adapter 也能使用主流程。

### Sprint B：完成 Phase 2 真实连接闭环

目标：真实检测 d-token 服务、cc-switch 和重点 Agent，并完成至少一个 Agent 的配置闭环。

本地任务：

1. 在本机安装或准备 cc-switch，并确认配置目录。
2. 收集 cc-switch 配置文件样例，记录 active profile/config/upstream 的字段。
3. 实现或修正 `cc-switch detector`。
4. 实现 active profile 识别。
5. 实现上游规范化比较：protocol、host、port、path、profile、provider。
6. 实现 cc-switch profile 切换后的同步检测。
7. 验证 d-token 能在 cc-switch 切换后同步上游或提示 out-of-sync。
8. 针对 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode 填入真实 `AgentDescriptor`。
9. 至少选择一个 Agent 完成：检测 → 计划 → diff → 备份 → 写入 → 验证 → 日志 → 回退。

验收标准：

1. cc-switch 当前 profile 能被 UI 显示。
2. cc-switch 当前上游能被 d-token 读取。
3. 用户切换 cc-switch 后，d-token 不会静默使用旧上游。
4. 至少一个 Agent 的真实请求能经过 d-token。
5. 所有写入都有备份和回退点。

### Sprint C：完成 Phase 3 智能学习真实扫描

目标：智能学习从 UI 骨架变成真实可用能力。

本地任务：

1. 实现候选目录发现器：常用项目目录、Git 仓库、Agent 会话路径、编辑器工作区。
2. 为 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode 补齐会话目录或项目关联目录规则。
3. 实现轻量索引：目录名、修改时间、项目标记、Agent 文档、语言指纹。
4. 实现评分公式：recency、agent activity、project confidence、doc gap、complexity、user preference、d-token history、risk penalty。
5. 在真实项目上测试排序准确率。
6. 实现深度扫描：README、docs、脚本、配置摘要、已有 Agent 文档。
7. 实现敏感文件排除：`.env`、密钥、认证文件、cookie、token、构建产物、依赖目录。
8. 实现 `LearningContext` 输出。
9. 实现 d-token 管理块写入。
10. 实现非管理内容不变性检查。
11. 实现写入前备份和写入后回退。
12. 实现 Claude/Codex 委托模式和提示词模式。

验收标准：

1. 智能学习能全面发现真实项目和 Agent 会话目录。
2. 大目录扫描不会卡死 UI。
3. 候选目录排序有可解释原因。
4. 直接写入只新增或更新 d-token 管理块。
5. 用户原有文档内容不被改动。
6. 写入可以一键回退。

### Sprint D：完成 Phase 4 分析、Doctor、日志和回退

目标：让用户看到 d-token 做了什么、出了什么问题、怎么修、怎么恢复。

本地任务：

1. 实现 `AppOperationalState` 聚合层。
2. 让首页、安装与连接、分析、Doctor、日志、回退使用同一个状态来源。
3. 实现 `AnalyticsSnapshot`：请求数、估算 token、估算节省、Agent 分布、上游分布、错误分布。
4. 实现 Doctor 问题库初版。
5. 实现低风险自动修复：重启服务、刷新检测、重新验证、清理临时状态。
6. 实现高风险修复确认：改 Agent 配置、改 hook、改 provider/upstream、写全局文档、执行回退。
7. 实现 `LogEvent` 和脱敏策略。
8. 实现日志导出 JSON/CSV，默认脱敏。
9. 扩展 `RollbackPoint`，覆盖 Agent 配置、d-token active config、智能学习文档、Doctor 修复。
10. 做跨页面一致性测试。

验收标准：

1. cc-switch out-of-sync 时，首页、安装页、Doctor 都显示一致状态。
2. 日志不泄露 API Key、token、cookie、认证头、完整 prompt、完整 response。
3. 分析页只显示“估算节省”，不伪装成真实账单。
4. 所有写入都能在“变更与回退”中看到。
5. Doctor 高风险修复必须确认。

### Sprint E：完成 Phase 5 发布收口

目标：达到公开 Beta / 候选发布标准。

本地任务：

1. 跑前端 build。
2. 跑后端测试，如果本机有 Rust/Cargo。
3. 跑 Tauri 桌面窗口手工验收。
4. 在 Windows 和 macOS 上验证核心流程。
5. 验证 d-token 服务启动、停止、端口冲突处理。
6. 验证 cc-switch profile 切换同步。
7. 验证至少一个 Agent 真实请求经过 d-token。
8. 验证智能学习写入和回退。
9. 验证日志导出脱敏。
10. 更新 README。
11. 写已知限制。
12. 按 Release Blockers 检查是否可发布。

验收标准：

出现以下任一问题，不允许发布：

1. d-token 本地服务无法稳定启动。
2. 首页无法区分真实数据和示例数据。
3. 第三方工具被自动安装或静默修改。
4. cc-switch 切换后 d-token 可能继续使用旧上游但 UI 不提示。
5. 配置写入没有备份或回退点。
6. 智能学习改动非 d-token 管理块内容。
7. 日志或导出泄露敏感信息。
8. Doctor 高风险修复没有确认流程。
9. 任意主页面出现无法解释的冷错误或空白页。
10. README 不能指导新用户完成安装、连接、验证和回退。

## 3. 本地优先执行顺序

建议你接下来按这个顺序执行：

1. 完成 Sprint A：页面整合。
2. 完成 Sprint B：cc-switch + 一个 Agent 真实闭环。
3. 完成 Sprint C：智能学习真实扫描和受控写入。
4. 完成 Sprint D：Doctor、日志、分析、回退。
5. 完成 Sprint E：跨平台和发布收口。

不要先追求支持所有 Agent。最优策略是先用一个 Agent 做成完整闭环，再复制到其他 Agent。

## 4. 当前建议优先级

最高优先级：cc-switch 上游同步。

原因：这是用户信任的核心。如果用户切了 cc-switch profile，但 d-token 还走旧上游，产品会显得不可控。

第二优先级：真实 Agent 配置闭环。

原因：没有真实 Agent 请求经过 d-token，所有分析、Doctor、日志都没有真实基础。

第三优先级：智能学习受控写入。

原因：这是产品差异化，但必须在写入安全和回退可靠后再扩大扫描范围。

## 5. 本轮仓库内完成情况总结

本轮已完成：

1. 收敛主导航。
2. 更新核心 UI 文案。
3. 在安装与连接页加入四个工作区说明。
4. 整理 docs 导航。
5. 新增 docs 目录整理规范。
6. 写成本地剩余实施计划。

本轮没有完成、必须本地继续：

1. 真实 cc-switch 检测。
2. 真实 Agent 检测和写入。
3. 智能学习扫描器。
4. Doctor 自动修复器。
5. 日志脱敏导出真实验证。
6. Windows/macOS 真实桌面验收。
