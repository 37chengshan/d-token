# d-token 执行计划（Living Plan）

状态：Living Document  
维护规则：本文件是 d-token 的唯一执行计划，按代码现状滚动维护。产品边界改 PRD，阶段任务改本文件。PRD（`docs/PRD.md`）是产品真理源，本文件不得与其验收标准冲突。App 页面、功能、UI 与用户体验逻辑见 `docs/APP_PAGE_UX_SPEC.md`。

## 0. 当前坐标

| 维度 | 现状 |
|------|------|
| PRD | 单一 living PRD，§18 已去计划化，指向本文件 |
| 页面/UX 规划 | `docs/APP_PAGE_UX_SPEC.md`，定义每个页面、用户路径、端口识别、体验红线 |
| OpenCode 测试计划 | `docs/OPENCODE_LOCAL_TEST_PLAN.md`，定义新端口本地真实联调 |
| 代码 | Rust workspace 13 crate + Tauri/React/Vite 桌面 App |
| 已交付里程碑 | M1（no-touch 基础）、M2（Headroom 只读集成）、M3（GUI MVP） |
| 交付证据 | `M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md`、`M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md`、`M3_GUI_CONTROL_PLANE_MVP_REPORT.md`、`M1_M3_DELIVERY_AUDIT.md` |
| 当前 Node | M3 已完成，M4 未启动 |
| 历史路线图 | `docs/archive/ROADMAP_M0_M6_HISTORICAL.md`（M0–M6 原文快照） |

代码现状盘点（M3 收尾时的骨架）：

```text
code/crates/
  dtoken-types, dtoken-config, dtoken-router, dtoken-compress,
  dtoken-process, dtoken-doctor, dtoken-event-ledger,
  dtoken-preprocessor, dtoken-headroom-adapter, dtoken-project-policy,
  dtoken-config-patcher, dtoken-core-api, dtoken-proxy
code/apps/desktop/   Tauri 2 + React + Vite，UI 已覆盖 Dashboard/RTK/Headroom/Doctor/策略/回滚矩阵/i18n
code/runtimes/headroom/   Headroom-derived runtime 落位占位
support/fixtures/ support/fake-bin/   no-touch 测试资产
.github/workflows/ci.yml   structure / cargo test / no-touch smoke / desktop build
```

已知缺口：

1. Dashboard 前端仍使用静态 fixture 数据，**未接本地 API**。
2. Headroom HTTP reader 已实现但 gated by `DTOKEN_HEADROOM_BASE_URL`，未在 GUI 暴露。
3. 生产 Tauri 打包（bundling/installer）尚未启用，留给 M4。
4. layered savings attribution 仍是 v0，统一 denominator 未落地。
5. Windows GNU 工具链需 MSYS2 MinGW（`C:\msys64\mingw64\bin`），CI 用 Linux gate 保证跨平台。
6. OpenCode 新端口测试计划已落文档，但尚未实现 fake-opencode route smoke、真实本地 route probe、GUI Test Runner。
7. App 尚缺 Port Discovery 页面：不能自动识别并展示 cc-switch、自定义 provider、OpenCode server、d-token test route 等端口关系。

## 1. 计划原则

1. **PRD 主产品契约，PLAN 主执行节奏**：边界/验收改 PRD，任务拆解/排期改 PLAN。
2. **APP_PAGE_UX_SPEC 主页面体验**：每个页面做什么、UI 大概如何、用户体验是否闭环，以 `docs/APP_PAGE_UX_SPEC.md` 为准。
3. **No-touch 优先**：M4/M5 成品写操作也必须先在 fixture/fake-binary 下可跑、可测、可回滚，再开 Managed 真实路径。
4. **每个任务带 DoD**：Definition of Done = 可验证项 + 涉及文件 + 测试 + 回滚证据。
5. **不破坏已交付里程碑**：M1–M3 验收项保持绿，新工作不得回退 no-touch 安全边界。
6. **Windows/macOS 一等公民**：任何 install/patch/rollback 任务必须显式覆盖 UTF-8、盘符路径、项目名连字符、`.cmd` shim、xattr/quarantine。
7. **无静默 fallback**：route/provider/安装失败必须可见、可解释、可审计。
8. **端口可见**：d-token、OpenCode、cc-switch、自定义 provider、测试 route 的端口关系必须在 GUI 中可见。
9. **candidate 不自动入生产**：自动发现的 cc-switch/custom provider candidate 必须由用户确认后才能用于生产 route。

## 2. M4：Production Managed Installer + Route/Port UX

目标：成品支持一键安装、图形化配置、端口自动识别、OpenCode 新端口测试、可回滚。用户只需下载 d-token App，不用先装 Headroom/RTK。

### M4.S1 安装事务引擎（InstallationTransaction）

落位：`code/crates/dtoken-config-patcher`（扩展）或新建 `dtoken-install-transaction`。

- [ ] 实现 `InstallationTransaction` 状态机：scan → plan → show diff → backup → confirm → apply → verify → doctor → commit rollback point。
- [ ] 失败语义：只停 d-token 自有新进程；kill 前校验进程身份；恢复 backup；清理 partial；输出精确失败与恢复动作。
- [ ] 事务日志进入 Event Ledger，stage=`install_step` / `config_patch`。
- DoD：fixture 下跑通 apply→fail→rollback 全链路单测；真实写操作在 M4.S5 之前禁止。
- 涉及：`dtoken-config-patcher`、`dtoken-event-ledger`、`dtoken-process`。

### M4.S2 Managed runtime 安装/启用/升级/停止/卸载

- [ ] runtime 打包策略：`code/runtimes/headroom/` 落 Headroom-derived runtime，记录 upstream base commit、license、NOTICE、修改边界。
- [ ] install/update/uninstall 语句走 InstallationTransaction；保留 version provenance。
- [ ] integrated RTK 通过 runtime 内部路径启用，不暴露 standalone 路径。
- [ ] RTK 只作为 agent wrap / command rewrite 链路能力，不允许被实现成 provider proxy hot path compressor。
- DoD：fake-binary 模式下完成 install/update/uninstall 全流程；version provenance 可查。
- 涉及：`dtoken-headroom-adapter`、`dtoken-config-patcher`、`code/runtimes/headroom/`、`support/fake-bin/`。

### M4.S3 Provider / Route / cc-switch 安装与配置

- [ ] ProviderProfile / RouteProfile / ChainBridgeProfile 的 patch 走 transaction。
- [ ] route decision 进 Event Ledger（input/output protocol、target provider、cc-switch endpoint）。
- [ ] 401/403/429/5xx 显式归类为 route/provider mismatch，禁止静默 fallback 到官方。
- [ ] 明确 credential precedence：explicit d-token route credential → sandbox config → explicit external mode existing credential；测试模式不得静默使用用户已有 auth。
- DoD：fixture 下覆盖 official / custom_openai / custom_anthropic / cc_switch / chain_bridge 五条路径，Event Ledger 可重建请求走向。
- 涉及：`dtoken-router`、`dtoken-config-patcher`、`dtoken-proxy`、`dtoken-event-ledger`。

### M4.S4 Agent wrap / Shell profile / 配置 patch

- [ ] agent wrap setup（Claude Code / Codex / Cursor / Cline / OpenCode 等矩阵，PRD §8.1）。
- [ ] shell profile / agent config patch：bash / zsh / powershell / cmd / wsl-bash。
- [ ] Agent instruction 文件中 d-token/runtime/RTK block 的 diff、backup、rollback。
- [ ] OpenCode patch 优先支持临时 HOME / 临时 project config；真实 patch 必须二次确认。
- DoD：每个 agent 在 fake-binary + 临时 HOME/PATH 下完成 patch 与回滚；CompatibilityMatrix 状态可写回。
- 涉及：`dtoken-config-patcher`、`dtoken-process`、前端 agent 矩阵页。

### M4.S5 Managed 真实路径 + Install Doctor

- [ ] 在 InstallationTransaction 之上打开真实写路径，仍强制 backup/verify/rollback。
- [ ] Install Doctor：runtime / proxy / 端口占用 / 进程身份 / integrated RTK / 外部 standalone RTK 冲突 / provider route / cc-switch route / config backup / Windows UTF-8/shim / macOS xattr/quarantine/keychain/launch agent。
- [ ] Doctor 增加 OpenCode 本地测试检查：OpenCode binary、Windows shim、OpenCode server、NO_PROXY、test route 是否指向 d-token。
- [ ] rollback point 落盘并可在 GUI 调用。
- DoD：在隔离 VM/临时机上完成覆盖安装、回滚、再升级；Install Doctor 全绿。
- 涉及：`dtoken-doctor`、`dtoken-config-patcher`、桌面 Doctor 页。

### M4.S6 前端接通本地 API

- [ ] Dashboard 从 fixture 静态数据切到 `dtoken-core-api` 真实数据；保留 Fixture Demo Mode 作为默认安全入口。
- [ ] InstallationTransaction / route decision / Doctor 状态在 GUI 实时可见。
- [ ] Shadow Detect 仍默认禁用，避免误碰真实环境。
- [ ] Dashboard 必须展示数据来源：fixture / shadow / managed / test route。
- DoD：开发期默认 fixture，切换到 managed 后链路条 probe 完整可见。
- 涉及：`code/apps/desktop/src/*`、`dtoken-core-api`。

### M4.S7 Port Discovery：自动识别 cc-switch/custom 转发端口

目标：App 必须有页面或一级 Tab 自动识别并展示 cc-switch 转发端口、自定义 provider 转发端口、d-token runtime 端口、OpenCode server 端口和测试端口。

- [ ] 新增 Port Discovery 数据模型：host、port、kind、confidence、owner、protocol_guess、health_status、source、last_seen_at。
- [ ] 端口来源：d-token route registry、Event Ledger、ProviderProfile、RouteProfile、OpenCode config、cc-switch config/进程参数、loopback health probe。
- [ ] 支持非侵入式探测：`/health`、`/v1/models`、`/global/health`、OpenCode server health。
- [ ] 识别结果分为 confirmed / candidate / unknown。
- [ ] candidate 不得自动接入生产 route；必须用户确认。
- [ ] GUI 展示：d-token runtime、OpenCode server、cc-switch candidate、custom provider、test route。
- [ ] 支持动作：Rescan、Confirm as cc-switch、Confirm as custom provider、Create test route、Ignore。
- DoD：用户能在 GUI 中清楚看到“哪个端口是谁开的、请求会转发到哪里、是否已确认”。
- 涉及：`dtoken-router`、`dtoken-doctor`、`dtoken-core-api`、`code/apps/desktop/src/*`、`dtoken-event-ledger`。

### M4.S8 OpenCode Local New-Port E2E

目标：用 OpenCode 作为 P0 测试 agent，真实验证新端口链路。

- [ ] 增加 fake OpenCode binary 和 OpenCode fixture route。
- [ ] 增加 test port allocator：默认建议 `127.0.0.1:18180` 起，冲突则顺延，不盲目 kill。
- [ ] 明确 OpenCode server port 与 d-token test port 的关系；两者不得混淆。
- [ ] 检查 `NO_PROXY=localhost,127.0.0.1`，避免本地代理循环。
- [ ] 支持链路 A：OpenCode → d-token test port → custom provider URL。
- [ ] 支持链路 B：OpenCode → d-token test port → cc-switch → final provider。
- [ ] 测试端口进入 Dashboard、Doctor、Event Ledger。
- [ ] 测试结束后释放端口，并验证无残留进程。
- DoD：fixture/fake-binary 下跑通两条链路；真实本地 route probe 至少能跑通 custom provider URL 链路。
- 涉及：`support/fake-bin/`、`support/fixtures/`、`dtoken-router`、`dtoken-proxy`、`dtoken-doctor`、`dtoken-core-api`、桌面 OpenCode Test Runner。

### M4.S9 App Page UX 落地

目标：按 `docs/APP_PAGE_UX_SPEC.md` 实现或预留页面骨架。

- [ ] Setup Wizard。
- [ ] Agent Selection。
- [ ] Runtime Status。
- [ ] Providers & Routes。
- [ ] Port Discovery。
- [ ] OpenCode Test Runner。
- [ ] Dashboard route chain。
- [ ] Doctor route/port diagnosis。
- [ ] Logs / Config Diff / Backup & Rollback / Compatibility Matrix / Settings。
- DoD：每个 P0 页面至少有 fixture 数据、空状态、错误态、成功态和中文文案。
- 涉及：`code/apps/desktop/src/*`、`dtoken-core-api`。

### M4 验收（对齐 PRD §18/§19 与 APP_PAGE_UX_SPEC）

- 成品可覆盖安装与配置，所有写操作可追踪、可备份、可回滚。
- 用户不需要独立安装 Headroom/RTK。
- 不出现静默 fallback；route decision 可审计。
- Windows UTF-8/path/shim 检查进入 Install Doctor 且为 gate。
- GUI 能展示 d-token、OpenCode、cc-switch、自定义 provider、测试 route 的端口关系。
- Port Discovery 可以识别 cc-switch/custom provider candidate，但必须用户确认后才能进入生产 route。
- OpenCode Test Runner 可以新开测试端口完成 custom provider route smoke，并预留 cc-switch route smoke。

## 3. M5：Production Hardening + Release

目标：可以放心让外部用户下载、覆盖安装、回滚、升级、排障。

### M5.S1 打包与签名

- [ ] Windows installer（Tauri MSI/NSIS，UTF-8、`.cmd` shim）。
- [ ] macOS bundle + notarization + xattr quarantine 处理。
- [ ] updater 通道与版本 provenance。
- DoD：干净机器上下载即装、覆盖安装、回滚、升级四场景通过。

### M5.S2 兼容性与 E2E

- [ ] Integrated RTK 兼容矩阵（PRD §16）。
- [ ] Provider / cc-switch E2E 兼容套件（official / custom_openai / custom_anthropic / cc_switch / chain_bridge）。
- [ ] OpenCode E2E 兼容套件：Windows native + npm shim、Windows WSL、macOS。
- [ ] Port Discovery E2E：d-token runtime、OpenCode server、cc-switch candidate、custom provider candidate、unknown local service。
- [ ] runtime install/uninstall rollback 压力测试。
- DoD：兼容矩阵无 untested 之外的状态在受控平台收敛；E2E 套件 CI 可跑。

### M5.S3 隐私、安全与合规审计

- [ ] privacy / export / redaction audit（PRD §15）。
- [ ] upstream license / NOTICE / base commit / 变更说明审计。
- [ ] 不上传 tracking DB / tee / prompt / 代码 / 命令输出；日志默认脱敏二次确认。
- [ ] Tauri IPC / capabilities / command scope 审计：前端只能调用白名单 command；install/apply/rollback 必须后端确认。
- [ ] API key / token 不进入 WebView 明文状态。
- DoD：审计报告归档 `docs/`；导出样本经脱敏校验。

### M5.S4 稳定性与文档

- [ ] crash recovery、benchmark/evals。
- [ ] user guide / developer guide / troubleshooting guide。
- [ ] OpenCode local test guide。
- [ ] Port Discovery troubleshooting guide。
- DoD：文档覆盖安装、配置、回滚、排障、端口识别、OpenCode 测试六大路径，可被 CI 引用结构检查。

### M5 验收

- 对齐 PRD §19 全部验收中 M5 范围项。
- CI gate 含 structure check、cargo check、cargo test、no-touch smoke、desktop build、fake-opencode route smoke、test port allocation test。
- Windows/macOS 打包和 OpenCode E2E 至少在受控机器上通过。

## 4. M6：d-token-native Runtime Innovation

目标：用户体验不变前提下，逐步把 Headroom-derived runtime 关键模块替换为 d-token-native。

- [ ] 替换脆弱 CLI/config/dashboard glue → Rust-native。
- [ ] dtoken-router 升级为 native route adapter（保留 Headroom-compatible API）。
- [ ] dtoken-event-ledger 升级 native observability（统一 denominator，落实 PRD §10 分层 savings）。
- [ ] dtoken-proxy / 协议适配层 native 化（OpenAI/Anthropic）。
- [ ] dtoken-project-policy 压缩策略引擎 native 化。
- [ ] Port Discovery 由 fixture/heuristic 升级为 runtime-native endpoint registry。
- [ ] 保留必要 Headroom-compatible 迁移层，可逐模块灰度替换并回退。
- DoD：每个模块替换前有对比基准 + 回退路径；用户仍只下载 d-token App。

## 5. CI Gate（贯穿 M4–M6）

当前 `.github/workflows/ci.yml` 已含 structure / cargo test / no-touch smoke / desktop build。计划延续并加强：

1. `python support/scripts/check_structure.py` —— 目录边界。
2. `cargo fmt --all -- --check` / `cargo check --workspace --tests` / `cargo test --workspace`。
3. no-touch fixture smoke（`support/scripts/run_fixture_tests.*`）。
4. `npm run build`（`code/apps/desktop`）。
5. M4 起：InstallationTransaction 回滚单测、Install Doctor 静态检查。
6. M4 起：Port Discovery fixture test，覆盖 confirmed / candidate / unknown。
7. M4 起：fake-opencode route smoke，覆盖 custom provider route 与 cc-switch route。
8. M4 起：local test port allocation test，验证端口冲突时顺延且不 kill 未验证进程。
9. M5 起：E2E 兼容套件子集、Windows/macOS 打包构建（条件 gate）。
10. M5 起：Tauri IPC/capabilities 静态审计。

本地 Windows GNU 工具链约定：`C:\msys64\mingw64\bin` 在 PATH（`dlltool.exe` / `gcc.exe`），交叉平台一致性靠 Linux CI gate。

## 6. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| 回退到静默 fallback | route 误走官方、安全/成本/合规风险 | route decision 强制进 Ledger；401 显式分类；CI 加 route 静默 fallback 检测单测 |
| Headroom 上游漂移 | runtime 吸收失锚、审计断裂 | 锁 upstream base commit + NOTICE/变更说明 + 定期 rebase 计划 |
| Windows 路径/编码 | 覆盖安装回滚失败、项目名连字符误反推 | UTF-8 强制 + session `cwd`/显式 project root 优先 + `.cmd` shim fallback + CI 加 Windows 兼容用例 |
| no-touch 边界被 Managed 写穿 | 开发期破坏本机环境 | InstallationTransaction 不在开发默认路径生效；fixture/fake-binary 为 gate |
| savings 双计 | Dashboard 误导、对外不可解释 | M6 native ledger 统一 denominator，禁止 rtk+runtime 直接相加（PRD §10） |
| 打包/签名复杂度 | M5 延期 | M4 先打通事务与回滚，打包单独切片；Tauri bundling 在 M4 末再启 |
| OpenCode test route 未纳入主计划 | 看似完成 M4，但真实 agent 链路没打通 | M4.S8 单独列为 gate；CI 增 fake-opencode route smoke 与 test port allocation test |
| cc-switch/custom provider 端口识别错误 | 请求走错上游或误把未知服务当 provider | Port Discovery 分 confirmed/candidate/unknown；candidate 必须用户确认；不扫描非 loopback |
| 测试端口污染生产 route | 本地测试影响真实使用 | test route 标记；端口独立分配；测试结束释放并验证无残留 |
| Tauri IPC 权限过宽 | 前端误触发写配置、kill 进程或泄露密钥 | M5 加 IPC/capabilities 审计；install/apply/rollback 后端白名单与二次确认 |

## 7. 文档维护约定

- 新里程碑交付后写交付报告 `docs/M{n}_*.md` 并更新本文件 §0 当前坐标。
- `docs/index.md` 同步增删条目。
- `docs/APP_PAGE_UX_SPEC.md` 维护页面、功能、UI、用户体验和验收逻辑。
- `docs/OPENCODE_LOCAL_TEST_PLAN.md` 维护 OpenCode 新端口本地测试细节。
- 旧计划只进 `docs/archive/`，不就地改写历史。
- PRD 边界变更先于 PLAN：先改 PRD，再在 PLAN 引用新契约，避免计划与产品真理源漂移。
