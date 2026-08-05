# 后端与 Tauri/Rust 架构研究

## 1. AiMaMi 后端结构

AiMaMi 使用 Tauri 2 + Rust 提供本机能力。固定源码版本的后端大体可以抽象为：

```text
Tauri command layer
├── repository / local data access
├── process management
├── proxy / relay management
├── account and quota operations
├── Codex config and session operations
└── platform integration
```

这种结构的优点是开发速度快、所有能力都在本地、前端调用直接。缺点是 Repository、Process、Proxy 等中心模块容易持续膨胀。当一个文件同时负责路径发现、格式解析、业务规则、写入、进程和错误翻译时，后续测试与安全审计成本会快速上升。

## 2. d-token 当前后端结构

d-token 的 Rust workspace 已拆为多个业务 crate：

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

生产链路：

```text
AI coding Agent
→ d-token loopback listener
→ dtoken-core-api / dtoken-proxy
→ protocol normalization
→ compression / Hybrid Context Optimization
→ explicit route decision
→ configured upstream
→ redacted Event Ledger / runtime evidence
→ Tauri desktop control plane
```

这种分层比 AiMaMi 更适合多 Agent、多协议和长期演进。但桌面 crate 的 `src-tauri/src/main.rs` 已超过合理的组合根职责，重新形成桌面侧中心模块。

## 3. Tauri main.rs 的问题

当前 `main.rs` 同时包含：

- Tauri command DTO。
- 应用状态路径。
- 原子文件写入。
- Windows/macOS 平台逻辑。
- 凭据存储。
- route/provider draft。
- 服务启动、停止与身份验证。
- Doctor 与 readiness。
- Dashboard 数据聚合。
- install transaction。
- OpenCode adapter command 注册。
- upstream discovery。
- 调试命令。

问题不是“文件长”本身，而是多个变更原因落在同一模块：

- 改 Windows 文件替换会触碰 main。
- 改凭据系统会触碰 main。
- 改路由草稿会触碰 main。
- 改服务生命周期也会触碰 main。

这会增加冲突、审查难度和无关回归。

## 4. 推荐目标目录

```text
src-tauri/src/
├── main.rs
├── app_state.rs
├── commands/
│   ├── mod.rs
│   ├── dashboard.rs
│   ├── doctor.rs
│   ├── routes.rs
│   ├── credentials.rs
│   ├── service.rs
│   ├── install.rs
│   ├── agents.rs
│   └── export.rs
├── services/
│   ├── runtime_service.rs
│   ├── config_service.rs
│   ├── credential_service.rs
│   ├── doctor_service.rs
│   └── evidence_service.rs
├── agents/
│   ├── mod.rs
│   ├── adapter.rs
│   ├── opencode.rs
│   ├── claude_code.rs
│   ├── codex.rs
│   ├── cursor.rs
│   └── cline.rs
├── platform/
│   ├── mod.rs
│   ├── filesystem.rs
│   ├── process.rs
│   ├── credential_store.rs
│   ├── macos.rs
│   └── windows.rs
├── discovery/
│   ├── mod.rs
│   ├── opencode.rs
│   └── cc_switch.rs
└── dto/
    ├── mod.rs
    ├── requests.rs
    └── responses.rs
```

`main.rs` 最终只负责：

1. 创建共享 State。
2. 注册插件和 command。
3. 配置窗口、托盘和单实例。
4. 启动应用。

## 5. Agent Adapter SPI

当前 OpenCode adapter 已是高质量参考实现，应抽象为统一接口。

```rust
trait AgentAdapter {
    fn id(&self) -> AgentKind;
    fn discover(&self) -> Result<AgentDiscovery>;
    fn preview(&self, request: ConnectionRequest) -> Result<ConnectionReview>;
    fn apply(&self, approved: ApprovedReview) -> Result<ConnectionReceipt>;
    fn verify(&self, receipt: &ConnectionReceipt) -> Result<VerificationResult>;
    fn rollback(&self, rollback_id: &str) -> Result<RollbackReceipt>;
    fn doctor(&self) -> Result<Vec<DoctorCheck>>;
}
```

接口必须保证：

- preview 无写入。
- apply 需要 review revision 和双 hash。
- 写入前重新检查目标状态。
- verify 不依赖前端声称。
- rollback 只覆盖仍属于该事务的状态。
- discovery 不读取或返回 secret。
- 每个 adapter 有独立兼容矩阵和 fixture。

## 6. ConfigPatcher 拆分

`dtoken-config-patcher/src/lib.rs` 已形成完整事务能力，但单文件继续增长。建议拆为：

```text
config-patcher/src/
├── lib.rs
├── plan.rs
├── snapshot.rs
├── validation.rs
├── diff.rs
├── backup.rs
├── atomic_write.rs
├── transaction.rs
├── restore.rs
├── permissions.rs
└── platform/
```

边界：

- `plan`：生成 before/after hash 和脱敏 diff。
- `snapshot`：文件存在性、长度、权限、hash。
- `validation`：UTF-8、严格 JSON、重复 key。
- `backup`：private directory、manifest、原始 bytes。
- `atomic_write`：临时文件、fsync、rename/MoveFileEx。
- `transaction`：多目标 all-or-restore。
- `restore`：所有权检查与精确恢复。

## 7. 服务生命周期

d-token 相比 AiMaMi 的关键优势是“进程身份”而不只是 PID。

启动应验证：

- executable path。
- config path。
- data directory。
- listener port。
- health endpoint。
- runtime identity。
- managed manifest。

停止应验证：

- manifest 仍属于 d-token。
- PID 未复用。
- executable 与记录一致。
- listener 仍由同一进程持有。

任何验证失败都应拒绝 kill，并提示用户手工处理。

## 8. Upstream Discovery

当前 `upstream_discovery.rs` 能读取：

- OpenCode 官方模型目录。
- 正常 OpenCode config 的非敏感元数据。
- cc-switch SQLite 的 provider/profile/proxy 信息。

优点：

- 只读模式。
- 敏感字段跳过。
- 新 schema fail closed。
- locked/corrupt 状态明确。
- 不自动选择候选。

风险：

- 依赖 cc-switch 内部表和 JSON 字段。
- 固定 schema version 和 commit 会随上游更新失效。
- SQLite JSON1 行为、锁与平台路径需要兼容测试。

建议把它独立为 versioned adapter：

```text
CcSwitchAdapterV12
CcSwitchAdapterV13
UnsupportedCcSwitchSchema
```

不要在通用 discovery 函数中不断增加字段兼容分支。

## 9. 错误模型

后端不应把所有错误转换为 String。推荐稳定错误枚举：

```rust
ConfigError::ChangedAfterReview
ConfigError::UnsafeTarget
ConfigError::InvalidFormat
ConfigError::BackupFailed
ConfigError::ApplyFailedRestored
ConfigError::ApplyFailedRestoreIncomplete
RouteError::Ambiguous
RouteError::ProviderMismatch
RuntimeError::IdentityMismatch
RuntimeError::PortOwnedByOtherProcess
CredentialError::StoreUnavailable
```

Tauri DTO 中返回：

- code。
- user_message。
- technical_summary。
- recoverability。
- suggested_action。
- evidence id。

前端根据 code 呈现，避免依赖英文字符串判断。

## 10. 并发模型

需要明确四类并发：

1. 多页面同时刷新 runtime evidence。
2. 用户重复点击 apply/start/stop。
3. 外部程序在 review 后修改配置。
4. 多个进程同时写 Event Ledger 或 metadata JSONL。

建议：

- 前端共享一个 Runtime Evidence Provider。
- command 使用 operation id 和幂等语义。
- apply 使用 before hash 拒绝陈旧审查。
- 文件事务使用 per-target lock 或原子所有权检查。
- JSONL 需要跨进程锁、轮转和背压策略。

## 11. 插件与系统权限

AiMaMi 的系统能力完整，但权限较宽。d-token 应按最小权限开启：

- Shell：只允许明确二进制和参数模板。
- Process：只操作已验证 d-token 进程。
- File system：只允许 App data、受支持 Agent config 和用户明确选择路径。
- Updater：只接受签名 manifest 和 HTTPS 来源。
- Global shortcut：可选启用，并可在设置中撤销。
- macOS private API：除非明确必要，不作为默认依赖。

## 12. 测试分层

- crate unit：纯逻辑、错误、序列化、hash、validation。
- fixture integration：临时 HOME、fake binary、配置事务和回退。
- Tauri command：DTO、状态、权限与 service orchestration。
- local E2E：真实 d-token proxy + fake upstream。
- routed Agent：真实 OpenCode/其他 Agent 请求链。
- platform acceptance：macOS/Windows 安装、凭据、进程、回退和签名。

## 13. 后端实施优先级

### P0

1. 拆分 main.rs。
2. 建立 Agent Adapter SPI。
3. 迁移 OpenCode adapter，不改变行为。
4. 用 transaction/evidence service 统一 command。
5. 完成同 commit 平台验收。

### P1

1. 拆分 ConfigPatcher。
2. 引入稳定错误代码。
3. versioned cc-switch adapter。
4. JSONL 跨进程锁、轮转和查询存储。
5. 统一 schema 生成到 TypeScript。

### P2

1. updater、tray、single-instance。
2. 插件权限自动审计。
3. 性能 profiling 和长期运行测试。