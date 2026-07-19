# CLAUDE.md

更新时间：2026-07-11 晚间

本文件是 Claude Code 在 d-token 仓库中的入口。通用规则以 `AGENTS.md` 为准；本文件保留 Claude Code 开工时最需要的上下文和检查顺序。

一句话项目介绍：d-token 是面向 AI Coding 的本地优先桌面控制面与 Rust runtime，在 Windows/macOS 上统一处理本地路由、上下文优化、Provider/Route、Doctor、Dashboard、Event Ledger、凭据引用、配置变更和回退。

## 1. 真理源

按顺序读取：

1. `docs/PRD.md` — 唯一产品契约。
2. `docs/PLAN.md` — 当前唯一执行计划。
3. `docs/product/IMPLEMENTATION_STATUS.md` — 按实际源码整理的能力和缺口。
4. `docs/ARCHITECTURE.md`、`docs/TECH_STACK.md`、`docs/SECURITY_BOUNDARIES.md`。
5. `docs/index.md` — 当前文档和归档入口。
6. `AGENTS.md` — 完整工作、安全、OpenCode 和 Git 审计规则。

`docs/history/` 与 `docs/archive/` 只用于追溯，不能覆盖当前源码、PRD、PLAN 和实现状态。

## 2. 当前代码入口

```text
code/apps/desktop/                 唯一正式 Tauri 2 + React 18 桌面 App
code/apps/desktop/src/app/         当前 UI
code/apps/desktop/src-tauri/       native commands、凭据、进程、配置、打包
code/crates/                       13 个 Rust 业务 crate
code/runtimes/headroom/            Headroom-derived provenance/迁移边界
support/                           测试、脚本、fixture、CI 和审计账本
```

不要把以下路径当成生产入口：

- `code/UI design for project/`：设计原型。
- `code/apps/desktop/src-old/`：旧实现。
- `code/packages/sdk/`：尚未实现的预留目录。

## 3. 当前产品链路

```text
AI coding agent
→ 127.0.0.1 d-token listener
→ d-token-owned Rust runtime
→ compression / context optimization / explicit route decision
→ configured upstream
→ redacted Event Ledger / metrics / Dashboard / Doctor / App
```

禁止依赖外部 Headroom 8787 作为成品 Dashboard 数据源。`code/runtimes/headroom/` 不是 Python sidecar。

## 4. 当前实现边界

已存在：

- OpenAI Chat/Responses、Anthropic Messages 本地入口。
- Provider/Route/Instance、active config、Dashboard、Doctor、Event Ledger、logs、export。
- d-token route 草稿、应用、rollback point 和 active config 恢复。
- OS credential store 写入/删除，配置只保存引用。
- App-managed d-token Rust runtime 的状态、启动、停止和身份检查。
- Optical shadow preview、Capability lifecycle 和 metadata-only events。

不得过度宣称：

- 缺失的 RTK/Headroom 当前不会自动下载，只会检测并安全失败。
- d-token route 草稿不会自动修改所有第三方 Agent 配置。
- 外部 Agent 配置的完整 transaction/rollback 尚未闭环。
- Optical 没有 renderer、patcher、active mutation、post-validator 或 rollback executor。
- macOS/Windows 公开发布证据尚未完成。

## 5. UI 规则

正式一级页面固定为：总体概览、安装与连接、分析、视觉压缩（Beta）、Doctor、变更与回退、设置。

`Advanced`、`LogsSecurity`、`Compatibility`、`SmartLearning` 只能作为工作台子流程或内部组件。除「视觉压缩（Beta）」外不得再新增工程化顶级导航。正式 UI 不得出现 workload benchmark、smoke test、fixture、mock、fake-bin、testing center 等开发术语。

真实状态必须带 source、freshness 和 action result。浏览器 preview、缺失数据和部分数据不能渲染成机器真实状态。

## 6. 安全规则

- 默认只绑定 loopback。
- 禁止记录或提交密钥、token、cookie、Authorization、auth 文件、完整 prompt/response 和原始模型输出。
- 凭据不得明文 fallback。
- 真实写入必须具备摘要、diff、backup、confirm、verify、rollback。
- 停止进程前验证 PID、可执行文件、端口所有权和 d-token identity。
- Provider/Route 禁止静默猜测和静默 fallback。
- Optical active transform 必须保持关闭。

## 7. OpenCode 验收

用户只授权 OpenCode 在当前阶段使用正常本机配置进行真实 routed test。默认不隔离 HOME、不覆盖正常配置、不读取 auth 文件。如需加入临时 d-token provider，必须权限受限备份、最小变更、精确恢复和脱敏报告。该授权不得扩展到 Claude Code、Codex、Cursor、Cline 或系统代理配置。

## 8. 文档和临时文件

- 产品边界改 `docs/PRD.md`。
- 当前执行改 `docs/PLAN.md`。
- 代码事实改 `docs/product/IMPLEMENTATION_STATUS.md` 和对应 README。
- 阶段报告进 `docs/history/`，被替代内容进 `docs/archive/`。
- 不在根目录创建或保留 `task_plan.md`、`findings.md`、`progress.md`；Agent 协作使用 `.ai-bridge/`。

## 9. Git 操作

commit、branch create/switch/delete/rename、merge、rebase、push 前，必须先用：

```bash
python3 support/scripts/git_operation_audit.py record ...
```

写入 `support/git-operations.json`。禁止删除或改写已有记录。

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

Tauri 使用 `npm run tauri:dev` 或 `npm run tauri:build`，以保证对应平台的 `dtoken-proxy` external binary 先被准备。

## 11. AI Coding 行为准则

本仓库使用主动分析、克制执行的 Karpathy Guidelines，完整规则见 `AGENTS.md` 和 `.agents/skills/karpathy-guidelines/SKILL.md`。

- 编码前先确认目标、证据、约束、关键假设和可验证的成功标准。
- 适度检查直接关联的调用链、数据/状态流、错误路径、边界条件、配置和测试；不因发散而扩大范围。
- 优先修复根因，采用最小完整方案，复用现有模式，不添加推测性抽象或配置。
- 只做能追溯到用户目标、根因修复或回归防护的精准修改；不顺手重构或删除无关代码。
- 对方案保持批判性，主动说明证据、风险、取舍和更简单的替代方案。
- 按“实现 → 验证 → 修正”闭环执行；未实际运行的检查不得声称通过。
- 汇报时区分已修复、建议后续处理和不在范围的问题，并明确验证结果与限制。
