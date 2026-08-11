# TASKS — v0.0.1 可勾选任务清单

- 状态词：`[ ]` 待执行 / `[~]` 在途（已在分支或已开工）/ `[!]` blocked（外部依赖缺失）/ `[x]` 完成（必须可引证据，见 [EVIDENCE.md](./EVIDENCE.md)）。
- 每个任务列出：requirement 映射、依赖、owner（主代理 Main / 切片 Agent / 验证节点）、output（交付物或证据路径）。
- 本清单是 [REQUIREMENTS.md](./REQUIREMENTS.md) 的追踪层：所有 requirement 都有任务；所有任务都有验收（DoD）与验证节点（[VERIFICATION.md](./VERIFICATION.md)）。

## 1. 基线（W0）

- [~] T-P0 preflight-baseline：核对 git status/diff（含 `code/apps/desktop/src-tauri/src/main.rs` 与 untracked `code/crates/dtoken-compress/examples/_diag_tmp.rs` 裁决），冻结基线并更新冻结 commit（当前 HEAD 事实 `51941806`，仍在变动）。独立分支**尚未创建**：P0 分支为计划名，执行时由主代理新建（不指向现有分支）。REQ：EVID-7（前置）、VERSION-0（基线事实）｜owner：P0 Agent / Main｜output：`.ai-bridge/release-iteration/preflight-report.md`（更新版）。依赖：—。验证：N0 前置。

## 2. 版本统一（V-1）

- [ ] T-V1 版本统一切片：用户确认目标版本 v0.0.1 后，一次性统一 manifest/PRD 版本门/PLAN/IMPLEMENTATION_STATUS/CHANGELOG/release workflow/`check_release_version.py`（`docs/PLAN.md` 与 `docs/product/IMPLEMENTATION_STATUS.md` 的版本修改由主代理合并期统一执行）。REQ：VERSION-0, VERSION-1, DOCS-4｜owner：V-1 Agent / Main｜output：版本检查通过 + 结构检查通过。依赖：T-P0, 全部功能切片（含 T-PR7，workflow 先合）。验证：N0（版本门）。
- [ ] T-V2 版本回归断言：活跃文档 grep 无 `0.2.0-beta.1`（除 history/archive/CHANGELOG 历史条目）。REQ：VERSION-0｜owner：Main｜output：grep 输出。依赖：T-V1。验证：N0。

## 3. 在途收口切片（PR-1..9）

- [~] T-PR1 runtime-receipt：OptimizationReceipt 类型 + RequestLog provider usage 字段 + `mark_upstream_outcome` 投影 + claimable 规则 + 脱敏断言。REQ：PRODUCT-1, RUNTIME-1, RUNTIME-4｜依赖：T-P0｜owner：PR-1 Agent｜output：切片报告 + 单测。验证：N1。
- [~] T-PR2 codex-ocxlocal-acceptance：acceptance 脚本 + 契约测试 + artifact schema（agent_id=codex）。REQ：AGENT-1, RUNTIME-6｜依赖：T-P0｜owner：PR-2 Agent｜output：`support/scripts/run_codex_ocxlocal_routed_acceptance.py` + 契约测试。验证：N2。
- [~] T-PR3 autoconfig-cutover：`needs-source-fix`/`has_resolution_gap` 全链路 + cc-switch 只读暴露 + spec v2.1/PLAN 1A.6 旧口径同步。REQ：AGENT-3, AGENT-5, SECURITY-7｜依赖：T-P0｜owner：PR-3 Agent｜output：cutover diff + grep 无残留输出。验证：N4/N5。
- [~] T-PR4 ui-polish：C1–C8 修复 + badge + 920×640 Playwright 断言 + Reduce Motion。REQ：UI-1, UI-2, UI-3, UI-5, UI-7｜依赖：T-P0｜owner：PR-4 Agent｜output：E2E 通过 + C1–C8 核对表。验证：N6。
- [ ] T-PR5 security-classifier-export：全部 production secret validation path 复用共享 SecretClassifier + export 绝对路径泄漏修复 + 回归。REQ：SECURITY-1, SECURITY-2, SECURITY-3｜依赖：T-PR1｜owner：PR-5 Agent｜output：classifier 覆盖矩阵 + 回归测试。验证：N0/N6。
- [~] T-PR6 config-acceptance：隔离临时根 harness（configure→reread→clear→exact restore；原文件仅 hash/权限；逆序回退）。REQ：AGENT-6, CONFIG-1, CONFIG-2, CONFIG-3｜依赖：T-P0｜owner：PR-6 Agent｜output：`support/scripts/run_agent_config_acceptance.py` + 契约测试。验证：N4。
- [ ] T-PR7 cc-switch-aba + gate wiring：A→B→A artifact harness + 聚合器 `--expected-commit`/`--expected-source-digest` 强绑定 + supplemental evidence + fan-in 接线。REQ：AGENT-5, EVID-1, EVID-3, EVID-6, PLATFORM-3｜依赖：T-PR2, T-PR3, T-PR6｜owner：PR-7 Agent｜output：`run_cc_switch_aba_check.py` + 聚合扩展测试。验证：N5/N9。
- [ ] T-PR8 upgrade-autostart-credential：macOS upgrade/rollback 审计 + LaunchAgent/HKCU + Keychain/Credential Manager 唯一隔离 round-trip + 故障注入。REQ：PLATFORM-1, PLATFORM-2, CONFIG-4｜依赖：T-PR7｜owner：PR-8 Agent｜output：两个新脚本 + 契约测试。验证：N7/N8。
- [ ] T-PR9 verification-orchestration：N0→N1–N8→N9 编排 + 候选变化失效 + 旧 pass artifact 拒绝 + 三结论模板。REQ：EVID-2, EVID-3, PLATFORM-4, DOCS-3｜依赖：T-V1, 全部功能切片｜owner：PR-9 Agent｜output：`run_final_candidate_verification.py` + dry-run。验证：N0–N9 本身。

## 4. Headroom 轨道（PR-H）

- [x] T-H1 headroom-pin：评估 release v0.34.0/`9fd5ae3d` 与 main `7f6950be` 相对旧基线 `01df2452` 的增量（行为/接口/许可证/NOTICE）；唯一 pin 决议 = `v0.34.0`/`9fd5ae3d`；更新 `code/runtimes/headroom/README.md`、`docs/legal/THIRD_PARTY_NOTICES.md`、根 `THIRD_PARTY_NOTICES.md` 及 PLAN/OSS/REQ/EVIDENCE 同步。REQ：OSS-1, CONTEXT-1｜依赖：T-P0｜owner：PR-H1 Agent｜output：评估记录（`code/runtimes/headroom/README.md` §三候选漂移审计与唯一 pin 决议）+ 漂移 D2 闭合。验证：N0（结构/漂移 grep）。证据：worktree `codex/final/headroom-pin`（base `c5a5849`）；三候选 commit（`01df2452`/`9fd5ae3d`/`7f6950be`）在外部克隆 `headroom-main-2026-08-08` 内核验；delta 统计与 NOTICE 差异见 README 决议节；git 操作审计记录 `3079608a-681b-49d1-809e-864561d7cd7c`（branch-create）。
- [ ] T-H2 headroom-matrix：117 项能力矩阵零占位/缺失或显式记录；B0→B6 顺序闭合残余缺口。REQ：CONTEXT-1, CONTEXT-2｜依赖：T-H1, T-PR1｜owner：PR-H2 Agent｜output：矩阵更新 + 切片报告。验证：N1/N3。
- [ ] T-H3 headroom-g9：§8 能力逐项绑定 `C_final`；G9 判定表（G0–G8 全绿后才开放）。REQ：CONTEXT-1, EVID-1｜依赖：T-H2, T-PR7, T-PR9｜owner：PR-H3 Agent｜output：G9 判定表 + 证据绑定。验证：N9。

## 5. LeanCTX 轨道（PR-L，九边界）

- [ ] T-L0 leanctx-pin：v3.9.17 vs main `95a1b7bc` 评估；唯一 pin。REQ：OSS-1｜依赖：T-P0｜owner：PR-L0 Agent｜output：评估记录 + 漂移 D4 闭合。验证：N0。
- [ ] T-L1 leanctx-core：Context Bus + Session Cache + 同 Agent 自命中防护（d-token-owned Rust）。REQ：CONTEXT-1｜依赖：T-L0｜owner：PR-L1 Agent｜output：crate + 测试。验证：N1（契约）。
- [ ] T-L2 leanctx-relay：Context Relay + Query-aware Retrieval + 收益分离计量。REQ：CONTEXT-1, CONTEXT-3｜依赖：T-L1｜owner：PR-L2 Agent｜output：crate + 测试。验证：N1。
- [ ] T-L3 leanctx-recovery：Recovery Pointer/Manifest + Stale/Superseded + SharedContext。REQ：CONTEXT-1｜依赖：T-L2｜owner：PR-L3 Agent｜output：crate + 测试。验证：N1。

## 6. Optical 轨道（PR-O）

- [x] T-O1 pxpipe-pin：v0.12.0/`0ceabbe` 评估 vs 旧 pin `7b7d6efa`；capability matrix 刷新；1928×1928 非生产声明保留。REQ：OPTICAL-3, OSS-1｜依赖：T-P0｜owner：PR-O1 Agent｜output：评估记录 + 漂移 D3 闭合。验证：N0。完成 2026-08-08：唯一 pin `0ceabbed`/v0.12.0（LICENSE 未变、上游无 NOTICE）；法律声明/`PXPIPE_SOURCE_COMMIT`/计划文档已同步；切片 commit 待 Main 串行拣选补录（worktree `final-pxpipe-pin` / 分支 `codex/final/pxpipe-pin`）。
- [ ] T-O2 spleen-renderer：Spleen 2.2.0 声明完整 + 跨平台渲染确定性测试。REQ：OPTICAL-4, OPTICAL-6｜依赖：T-O1｜owner：PR-O2 Agent｜output：确定性测试。验证：N1/N6。
- [ ] T-O3 optical-active-gate：active 不可达断言 + shadow raw ingress 测试 + 门槛文档。REQ：OPTICAL-1, OPTICAL-2, OPTICAL-5｜依赖：T-O2, T-PR1｜owner：PR-O3 Agent｜output：gate 测试 + `docs/optical/release-gates.md` 更新。验证：N6。

## 7. Codebase Memory 轨道（PR-CBM）

- [ ] T-CBM1 notice-drift：根 NOTICE v0.9.0→v0.9.1-rc.1 统一（v0.9.0 标 superseded）。REQ：OSS-1, OSS-4｜依赖：T-P0｜owner：PR-CBM1 Agent｜output：漂移 D1 闭合 + 结构检查。验证：N0。
- [ ] T-CBM2 cbm-windows-fix：v0.9.0 配置 Bug 复现→失败测试→临时 Agent 配置 preview/apply/verify/rollback + 原始 hash/权限恢复；Windows archive/binary SHA + Cosign/OIDC 落证；状态五维不冒充。REQ：PRODUCT-6, OSS-4, CONFIG-1, CONFIG-2｜依赖：T-CBM1, T-PR6｜owner：PR-CBM2 Agent｜output：先红后绿测试 + Windows 证据。验证：N4/N8。

## 8. RTK 轨道（PR-R）

- [ ] T-R1 rtk-pin：v0.45.0/`b34be37` 许可证/NOTICE 核验 + pin 登记；与 PRD「内建能力」对齐。REQ：OSS-1, PLATFORM-5｜依赖：T-P0｜owner：PR-R1 Agent｜output：漂移 D5 闭合 + 登记更新。验证：N0。
- [ ] T-R2 rtk-windows：Windows/WSL managed lifecycle + hook integrity 证据（runner 或 external/blocked）。REQ：PLATFORM-5｜依赖：T-R1｜owner：PR-R2 Agent｜output：runner 报告或 external 标注。验证：N8。
- [ ] T-R3 rtk-savings：savings 分账契约测试；无双计路径。REQ：RUNTIME-5, CONTEXT-3｜依赖：T-R2｜owner：PR-R3 Agent｜output：契约测试。验证：N1。

## 9. AiMaMi 轨道（PR-A）

- [ ] T-A1 aimami-provenance：pin `297c7af5` 与 LICENSE 副本哈希复核；适配范围表与源码注释一致；品牌资产不进入分发复验。REQ：OSS-1, OSS-2｜依赖：T-P0｜owner：PR-A Agent｜output：复核记录。验证：N0（SBOM/license）。

## 10. 真实验证节点（W5–W7，见 [VERIFICATION.md](./VERIFICATION.md)）

- [ ] T-N0 候选全量质量门：structure + Rust fmt/clippy/workspace tests + Vitest/tsc/build/E2E + no-touch smoke + 版本门 + loopback-only 默认绑定断言。REQ：EVID-2, SECURITY-3/4/5/8/9, RUNTIME-3, OSS-2/3｜owner：Main｜output：门禁报告（绑定 `C_final`）。依赖：全部切片合并。
- [ ] T-N1 Receipt 真实验证：真实 routed 请求核对 PRD 最小字段、Local/Provider/claimable、Analytics/export 脱敏。REQ：PRODUCT-1/2/3/4, RUNTIME-2/5, CONTEXT-2, RUNTIME-5, PR-R3｜owner：Main/验收环境。
- [ ] T-N2 Codex routed：`Codex→18001→10100→动态 model` 真实运行 + artifact。REQ：AGENT-1, RUNTIME-6｜owner：Main。
- [ ] T-N3 OpenCode routed：`C_final` 重跑真实 OpenCode；旧 G7 仅历史。REQ：AGENT-2, CONTEXT-2｜owner：Main。
- [ ] T-N4 配置验收：隔离副本 configure→reread→clear→exact restore + 逆序回退 + 多 Agent 批次。REQ：AGENT-4/6/7, CONFIG-1/2/3, SECURITY-6｜owner：Main。
- [ ] T-N5 cc-switch A→B→A：主代理 Computer 真实切换；identity 恢复 + `wrote_cc_switch=false` + ledger。REQ：AGENT-5, SECURITY-7｜owner：Main（Computer）。
- [ ] T-N6 Tauri UI：八页/主题/语言/窄窗/Reduce Motion/native action/重启恢复 + 侧栏 S1–S4 真实窗口复核 + Optical active 不可达。REQ：UI-1..7, PRODUCT-3/5/7, OPTICAL-1/6, EVID-5｜owner：Main（Computer）。
- [ ] T-N7 macOS：安装/升级/卸载/回滚 + autostart + 凭据 round-trip（唯一隔离）。REQ：PLATFORM-1, CONFIG-4｜owner：Main。
- [ ] T-N8 Windows/G6/G8：NSIS/DACL/MSVC/PowerShell G6 + 双平台同 commit fan-in + CBM/RTK Windows 证据；缺则 external/blocked。REQ：PLATFORM-2/3/5, OSS-4｜owner：外部 runner/CI。
- [ ] T-N9 聚合：仅 N1–N8 全结算后，以 `C_final` + source digest 运行 final gate/status；三结论之一落盘 + 发布报告。REQ：EVID-1/2/3/6, PLATFORM-4, DOCS-3｜owner：Main。

## 11. 收口文档（W7 后）

- [ ] T-DOC 发布收口：更新 `docs/PRD.md`（版本门）、`docs/PLAN.md`、`docs/product/IMPLEMENTATION_STATUS.md`、README、CHANGELOG 与最终发布报告；本包 [EVIDENCE.md](./EVIDENCE.md) 滚动同步。REQ：DOCS-3, DOCS-4, EVID-4｜依赖：T-N9｜owner：Main。

## 12. 完整性自查

- 每个 requirement 是否都有任务：见 [REQUIREMENTS.md](./REQUIREMENTS.md) owner 列 → 本文件 T-* 任务；每个任务是否都有验证节点：本文件「验证」列 → [VERIFICATION.md](./VERIFICATION.md)。
- 迁移不漏项核对：用户 GOAL 13 项（G1–G13 映射表）→ 侧栏 S1–S4 → 硬边界 B1–B8 → 阻断 K1–K6 → 九边界 → 七 OSS 轨道 → 全部落任务。
