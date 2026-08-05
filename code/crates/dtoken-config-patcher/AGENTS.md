# dtoken-config-patcher AGENTS.md

本文件是 `dtoken-config-patcher` crate 的 per-directory 入口。通用规则以根 `AGENTS.md` 和 `code/crates/AGENTS.md` 为准。

## 1. Crate 职责

Agent 配置 patch + agent transport + cc-switch fixture 支持。

## 2. 关键模块

- `src/lib.rs` — crate 入口
- `src/agent.rs` — agent 配置 patch 逻辑
- `src/agent_integration.rs` — agent 集成
- `src/agent_transport.rs` — agent transport（GEMINI_AUTH_TYPE_API_KEY 等 auth type 常量）
- `examples/agent_transport_candidate_probe.rs` — agent transport candidate probe 示例
- `tests/agent_bridge_fixture.rs` — agent bridge fixture 测试
- `tests/cc_switch_fixture.rs` — cc-switch fixture 测试

## 3. Auth type 常量（硬约束）

`agent_transport.rs` 中定义的 auth type 常量：

- `GEMINI_AUTH_TYPE_API_KEY = "gemini-api-key"`
- 其他（见源码）

**这些常量是凭据引用的类型标记，不是明文凭据**。修改时必须同步：

1. `dtoken-types::agent` 的 auth type 枚举
2. `support/fixtures/cc-switch/` 的 fixture
3. `docs/TECH_STACK.md` 的 auth type 说明

## 4. cc-switch fixture 边界

1. **fixture 是测试证据**：`support/fixtures/cc-switch/cc-switch.db` 是二进制 fixture，用于验证 cc-switch 配置 patch。
2. **fixture 不含真实凭据**：fixture db 中的 auth 字段是测试占位符，不是真实 key。
3. **修改 fixture 必须同步**：`tests/cc_switch_fixture.rs`、`support/scripts/generate_cc_switch_fixture_db.py`。

## 5. 验证

```bash
cd code/crates/dtoken-config-patcher
cargo check
cargo clippy --all-targets -- -D warnings
cargo test

# 专项测试
cargo test --test agent_bridge_fixture
cargo test --test cc_switch_fixture
```

## 6. 依赖关系

- **依赖**：`dtoken-types`、`dtoken-config`、`dtoken-context-store`（可能）
- **被依赖**：`dtoken-core-api`、`apps/desktop/src-tauri`
- **这是 agent 配置 patch 根**：修改它的公开 API 可能影响 core-api 和桌面 App
