# scripts

本目录存放本地开发脚本和辅助检查脚本。

正式产品代码不要放在这里；正式代码统一放入 `code/`。

## 发布门禁

`run_release_verification.ps1` 默认执行 **no-touch** 发布验证，不调用任何真实 Agent，也不会启动、停止或修改外部 Headroom、RTK、cc-switch：

```powershell
support\scripts\run_release_verification.ps1
```

门禁明确包含以下质量检查：

- 前端 Vitest：`npm run test:run`
- TypeScript：`npx tsc --noEmit`
- 前端生产构建：`npm run build`
- Rust 格式：`cargo fmt --all -- --check`
- Rust 静态检查：`cargo clippy --workspace --all-targets -- -D warnings`
- Rust 全工作区测试：`cargo test --workspace`

只有在用户已单独授权真实 OpenCode 验收时，才显式追加：

```powershell
support\scripts\run_release_verification.ps1 -IncludeOpenCode
```

发布门禁不测试 Codex。`-SkipOpenCode` 暂时保留为兼容参数，但默认行为本身已经是不调用 OpenCode。

每份发布验证报告会记录 Git HEAD、dirty 状态、候选指纹、操作系统/架构、PowerShell、Node、rustc 和 Cargo 版本。正式桌面打包时，`prepare_desktop_sidecars.ps1 -Release` 会写入 `code/target/release/d-token-release-candidate.json`；bundle audit 要求该指纹与当前工作树一致，并要求 App 与安装包生成时间晚于候选戳。源码变化后必须重新打包，旧安装包不能继续冒充当前候选。

`run_release_gate_contract_test.ps1` 是轻量回归检查，可在不运行真实 Agent 或外部服务的情况下验证 CI、no-touch 门禁、候选溯源和旧产物拦截规则没有被删掉。

`run_release_candidate_provenance_test.ps1` 会在系统临时目录创建一次性 Git 仓库，动态证明“源码变化会让旧候选戳失效”和“早于候选戳的产物会被拒绝”；测试结束会删除它自己创建的临时目录。
