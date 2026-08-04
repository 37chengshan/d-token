# code/apps/desktop/src-tauri/ AGENTS.md

本文件是 Tauri native 层（Rust）的 per-directory 入口。通用规则以根 `AGENTS.md` 和 `code/apps/desktop/AGENTS.md` 为准。

## 1. Tauri native 层职责

1. **Tauri command 桥接**：调用 `dtoken-core-api` 等 crate 的能力，通过 IPC 暴露给前端。
2. **平台适配**：凭据（keyring）、进程、系统集成、打包。
3. **tray / window / panel 管理**：系统托盘、窗口事件、panel material。

## 2. 文件结构

```text
src-tauri/
├── src/
│   ├── main.rs                 Tauri command 注册 + app 入口（~14k 行）
│   ├── tray.rs                 系统托盘
│   ├── upstream_discovery.rs   上游发现
│   ├── opencode_adapter.rs     OpenCode 适配
│   └── ...
├── Cargo.toml                  Tauri native 依赖
├── tauri.conf.json             Tauri 配置（devUrl, bundle, icons）
├── tauri.macos.conf.json       macOS 专用 Tauri 配置
├── icons/                      应用图标
└── examples/                   Tauri command 示例
```

## 3. Tauri command 注册结构

`main.rs` 使用宏来管理 debug/release command：

```rust
// Debug 模式注册所有 command（含 debug-only）
debug_desktop_handlers!(run_desktop_app_with_handlers);

// Release 模式只注册 release command
desktop_release_handlers!(run_desktop_app_with_handlers, []);
```

`run_desktop_app_with_handlers` 宏内部调用 `tauri::generate_handler![...]`。

**添加新 command 时**：

1. 在 `main.rs` 中定义 `#[tauri::command] fn my_command(...) { ... }`
2. 如果是 debug-only，加入 `debug_desktop_handlers!` 宏的列表
3. 如果是 release，加入 `desktop_release_handlers!` 宏的列表
4. 如果用 tauri-specta，还要在 `collect_commands!` 中注册

## 4. tauri-specta 集成（类型安全 IPC）

### 依赖（在 `src-tauri/Cargo.toml`）

三个 specta crate 必须 pin 到完全相同的版本，否则跨版本会破坏构建。当前 pin：

```toml
specta = { version = "=2.0.0-rc.25", features = ["derive"] }
specta-typescript = "0.0.12"
tauri-specta = { version = "=2.0.0-rc.25", features = ["derive", "typescript"] }
```

升级时三个版本必须一起改并重新跑 `cargo check`。

### 架构（P1 修复后）

**关键约束：禁止双重 `invoke_handler`。** Tauri 的 `Builder::invoke_handler`
是 *replace* 语义，第二次调用会覆盖第一次注册的所有既有命令。因此
tauri-specta 的 `Builder::invoke_handler()` 不能与
`tauri::generate_handler!` 并列调用。

当前架构：

1. 所有命令（含 `#[specta::specta]` 的）统一注册进
   `desktop_release_handlers!` / `debug_desktop_handlers!` 宏，由单一的
   `tauri::generate_handler!` 暴露。
2. tauri-specta 的 `Builder` 只用于 **debug 模式下导出 `bindings.ts`**
   （见 `export_specta_bindings`），不参与运行时命令路由。
3. `bindings.ts` 导出失败时只 `eprintln!` 警告，不 panic，避免破坏 desktop
   启动。

```rust
// main.rs — debug 入口
#[cfg(debug_assertions)]
fn main() {
    export_specta_bindings();          // 只导出 bindings，不注册命令
    debug_desktop_handlers!(run_desktop_app_with_handlers);
}

// run_desktop_app_with_handlers! 只调用一次 invoke_handler：
//   .invoke_handler(tauri::generate_handler![
//       ...既有命令...,
//       get_specta_ipc_status,         // specta 命令也在同一列表
//   ])
```

### 前端使用（在 `src/app/lib/native.ts`）

`bindings.ts` 被 gitignore，可能不存在。前端通过
`loadSpectaBindings()` 异步惰性加载；缺失时 `spectaCommands` 抛
`native_unavailable`（fail-closed），不引入 Node `require` 全局：

```typescript
// native.ts — 不裸 require，不静态 import("./bindings")（moduleResolution: Node
// 下 TS2307），用非 literal 动态 import + try/catch
const BINDINGS_MODULE = "./bindings";

async function loadSpectaBindings(): Promise<SpectaBindings | null> {
  try {
    return (await import(BINDINGS_MODULE)) as SpectaBindings;
  } catch {
    return null;   // bindings 缺失，fail-closed
  }
}
```

## 5. 凭据与安全边界

1. **凭据只进 OS 凭据库**：用 `keyring` crate（Windows）或 macOS Keychain。
2. **配置只保存不透明引用**：`api_key_ref: Option<String>`，不是明文 key。
3. **禁止日志泄漏**：`tauri::command` 中不得 `println!` / `eprintln!` 凭据、完整 prompt/response。
4. **loopback only**：所有本地服务默认只绑定 `127.0.0.1`。
5. **进程停止前必须验证**：PID、可执行文件、端口所有权、d-token identity。

## 6. 验证

```bash
cd code/apps/desktop/src-tauri
cargo check
cargo clippy --all-targets -- -D warnings
cargo test

# 完整 Tauri 构建
cd code/apps/desktop
npm run tauri:build
```

## 7. 常见坑

1. **`tauri.conf.json` 的 `devUrl` 必须与 Vite dev server 端口一致**（当前 `http://localhost:5180`）。
2. **`beforeDevCommand` / `beforeBuildCommand` 必须正确**：确保 Vite dev server 先启动。
3. **icons 路径**：`tauri.conf.json` 中 `icon` 字段必须指向 `icons/` 下的真实文件。
4. **`target/` 不要共享**：每个 worktree 用独立 `target/`，配合 `sccache` 加速。
