# GOALS — v0.0.1 用户目标 / 非目标 / 不变量

- 版本：v0.0.1 发布计划包
- 来源：用户发布前最终迭代目标（`.ai-bridge/release-iteration/GOAL.md` 13 项 + 侧栏专项 + 产品与安全硬边界 + 已知发布阻断 + 完成定义）、`docs/PRD.md` P0.0/P0.0.7/P0.0.10、`docs/PLAN.md` §1C/1D、FINAL_BLUEPRINT §1
- 每条目标映射到 [REQUIREMENTS.md](./REQUIREMENTS.md) 的唯一 requirement ID；映射关系是验收追踪的唯一依据

## 1. 用户目标（13 项，全部 MUST）

| # | 目标 | requirement 映射 |
|---|---|---|
| G1 | 前端不允许大改，只允许小范围打磨；保持当前纯色 shell、视觉风格、动画语言、中文/英文、Reduce Motion 和 Windows/macOS 共用 UI | UI-6, UI-1, UI-2, INV-1 |
| G2 | 允许增加首次使用配置页面/面板：首次无配置时进入；自动发现 Agent/上游/cc-switch/d-token 状态；复用真实配置事务；配置完成后回到产品；右上角保留重复进入入口 | PRODUCT-5, AGENT-3, CONFIG-1 |
| G3 | 日志放在「分析」页的日志子页，不新增一级导航 | PRODUCT-7, UI-5 |
| G4 | 分析日志逐请求展示真实 d-token 信息：时间、Agent、Provider、模型、协议、route、状态、request id、Local Token、Provider Token（有上游 usage 证据才显示）、压缩前后 Token、节省 Token、耗时、transforms、失败分类和脱敏详情/receipt | PRODUCT-3, RUNTIME-5, SECURITY-3 |
| G5 | 设置页允许用户维护已有日志模型的输入/输出价格、币种、计价单位、来源和更新时间；支持编辑、保存、删除；缺价格不能当作 0 | PRODUCT-4, RUNTIME-5 |
| G6 | 只有 Token 与价格证据同时存在时，才能展示压缩前费用、压缩后费用和费用差额；缺证据显示 unavailable/partial/not verified，不能伪造账单 | PRODUCT-4, INV-6 |
| G7 | 首页展示累计节省，并提供四区间：累计、当周、当月、当天；所有值来自统一 runtime/Event Ledger/设置证据；真实 0 与 unavailable 必须区分 | PRODUCT-2, RUNTIME-5, INV-6 |
| G8 | 可以把 Headroom 和另外两个开源项目克隆到本地只读研究目录；必须先确认官方来源、固定版本/commit、许可证、NOTICE 和行为差异；外部仓库不得作为隐式运行依赖或直接复制进生产 | OSS-1, OSS-2, OSS-3, INV-4 |
| G9 | Codebase Memory 升级到官方时间最新发布 `v0.9.1-rc.1`（明确标识 prerelease），固定 tag/commit、平台 archive/binary SHA 与 Sigstore/provenance；不得放宽校验；独立修复用户使用官方 `v0.9.0` 正确 release 目录仍无法配置的 Bug：系统化复现、找到 `package ready` 之后的真实阻塞、先补失败测试，再以临时 Agent 配置完成 preview/apply/verify/rollback 和原始 hash/权限恢复 | OSS-4, OSS-1, CONFIG-2, CONFIG-1 |
| G10 | 视觉压缩 Beta 先研究和计划，再派实现代理进行小范围迭代；不得扩大为普通图片压缩，不得开启 production active，不得把 shadow/metadata-only 说成 verified | OPTICAL-1, OPTICAL-2, OPTICAL-6, INV-11 |
| G11 | macOS 侧栏必须修复点击命中/展开位置偏移与交互性能问题；正式导航图标改为项目自有、当前色、可访问且符合 Reduce Motion 的小猫图标；不重构 shell，不改变 IA | UI-4, UI-6, INV-1 |
| G12 | 测试方式覆盖复制/应用各 Agent 对应配置并复读验证；cc-switch 由主代理操控电脑完成 A→B→A 切换测试；任何用户配置写入必须备份、diff、确认、verify 和精确恢复 | AGENT-6, AGENT-5, CONFIG-1, CONFIG-3 |
| G13 | 最终需要不同分支创建不同 PR，完成审查和必要返工，再合并 `main` | EVID-7 |

## 2. 侧栏专项验收（GOAL 侧栏专项，全部 MUST）

| 项 | 验收 | 映射 |
|---|---|---|
| S1 | collapsed/expanded、窄窗口（920×640）、浅/深色、键盘 focus、hover、点击、tooltip 命中矩形一致 | UI-4 |
| S2 | repeated click 不产生累积动画/effect；Reduce Motion 关闭 transform；页面切换不阻塞 | UI-4, UI-6 |
| S3 | 小猫图标使用产品自有 SVG/path，不引入外部品牌资产，图标不替代文字可访问名称 | UI-4 |
| S4 | 真实 macOS Tauri 窗口至少完成一轮点击坐标、截图和布局复核；浏览器 preview 只能作为补充 | UI-4, EVID-5 |

## 3. 产品与安全硬边界（GOAL 硬边界，违反即返工）

| # | 边界 | 映射 |
|---|---|---|
| B1 | d-token runtime、Tauri command、Event Ledger 是正式 UI 唯一事实源 | RUNTIME-2, INV-1 |
| B2 | 不依赖外部 Headroom 8787；cc-switch 永远只读 | AGENT-5, SECURITY-7, INV-11 |
| B3 | 默认只绑定 `127.0.0.1` | SECURITY-5, INV-2 |
| B4 | API key、token、cookie、Authorization、完整认证文件、prompt/response、源码和大段原始输出不进入普通日志、配置、导出或提交 | SECURITY-3, SECURITY-4, INV-3 |
| B5 | 配置写入：detect/parse → plan → redacted diff → backup → confirm → apply → reread/verify → receipt；失败精确回滚 | CONFIG-1, INV-4 |
| B6 | detected/configured/supported 不得冒充 connected/verified/healthy/active | SECURITY-6, INV-6 |
| B7 | Optical 只允许 shadow/preview；production active 不可由产品 UI 打开 | OPTICAL-1, INV-11 |
| B8 | 发布门 G0–G8 未全部绑定同一候选证据时 `release_ready=false` | EVID-1, EVID-2, INV-13 |

## 4. 已知发布阻断（不得用代码存在掩盖）

| # | 阻断 | 状态（2026-08-08） | 收敛切片 |
|---|---|---|---|
| K1 | G6 PowerShell 门尚未在当前候选完整执行 | 本机无 pwsh，聚合 unavailable | PR-8, N8 |
| K2 | G8 Windows/MSVC 与双平台同 commit 产物证据尚未闭环 | fan-in 契约已修复，实机证据缺失 | PR-7, PR-8, N8 |
| K3 | 正式 macOS Developer ID/notarization/Gatekeeper 与 Windows Authenticode 不具备 → 只能是明确未签名 prerelease/local candidate | 外部凭据 | PR-9, N9（三结论判定） |
| K4 | 真实认证 Provider 的 Keychain/Credential Manager 候选证据仍需核对 | 代码存在，实机候选证据缺失 | PR-8, N7 |
| K5 | Codex 必须有独立真实 routed-flow，不能由配置恢复替代 | 代码链路已实现，acceptance 脚本缺失 | PR-2, N2 |
| K6 | Headroom/LeanCTX/Context Bus/CCR 长期能力不得整体误报完成 | 部分/未迁移 | PR-H*, PR-L*, N1/N3 |

## 5. 非目标（明确不做）

1. 不新增工程化一级页面、不重排八页 IA、不重构 shell（G1/G11 边界）。
2. 不修复用户自行处理的 GitHub PR #6 云端 CI 失败；不围绕 GitHub PR #6 修改 CI 逻辑（独立发布证据脚本/工作流缺口仍须补齐）。
3. 不开启 Optical production active；不扩大视觉压缩为普通图片压缩。
4. 不实现 Agent/Provider/上游/模型/协议/endpoint chooser（任何普通/高级 UI 均禁止）。
5. 不引入外部 Headroom 8787 运行依赖；cc-switch 永远只读。
6. 不把 Windows 实机/签名凭据、OpenCode G7 旧证据、旧 DMG、浏览器 preview 当作最终候选证据。
7. 不复制整份 HybridContextAccounting 进 receipt；不把完整 prompt/response/密钥/绝对路径写入日志、配置、导出或提交。
8. 不注册 benchmark/smoke/test-only commands 到 release build。
9. 本计划包不直接修改 PRD、业务代码、manifest 与版本号（版本统一由切片 V-1 执行，见 [EXECUTION.md](./EXECUTION.md#v-1-version-unify)）。
10. 不把「纳入 v0.0.1 计划」等同于「v0.0.1 必须完成」：release claim boundary 见 [RISKS_DECISIONS.md](./RISKS_DECISIONS.md#1-关键决策)，由用户最终裁决是否阻断。

## 6. 不变量（任何切片不得违反，违反即返工）

沿用 FINAL_BLUEPRINT §1.3，全部编号 INV-1..13：

1. 真实数据只来自 d-token-owned runtime / Tauri command / Event Ledger；浏览器 preview 只证明 UI。
2. 所有本地服务只绑定 `127.0.0.1`；Provider/Route 选择显式、可解释；禁止静默猜测与静默 fallback。
3. 凭据只进操作系统凭据库；配置只保存不透明引用，禁止明文 fallback。
4. 所有真实写操作必须有用户可读摘要、diff、backup、明确确认、verify 与 rollback evidence；失败精确回滚。
5. 进程停止前必须验证 PID、可执行文件、端口所有权与 d-token identity。
6. `detected/configured/supported` 不得冒充 `connected/verified/healthy/active`；无证据显示 unavailable/partial/unknown。
7. Local Token 与 Provider Token 分离；`claimable` 仅在 Provider 计费/usage 证据与归因链完整时为 true。
8. 每个 commit/branch/merge/rebase/push 前必须 `python3 support/scripts/git_operation_audit.py record ...`（AGENTS.md §9），禁止删除或改写既有记录。
9. 每 PR 独立分支 + worktree + PR；主代理审查通过后合并；发布结论必须绑定同一候选 commit 与真实验证证据。
10. Windows/macOS 共享同一 React UI、信息架构与产品语义；平台差异只在凭据/进程/系统集成/打包适配层。
11. cc-switch、配置源、Provider 凭据探测一律只读；Optical 保持 shadow/preview，hard-close 语义不变。
12. UI 只在 1180×780 与 920×640、浅/深色、中/英、Reduce Motion 下可读才算通过；缺 Windows 实机不得写双端完成。
13. 最终候选 evidence 必须机器可验证地绑定 `candidate_commit=C_final` 与同一 `source_digest`；聚合器不得仅按 mtime、文件名或 `status=pass` 接受旧 artifact，任一绑定缺失/不匹配一律 blocked。

## 7. 完成定义（v0.0.1）

见 [README.md](./README.md#3-完成定义definition-of-done)；逐项核对清单见 [VERIFICATION.md](./VERIFICATION.md#7-完成度核对清单) 与 [TASKS.md](./TASKS.md)。
