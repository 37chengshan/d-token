# code/crates/ AGENTS.md

本文件是 Rust crate workspace 的 per-directory 入口。通用规则以根 `AGENTS.md` 为准；本文件保留进入 crate 时最需要的上下文。

## 1. Workspace 结构

```text
code/crates/
├── dtoken-types/              共享类型 (agent, runtime, ccr)
├── dtoken-config/             配置读写与凭据引用
├── dtoken-config-patcher/     Agent 配置 patch + cc-switch fixture
├── dtoken-router/             Provider/Route 决策
├── dtoken-compress/           压缩 / 上下文优化 / codec evidence / tool schema
├── dtoken-context-store/      上下文存储 + memory capability + windows private ACL
├── dtoken-core-api/           核心 API surface + MCP gateway + ccr store
├── dtoken-preprocessor/       预处理 pipeline + secret redaction
├── dtoken-agent-bridge/       Agent 桥接 + managed RTK candidate probe
├── dtoken-codebase-memory/    代码库记忆
├── dtoken-doctor/             诊断
├── dtoken-event-ledger/       事件账本 + redacted ledger
├── dtoken-headroom-adapter/   Headroom-derived provenance
├── dtoken-learning/           学习
├── dtoken-process/            进程管理
├── dtoken-project-policy/     项目策略
├── dtoken-proxy/              本地 proxy 入口
└── dtoken-runtime-pipeline/   runtime pipeline
```

## 2. Crate 边界

1. **`dtoken-types` 是依赖根**：所有 crate 都可以依赖它，但它不依赖其他业务 crate。
2. **`dtoken-core-api` 是 API surface 根**：它聚合多个 crate 的能力，对外暴露统一 API。
3. **禁止循环依赖**：crate A 依赖 crate B，B 就不能反过来依赖 A。
4. **业务逻辑放 crate，Tauri command 只桥接**：不要把核心业务堆进 `apps/desktop/src-tauri/`。

## 3. 凭据与安全边界（适用于所有 crate）

1. **凭据只进 OS 凭据库**：crate 中不保存明文 API key / token / cookie / Authorization。
2. **配置只保存不透明引用**：例如 `api_key_ref: Option<String>`，不是明文 key。
3. **secret redaction 清单**：`bearer`、`api_key`、`apikey`、`password`、`authorization` 等关键字必须被 redaction 逻辑覆盖。
4. **禁止日志泄漏**：`tracing::info!` / `debug!` 中不得出现凭据、完整 prompt/response、源代码。
5. **loopback only**：所有本地服务默认只绑定 `127.0.0.1`。

## 4. 验证（crate 级）

进入某个 crate 目录后：

```bash
cd code/crates/<crate-name>
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

整个 workspace：

```bash
cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 5. 常见坑

1. **`Cargo.toml` 版本要对齐**：`specta` / `tauri-specta` / `specta-typescript` 三个 crate 必须用 pin 版本，否则会 build break。
2. **workspace.dependencies 的复用**：常用依赖（serde、tokio、anyhow）在 `code/Cargo.toml` 的 `[workspace.dependencies]` 定义，各 crate 用 `serde.workspace = true` 引用。
3. **新增 crate 后要更新**：`code/Cargo.toml` 的 `members` 列表、`docs/index.md`、`support/scripts/check_structure.py`。

## 6. 相关 crate 专用规范

- `dtoken-compress/AGENTS.md` — compress / codec / optical 边界
- `dtoken-core-api/AGENTS.md` — API surface / MCP gateway / ccr store
- `dtoken-context-store/AGENTS.md` — memory capability / windows private ACL / migrations
- `dtoken-config-patcher/AGENTS.md` — agent transport / cc-switch fixture
