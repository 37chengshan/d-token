# dtoken-core-api AGENTS.md

本文件是 `dtoken-core-api` crate 的 per-directory 入口。通用规则以根 `AGENTS.md` 和 `code/crates/AGENTS.md` 为准。

## 1. Crate 职责

核心 API surface 聚合 + MCP gateway + ccr store + compress gateway + surface parity。

## 2. 关键模块

- `src/lib.rs` — crate 入口，~17k 行，聚合多个 crate 能力
- `src/ccr_store.rs` — ccr (compression decision) store
- `src/mcp.rs` — MCP gateway
- `examples/ccr_candidate_probe.rs` — ccr candidate probe 示例
- `examples/surface_parity_server.rs` — surface parity server 示例
- `tests/compress_gateway.rs` — compress gateway 测试
- `tests/surface_parity.rs` — surface parity 测试

## 3. API surface 契约

1. **常量 revision**：
   - `COMPRESSION_DECISION_CONTRACT_REVISION = "dtoken-compression-decision-v1"`
   - `RUST_LIBRARY_IMPLEMENTATION_REVISION = "dtoken-rust-library-v1"`
   - `TYPESCRIPT_SDK_IMPLEMENTATION_REVISION = "dtoken-typescript-sdk-http-v1"`
2. **修改 revision 必须同步**：`code/packages/sdk/` 的版本、`docs/product/IMPLEMENTATION_STATUS.md`。

## 4. MCP gateway 边界

1. **MCP gateway 只暴露 metadata**：不暴露凭据、完整 prompt/response。
2. **tool schema 由 `dtoken-compress::tool_schema` 生成**：不要在这里重新实现。
3. **loopback only**：MCP gateway 默认只绑定 `127.0.0.1`。

## 5. 验证

```bash
cd code/crates/dtoken-core-api
cargo check
cargo clippy --all-targets -- -D warnings
cargo test

# 专项测试
cargo test --test compress_gateway
cargo test --test surface_parity
```

## 6. 依赖关系

- **依赖**：`dtoken-types`、`dtoken-compress`、`dtoken-context-store`、`dtoken-config`、`dtoken-config-patcher`、`dtoken-event-ledger`、`dtoken-doctor`、`dtoken-headroom-adapter`、`dtoken-learning`、`dtoken-preprocessor`、`dtoken-project-policy`、`dtoken-runtime-pipeline`
- **被依赖**：`dtoken-proxy`、`apps/desktop/src-tauri`
- **这是 API surface 根**：修改它的公开 API 可能影响 SDK 和桌面 App
