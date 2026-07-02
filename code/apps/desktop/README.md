# d-token Desktop

Tauri 2 + React 19 + TypeScript 桌面 App（Windows / macOS）。

M3 阶段目标：fixture-first 的可导航控制台外壳，UI 参考 AiMaMi 的桌面控制台风格，
支持深/浅色主题切换、中/英文切换与动态效果。所有数据均为演示 fixture，
**绝不触碰**真实的 RTK / Headroom / Agent 配置或运行中的服务（No-Touch）。

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
  main.tsx      入口，挂载 Providers + App
  App.tsx       应用外壳（侧栏 + 顶栏 + 页面路由）
  context.tsx   主题 + i18n 的 React Context（localStorage 记忆）
  i18n.ts       中英文文案词典
  pages.tsx     全部页面组件
  ui.tsx        复用 UI 组件（Card/StatCard/Badge/Progress/Sparkline...）
  fixtures.ts   演示数据（fixture-first）
  styles.css    设计系统（CSS 变量驱动的亮/暗色主题 + 动效）
src-tauri/      Tauri 外壳配置（M4 再开启生产打包）
```

## 本地运行

```bash
npm install        # 安装依赖
npm run dev        # 启动 Vite 开发服务器 (http://127.0.0.1:5173)
npm run build      # 类型检查 + 生产构建 (tsc --noEmit && vite build)
npm run preview    # 预览生产构建
```

Tauri 外壳（需本机安装 Rust / Tauri 依赖）指向上面的 Vite 前端；
生产打包（Windows 安装器 / macOS bundle）按里程碑计划留到 M4/M5 开启。

原则：Desktop 是主入口；CLI 只作为调试和高级入口。
