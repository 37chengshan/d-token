# d-token Desktop

更新时间：2026-07-11 晚间

`code/apps/desktop/` 是 d-token 唯一正式桌面 App：Tauri 2 + React 18 + TypeScript + Vite，Windows/macOS 共用同一套 UI、信息架构和产品语义。

## 当前页面

正式一级导航定义在 `src/app/lib/nav.ts`：

```text
总体概览
安装与连接
分析
Doctor
变更与回退
设置
```

`Advanced`、`LogsSecurity`、`Compatibility`、`SmartLearning` 等是工作台子页面或内部组件，不是新的一级导航。

## 目录

```text
src/
├── main.tsx                         当前入口
├── app/
│   ├── App.tsx
│   ├── components/shell/            Sidebar / Topbar / StatusBar / AppShell
│   ├── components/pages/            当前产品页面与工作台子页面
│   ├── components/common/           通用展示与状态组件
│   ├── components/ui/               UI primitives
│   └── lib/                         i18n、theme、native invoke、runtime evidence
├── App.tsx                          兼容转发到 src/app/App
├── pages.tsx                        兼容转发，不是新功能入口
├── fixtures.ts                     浏览器 preview/测试兼容数据
└── src-old/                         旧实现，不得新增功能

src-tauri/                            Tauri native commands、凭据、进程和打包
```

## 数据边界

- Tauri 桌面模式通过 native command 和 `127.0.0.1` d-token-owned service 读取真实状态。
- 浏览器模式只能用于 UI preview，必须清楚标识，不能伪装成本机真实运行状态。
- runtime 数据必须带 source/freshness；缺失或不完整时显示 unavailable/partial。
- release build 不注册 OpenCode workload、模型测试和 Headroom smoke 等开发命令。

## 当前 native 能力

- App 设置读写。
- Provider credential 的 OS credential store 写入/删除。
- Route/provider 草稿、active config、rollback point 与恢复。
- d-token Rust runtime 状态、启动、停止和 identity 检查。
- 安装事务、Doctor、route 验证、产品 readiness、Dashboard 与脱敏导出。

当前限制：第三方 Agent 自动配置和完整外部配置 rollback 尚未闭环；缺失 RTK/Headroom 时只检测并安全失败，不执行未经验证的下载。

## 开发

```bash
npm install
npm run dev
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

## Tauri

```bash
npm run tauri:dev
npm run tauri:build
npm run tauri:build -- --target aarch64-apple-darwin
npm run tauri:build -- --target x86_64-apple-darwin
npm run tauri:build -- --target universal-apple-darwin
```

`tauri:dev` / `tauri:build` 会先准备当前 target 的 `dtoken-proxy` external binary：macOS 使用 Bash 准备脚本，Windows 使用 PowerShell 准备脚本。交叉构建前需安装对应 Rust target。

生产 runtime 是本仓库 Rust workspace 产物，不存在 Python sidecar 或外部 Headroom dashboard 运行依赖。

## 验收

前端通过不等于真实功能通过。涉及服务、路由、配置、Doctor、日志或导出时，还必须提供 Tauri command、d-token runtime、route decision、Event Ledger 或 artifact 证据。

完整页面规格见 `docs/APP_FUNCTIONS_AND_PAGES.md`，当前实现状态见 `docs/product/IMPLEMENTATION_STATUS.md`。
