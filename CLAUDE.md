# CLAUDE.md

一句话项目介绍：d-token 是一个 Headroom-compatible 的本地 AI Coding 压缩路由桌面系统，目标是在 Windows/macOS 上完整保留并迁移 Headroom 能力，同时新增 provider、route、instance、Event Ledger、Doctor、Dashboard 等可观测管理能力。

本文件给 Claude Code 使用。产品真理源始终以 `docs/PRD.md` 为准。

## 当前最高优先级产品链路

必须按以下链路理解和实现 d-token：

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

## 不可违反的边界

1. `D:\D-TOKEN\headroom` 是源码参考、能力迁移来源和兼容行为参考，不是 d-token App 的运行时数据源。
2. 外部 Headroom 服务，例如 `http://127.0.0.1:8787/dashboard`，只能作为迁移对照和 no-touch 只读参考；不能作为成品 App Dashboard 的依赖。
3. d-token 必须拥有自己的本地服务、端口、stats/history/metrics、Event Ledger、ProviderProfile、RouteProfile、InstanceProfile 和 Dashboard 数据模型。
4. Headroom Dashboard 的页面与数据能力要迁移进 d-token App，但数据必须来自 d-token-owned runtime。
5. OpenCode 是第一条真实测试入口；真实测试直接使用用户本机原本 OpenCode 配置中的 provider/model，不强制使用官方免费模型，也不做隔离配置。
6. 正确测试链路是 `OpenCode -> d-token local service -> d-token Headroom-derived runtime -> d-token route adapter -> official/custom/cc-switch upstream`。
7. 禁止把实现写成 `App -> external Headroom 8787 -> Dashboard data`。
8. 开发期不得启动、停止、重启、改配置、占用或依赖外部 Headroom / RTK / cc-switch 服务。
9. OpenCode 真实测试已获用户授权按原本 OpenCode 配置执行；默认不隔离 HOME、不替换 fake config、不另造临时 OpenCode 配置。执行 OpenCode real routed / streaming / workload test 时，默认直接调用本机 `opencode` 和原配置，目标是证明真实 OpenCode 流量经过 d-token。
10. OpenCode 是本阶段唯一允许使用真实用户配置进行测试的 agent；不得因此扩展到 Claude Code、Codex、Cursor、Cline 或系统代理配置。
11. 若测试需要修改 OpenCode 配置以加入 d-token provider/route，必须先备份、只写最小变更、记录到测试报告；不得读取、打印、持久化或提交 OpenCode 密钥、token、cookie、auth 文件、完整 prompt/response。
12. 正式 App UI 不得产品化测试概念：workload benchmark、smoke test、fixture/mock/fake-bin、testing center 只能存在于 `support/`、CI、隐藏 dev mode 或报告中。
13. OpenCode 只是第一条真实验收入口，不是最终产品信息架构中心；UI 必须保持多 agent 控制面结构。
14. UI 动作必须先翻译成用户任务：安装、连接、配置、验证配置、诊断、查看状态、查看日志、导出。不得把开发脚本按钮原样搬进用户界面。
15. 浏览器/桌面窗口验收只证明显示和交互；真实功能接入必须由 d-token-owned service/API/Tauri command、Event Ledger、metrics/export 和 routed agent flow 证明。
16. 新增功能前必须先对照 `docs/PRD.md` 和成熟桌面控制面产品逻辑判断产品路径，再写代码。

## 必读顺序

1. `docs/PRD.md` — 唯一 PRD，也是产品真理源。
2. `docs/TECH_STACK.md` — 桌面 App 与 runtime 技术选型。
3. `docs/ARCHITECTURE.md` — 总体架构。
4. `docs/SECURITY_BOUNDARIES.md` — 本地优先安全边界。
5. `docs/history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md` 与 `docs/history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md` — 当前工作树最新前后端审查；`docs/history/FINAL_REVIEW.md` 仅是历史快照。

## 工作原则

- 修改前先读 `docs/PRD.md`。
- 产品变更直接修改 `docs/PRD.md`，不新增多版本 PRD。
- 正式代码放 `code/`，正式文档放 `docs/`，测试、脚本、CI、fixture 和 fake-bin 放 `support/`。
- 不直接复制外部项目代码；如复用开源代码，必须遵守许可证、NOTICE 和变更记录要求。
- 所有真实密钥、token、cookie、auth 文件都不得提交。
- 所有本地服务默认绑定 `127.0.0.1`。
- Windows/macOS 兼容优先：UTF-8、路径、端口、`.cmd` shim、WebView2、Xcode Command Line Tools。
- Dashboard 不做简化版，Headroom 功能迁移不缩水。
- 正式 UI 只展示产品状态和用户操作，不展示开发测试语义。
- 当前接受的技术栈是 Rust 核心业务 + Tauri 2 + React/TypeScript；macOS 仅在 Rust/Tauri 无法可靠覆盖时使用极少量 Swift/系统桥接，不使用 Python sidecar 作为当前运行架构。
- Windows 与 macOS 共享同一套 React UI、信息架构与功能语义；平台差异限制在系统适配和打包层，当前先完成 macOS 窗口与真实功能验收。
- Git commit、branch create/switch/delete/rename、merge、rebase、push 前，必须先用 `support/scripts/git_operation_audit.py record` 写入 `support/git-operations.json`，记录时间、分支与一句话简介；禁止删除或改写既有记录。

## 提交前检查

```bash
python support/scripts/check_structure.py
```
