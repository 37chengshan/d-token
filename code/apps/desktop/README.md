# d-token Desktop

Tauri 2 + React 19 + TypeScript 桌面 App（Windows / macOS）。

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
src-tauri/      Tauri 外壳配置（M4 再开启生产打包）
```

## 本地运行

```bash
npm install        # 安装依赖
npm run dev        # 启动 Vite 开发服务器 (http://127.0.0.1:5180)
npm run build      # 类型检查 + 生产构建 (tsc --noEmit && vite build)
npm run preview    # 预览生产构建
```

Tauri 外壳（需本机安装 Rust / Tauri 依赖）指向上面的 Vite 前端；
生产打包（Windows 安装器 / macOS bundle）按里程碑计划留到 M4/M5 开启。

原则：Desktop 是主入口；CLI 只作为调试和高级入口。
