# d-token 执行计划（Living Plan）

状态：Living Document
维护规则：本文件是 d-token 的唯一执行计划，按代码现状滚动维护。产品边界改 PRD，阶段任务改本文件；当前代码事实同步维护在 `docs/product/IMPLEMENTATION_STATUS.md`。PRD（`docs/PRD.md`）是产品真理源，本文件不得与其验收标准冲突。

## 0. 当前坐标

| 维度 | 现状 |
|------|------|
| PRD | 单一 living PRD，§18 已去计划化，指向本文件 |
| 文档 | 当前导航见 `docs/index.md`，代码事实见 `docs/product/IMPLEMENTATION_STATUS.md`；旧计划/矩阵/研究已归档 |
| 代码 | 13 个 Rust 业务 crate + 1 个 Tauri app crate + React 18/TypeScript/Vite 桌面 UI |
| 已交付里程碑 | M1（no-touch 基础）、M2（Headroom 只读集成）、M3（GUI MVP） |
| 交付证据 | `docs/history/M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md`、`docs/history/M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md`、`docs/history/M3_GUI_CONTROL_PLANE_MVP_REPORT.md`、`docs/history/M1_M3_DELIVERY_AUDIT.md` |
| 当前 Node | M4 macOS 主链路已完成真实候选验证：安装/路由事务、持久化数据、OpenCode routed compression、Doctor 与桌面 UX 已闭环；Windows、正式签名发布和其他 Agent 仍未验收 |
| 历史路线图 | `docs/archive/ROADMAP_M0_M6_HISTORICAL.md`（M0–M6 原文快照） |

当前 M4 历史设计输入：

- `docs/history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md` — 当时的真实能力接入计划，仅作为历史设计和既有测试证据；当前执行状态与剩余工作以本文件、PRD、`docs/product/IMPLEMENTATION_STATUS.md` 和当前源码/测试为准。

代码现状盘点（2026-07-13）：

```text
code/crates/
  dtoken-types, dtoken-config, dtoken-router, dtoken-compress,
  dtoken-process, dtoken-doctor, dtoken-event-ledger,
  dtoken-preprocessor, dtoken-headroom-adapter, dtoken-project-policy,
  dtoken-config-patcher, dtoken-core-api, dtoken-proxy
code/apps/desktop/   Tauri 2 + React 18 + Vite，共享 Windows/macOS UI 与 native commands
code/runtimes/headroom/   Headroom-derived runtime provenance/兼容边界（非 Python sidecar）
support/fixtures/ support/fake-bin/   no-touch 测试资产
.github/workflows/ci.yml   structure / cargo test / no-touch smoke / desktop build
.github/workflows/macos-release.yml   macOS app/DMG、强制签名/notarization/Gatekeeper/hash workflow
.github/workflows/windows-release.yml Windows NSIS、强制 Authenticode、静默安装/启动/hash workflow
```

当前已交付基础：

1. d-token-owned Rust local service 已提供 OpenAI Chat/Responses、Anthropic Messages、Dashboard、stats/history/metrics、Provider/Route/Instance、Doctor、Event/Logs/Export 与 Optical shadow APIs。
2. Dashboard、Doctor、Analytics、Logs、Changes 和 Export 已统一从 Tauri command 与 d-token-owned local service 读取真实证据；桌面页面不再用浏览器 fixture 或 mock 数字补齐状态。
3. 当前 macOS 候选已完成真实 `OpenCode -> d-token 127.0.0.1:18001 -> OpenCode Zen` 验证：单请求物理减少 33,705 Token，`claimable=0`；持久化 runtime 汇总为 89 条 routed request、20 条可见 request record、物理减少 279,270 Token（约 4.0%）。
4. Tauri bundling 已启用：Windows NSIS，macOS `.app`/DMG；两个公开发布 workflow 都已改为 fail-closed，缺少签名凭据、签名/公证校验、安装启动证据或 SHA-256 manifest 时不会上传发布候选。
5. OS credential store 写入、引用和出站短生命周期读取已有 Rust 实现；仍需认证 Provider 的候选版实机证据。
6. 桌面统一采用 AiMaMi 信息层级取向的纯色 shell/surface、默认全年 heatmap、分组设置、统一 `OperationDialog` 与持久化 `NotificationCenter`；成功通知不占用顶栏 banner。
7. OpenCode 配置与 d-token active route 均已通过最小 diff、双 hash、备份、验证和精确恢复闭环；恢复后 SHA-256 分别为 `47772c85307f02e1d910dc3cc93b85165a9fc37db5911b7d4201be766f85baab` 与 `37149d6c69561a4562a67d7d53bf03337e37e98104424c694aaa4a4038c45843`。Smart Learning 也已在临时目录证明 managed block 精确回退。

当前发布阻断：

1. macOS 正式签名/notarization/Gatekeeper workflow 已 fail-closed 配置，但仍缺 Apple Developer ID 凭据和同一候选提交的真实执行证据；当前本机 `.app` 是 ad-hoc 签名候选。
2. Windows 正式 NSIS/AuthentiCode/静默安装启动/hash workflow 已配置，但仍需具备签名凭据的 Windows runner 执行，并补 UTF-8/路径、Credential Manager、回退和视觉一致性实机证据。
3. 平台自动启动控制面已接通 macOS LaunchAgent 与 Windows HKCU Run，并具备写入校验与失败回滚；仍需同一候选在真实登录会话完成 macOS/Windows 启动、状态恢复和关闭验证。
4. Claude Code、Codex、Cursor、Cline 等其他 Agent 尚未完成与 OpenCode 同等级的真实配置事务和 routed request 验收。
5. 真实认证 Provider 的 macOS Keychain/Windows Credential Manager 出站读取仍缺不暴露凭据的候选版实证。

## 1. 计划原则

1. **PRD 主产品契约，PLAN 主执行节奏**：边界/验收改 PRD，任务拆解/排期改 PLAN。
2. **事务优先**：任何真实写操作必须先有 fixture/fake-binary 故障验证、diff、backup、verify、rollback；真实路径仅在用户明确操作和受控边界内开启。
3. **每个任务带 DoD**：Definition of Done = 可验证项 + 涉及文件 + 测试 + 回滚证据。
4. **不破坏已交付里程碑**：M1–M3 验收项保持绿，新工作不得回退 no-touch 安全边界。
5. **Windows/macOS 一等公民**：任何 install/patch/rollback 任务必须显式覆盖 UTF-8、盘符路径、项目名连字符、`.cmd` shim、xattr/quarantine。
6. **无静默 fallback**：route/provider/安装失败必须可见、可解释、可审计。

## 2. M4：Production Managed Installer（实施中）

目标：成品支持一键安装、图形化配置、可回滚。用户只需下载 d-token App，不用先装 Headroom/RTK。

M4 的实际落地先走 `OpenCode Real Route Track`：使用用户原本 OpenCode 配置，不隔离 HOME、不覆盖正常认证配置；临时写入前创建权限受限备份，只写最小 provider/model/baseURL 变更，再由 d-token 完成压缩、路由、Event Ledger 和 upstream 转发。当前 macOS 候选已完成真实请求与 UI 精确回退，配置 hash 已恢复到操作前值。历史设计见 `docs/history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md`。

### M4.S1 安装事务引擎（InstallationTransaction）

落位：`code/crates/dtoken-config-patcher`（扩展）或新建 `dtoken-install-transaction`。

- [x] 实现 `InstallationTransaction` 状态机：scan → plan → show diff → backup → confirm → apply → verify → doctor → commit rollback point。
- [x] 失败语义：只停 d-token 自有新进程；kill 前校验进程身份；恢复 backup；清理 partial；输出精确失败与恢复动作。
- [x] 事务状态、动作结果和回滚证据进入桌面运行证据与 Event Ledger。
- DoD：fixture 下跑通 apply→fail→rollback 全链路；真实写路径只有在同一事务门、明确确认和可验证恢复证据下开启。当前 OpenCode/macOS 路径已满足，其他 Agent/平台仍关闭。
- 涉及：`dtoken-config-patcher`、`dtoken-event-ledger`、`dtoken-process`。

### M4.S2 Managed d-token runtime 安装/启用/升级/停止/卸载

- [x] runtime 打包策略：正式产物是本仓库 Rust workspace 的 `dtoken-proxy` external binary；`code/runtimes/headroom/` 仅保留 Headroom-derived provenance/迁移边界，不作为 Python 或外部服务运行目录。
- [ ] install/update/uninstall 全部走 InstallationTransaction，并保留 binary hash、version、target triple 与候选 commit provenance。
- [ ] integrated RTK 只能通过 d-token runtime 内部能力启用，不暴露 standalone RTK 为产品主路径。
- DoD：fake-binary 模式下完成 install/update/uninstall 全流程；真实 bundle 能证明 binary identity、版本与候选来源。
- 涉及：`dtoken-process`、`dtoken-config-patcher`、`dtoken-proxy`、Tauri sidecar 准备脚本、`support/fake-bin/`。

### M4.S3 Provider / Route / cc-switch 安装与配置

- [x] OpenCode 已验收的 ProviderProfile / RouteProfile patch 走 transaction；ChainBridge 与其他 Agent 矩阵仍留待后续。
- [x] route decision 进 Event Ledger（input/output protocol、target provider、route identity）。
- [x] 401/403/404/408/429/5xx 已稳定归类为 provider auth、route not found、rate/timeout 与 server error；原状态码进入响应/Ledger，且禁止静默 fallback 到其他 Provider。
- [x] OpenCode 真实 route 使用用户原本 OpenCode 配置；临时 provider/model/baseURL 变更已完成 backup、双 hash、verify、真实 request 与精确恢复。
- DoD：fixture 下覆盖 official / custom_openai / custom_anthropic / cc_switch / chain_bridge 五条路径，Event Ledger 可重建请求走向。
- 涉及：`dtoken-router`、`dtoken-config-patcher`、`dtoken-proxy`、`dtoken-event-ledger`。

### M4.S4 Agent wrap / Shell profile / 配置 patch

- [ ] agent wrap setup（Claude Code / Codex / Cursor / Cline / OpenCode 等矩阵，PRD §8.1）。
- [ ] shell profile / agent config patch：bash / zsh / powershell / cmd / wsl-bash。
- [x] Smart Learning managed instruction block 已在临时目录完成 diff、backup、并发保护与 exact rollback；真实用户项目默认仍需明确确认。
- DoD：每个 agent 在 fake-binary + 临时 HOME/PATH 下完成 patch 与回滚；CompatibilityMatrix 状态可写回。
- 涉及：`dtoken-config-patcher`、`dtoken-process`、前端 agent 矩阵页。

### M4.S5 Managed 真实路径 + Install Doctor

- [x] OpenCode 与 d-token active route 已在 InstallationTransaction 之上完成真实写入、backup、verify 和 rollback；其他 Agent/平台仍未开放为已验收路径。
- [ ] Install Doctor：runtime / proxy / 端口占用 / 进程身份 / integrated RTK / 外部 standalone RTK 冲突 / provider route / cc-switch route / config backup / Windows UTF-8/shim / macOS xattr/quarantine/keychain/launch agent。
- [x] rollback point 已落盘并可在 GUI 调用；坏备份 fail-closed，missing-file 与并发修改语义已有回归覆盖。
- DoD：在隔离 VM/临时机上完成覆盖安装、回滚、再升级；Install Doctor 全绿。
- 涉及：`dtoken-doctor`、`dtoken-config-patcher`、桌面 Doctor 页。

### M4.S6 前端接通本地 API

- [x] Tauri Dashboard 主路径从 `dtoken-core-api` 读取 d-token-owned runtime；非 Tauri 环境 fail-closed，不向正式页面注入示例数据。
- [x] InstallationTransaction / route decision / Doctor 状态、source/freshness/action result 在 GUI 实时可见。
- [x] Optical shadow 默认关闭 active transform，仅提供 preview/metadata observability。
- DoD：Tauri 窗口不以 fixture/硬编码冒充本机事实，managed 链路的 source/freshness/action result 完整可见。
- 涉及：`code/apps/desktop/src/*`、`dtoken-core-api`。

### M4 验收（对齐 PRD §18 M4 与 §19）

- [x] macOS 候选可安装/启动 d-token-owned runtime；OpenCode 与 active route 写操作可追踪、可备份、可验证、可回滚。
- [x] 用户不需要独立安装 Headroom；Headroom 缺失不阻断 Doctor readiness。
- [x] 真实 OpenCode 请求没有静默 fallback，route/provider 与脱敏 Ledger 可审计。
- [x] macOS 真实窗口 Doctor 为 9/9；Dashboard 重启后保留 89/20 请求证据。
- [ ] Windows UTF-8/path/shim 检查进入 Install Doctor 且完成 NSIS 实机 gate。

## 3. M5：Production Hardening + Release

目标：可以放心让外部用户下载、覆盖安装、回滚、升级、排障。

### M5.S1 打包与签名（构建配置已启用，发布证据未闭环）

- [x] Windows NSIS 与 macOS `.app`/DMG 的 Tauri bundle 配置。
- [x] Windows 发布 workflow：Windows runner、Tauri bundling 阶段签名、AuthentiCode 验证、当前候选静默安装/启动/卸载、SHA-256 manifest 和 artifact upload；缺证书时 fail-closed。
- [x] 平台自动启动控制面：macOS LaunchAgent 与 Windows HKCU Run 均使用精确启动参数、原值快照、写入后校验和失败回滚；设置页读取 native 状态并使用统一确认弹窗。
- [x] 干净 checkout 在 workspace Clippy/test 前按 Linux/macOS/Windows host 构建并准备真实 `dtoken-proxy` externalBin，避免本地残留生成物掩盖 Tauri build-script 失败。
- [ ] 同一候选在 macOS 与 Windows 真实登录会话验证自动启动、重复启用、禁用、损坏配置和 App 升级路径。
- [x] macOS 发布 workflow：Developer ID 导入、严格 codesign、notarization/staple、Gatekeeper、SHA-256 manifest 和 artifact upload；缺证书/公证凭据时 fail-closed。
- [ ] Windows 安装后 UTF-8、路径、credential、`.cmd` shim 与回退实机验收。
- [x] macOS 本机 `.app`/DMG、安装后 runtime、真实窗口与 artifact hash 候选证据。
- [ ] Apple Developer ID、notarization、xattr/quarantine 分发验收。
- [ ] signed updater feed、通道与版本 provenance。
- DoD：干净机器上下载即装、覆盖安装、回滚、升级四场景通过。

### M5.S2 兼容性与 E2E

- [ ] Integrated RTK 兼容矩阵（PRD §16）。
- [ ] Provider / cc-switch E2E 兼容套件（official / custom_openai / custom_anthropic / cc_switch / chain_bridge）。
- [ ] runtime install/uninstall rollback 压力测试。
- DoD：兼容矩阵无 untested 之外的状态在受控平台收敛；E2E 套件 CI 可跑。

### M5.S3 隐私与合规审计

- [ ] privacy / export / redaction audit（PRD §15）。
- [ ] upstream license / NOTICE / base commit / 变更说明审计。
- [ ] 不上传 tracking DB / tee / prompt / 代码 / 命令输出；日志默认脱敏二次确认。
- DoD：审计报告归档 `docs/`；导出样本经脱敏校验。

### M5.S4 稳定性与文档

- [ ] crash recovery、benchmark/evals。
- [ ] user guide / developer guide / troubleshooting guide。
- DoD：文档覆盖安装、配置、回滚、排障四大路径，可被 CI 引用结构检查。

### M5 验收

- 对齐 PRD §19 全部 21 条验收中 M5 范围项。
- CI gate 含 structure check、cargo check、cargo test、no-touch smoke、desktop build；平台 release workflow 另执行签名、安装启动和产物 provenance 门禁。

## 4. M6：d-token-native Runtime Innovation

目标：用户体验不变前提下，逐步把 Headroom-derived runtime 关键模块替换为 d-token-native。

- [ ] 替换脆弱 CLI/config/dashboard glue → Rust-native。
- [ ] dtoken-router 升级为 native route adapter（保留 Headroom-compatible API）。
- [ ] dtoken-event-ledger 升级 native observability（统一 denominator，落实 PRD §10 分层 savings）。
- [ ] dtoken-proxy / 协议适配层 native 化（OpenAI/Anthropic）。
- [ ] dtoken-project-policy 压缩策略引擎 native 化。
- [ ] 保留必要 Headroom-compatible 迁移层，可逐模块灰度替换并回退。
- DoD：每个模块替换前有对比基准 + 回退路径；用户仍只下载 d-token App。

## 4A. Experimental Track：Hybrid Context Optimization / Optical

定位：Optical 是混合上下文优化引擎中的一种有损 codec。后端 active 执行链已实现但默认关闭；只有精确签名 Capability、通过的绑定 eval、同协议 Anthropic 路由和全部安全/资源门同时满足时才可达，当前尚无仓库内置可信模型或默认 active。

截至 2026-07-13 已完成：

- [x] 安全默认关闭的全局/路由策略与版本化 Capability Snapshot。
- [x] Capability 生命周期：schema v1、active/revoked/superseded、撤销 revision 屏障、重复 snapshot ID 阻断。
- [x] Request-local TextArena、真实文本风险分类、Capability-driven geometry、selected-only economics。
- [x] Anthropic 工具依赖图、并行 tool use/result、orphan/duplicate/recent-tail fail-closed。
- [x] OpenAI Chat `tool_calls` / `tool` 的独立 inventory 与状态机 lowering；尚未接入 active 决策。
- [x] Raw request bytes SHA-256 与 Canonical JSON SHA-256 v1 计划证据。
- [x] Preview、Status、Capabilities、Capability Lifecycle、Recent Events API。
- [x] metadata-only 事件：状态、模式、候选计数、图计数、经济摘要和 Capability 规则计数。
- [x] `support/pending-commits.json` schema v2：阶段、时间、文件、验证、共享文件暂存方式、是否提交、提交哈希。
- [x] Protected Context IR 与安全 semantic codec；协议结构、近期文本、exact/secret、schema、tool id/arguments 始终保留。
- [x] 确定性纯 Rust PNG renderer、自解码、分页以及 deadline/取消/并发/像素/PNG/编码/内存/body 预算。
- [x] Anthropic sealed patcher、artifact-derived post-transform validator 和 send-commit fallback 状态机。
- [x] 同协议 Anthropic JSON/SSE 转发、shadow/passthrough 原始 bytes 保持和跨协议 Optical active 阻断。
- [x] Ed25519 route-bound Capability/eval attestation、signed multimodal accounting、typed Ledger、metrics 与 Doctor/status evidence。

下一实施顺序：

1. 取得安全的 OS credential reference 或明确 auth-free upstream，运行真实 OpenCode → d-token → Anthropic-compatible upstream 候选验收。
2. 用真实 Provider 结果生成 fidelity/cost/latency eval artifact，并由配置 authority 签署精确 provider/route/model/endpoint Capability。
3. 增加 malformed/parallel/adversarial/fault-injection corpus；OpenAI Provider-specific planner 继续保持 shadow-only。
4. 后续独立实现 history collapse/cache-zone 与 P10/Expected/P90 经济区间，不与首批 active codec 混合。
5. 前端接入另立任务；只消费 public summary/lifecycle/events，不新增面向用户的实验室顶级导航。

发布禁令：真实模型 fidelity/cost/latency gate、精确签名 Capability 和同候选版真实 routed evidence 完成前，禁止默认开启 production active transform；post-commit 永不自动原文重试。

事实源：

- `docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`
- `docs/optical/implementation-status.md`
- `docs/optical/review-round-5.md`
- `support/pending-commits.json`

## 5. CI Gate（贯穿 M4–M6）

当前 `.github/workflows/ci.yml` 已含 structure、前端 test/typecheck/build、Rust fmt/clippy/test 和 no-touch fixture smoke；`macos-release.yml` 对公开产物强制 Developer ID、notarization、staple、Gatekeeper 和哈希，缺凭据即失败。2026-07-13 当前本地候选门禁为前端 125 项通过、Playwright E2E 20 项通过、`npm audit` 0 个漏洞；macOS 六页基线截图位于 `support/artifacts/macos-ui/final-candidate-2026-07-12T18-52-02-502Z/`，当前 autostart/窄窗口证据位于 `support/artifacts/macos-ui/final-candidate-2026-07-13/`。当前工作树 ad-hoc desktop binary SHA-256 为 `6a04409997ac2741b7a61e4e72ad7eac87649d56eaf8fc70918d8c336fbe3048`，正式发布仍须在候选 commit 上重新签名构建。计划延续并加强：

1. `python support/scripts/check_structure.py` —— 目录边界。
2. `cargo fmt --all -- --check` / `cargo check --workspace --tests` / `cargo test --workspace`。
3. no-touch fixture smoke（`support/scripts/run_fixture_tests.*`）。
4. `npm run build`（`code/apps/desktop`）。
5. M4 起：InstallationTransaction 回滚单测、Install Doctor 静态检查。
6. M5：E2E 兼容套件、Windows installer 与 macOS 签名产物成为候选版硬门禁。

## 6. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| 回退到静默 fallback | route 误走官方、安全/成本/合规风险 | route decision 强制进 Ledger；401 显式分类；CI 加 route 静默 fallback 检测单测 |
| Headroom 上游漂移 | runtime 吸收失锚、审计断裂 | 锁 upstream base commit + NOTICE/变更说明 + 定期 rebase 计划 |
| Windows 路径/编码 | 覆盖安装回滚失败、项目名连字符误反推 | UTF-8 强制 + session `cwd`/显式 project root 优先 + `.cmd` shim fallback + CI 加 Windows 兼容用例 |
| no-touch 边界被 Managed 写穿 | 开发期破坏本机环境 | InstallationTransaction 不在开发默认路径生效；fixture/fake-binary 为 gate |
| savings 双计 | Dashboard 误导、对外不可解释 | M6 native ledger 统一 denominator，禁止 rtk+runtime 直接相加（PRD §10） |
| 打包/签名复杂度 | 构建配置存在但被误判为可发布 | 同一候选 commit 绑定签名、notarization、artifact hash、安装后运行与实机验收 |

## 7. 文档维护约定

- 新里程碑交付后写交付报告 `docs/history/M{n}_*.md` 并更新本文件 §0 当前坐标。
- `docs/index.md` 同步增删条目。
- 旧计划只进 `docs/archive/`，不就地改写历史。
- PRD 边界变更先于 PLAN：先改 PRD，再在 PLAN 引用新契约，避免计划与产品真理源漂移。
