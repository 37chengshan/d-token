# d-token

> Local-first token compression, routing, and observability control plane for AI coding workflows.

`d-token` 是一个面向 Windows/macOS 的本地桌面控制面，把 AI 编码工具的上下文压缩、上游路由、连接配置、智能学习、问题诊断和变更回退整合到一个 App 中。用户只需下载安装 d-token，不需要先安装 Headroom 或 RTK。

## 第一版能做什么

1. **总体概览**：一眼看到本地服务是否运行、已连接哪些 Agent、cc-switch 是否同步、今日估算节省了多少 token、最近是否有问题。
2. **安装与连接**：4 个上栏——一键安装（服务/端口/Agent/cc-switch 检测与配置计划）、智能学习（全面扫描+排序+受控写入）、高级安装（自定义上游/端口/协议）、日志（安装过程记录）。
3. **分析**：估算节省、请求趋势、Agent/上游/错误分布；明确标注"估算"，不伪装真实账单。
4. **Doctor**：检测已知问题，低风险自动修复，高风险必须确认并预览 diff。
5. **变更与回退**：所有写入（配置、文档、Doctor 修复）都有备份和回退点，可一键恢复。
6. **设置**：主题、语言、端口、脱敏级别、扫描策略、数据保留。

## 当前重点支持 Agent

| Agent | 能力状态 |
|---|---|
| OpenCode | 已可一键配置 |
| Claude Code | 可检测，需用户确认配置 |
| Codex | 可检测，需用户确认配置 |
| Cursor | 可生成配置建议，能写则受控写入 |
| Cline | 可生成配置建议，能写则受控写入 |
| CommandCode | 仅支持高级手动配置（资料待补） |

d-token 只检测和配置这些第三方工具，**不会自动安装它们**。

## cc-switch 只读同步

d-token 默认只读读取 cc-switch 配置：检测当前 profile、读取当前上游，并在用户切换 cc-switch 后同步 d-token 上游或明确提示"不一致"。常规一键安装不会静默改写 cc-switch 配置。

## 智能学习如何扫描和写入

- 全面扫描本机项目目录和 Agent 会话路径，用轻量索引快速排序（不读取完整源代码）。
- 按学习价值评分排序，显示可解释的排序原因。
- 对高价值目录做深度扫描，生成面向 Agent 的项目上下文。
- 直接写入时**只新增或更新 d-token 管理块**，不会改动你的其他内容；写入前自动备份并生成回退点。
- 敏感文件（.env、密钥、认证文件）永远被排除。

## 安全与隐私边界

- 默认只监听 `127.0.0.1`，所有数据在本地处理。
- API key、token、cookie、认证头永不明文展示，日志和导出默认脱敏。
- 完整 prompt、完整 response、完整源代码不落盘、不导出。
- 不上传任何数据到远端，不自动开启遥测。
- 所有真实写入都有 diff、备份、验证和回退。

## Experimental：Hybrid Context Optimization / Optical Preview

仓库正在开发一个默认关闭、只做预览的混合上下文优化控制面。Optical 只是其中一种有损 codec，不等同于整个压缩系统。

当前已完成：

- 版本化 Capability Snapshot、撤销与 supersede 生命周期。
- Anthropic/OpenAI 工具调用依赖图和 fail-closed 边界。
- Raw-byte / Canonical JSON SHA-256 计划证据。
- metadata-only Preview、Capability Lifecycle 和 Recent Events API。

当前明确未完成且禁止宣称可用：

- 图片 renderer、真实请求 patch、active provider routing。
- post-execution validator、rollback executor、history collapse。

当前实现状态和审查证据见 `docs/optical/README.md`；分阶段规划与验证历史见 `support/pending-commits.json`。

## 安装步骤

1. 下载 d-token 安装包（Windows .exe / macOS .dmg）。
2. 运行安装程序。
3. 打开 d-token App。

## 第一次连接步骤

1. 打开"安装与连接"页。
2. 在"一键安装"上栏点击"启动本地服务"。
3. 等待端口检测、Agent 检测、cc-switch 检测完成。
4. 选择一个 Agent，查看配置计划和 diff。
5. 确认应用（已自动备份）。
6. 验证连接是否通过。
7. 如果结果不符合预期，在"变更与回退"中一键恢复。

## 常见问题

- **服务无法启动**：检查端口是否被占用，打开 Doctor 查看。
- **Agent 请求没经过 d-token**：确认 Agent 上游已指向 `127.0.0.1:18001`，在 Doctor 运行连接检查。
- **cc-switch 切换后 d-token 没跟上**：首页会提示"不一致"，点击"同步上游"。
- **智能学习写入失败**：说明检测到非管理块内容可能变化，已中止并保留备份，原文档未改动。

## 如何回退

1. 打开"变更与回退"页。
2. 找到对应的回退点（按时间线排列）。
3. 点击"恢复"。
4. 恢复后自动验证。

## 已知限制

- Windows native 模式通常只能获得部分 hook 能力，WSL 才能获得完整 shell hook 体验。
- CommandCode 的具体命令名、配置路径、hook 能力仍在补齐，暂按高级手动配置处理。
- 公开发布版需要签名/notarization；当前开发构建未签名。
- macOS 签名与 quarantine 提示需要在真实机器上验收。

## 开发

```bash
# 前端开发
cd code/apps/desktop
npm install
npm run dev

# 前端构建（CI gate）
npm run build

# Rust 检查
cd code
cargo check --workspace
cargo test --workspace
```

## 目录结构

```text
docs/       所有正式文档（唯一 PRD、架构、安全边界、发布计划）
code/       所有正式代码（Rust workspace + Tauri/React 桌面 App）
support/    测试、脚本、CI 模板、开发辅助
```

## 设计原则

1. `docs/PRD.md` 是唯一 PRD。
2. 不创建并行 PRD 版本。
3. 不静默修改 agent、RTK、Headroom 或 shell 配置。
4. 不静默开启遥测。
5. 不把 RTK 节省和 Headroom 节省直接相加（需要统一基线）。
6. 不在默认日志中存储完整命令输出、完整 prompt、完整源文件或完整 API 响应。
7. 先只读检测，再受控写入。
8. 每个用户可见的路由/fallback/压缩决策都必须可解释。

## License

License status is not finalized in this repository. If Apache-2.0 upstream code is reused, license notices must be preserved. Until licensing is finalized, treat the repository as a planning and early implementation workspace.
