# d-token Tauri Shell

更新时间：2026-09-06

本目录是 d-token 桌面 App 的 Tauri 2/Rust native 层，已经负责真实 native command、凭据存储、d-token 配置、服务生命周期、Doctor、运行证据和生产打包，不再是 M3 fixture-first 占位壳。

## 当前职责

- 将 React UI 连接到 d-token-owned local state 与 loopback runtime。
- 保存 App 设置和 Provider/Route 草稿。
- 将 Provider credential 写入 macOS Keychain / Windows Credential Manager；普通配置只保存引用。
- 应用 d-token active config，并在写入前创建 rollback point。
- `get_config_change_records` 只读取本机备份、草稿和操作记录，不要求 runtime 在线；`managed-agent-config` 恢复前同时校验备份、工具登记路径与当前文件，拒绝并发修改和符号链接。
- 启动、停止和验证打包的 `dtoken-proxy` Rust external binary。
- 运行安装事务、Doctor、产品 readiness、route 配置验证和脱敏导出。
- 管理代码图谱的用户导入目录、pinned artifact 自动识别、Agent 批量配置事务、五维状态、d-token-owned 索引/取消/删除与短期授权只读图页；Agent-owned MCP 进程只观察、不接管。
- 为 Windows/macOS 生成平台 bundle。

## OpenCode 默认接入（2026-09-07）

安装页使用 `preview_managed_agent_transport` 与 `apply_reviewed_managed_agent_transport`，同时更新专用 provider 和 OpenCode 的 `model` 默认选择；新会话不再需要手动选择 d-token provider。原有 provider、认证和其他配置保留；已有会话显式选择的模型不由默认设置覆盖。

`.<config-name>.dtoken-selection.json` 保存原默认选择及所属连接，权限受限，包含在同一次备份/应用事务中。清除恢复原默认值，写入 `null` 撤销选择记录；失败回退使用 receipt.transaction.changes 中的备份倒序精确恢复。旧的专用 OpenCode command 仍只管理 provider，不能据其成功宣称默认接入已经启用。

## 前端对接补充（2026-09-06）

`get_active_route_config_status` 返回当前文件字节的 `content_sha256`；未创建文件使用空内容摘要，读取失败不提供有效摘要。安装失败恢复原服务前必须核对它与写入前摘要一致，且状态为 `loaded` 或 `missing`；摘要相同不替代 native 启动时的配置、进程和身份验证。

`discover_upstream_candidates` 增加 `agent_catalog`，来自同一 Rust 注册表，包含全部 23 个工具。每项含 `agent_id`、`agent_name`、`installed`、`integration_kind`、`can_configure`、`can_clear`。扫描失败时 `installed=null`，不能当作未安装；未安装/未知项的两个写操作标记均为 `false`。`agent_bindings` 保持原来的已检测事务范围，前端可按 `agent_id` 合并详情；不要把整份 catalog 当成配置请求。

`apply_cc_switch_sync_plan` 与 `set_cc_switch_auto_follow` 都会变更本地状态。调用完成后应刷新共享的连接/runtime 状态；单纯同步连接与授权后续自动跟随是两个独立结果。同步失败可能返回 `rolled_back_*` 或 `*_rollback_failed`，前端应读取 `ok`、`runtime` 和恢复状态，不应只靠 `applied` 字样判定成功。

`apply_hybrid_agent_integration` 保留旧的 aggregate `receipt.transaction_id`，`verify_hybrid_agent_integration` 与 `rollback_hybrid_agent_integration` 仍接受这个 id。新增的 `receipt.state`、`receipt.group_results` 和 `receipt.agent_results` 描述逐 target-group / 逐 Agent 结果；`ok=false,state=partial` 表示至少一个 Agent 或共享 target 失败，但已经成功并验证的独立 Agent 组会保留。共享 Codex/OpenCode `AGENTS.md` 和 `.agents/skills` 失败只影响依赖该共享 target 的 Agent；独占 target 失败只影响对应 Agent。启用 Managed SharedContext MCP 时，失败 Agent 的 capability 会被撤销，成功 Agent 的 capability manifest 继续绑定同一个 aggregate id 用于后续 verify / rollback。

## Debug 与 Release 边界

Debug build 可以注册 OpenCode workload、模型测试和 capability smoke 等内部验证命令。

Release build 不注册这些开发测试命令，只保留面向产品的设置、配置、服务、Doctor、Dashboard、验证和导出能力。

## 当前限制

- 缺失 RTK/Headroom 时不会自动下载，只检测并安全失败。
- d-token route 草稿只写 d-token 自有状态，不等于已修改第三方 Agent 配置。
- 代码图谱已支持自动扫描 OpenCode/Codex/Claude、多选批量 MCP 事务与 Claude Hook；每个目标仍有独立 diff/backup/verify/receipt，其他 Agent/surface 组合保持 fail closed。
- 平台签名、notarization 和公开分发证据不由“能构建 bundle”自动证明。

## 构建

从 `code/apps/desktop/` 执行：

```bash
npm run tauri:dev
npm run tauri:build
```

这两个命令会先运行平台 sidecar 准备脚本，为目标架构构建并放置 `dtoken-proxy`、`dtoken-agent-bridge` 与 `dtoken-codebase-memory-launcher`。大型 headless upstream 不进入 bundle，由用户从页面固定的官方 release 手工导入并由运行时 pinned 校验。不要直接绕过脚本调用 Tauri CLI 生成不完整 bundle。

当前实现状态见 `docs/product/IMPLEMENTATION_STATUS.md`。
