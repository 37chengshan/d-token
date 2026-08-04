# dtoken-context-store AGENTS.md

本文件是 `dtoken-context-store` crate 的 per-directory 入口。通用规则以根 `AGENTS.md` 和 `code/crates/AGENTS.md` 为准。

## 1. Crate 职责

上下文存储 + memory capability + windows private ACL + migrations。

## 2. 关键模块

- `src/lib.rs` — crate 入口，~3.4k 行，含 secret redaction patterns
- `src/memory_capability.rs` — memory feature 支持
- `src/windows_private_acl.rs` — Windows ACL 处理
- `tests/migrations.rs` — migrations 测试

## 3. Secret redaction 清单

`lib.rs` 中维护的 redaction 关键字清单：

- `bearer`
- `api_key`
- `apikey`
- `password`
- `authorization`
- 其他（见源码）

**修改清单时必须同步**：`dtoken-preprocessor` 的 redaction 逻辑、`docs/SECURITY_BOUNDARIES.md`。

## 4. Windows private ACL 边界

1. **`windows_private_acl.rs` 只在 `cfg(target_os = "windows")` 下编译**。
2. **ACL 修改必须可回退**：记录原始 ACL，支持恢复。
3. **禁止放宽 ACL**：只收紧，不放松。

## 5. Migrations 边界

1. **migrations 只前进，不回退**：删除 migration 文件会破坏已升级的用户。
2. **新增 migration 必须 bump version**：并在 `tests/migrations.rs` 中添加测试。
3. **migration 不处理凭据**：凭据只进 OS 凭据库，不进 context store。

## 6. 验证

```bash
cd code/crates/dtoken-context-store
cargo check
cargo clippy --all-targets -- -D warnings
cargo test

# 专项测试
cargo test --test migrations
```

## 7. 依赖关系

- **依赖**：`dtoken-types`、`dtoken-config`（可能）
- **被依赖**：`dtoken-core-api`、`apps/desktop/src-tauri`
- **这是上下文存储根**：修改它的公开 API 可能影响 core-api 和桌面 App
