# AGENTS.md

更新时间：2026-07-11 晚间

本文件是 Codex、Claude Code、Cline、OpenCode 等 AI coding agent 的仓库入口。

一句话项目介绍：d-token 是面向 AI Coding 的本地优先桌面控制面与 Rust runtime，在 Windows/macOS 上统一处理本地路由、上下文优化、Provider/Route、Doctor、Dashboard、Event Ledger、凭据引用、配置变更和回退。

产品真理源始终是 `docs/PRD.md`。当前执行顺序看 `docs/PLAN.md`，当前代码能力看 `docs/product/IMPLEMENTATION_STATUS.md`。历史和归档文件不得覆盖这三者。

## 1. 开始工作前必读

1. `docs/PRD.md` — 唯一产品契约。
2. `docs/PLAN.md` — 唯一 Living Plan。
3. `docs/product/IMPLEMENTATION_STATUS.md` — 按实际代码整理的能力和缺口。
4. `docs/ARCHITECTURE.md` — 当前 Rust-owned runtime 架构。
5. `docs/TECH_STACK.md` — Windows/macOS 技术栈与边界。
6. `docs/SECURITY_BOUNDARIES.md` — 凭据、日志、写入、进程和 loopback 边界。
7. `docs/index.md` — 当前文档入口与归档说明。

涉及桌面 UI 时再读：

- `docs/APP_FUNCTIONS_AND_PAGES.md`
- `docs/PRD.md` 第 12.5 节（桌面视觉、布局与交互产品契约）

涉及 Optical 时再读：

- `docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`
- `docs/optical/README.md`
- `docs/optical/implementation-status.md`
- `docs/optical/release-gates.md`

## 2. 当前产品链路

```text
OpenCode / Claude Code / Codex / Cursor / Cline / other AI coding agent
→ d-token loopback listener
→ d-token-owned Rust runtime
→ compression / context optimization / route decision
→ configured official / custom / cc-switch-compatible / local upstream
→ redacted Event Ledger / metrics / Dashboard / Doctor / App
```

`code/runtimes/headroom/` 是 Headroom-derived 能力的 provenance 与迁移边界，不是 Python sidecar，也不是外部 Headroom 服务依赖。

## 3. 代码所有权

```text
code/apps/desktop/                 唯一正式桌面 App
code/apps/desktop/src/app/         当前 React 产品 UI
code/apps/desktop/src-tauri/       Tauri native commands、平台适配与打包
code/crates/                       13 个 Rust 业务 crate
code/runtimes/headroom/            Headroom-derived provenance/迁移边界
support/                           测试、脚本、fixture、fake-bin、CI 与审计账本
docs/                              正式文档、历史和归档
```

以下路径不是生产功能落点：

- `code/UI design for project/`：设计原型/历史 UI 输入。
- `code/apps/desktop/src-old/`：旧前端实现。
- `code/packages/sdk/`：尚未实现的预留目录。
- `docs/history/`、`docs/archive/`：追溯材料，不是当前实现规范。

## 4. 不可违反的产品与安全边界

1. Dashboard、Doctor、Analytics、Logs 和 Export 的真实数据必须来自 d-token-owned service、Tauri command 或 Event Ledger。
2. 禁止实现成 `App -> external Headroom 8787 -> Dashboard data`。
3. 外部 Headroom、standalone RTK、cc-switch 只能在明确的 no-touch/只读边界内检测或对照，不能成为隐式运行依赖。
4. 所有本地服务默认只绑定 `127.0.0.1`。
5. Provider/Route 选择必须显式、可解释；禁止静默猜测和静默 fallback。
6. API key、token、cookie、Authorization、auth 文件、完整 prompt/response、源代码和大段原始命令输出不得进入普通日志、配置、导出或提交。
7. 凭据只进入操作系统凭据库；d-token 配置只保存不透明引用，禁止明文 fallback。
8. 所有真实写操作必须具备用户可读摘要、diff、backup、明确确认、verify 和 rollback evidence。
9. 进程停止前必须验证 PID、可执行文件、端口所有权和 d-token identity，防止误杀或 PID reuse。
10. 浏览器 preview 只能证明 UI；真实功能必须由 runtime、route decision、Event Ledger、export 或 routed Agent flow 证明。
11. Optical 当前仅允许 shadow preview 和 metadata-only observability；没有 renderer、patcher、post-validator、rollback 前禁止 active transform。
12. Windows/macOS 共享同一 React UI、信息架构和产品语义；平台差异只放在凭据、进程、系统集成和打包适配层。

## 5. OpenCode 真实验收边界

OpenCode 是当前第一条真实 Agent 验收入口，不是产品信息架构中心。

1. 用户已授权在真实验收时使用本机正常 OpenCode 配置和现有 provider/model。
2. 默认不隔离 HOME、不替换 fake config、不使用 `OPENCODE_CONFIG_CONTENT` 覆盖正常配置。
3. 如需临时加入 d-token provider/route，必须先创建权限受限的持久备份，只写最小变更，验证后按精确内容恢复。
4. 不读取或打印 `auth.json`、密钥、token、cookie、完整 prompt/response 或原始模型输出。
5. 必须证明 `OpenCode -> d-token listener -> configured upstream`，并关联 runtime identity、route/provider、Event Ledger 和脱敏报告。
6. 该授权不得扩展到 Claude Code、Codex、Cursor、Cline 或系统代理的真实配置写入。

## 6. UI 与产品语言

正式一级页面固定为：总体概览、安装与连接、分析、Doctor、变更与回退、设置。

- `Advanced`、`LogsSecurity`、`Compatibility`、`SmartLearning` 是工作台子页面或内部组件，不得重新扩张为工程化顶级导航。
- workload benchmark、smoke test、fixture、mock、fake-bin、testing center 只能存在于 `support/`、CI、debug build 或内部报告。
- UI 动作必须翻译成用户任务：安装、连接、配置、预览变更、应用、验证、诊断、修复、查看状态、查看日志、导出、回退。
- 无完整 source/freshness/action-result 证据时，必须显示 unavailable、partial、preview 或 unknown，禁止用示例数字和绿色状态补齐。

## 7. 文档规则

1. 不创建 PRD v2/v3；产品边界直接修改 `docs/PRD.md`。
2. 当前执行事项只写入 `docs/PLAN.md`。
3. 代码事实变化同步更新 `docs/product/IMPLEMENTATION_STATUS.md` 和对应 README。
4. 阶段交付报告进入 `docs/history/`；被替代的计划、研究、审查和 Agent 过程文件进入 `docs/archive/`。
5. 根目录不得长期保留 `task_plan.md`、`findings.md`、`progress.md` 等会话文件；使用 `.ai-bridge/` 或任务结束后归档。
6. 历史/归档正文中的旧路径和旧结论只代表当时语境。
7. 新增、移动、删除文档后更新 `docs/index.md` 和 `support/scripts/check_structure.py`。

## 8. 工作方式

1. 修改前先检查当前 Git 状态和已有未提交改动，不覆盖用户或其他 Agent 的工作。
2. 优先小范围、可验证、可回滚的改动；不要顺手重构无关文件。
3. 不直接复制外部项目代码；复用时必须记录许可证、NOTICE、来源 commit 和修改边界。
4. Rust 业务逻辑放在 crate 中，Tauri command 负责桌面桥接，不把核心业务堆进 React 或平台脚本。
5. Release build 不得注册 benchmark/smoke/test-only commands；开发验证命令必须受 debug gate 限制。
6. 不把“代码路径存在”写成“功能已完成”或“可发布”；发布结论必须绑定同一候选 commit 和实际验证证据。

## 9. Git 操作审计

所有 commit、branch create/switch/delete/rename、merge、rebase 和 push 操作，执行前必须使用：

```bash
python3 support/scripts/git_operation_audit.py record ...
```

写入 `support/git-operations.json`。每条记录必须包含时间、分支和一句话说明。commit 记录使用操作前 HEAD，避免提交自引用；禁止删除或改写既有记录。

## 10. 基础验证

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

Tauri 运行和构建使用 `npm run tauri:dev` / `npm run tauri:build`，不要绕过 sidecar 准备脚本直接制造不完整 bundle。
