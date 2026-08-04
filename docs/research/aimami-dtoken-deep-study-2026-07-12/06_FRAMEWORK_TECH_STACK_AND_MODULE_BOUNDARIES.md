# 框架、技术栈与模块边界

## 1. 为什么选择 Tauri + React + Rust

AiMaMi 与 d-token 都选择 Tauri 2、React/TypeScript 和 Rust，原因相似：

- 需要访问本机配置、进程、凭据和文件系统。
- 需要比 Electron 更小的包体和更低的常驻内存。
- 前端需要成熟组件生态、图表、表单和国际化能力。
- 后端需要内存安全、并发安全和跨平台系统 API。
- 应用需要同时面向 macOS 与 Windows。

这套技术栈适合“桌面控制面”，但前提是 Tauri command 和 capability 设计严格。若前端可以随意执行 Shell 或读写任意文件，Tauri 的安全优势会被抵消。

## 2. AiMaMi 技术栈启示

固定源码显示其围绕 React、Tauri、Rust、本地存储、进程、代理、更新和系统快捷能力构建。值得借鉴：

- 业务页面组件化。
- 懒加载降低首屏成本。
- Rust 处理本地系统能力。
- 前端负责交互和可视化。
- Tauri 插件提供托盘、更新、Shell、Process 和快捷键。

需要规避：

- 过宽 capability。
- command 层直接耦合文件格式。
- Repository 持续吸收所有业务。
- 前端状态与 Native 状态重复推断。

## 3. d-token 当前技术栈

### 前端

- React 18。
- TypeScript 5。
- Vite 6。
- Vitest + Testing Library。
- Playwright。
- Radix UI。
- Tailwind CSS 4。
- `motion/react`。
- Recharts。

### 桌面与系统层

- Tauri 2。
- Rust。
- OS credential store。
- 外部 sidecar `dtoken-proxy`。
- macOS `.app`/DMG 与 Windows NSIS bundle。

### Runtime

- Axum 风格本地 HTTP 服务。
- OpenAI Chat/Responses 和 Anthropic Messages 协议入口。
- Router、Proxy、Compression、Doctor、Ledger 和 ConfigPatcher 多 crate。

## 4. 推荐模块原则

### 4.1 依赖方向

```text
UI components
→ frontend domain hooks/stores
→ generated DTO client
→ Tauri commands / local HTTP API
→ application services
→ domain crates
→ platform adapters
```

禁止反向依赖：

- domain crate 不依赖 Tauri。
- config patcher 不依赖前端 DTO。
- UI 不直接知道文件路径和平台命令。
- Agent adapter 不直接操作 UI state。

### 4.2 Domain 与 Platform 分离

例如“停止服务”包含两个层面：

- Domain：何时允许停止，身份要求是什么。
- Platform：macOS/Windows 如何检查进程和发送信号。

不要把策略和系统调用混在一个函数。

### 4.3 Read Model 与 Write Model 分离

- Read model：Dashboard、Doctor、Route list、Recent events。
- Write model：Draft、Review、Approved operation、Receipt、Rollback。

读取可以容忍 partial/stale；写入必须强一致、显式确认和失败恢复。

## 5. 前端目录建议

```text
src/app/
├── shell/
├── pages/
├── features/
│   ├── runtime/
│   ├── routes/
│   ├── transactions/
│   ├── doctor/
│   ├── analytics/
│   └── settings/
├── components/
│   ├── ui/
│   ├── evidence/
│   └── common/
├── generated/
│   └── native-bindings.ts
├── lib/
│   ├── i18n/
│   ├── formatting/
│   ├── accessibility/
│   └── errors/
└── styles/
```

原则：

- 页面只编排 feature，不实现底层数据访问。
- Native invoke 封装在 generated client。
- Evidence 展示组件统一。
- Dialog、Button、Tabs 等基础组件必须通过可访问性测试。
- 页面级组件按路由懒加载。

## 6. Rust workspace 边界建议

现有 crate 边界总体合理，可进一步明确：

| Crate | 唯一职责 |
|---|---|
| dtoken-types | 跨层稳定类型、枚举和证据模型 |
| dtoken-config | d-token 自有配置解析与默认值 |
| dtoken-router | 纯路由决策，不执行网络请求 |
| dtoken-compress | 纯上下文优化与安全门控 |
| dtoken-process | 进程计划、身份检查和脱敏输出 |
| dtoken-doctor | 检查编排与结果模型 |
| dtoken-event-ledger | 脱敏事件持久化与查询 |
| dtoken-config-patcher | 配置事务和精确回退 |
| dtoken-core-api | 本地 API 与 application orchestration |
| dtoken-proxy | 可执行入口、监听与生命周期 |
| desktop crate | UI command adapter 与平台集成 |

注意：`dtoken-core-api` 不能重新成为所有业务逻辑的容器；它应调用领域 crate。

## 7. Tauri command 设计

### 7.1 Command 粒度

推荐 command 对应用户操作或 read model，而不是低级系统调用：

正确：

- `preview_agent_connection`
- `apply_reviewed_agent_connection`
- `rollback_configuration`
- `get_runtime_snapshot`
- `run_product_doctor`

不推荐：

- `read_file`
- `write_file`
- `execute_command`
- `kill_pid`

### 7.2 输入验证

- DTO 使用严格反序列化。
- 枚举替代任意字符串。
- 端口、路径、URL、model 长度有边界。
- credential 只进入专用 command，不进入普通 route DTO。
- command 重新验证 review/hash，不信任前端。

### 7.3 输出

- 使用版本化 DTO。
- 返回稳定 error code。
- 返回 receipt/evidence id。
- 不返回原始 secret、prompt、response 或完整命令输出。

## 8. Sidecar 设计

`dtoken-proxy` 作为 external binary 的优点：

- 桌面壳崩溃时 runtime 可独立管理。
- 网络服务和窗口线程隔离。
- 可单独测试和复用。

注意：

- sidecar 必须与应用版本和 artifact hash 绑定。
- 启动前验证路径和签名/来源。
- manifest 记录 PID、exe、config、data dir、port、version。
- Tauri build 前自动准备对应 target binary。
- 安装和升级要处理旧进程与旧 sidecar。

## 9. 本地 API 与 Tauri command 的分工

### 本地 HTTP API

适合：

- Agent 请求入口。
- Dashboard read model。
- stats、metrics、history。
- route decision 与事件查询。

### Tauri command

适合：

- OS credential store。
- 安装、配置和回退。
- 进程生命周期。
- 本机路径与系统能力。
- updater、tray、single-instance。

不要让浏览器或第三方进程能够调用危险 Tauri command。

## 10. 数据持久化

### JSON/JSONL

适合：

- 小型设置。
- append-only 脱敏事件。
- 可人工检查的 manifest。

限制：

- 跨进程锁。
- 轮转。
- 损坏隔离。
- 查询性能。

### SQLite

适合：

- 长期请求历史。
- 聚合查询。
- 多条件筛选。
- schema migration。

推荐路线：

- Event Ledger 保留 append-only JSONL 作为审计源。
- 建立 SQLite projection 作为查询索引。
- projection 可从 Ledger 重建。
- 不在 SQLite 中保存 raw prompt/response。

## 11. CI 技术门禁

统一命令建议：

```text
npm run check
```

内部执行：

```text
python structure check
cargo fmt --check
cargo clippy -D warnings
cargo test --workspace
tsc --noEmit
vitest run
vite build
playwright test
secret scan
license/source audit
```

当前 d-token 缺少 `typecheck`，应新增：

```json
"typecheck": "tsc --noEmit",
"check": "npm run typecheck && npm run test:run && npm run build"
```

CI 不应仅依赖 Vite build，因为 Vite 默认不做完整 TypeScript 类型检查。

## 12. 版本与迁移

- 所有持久化 DTO 带 schema version。
- migration 必须可测试、可回滚或可重建。
- Agent adapter 声明支持的上游版本范围。
- cc-switch 等第三方内部 schema 使用固定 adapter。
- Capability snapshot、eval、renderer/font digest 必须绑定版本。
- App、sidecar、runtime config 和 Ledger schema 在发布证据中一起记录。

## 13. 依赖治理

- 锁定 Cargo.lock 和 package lock。
- 定期 audit 但不自动升级破坏性版本。
- Tauri plugin 每项单独评估 capability。
- 动画只使用一套库。
- UI 组件库 wrapper 统一处理 ref、class 和 accessibility。
- 第三方适配保留许可证、固定 commit 和修改声明。

## 14. 框架层缺陷清单

1. 前端缺少显式 typecheck script。
2. Rust → TypeScript 类型未自动生成。
3. Tauri main.rs 过度集中。
4. Agent adapter 未统一。
5. JSONL 长期持久化能力仍不足。
6. updater/single-instance/tray 尚未形成发布级实现。
7. cc-switch 依赖内部 schema。
8. 真平台构建、签名和安装证据不足。

## 15. 目标架构判断标准

达到目标不是“目录拆得更多”，而是：

- 修改一个 Agent 不触碰其他 Agent。
- 修改 Windows 进程实现不改变领域规则。
- 前端 DTO 变化能在编译期报错。
- 所有写操作统一经过 transaction service。
- 所有页面统一消费 evidence envelope。
- 新增 Tauri capability 需要显式安全审查。
- 发布可以由同一 commit 的证据包复现。