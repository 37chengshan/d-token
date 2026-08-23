# VERIFICATION — v0.0.1 验证与发布门

- 规则：**证据只对精确候选 commit 有效**。最终候选 `C_final` 冻结后，任何机器消费的证据必须匹配 `C_final` + 同一 `source_digest`；修复产生新 commit 后旧证据全部失效，必须重跑 N0 并重做受影响节点。
- 聚合器：`support/scripts/generate_release_gate_status.py`（fail-closed：任一 gate 非 passed 即 exit 1；`--report-only` 仅供诊断）；PR-7 扩展 candidate binding 与 supplemental evidence，PR-9 负责 DAG 编排。
- 过期证据隔离清单见 [EVIDENCE.md](./EVIDENCE.md#2-过期证据隔离清单)。

## 1. N0 — 候选全量质量门（W5，所有切片合并后）

命令集（全部必须通过；任一失败 → blocked，修复后完整重跑）：

1. `python3 support/scripts/check_structure.py`（含 v0.0.1 包完整性 + 活跃链接）
2. `python3 support/scripts/check_release_version.py`（V-1 合并后 = v0.0.1）
3. Rust：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`（2026-08-07 参考基线：936 pass/5 ignored，绑定 `C_final` 重跑）
4. 前端：`npx tsc --noEmit`、Vitest（参考 634 pass）、`npm run build`、Playwright Chromium+WebKit E2E（参考各 13 pass）
5. no-touch：`support/scripts/run_fixture_tests.sh` + `run_fake_binary_tests.sh`（Windows 对应 `.ps1` 由 runner 执行）
6. 安全 grep 断言：无 `sk-` 明文写日志/配置/导出、无绝对 config path 进 export、无 raw prompt/response/Authorization、cc-switch 只读、无 chooser 残留
7. **loopback-only 默认绑定断言（RUNTIME-3 / SECURITY-5）**：出现非 127.0.0.1 默认绑定即 blocked。
   - 现有单测入口：`cargo test -p dtoken-proxy loopback_host_validation`（`parse_loopback_host` 只接受 `127.0.0.1`/`::1`，非 loopback 一律 fail-closed；`dtoken-proxy --host` 默认值 `127.0.0.1`，绑定路径 `parse_loopback_host(&cli.host)` → `TcpListener::bind`）
   - 非回环 bind grep（命中即失败）：
     ```bash
     if grep -rInE --include='*.rs' -E 'bind\([^)]*"0\.0\.0\.0"|bind\([^)]*"::"' code/crates code/apps/desktop/src-tauri; then
       echo "non-loopback default bind found" >&2; exit 1
     else
       echo "loopback-only default bind OK"
     fi
     ```
8. 版本门：活跃文档无 `0.2.0-beta.1`；旧版本号仅允许出现在明确标注为 V-1 前历史快照、版本迁移说明或历史比较链接的语境，manifest/锁文件/workflow 与活跃当前事实断言不得残留
9. OSS 漂移 grep：全仓 pin 唯一（[OSS_MIGRATION.md](./OSS_MIGRATION.md) 漂移 D1–D5 全闭合）

输出：`support/artifacts/release-gate-status-<ts>.json`（绑定 `C_final`）+ N0 报告。

## 2. G0–G9 门禁（PLAN §1C.2 语义，禁止漂移）

| 门 | 语义 | 当前事实（2026-08-08） | 收敛方式 |
|---|---|---|---|
| G0 Truth | release 无 fixture 默认、无测试身份、无自报成功；capability registry 唯一状态源 | 需在最新全仓门禁复证 | N0 |
| G1 Contracts | RequestScope/pipeline 状态/CCR 事务/store 迁移/Agent adapter 契约测试 | 需复证 | N0 |
| G2 Codec | 九类 production codec release probe 全绿（evidence id 由 registry 投影） | 本地 probe 已全绿；未绑定 `C_final` | N0 + N1 |
| G3 CCR/ACL | CCR lifecycle journal/reconciliation、Unix 权限、Windows DACL | 实现完成；Windows MSVC 实测缺失 | N8 |
| G4 Surface parity | Rust/HTTP/MCP/TS parity；Responses fail-closed/stream terminal | 已通过（历史） | N0 重跑 + N1 |
| G5 Agent/cc-switch/RTK | 检测矩阵、只读 cc-switch、managed RTK 本机证据 | 部分推进；RTK v0.45.0 pin 待审计 | N4/N5/N7/N8 |
| G6 Ops | metrics/history/cost/redaction/Doctor/performance/security/SBOM/license + PowerShell `release-verification-*.json` | 本机无 pwsh → unavailable/blocked | N8（external/blocked 或 runner 通过） |
| G7 Real OpenCode | 正常配置真实 OpenCode 全链路 + 精确恢复不泄密 | 2026-08-04 candidate_id=G7 已通过（历史）；**必须 `C_final` 重跑** | N3 |
| G8 Candidate | fmt/clippy/test/audit/build；macOS/Windows 同 commit；hash/SBOM/NOTICE/限制绑定 manifest；fan-in ok=true | fan-in 契约已修复；双平台同 commit 证据缺失 | N8 |
| G9 Frontend completeness | 仅 G0–G8 全绿后开放；完整 Headroom 迁移前端闭环候选验收；不等同冻结 UI 安全修复 | 未开放（依赖 G0–G8） | PR-H3, N9 |

`generate_release_gate_status.py` 只聚合 G0–G8；G9 为 PR-H3 的判定表，禁止把 Codex/config/cc-switch supplemental evidence 冒充 G7 或改写既有门语义。

## 3. 两个 Agent relay（N2 / N3）

| 节点 | 链路 | 必须证据 | 不可替代 |
|---|---|---|---|
| N2 Codex routed | `Codex → d-token:18001/v1 → ocxlocal:10100/v1 → 动态模型上游` | artifact：agent_id=codex、provider/model/protocol、route verified、请求匹配数、Event Ledger/metrics 计数、失败分类、恢复状态、隐私断言；Responses 协议、无 OpenAI 认证语义、model 透传（不固定安装时值） | 配置恢复、MCP/Hook 接入证明、静态代码 |
| N3 OpenCode routed | `OpenCode → d-token 127.0.0.1:18001 → 上游`（`C_final` 重跑） | 真实请求全 pipeline + Ledger/metrics/CCR + 精确恢复 + 不泄密；失败分类 | 2026-08-04 旧 G7、旧 DMG、browser preview |

两个 relay 都必须在最终候选上重新执行并绑定 `C_final`；任一缺失或失败 → blocked。

## 4. 能力真实验证节点（N1–N8 并行）

| 节点 | 覆盖 requirement | 证据与通过标准 | owner |
|---|---|---|---|
| N1 Receipt | PRODUCT-1/2/3/4, RUNTIME-1/2/5, CONTEXT-2, OPTICAL-4, PR-R3 | 真实 routed 请求核对 PRD 最小字段；Local/Provider/claimable 分离；Analytics/export 脱敏；四区间数值同源；费用缺证据显示 unavailable；CCR lifecycle 全链路；savings 分账无双计 | Main |
| N2 Codex | AGENT-1, RUNTIME-6 | 见 §3 | Main |
| N3 OpenCode | AGENT-2, CONTEXT-2 | 见 §3；G7 语义 | Main |
| N4 配置验收 | AGENT-4/6/7, CONFIG-1/2/3, SECURITY-6, PR-CBM2 | 隔离副本 configure→reread→clear→exact restore；原文件 hash/权限；整批逆序回退；MCP/Hook 不冒充 Active route；CBM v0.9.0 Bug 修复路径 | Main |
| N5 cc-switch | AGENT-5, SECURITY-7 | 主代理 Computer 执行 A→B→A；route identity 前后相同；`wrote_cc_switch=false`；ledger events；截图/时间戳证据 | Main（Computer） |
| N6 Tauri UI | UI-1..7, PRODUCT-3/5/7, OPTICAL-1/6, EVID-5 | 真实 Tauri 窗口：八页、1180×780/920×640、浅/深、中/英、Reduce Motion、native action、重启恢复；侧栏 S1–S4；Optical active 无开关、shadow 行为；Analytics 日志高度无 diff；browser preview 只作补充 | Main（Computer） |
| N7 macOS | PLATFORM-1, CONFIG-4 | 同一候选：DMG 安装→升级→卸载/回滚（hash/权限/LaunchServices）；LaunchAgent round-trip；Keychain 唯一隔离 set/get/delete；RTK macOS managed probe（v0.45.0） | Main |
| N8 Windows/G6/G8 | PLATFORM-2/3/5, OSS-4, PR-CBM2, PR-R2 | Windows 产物、DACL、NSIS、PowerShell G6、同 commit fan-in、CBM Windows SHA/Cosign、RTK Windows/WSL；无法取得 → 显式 external/blocked，不得冒充 | 外部 runner/CI |

## 5. Optical 验证（shadow/preview 边界）

1. UI 断言：桌面视觉压缩（Beta）页无 production active 开关；`allow_active=false` 强制提示存在；`/compression/optical/status|capabilities|lifecycle|events/recent` 与 `POST /preview` 只读 summary。
2. 行为断言：active 默认空且关闭；启用记录必须绑定 exact provider/route/model revision、renderer/font/profile hash、版本化 token 公式、eval digest、可信 authority；wildcard/未知模型拒绝 active。
3. 数据断言：shadow 保留 raw ingress bytes；任何持久化/日志/导出无 raw prompt/response/PNG/base64/secret；post-commit 永不自动原文重试。
4. 不扩大断言：无普通图片压缩入口（OPTICAL-2）。
5. 参考轨：pxpipe/Spleen 只作评测配方与字体数据（PR-O1/O2），renderer 为 d-token 独立实现。

## 6. 三种发布结论判定（fail-closed，N9）

最终候选结论只允许三种，均由同一 `C_final` + `source_digest` 的证据驱动：

| 结论 | 必要条件（全部满足） | 缺什么 |
|---|---|---|
| **正式签名发布** | 同一候选 commit 的 G0–G8 全 passed；macOS Developer ID + notarization + Gatekeeper 证据；Windows Authenticode 签名证据；双平台同 commit fan-in artifact ok=true；版本 = v0.0.1 | macOS/Windows 签名凭据为外部证据 |
| **未签名技术预览（明确标注）** | 同一候选 commit 的仓库可执行门通过；macOS/Windows artifact contract 与同 commit fan-in 证据完整；产物明确标 `unsigned`/`unnotarized` | 必须保持 `distribution_ready_public=false`、`release_ready=false`；不得写成 G0–G8 全 passed 或 release-ready |
| **不可发布（blocked）** | 上述任一关键证据缺席或失败（receipt、codex routed、cc-switch A→B→A、配置验收、UI 真实验收、upgrade/rollback、G0–G8 任一非 passed、VERSION-0 未闭合） | 缺项在最终报告中逐条列出，禁止用代码存在掩盖 |

判定执行：`generate_release_gate_status.py` 聚合（含 PR-7 的 supplemental required evidence）结果 `release_ready=false` 时，任何文案/README/PR 不得写「可发布」；聚合器强校验 `expected_commit=C_final` 与 `expected_source_digest`，不得只取目录中「最新」文件。

## 7. 完成度核对清单（W7 主代理逐项）

- [ ] 版本 = v0.0.1（VERSION-0 闭合），活跃文档无旧版本号（仅允许明确标注为 V-1 前历史快照、版本迁移说明或历史比较链接的保留）
- [ ] N0 在 `C_final` 完整通过；任何修复后完整重跑
- [ ] N1–N8 全部绑定同一 `C_final` + `source_digest`（含 OpenCode 重跑与 Windows/G6/G8 外部节点）
- [ ] 两个 Agent relay（N2 Codex / N3 OpenCode）真实证据在仓
- [ ] G0–G9 判定表落盘；G7 语义未被 supplemental evidence 改写；G9 仅在 G0–G8 全绿后开放
- [ ] Optical：active 不可达、shadow 行为、无普通图片压缩（N6）
- [ ] RTK：v0.45.0 pin、macOS probe、Windows/WSL 证据或 external（N7/N8）
- [ ] Windows/macOS artifact contract + 同 commit fan-in；签名三结论之一落盘
- [ ] cc-switch A→B→A 由主代理 Computer 完成（N5）
- [ ] 配置验收与 CBM v0.9.0 Bug 修复路径（N4）
- [ ] 过期证据隔离清单生效；旧 pass artifact 不可穿透（EVID-1）
- [ ] 切片报告全部落盘且含未验证声明；git-operations.json 审计记录完整
- [ ] 发布收口文档（PRD 版本门/PLAN/IMPLEMENTATION_STATUS/README/CHANGELOG/发布报告）与 `C_final` 一致（DOCS-3）

## 8. 证据绑定规则（机器可验证）

1. 每个证据 artifact 必须携带 `candidate_commit` 与 `source_digest` 字段（必要时含 dirty state/platform）。
2. 聚合器逐 artifact 校验；任一缺失/不匹配 → blocked，即使 `status=pass`。
3. 过期清单（[EVIDENCE.md](./EVIDENCE.md#2)）内的 artifact 禁止写入最终证据表。
4. 截图证据由主代理 Computer 操作产生并附时间戳/窗口信息；实现 Agent 不得代拍。
