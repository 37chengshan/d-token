# ADR-0001：d-token 的产品边界

状态：Accepted  
日期：2026-07-01

## 背景

d-token 容易被误解成三种东西：

1. 简单 API 中转。
2. 单纯 token 压缩库。
3. Headroom 或 Cockpit Tools 的复刻。

但用户真实需要的是：把 AI Coding 工具的 **压缩、路由、provider、cc-switch、实例管理、日志可视化、Windows/macOS 体验** 收到一个本地桌面应用里。

## 决策

d-token 的产品边界定义为：

> 本地优先的 AI Coding 压缩路由桌面 App。

它包含：

- Local proxy。
- Provider adapter。
- Compression engine。
- Route profile。
- Instance manager。
- Desktop dashboard。
- cc-switch bridge。
- 本地日志与用量观测。

它不包含：

- 云端账号托管。
- 远程共享账号池。
- 与平台条款冲突的用途。
- 直接复制其他开源项目代码。

## 影响

- 仓库采用 App + crates + sidecars + docs 的结构。
- CLI 不是主产品入口，只作为开发、调试与高级用户入口。
- Windows/macOS 优先于 Linux。
- 中文体验作为第一优先级。
- Provider 与 route 必须显式配置，避免请求误发。
