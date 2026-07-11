# d-token 发布实施计划总索引

本文件夹是 `docs/PRD.md` 之后的执行层规划，用于把“发布版产品决策”拆成可研究、可实现、可审查、可验收的阶段规格。

## 当前状态

已完成多轮多维度审查和迭代。5 个 Phase 已从方向性计划升级为可直接开工的实施规格，补齐了页面映射、状态矩阵、cc-switch 同步状态机、Agent 抽象模型、智能学习评分算法、文档写入协议、Doctor 问题库、日志脱敏模型、回退点模型、测试矩阵和发布阻断条件。

当前结论：没有发现会阻塞开工的已知设计缺口。CommandCode 的具体适配资料需要在实现阶段补入 `AgentDescriptor`，但不影响整体架构启动。

## 规划边界

当前发布目标不是内部 demo，而是可公开试用的稳定 Beta / 候选发布版。第一版必须做到所有核心功能真实可用、UI 完整、中文文案友好、配置可备份可回退、日志可脱敏、问题可诊断。

d-token 第一版只安装和管理 d-token 自有本地服务。Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode、cc-switch 等第三方工具只检测、读取配置、生成配置计划和受控配置写入，不自动安装。

cc-switch 是第一版的关键上游与配置来源。d-token 默认只读 cc-switch 配置，但必须能读取当前 profile/config/upstream，并在用户切换 cc-switch 配置时自动同步 d-token 上游，避免 d-token 指向旧上游。

智能学习是第一版重要创新点。它必须全面扫描本机 Agent 会话目录和项目工作区，快速准确排序，并且可以直接写入项目级或全局 Agent 文档；但只能以 d-token 管理的追加块或插入块写入，不允许重写、删除、覆盖或改动用户原有内容。

## 文件结构

1. `phase-01-product-ia-ui-copy.md`：产品信息架构、页面重构、UI 文案实施规格。
2. `phase-02-install-agent-ccswitch.md`：一键安装、Agent 检测、cc-switch 读取与上游自动同步实施规格。
3. `phase-03-smart-learning.md`：智能学习、全面扫描、排序算法、文档安全写入实施规格。
4. `phase-04-analytics-doctor-logs-rollback.md`：分析、Doctor、日志、变更与回退实施规格。
5. `phase-05-ui-stability-release.md`：UI 完整度、稳定性、发布验收实施规格。
6. `multi-round-audit.md`：多轮多维度审查报告。
7. `overall-review.md`：跨阶段总审查。
8. `implementation-kickoff.md`：实施启动指令。
9. `local-remaining-plan.md`：本轮仓库内已完成事项与剩余本地真实环境任务。
10. `index.html`：计划介绍网页，可直接打开预览整体路线。

## 推荐执行顺序

先完成 Phase 1，把页面结构和文案规范固定下来；再做 Phase 2，把 d-token 服务、Agent、cc-switch 的真实连接闭环跑通；接着做 Phase 3，把智能学习作为差异化功能做成可用；然后 Phase 4 补齐分析、Doctor、日志、回退；最后 Phase 5 统一做 UI 完整度、稳定性和发布验收。

## 第一版重点验收 Agent

第一版必须重点验收：Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode。

CommandCode 是一个独立 Agent 名称，不是命令行 Agent 的统称。实现前必须补齐其命令名、配置路径、hook 能力、上游配置格式、可检测信号和可验证方式。计划层已经为它预留 `AgentDescriptor` 适配位。

## 总体验收原则

每个阶段都必须同时满足三类证据：

1. UI 证据：界面能清楚展示状态、下一步操作、错误原因和成功结果。
2. 运行证据：后端真实执行、读取、写入或诊断，而不是纯 mock。
3. 安全证据：密钥、token、cookie、认证头、完整 prompt、完整 response、敏感文件不被泄露。

## 立即开工入口

实现阶段从 `implementation-kickoff.md` 开始，然后按 Phase 1 到 Phase 5 顺序推进。
