# 实施路线图

## 1. 总体目标

把 d-token 从“核心能力很强、桌面和发布仍在收口”的开发候选版，推进为：

> 具备 AiMaMi 级桌面完成度，同时保留 d-token 事务安全、显式路由、证据链与多 Agent 扩展能力的本地控制面。

路线必须先消除架构和发布阻塞，再增加新的可见功能。

## 2. 实施原则

1. 不降低现有安全边界换取开发速度。
2. 不同时扩展多个 Agent，先建立 Adapter SPI 和 golden implementation。
3. 不把 browser preview、旧报告或本地估算当成发布证据。
4. 每一阶段有 Definition of Done、测试、回退和文档更新。
5. 架构重构先保持行为一致，再引入新功能。
6. macOS 与 Windows 共享产品语义，平台差异集中在 platform adapter。
7. Optical active 继续默认关闭。

## 3. Phase 0：当前工作树收口

### 目标

把当前大规模未提交工作整理为可审查、可验证的候选基线。

### 工作

- 对当前改动按模块拆分提交：UI shell、OpenCode adapter、upstream discovery、ConfigPatcher、Optical、文档/法律、测试工具。
- 更新 `docs/product/IMPLEMENTATION_STATUS.md`，避免旧测试数量和旧阻塞继续存在。
- 更新 `task_plan.md`、`progress.md` 和 `findings.md`，使其与真实 Git 状态一致。
- 运行完整门禁。

### DoD

- 工作树只有明确记录的待办或完全干净。
- 每个提交有单一主题和验证证据。
- `cargo test --workspace`、Clippy、typecheck、Vitest、build、E2E、structure、secret scan 全部有当前结果。
- 测试 stderr 无 React warning。

## 4. Phase 1：架构拆分

### 1.1 Tauri main.rs

建立 `commands/`、`services/`、`agents/`、`platform/`、`dto/`。

迁移顺序：

1. DTO 和纯 helper。
2. platform filesystem/process。
3. credential service。
4. runtime service。
5. route/config service。
6. Doctor/evidence service。
7. command registration。

每一步保持 tests 通过，不一次性大改。

### 1.2 Agent Adapter SPI

定义统一接口和数据模型：

- discovery。
- preview。
- apply。
- verify。
- rollback。
- doctor。

迁移 OpenCode adapter 作为 golden adapter。

### 1.3 ConfigPatcher

按 snapshot、validation、backup、atomic_write、transaction、restore 拆分。

### DoD

- main.rs 仅负责启动与注册。
- OpenCode 行为无回归。
- 新 adapter fixture 可以复用统一测试合同。
- 事务模块接口稳定，现有安全测试全部通过。

## 5. Phase 2：状态与类型统一

### 工作

- 定义版本化 `EvidenceEnvelope<T>`。
- 定义稳定 error code 与 recoverability。
- 使用 `specta`、`ts-rs` 或 schema generator 生成 TypeScript DTO。
- 移除 `Record<string, any>` 和页面级旧字段 fallback。
- Runtime、Route、Transaction、Doctor、Settings 分领域 store。
- Tauri event 推送 operation progress，snapshot 轮询作为最终一致性。

### DoD

- Rust DTO 变化会导致 TypeScript 编译失败或 bindings 变化。
- 浏览器 preview 与 runtime 状态没有混淆路径。
- stale 数据保留并标记，不清空冒充零。
- 所有页面使用统一 source/freshness/state。

## 6. Phase 3：前端完成度

### 工作

- 连接工作台改为八步任务流。
- Overview 收敛为关键状态、KPI、当前链路、最近事件和待办。
- Doctor 显示证据、影响和安全修复。
- Changes 完整展示 receipt、hash、diff、verify 和 rollback ownership。
- Settings 增加桌面集成和数据保留。
- 修复 Dialog forwardRef。
- 设定 bundle budget，拆分大型 chunk。
- 完成 Reduce Motion、键盘和最小窗口测试。

### DoD

- 新用户不编辑文件即可完成 OpenCode 连接和回退。
- 核心路径键盘可用。
- Vitest stderr 零 warning。
- macOS/Windows 真窗口无溢出和焦点问题。
- Browser preview 不产生真实运行声明。

## 7. Phase 4：多 Agent 接入

### 顺序

1. OpenCode：golden adapter，真实 routed test。
2. Claude Code：Anthropic-native 配置和协议路径。
3. Codex：OpenAI/Codex 配置与会话边界。
4. Cursor/Cline：只做经过验证的最小配置接入。

### 每个 Adapter Gate

- 支持版本范围。
- 只读 discovery。
- strict target validation。
- preview + diff。
- hash-bound apply。
- post-write verify。
- exact rollback。
- concurrent modification tests。
- secret scan。
- real Agent routed test。

### DoD

Compatibility Matrix 只使用：

```text
verified
partial
unsupported
untested
```

禁止因为存在图标或初步 discovery 就标记 supported。

## 8. Phase 5：持久化与长期稳定性

### 工作

- JSONL 跨进程锁。
- 文件轮转和大小/时间保留。
- 坏行隔离和损坏诊断。
- SQLite projection 与索引。
- Ledger 可重建 projection。
- Provider concurrency、timeout、retry、backpressure、circuit breaker。
- streaming bridge matrix。
- crash recovery 与长时间运行测试。

### DoD

- 多进程并发不产生半行或重复 event id。
- 1GB 级历史仍能快速查询。
- projection 删除后可重建。
- Provider 异常分类稳定，无静默 fallback。
- 流式请求成功、失败和中断均有正确 Ledger 状态。

## 9. Phase 6：桌面系统能力

### 工作

- Single-instance。
- Tray。
- Signed updater。
- 可选全局快捷键。
- 启动时运行/LaunchAgent/Windows startup。
- 窗口恢复、主题和语言持久化。

### 安全要求

- 托盘显示 verified 状态。
- 停止服务仍验证进程身份。
- Updater 验签并绑定 sidecar。
- 快捷键默认关闭，可撤销。
- capability 最小化。

## 10. Phase 7：平台发布证据

### macOS

同一候选 commit 绑定：

- `.app` 和 DMG。
- artifact SHA-256。
- code signing/notarization。
- xattr/quarantine。
- Keychain。
- sidecar hash 和 runtime identity。
- OpenCode 真实 route。
- Ledger/export。
- rollback/upgrade。
- 真窗口截图。

### Windows

同一候选 commit 绑定：

- NSIS。
- code signing。
- UTF-8 和非 ASCII 路径。
- `.cmd` shim。
- Credential Manager。
- sidecar/service identity。
- route/rollback/upgrade。
- 100%/125%/150% 缩放 UI。

### DoD

发布报告不是“构建成功”，而是下载、安装、连接、请求、回退、覆盖升级和卸载全链路通过。

## 11. Phase 8：Optical 真实 Gate

只有前述 runtime、ledger 和发布基础稳定后推进。

### 工作

- 固定真实模型和 Provider corpus。
- Fidelity evaluation。
- token/cost/latency measurement。
- cache effect。
- failure injection。
- route-bound signed capability/eval。
- post-commit failure policy。

### DoD

- active 仅对精确 route/model/capability 开启。
- 质量门、经济门和资源门全部通过。
- 失败可在发送前回退。
- UI 不混淆 shadow/estimate/actual。
- 外部用户默认仍处于安全模式，直到 Beta Gate 明确批准。

## 12. 两周执行建议

### 第 1 周

- 收口当前工作树。
- 增加 typecheck/check。
- 修复 Dialog warning。
- 拆 Tauri DTO/commands/platform 第一批。
- 定义 Agent Adapter SPI。
- 迁移 OpenCode adapter。

### 第 2 周

- EvidenceEnvelope + generated bindings。
- 连接工作台步骤化。
- 完成当前 commit OpenCode routed test。
- macOS 真窗口和 unsigned 本地 bundle 证据。
- 制定签名/notarization release checklist。

## 13. 四到八周执行建议

### 第 3–4 周

- Claude Code adapter。
- JSONL lock/rotation。
- SQLite projection。
- single-instance/tray。
- macOS signed candidate。

### 第 5–6 周

- Codex adapter。
- Windows NSIS 实机。
- updater。
- streaming/provider matrix。

### 第 7–8 周

- 双平台 release candidate。
- 隐私/许可证审计。
- crash recovery/benchmark。
- Optical 真实 eval 准备，不默认开启。

## 14. 任务模板

每个阶段任务必须包含：

```text
Goal
User-visible result
Files/modules
Security invariants
Data/evidence source
Failure semantics
Tests
Platform matrix
Rollback
Documentation updates
DoD
```

## 15. 禁止提前完成的检查

任何任务不得仅凭以下情况标记 complete：

- 代码存在。
- 单元测试通过但 E2E 未跑。
- browser preview 看起来正确。
- 历史测试曾通过。
- macOS 通过但 Windows 未验证，或反之。
- 写入成功但 verify/rollback 未验证。
- Optical shadow 有结果但没有 active fidelity 证据。

完成状态必须与当前候选 commit 和验证范围绑定。