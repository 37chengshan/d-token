# 历史路线图归档：M0–M6

状态：Historical Snapshot
归档时间：2026-07-02
来源：原 `docs/PRD.md` §18 里程碑路线图，PRD 去计划化前的原文快照。

> 本文件是历史归档，仅供追溯。当前活跃计划以 `docs/PLAN.md` 为准。
> PRD 只保留产品真理源（目标、边界、契约、验收），不再承载阶段执行计划。
> 归档与现状的差异：M1–M3 已交付（见各里程碑交付报告与 `M1_M3_DELIVERY_AUDIT.md`），M4–M6 尚未启动，相关条目在 `docs/PLAN.md` 中按当前代码现状重新拆解。

## M0：文档、研究与路线收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. Headroom-derived runtime 吸收路线研究。
6. Provider / cc-switch / chain bridge 路由研究。
7. PRD 多维审查报告。
8. 仓库目录收敛。

## M1：No-touch Dev Foundation

目标：开发期完全不破坏本机已有环境。

1. runtime/RTK/cc-switch fixtures。
2. fake runtime / fake rtk / fake cc-switch binary。
3. fixture mode。
4. fake-binary mode。
5. Runtime adapter skeleton。
6. Integrated RTK status skeleton。
7. ProviderRoute skeleton。
8. ProjectPolicy skeleton。
9. ConfigPatcher skeleton，但默认只 plan 不 apply。
10. EventLedger skeleton。
11. local API skeleton。
12. 最小 CI：structure check、cargo check、cargo test、no-touch smoke。

M1 验收：d-token 能在不调用真实 Headroom/RTK/cc-switch、不读取真实 agent 配置、不修改真实环境的前提下完成基础 API 与测试。

## M2：Headroom-derived Runtime Import + External Read-only Integration

目标：把 Headroom 能力作为 d-token runtime 的来源纳入仓库/构建/打包计划，同时保留只读外部连接用于迁移验证。

1. 引入 Headroom-derived runtime 代码或子模块/源码镜像策略。
2. 记录 upstream base commit、license、NOTICE、修改边界。
3. 真实读取 runtime health/stats/history/metrics。
4. 真实检测 integrated RTK binary/version/metrics。
5. 只读解析 existing Headroom wrap / agent 状态。
6. 真实检测 provider route 与 cc-switch health。
7. EventLedger JSONL 持久化。
8. layered savings attribution v0。
9. Doctor 能诊断 runtime/RTK/provider/cc-switch/route/config/platform。

M2 验收：d-token 可以只读连接真实现有安装，也可以在开发环境中运行 Headroom-derived runtime，但不会默认写真实配置或控制真实服务。

## M3：GUI Control Plane MVP

目标：开始图形化，但 GUI 开发默认 fixture，不破坏开发者本机环境。

1. Tauri shell。
2. React Dashboard。
3. Fixture Demo Mode 默认入口。
4. 傻瓜模式 / 高级模式 UI。
5. Agent selection UI。
6. Provider setup UI。
7. 自定义 URL UI。
8. cc-switch route UI。
9. Runtime Status cards。
10. Integrated RTK Status cards。
11. Layered savings cards。
12. Project compression policy cards。
13. Doctor UI。
14. Compatibility warning cards。
15. Config diff / backup / rollback UI。
16. 中文 / English i18n。

M3 验收：开发期 GUI 不碰真实环境；用户主动 shadow/external 时只读连接。

## M4：Production Managed Installer

目标：成品支持一键安装、图形化配置和可回滚。用户只需要下载 d-token App，不需要先安装 Headroom 或 RTK。

1. InstallationTransaction。
2. Tool selection wizard。
3. Managed d-token runtime install/update/uninstall。
4. Integrated RTK install/update/uninstall through runtime path。
5. d-token desktop/proxy/route adapter install/update/uninstall。
6. Optional cc-switch install/connect/configure。
7. Agent wrap setup。
8. Shell profile / agent config patch。
9. Provider route patch。
10. Config diff / backup / apply / rollback。
11. Install Doctor。
12. rollback point。
13. version provenance。
14. Windows UTF-8/path/shim compatibility checks。

M4 验收：成品可以覆盖安装和配置，但所有写操作都可追踪、可备份、可回滚；用户不需要独立安装 Headroom/RTK。

## M5：Production Hardening + Release

目标：正式发布。

1. Windows installer。
2. macOS bundle。
3. updater。
4. signing / notarization。
5. benchmark / evals。
6. crash recovery。
7. Integrated RTK compatibility matrix。
8. Provider / cc-switch E2E compatibility suite。
9. runtime install/uninstall rollback。
10. privacy/export/redaction audit。
11. upstream license/NOTICE audit。
12. CI gate：structure check、cargo check、cargo test、no-touch smoke、desktop build。
13. E2E compatibility test suite。
14. User guide / developer guide / troubleshooting guide。

M5 验收：可以放心让外部用户下载、覆盖安装、回滚、升级和排障。

## M6：d-token-native Runtime Innovation

目标：在保持用户体验稳定的前提下，逐步替代 Headroom-derived runtime 中的关键模块。

1. 替换脆弱 CLI/config/dashboard glue。
2. Rust-native route adapter。
3. d-token-native observability ledger。
4. d-token-native provider protocol adapter。
5. d-token-native compression policy engine。
6. 保留必要的 Headroom-compatible API 和迁移层。

M6 验收：用户仍然只下载 d-token App；内部 runtime 可以逐步从 Headroom-derived 迁移为 d-token-native。