# Local-first 安全边界

状态：Accepted  
关联：`docs/PRD.md`

## 1. 安全原则

d-token 是本地开发工具，不是云端账号托管服务，也不是远程 relay 服务。

默认原则：

1. 默认只监听 `127.0.0.1`。
2. 默认不开放公网端口。
3. 默认不上传用户代码、日志、prompt。
4. 默认本地保存配置和统计。
5. 敏感字段默认脱敏。
6. 所有 provider/route/instance 行为必须可见、可解释、可审计。

## 2. 明确不做

1. 不做云端账号托管。
2. 不做账号共享、账号池、转售或代登录。
3. 不做违反平台条款的自动化用途。
4. 不偷偷读取或迁移用户已有 token。
5. 不把用户代码、日志、prompt 上传到 d-token 服务器。
6. 不做公网 relay 服务。
7. 不做静默 provider fallback。
8. 不隐藏上游错误。
9. 不默认持久保留完整请求正文。

## 3. 凭据处理

### 3.1 开发期

开发期可以使用：

1. 环境变量。
2. 本地 `.env.local`。
3. 本地 `providers.local.yaml`。

但这些文件必须 gitignored。

### 3.2 产品期

产品期优先使用：

1. Windows Credential Manager。
2. macOS Keychain。
3. Tauri Stronghold 或同等级本地密钥方案。

### 3.3 显示规则

Dashboard 中永远不明文展示：

1. API key。
2. Authorization header。
3. Cookie。
4. Refresh token。
5. Session token。
6. Local auth file content。

## 4. 日志规则

日志默认记录：

1. request id。
2. timestamp。
3. agent。
4. instance id。
5. provider id。
6. route id。
7. model。
8. token counts。
9. latency。
10. transform names。
11. error type。

日志默认不记录：

1. 完整 API key。
2. 完整 Cookie。
3. 完整 Authorization。
4. 大段用户代码。
5. 大段 prompt。
6. 大段 tool result。

如用户显式开启 full debug log，也必须二次确认，并在导出时再次脱敏。

## 5. 网络边界

默认监听：

```text
127.0.0.1
```

默认不允许：

```text
0.0.0.0
公网 IP
局域网自动暴露
```

如果未来需要局域网模式，必须单独设计：

1. UI 显式警告。
2. 密码或 token 保护。
3. CORS 限制。
4. 访问日志。
5. 明确开关。

## 6. Route 安全

1. 每条 route 必须显式配置 target provider。
2. 不允许配置缺失时自动猜测 provider。
3. 不允许 silent fallback。
4. fallback 必须显示在 Dashboard。
5. route decision 必须进入 request log。
6. chain bridge 必须显示每段链路状态。

## 7. Sidecar 安全

1. sidecar 必须有唯一 instance id。
2. sidecar 必须有 healthcheck。
3. sidecar 必须有 log path。
4. sidecar 必须可停止、可重启。
5. sidecar 异常退出必须可见。
6. sidecar 不允许长期无记录后台残留。

## 8. 数据保留

默认保留：

1. 汇总指标。
2. token savings ledger。
3. route decision metadata。
4. error metadata。

默认不长期保留：

1. 完整 prompt。
2. 完整代码文件。
3. 完整 tool output。
4. 完整 response body。

如果用户开启完整保留，必须：

1. 本地保存。
2. 明确存储位置。
3. 可一键删除。
4. 导出时脱敏。

## 9. 合规实现原则

1. d-token 只做用户本地开发环境中的压缩、路由、观测和配置管理。
2. d-token 不替用户规避第三方平台条款。
3. d-token 不提供账号托管或共享能力。
4. d-token 不帮助隐藏真实使用来源。
5. d-token 的 Dashboard 要透明展示 route 和 provider，而不是隐藏链路。
