# d-token Desktop

Tauri 2 + React 18 + TypeScript 桌面 App（Windows / macOS）。

当前目标：d-token-owned runtime 的桌面控制面。浏览器预览使用示例数据；Tauri 桌面窗口通过
Tauri commands / d-token local service 读取真实运行时数据。正式 UI 展示安装、连接、配置、
状态、诊断、日志和回滚，不展示 workload/benchmark/smoke 等开发测试入口。

## 功能

1. 分组侧栏导航 + 顶栏，覆盖 PRD 定义的全部页面：
   总览、RTK/Headroom 面板、安装向导、Agents 兼容矩阵、项目、路由、
   服务商、RTK/Headroom 配置、Doctor、日志、变更预览、备份与回滚、设置。
2. 深/浅色主题切换（顶栏一键切换 + 设置页），跟随系统偏好并记忆到本地。
3. 中/英文切换（顶栏 中/EN + 设置页），首启跟随系统语言并记忆。
4. 动态效果：页面进场、卡片悬停、数字滚动、进度条与折线图动画、状态点脉冲。
5. 敏感信息保护：绝不显示原始密钥 / Authorization / Cookie / 会话令牌。

## 目录结构

```
src/
  main.tsx      当前入口，挂载 src/app/App
  app/
    App.tsx
    components/
      shell/    Sidebar / Topbar / StatusBar / AppShell
      pages/    Overview / Install / Advanced / Analytics / Doctor / Changes / Logs / Settings
      common/   DataSourceBadge / Heatmap / cards / terminal 等
      ui/       复用 UI primitives
    lib/        i18n、theme、runtime dashboard hook、native invoke、preview data
  App.tsx       兼容转发到 src/app/App，避免旧入口复活
  pages.tsx     兼容转发到 src/app/components/pages
  fixtures.ts   兼容转发到 src/app/lib/fixtures
  styles/       设计系统（CSS 变量驱动的亮/暗色主题 + 动效）
src-tauri/      Tauri 外壳、Rust commands、平台配置与生产打包
```

## 本地运行

```bash
npm install        # 安装依赖
npm run dev        # 启动 Vite 开发服务器 (http://127.0.0.1:5180)
npx tsc --noEmit   # 独立类型检查
npm run build      # Vite 生产构建
npm run test:run   # Vitest 非交互测试
npm run preview    # 预览生产构建
```

Tauri 外壳（需本机安装 Rust / Tauri 依赖）指向上面的 Vite 前端；

```bash
npm run tauri:dev
npm run tauri:build
npm run tauri:build -- --target aarch64-apple-darwin
npm run tauri:build -- --target x86_64-apple-darwin
npm run tauri:build -- --target universal-apple-darwin
```

`tauri:dev` / `tauri:build` 会先准备当前平台的 `dtoken-proxy` sidecar。macOS 使用 Bash
脚本并按 Tauri 的 `--target` 构建 Apple Silicon、Intel 或 universal sidecar；Windows 继续使用
现有 PowerShell 准备流程。交叉构建前需先安装对应 Rust target，例如
`rustup target add x86_64-apple-darwin`。

macOS bundle 当前启用 `app` 和 `dmg` target，并通过平台配置移除 Windows 专属的 ICO 与
`WebView2Loader.dll`。桌面图标由仓库中的矢量品牌源生成：macOS 使用 `.icns` 与 PNG，
Windows 使用 ICO；两端保持同一视觉标识。

Windows 与 macOS 使用同一套 React UI、信息架构和功能语义；平台差异只放在 Tauri/Rust
系统适配、凭据存储和 bundle 配置。macOS 原生能力默认由 Rust/Tauri 提供，只有缺少可靠
实现时才引入极少量 Swift 桥接，Swift 不承载压缩、路由或配置事务等业务逻辑。

生产运行时是 d-token-owned Rust core。`dtoken-proxy` 虽由 Tauri 以 external binary 形式打包，
仍是本仓库 Rust workspace 产物；不存在 Python sidecar 或外部 Headroom dashboard 运行依赖。

原则：Desktop 是主入口；CLI 只作为调试和高级入口。
