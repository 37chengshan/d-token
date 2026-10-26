# d-token Desktop

更新时间：2026-09-06

`code/apps/desktop/` 是 d-token 唯一正式桌面 App：Tauri 2 + React 18 + TypeScript + Vite，Windows/macOS 共用同一套 UI、信息架构和产品语义。

## 当前页面

正式一级导航定义在 `src/app/lib/nav.ts`：

```text
总体概览
安装与连接
代码图谱（实验性）
分析
视觉压缩（Beta）
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
- 安装事务、Doctor v3 自动扫描并修复/仅扫描、专项模块、Agent 多选逐项事务、确认与自动复检、route 验证、产品 readiness、Dashboard 与脱敏导出。
- Changes 默认备份与回滚，提供 integrity/current-state 预检、revision/hash-bound 确认、可逆安全备份和文件/runtime 分层复检；Agent 多文件事务聚合为一个回退点，Agent Integration/Optical 按 `executor_kind` 调用来源 executor。
- Changes 通过 `get_config_change_records` 独立读取本机备份、草稿和操作记录，不依赖模型服务在线或历史统计新鲜度。工具登记范围内的配置备份复用精确恢复；失败历史、旧版记录与损坏备份使用可操作的中英文说明。
- 外观保存以本次完成 revision 通知，连续同结果保存也能反馈；重新进入设置不重放旧 Toast，成功不插入会挤动布局的横幅。分析无请求时只显示一处空态，加载/失败仍明确，实际请求存在时保留详细统计和分页。
- OpenCode 安装统一走受管事务：连接与默认模型一起应用，原默认选择单独记录，清除恢复原选择；preview/diff/双 hash/backup/apply/verify/exact rollback 覆盖配置及记录。最新真实 App 点击复验仍待完成。
- Smart Learning 本地扫描、managed block preview/apply 与 exact rollback。
- 代码图谱实验页：App 不内置或下载大型 upstream；App 先只读识别本机安装，也可导入用户下载的官方压缩包；压缩包与二进制均通过固定版本校验后才切换到新的自有目录。已解压目录保留为可展开的替代入口。批准项目后自动扫描本机 Agent，多选批量 preview/apply/verify/rollback MCP（Hook 仍单独授权），管理 d-token-owned 索引与短期授权只读图页，并独立显示 package、integration、Agent session、UI session、index 五维证据。
- 代码图谱（实验性）批量事务、独立 index/UI host、只读 graph open/start/stop/delete；按钮不可用时显示明确的 package/project/Agent/组合原因。
- Claude Code/Codex/OpenCode Hybrid Agent 接入 preview/diff/backup/apply/verify/rollback；规则、Hook/Plugin、Skill 与可选 MCP 由 Rust-owned 事务生成。
- `dtoken-agent-bridge` sidecar：SessionStart 状态说明与 `hybrid_status`/`explain_last_decision` 两个 metadata-only MCP 工具。
- 本地持久化通知和统一 `OperationDialog`：右下角 Toast 两秒后退出，悬停/键盘聚焦暂停，最多三条并保留通知历史；查看打开对应详情，关闭提示不删除记录。操作错误保留在弹窗/页面供重试；危险动作按影响确认。

OpenCode 历史验收与当前候选分开记录。2026-09-06 的真实基础请求已有路由记录，但当前免费 MiMo 历史工具输出压缩验收被 HTTP 429 阻断，尚不能声明当前候选真实压缩通过；每次临时配置均使用私有备份并精确恢复。该授权不扩展到其他工具真实配置。缺失 RTK/Headroom 时只检测并安全失败，不执行未经验证的下载；Headroom 是可选能力，不阻断 Doctor readiness。

## 当前 UI

- 纯色、低阴影、非玻璃化的 shell/surface，侧栏和顶部工作区视觉连贯；不使用动态背景。
- 页面层级与密度参考 AiMaMi，但保留 d-token 自有品牌、数据和交互语义。
- 概览默认全年 heatmap，四格指标严格展示真实请求、物理减少、物理比例与错误数。
- Doctor 采用“自动扫描/规划 → 脱敏 Diff → 一次确认 → 逐项执行 → 自动复检”的维护中心；Agent 专项修复支持多选、独立事务和部分成功。变更与回退只保留“备份与回滚 / 变更记录”两个顶级标签。
- 安装工具采用局部三列响应式网格，扫描与批量配置共用工具栏；删除重复大标题和导航外框，详情用就近弹出层避免撑开整行。首页保持原有样式。
- 设置使用 grouped rows；1180×780 展开侧栏，920×640 自动折叠且保持可滚动。
- success 通知留在 Notification Center；只有未读 warning/error/action-required 可占用顶栏 banner。

## 发布文案约定

- `src/app/lib/i18n.ts` 是正式 UI 的双语词典；新增可见静态文案、数量或名称插值必须同步提供中文和 English。
- `src/app/lib/userCopy.ts` 将 native 结果转换为安全的用户说明。页面、通知和弹窗不得直接显示 native `summary`、`error` 或异常文本。
- 文案使用“事实 → 影响 → 下一步”，按钮使用“动词 + 对象”。不要使用审核口吻、测试/演示词或拟人化 AI 话术；真实产品语义的 preview 除外。
- `src/app/lib/copyPolicy.test.ts` 检查生产源的禁用表达与可见错误直传。内部协议字段、命令名和测试代码可以保留其技术名称。

## 历史 macOS 实证（2026-07-18 快照）

- `OpenCode -> d-token 127.0.0.1:18001 -> OpenCode Zen` 已真实通过。
- 关键请求物理减少 33,705 Token，claimable 0；runtime 重启后保留 89 条 routed request、20 条可见 record，汇总物理减少 279,270（4.0%）。
- 历史候选的 product-readiness 真实窗口为 9/9、旧 Changes 显示 0 项待处理；该证据早于当前 Doctor 主动修复闭环和新回退预检，不能替代新页面的跨平台实机验收。Smart Learning 临时目录 exact rollback 通过。
- OpenCode 配置恢复 SHA-256：`47772c85307f02e1d910dc3cc93b85165a9fc37db5911b7d4201be766f85baab`。
- active route 恢复 SHA-256：`37149d6c69561a4562a67d7d53bf03337e37e98104424c694aaa4a4038c45843`。
- 截图目录：`support/artifacts/macos-ui/final-candidate-2026-07-12T18-52-02-502Z/`。
- 本机 ad-hoc 产物的 SHA-256 不写死在源码文档中；每次候选构建都由 `support/artifacts/` 下的 commit-bound manifest 或平台 workflow manifest 记录 App、sidecar 与安装介质哈希。正式发布仍须在候选 commit 上用 Developer ID 重新构建、notarize 与 staple。

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

`tauri:dev` / `tauri:build` 会先准备当前 target 的 `dtoken-proxy` 与 `dtoken-agent-bridge` external binaries：macOS 使用 Bash 准备脚本，Windows 使用 PowerShell 准备脚本。交叉构建前需安装对应 Rust target。

Windows 公开发布会在准备后、Tauri bundling 前显式签名其实际使用的 MSVC sidecar 输入，并验证 release 副本和 NSIS 安装后副本的 Authenticode；本地无凭据构建不会伪装成正式签名候选。macOS 公开 artifact 只上传已经 notarize/staple 的 DMG，未单独 staple 的裸 `.app` 不作为远程发布 artifact。

生产 runtime 是本仓库 Rust workspace 产物，不存在 Python sidecar 或外部 Headroom dashboard 运行依赖。

## 验收

前端通过不等于真实功能通过。涉及服务、路由、配置、Doctor、日志或导出时，还必须提供 Tauri command、d-token runtime、route decision、Event Ledger 或 artifact 证据。

完整页面规格见 `docs/APP_FUNCTIONS_AND_PAGES.md`，当前实现状态见 `docs/product/IMPLEMENTATION_STATUS.md`。

2026-07-13 当前候选门禁：前端 125 项测试、Playwright E2E 20 项、TypeScript、Vite build 和 Tauri build 通过，`npm audit` 为 0 个漏洞。macOS bundle 当前仅为 ad-hoc 签名候选，不等于正式发布签名。
