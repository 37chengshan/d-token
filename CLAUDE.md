# CLAUDE.md

本文件给 Claude Code 使用。

## 当前任务主线

当前仓库处于 d-token v0.1 初始化阶段。优先完成：

1. 产品 PRD。
2. 系统架构。
3. Provider/Route 配置模型。
4. Windows/macOS 桌面 App 骨架。
5. 本地 proxy/router/compress 模块边界。
6. cc-switch bridge 边界。

## 你应该如何工作

- 先读 `docs/prd/d-token-prd-v0.1.md`。
- 再读 `docs/architecture/system-architecture.md`。
- 修改代码前确认目录边界。
- 保持中文文档清晰。
- 所有真实密钥、token、cookie、auth 文件都不得提交。
- Windows 兼容问题必须优先考虑：UTF-8、路径、端口、`.cmd` shim。

## 架构重点

核心链路：

```text
Agent -> d-token proxy -> compress -> provider
Agent -> d-token proxy -> compress -> cc-switch -> provider
```

Codex 双实例：

```text
codex-official -> d-token proxy A -> OpenAI official
codex-relay    -> d-token proxy B -> custom provider or cc-switch
```

## 提交前检查

```bash
python scripts/check_structure.py
```
