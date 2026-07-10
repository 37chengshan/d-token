# M4 Real Capability Integration Plan — OpenCode Test Track

状态：Reviewed draft — multidimensional review applied on 2026-07-04
关联：`docs/PRD.md`、`docs/PLAN.md`、`docs/ARCHITECTURE.md`、`docs/SECURITY_BOUNDARIES.md`
目标：把当前 desktop fixture UI 逐步接入真实本地能力，并优先使用用户原本 OpenCode 配置做真实 routed test，避免误改 Claude Code、Codex、Cursor、Cline、外部 Headroom、RTK、cc-switch 或系统代理配置。

## 1. 一句话目标

M4 先不追求一次性接完整 Headroom/RTK/所有 agent，而是先做一条可控、可观测、可回滚的真实链路：

```text
OpenCode 原本配置
  -> d-token provider/baseURL
  -> d-token local service
  -> d-token Headroom-derived runtime
  -> d-token route adapter
  -> configured official/custom/cc-switch upstream
  -> Dashboard / Doctor / Logs 真实展示
```

OpenCode 是第一条真实测试线。用户已授权它直接使用现有 OpenCode 配置和真实 provider/model，默认不隔离 HOME、不替换 fake config、不另造临时 OpenCode 配置，也不限制为官方免费模型；OpenCode-only / fake-bin / direct command 只保留为诊断辅助，不代表产品只支持 OpenCode。

## 多维度审查结论

本轮审查结论：计划方向正确，但必须把“能跑通 OpenCode”升级成“可证明没有碰坏用户环境、可证明流量走了 d-token、可证明 UI 展示的是同一条事件链”。因此 M4 不应直接进入 managed 写入，而应先补齐以下闸门。

| 维度 | 结论 | 必须补强 |
|------|------|----------|
| 产品目标 | OpenCode 原配置 routed test 是第一条真实链路，但不能让用户误以为已经完整接入所有 agent。 | UI 明确展示当前模式：Fixture / Shadow / OpenCode Real Route / Managed。 |
| 安全边界 | 现有计划符合本地优先、127.0.0.1、无静默 fallback 的方向。 | 每个阶段都要有 no-touch 证据：未改其它 agent 配置、未碰外部 Headroom/RTK/cc-switch、未设置系统代理、未保存完整 prompt/output。 |
| 技术落点 | 代码里已经有 `test_opencode` 与 `run_opencode_test_capture` 雏形。 | 先把它拆成可单测 helper，再接 UI；不要继续把真实逻辑塞在 Tauri command 里。 |
| 数据闭环 | 计划提到 Dashboard / Logs / Ledger，但缺少“同一 request id 串起来”的硬要求。 | OpenCode 测试、route decision、provider result、UI card 必须共享 request id / run id。 |
| 测试可验证性 | fake-binary 与 direct command 可作为诊断，但验收必须看 real routed OpenCode。 | 加 no-touch regression：Fixture 模式不得 spawn `opencode`；Shadow 模式不得写文件；OpenCode routed test 不得继承危险代理变量或修改其它 agent。 |
| 用户体验 | Install / Doctor / Logs 三处入口合理。 | 失败分类必须用大白话展示，例如“OpenCode 没装”“模型不可用”“登录失效”，不要只显示 exit code。 |
| 后续扩展 | 先 passthrough 再压缩是正确顺序。 | 压缩接入前必须冻结 token denominator，避免 Dashboard savings 双计。 |

### 审查后的硬闸门

1. **No-touch Gate**：默认 Fixture 模式下，任何 UI 打开、刷新、切换页面都不得调用真实 `opencode`、不得读写 agent 配置。
2. **Shadow Gate**：Shadow 只允许读取 binary 路径、版本、端口 health 和 d-token 自己的状态；不允许写配置、不允许启动/停止真实服务。
3. **OpenCode Real Route Gate**：OpenCode 真实测试使用用户原本配置；如需增加 d-token provider/baseURL，必须先备份原配置、只写最小变更、记录 backup 和 diff 摘要，并移除 Anthropic/Claude 代理变量。
4. **Ledger Gate**：一条真实 routed test 必须至少落 4 类事件：`agent_request`、`runtime_compress`、`route_decision`、`provider_result`。direct command 测试如果保留，只能标记 `route=none/direct-command`，不能作为真实接入完成证据。
5. **Privacy Gate**：日志只保存摘要、exit code、错误分类、token 统计、耗时、request id；不保存完整 prompt、完整 stdout/stderr、完整 response body。
6. **Rollback Gate**：任何 d-token-owned 写入都必须进入 InstallationTransaction；OpenCode 配置例外仅限用户批准的真实 routed test provider/baseURL 最小变更，并必须先备份。M4 不允许写 Claude/Codex/Cursor/Cline/Roo/Continue/Windsurf 配置。
7. **UI Truth Gate**：Dashboard 徽标必须显示数据来源，不能把 fixture 图表和真实 OpenCode 结果混成一种状态。

## 2. 总体原则

1. 默认仍是 Fixture Mode，不碰真实环境。
2. 第一批真实能力走 Shadow Mode，只读检测，不写配置、不启动未知服务、不 kill 进程。
3. OpenCode routed test 使用用户原本 OpenCode 配置和真实 provider/model；默认不隔离 HOME、不替换 fake config、不另造临时 OpenCode 配置。
4. OpenCode 测试不得写入 Claude Code、Codex CLI、Cursor、Cline、Roo、Continue、Windsurf、外部 Headroom、RTK、cc-switch 或系统代理配置。
5. 所有真实写操作必须进入 InstallationTransaction。
6. 所有请求路径必须能在 Logs / Event Ledger / Dashboard 里还原。
7. 任何 provider fallback 默认禁用，不允许静默走官方上游。
8. 默认只监听 `127.0.0.1`。
9. 不保存 prompt、代码、完整命令输出和完整 response body；只保存脱敏元数据与统计。
10. 所有真实测试都必须能从 UI 看到“这次到底有没有经过 d-token route”；没有经过 route 时必须明确显示为 direct command test。

## 3. 当前可利用基础

代码中已有一些 M3/M4 雏形，可以继续利用：

- `code/apps/desktop/src-tauri/src/main.rs`
  - `run_opencode_test_capture(...)`
  - `test_opencode(model: String)`
  - OpenCode direct command helper（仅作诊断辅助）
  - `NO_PROXY=127.0.0.1,localhost`
  - 移除 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_URL`、`CLAUDE_CODE_PROXY`
- `code/crates/dtoken-doctor`
  - no-touch doctor 基础
- `code/crates/dtoken-headroom-adapter`
  - fixture / shadow / managed mode 边界
- `code/crates/dtoken-preprocessor`
  - fixture / shadow / managed mode 边界
- `code/crates/dtoken-core-api`
  - Dashboard/Compatibility/Route/Provider mock API 基础
- `support/fake-bin`
  - fake-binary 测试资产

## 4. 分阶段执行

### M4A. Shadow 只读检测

目标：让 App 真的知道本机有什么，但不改任何东西。

范围：

- 检测 `opencode` 是否存在。
- 检测 `opencode --version` 或等价版本命令。
- 检测 `rtk` / `headroom` / `cc-switch` 是否存在，仅记录路径和版本。
- 检测候选本地端口：
  - `8787`: Headroom / Anthropic-compatible proxy 候选
  - `18001`: d-token local service / OpenCode routed test
  - `8123`: cc-switch 候选
  - `8080`: custom provider 候选
- 检测 `127.0.0.1` health endpoint，不扫描公网。
- 检测 Windows `.cmd` / `.exe` / `.bat` shim。
- 检测 UTF-8 输出是否正常。

不做：

- 不读取 API key 明文。
- 不读取完整 auth/session 文件。
- 不修改 agent config。
- 不修改 shell profile。
- 不启动或停止真实服务。
- 不写 provider route。

交付：

- Install 页面“一键检测环境”使用真实 Shadow 检测结果。
- Doctor 页面显示 Shadow 检测结果。
- Logs 页面显示检测过程。
- StatusBar 显示当前模式：`Fixture` / `Shadow`。

DoD：

- `DTOKEN_PROFILE=fixture` 时不调用真实命令。
- `DTOKEN_PROFILE=shadow` 且用户点击检测时，才允许只读调用真实 binary。
- 检测失败只给 warn/error，不中断 UI。
- 本地测试覆盖缺失 OpenCode、存在 OpenCode、命令超时、GBK/UTF-8 输出四类情况。

### M4B. OpenCode 原配置真实 routed test

目标：用用户原本 OpenCode 配置做第一条真实 agent routed test。

当前定位：M4B 必须证明 OpenCode 请求进入 d-token local service，再由 d-token 完成压缩、路由和 upstream 转发。OpenCode direct command / fake-bin 只作为诊断工具，不能作为“d-token 已经接管流量”的证据。

真实测试链路：

```text
OpenCode
  -> provider/model: dtoken/<model> or existing real OpenCode model routed through d-token
  -> baseURL: http://127.0.0.1:18001/v1
  -> d-token /v1/chat/completions or /v1/responses
  -> d-token route adapter
  -> configured real upstream from the user's OpenCode setup
```

允许的 OpenCode 配置变更：

```text
add provider: dtoken
map model: dtoken/<existing-real-model> or dtoken/<user-selected-model>
set baseURL: http://127.0.0.1:18001/v1
```

要求：

- 修改前备份 `C:\Users\10777\.config\opencode\opencode.json`。
- 只写 provider/model/baseURL 最小变更。
- 不读取、不打印、不提交 OpenCode 密钥。
- 不修改其它 agent 配置。
- 不启动、停止、重启或依赖外部 Headroom / RTK / cc-switch。
- 运行环境移除 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_URL`、`CLAUDE_CODE_PROXY`。

UI 入口：

- Install 页面：`OpenCode 真实路由测试`
- Advanced 页面：OpenCode test route
- Doctor 页面：OpenCode route check
- Logs 页面：OpenCode routed result

测试通过标准：

- OpenCode 命令能执行并选择用户原本配置中可用的真实模型；如为了证明 routed path 新增 `dtoken/<model>` 映射，该映射必须指向用户已有真实上游。
- d-token local service 收到请求。
- d-token 记录 compression / route decision / provider result。
- upstream 返回内容包含测试期望值，或返回明确的 provider/model 错误。
- 日志明确写出：
  - model
  - d-token service URL
  - route id
  - provider id
  - exit status
  - sanitized stderr/stdout summary
- 没有修改其它 agent 配置。
- 没有设置系统级代理。

失败分类：

| 失败 | UI 分类 | 建议 |
|------|---------|------|
| `opencode` 不存在 | missing_tool | 提示安装 OpenCode |
| model 不存在 | model_error | 换模型或检查 provider |
| provider 401/403 | provider_auth | 检查 OpenCode 自身登录/凭据 |
| 网络/超时 | network_timeout | 检查代理/网络 |
| 输出不等于 `d-token-ok` | unexpected_response | 展示脱敏输出摘要 |
| 命令崩溃 | command_failed | 展示 exit code 和脱敏 stderr |

DoD：

- OpenCode routed test 可以在 UI 或脚本手动触发。
- 测试结果进入 Event Ledger，stage 至少覆盖 `runtime_compress`、`dtoken_route_decision`、`provider_forward`。
- 每次测试生成 `request_id` / `run_id`，Install terminal status、Doctor、Logs、Dashboard 使用同一个 id 展示。
- 不保存完整 prompt 和完整输出，只保存摘要。

### M4C. OpenCode route hardening

目标：把已经跑通的 OpenCode routed path 做成稳定、可重复、可观测的真实测试链路。

定位：M4C 不再证明“是否经过 d-token”，而是强化 route profile、provider profile、错误分类、fallback disabled、持久化和 Dashboard 一致性。

建议测试 route：

```text
RouteProfile:
  id: opencode-test
  input_protocol: openai
  output_protocol: same_as_input
  agent_id: opencode
  chain_mode: direct
  bind_host: 127.0.0.1
  local_port: 18001
  fallback_policy: disabled
```

Provider:

```text
ProviderProfile:
  id: opencode-zen or user-configured-upstream
  kind: official | custom | cc_switch
  protocol: openai
  base_url: configured upstream URL
  api_key_ref: user OpenCode/upstream config reference, never logged
```

实现顺序：

1. RouteProfile / ProviderProfile 落本地 d-token config。
2. d-token proxy 接受 OpenAI-compatible request。
3. 先压缩，再记录 route decision。
4. 转发到 configured upstream。
5. Dashboard 展示 OpenCode real route。
6. 再扩展 cc-switch / custom provider profile。

DoD：

- OpenCode test route 出现在 Dashboard。
- 每次测试都有 request id。
- request id 能串起 proxy request、route decision、provider result、Logs 行和 Dashboard 卡片。
- fallback 显示为 disabled；如果 provider 不通，必须失败并分类，不允许改走官方上游。
- Logs 能显示：
  - agent=OpenCode
  - route=opencode-test
  - provider=opencode-test-provider
  - fallback=disabled
  - protocol=openai

### M4D. Dashboard 真实数据接入

目标：让现在的 App UI 从 fixture 数据切到真实 ledger/API 数据。

优先接入卡片：

1. Runtime running/stopped。
2. OpenCode availability。
3. OpenCode test result。
4. Route decision count。
5. Provider error count。
6. Token usage summary。
7. Tool/command test logs。
8. Doctor status。

数据流：

```text
Tauri command / local API
  -> Rust core
  -> JSON response
  -> React query/state
  -> Dashboard cards
```

保留：

- Fixture Demo Mode。
- 空数据占位。
- “Fixture 数据”徽标改为动态：
  - Fixture 数据
  - Shadow 只读
  - OpenCode 测试
  - Managed 真实

DoD：

- 没有真实数据时 UI 不崩。
- 切换模式时卡片显示来源。
- OpenCode 测试后 Dashboard 能看到最新状态。
- Fixture 图表和真实 OpenCode 测试结果必须在 UI 上明确区分，避免“半真半假”。

### M4E. InstallationTransaction 接入真实写入前置

目标：后续任何写配置前，都先经过事务系统。

事务步骤：

```text
scan
plan
show diff
backup
confirm
apply
verify
doctor
commit rollback point
```

OpenCode 第一批写入只允许写 d-token 自己的配置：

- d-token local config
- d-token route profile
- d-token provider profile
- d-token ledger
- d-token rollback manifest

明确不写：

- OpenCode 自身配置
- Claude Code 配置
- Codex CLI 配置
- Cursor 配置
- shell profile
- 系统代理

DoD：

- `apply` 前必须能看到 diff。
- `apply` 后生成 rollback point。
- `rollback` 能删除本次 d-token-owned config。
- 失败时不会影响其它 agent。

### M4F. Compression pipeline 接入

目标：OpenCode 测试链路跑通后，再接真实压缩。

顺序：

1. passthrough route + ledger。
2. basic preprocessor：ANSI 清理、重复行折叠、超长输出截断。
3. Headroom-compatible compression adapter。
4. integrated RTK metrics bridge。
5. layered savings attribution。

不先做：

- CCR 原文读取。
- prompt/full response 保存。
- standalone RTK 用户路径。

DoD：

- Dashboard 区分：
  - raw tokens
  - preprocessed tokens
  - runtime final tokens
  - provider input tokens
- 禁止把 `rtk_savings + runtime_savings` 直接当总 savings。

## 5. UI 调整计划

### 全 App 按钮真实接入原则

审查新增要求：App 里所有看起来能执行动作的按钮，M4 起必须满足三选一。

1. **真实接入**：按钮调用 Tauri command / local API，并把结果写入 Logs 或 Event Ledger。
2. **明确预览**：按钮只展示 diff / preview / fixture 时，按钮文案和徽标必须写明“预览”或“Fixture”，不能伪装成真实执行。
3. **禁用说明**：真实能力尚未实现时，按钮禁用，并展示简短原因与预计接入阶段。

不允许：

- 点击后只 `setTimeout` 模拟成功，但 UI 文案像真实操作。
- 没有 `onClick` 的执行型按钮保持可点击外观。
- 把 fixture 数据和真实结果混在同一个按钮反馈里。
- 真实动作失败后只静默关闭弹窗。

按钮状态标准：

| 状态 | UI 表现 | 行为 |
|------|---------|------|
| `fixture-preview` | 显示 `Fixture/预览` 徽标 | 只改前端本地状态，不写 ledger，不称为成功 |
| `shadow-ready` | 显示 `只读` 徽标 | 允许调用只读 scan / doctor |
| `real-ready` | 普通主按钮 | 调用真实 command/API，写事件 |
| `blocked` | 禁用 + tooltip/说明 | 告诉用户缺少 OpenCode、provider、权限或事务能力 |
| `danger-confirm` | 二次确认 | 涉及 stop / rollback / clean / apply |

### 按钮接入矩阵

| 页面 | 当前按钮/控件 | M4 接入要求 | 没接好前 UI |
|------|---------------|-------------|-------------|
| Install | 一键安装 / 重启 / 停止 | 接 `run_install_transaction` 或 `manage_headroom`，并写 `install_step` 事件 | 显示 `Fixture 预演`，不要写“安装完成” |
| Install | 一键检测环境 | 接 Shadow scan：`detect_tools` + 端口 health + OpenCode version | `只读检测` 徽标 |
| Install | 配置选中 agent / 确认配置 | 接 InstallationTransaction diff/confirm/apply；M4A/M4B 不允许写其它 agent | 默认禁用真实确认，只允许“查看预览 diff” |
| Install | OpenCode 真实路由测试 | 调用真实 OpenCode 配置里的可用模型，或调用最小新增的 `dtoken/<model>` 映射；请求必须经过 d-token `:18001` | OpenCode 缺失、d-token service 未运行或 route 未配置时禁用并提示 |
| Doctor | 运行全部检查 | 接 `run_doctor`，结果进入 Logs/Doctor cards | 未接时显示“Fixture 检查” |
| Doctor | 修复 | 只导航到对应页面或打开事务计划，不直接修复 | 文案改成“查看修复计划” |
| Doctor | 重启/发布/重建/清理确认 | 分别接 `manage_headroom`、release check、rebuild route、cleanup transaction | 真实清理前禁用或二次确认 |
| Advanced | 自动启动 switch | 接 `save_app_settings`，只保存 d-token 自己设置 | 保存失败回滚 UI |
| Advanced | bind host / port input | 接 settings draft + validation，不直接启动服务 | 显示“未应用”状态 |
| Advanced | cc-switch 自动 / 模式 | 接 route config draft；apply 必须走 transaction | 只保存草稿，不改真实 cc-switch |
| Advanced | 扫描项目 | 接 project shadow scan / policy recommender | 未接前按钮写“预览扫描” |
| Advanced | 应用策略 | 接 ProjectCompressionPolicy transaction | 未接前禁用，展示推荐结果即可 |
| Changes | 拒绝 / 批准 | 接 InstallationTransaction pending changes | 没有真实 pending change 时禁用 |
| Changes | 回滚 | 接 `manage_backup` rollback + doctor verify | 必须二次确认 |
| Logs | 导出 | 接 redacted export，本地生成文件 | 未实现前禁用并说明 |
| Logs | 密钥轮换 / 测试 | 第一版不直接轮换密钥；测试只验证存在/脱敏状态 | 禁用轮换；测试走 provider health check |
| Settings | 主题/语言/颜色/玻璃/背景速度 | 已是前端真实设置，继续保留并持久化 | 若未持久化，标记“本次会话” |
| Settings | Runtime mode | 接真实 mode state：Fixture / Shadow / OpenCode Real Route / Managed | 未接前 Shadow/Managed 禁用 |
| Settings | Redaction level | 接 `save_app_settings` 并影响日志导出 | 未接前显示“即将支持” |
| Settings | Telemetry switch | 默认关闭；M4 不开启远端 telemetry | 禁用或仅本地诊断 |
| Topbar | 咖啡 / GitHub / 工具 / cast / 通知图标 | 接具体弹窗/链接/通知中心；没有功能就隐藏或禁用 | 不能保留无动作图标 |
| StatusBar | 详情展开 | 接真实 runtime/route/doctor 摘要 | Fixture 模式显示数据来源 |

### 按钮接入实施顺序

1. 先做统一 `ActionState` / `CapabilityState`，每个按钮能知道自己是 fixture、shadow、real、blocked 还是 danger。
2. 把所有执行型按钮包一层 `CapabilityButton`，统一 loading、disabled reason、tooltip、event logging。
3. 优先接 M4A/M4B 真能力：
   - Install 一键检测环境。
   - Install OpenCode 真实路由测试。
   - Doctor 运行全部检查。
   - Logs 导出脱敏日志。
   - Settings runtime mode。
4. 再接 M4C/M4E：
   - Advanced route/provider draft。
   - Changes approve/reject/rollback。
   - Install configure/apply。
5. 最后处理顶部图标与低频维护动作；没有真实功能的图标先隐藏，避免干扰。

DoD：

- `rg "onClick=\\{\\(\\) => \\{\\}\\}" code/apps/desktop/src/app` 无执行型占位。
- `rg "setTimeout" code/apps/desktop/src/app/components/pages` 中只允许动画/演示，不能用于伪装真实成功。
- 每个执行型按钮都有 disabled/loading/error/success 四种状态。
- 每个真实按钮点击后至少产生一条 Logs 或 Event Ledger 记录。
- Fixture 模式按钮文案必须包含 Fixture/预览/只读等来源提示。

### Install 页面

- “一键检测环境”接 Shadow scan。
- “OpenCode 真实路由测试”按钮。
- 两行 terminal status 显示真实步骤：

```text
> opencode dtoken/<model> -> d-token 127.0.0.1:18001 running
> compression -> route decision -> upstream -> ledger -> dashboard
```

### Overview 页面

- 活跃趋势继续可展示 fixture / real mixed。
- Token 图表优先接 OpenCode test result。
- 工具排行显示 command/test/tool usage。

### Advanced 页面

- Provider / Route 从 fixture 改为 d-token local config。
- 新增 `opencode-test` route 状态。

### Doctor 页面

- 增加 OpenCode 专区：
  - binary found
  - version
  - d-token provider configured
  - service URL `127.0.0.1:18001`
  - model response
  - no global proxy changed

### Logs 页面

- 展示 OpenCode 测试日志。
- 默认脱敏。
- 支持按 `opencode / route / provider / doctor` 筛选。

### Settings 页面

- Runtime mode：
  - Fixture
  - Shadow
  - OpenCode Real Route
  - Managed
- OpenCode 默认模型选择。
- d-token service port，默认 `18001`。

## 6. 测试计划

审查后测试分层：先证明“不碰其它环境”，再证明“OpenCode 原配置能真实请求 d-token”，最后证明“Dashboard/Logs/Ledger 是同一条请求链”。

### Unit tests

- opencode command builder。
- routed test env sanitizer。
- output sanitizer。
- route decision event。
- provider error classifier。
- rollback manifest。
- no-touch mode guard：Fixture 模式下禁止 spawn real binary。
- run_id/request_id generator 与日志关联。

### Fake-binary tests

使用 `support/fake-bin/opencode` 做诊断辅助，不作为真实 routed path 验收：

1. success：输出 `d-token-ok`。
2. missing model。
3. auth error。
4. timeout。
5. GBK/UTF-8 mixed output。
6. huge output summarization。

### Real OpenCode manual test

前置条件：

- 用户机器安装 OpenCode。
- 用户自己完成 OpenCode 登录或 provider 配置。
- d-token 不读取 OpenCode 密钥。
- OpenCode 使用用户原本配置里的真实 provider/model；若为了验证 routed path 需要新增 `dtoken` provider，则 baseURL 指向 `http://127.0.0.1:18001/v1`，并先备份原配置。

手动流程：

1. 打开 App。
2. 切到 Shadow / OpenCode Real Route。
3. 点击检测环境。
4. 选择当前 OpenCode 原配置中可用的真实模型，或选择已映射到 d-token 的 `dtoken/<model>`。
5. 点击 OpenCode 真实路由测试。
6. 查看 Install terminal status。
7. 查看 Doctor。
8. 查看 Logs。
9. 查看 Dashboard。

成功标准：

- OpenCode 返回 `d-token-ok` 或明确的 provider/model 错误。
- d-token dashboard payload 中出现同一 request id。
- Event Ledger 有 compression、route decision、provider forward 记录。
- App 不改其它 agent 配置。
- Logs 无明文 key。
- Dashboard 能看到 OpenCode test event。

### Regression tests

- Fixture mode 不调用真实 `opencode`。
- Shadow mode 不写 config。
- OpenCode real route test 不设置系统代理。
- OpenCode real route test 不继承 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_URL`、`CLAUDE_CODE_PROXY`。
- OpenCode direct command / fake-bin test 不伪装成 routed request。
- no-touch smoke 保持通过。
- desktop build 通过。

### Review evidence

每个 PR 合并前需要贴出这些证据：

1. `cargo test --workspace`。
2. `npm run build` in `code/apps/desktop`。
3. fake-binary OpenCode success / missing / auth / timeout / huge output 诊断用例结果。
4. Fixture mode no-touch 证明：没有真实 `opencode` spawn，没有 agent config diff。
5. 日志脱敏样例：无 API key、无完整 prompt、无完整 stdout/stderr。
6. OpenCode real route：同一 request id 出现在 compression、route decision、provider result、Logs、Dashboard。

## 7. 安全检查清单

- [ ] 只绑定 `127.0.0.1`。
- [ ] OpenCode real route 如需写 OpenCode 配置，必须先备份，只写 d-token provider/baseURL 最小变更。
- [ ] 不读 API key 明文。
- [ ] 不保存 full prompt。
- [ ] 不保存 full response。
- [ ] 不保存 full command output。
- [ ] 日志默认脱敏。
- [ ] 不设置系统代理。
- [ ] 不修改 Claude Code / Codex / Cursor / Cline。
- [ ] provider fallback 默认 disabled。
- [ ] route decision 必须进入 ledger。
- [ ] `apply` 前必须 show diff。
- [ ] 写入前必须 backup。
- [ ] 失败必须 rollback。

## 8. 已确认默认值

用户已确认：OpenCode 可以直接使用原本配置做真实测试，目标是真实 routed path，不需要隔离环境。

1. OpenCode 第一批真实测试使用用户原本 OpenCode 配置里的真实 provider/model；不限制为官方免费模型。若当前已存在可用的 `opencode/deepseek-v4-flash-free` 或其它模型，可以直接沿用；如果为了证明 routed path 需要 `dtoken/<model>` 映射，必须只做最小配置并记录备份。
2. d-token service 测试端口默认固定用 `18001`，不占用外部 Headroom `8787`。
3. 第一版只保存 OpenCode 命令输出摘要、状态和指标，不保存完整 stdout/stderr、prompt 或 response。
4. Provider 真实接入优先级：
   - OpenCode 自身已有 provider
   - cc-switch
   - 自定义 OpenAI-compatible URL
   - 自定义 Anthropic-compatible URL
5. 第一版 Windows native 优先，WSL 暂时只检测不写入。
6. 真实 OpenCode 测试不得读取、打印、提交或持久化 OpenCode 密钥；不得修改其它 agent 配置、外部 Headroom、RTK、cc-switch、系统代理或 shell profile。

## 9. 推荐执行顺序

```text
1. 整理 OpenCode test command + fake-bin 测试
2. 接 Shadow scan 到 Install/Doctor
3. 配置/校验 OpenCode `dtoken` provider 指向 d-token `:18001`
4. 接 OpenCode 真实路由测试按钮
5. 写 compression / route / provider Event Ledger 事件
6. Dashboard 从 d-token runtime payload 读真实状态
7. 强化 ProviderProfile / RouteProfile
8. 最后扩展 cc-switch / custom provider / RTK metrics
```

这个顺序的好处是：每一步都有可见结果，也不会一下子碰到 provider、proxy、压缩、agent config 四个雷区。

## 10. M4A/M4B 首批交付 DoD

第一批 PR 建议只做到：

- [ ] Shadow scan command。
- [ ] OpenCode real route helper，并从 Tauri command 中拆出可单测逻辑。
- [ ] fake-bin OpenCode 诊断测试。
- [ ] Install 页面 OpenCode 真实路由测试入口。
- [ ] Doctor 页面 OpenCode 检查结果。
- [ ] Logs 显示脱敏测试日志。
- [ ] Fixture mode no-touch regression。
- [ ] `run_id` 串起 Install / Doctor / Logs。
- [ ] UI 明确显示 fake-bin/direct command 只是诊断辅助，不是 routed request。
- [ ] `npm run build`、`cargo test`、no-touch smoke 通过。

不在第一批 PR 做：

- 修改 OpenCode 配置中非 d-token provider 的任何内容。
- 修改任何其它 agent 配置。
- 保存完整 prompt/output。
