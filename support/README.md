# support

更新时间：2026-07-11 晚间

`support/` 保存 d-token 的开发验证、CI、fixture、fake-bin、平台准备脚本、审计账本和生成证据。它不是正式产品文档目录，也不应堆放 Agent 临时 notes。

## 当前结构

```text
support/
├── scripts/                 结构检查、构建、运行、路由、发布和审计脚本
├── tests/                   支持层测试
├── fixtures/                no-touch 测试输入
├── fake-bin/                隔离测试用假命令
├── artifacts/               生成的脱敏验证证据
├── ci/                      CI 配置与辅助
├── config/                  支持脚本配置
├── git-operation-audit.schema.json
├── git-operations.json      真实 Git 操作审计账本
└── pending-commits.json     阶段计划/提交追溯账本
```

## 规则

1. 正式产品规范放在 `docs/`，正式代码放在 `code/`。
2. 根目录不保留 `task_plan.md`、`findings.md`、`progress.md`；Agent 协作使用 `.ai-bridge/`。
3. `artifacts/` 只能保存脱敏证据，不得包含 credential、auth 文件、完整 prompt/response 或原始模型输出。
4. 开发脚本不得把 workload、smoke、fixture 等术语直接产品化为正式 UI。
5. 真实 OpenCode 验收必须遵守 `AGENTS.md` 的授权、备份、最小修改和恢复规则。
6. Git commit/branch/merge/rebase/push 前必须先运行 `git_operation_audit.py record`。
7. 结构或文档变更后运行：

```bash
python3 support/scripts/check_structure.py
```

已被淘汰的 support 根目录占位和重复 notes 已迁入 `docs/archive/repository-cleanup/legacy-support-notes/`。
