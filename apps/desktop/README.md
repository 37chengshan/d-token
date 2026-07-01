# d-token Desktop App

目标：用 Windows/macOS 可视化 App 取代纯命令行安装与 Web Dashboard。

## 推荐技术栈

- Tauri：跨平台桌面壳与本地系统能力。
- React + TypeScript：前端 UI。
- Vite：开发构建。
- Rust command bridge：调用 d-token proxy/router/compress sidecar。

## 页面规划

1. Onboarding
   - 选择使用场景：Claude 官方、Claude 第三方、Codex 官方、Codex 中转、cc-switch 链路。
   - 检查端口、配置目录、系统权限。

2. Dashboard
   - 当前运行实例。
   - 今日请求量。
   - token before/after。
   - 压缩节省率。
   - 最近错误。

3. Providers
   - 官方 provider。
   - 第三方 provider。
   - cc-switch endpoint。
   - 健康检查。

4. Instances
   - Claude instances。
   - Codex official instance。
   - Codex relay instance。
   - 每个实例的端口、环境变量、日志、启停按钮。

5. Request Logs
   - 请求链路。
   - 模型。
   - provider。
   - latency。
   - tokens。
   - 错误分类。

6. Learn / Rules
   - 从失败会话中提取规则。
   - 应用到本地项目规则文件。

## UX 要求

- 中文优先。
- 所有配置均可导入/导出。
- 每条路由都必须可视化展示：`Agent -> d-token -> Provider` 或 `Agent -> d-token -> cc-switch -> Provider`。
- 错误提示要区分：说明、警告、错误。
