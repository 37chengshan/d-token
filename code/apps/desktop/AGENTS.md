# code/apps/desktop/ AGENTS.md

本文件是桌面 App 的 per-directory 入口。通用规则以根 `AGENTS.md` 为准。

## 1. App 结构

```text
code/apps/desktop/
├── src/
│   ├── app/                    React 产品 UI
│   │   ├── components/         页面组件（Analytics, Changes, Overview, Install, SmartLearning）
│   │   ├── lib/                桥接层（native.ts, useRuntimeDashboard.ts, agents/registry.ts）
│   │   └── main.tsx            React 入口
│   ├── styles/                 全局样式
│   └── main.tsx                Vite 入口
├── src-tauri/                  Tauri native 层（Rust）
│   ├── src/
│   │   ├── main.rs             Tauri command 注册 + app 入口
│   │   ├── tray.rs             系统托盘
│   │   ├── upstream_discovery.rs  上游发现
│   │   └── opencode_adapter.rs    OpenCode 适配
│   ├── Cargo.toml              Tauri native 依赖
│   ├── tauri.conf.json         Tauri 配置
│   └── icons/                  应用图标
├── package.json                前端依赖与脚本
├── vite.config.ts              Vite 配置
└── scripts/                    构建/检查脚本
```

## 2. 双层开发模式

### 模式 1: 纯前端开发（不碰 Rust）

```bash
cd code/apps/desktop
npm run dev          # Vite dev server on :5180
```

适用场景：UI 调整、CSS、React 组件重构、新增页面布局。

### 模式 2: 全栈开发（Rust + React）

```bash
cd code/apps/desktop
npm run tauri:dev    # Tauri dev: Rust 编译 + 连接 Vite dev server
```

适用场景：新增/修改 Tauri command、改 IPC 契约、改 Tauri 配置。

## 3. IPC 契约（tauri-specta）

本仓库使用 `tauri-specta` 实现类型安全的 Tauri IPC。

### 工作流

1. **改 Rust command 签名** → `src-tauri/src/main.rs`
2. **自动生成 TypeScript bindings** → `src/app/lib/bindings.ts`（debug build 时自动导出）
3. **前端调用 bindings** → `src/app/lib/native.ts`

### 添加新 command 的步骤

```rust
// src-tauri/src/main.rs
#[tauri::command]
#[specta::specta]  // <-- 必须加这个
fn my_new_command(arg: String) -> Result<MyReturnType, String> {
    // ...
}
```

然后在 `specta_commands!` 宏中注册（用于 debug bindings 导出）：

```rust
macro_rules! specta_commands {
    () => { collect_commands![my_new_command,] };
}
```

同时把 `my_new_command` 加入 `desktop_release_handlers!` / `debug_desktop_handlers!` 宏的命令列表，使其通过单一的 `tauri::generate_handler!` 注册。**不要**单独调用 `tauri_specta::Builder::invoke_handler()`，那会覆盖既有命令。

前端调用：

```typescript
// src/app/lib/native.ts — 通过 spectaCommands Proxy 调用
export async function myNewCommand(arg: string) {
  return spectaCommands.myNewCommand(arg);
}
```

注意：bindings.ts 生成后，`spectaCommands` 可获取完全类型签名；缺失时抛
`native_unavailable`（fail-closed），不引入 Node `require` 全局。`spectaCommands`
由 `native.ts` 的 Proxy 提供，无需消费者手动 import bindings 模块。

### 关键约束

1. **`#[specta::specta]` 必须加在 `#[tauri::command]` 之后**。
2. **bindings.ts 是自动生成的，不要手改**。
3. **改 command 签名后，前端类型检查会立即报错** → 这是 feature，不是 bug。
4. **禁止双重 `invoke_handler`**：Tauri 的 `Builder::invoke_handler` 是 replace 语义，第二次调用覆盖第一次注册的所有既有命令。所有命令（含 specta）统一注册进 `desktop_release_handlers!` / `debug_desktop_handlers!` 宏的单一 `tauri::generate_handler!`。tauri-specta `Builder` 只用于 debug 模式导出 `bindings.ts`（`export_specta_bindings`），不参与运行时命令路由。
5. **bindings 导出失败不 panic**：`export_specta_bindings` 用 `eprintln!` 降级，避免破坏 desktop 启动。
6. **前端禁止裸 `require` 和静态 `import("./bindings")`**：`native.ts` 用非 literal 动态 `import(BINDINGS_MODULE)` + try/catch 惰性加载；缺失时 `spectaCommands` 抛 `native_unavailable`（fail-closed），不引入 Node 全局。

## 4. 页面与导航边界（硬约束）

正式一级页面固定为：

- 总体概览（Overview）
- 安装与连接（Install）
- 代码图谱（实验性）
- 分析（Analytics）
- 视觉压缩（Beta）
- Doctor
- 变更与回退（Changes）
- 设置（Settings）

**禁止事项**：

1. `Advanced`、`LogsSecurity`、`Compatibility`、`SmartLearning` 不得重新扩张为工程化顶级导航。
2. 除「视觉压缩（Beta）」与「代码图谱（实验性）」外不得再新增工程化顶级导航。
3. 正式 UI 不得出现 workload benchmark、smoke test、fixture、mock、fake-bin、testing center 等开发术语。
4. 无完整 source/freshness/action-result 证据时，必须显示 unavailable、partial、preview 或 unknown，禁止用示例数字和绿色状态补齐。

## 5. 验证

```bash
cd code/apps/desktop

# 前端验证
npm run test:run
npx tsc --noEmit
npm run build

# 全栈验证（需要 Rust 工具链）
npm run tauri:dev    # 开发模式
npm run tauri:build  # 生产构建

# bundle budget
node scripts/check-bundle-budget.mjs
```

## 6. 子目录专用规范

- `src-tauri/AGENTS.md` — Tauri native 层（Rust）专用规范
- `src/app/AGENTS.md` — React UI 层专用规范
