# 开源参考笔记

> 本文只记录可借鉴的产品/架构方向，不代表复制代码。落地实现必须重新设计并遵守对应开源许可证。

## 1. headroomlabs-ai/headroom

仓库：https://github.com/headroomlabs-ai/headroom

可借鉴点：

- 本地上下文压缩层。
- Proxy 模式：通过本地代理实现零侵入接入。
- MCP 模式：提供压缩、检索、统计工具。
- 可逆压缩缓存：压缩后仍可按 retrieval id 找回原文。
- ContentRouter：按内容类型选择压缩策略。
- `learn` 思路：从失败会话中提取可复用规则。
- token savings 可观测。

需要改进/规避的问题：

- 第三方 provider 不能默认回落官方上游。
- Dashboard 必须能验证请求是否真实走到代理。
- Windows 编码、路径、端口、CLI shim 必须作为一等支持。
- Output token savings 等提示要区分“说明/警告/错误”。

## 2. jlcodes99/cockpit-tools

仓库：https://github.com/jlcodes99/cockpit-tools

可借鉴点：

- 桌面 App 形态优于纯 CLI/Web Dashboard。
- 多账号、多平台、多实例管理。
- 本地 sidecar API。
- 配额、状态、实例、日志可视化。
- Codex 多实例和本地配置投影。
- 多语言界面，中文体验成熟。

需要注意：

- d-token 不做账号运营工具，而是压缩路由工具。
- cockpit-tools 可以作为外部实例/账号来源，也可以只借鉴其 UX 和 sidecar 架构。
- 与外部工具联动时，d-token 只读取必要的本地配置，不接管账号生命周期。

## 3. cc-switch / 二级路由工具

定位：cc-switch 负责模型与上游路由，d-token 负责前置压缩、日志、链路观测。

推荐链路：

```text
Agent -> d-token local proxy -> cc-switch local endpoint -> final provider
```

边界：

- d-token 不替代 cc-switch 的路由规则。
- d-token 只在请求进入 cc-switch 前做压缩和记录。
- UI 中必须显示这是二级链路，避免用户以为请求直达 provider。

## 4. d-token 的差异化

| 方向 | headroom | cockpit-tools | d-token |
|---|---|---|---|
| 核心能力 | 压缩 | 账号/实例管理 | 压缩 + 路由 + 桌面 App + 多实例 |
| 主入口 | CLI/Web/SDK/MCP | 桌面 App | 桌面 App |
| cc-switch 链路 | 非核心 | 非核心 | 核心场景 |
| Claude/Codex 双上游 | 部分支持 | 账号侧支持 | 路由侧一等支持 |
| Windows 细节 | 需要加强 | 较成熟 | 一等支持 |
| 中文体验 | 非核心 | 较成熟 | 一等支持 |
