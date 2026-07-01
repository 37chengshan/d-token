# CLAUDE.md

一句话项目介绍：d-token 是一个 Headroom-compatible 的本地 AI Coding 压缩路由桌面系统，目标是在 Windows/macOS 上完整保留 Headroom 能力、迁移 Dashboard，并新增 provider、route、instance 可观测管理。

本文件给 Claude Code 使用。

## 必读顺序

1. `docs/PRD.md` — 唯一 PRD，也是产品真理源。
2. `docs/TECH_STACK.md` — 桌面 App 与 runtime 技术选型。
3. `docs/ARCHITECTURE.md` — 总体架构。
4. `docs/SECURITY_BOUNDARIES.md` — 本地优先安全边界。
5. `docs/FINAL_REVIEW.md` — 最新多维审查。

## 当前主线

当前仓库已进入“单一 PRD + 目录收敛”阶段。

优先级：

1. 不再新增多版本 PRD。
2. 后续产品变更直接修改 `docs/PRD.md`。
3. 代码进入 `code/`。
4. 测试、脚本、CI 模板和杂项进入 `support/`。
5. 根目录保持极简。

## 目录边界

```text
AGENTS.md   AI agent 入口说明
CLAUDE.md   Claude Code 入口说明
docs/       所有正式文档
code/       所有正式代码
support/    测试、脚本、CI 模板、开发辅助和杂项
```

## 工作原则

- 修改前先读 `docs/PRD.md`。
- 不直接复制外部项目代码；如复用开源代码，必须遵守许可证。
- 所有真实密钥、token、cookie、auth 文件都不得提交。
- 所有本地服务默认绑定 `127.0.0.1`。
- Windows/macOS 兼容优先：UTF-8、路径、端口、`.cmd` shim、WebView2、Xcode Command Line Tools。
- Dashboard 不做简化版。
- Headroom 功能不缩水。

## 提交前检查

```bash
python support/scripts/check_structure.py
```
