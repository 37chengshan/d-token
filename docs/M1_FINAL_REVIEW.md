# M1 Final Multidimensional Review

日期：2026-07-01  
对象：M1 Rust workspace + RTK/Headroom adapter skeleton + project policy + config patcher + event ledger。

## 1. 总结论

M1 的“代码骨架范围”已经基本完成：

```text
core skeleton
  + RTK adapter skeleton
  + Headroom adapter skeleton
  + project policy skeleton
  + config patch/backup/rollback skeleton
  + event ledger skeleton
  + local API endpoints
```

但 M1 还不能标记为完全 Verified，因为本轮没有在远端环境实际运行：

```bash
cargo check --workspace
cargo test --workspace
python ../support/scripts/check_structure.py
```

因此最终状态是：

```text
M1 Scope Implemented / Awaiting Local Cargo Verification
```

## 2. 产品维度审查

评分：9.0/10。

通过项：

1. 对齐唯一 PRD。
2. 对齐报告 03 的 M1A-M1E。
3. RTK 和 Headroom 都作为一等适配对象出现。
4. 项目级压缩策略已经有数据模型和 v0 推荐引擎。
5. 配置 diff/backup/rollback 已有 skeleton。
6. event ledger 已有 skeleton，后续可支撑分层 savings。

不足：

1. 还没有真实图形 App。
2. 还没有真实一键安装。
3. 还没有真实 RTK/Headroom 数据导入。

## 3. 架构维度审查

评分：8.8/10。

优点：

1. crate 边界清楚。
2. `dtoken-types` 作为统一领域模型层。
3. `dtoken-core-api` 只负责 local API 聚合。
4. RTK adapter 和 Headroom adapter 分离。
5. Project policy 独立，避免压缩策略散落在 API 层。
6. Config patcher 独立，符合“所有配置修改唯一入口”的安全原则。
7. Event ledger 独立，后续可从内存迁移到 JSONL/SQLite。

风险：

1. `dtoken-types` 已经较大，后续可能需要按领域拆分。
2. `dtoken-core-api` M1 聚合了较多 mock endpoint，M2/M3 应拆 handler modules。
3. 当前 event ledger 仅内存 mock，不具备持久化能力。
4. Config patcher 当前是 naive diff，不适合真实 JSON/TOML/YAML merge。

## 4. 安全与隐私维度审查

评分：9.1/10。

通过项：

1. 默认本地 API。
2. RTK adapter 不修改 RTK DB。
3. Headroom adapter 不读取 CCR 原文。
4. Config patcher 有 stale hash protection。
5. Patch plan 有 risk level。
6. 不默认开启 telemetry。
7. 不写入真实 key。

风险：

1. 后续真实 config patch 必须脱敏 diff。
2. 后续 managed install 必须校验 binary provenance。
3. 后续 Headroom request/response logs 必须默认不读 full body。
4. 后续 RTK tee file 只能展示摘要和路径。

## 5. Windows/macOS 维度审查

评分：7.8/10。

通过项：

1. 适配器与 process/config/doctor 分层，为跨平台实现留出空间。
2. Headroom/RTK detection 通过 binary command，天然适配 PATH。
3. Config patcher 支持 Path 抽象。

不足：

1. 尚未实现 WebView2 检测。
2. 尚未实现 Xcode CLT 检测。
3. 尚未实现 WSL/native Windows 分支。
4. 尚未实现 macOS quarantine/xattr 检测。
5. 尚未实现 Credential Manager / Keychain。

这些属于 M2/M5。

## 6. 可测试性维度审查

评分：8.0/10。

已补测试：

1. router decision smoke test。
2. doctor smoke test。
3. project recommendation tests。
4. config patcher tests。
5. event ledger test。

仍缺：

1. core-api endpoint integration tests。
2. RTK adapter missing/found command tests。
3. Headroom adapter missing/found command tests。
4. Config patcher backup/apply filesystem tests。
5. workspace-level CI。

## 7. M1 完成度判断

| 项目 | 状态 |
|---|---|
| Rust workspace | 已完成 |
| shared types | 已完成 |
| local API skeleton | 已完成 |
| proxy entrypoint | 已完成 |
| config loader | 已完成 |
| router skeleton | 已完成 |
| process manager skeleton | 已完成 |
| doctor skeleton | 已完成 |
| RTK adapter skeleton | 已完成 |
| Headroom adapter skeleton | 已完成 |
| project policy skeleton | 已完成 |
| config patcher skeleton | 已完成 |
| event ledger skeleton | 已完成 |
| mock endpoints | 已完成 |
| smoke tests | 部分完成 |
| cargo check | 未实际运行 |
| cargo test | 未实际运行 |
| CI gate | 未恢复 |

## 8. 下一步强制顺序

不要直接进入复杂 UI。下一步必须是：

```text
1. 本地执行 cargo check --workspace
2. 修正编译错误
3. 本地执行 cargo test --workspace
4. 恢复 GitHub Actions 最小 CI
5. 进入 M2 Tauri shell + Dashboard/Installer mock
```

## 9. 最终判断

M1 的代码和文档范围已经补齐，已经可以作为 M2 的基础。

但工程状态必须诚实标记为：

```text
Implemented, not yet verified by cargo/CI.
```

只有 cargo check、cargo test、structure check 全部通过后，才能改为：

```text
M1 Verified / Ready for M2.
```
