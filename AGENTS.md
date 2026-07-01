# AGENTS.md

一句话项目介绍：d-token 是一个 Headroom-compatible 的本地 AI Coding 压缩路由桌面系统，目标是在 Windows/macOS 上完整保留 Headroom 能力、迁移 Dashboard，并新增 provider、route、instance 可观测管理。

本文件给 Codex、OpenAI Agent、Claude Code、Cline 等 AI coding agents 使用。

## 必读顺序

1. `docs/PRD.md` — 唯一 PRD，也是产品真理源。
2. `docs/TECH_STACK.md` — Windows/macOS App 技术选型。
3. `docs/ARCHITECTURE.md` — 总体架构。
4. `docs/SECURITY_BOUNDARIES.md` — 本地优先安全边界。
5. `docs/FINAL_REVIEW.md` — 最新多维审查结论。

## 目录边界

```text
AGENTS.md   AI agent 入口说明
CLAUDE.md   Claude Code 入口说明
docs/       所有正式文档
code/       所有正式代码
support/    测试、脚本、CI 模板、开发辅助和杂项
```

## 工作原则

1. 不再新增多版本 PRD；产品变更直接修改 `docs/PRD.md`。
2. 所有正式文档放 `docs/`。
3. 所有正式代码放 `code/`。
4. 所有测试、脚本、CI 模板和杂项放 `support/`。
5. 根目录保持极简。
6. 不直接复制外部项目代码；若复用开源代码，必须遵守许可证与 notice 要求。
7. 所有真实密钥、token、cookie、auth 文件都不得提交。
8. 所有本地服务默认绑定 `127.0.0.1`。
9. Windows/macOS 是第一优先级。
10. Dashboard 不做简化版，Headroom 功能不缩水。
