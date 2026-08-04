# AiMaMi 与 d-token 研究总览

更新时间：2026-07-12  
研究对象：`borawong/AiMaMi` 固定源码版本与 d-token 本地活跃工作树  
用途：为 d-token 产品、架构、桌面体验与发布路线提供可执行参考

## 1. 一句话结论

AiMaMi 是以 Codex 用户日常任务为中心的桌面工作流管理器；d-token 是以多 Agent 本地压缩、路由、协议适配、安全配置和运行证据为中心的控制面。两者外观相近，但核心职责不同，不能把 AiMaMi 当成 d-token 的直接模板。

最优策略是：

> 借鉴 AiMaMi 的桌面产品组织、任务化交互、懒加载、托盘、更新、单实例和系统集成体验；保留 d-token 的显式路由、事务式写入、凭据隔离、Event Ledger、Doctor、Evidence 与 fail-closed 安全模型。

## 2. 产品定位差异

| 维度 | AiMaMi | d-token |
|---|---|---|
| 第一用户 | Codex 高频用户 | 使用 OpenCode、Claude Code、Codex、Cursor、Cline 等工具的开发者 |
| 第一问题 | 账号、配额、路由、中转、会话、MCP、Skills 与配置管理分散 | 请求上下文昂贵、路由不透明、配置修改危险、运行证据不足 |
| 核心资产 | Codex 本机工作流与成熟桌面交互 | Rust-owned runtime、压缩算法、协议桥接、配置事务、证据链 |
| 产品入口 | Codex 桌面伴侣 | 多 Agent 本地控制面 |
| 安全重点 | 本地配置与桌面权限 | 路由正确性、密钥隔离、原子写入、回退、脱敏与证据真实性 |
| 当前成熟优势 | 桌面交付与 Codex 闭环 | 后端正确性、安全事务与自动化测试 |

## 3. AiMaMi 值得借鉴的核心能力

1. 以真实任务而不是技术模块组织导航。账号、额度、路由、会话、MCP、Skills 和维护都是用户能够理解的动作。
2. 前端页面懒加载，避免一次性装载所有管理页面。
3. Tauri 桌面能力完整，包含托盘、全局快捷键、进程、Shell、更新和单实例等系统能力。
4. 配置、账号、代理和进程管理集中在本地，响应快，离线可用性强。
5. 产品视觉和交互动效形成统一桌面语言，不是网页直接套壳。
6. 对 Codex 生态覆盖深，能够形成从配置到会话的完整日常使用闭环。

## 4. AiMaMi 需要谨慎对待的地方

1. 核心 Repository、Process 与 Proxy 模块体积较大，容易形成中心化“上帝模块”。
2. 直接操作 `~/.codex` 等用户配置时，如果缺少完整 diff、备份、并发检查和回退，风险会随功能增长放大。
3. Tauri 权限较宽，Shell、Process、Global Shortcut、Updater 和 macOS private API 都扩大了攻击面。
4. CSP 关闭或配置过宽时，前端注入风险会直接跨越到 Native command。
5. 账号和配额数据容易出现来源陈旧、时区错误、刷新竞争、限额误判等状态问题。
6. 过度绑定 Codex 内部文件结构会承受上游格式变化风险。

## 5. d-token 当前优势

1. `ConfigPatcher` 已具备 SHA-256 绑定、严格 JSON、备份 manifest、权限快照、原子替换、并发修改拒绝与精确恢复。
2. OpenCode adapter 已实现 preview → review revision → before/after hash → backup → apply → verify → rollback 的完整受控写入链。
3. Route、Provider、Instance 选择明确，存在歧义时 fail closed，而不是静默选第一个。
4. 凭据使用操作系统凭据库引用，普通配置不保存明文 secret。
5. Runtime Dashboard 使用统一 Evidence Provider，能够区分 runtime、partial、stale、unavailable 和 browser preview。
6. Rust workspace、前端单元测试和 Clippy 当前均有较强覆盖。
7. Optical/Hybrid Context Optimization 已形成 capability、plan、renderer、patcher、post-validator 与 fallback 的安全链路。

## 6. d-token 当前主要缺陷

### P0：影响发布

- Tauri `main.rs` 仍承载过多职责，命令、状态、文件、进程、凭据、Doctor、安装和数据聚合高度集中。
- 第三方 Agent adapter 尚未形成统一 SPI，OpenCode 是先进的单点实现，Claude Code、Codex、Cursor、Cline 等仍未闭环。
- macOS 与 Windows 缺少同一候选 commit 的安装、签名、凭据、运行、路由、回退和 artifact hash 证据包。
- 真实 OpenCode routed test 尚需绑定当前候选代码、实际 runtime identity、route decision 与脱敏 Ledger。
- Optical active 路径虽已存在代码，但没有真实 Provider fidelity、成本和时延证据，必须继续默认关闭。

### P1：影响维护与质量

- `ConfigPatcher` 和 Optical Engine 继续增长，需按责任拆分。
- 前端大量使用 `Record<string, any>`，Rust 与 TypeScript 合同不够强。
- `package.json` 缺少独立 `typecheck` 脚本，`vite build` 不能替代 `tsc --noEmit`。
- 前端测试仍输出 Radix Dialog `ref/forwardRef` 警告。
- Vite 构建存在较大的主包、图表包和 InstallWorkbench 包，需要预算与进一步切分。
- cc-switch 读取依赖固定 SQLite schema，需要版本协商和更稳定的 adapter 边界。

### P2：影响体验完整度

- updater、单实例、托盘状态、全局快捷键等桌面能力尚未形成完整闭环。
- 历史 UI 原型和旧源码目录仍有所有权债务。
- 用户旅程仍偏“控制面”，缺少 AiMaMi 式任务化引导和渐进式复杂度。

## 7. 推荐实施顺序

1. 先拆分 Tauri main 和定义 Agent Adapter SPI，避免继续扩大中心模块。
2. 把 OpenCode adapter 迁移为 SPI 参考实现，再扩展其他 Agent。
3. 建立统一状态信封和 Rust → TypeScript schema 生成，减少动态对象。
4. 建立统一 `check` 门禁：format、clippy、Rust tests、typecheck、Vitest、build、E2E、structure 和 secret scan。
5. 完成 macOS 同 commit 发布证据，再做 Windows 对称证据。
6. 补齐 updater、单实例和托盘；此后再扩大桌面增强能力。
7. Optical 继续保持证据门控，真实模型评估通过前不得作为默认生产能力宣传。

## 8. 当前研究边界

- AiMaMi 分析基于固定 commit 的源码、配置和项目结构，不把 README 声明自动等同于运行实测。
- d-token 分析基于本地 `codex/prd-macos-polish` 活跃工作树，不仅依据已提交版本。
- 本轮 d-token 已验证前端 108 项测试、Vite build、Rust workspace tests 和 Clippy；`npm run typecheck` 因脚本不存在而失败。
- 被本地安全 allowlist 阻止的命令不记为成功证据。
- 产品边界仍以 `docs/PRD.md` 为准，本研究不直接修改产品契约。