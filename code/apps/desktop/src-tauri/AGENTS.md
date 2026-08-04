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

```toml
[dependencies]
specta = { version = "=2.0.0-rc.26", features = ["derive"] }
specta-typescript = "0.0.13"
tauri-specta = { version = "=2.0.0-rc.25", features = ["derive", "typescript"] }
```

### Builder 配置（在 `main.rs` 的 `run_desktop_app_with_handlers` 宏中）

```rust
use tauri_specta::{ts, collect_commands};

// 在 app setup 中导出 bindings
let builder = tauri_specta::Builder::<tauri::Wry>::new()
    .commands(collect_commands![
        // 列出所有 #[specta::specta] 标注的 command
        my_command_1,
        my_command_2,
    ]);

#[cfg(debug_assertions)]
{
    // debug 模式下导出 bindings.ts 到前端
    builder
        .export_events("../src/app/lib/bindings.ts")
        .expect("Failed to export tauri-specta bindings");
}

// 正常 run tauri
tauri::Builder::default()
    .invoke_handler(builder.invoke_handler())
    .run(tauri::generate_context!())
    .expect("error while running d-token desktop");
```

### 前端使用（在 `src/app/lib/native.ts`）

```typescript
import { commands } from "./bindings";

// 直接调用，有类型检查
const result = await commands.myCommand(arg);
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
