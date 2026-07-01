# M1 Implementation Review

日期：2026-07-01  
审查对象：M1 Rust workspace skeleton。

## 1. 审查结论

M1 已经达到“可以继续开发”的骨架状态。

通过项：

1. 代码已进入 `code/`。
2. 文档已进入 `docs/`。
3. 测试/脚本已进入 `support/`。
4. Rust workspace 已建立。
5. 核心 crate 边界已建立。
6. local API skeleton 已建立。
7. proxy/core-api 启动入口已建立。
8. config/router/compress/process/doctor skeleton 已建立。
9. 已有最小 router/doctor 测试。
10. 没有引入真实密钥或远程服务配置。

## 2. 架构审查

评分：8.6/10。

优点：

1. crate 分层清楚。
2. 领域类型集中在 `dtoken-types`。
3. route decision 与 config loader 解耦。
4. compression 当前只是 interface，避免 M1 过早实现复杂压缩。
5. local API endpoint 与 PRD 对齐。

风险：

1. 尚未跑真实 `cargo check`。
2. `dtoken-core-api` 尚无集成测试。
3. `dtoken-proxy` 当前还不是实际 request forwarder。
4. SQLite ledger 尚未开始。

## 3. 安全审查

评分：9/10。

通过点：

1. 默认监听 `127.0.0.1`。
2. 示例配置不包含真实密钥。
3. route decision 显式依赖 provider id。
4. 当前没有实现静默 fallback。
5. doctor 已包含本地端口检查。

风险：

1. 后续实现真实转发时必须补全 header/redaction 策略。
2. 后续实现 config loader 时必须避免把敏感字段写入日志。

## 4. Windows/macOS 审查

评分：7.8/10。

当前 M1 只是 Rust core skeleton，尚未进入 Tauri UI 和 installer 阶段。

已考虑：

1. process manager 单独成 crate。
2. doctor 单独成 crate。
3. proxy 默认本地监听。

后续必须补：

1. WebView2 检查。
2. Xcode Command Line Tools 检查。
3. `.cmd` / `.exe` / `.bat` shim 检查。
4. 盘符路径和 UTF-8 日志检查。

## 5. 可测试性审查

评分：7.5/10。

已完成：

1. router decision smoke test。
2. doctor smoke test。
3. structure checker。

下一步：

1. `cargo check --workspace`。
2. `cargo test --workspace`。
3. core-api endpoint integration test。
4. config loader test。
5. proxy startup smoke test。

## 6. 下一步建议

按优先级：

1. 本地执行 `cargo check --workspace`。
2. 修正编译问题。
3. 补 core-api 集成测试。
4. 补 config loader 测试。
5. 开始 M2 Dashboard skeleton。

## 7. 最终判断

M1 当前完成的是“骨架建立”，不是“功能完成”。

它已经足够作为后续 M2/M3/M4 的基础，但必须在下一轮通过本地或 CI 的 `cargo check` 和 `cargo test` 后，才能标记为 M1 Done。
