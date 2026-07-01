# M1 Status

状态：M1 No-touch Test Framework Implemented / Awaiting Cargo CI Result  
日期：2026-07-01

## 1. M1 完成范围

M1 目标是建立可继续开发的本地 Rust core skeleton，并补齐最新 PRD 要求的 RTK/Headroom adapter、项目级策略、配置安全、事件账本和 no-touch 测试底座。

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
23. No-touch testing guide：`docs/TESTING_NO_TOUCH.md`。
24. RTK fixtures。
25. Headroom fixtures。
26. Fake RTK binary。
27. Fake Headroom binary。
28. Fixture mode smoke script。
29. Fake-binary mode smoke script。
30. RTK adapter supports `DTOKEN_PROFILE=fixture`。
31. Headroom adapter supports `DTOKEN_PROFILE=fixture`。

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
6. Fixture mode：`DTOKEN_PROFILE=fixture` 时只读 `support/fixtures/rtk/*`，不调用真实 RTK。

当前不做：真实 `rtk gain/discover/session` parser、hook integrity parser、RTK DB 读取。

### dtoken-headroom-adapter

当前支持：

1. Headroom CLI/version detection：`headroom --version`。
2. Headroom missing 状态。
3. HeadroomStats M1 mock。
4. CCR cache summary mock，不读取原文。
5. Fixture mode：`DTOKEN_PROFILE=fixture` 时只读 `support/fixtures/headroom/*`，不调用真实 Headroom。

当前不做：真实 proxy/MCP/status parser、stats endpoint import、CCR 原文读取。

### dtoken-project-policy

当前支持：ProjectProfile default、ProjectCompressionPolicy default safe policy、RuleBasedRecommendationEngine v0。

### dtoken-config-patcher

当前支持：patch plan、simple diff、stable hash、backup directory creation、stale hash mismatch protection、mock patch plan。

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

## 3. No-touch 验证方式

推荐先运行 fixture mode：

```bash
bash support/scripts/run_fixture_tests.sh
```

它会设置：

```bash
DTOKEN_PROFILE=fixture
DTOKEN_FIXTURE_ROOT=support/fixtures
```

并且只读取 fixture 文件，不调用真实 `rtk` 或 `headroom`。

也可以运行 fake-binary mode：

```bash
bash support/scripts/run_fake_binary_tests.sh
```

它会把 `support/fake-bin` 放在 PATH 最前面，并使用临时 `HOME`，因此不会碰真实 agent/RTK/Headroom 配置。

## 4. 普通本地运行方式

```bash
cd code
cargo run -p dtoken-proxy -- --config configs/config.example.json
```

如需完全 no-touch：

```bash
cd code
DTOKEN_PROFILE=fixture DTOKEN_FIXTURE_ROOT=../support/fixtures cargo run -p dtoken-proxy -- --port 38101 --config configs/config.example.json
```

## 5. CI 验证方式

CI：

```text
.github/workflows/ci.yml
  - structure check
  - cargo check --workspace
  - cargo test --workspace
```

说明：本轮已完成文件落库、静态一致性审查、no-touch 测试资产和 CI gate 创建，但我没有读取到 CI 运行结果。M1 仍需以 CI 或本地命令全部通过作为 `M1 Verified` 条件。

## 6. M1 不做内容

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
11. 任何对用户真实 agent/RTK/Headroom 环境的默认触碰。

这些属于 M2/M3/M4/M5，并且必须由用户主动选择。

## 7. 下一步建议

优先顺序：

1. 运行 `bash support/scripts/run_fixture_tests.sh`。
2. 修正 cargo check / cargo test 错误。
3. 运行 `bash support/scripts/run_fake_binary_tests.sh`。
4. 通过后将状态改为 `M1 No-touch Verified`。
5. 进入 M2：真实 external read-only integration，但仍必须由用户主动开启。
