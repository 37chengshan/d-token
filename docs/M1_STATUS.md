# M1 Status

状态：M1 Scope Implemented / Awaiting Cargo CI Result  
日期：2026-07-01

## 1. M1 完成范围

M1 目标是建立可继续开发的本地 Rust core skeleton，并补齐最新 PRD 要求的 RTK/Headroom adapter、项目级策略、配置安全和事件账本底座。

已完成：

1. Rust workspace：`code/Cargo.toml`。
2. Shared domain types：`dtoken-types`。
3. Config loader skeleton：`dtoken-config`。
4. Route decision skeleton：`dtoken-router`。
5. Compression interface skeleton：`dtoken-compress`。
6. Process manager skeleton：`dtoken-process`。
7. Doctor skeleton：`dtoken-doctor`。
8. Event ledger skeleton：`dtoken-event-ledger`。
9. RTK adapter skeleton：`dtoken-preprocessor`。
10. Headroom adapter skeleton：`dtoken-headroom-adapter`。
11. Project policy skeleton：`dtoken-project-policy`。
12. Config diff / backup / rollback skeleton：`dtoken-config-patcher`。
13. Local API skeleton：`dtoken-core-api`。
14. Proxy/core API binary entrypoint：`dtoken-proxy`。
15. Safe local config example：`code/configs/config.example.json`。
16. Updated repository structure checker：`support/scripts/check_structure.py`。
17. Router decision smoke test。
18. Doctor smoke test。
19. Project recommendation smoke tests。
20. Config patcher smoke tests。
21. Event ledger smoke test。
22. Minimal CI gate：`.github/workflows/ci.yml`。

## 2. 当前 crate 职责

### dtoken-types

统一定义：ProviderProfile、RouteProfile、InstanceProfile、ProjectProfile、ProjectCompressionPolicy、CompressionRecommendation、PreprocessorProfile、RtkStatus、RtkGainReport、HeadroomStatus、HeadroomStats、CcrCacheSummary、LayeredCompressionStage、DtokenEvent、ConfigPatchPlan、RollbackPoint、DoctorReport 等。

### dtoken-config

当前支持：读取 JSON 配置，查找 provider、route、instance。

### dtoken-router

当前支持：按 instance id 和 input protocol 匹配 route，查找 target provider，生成 RouteDecision。

### dtoken-compress

当前支持：compression policy 接口、tokens before/after 占位估算、transforms_applied 占位。

### dtoken-process

当前支持：ProcessManager 类型、instance status skeleton、start/stop placeholder。

### dtoken-doctor

当前支持：local-only bind check、port availability check。

### dtoken-event-ledger

当前支持：内存事件账本、M1 mock event、recent query。M2/M3 可迁移到 JSONL/SQLite。

### dtoken-preprocessor

当前支持：

1. RTK binary/version detection：`rtk --version`。
2. RTK missing 状态。
3. RtkStatus 结构。
4. RtkGainReport M1 mock。
5. PreprocessorProfile from status。

当前不做：真实 `rtk gain/discover/session` parser、hook integrity parser、RTK DB 读取。

### dtoken-headroom-adapter

当前支持：

1. Headroom CLI/version detection：`headroom --version`。
2. Headroom missing 状态。
3. HeadroomStats M1 mock。
4. CCR cache summary mock，不读取原文。

当前不做：真实 proxy/MCP/status parser、stats endpoint import、CCR 原文读取。

### dtoken-project-policy

当前支持：

1. ProjectProfile default。
2. ProjectCompressionPolicy default safe policy。
3. RuleBasedRecommendationEngine v0。
4. high-risk → safe。
5. large/noisy context → aggressive。
6. normal coding → balanced。

### dtoken-config-patcher

当前支持：

1. patch plan。
2. simple diff。
3. stable hash。
4. backup directory creation。
5. stale hash mismatch protection。
6. mock patch plan。

当前不做：复杂 JSON/YAML/TOML merge、shell profile AST patch、真实 rollback UI。

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
- `GET /preprocessors`
- `GET /rtk/status`
- `GET /rtk/gain`
- `GET /rtk/discover`
- `GET /rtk/session`
- `GET /headroom/status`
- `GET /headroom/proxy`
- `GET /headroom/mcp`
- `GET /headroom/stats`
- `GET /headroom/cache-summary`
- `GET /projects`
- `GET /projects/default-policy`
- `POST /projects/recommend-compression`
- `POST /config/patches/plan`
- `GET /config/rollback-points`
- `GET /events`
- `GET /install/scan`
- `POST /install/plan`

### dtoken-proxy

当前支持：CLI 参数 host、port、config；默认监听 `127.0.0.1:18001`；载入 JSON config；启动 local API router。

## 3. 本地运行方式

```bash
cd code
cargo run -p dtoken-proxy -- --config configs/config.example.json
```

然后访问：

```bash
curl http://127.0.0.1:18001/health
curl http://127.0.0.1:18001/stats
curl http://127.0.0.1:18001/rtk/status
curl http://127.0.0.1:18001/headroom/status
curl http://127.0.0.1:18001/projects/default-policy
curl -X POST http://127.0.0.1:18001/projects/recommend-compression
curl -X POST http://127.0.0.1:18001/config/patches/plan
```

## 4. 验证方式

本地：

```bash
cd code
cargo check --workspace
cargo test --workspace
python ../support/scripts/check_structure.py
```

CI：

```text
.github/workflows/ci.yml
  - structure check
  - cargo check --workspace
  - cargo test --workspace
```

说明：本轮已完成文件落库、静态一致性审查和 CI gate 创建，但我没有读取到 CI 运行结果。M1 仍需以 CI 或本地命令全部通过作为 `M1 Verified` 条件。

## 5. M1 不做内容

M1 当前不做：

1. 真实 provider 转发。
2. 真实 streaming proxy。
3. 真实 token compression。
4. 真实 `rtk gain/discover/session` parser。
5. 真实 Headroom proxy/MCP/stats import。
6. SQLite event ledger。
7. Tauri desktop UI。
8. Dashboard cards。
9. sidecar spawn。
10. 配置文件真实修改 UI。

这些属于 M2/M3/M4/M5。

## 6. 下一步建议

优先顺序：

1. 查看 GitHub Actions CI 结果。
2. 修正 cargo check / cargo test 错误。
3. 通过后将状态改为 `M1 Verified / Ready for M2`。
4. 进入 M2：Tauri shell + installer/dashboard mock UI。
