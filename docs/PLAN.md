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
| 当前 Node | M4 实施中：真实 Rust route/runtime、桌面控制面和 macOS 打包已推进；安装事务、数据持久化和平台发布门禁未闭环 |
| 历史路线图 | `docs/archive/ROADMAP_M0_M6_HISTORICAL.md`（M0–M6 原文快照） |

当前 M4 历史设计输入：

- `docs/history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md` — 当时的真实能力接入计划，仅作为历史设计和既有测试证据；当前执行状态与剩余工作以本文件、PRD、`docs/product/IMPLEMENTATION_STATUS.md` 和当前源码/测试为准。

代码现状盘点（2026-07-11）：

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
.github/workflows/macos-release.yml   macOS app/DMG、可选签名与 notarization workflow
```

当前已交付基础：

1. d-token-owned Rust local service 已提供 OpenAI Chat/Responses、Anthropic Messages、Dashboard、stats/history/metrics、Provider/Route/Instance、Doctor、Event/Logs/Export 与 Optical shadow APIs。
2. Dashboard 主运行数据可以经 Tauri command 从 d-token local service 读取；浏览器预览仍使用示例数据并明确不代表真实运行时。
3. 已有真实 OpenCode routed test 证据，证明过一次 `OpenCode -> d-token -> upstream`；本轮仍需在同一候选 commit 上复验。
4. Tauri bundling 已启用：Windows NSIS，macOS `.app`/DMG；macOS workflow 已提供可选签名/notarization步骤。
5. OS credential store 写入、引用和出站短生命周期读取已有 Rust 实现；仍需认证 Provider 的候选版实机证据。

当前发布阻断：

1. 安装/Agent 配置/回退尚未形成覆盖原子写入、语义验证、并发修改与故障注入的完整事务。
2. `get_desktop_data` 等桌面聚合面仍有硬编码/多事实源，必须统一 runtime evidence、freshness 与 action result。
3. JSONL 的坏行隔离、轮转、锁、损坏诊断与 SQLite 查询索引未完成。
4. Provider 并发/背压/熔断、流式桥接完整性和 Doctor 全矩阵仍未达到稳定 Beta 门禁。
5. macOS 需要把同一候选 commit 的窗口 UX、安装后 runtime、真实 OpenCode、artifact hash 与签名状态绑定；Windows 仍需安装包/凭据/路径/回退/视觉一致性实机验收。

## 1. 计划原则

1. **PRD 主产品契约，PLAN 主执行节奏**：边界/验收改 PRD，任务拆解/排期改 PLAN。
2. **事务优先**：任何真实写操作必须先有 fixture/fake-binary 故障验证、diff、backup、verify、rollback；真实路径仅在用户明确操作和受控边界内开启。
3. **每个任务带 DoD**：Definition of Done = 可验证项 + 涉及文件 + 测试 + 回滚证据。
4. **不破坏已交付里程碑**：M1–M3 验收项保持绿，新工作不得回退 no-touch 安全边界。
5. **Windows/macOS 一等公民**：任何 install/patch/rollback 任务必须显式覆盖 UTF-8、盘符路径、项目名连字符、`.cmd` shim、xattr/quarantine。
6. **无静默 fallback**：route/provider/安装失败必须可见、可解释、可审计。

## 2. M4：Production Managed Installer（实施中）

目标：成品支持一键安装、图形化配置、可回滚。用户只需下载 d-token App，不用先装 Headroom/RTK。

M4 的实际落地先走 `OpenCode Real Route Track`：使用用户原本 OpenCode 配置，不做隔离 HOME / fake config，把 OpenCode provider/baseURL 指向 d-token 自有端口，再由 d-token 完成压缩、路由、Event Ledger 和 upstream 转发。该链路已有一次历史真实证据，但必须在当前候选 commit 上重跑并与 App/runtime/ledger 证据关联。历史设计见 `docs/history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md`。

### M4.S1 安装事务引擎（InstallationTransaction）

落位：`code/crates/dtoken-config-patcher`（扩展）或新建 `dtoken-install-transaction`。

- [ ] 实现 `InstallationTransaction` 状态机：scan → plan → show diff → backup → confirm → apply → verify → doctor → commit rollback point。
- [ ] 失败语义：只停 d-token 自有新进程；kill 前校验进程身份；恢复 backup；清理 partial；输出精确失败与恢复动作。
- [ ] 事务日志进入 Event Ledger，stage=`install_step` / `config_patch`。
- DoD：fixture 下跑通 apply→fail→rollback 全链路单测；真实写操作在 M4.S5 之前禁止。
- 涉及：`dtoken-config-patcher`、`dtoken-event-ledger`、`dtoken-process`。

### M4.S2 Managed d-token runtime 安装/启用/升级/停止/卸载

- [ ] runtime 打包策略：正式产物是本仓库 Rust workspace 的 `dtoken-proxy` external binary；`code/runtimes/headroom/` 仅保留 Headroom-derived provenance/迁移边界，不作为 Python 或外部服务运行目录。
- [ ] install/update/uninstall 全部走 InstallationTransaction，并保留 binary hash、version、target triple 与候选 commit provenance。
- [ ] integrated RTK 只能通过 d-token runtime 内部能力启用，不暴露 standalone RTK 为产品主路径。
- DoD：fake-binary 模式下完成 install/update/uninstall 全流程；真实 bundle 能证明 binary identity、版本与候选来源。
- 涉及：`dtoken-process`、`dtoken-config-patcher`、`dtoken-proxy`、Tauri sidecar 准备脚本、`support/fake-bin/`。

### M4.S3 Provider / Route / cc-switch 安装与配置

- [ ] ProviderProfile / RouteProfile / ChainBridgeProfile 的 patch 走 transaction。
- [ ] route decision 进 Event Ledger（input/output protocol、target provider、cc-switch endpoint）。
- [ ] 401/403/429/5xx 显式归类为 route/provider mismatch，禁止静默 fallback 到官方。
- [ ] OpenCode 真实 route 使用用户原本 OpenCode 配置；如需加入 d-token provider，先备份原配置，只写最小 provider/model/baseURL 变更，并在报告记录 backup 与 diff 摘要。
- DoD：fixture 下覆盖 official / custom_openai / custom_anthropic / cc_switch / chain_bridge 五条路径，Event Ledger 可重建请求走向。
- 涉及：`dtoken-router`、`dtoken-config-patcher`、`dtoken-proxy`、`dtoken-event-ledger`。

### M4.S4 Agent wrap / Shell profile / 配置 patch

- [ ] agent wrap setup（Claude Code / Codex / Cursor / Cline / OpenCode 等矩阵，PRD §8.1）。
- [ ] shell profile / agent config patch：bash / zsh / powershell / cmd / wsl-bash。
- [ ] Agent instruction 文件中 d-token/runtime/RTK block 的 diff、backup、rollback。
- DoD：每个 agent 在 fake-binary + 临时 HOME/PATH 下完成 patch 与回滚；CompatibilityMatrix 状态可写回。
- 涉及：`dtoken-config-patcher`、`dtoken-process`、前端 agent 矩阵页。

### M4.S5 Managed 真实路径 + Install Doctor

- [ ] 在 InstallationTransaction 之上打开真实写路径，仍强制 backup/verify/rollback。
- [ ] Install Doctor：runtime / proxy / 端口占用 / 进程身份 / integrated RTK / 外部 standalone RTK 冲突 / provider route / cc-switch route / config backup / Windows UTF-8/shim / macOS xattr/quarantine/keychain/launch agent。
- [ ] rollback point 落盘并可在 GUI 调用。
- DoD：在隔离 VM/临时机上完成覆盖安装、回滚、再升级；Install Doctor 全绿。
- 涉及：`dtoken-doctor`、`dtoken-config-patcher`、桌面 Doctor 页。

### M4.S6 前端接通本地 API

- [x] Tauri Dashboard 主路径从 `dtoken-core-api` 读取 d-token-owned runtime；浏览器模式保留为明确标识的 UI 预览。
- [ ] InstallationTransaction / route decision / Doctor 状态在 GUI 实时可见。
- [x] Optical shadow 默认关闭 active transform，仅提供 preview/metadata observability。
- DoD：Tauri 窗口不以 fixture/硬编码冒充本机事实，managed 链路的 source/freshness/action result 完整可见。
- 涉及：`code/apps/desktop/src/*`、`dtoken-core-api`。

### M4 验收（对齐 PRD §18 M4 与 §19）

- 成品可覆盖安装与配置，所有写操作可追踪、可备份、可回滚。
- 用户不需要独立安装 Headroom/RTK。
- 不出现静默 fallback；route decision 可审计。
- Windows UTF-8/path/shim 检查进入 Install Doctor 且为 gate。

## 3. M5：Production Hardening + Release

目标：可以放心让外部用户下载、覆盖安装、回滚、升级、排障。

### M5.S1 打包与签名（构建配置已启用，发布证据未闭环）

- [x] Windows NSIS 与 macOS `.app`/DMG 的 Tauri bundle 配置。
- [ ] Windows 安装后 UTF-8、路径、credential、`.cmd` shim 与回退实机验收。
- [ ] macOS 签名/notarization、xattr/quarantine、安装后 runtime 与 artifact hash 绑定验收。
- [ ] updater 通道与版本 provenance。
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
- CI gate 含 structure check、cargo check、cargo test、no-touch smoke、desktop build。

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

定位：Optical 是混合上下文优化引擎中的一种有损 codec，当前只允许 shadow preview 和 metadata-only observability，不进入生产请求改写。

截至 2026-07-11 已完成：

- [x] 安全默认关闭的全局/路由策略与版本化 Capability Snapshot。
- [x] Capability 生命周期：schema v1、active/revoked/superseded、撤销 revision 屏障、重复 snapshot ID 阻断。
- [x] Request-local TextArena、真实文本风险分类、Capability-driven geometry、selected-only economics。
- [x] Anthropic 工具依赖图、并行 tool use/result、orphan/duplicate/recent-tail fail-closed。
- [x] OpenAI Chat `tool_calls` / `tool` 的独立 inventory 与状态机 lowering；尚未接入 active 决策。
- [x] Raw request bytes SHA-256 与 Canonical JSON SHA-256 v1 计划证据。
- [x] Preview、Status、Capabilities、Capability Lifecycle、Recent Events API。
- [x] metadata-only 事件：状态、模式、候选计数、图计数、经济摘要和 Capability 规则计数。
- [x] `support/pending-commits.json` schema v2：阶段、时间、文件、验证、共享文件暂存方式、是否提交、提交哈希。

下一实施顺序：

1. Snapshot authority/signature/trust source 与只读 revocation audit。
2. 在 HTTP ingress 保留真正的原始请求 bytes，并将 raw-byte fingerprint 接入 Preview Plan。
3. OpenAI Provider-specific planner 保持 shadow-only，增加 malformed/parallel/adversarial fixture。
4. Economics 从单点估算升级为 P10 / Expected / P90 区间。
5. 在现有上下文优化信息架构中展示 public summary、lifecycle 和 recent events，不新增面向用户的实验室顶级导航。
6. Fidelity corpus、renderer feature flag、Anthropic patcher、post-execution validator、rollback。

发布禁令：在 renderer、patcher、post-validator、rollback 和真实模型 fidelity gate 完成前，禁止开启 production active transform。

事实源：

- `docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`
- `docs/optical/implementation-status.md`
- `docs/optical/review-round-5.md`
- `support/pending-commits.json`

## 5. CI Gate（贯穿 M4–M6）

当前 `.github/workflows/ci.yml` 已含 structure、前端 test/typecheck/build、Rust fmt/clippy/test 和 no-touch fixture smoke；`macos-release.yml` 增加 macOS bundle 与可选签名/notarization。计划延续并加强：

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
