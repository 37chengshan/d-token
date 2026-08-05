# code/apps/desktop/src/app/ AGENTS.md

本文件是 React UI 层的 per-directory 入口。通用规则以根 `AGENTS.md` 和 `code/apps/desktop/AGENTS.md` 为准。

## 1. UI 层职责

1. **渲染产品 UI**：总体概览、安装与连接、代码图谱、分析、视觉压缩、Doctor、变更与回退、设置。
2. **通过 IPC 调用 Rust runtime**：不直接做业务逻辑。
3. **显示真实状态**：带 source、freshness、action result，禁止用示例数字补齐。

## 2. 文件结构

```text
src/app/
├── components/
│   ├── common/                 通用组件（RuntimeNotificationBridge）
│   ├── pages/                  一级页面（Analytics, Changes, Overview, Install, SmartLearning, Settings）
│   └── shell/                  AppShell, AnimatedSegmentedControl
├── lib/
│   ├── native.ts               IPC 桥接（invokeNative, isTauriRuntime）
│   ├── useRuntimeDashboard.ts  runtime dashboard 数据 hook
│   ├── agents/registry.ts      agent registry
│   ├── install/                install flow（InstallView, presentation）
│   ├── installMachine.ts       install state machine
│   ├── visual-compression/     视觉压缩 workbench
│   ├── connectionSetup.tsx     连接设置
│   └── learning/               学习（已删除 discovery.ts）
├── TrayPanelApp.tsx            系统托盘 panel app
├── TrayPanelApp.test.tsx
└── ...
```

## 3. IPC 调用模式

### 旧模式（字符串 invoke）

```typescript
import { invoke } from "@tauri-apps/api/core";
const result = await invoke<MyReturnType>("my_command", { arg });
```

### 新模式（tauri-specta 类型安全）

`bindings.ts` 被 gitignore，可能不存在。前端通过
`loadSpectaBindings()` 异步惰性加载（动态 `import("./bindings")` +
try/catch）；缺失时 `spectaCommands` 抛 `native_unavailable`
（fail-closed），不引入 Node `require` 全局：

```typescript
// src/app/lib/native.ts — 不裸 import，用非 literal 动态 import + try/catch
const BINDINGS_MODULE = "./bindings";

async function loadSpectaBindings(): Promise<SpectaBindings | null> {
  try {
    return (await import(BINDINGS_MODULE)) as SpectaBindings;
  } catch {
    return null;   // bindings 缺失，fail-closed
  }
}

export const spectaCommands = new Proxy(
  {} as Record<string, (...args: unknown[]) => Promise<unknown>>,
  {
    get(_target, prop: string) {
      return async (...args: unknown[]) => {
        const bindings = await loadSpectaBindings();
        if (!bindings) throw new Error("native_unavailable");
        const fn = bindings.commands[prop];
        if (typeof fn !== "function") {
          throw new Error(`specta command not found: ${prop}`);
        }
        return fn(...args);
      };
    },
  },
);
```

**迁移策略**：新 command 直接用 `spectaCommands.xxx()`，旧 command 逐步迁移。

### 前端 mock 层（独立开发 UI 时不启动 Rust）

```typescript
// native.ts 中
if (!isTauriRuntime()) {
  return mockInvoke<T>(command, args);
}
```

这样前端可以 `npm run dev` 独立跑，不依赖 Rust 后端。

## 4. 页面与导航边界（硬约束，重复强调）

正式一级页面固定为：总体概览、安装与连接、代码图谱（实验性）、分析、视觉压缩（Beta）、Doctor、变更与回退、设置。

**禁止事项**：

1. `Advanced`、`LogsSecurity`、`Compatibility`、`SmartLearning` 不得重新扩张为工程化顶级导航。
2. 除「视觉压缩（Beta）」与「代码图谱（实验性）」外不得再新增工程化顶级导航。
3. 正式 UI 不得出现 workload benchmark、smoke test、fixture、mock、fake-bin、testing center 等开发术语。
4. 无完整 source/freshness/action-result 证据时，必须显示 unavailable、partial、preview 或 unknown，禁止用示例数字和绿色状态补齐。

## 5. 验证

```bash
cd code/apps/desktop

# 单元测试
npm run test:run

# 类型检查
npx tsc --noEmit

# 构建
npm run build

# E2E
npm run test:e2e
```

## 6. 常见坑

1. **`isTauriRuntime()` 判断**：在浏览器 dev 模式下 `__TAURI_INTERNALS__` 不存在，要走 mock 分支。
2. **`invokeNative` 的泛型**：前端调用时要显式指定返回类型 `invokeNative<MyReturnType>(...)`。
3. **bindings.ts 是自动生成的**：不要手改，改 Rust command 后重新 `npm run tauri:dev` 即可重新生成。文件被 gitignore，缺失时前端 fail-closed 为 `native_unavailable`。
