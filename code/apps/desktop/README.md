# d-token Desktop

更新时间：2026-07-13

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
│   ├── components/common/           Surface / OperationDialog / NotificationCenter 等
│   ├── components/ui/               UI primitives
│   └── lib/                         i18n、theme、native invoke、runtime evidence
├── App.tsx                          兼容转发到 src/app/App
├── pages.tsx                        兼容转发，不是新功能入口
├── fixtures.ts                     仅测试/开发兼容数据，不进入正式桌面事实链
└── src-old/                         旧实现，不得新增功能

src-tauri/                            Tauri native commands、凭据、进程和打包
```

## 数据边界

- Tauri 桌面模式通过 native command 和 `127.0.0.1` d-token-owned service 读取真实状态。
- 非 Tauri 模式只用于开发预览并 fail-closed，不向正式页面注入 mock/fixture 数字。
- runtime 数据必须带 source/freshness；缺失或不完整时显示 unavailable/partial。
- release build 不注册 OpenCode workload、模型测试和 Headroom smoke 等开发命令。

## 当前 native 能力

- App 设置读写。
- Provider credential 的 OS credential store 写入/删除。
- Route/provider 草稿、active config、rollback point 与恢复。
- d-token Rust runtime 状态、启动、停止和 identity 检查。
- 安装事务、Doctor、route 验证、产品 readiness、Dashboard 与脱敏导出。
- OpenCode 配置 preview/diff/双 hash/backup/apply/verify/exact rollback。
- Smart Learning 本地扫描、managed block preview/apply 与 exact rollback。
- 本地持久化通知和统一 `OperationDialog`，危险动作在确认前显示影响、备份与恢复方式。

OpenCode 已完成真实配置事务与 routed compression 验收；该结论不扩展到 Claude Code、Codex、Cursor、Cline 或 cc-switch。缺失 RTK/Headroom 时只检测并安全失败，不执行未经验证的下载；Headroom 是可选能力，不阻断 Doctor readiness。

## 当前 UI

- 纯色、低阴影、非玻璃化的 shell/surface，侧栏和顶部工作区视觉连贯；不使用动态背景。
- 页面层级与密度参考 AiMaMi，但保留 d-token 自有品牌、数据和交互语义。
- 概览默认全年 heatmap，四格指标严格展示真实请求、物理减少、物理比例与错误数。
- 设置使用 grouped rows；1180×780 展开侧栏，920×640 自动折叠且保持可滚动。
- success 通知留在 Notification Center；只有未读 warning/error/action-required 可占用顶栏 banner。

## 当前 macOS 实证

- `OpenCode -> d-token 127.0.0.1:18001 -> OpenCode Zen` 已真实通过。
- 关键请求物理减少 33,705 Token，claimable 0；runtime 重启后保留 89 条 routed request、20 条可见 record，汇总物理减少 279,270（4.0%）。
- Doctor 真实窗口 9/9；Changes 为 0 项待处理；Smart Learning 临时目录 exact rollback 通过。
- OpenCode 配置恢复 SHA-256：`47772c85307f02e1d910dc3cc93b85165a9fc37db5911b7d4201be766f85baab`。
- active route 恢复 SHA-256：`37149d6c69561a4562a67d7d53bf03337e37e98104424c694aaa4a4038c45843`。
- 截图目录：`support/artifacts/macos-ui/final-candidate-2026-07-12T18-52-02-502Z/`。
- 当前工作树 ad-hoc `d-token-desktop` SHA-256：`6a04409997ac2741b7a61e4e72ad7eac87649d56eaf8fc70918d8c336fbe3048`；正式发布仍须在候选 commit 上用 Developer ID 重新构建、notarize 与 staple。

仓库已配置 fail-closed 的 macOS Developer ID/notarization/Gatekeeper/hash workflow，以及 Windows Authenticode/NSIS 静默安装启动/hash workflow。平台 autostart 已接通 macOS LaunchAgent 与 Windows HKCU Run，并采用统一确认、写后校验和失败回滚。仍未完成：真实签名凭据执行、signed update feed、两端真实登录会话 autostart 验收、Windows 凭据/路径/视觉实机验收、其他 Agent 真实事务验收、认证 Provider Keychain/Credential Manager 实证。

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

Windows 公开发布会在准备后、Tauri bundling 前显式签名其实际使用的 MSVC sidecar 输入，并验证 release 副本和 NSIS 安装后副本的 Authenticode；本地无凭据构建不会伪装成正式签名候选。macOS 公开 artifact 只上传已经 notarize/staple 的 DMG，未单独 staple 的裸 `.app` 不作为远程发布 artifact。

生产 runtime 是本仓库 Rust workspace 产物，不存在 Python sidecar 或外部 Headroom dashboard 运行依赖。

## 验收

前端通过不等于真实功能通过。涉及服务、路由、配置、Doctor、日志或导出时，还必须提供 Tauri command、d-token runtime、route decision、Event Ledger 或 artifact 证据。

完整页面规格见 `docs/APP_FUNCTIONS_AND_PAGES.md`，当前实现状态见 `docs/product/IMPLEMENTATION_STATUS.md`。

2026-07-13 当前候选门禁：前端 125 项测试、Playwright E2E 20 项、TypeScript、Vite build 和 Tauri build 通过，`npm audit` 为 0 个漏洞。macOS bundle 当前仅为 ad-hoc 签名候选，不等于正式发布签名。
