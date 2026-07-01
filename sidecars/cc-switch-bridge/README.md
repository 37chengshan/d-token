# cc-switch-bridge

cc-switch 联动层。

## 定位

cc-switch-bridge 不是 provider，也不负责最终模型选择。它只负责 d-token 与 cc-switch 的本地联动：

```text
Agent -> d-token proxy -> cc-switch endpoint -> final provider
```

## 职责

- 保存 cc-switch endpoint 配置。
- 健康检查 cc-switch。
- 展示二级路由状态。
- 把请求日志标记为 `via_cc_switch`。
- 保证压缩发生在进入 cc-switch 之前。

## v0.4 目标

- 从 UI 创建 cc-switch route profile。
- 显示 d-token 与 cc-switch 的双段连通性。
- 为 Claude/Codex 生成可复制的本地 base URL。
