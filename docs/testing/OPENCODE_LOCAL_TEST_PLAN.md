# OpenCode Local New-Port Test Plan

状态：PRD companion document  
归属：本文件补充 `docs/PRD.md` 中关于 OpenCode 本地真实联调的验收要求。后续如 PRD 需要合并本文件内容，应以本文件为准。

## 1. 背景

d-token 第一阶段需要验证真实 agent 链路，而不是只验证 fixture UI。OpenCode 应作为 P0 测试 agent，用来验证：

```text
OpenCode
  ↓
d-token local test entry
  ↓
d-token Headroom-derived runtime / compression layer
  ↓
d-token route adapter
  ↓
custom provider URL or cc-switch
  ↓
final upstream provider
```

## 2. 核心要求

### 2.1 必须新开独立测试端口

本地真实联调不得复用用户已有的 Headroom、cc-switch、OpenCode、Claude、Codex 或生产 route 端口。

每次 OpenCode 本地测试必须使用新的 d-token 测试端口，并满足：

1. 默认绑定 `127.0.0.1`。
2. GUI / CLI 明确展示端口号。
3. Event Ledger 记录端口号和 route id。
4. Dashboard 标记为 `test route`，避免和生产 route 混淆。
5. 测试结束后释放端口。
6. 如果端口占用，自动换用新的测试端口或提示用户选择。
7. 禁止在未验证进程身份的情况下 kill 端口占用进程。

### 2.2 OpenCode 是 P0 E2E 测试 agent

OpenCode 必须进入 P0 测试矩阵，优先验证以下能力：

1. OpenCode binary / shim 检测。
2. OpenCode base URL 注入或测试配置生成。
3. d-token runtime 是否收到请求。
4. compression 是否执行。
5. route decision 是否正确。
6. custom provider URL 是否收到请求。
7. cc-switch 是否收到二次转发请求。
8. Dashboard 是否能看到请求。
9. Doctor 是否能解释失败原因。
10. 测试端口是否在结束后释放。

## 3. 必须覆盖的两条测试链路

### 3.1 自定义 provider URL 链路

```text
OpenCode
  ↓
d-token test port
  ↓
d-token compression runtime
  ↓
d-token route adapter
  ↓
custom OpenAI-compatible or Anthropic-compatible provider URL
```

验收条件：

1. OpenCode 请求进入 d-token 测试端口。
2. Dashboard 显示 agent = OpenCode。
3. Dashboard 显示 route kind = custom provider。
4. Dashboard 显示最终 provider base URL。
5. 若 provider 返回 401，Doctor 显示 route/provider mismatch 或 key invalid，不得误判为 d-token runtime 崩溃。

### 3.2 cc-switch 二次转发链路

```text
OpenCode
  ↓
d-token test port
  ↓
d-token compression runtime
  ↓
d-token route adapter
  ↓
cc-switch
  ↓
final upstream provider
```

验收条件：

1. d-token 先执行压缩，再转发给 cc-switch。
2. Dashboard 显示 route kind = cc-switch。
3. Dashboard 显示 cc-switch endpoint。
4. Dashboard 显示 final upstream provider。
5. Event Ledger 记录 `dtoken_route_decision`、`cc_switch_forward`、`provider_forward`。
6. 不允许因为 cc-switch 配置缺失静默改走官方 provider。

## 4. 建议端口策略

端口策略不应硬编码单个端口，建议：

```text
preferred_start_port: 18180
scan_range: 18180-18250
bind_host: 127.0.0.1
route_tag: opencode_local_test
```

如果 `18180` 被占用，自动尝试下一个可用端口。端口选择结果必须展示给用户。

## 5. GUI 要求

GUI 需要新增或预留：

1. OpenCode Test Runner 页面或面板。
2. New test port 按钮。
3. 自定义 provider URL 测试选项。
4. cc-switch route 测试选项。
5. Start / Stop test route。
6. Route chain probe。
7. Live request log。
8. Doctor explanation。
9. Export test report。

## 6. Event Ledger 字段要求

OpenCode 本地测试事件至少需要：

```ts
interface OpenCodeLocalTestEvent {
  event_id: string;
  timestamp_ms: number;
  agent_id: 'opencode';
  route_id: string;
  local_port: number;
  is_test_port: true;
  route_kind: 'custom_provider' | 'cc_switch';
  target_provider_id?: string;
  cc_switch_provider_id?: string;
  status: 'ok' | 'failed' | 'skipped';
  error_code?: string;
  metadata?: Record<string, unknown>;
}
```

默认不得记录完整 prompt、完整 response、完整代码文件、完整命令输出。

## 7. Doctor 检查项

Doctor 必须检查：

1. 测试端口是否绑定在 `127.0.0.1`。
2. 测试端口是否与生产端口冲突。
3. OpenCode 是否可执行。
4. OpenCode Windows shim 是否可执行。
5. d-token runtime 是否收到请求。
6. custom provider URL 是否可达。
7. cc-switch endpoint 是否可达。
8. route decision 是否匹配用户选择。
9. 是否发生未授权 fallback。
10. 测试结束后是否存在残留进程或端口占用。

## 8. 里程碑映射

### M1

- 增加 fake OpenCode binary。
- 增加 OpenCode fixture route。
- 增加 test port allocation fixture。

### M2

- 实现 OpenCode 真实只读检测。
- 实现本地新端口 route probe。
- 至少跑通 OpenCode → d-token test port → custom provider URL。

### M3

- GUI 增加 OpenCode Test Runner。
- Dashboard 显示 OpenCode test route。
- Doctor 能解释 route/proxy/provider 错误。

### M4

- Managed installer 支持 OpenCode route setup。
- 支持 custom provider route 与 cc-switch route。
- 支持测试端口自动分配与释放。

### M5

- OpenCode E2E 进入发布前兼容性测试。
- Windows/macOS 均需要覆盖。
