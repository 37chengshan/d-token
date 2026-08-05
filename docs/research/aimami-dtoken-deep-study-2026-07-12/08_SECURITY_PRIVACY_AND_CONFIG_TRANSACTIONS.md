# 安全、隐私与配置事务

## 1. 威胁模型

AiMaMi 和 d-token 都是高权限桌面应用，能够接触开发者的本地配置、进程、模型 Provider 和凭据引用。主要风险包括：

- 前端注入后调用高权限 Tauri command。
- Shell/Process capability 被滥用。
- 误读或泄露 API key、token、cookie、Authorization。
- 配置覆盖导致 Agent 无法启动。
- PID reuse 或端口误判导致误杀其他进程。
- 本地服务绑定非 loopback，被局域网访问。
- 静默 fallback 到错误 Provider，造成成本或隐私风险。
- 日志、导出、截图中泄露 prompt、response、代码或 secret。
- 第三方配置 schema 改变，应用仍按旧格式写入。
- updater 或 sidecar 被替换。

## 2. AiMaMi 的安全关注点

AiMaMi 的系统能力较完整，包含 Shell、Process、Global Shortcut、Updater 和平台扩展。优点是桌面体验成熟，风险是 capability 面积较大。

需要特别关注：

1. CSP 不应长期关闭。若确实因开发阶段关闭，应在 release 强制收紧。
2. Shell 只能允许固定程序和参数模板，不能接受任意用户字符串拼接。
3. Process 管理必须验证身份，不应只依赖 PID。
4. 对 `~/.codex` 的写入必须最小化，并保留用户未知字段。
5. 账号、额度和会话数据的缓存需有权限、保留期限和清理机制。
6. Updater 必须校验签名和发布来源。
7. macOS private API 的使用需要明确必要性和审核影响。

## 3. d-token 已有安全优势

### 3.1 Loopback 边界

本地服务默认绑定 `127.0.0.1`，并检查 Host/Origin，避免成为局域网代理入口。

### 3.2 凭据隔离

- API key 进入操作系统凭据库。
- 普通配置只保存 `keychain://` 或 `os-credential://` 等不透明引用。
- route draft 拒绝明文 credential ref。
- runtime metadata API 不返回真实凭据。

### 3.3 显式路由

- Route/Provider/Instance 必须明确。
- 歧义返回冲突。
- 上游指向同一 d-token listener 时拒绝，防止自环。
- 401/403/429/5xx 应归类，不允许静默回到官方。

### 3.4 配置事务

`ConfigPatcher` 已覆盖：

- 目标路径规范化。
- symlink 拒绝。
- 严格 JSON 和重复 key 拒绝。
- before/after SHA-256。
- 脱敏 diff。
- 私有 backup 目录。
- manifest 与原始 bytes 绑定。
- 权限快照。
- 原子替换。
- 并发修改检测。
- 精确恢复。
- 多目标事务失败恢复。

### 3.5 进程身份

停止服务前检查 PID、exe、port、manifest 和 d-token identity，减少 PID reuse 和误杀风险。

### 3.6 日志与证据

普通日志、Ledger 和导出不应保存：

- 完整 prompt/response。
- 源代码。
- auth 文件。
- API key/token/cookie。
- 大段原始命令输出。

## 4. OpenCode adapter 安全链

当前 adapter 的关键设计：

1. 发现正常 `opencode.json`，拒绝 `auth.json`。
2. 拒绝 JSONC 写入，避免不安全重序列化。
3. 拒绝 symlink 或非普通文件。
4. 只添加 d-token provider，不修改已有 Provider。
5. 若 provider 已存在，要求用户去 Changes 验证或回退。
6. 生成 before/after hash 和 diff。
7. revision 绑定 model、port 和双 hash。
8. apply 前重新 prepare 并核对审查结果。
9. durable backup 后原子应用。
10. 应用后重新解析并验证 baseURL 和 model。
11. verify 失败自动恢复原始 bytes。

这是应推广到其他 Agent 的标准，而不是 OpenCode 特例。

## 5. 配置事务的安全不变量

### 不变量 1：未确认不写

Preview、scan、discovery 和 Doctor 默认只读。任何真实修改必须由明确用户动作触发。

### 不变量 2：审查内容与应用内容一致

`reviewed_before_sha256`、`reviewed_after_sha256` 和 revision 必须与 apply 时重新生成的计划一致。

### 不变量 3：外部修改不覆盖

review 后目标发生变化，apply 必须拒绝。apply 后目标又被外部修改，rollback 也必须拒绝覆盖。

### 不变量 4：恢复精确原始状态

恢复不仅是“写回 JSON”，而是恢复：

- 原始 bytes。
- 文件是否存在。
- Unix mode/readonly 状态。
- 目录同步。

### 不变量 5：失败可解释

区分：

- backup_failed。
- apply_failed_restored。
- apply_failed_restore_incomplete。
- verify_failed_restored。
- rollback_refused_external_change。

## 6. Tauri Capability 设计

建议将 capability 按窗口和功能拆分：

```text
core-window-read.capability
config-transaction.capability
credential-store.capability
service-control.capability
updater.capability
```

原则：

- 默认窗口不具备任意 Shell。
- 开发命令仅 debug 注册。
- release 不注册 benchmark、smoke 和模型测试 command。
- 每个 command 仍做后端输入验证，capability 不是唯一防线。
- capability 文件进入 CI 审计。

## 7. CSP 与前端供应链

Release 建议：

- 设置严格 CSP，限制 `default-src`、`script-src`、`connect-src`、`img-src`。
- 本地 HTTP 只允许 d-token loopback 端口。
- 禁止远程 script 和任意 eval。
- updater 与外部链接分开处理。
- 锁定 npm 依赖并运行 audit。
- UI wrapper 统一，减少第三方组件直接散落。

## 8. Updater 与发布安全

- Release manifest 签名。
- App bundle、sidecar 和配置 schema 版本绑定。
- macOS code signing + notarization。
- Windows code signing。
- 发布记录 artifact SHA-256。
- 安装后验证实际运行二进制 hash。
- 不允许 updater 静默降级到未签名版本。
- rollback 包也必须有来源和版本证据。

## 9. 第三方适配与许可证

本地仓库已建立 AiMaMi Apache-2.0 适配声明：

- 固定上游 commit。
- 保留许可证副本。
- 列出适配文件和修改边界。
- 明确未复制名称、Logo、图标、截图、角色、声音和 Type4Me 资产。

后续要求：

- 继续复用适配代码时更新来源表。
- 独立实现也保留“参考原则”与“代码复制”区别。
- 不把 Apache-2.0 理解为商标授权。
- 发布包包含所需 license/notice。

## 10. 隐私设计

### 默认最小收集

- 不上传 tracking DB。
- 不上传 prompt/response。
- 不上传源代码和命令输出。
- 不在普通日志保存 Provider secret。

### 数据保留

建议设置：

- request metadata：默认 30 天，可配置。
- transaction/rollback：用户明确删除前保留，但显示占用。
- debug logs：短期轮转。
- exported files：由用户选择位置，导出前二次脱敏检查。

### UI 透明度

Settings 应显示：

- 保存了哪些数据。
- 保存在哪里。
- 保留多久。
- 如何清理。
- 哪些信息永不保存。

## 11. Event Ledger 安全

- append-only metadata。
- 每条事件有 schema version。
- 限制字段长度。
- 写入前统一 redaction。
- 读取旧/损坏数据时再次 redaction。
- 尾部损坏隔离。
- 跨进程锁和轮转。
- 导出时不信任历史事件已安全，重新过滤。

## 12. Doctor 安全检查

建议至少包括：

- listener 是否 loopback。
- PID/exe/port identity。
- active config 是否有自环。
- credential ref 是否为 OS store 引用。
- backup 目录权限。
- rollback manifest 完整性。
- Agent config 是否 symlink。
- macOS quarantine/xattr、Keychain、LaunchAgent。
- Windows path/UTF-8/ACL/`.cmd` shim。
- App/sidecar version 与 hash。
- Tauri capability 和 CSP release 配置。

## 13. 安全测试

- secret corpus 扫描日志、Ledger、导出和错误。
- symlink/path traversal。
- TOCTOU：review 后替换文件。
- PID reuse 和端口抢占。
- malicious local HTTP Host/Origin。
- 自环 route。
- malformed config、duplicate key、超大文件。
- corrupt backup manifest/content。
- updater signature failure。
- XSS 后尝试调用 Native command。
- Windows/macOS 权限和路径异常。

## 14. 当前缺陷

1. JSONL 跨进程锁、轮转和查询持久化仍需硬化。
2. 发布级 CSP/capability 审计证据不足。
3. macOS/Windows 签名、安装后 hash 与真实运行证据尚未闭环。
4. 第三方 Agent 配置事务矩阵尚未完成。
5. cc-switch 读取依赖内部 schema，需要隔离。
6. Optical active 的真实 Provider 安全和 fidelity 证据不足。
7. 前端动态 DTO 可能掩盖缺失字段，应改为强类型。

## 15. 发布前安全 Gate

只有全部满足才能称稳定 Beta：

- 所有真实写操作可审查、备份、验证、回退。
- secret scan 全绿。
- route 无静默 fallback。
- release CSP 和 capability 通过审查。
- macOS/Windows 签名安装通过。
- sidecar hash 与应用版本绑定。
- 真实 routed Agent 证据与候选 commit 绑定。
- 日志、Ledger、导出经过隐私审计。
- Optical active 仍关闭，或已通过完整真实模型 gate。