# d-token 当前实现状态

更新时间：2026-07-13
状态类型：代码事实快照，不替代 `docs/PRD.md` 与 `docs/PLAN.md`

## 1. 当前结论

d-token 当前已形成可运行的 Rust-owned 本地 runtime、Tauri 2 桌面控制面和 React 18/TypeScript UI。macOS 候选已完成安装/服务生命周期、OpenCode 配置事务、真实 routed compression、持久化 Dashboard/Event Ledger、Doctor 9/9、Smart Learning exact rollback 与真实窗口 UX 验证；macOS/Windows 的 fail-closed 签名、安装验证与 hash workflow 已配置。它仍不是可公开发布版本，正式 Apple/Windows 签名凭据执行、signed update feed、Windows 实机、其他 Agent 真实事务和认证 Provider 凭据实证尚未闭环。

## 2. 代码所有权

| 范围 | 当前定位 |
|---|---|
| `code/apps/desktop/` | 唯一正式桌面 App；Tauri 2 + React 18 + TypeScript + Vite |
| `code/crates/` | 13 个 Rust 业务 crate，负责类型、配置、路由、压缩、进程、Doctor、事件、策略、API 与 proxy |
| `code/apps/desktop/src-tauri/` | 桌面 native command、凭据存储、配置草稿/应用、服务生命周期、安装事务、Doctor 与导出 |
| `code/runtimes/headroom/` | Headroom-derived 能力迁移与 provenance 边界，不是 Python sidecar 或外部服务依赖 |
| `code/UI design for project/` | 设计原型/历史 UI 输入，不是生产代码入口 |
| `code/apps/desktop/src-old/` | 旧前端实现，不得作为新功能落点 |
| `code/packages/sdk/` | 尚未实现的 SDK 预留目录，不是当前对外能力 |

## 3. 当前 Rust workspace

`code/Cargo.toml` 当前注册 13 个业务 crate 与 1 个 Tauri app crate：

```text
dtoken-types
dtoken-config
dtoken-router
dtoken-compress
dtoken-process
dtoken-doctor
dtoken-event-ledger
dtoken-preprocessor
dtoken-headroom-adapter
dtoken-project-policy
dtoken-config-patcher
dtoken-core-api
dtoken-proxy
d-token-desktop
```

当前生产主链路：

```text
AI coding agent
→ 127.0.0.1 d-token listener
→ dtoken-core-api / dtoken-proxy
→ compression + route selection
→ configured upstream
→ redacted runtime evidence / Event Ledger / Dashboard / Doctor / App
```

## 4. 已存在的本地 API

`dtoken-core-api` 当前提供以下主要能力：

- OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 入口。
- `/health`、`/dashboard/payload`、`/stats`、`/stats-history`、`/metrics`。
- Provider、Route、Instance、active config、chain status 与近期 route decision。
- Doctor、recent logs、JSON/CSV export、Event Ledger。
- CCR、memory/learn、output shaping、项目策略与兼容性信息。
- Headroom-derived 与 integrated RTK 的 metadata-only 状态面。
- Optical shadow preview、status、capabilities、lifecycle 与 recent events。

这些端点的存在只证明实现面存在，不自动等于已达到稳定 Beta。真实 release claim 仍需同一候选 commit 的构建、运行、路由、持久化和平台证据。

## 5. 当前 Tauri 能力

生产版 invoke 当前包含：

- App 设置读写。
- Provider credential 的操作系统凭据库写入/删除，配置中只保存引用。
- Route/provider 草稿保存、应用和 active config 状态读取。
- d-token active config 的 rollback point 与恢复。
- 工具检测、安装计划预览、d-token 服务状态、启动和停止。
- 安装事务、Doctor、产品 readiness 聚合、Dashboard 读取和脱敏导出。
- Agent route 配置验证与项目压缩策略推荐。
- OpenCode 配置事务：preview、脱敏 diff、双 SHA-256、backup、明确确认、apply、verify 和 exact rollback。
- Smart Learning managed block：本地扫描、敏感路径排除、临时目录 transaction 与 exact rollback。
- 统一 `OperationDialog` 与本地持久化 `NotificationCenter`；warning/error/action-required 才占用顶栏 banner，success 留在通知中心。

OpenCode workload、模型测试、Headroom capability smoke 等开发命令只在 debug build 注册；release build 不注册这些测试入口。

当前限制：

1. `install_tool` 对缺失的 RTK/Headroom 不执行真实下载，只做检测并安全失败。
2. OpenCode 已完成真实配置事务和恢复，但该授权与证据不扩展到 Claude Code、Codex、Cursor、Cline 或 cc-switch。
3. 平台自动启动控制面已接通 macOS LaunchAgent 与 Windows HKCU Run，具备快照、原子写入/注册表写入、写后校验与失败回滚；真实登录会话和覆盖升级后的自动启动证据仍待补齐。
4. 凭据库代码路径存在，但真实认证 Provider 的 macOS Keychain/Windows Credential Manager 候选版证据仍需补齐。

## 6. 当前桌面信息架构

正式一级导航与 PRD 一致：

1. 总体概览。
2. 安装与连接工作台。
3. 分析。
4. Doctor。
5. 变更与回退。
6. 设置。

Advanced、Logs/Security、Compatibility、Smart Learning 等仍作为工作台子页面或内部组件，不应重新扩张为工程化顶级导航。Smart Learning 已接入“安装与连接”二级页，并使用真实 native scan/preview/apply/rollback action result。

正式桌面页面只接受 Tauri command 与 d-token-owned local service 的 source/freshness/action-result 证据；缺失数据保持 unavailable/partial/unknown。非 Tauri 环境仅用于开发预览并 fail-closed，不向正式页面注入示例数字。

当前 UI 基础层为 AiMaMi 信息层级取向的纯色 shell/surface：侧栏和顶部工作区连成一个整体，正式页面不使用玻璃卡片堆叠或动态背景；概览默认展示全年 heatmap，设置使用 grouped rows。Windows/macOS 共用同一套 React UI，当前只完成 macOS 真实视觉验收。

## 7. 当前 macOS 真实候选证据

2026-07-13 已在精确最新 `.app` 上完成以下验证：

- 真实链路：`OpenCode -> d-token 127.0.0.1:18001 -> OpenCode Zen`。
- 关键 request `bc88fde0-3ce9-45ad-8e7f-b733096e9f0f`：压缩前 365,650、压缩后 331,945，物理减少 33,705；处理步骤为 `repeat_line_fold` 与 `semantic_closed_tool_result`。
- `tokens_saved/claimable=0` 是有意的 fail-closed 语义：没有上游归因证据时不把物理减少宣称为可计入节省。
- 重启后 runtime 仍保留 89 条 routed request、20 条可见 request record；物理减少汇总 279,270，约 4.0%，claimable 为 0。
- OpenCode 配置已恢复到 SHA-256 `47772c85307f02e1d910dc3cc93b85165a9fc37db5911b7d4201be766f85baab`；d-token active route 已恢复到 `37149d6c69561a4562a67d7d53bf03337e37e98104424c694aaa4a4038c45843`，当前 route 为 `Local verification`。
- Doctor 在真实窗口运行全部检查为 9/9；Headroom 缺失是正常可选状态，OpenCode 和 RTK 检测通过。
- Changes 显示 0 项待处理；已应用 route draft 保留 resolution/backup history，不再重新出现为待批变更。
- Smart Learning 在临时目录完成 managed block apply 与 exact rollback，没有改写用户项目。
- macOS 六页基线截图位于 `support/artifacts/macos-ui/final-candidate-2026-07-12T18-52-02-502Z/`；当前工作树新增 Overview、Settings autostart 与 920×640 实机证据位于 `support/artifacts/macos-ui/final-candidate-2026-07-13/`。本机 ad-hoc 哈希由候选专属 manifest 记录，不写死在源码状态文档中，也不等同于正式签名发布产物。

## 8. Optical / Hybrid Context Optimization

当前后端已实现默认关闭、证据门控的完整 Hybrid 执行链：

- 版本化 Capability Snapshot 与 revoked/superseded 生命周期。
- Anthropic/OpenAI 工具调用依赖图与 fail-closed 规则。
- request-local TextArena、风险分类、geometry 与 economics 计划。
- raw-byte/canonical JSON fingerprint 计划证据。
- preview/status/capabilities/lifecycle/events API。
- route-resolved Protected Context IR 与仅批准 closed historical low-risk tool result 的 semantic/Optical codec。
- 确定性纯 Rust PNG renderer、分页、自解码、deadline/取消/并发/内存/像素/编码预算。
- Anthropic typed patcher、artifact-derived post-transform validator 和 pre-send 原文 fallback。
- 同协议 Anthropic JSON/SSE 转发；shadow/passthrough 保留 ingress 原始 bytes，跨协议不授予 Optical active。
- Ed25519 route-bound Capability/eval attestation、signed multimodal accounting、typed Ledger、metrics 与 Doctor evidence。

active 代码路径存在但生产默认仍关闭：必须同时满足全局/路由显式授权、同协议 Anthropic、精确签名 Capability、通过且绑定 renderer/font/corpus 的 eval、经济门、资源门、patch/post-validation 与 durable pre-send Ledger。当前没有真实 Provider fidelity/cost/latency 候选证据、OpenAI active patcher、history collapse 或 post-commit 自动 rollback；任何文档和 UI 都不得把本地估算或第三方数据宣称为已验证节省。

## 9. 仍然阻断发布的事项

1. macOS 发布 workflow 已强制 Developer ID、notarization、staple、Gatekeeper 与 SHA-256 manifest，但缺真实 Apple 凭据执行和 signed update feed；当前本机候选仍是 ad-hoc 签名产物。
2. Windows 发布 workflow 已强制 Tauri bundling 阶段 Authenticode、NSIS 静默安装/启动/卸载、逐文件一致性与 SHA-256 manifest，但缺具备签名凭据的 Windows 实机/runner 结果，以及 UTF-8/路径、Credential Manager、回退和视觉一致性证据。
   Linux CI、macOS release 与 Windows release 均会在 Rust workspace 门禁前从当前源码准备 host 对应的真实 `dtoken-proxy` externalBin，干净 checkout 不再依赖仓库内或开发机残留二进制。
3. macOS LaunchAgent 与 Windows HKCU Run 已接通，但尚未绑定同一候选完成两端真实登录、禁用和覆盖升级验证，不能宣称平台 autostart 已通过发布验收。
4. Claude Code、Codex、Cursor、Cline 等其他 Agent 缺 plan → diff → backup → confirm → apply → verify → rollback 与 routed request 的真实验收。
5. 真实认证 Provider 的 Keychain/Credential Manager 引用、短生命周期出站读取和不泄露凭据的候选版实证仍缺失。
6. `code/UI design for project/` 与 `src-old/` 等旧实现需要在后续清理任务中决定保留、迁移或删除；它们不是生产入口。

## 10. 当前验证入口

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

2026-07-13 最终候选门禁结果：

- `python3 support/scripts/check_structure.py`：通过。
- `npm run test:run`：125 项前端测试通过。
- `npx tsc --noEmit` 与 `npm run build`：通过。
- `npm run test:e2e`：20 项 Playwright E2E 通过。
- Rust route lifecycle、配置 patch/rollback 与 workspace gates 通过。
- `npm audit`：0 个漏洞。
- `npm run tauri:build`：macOS `.app` 与 DMG 生成成功；bundle `codesign --verify --deep --strict` 通过 ad-hoc 签名校验。该结果不等于 Developer ID/notarization。

Tauri 构建必须通过 `npm run tauri:dev` 或 `npm run tauri:build`，以便先准备对应平台的 `dtoken-proxy` external binary。
