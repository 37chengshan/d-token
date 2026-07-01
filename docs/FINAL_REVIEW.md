# 最终多维审查报告

日期：2026-07-01  
审查对象：单一 Living PRD、技术选型、架构、安全边界、目录结构。

## 1. 总结论

本轮方案通过审查，可以作为 d-token 后续开发的唯一执行基线。

核心判断：

1. 只保留一个 PRD：`docs/PRD.md`。
2. 后续产品变更直接修改 `docs/PRD.md`，不再新增 PRD 版本。
3. 所有正式文档进入 `docs/`。
4. 所有代码进入 `code/`。
5. 测试、脚本、CI 模板和杂项进入 `support/`。
6. 根目录只保留 AI 入口说明文件和必要目录。
7. Desktop 技术主线固定为 Tauri 2 + React/TypeScript + Rust core + Python runtime transition。
8. Headroom 功能不得缩水。
9. Dashboard 不得做简化版。

## 2. 产品目标审查

评分：9.2/10。

优点：

1. 定位清楚：Headroom-compatible enhanced desktop system。
2. 目标明确：全功能保留、全 Dashboard 迁移、Windows/macOS App。
3. 增强层明确：ProviderProfile、RouteProfile、Instance、Chain mode、OS process status。

风险：

1. Headroom parity 工作量大。
2. Dashboard 全量迁移工作量大。
3. 多实例和本地进程管理会增加复杂度。

结论：目标正确，但必须按里程碑拆分，不能一口气全做。

## 3. 边界审查

评分：9.5/10。

通过点：

1. 明确 local-first。
2. 默认 `127.0.0.1`。
3. 不做云端账号托管。
4. 不做账号共享、账号池、转售或代登录。
5. 不做静默 provider fallback。
6. 不上传用户代码、日志、prompt。
7. 所有敏感字段默认脱敏。

结论：边界足够清晰，后续实现必须严格遵守。

## 4. 技术选型审查

评分：9/10。

Tauri 2 + React/TypeScript 的选择合理：

1. 更适合轻量桌面 App。
2. 适合本地 sidecar 管理。
3. 适合 Windows/macOS 打包。
4. 适合把 Headroom Dashboard 组件化迁移。

Rust core + Python runtime transition 的选择合理：

1. Rust 适合本地进程、proxy、router、日志和配置。
2. Python 适合先保留 Headroom-compatible 压缩能力。
3. 避免一开始全量 Rust 重写导致功能缩水。

结论：技术选型通过。

## 5. 目录结构审查

评分：9/10。

目标结构：

```text
AGENTS.md
CLAUDE.md
docs/
code/
support/
.gitignore
```

通过点：

1. 文档集中。
2. 代码集中。
3. 杂项集中。
4. 根目录更适合 AI agent 进入后快速理解项目。

风险：

1. GitHub workflow 若要真正生效，仍然需要 `.github/workflows`。
2. 如果严格保持根目录极简，CI 可先放在 `support/ci` 作为模板，真正启用时再恢复 `.github`。

结论：当前阶段以根目录极简优先。

## 6. Windows/macOS 审查

评分：8.8/10。

Windows 必须优先验证：

1. WebView2。
2. MSVC toolchain。
3. 端口占用。
4. 旧进程残留。
5. UTF-8。
6. `.cmd` / `.exe` / `.bat`。
7. 盘符路径。

macOS 必须优先验证：

1. Xcode Command Line Tools。
2. Intel / Apple Silicon。
3. quarantine / xattr。
4. Keychain。
5. LaunchAgent。
6. signing / notarization。

结论：Windows/macOS 风险已识别，下一步应在 Doctor 页面中前置实现。

## 7. 最终判定

方案通过。

下一步优先级：

1. 完成旧文件删除和目录收敛。
2. 创建 M1 Rust workspace skeleton。
3. 创建 Desktop Tauri skeleton。
4. 创建 mock Dashboard 全卡片占位。
5. 创建 Doctor skeleton。

本报告之后，`docs/PRD.md` 是唯一产品真理源。
