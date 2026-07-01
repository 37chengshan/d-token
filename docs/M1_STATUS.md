# M1 Status

状态：Started / Core Skeleton Created  
日期：2026-07-01

## 1. 本轮完成内容

M1 目标是建立可继续开发的 Rust core skeleton，而不是直接完成真实压缩和完整代理转发。

已完成：

1. Rust workspace：`code/Cargo.toml`。
2. Shared domain types：`dtoken-types`。
3. Config loader skeleton：`dtoken-config`。
4. Route decision skeleton：`dtoken-router`。
5. Compression interface skeleton：`dtoken-compress`。
6. Process manager skeleton：`dtoken-process`。
7. Doctor skeleton：`dtoken-doctor`。
8. Local API skeleton：`dtoken-core-api`。
9. Proxy/core API binary entrypoint：`dtoken-proxy`。
10. Safe local config example：`code/configs/config.example.json`。
11. Updated repository structure checker：`support/scripts/check_structure.py`。
12. Router decision smoke test：`code/crates/dtoken-router/tests/route_decision.rs`。
13. Doctor smoke test：`code/crates/dtoken-doctor/tests/basic.rs`。

## 2. 当前模块职责

### dtoken-types

统一定义：

- ProviderProfile。
- RouteProfile。
- InstanceProfile。
- RouteDecision。
- CompressionResult。
- RequestLog。
- DoctorReport。
- HealthResponse。

### dtoken-config

当前支持：

- 读取 JSON 配置。
- 查找 provider。
- 查找 route。
- 查找 instance。

后续可扩展：

- YAML。
- OS keychain ref。
- config source tracking。

### dtoken-router

当前支持：

- 按 instance id 和 input protocol 匹配 route。
- 查找 target provider。
- 生成 RouteDecision。
- 基础 route decision 测试。

后续可扩展：

- model-aware routing。
- fallback visibility。
- chain bridge status。
- route decision ledger。

### dtoken-compress

当前支持：

- compression policy 接口。
- tokens before/after 占位估算。
- transforms_applied 占位。

后续可扩展：

- Headroom-compatible compression runtime。
- CCR。
- SmartCrusher。
- CodeCompressor。
- output shaping。

### dtoken-process

当前支持：

- ProcessManager 类型。
- instance status skeleton。
- start/stop placeholder。

后续可扩展：

- real sidecar spawn。
- stdout/stderr capture。
- Windows/macOS process cleanup。
- port conflict handling。

### dtoken-doctor

当前支持：

- local-only bind check。
- port availability check。
- 基础 doctor smoke test。

后续可扩展：

- WebView2 check。
- Xcode Command Line Tools check。
- Python runtime check。
- sidecar healthcheck。
- route/provider diagnosis。

### dtoken-core-api

当前支持：

- `GET /health`
- `GET /stats`
- `GET /stats-history`
- `GET /metrics`
- `GET /providers`
- `GET /routes`
- `GET /instances`
- `GET /chain/status`
- `GET /route-decisions/recent`
- `GET /logs/recent`
- `POST /doctor/run`

### dtoken-proxy

当前支持：

- CLI 参数：host、port、config。
- 默认监听 `127.0.0.1:18001`。
- 载入 JSON config。
- 启动 local API router。

## 3. 本地运行方式

```bash
cd code
cargo run -p dtoken-proxy -- --config configs/config.example.json
```

然后访问：

```bash
curl http://127.0.0.1:18001/health
curl http://127.0.0.1:18001/stats
curl http://127.0.0.1:18001/providers
curl -X POST http://127.0.0.1:18001/doctor/run
```

## 4. 本地验证方式

```bash
cd code
cargo check --workspace
cargo test --workspace
python ../support/scripts/check_structure.py
```

说明：当前我已完成文件落库，但没有在远端仓库运行 `cargo check`。下一轮应在本地或 CI 环境执行上述命令并修正编译问题。

## 5. 未完成内容

M1 当前未做：

1. 真实 provider 转发。
2. 真实 streaming proxy。
3. 真实 token compression。
4. Headroom-compatible runtime 调用。
5. SQLite ledger。
6. Tauri desktop UI。
7. Dashboard cards。
8. sidecar spawn。

这些属于 M2/M3/M4。

## 6. 下一步建议

优先顺序：

1. 本地跑 `cargo check --workspace` 修正编译问题。
2. 为 `dtoken-core-api` 增加最小集成测试。
3. 给 `dtoken-router` 增加更多 route decision 单元测试。
4. 给 `dtoken-doctor` 增加端口占用测试。
5. 开始 M2：Dashboard skeleton。
