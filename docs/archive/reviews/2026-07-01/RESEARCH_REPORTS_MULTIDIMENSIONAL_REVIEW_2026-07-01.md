# 研究报告多维度审查与扩展计划

日期：2026-07-01  
审查对象：

1. `docs/research/REPORT_01_RTK_HEADROOM_INTERNALS.md`
2. `docs/research/REPORT_02_DESKTOP_INTEGRATION_ONE_CLICK_APP.md`
3. `docs/research/REPORT_03_FEATURES_AND_FIXES_IMPLEMENTATION.md`

## 1. 总结论

原三份报告方向正确，但还不能算“完全全面”。它们已经覆盖了产品主线、分层压缩、桌面集成、一键安装、项目级压缩策略和 M1-M5 路线，但在以下方面深度不足：

1. RTK hook integrity / permission model / trust model。
2. RTK TOML filter pipeline 与 project-local filter trust。
3. RTK analytics 的 read-only 边界与 tracking DB schema。
4. Headroom live-zone-only compression 与安全保证。
5. Headroom MCP 的 local/proxy retrieve 路径、TTL、stats aggregation。
6. Headroom proxy 的 production features：rate limiting、retry、provider fallback、Prometheus metrics、request logging。
7. 一键安装的 transaction model：plan → diff → backup → apply → validate → rollback。
8. 版本管理：version pinning、managed binary provenance、update channel。
9. 跨平台安装风险：macOS quarantine/signing、Windows WSL/native split、Python runtime isolation。
10. 双层 savings 的 event correlation schema。
11. Dashboard 数据契约和 error boundary。
12. 测试矩阵、CI gate、compatibility matrix 的细化。

本轮结论：

```text
报告 01：从“机制说明”扩展为“内部边界 + 数据契约 + 安全机制 + d-token 采纳策略”。
报告 02：从“图形 App 设想”扩展为“一键安装事务系统 + 运行时控制面 + 项目策略产品化”。
报告 03：从“实现清单”扩展为“分阶段工程蓝图 + 数据模型 + 测试/CI/回滚策略”。
```

## 2. 评分

| 报告 | 原评分 | 扩展目标评分 | 主要缺口 |
|---|---:|---:|---|
| 报告 01 | 8.1 | 9.4 | RTK/Headroom 内部边界、MCP、security、analytics、data contract |
| 报告 02 | 8.0 | 9.2 | installer transaction、version/update、platform edge cases、UX states |
| 报告 03 | 8.2 | 9.3 | detailed crates、schemas、tests、CI、compat matrix、migration order |

## 3. 对报告 01 的审查

### 已覆盖

1. RTK 与 Headroom 的层级差异。
2. RTK rewrite pipeline。
3. RTK filter strategy taxonomy。
4. RTK token tracking 与 tee recovery。
5. Headroom 的 CacheAligner、ContentRouter、CCR、memory、learn、output shaper。

### 缺口

1. 没有明确 RTK hook lifecycle management 的边界。
2. 没有写 RTK integrity verification：hash baseline、tamper detection、verify states。
3. 没有写 RTK permission precedence：Deny > Ask > Allow > Default。
4. 没有写 RTK TOML filter 8 阶段 pipeline。
5. 没有写 RTK analytics 只读 tracking DB 的边界。
6. 没有写 Headroom live-zone-only compression：不压老上下文、不破坏 cache hot zone。
7. 没有写 Headroom transform safety rules：never remove human content、parse failure no-op、error preservation。
8. 没有写 MCP 三工具：compress / retrieve / stats。
9. 没有写 Headroom proxy production features。

### 扩展要求

报告 01 必须新增：

1. RTK hook lifecycle and security。
2. RTK core infrastructure and TOML DSL pipeline。
3. RTK analytics boundary。
4. Headroom transform safety model。
5. Headroom MCP data flow。
6. Headroom proxy observability model。
7. d-token 对上述机制的采纳/不采纳边界。

## 4. 对报告 02 的审查

### 已覆盖

1. Install Wizard。
2. Tool Manager。
3. ConfigPatcher。
4. Project Policy Manager。
5. Dashboard IA。
6. Windows/macOS 安装策略。

### 缺口

1. 安装流程还不是 transaction 设计。
2. 没有 version pinning / update channel / provenance。
3. 没有 managed vs external 的权限边界。
4. 没有 shell/agent config patch conflict resolution。
5. 没有 offline/air-gapped 安装策略。
6. 没有 UI state machine 的错误恢复路径。
7. 没有明确 Headroom Python runtime isolation。
8. 没有明确 RTK native Windows 与 WSL 两条安装路径的 UI 文案边界。

### 扩展要求

报告 02 必须新增：

1. Installation transaction model。
2. Managed tool lifecycle：install/update/uninstall/rollback。
3. Version/provenance policy。
4. Conflict resolution strategy。
5. Runtime isolation for Headroom。
6. Cross-platform installation paths。
7. UI empty/loading/error/success states。

## 5. 对报告 03 的审查

### 已覆盖

1. M1A-M5 实现路线。
2. dtoken-preprocessor。
3. dtoken-project-policy。
4. dtoken-headroom-adapter。
5. dtoken-config-patcher。
6. 一键安装向导。
7. 防重复统计。

### 缺口

1. 没有给出 event correlation 的具体 schema。
2. 没有给出 endpoint response contract。
3. 没有给出 config patch atomicity contract。
4. 没有给出 CI workflow 例外路径和 gate 规则。
5. 没有给出 feature flags / staged rollout。
6. 没有给出 failure taxonomy。
7. 没有给出 compatibility matrix 的表结构。
8. 没有给出迁移顺序里的“先模拟、再接真工具、再写入配置”的安全节奏。

### 扩展要求

报告 03 必须新增：

1. `dtoken-event-ledger`。
2. Event correlation schema。
3. Endpoint response contracts。
4. Config patch atomicity contract。
5. Failure taxonomy。
6. Feature flags。
7. CI gate spec。
8. Compatibility matrix schema。

## 6. 是否需要改 PRD

当前 PRD 已经包含一键安装、项目压缩策略、diff/backup/rollback、Doctor、M1-M5 与验收标准。短期不必再次改 PRD。

但如果后续采纳本报告的全部扩展，PRD 可以补充 3 个条目：

1. `dtoken-event-ledger` crate。
2. InstallationTransaction 模型。
3. CompatibilityMatrix 模型。

这三个目前可以先进入报告 03，不急着写入 PRD。

## 7. 最终判断

原三份报告不算差，但不是最终版。

本轮扩展后，目标是让它们能直接指导：

1. M1B-M1E 代码实现。
2. M2 Desktop UI。
3. M3 真实 RTK/Headroom 集成。
4. M4 savings attribution 与推荐等级。
5. M5 生产级安装、CI、兼容性测试。
