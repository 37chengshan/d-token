# dtoken-compress AGENTS.md

本文件是 `dtoken-compress` crate 的 per-directory 入口。通用规则以根 `AGENTS.md` 和 `code/crates/AGENTS.md` 为准。

## 1. Crate 职责

压缩 / 上下文优化 / codec evidence / tool schema / content router / context pipeline / optical inventory。

## 2. 关键模块

- `src/lib.rs` — crate 入口，导出 CompressionEngine、ResolvedOptimizationContext 等
- `src/content_router.rs` — 内容路由
- `src/context_pipeline.rs` — 上下文 pipeline
- `src/codec_evidence.rs` — codec evidence
- `src/compressor_registry.rs` — 压缩器注册
- `src/hybrid.rs` — hybrid 压缩
- `src/optical/inventory.rs` — Optical inventory（shadow preview only）
- `src/tool_schema.rs` — MCP tool schema 生成

## 3. Optical 边界（硬约束）

1. **当前仅允许 shadow preview 和 metadata-only observability**。
2. **没有 renderer、patcher、post-validator、rollback 之前，禁止 active transform**。
3. `optical/inventory.rs` 只做 inventory，不做 mutation。

## 4. Codec evidence 边界

1. **`contains_secret_like_content` 是防护函数**：检测并阻止 secret 进入 codec。
2. **secret-like 清单**：`authorization:`、`x-api-key:` 等。
3. **token_signature**：用于验证 codec 变换前后 token 一致性。

## 5. 验证

```bash
cd code/crates/dtoken-compress
cargo check
cargo clippy --all-targets -- -D warnings
cargo test

# 专项测试
cargo test --test code_codec_gate
cargo test --test codec_candidate_evidence
cargo test --test context_pipeline_contract
```

## 6. 依赖关系

- **依赖**：`dtoken-types`、`dtoken-config`（可能）
- **被依赖**：`dtoken-core-api`、`dtoken-preprocessor`（可能）
- **禁止反向依赖**：`dtoken-types` 不能依赖 `dtoken-compress`
