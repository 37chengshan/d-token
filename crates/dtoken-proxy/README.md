# dtoken-proxy

本地 HTTP proxy 内核。

## 职责

- 监听本地端口。
- 接收 Anthropic-compatible / OpenAI-compatible 请求。
- 调用 dtoken-router 获取 route decision。
- 调用 dtoken-compress 执行压缩。
- 转发到官方 provider、第三方 provider 或 cc-switch。
- 写入 request log 与 metrics。

## v0.2 MVP

- `GET /health`
- `POST /v1/messages` Anthropic-compatible passthrough
- `POST /v1/chat/completions` OpenAI-compatible passthrough
- route profile 加载
- provider profile 加载
- safe compression stub
