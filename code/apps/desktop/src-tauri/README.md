# d-token Tauri Shell

更新时间：2026-07-18

本目录是 d-token 桌面 App 的 Tauri 2/Rust native 层，已经负责真实 native command、凭据存储、d-token 配置、服务生命周期、Doctor、运行证据和生产打包，不再是 M3 fixture-first 占位壳。

## 当前职责

- 将 React UI 连接到 d-token-owned local state 与 loopback runtime。
- 保存 App 设置和 Provider/Route 草稿。
- 将 Provider credential 写入 macOS Keychain / Windows Credential Manager；普通配置只保存引用。
- 应用 d-token active config，并在写入前创建 rollback point。
- 启动、停止和验证打包的 `dtoken-proxy` Rust external binary。
- 运行安装事务、Doctor、产品 readiness、route 配置验证和脱敏导出。
- 管理代码图谱的用户导入目录、pinned artifact 自动识别、Agent 批量配置事务、五维状态、d-token-owned 索引/取消/删除与短期授权只读图页；Agent-owned MCP 进程只观察、不接管。
- 为 Windows/macOS 生成平台 bundle。

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
