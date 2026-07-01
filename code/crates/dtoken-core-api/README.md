# dtoken-core-api

Desktop 调用的本地 API 模块。

职责：

1. 提供 `/health`。
2. 提供 `/stats`。
3. 提供 `/stats-history`。
4. 提供 `/providers`。
5. 提供 `/routes`。
6. 提供 `/instances`。
7. 提供 `/logs/recent`。
8. 提供 `/doctor/run`。
9. 提供 JSON/CSV export。

原则：API 仅服务本机 Desktop 和本地开发工具，默认只绑定 `127.0.0.1`。
