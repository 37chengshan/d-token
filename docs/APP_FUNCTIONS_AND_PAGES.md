# d-token App 功能与页面

更新时间：2026-09-06
产品真理源：`docs/PRD.md`
代码入口：`code/apps/desktop/src/app/`

当前阶段：用户要求先恢复本机已安装前端，再对齐新方案。前端基准与修改边界见 [FRONTEND_CONSTRAINTS.md](FRONTEND_CONSTRAINTS.md)。本文件保留八页的目标任务、接口职责与完成条件；恢复旧前端不表示这些目标均已在该前端接入，实际完成情况以实现状态与当前验证为准。

第二轮页面约定（2026-09-06，尚未实施）：安装每排 3–4 张两行小纸片卡，整卡切换选中、详情独立展开；诊断左侧真实通过项/总检查项圆环和操作，右侧轻量结果行；分析提高首屏可读性并保留下方可滚动明细；全部页面清理无用文案、标题与嵌套，保留可执行内容和既有质感。普通操作结果使用右下角两秒 Toast，持久结果、错误处理和进度有独立承载。

## 1. 当前信息架构

正式桌面 App 使用八个一级页面（含 1 个 Beta 与 1 个实验性本地工具页）：

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

实际一级导航定义在 `code/apps/desktop/src/app/lib/nav.ts`。代码中仍存在 `advanced`、`logs`、`compatibility` 等 PageId 和页面组件，它们是安装工作台子流程、内部跳转或既有实现来源，不应重新扩张为工程化顶级导航。

## 2. 页面任务与代码映射

| 正式页面 | 主要代码 | 用户任务 | 当前边界 |
|---|---|---|---|
| 总体概览 | `components/pages/Overview.tsx` | 查看 d-token 服务、运行证据、关键指标、近期问题和下一步动作 | 只有 Tauri + d-token-owned runtime 的完整新鲜证据才能显示为真实状态；浏览器预览必须明确标记 |
| 安装与连接 | `Install.tsx`、`InstallWorkbench.tsx` | 配置 d-token 服务、自动扫描全部 Agent、已安装受支持项默认勾选且允许取消、逐 Agent 自动解析链路、一次点击配置、后台备份/应用/复读验证、逐 Agent 展示结果 | 当前能保存和应用 d-token 自有 route config；第三方 Agent 自动写入必须按 P0.0.10 的独立事务闭环，不得 whole-batch rollback |
| 代码图谱（实验性） | `CodebaseMemory.tsx`、`lib/codebaseMemory.ts` | 按当前 manifest/allowlist 导入并校验受支持的图谱组件，打开当前官方 release，自定义目录后自动识别；自动扫描本机 Agent，批量预览/配置/验证/回退 MCP；一键启动并打开 d-token-owned 只读图谱并管理独立索引 | App 不 bundle/下载 upstream；五维状态独立；批量操作必须在 Native 返回前显示 Agent、阶段和 current/total 进度，并在刷新/重启后恢复所有未回退 transaction；大型 artifact 校验、进程与 loopback 工作必须在后台 worker 执行；仅允许当前 manifest/allowlist 绑定的 headless artifact、受限 launcher、`persistence=false` 和短期授权 loopback；旧协议/配置不兼容版本仍 fail-closed；不得直出上游原生 UI，也不得停止 Agent-owned MCP 进程 |
| 分析 | `Analytics.tsx` | 查看请求、token、路由、延迟和错误的聚合趋势 | 不得把 fixture 或推算值冒充真实账单；估算必须标注 |
| 视觉压缩（Beta） | `VisualCompression.tsx`、`lib/optical/*` | 一键启用安全 Shadow 默认值；高级配置按 route 调整策略；预览 diff、确认应用与回退；状态与预览查看 status/capabilities/lifecycle/events 与 shadow plan | 信息架构对齐安装工作台（一键 + 高级 + 状态）；仅 shadow/metadata；产品路径强制 `allow_active=false`；写盘走 ConfigPatcher + 服务重启；不渲染 PNG/base64/raw body；节省为估算；Windows/macOS 共享同一 UI |
| Doctor | `Doctor.tsx` | 自动扫描并修复或仅扫描；使用专项模块修复服务、已有 Agent 接入、路由/Optical/平台等受管项并自动复检 | v3 快照动态判定 freshness；服务和已有管理证据的 Claude Code/Codex/OpenCode Agent Integration 已接 executor，其他模块按 capability 锁定或返回来源流程；Optical Active/cc-switch 写入永远禁止 |
| 变更与回退 | `Changes.tsx` | 默认查看备份与回滚，预检当前可恢复性；查看已提交的持久变更记录 | 顶级待批/Diff 已移除；P0.0.10 Agent 自动配置的 Diff/backup/receipt 在后台生成并进入详情/历史，不再作为第二层普通确认；每 Agent 保持独立回退点，普通/Agent/Optical 按 `executor_kind` 分发并先创建反向安全备份 |
| 设置 | `Settings.tsx` | 管理主题、语言、本地端口、安全、项目策略和应用偏好 | 设置只写 d-token 本地状态，不存储明文凭据或原始 prompt/response |

## 3. 安装与连接工作台

安装页承担完整用户主路径，而不是测试中心：

```text
确认服务状态
→ 自动扫描全部 Agent（已安装受支持项默认勾选，用户可取消）
→ 逐 Agent 自动解析链路（歧义 fail-closed，不提供 Provider/model/endpoint chooser）
→ 用户一次点击“配置”授权选中 Agent
→ 后台逐 Agent 建立 diff / backup / apply / readback verify / receipt
→ 成功项保持成功，失败项单独回退与重试
→ 验证配置和运行证据
→ 进入 Doctor / 概览 / 日志
```

现有子组件：

- `InstallWorkbench.tsx`：安装与连接主工作台。
- `SmartLearning.tsx`：项目发现、评分、深度扫描和 managed block 方向；未完成 native 事务前必须 fail closed。
- `Advanced.tsx`：高级 d-token 运行参数、网络/可靠性、项目策略和诊断入口；不得提供 Provider/model/protocol/endpoint/cc-switch profile chooser。
- `LogsSecurity.tsx`：日志、安全与导出能力。
- `Compatibility.tsx`：Agent/协议/平台兼容信息。

这些子页面可以通过工作台或内部导航进入，但正式 UI 文案必须始终翻译成用户任务，禁止暴露 workload benchmark、smoke test、fixture、mock、fake-bin 等开发语言。

## 4. 前后端对接清单

以下能力通过 Tauri command 和 d-token local service 对接；恢复基准后的前端必须逐项核对，不把接口存在当作页面已完成：

1. 读取 d-token-owned Dashboard、health、route、provider、instance、日志和 Event Ledger 元数据。
2. 保存 Provider/Route 草稿，拒绝回指本地 listener 的自循环上游。
3. 将草稿应用为 d-token active config，并在写入前创建 rollback point。
4. 使用操作系统凭据库保存 Provider credential，配置中只保留不透明引用。
5. 启动、停止和检查由 App 管理的 d-token Rust runtime，并验证进程、可执行文件、端口和 health identity。
6. 运行安装事务、Doctor v3 统一快照/模块注册表/自动扫描与 Agent 专项修复、预览/确认/自动复检、产品 readiness 聚合和 Agent route 配置验证。
7. 导出经过脱敏的 JSON/CSV runtime 数据。
8. 读取 Optical 观测切片（status/capabilities/lifecycle/events）并运行 shadow preview public summary（Tauri → loopback）。
9. 通过 `get_optical_config_editor` / `preview_optical_config_change` / `apply_optical_config_change` 编辑 active config 的 optical 全局与 route 策略（shadow-only，强制关闭 active）。
10. 在 debug build 中运行内部 OpenCode/benchmark/smoke 验证；release build 不注册这些开发命令。
11. 通过代码图谱页读取 package/integration/agent session/UI session/index 五维证据；先完成用户导入与固定校验，再以显式项目范围驱动本机 Agent 扫描；已安装且受支持 Agent 默认勾选并允许用户取消，按选中 Agent 执行独立 MCP 配置事务、索引与只读图谱页面。按钮不可用时必须就地显示阻塞原因。

## 5. 尚未完成的产品闭环

1. Claude Code、Codex、OpenCode 的已有 d-token 管理区已使用 Agent Integration 完整事务；Cursor、Cline 等 Agent 仍缺专用 executor，首次接入仍归安装与连接。
2. 自动安装 RTK/Headroom 的下载源和签名验证尚未实现；当前缺失时只检测并安全失败。
3. Smart Learning 的真实 native 扫描、深度扫描、delegation 和 managed-block 写入事务尚未闭环。
4. Changes 已分发普通 ConfigPatcher、Agent Integration 与 Optical executor，但不能声称恢复所有外部配置；Route/安装等无来源 executor 或无完整 manifest 的专项 scope 继续锁定。
5. Analytics、Doctor 和 Dashboard 的每项数据都需要继续收敛到统一 source/freshness/action-result 合同。
6. Optical active transform、PNG renderer 生产路径、post-validator 与 rollback executor 仍未开放；视觉压缩页保持 Beta / shadow-only。
7. macOS 和 Windows 仍缺少同一候选 commit 的完整实机发布证据。

## 6. 状态与证据规则

每个用户可见状态至少包含：

- `source`：数据来自 d-token runtime、Tauri 本地状态、浏览器 preview 或不可用。
- `freshness`：采集时间和是否过期。
- `state`：loading、empty、unavailable、partial、ready、error。
- `action result`：用户操作是否真实执行、写入了什么、如何验证、如何回退。

正式异步动作必须及时显示按钮/区域级中间态并防重复提交；成功、部分完成、失败或前置条件不足使用右下角两秒 Toast 提示。失败原因、重试和持续进度保留在对应页面，操作结果与详情可从通知中心查看；批量汇总去重，后台无变化刷新不弹提示。通知使用用户任务语言，只在详情保留必要的脱敏信息。

没有完整证据时，UI 必须显示未知、不可用或部分状态，不能用默认绿色、示例数字或静态工具路径补齐。

## 7. 安全和隐私

- 默认只连接 `127.0.0.1` 上的 d-token-owned service。
- API key、token、cookie、Authorization 和认证文件不得进入普通配置、日志或导出。
- 完整 prompt、response、源代码和命令原始输出不得默认落盘。
- 所有真实写操作必须具备摘要、diff、备份、确认、验证和回退证据。
- 外部 Headroom、standalone RTK 和 cc-switch 只能在明确边界内检测或对照，不得成为 App 的隐式运行依赖。

## 8. 页面验收

页面验收必须同时满足：

1. React/Vitest 行为测试。
2. TypeScript 与 Vite 构建。
3. 浏览器 preview 的 fail-closed 状态检查。
4. 真实 Tauri 窗口的交互、布局、键盘和主题检查。
5. 对涉及 runtime 的页面，提供 d-token-owned service、route decision、Event Ledger 或导出证据。

仅看到页面正常显示，不足以证明安装、配置、路由、压缩、Doctor 或回退功能已经生效。
