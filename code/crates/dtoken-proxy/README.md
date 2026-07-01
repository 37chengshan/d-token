# dtoken-proxy

本地 HTTP proxy 模块。

职责：

1. 监听本机 `127.0.0.1:<port>`。
2. 接收 AI Coding 工具的本地请求。
3. 调用 compression interface。
4. 调用 router 做 route decision。
5. 处理 streaming response。
6. 写入 request log 和 metrics。

安全要求：默认只绑定 `127.0.0.1`，不作为公网服务。
