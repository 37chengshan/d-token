# d-token App 功能与页面

更新时间：2026-07-11 晚间
产品真理源：`docs/PRD.md`
代码入口：`code/apps/desktop/src/app/`

## 1. 当前信息架构

正式桌面 App 使用六个一级页面：

```text
总体概览
安装与连接
分析
Doctor
变更与回退
设置
```

实际一级导航定义在 `code/apps/desktop/src/app/lib/nav.ts`。代码中仍存在 `advanced`、`logs`、`compatibility` 等 PageId 和页面组件，它们是安装工作台子流程、内部跳转或既有实现来源，不应重新扩张为工程化顶级导航。

## 2. 页面与代码映射

| 正式页面 | 主要代码 | 用户任务 | 当前边界 |
|---|---|---|---|
| 总体概览 | `components/pages/Overview.tsx` | 查看 d-token 服务、运行证据、关键指标、近期问题和下一步动作 | 只有 Tauri + d-token-owned runtime 的完整新鲜证据才能显示为真实状态；浏览器预览必须明确标记 |
| 安装与连接 | `Install.tsx`、`InstallWorkbench.tsx` | 配置 d-token 服务、选择 Agent、建立 Provider/Route、预览变更、应用、验证 | 当前能保存和应用 d-token 自有 route config；第三方 Agent 自动写入尚未形成完整生产事务 |
| 分析 | `Analytics.tsx` | 查看请求、token、路由、延迟和错误的聚合趋势 | 不得把 fixture 或推算值冒充真实账单；估算必须标注 |
| Doctor | `Doctor.tsx` | 检查服务、配置、路由、凭据、平台和已知问题，给出修复建议 | 低风险动作可自动执行；涉及配置写入、外部工具或危险修复时必须预览并确认 |
| 变更与回退 | `Changes.tsx` | 查看 d-token 自有变更、rollback point、恢复和清理 | 当前可恢复 d-token active config；外部 Agent 配置恢复仍受限 |
| 设置 | `Settings.tsx` | 管理主题、语言、本地端口、安全、项目策略和应用偏好 | 设置只写 d-token 本地状态，不存储明文凭据或原始 prompt/response |

## 3. 安装与连接工作台

安装页承担完整用户主路径，而不是测试中心：

```text
确认服务状态
→ 选择 Agent
→ 配置 Provider / Route
→ 预览摘要与 diff
→ 建立备份/回退点
→ 明确应用
→ 验证配置和运行证据
→ 进入 Doctor / 概览 / 日志
```

现有子组件：

- `InstallWorkbench.tsx`：安装与连接主工作台。
- `SmartLearning.tsx`：项目发现、评分、深度扫描和 managed block 方向；未完成 native 事务前必须 fail closed。
- `Advanced.tsx`：高级 Provider、Route、项目策略和实验能力入口。
- `LogsSecurity.tsx`：日志、安全与导出能力。
- `Compatibility.tsx`：Agent/协议/平台兼容信息。

这些子页面可以通过工作台或内部导航进入，但正式 UI 文案必须始终翻译成用户任务，禁止暴露 workload benchmark、smoke test、fixture、mock、fake-bin 等开发语言。

## 4. 当前已接入能力

桌面 App 当前可以通过 Tauri command 和 d-token local service 使用以下能力：

1. 读取 d-token-owned Dashboard、health、route、provider、instance、日志和 Event Ledger 元数据。
2. 保存 Provider/Route 草稿，拒绝回指本地 listener 的自循环上游。
3. 将草稿应用为 d-token active config，并在写入前创建 rollback point。
4. 使用操作系统凭据库保存 Provider credential，配置中只保留不透明引用。
5. 启动、停止和检查由 App 管理的 d-token Rust runtime，并验证进程、可执行文件、端口和 health identity。
6. 运行安装事务、Doctor、产品 readiness 聚合和 Agent route 配置验证。
7. 导出经过脱敏的 JSON/CSV runtime 数据。
8. 在 debug build 中运行内部 OpenCode/benchmark/smoke 验证；release build 不注册这些开发命令。

## 5. 尚未完成的产品闭环

1. 对 OpenCode、Claude Code、Codex、Cursor、Cline 等第三方 Agent 的配置写入尚未统一为完整事务。
2. 自动安装 RTK/Headroom 的下载源和签名验证尚未实现；当前缺失时只检测并安全失败。
3. Smart Learning 的真实 native 扫描、深度扫描、delegation 和 managed-block 写入事务尚未闭环。
4. Changes 页面不能声称已能恢复所有外部配置；当前可靠范围主要是 d-token 自有 active config。
5. Analytics、Doctor 和 Dashboard 的每项数据都需要继续收敛到统一 source/freshness/action-result 合同。
6. macOS 和 Windows 仍缺少同一候选 commit 的完整实机发布证据。

## 6. 状态与证据规则

每个用户可见状态至少包含：

- `source`：数据来自 d-token runtime、Tauri 本地状态、浏览器 preview 或不可用。
- `freshness`：采集时间和是否过期。
- `state`：loading、empty、unavailable、partial、ready、error。
- `action result`：用户操作是否真实执行、写入了什么、如何验证、如何回退。

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
