# 源码与证据索引

## 1. 研究版本

### AiMaMi

- Repository：`https://github.com/borawong/AiMaMi`
- 固定 commit：`297c7af56f10fb371b77bc9b6b65aa320afcbe7e`
- License：Apache License 2.0
- 本地唯一法律入口：`docs/legal/THIRD_PARTY_NOTICES.md`
- 本地许可证副本：`docs/legal/AIMAMI_APACHE_2_0_LICENSE.txt`

### d-token

- Workspace：`/Users/cc/Documents/d-token`
- Branch：`codex/prd-macos-polish`
- 研究时 HEAD：`08f7f9f5526273f8ef6af57f0d80fac16d3fda61`
- 范围：包含 2026-07-12 活跃未提交工作树，不只分析 HEAD。

## 2. AiMaMi 直接检查的源码范围

| 路径 | 研究用途 |
|---|---|
| `README.md` | 产品定位、功能表面、安装和使用路径 |
| `package.json` | 前端框架、脚本和依赖 |
| `src/App.tsx` | 页面组织、懒加载和应用入口 |
| `src/components/layout/*` | 壳层、页面转场和导航结构 |
| `src/components/ui/*` | 分段控制器、Sidebar 和基础交互 |
| `src-tauri/Cargo.toml` | Rust/Tauri/plugin 依赖 |
| `src-tauri/tauri.conf.json` | 窗口、CSP、bundle 和平台配置 |
| `src-tauri/capabilities/*` | Shell、Process、快捷键、更新等权限面 |
| `src-tauri/src/lib.rs` | Tauri command 注册和应用启动 |
| `src-tauri/src/repository.rs` | 本地数据、配置和 Repository 集中度 |
| `src-tauri/src/process.rs` | 进程生命周期与系统调用 |
| `src-tauri/src/proxy.rs` | 代理/中转控制 |
| `src-tauri/src/models.rs` | 核心数据模型 |
| `CONTRIBUTING.md` | 开发、构建和贡献约束 |
| `LICENSE` | Apache-2.0 许可依据 |
| `LICENSES/THIRD_PARTY.md` | 第三方资产说明 |

## 3. d-token 直接检查的核心代码

### 前端壳层和状态

| 路径 | 结论 |
|---|---|
| `code/apps/desktop/src/app/components/shell/AppShell.tsx` | 六页懒加载、共享壳、PageStage、Reduce Motion |
| `code/apps/desktop/src/app/components/shell/Sidebar.tsx` | AiMaMi 结构适配后的 d-token 导航 |
| `code/apps/desktop/src/app/components/shell/PageStage.tsx` | 页面转场适配 |
| `code/apps/desktop/src/app/components/shell/AnimatedSegmentedControl.tsx` | 分段控制器、键盘和 Reduce Motion |
| `code/apps/desktop/src/app/components/shell/AnimatedNavIcon.tsx` | d-token 自有导航动画 |
| `code/apps/desktop/src/app/components/common/Surface.tsx` | 独立实现的 grouped surface |
| `code/apps/desktop/src/app/lib/useRuntimeDashboard.ts` | app-wide Runtime Evidence、轮询、退避和 operational slices |
| `code/apps/desktop/src/app/lib/state/evidence.ts` | verified/partial/stale/unavailable 分类 |
| `code/apps/desktop/src/app/components/pages/Install.tsx` | OpenCode transaction UI |
| `code/apps/desktop/src/app/components/pages/InstallWorkbench.tsx` | 连接工作台 |
| `code/apps/desktop/src/app/components/pages/Changes.tsx` | 变更和回退 |
| `code/apps/desktop/src/app/components/pages/Analytics.tsx` | 指标与图表 |
| `code/apps/desktop/package.json` | 缺少 typecheck，现有 test/build/E2E scripts |

### Tauri 与平台层

| 路径 | 结论 |
|---|---|
| `code/apps/desktop/src-tauri/src/main.rs` | 功能完整但职责过度集中 |
| `code/apps/desktop/src-tauri/src/opencode_adapter.rs` | hash-bound preview/apply/verify/rollback 的 golden adapter |
| `code/apps/desktop/src-tauri/src/upstream_discovery.rs` | OpenCode/cc-switch 只读发现、secret 跳过、schema fail closed |
| `code/apps/desktop/src-tauri/Cargo.toml` | Tauri 与平台依赖 |
| `code/apps/desktop/scripts/tauri-with-sidecar.mjs` | 构建前 sidecar 准备 |

### Runtime 与领域 crate

| 路径 | 结论 |
|---|---|
| `code/crates/dtoken-config-patcher/src/lib.rs` | 严格配置事务、备份、原子写入和精确恢复 |
| `code/crates/dtoken-router/src/lib.rs` | 显式路由和歧义拒绝 |
| `code/crates/dtoken-core-api/src/lib.rs` | 本地协议入口、路由、运行数据与 API |
| `code/crates/dtoken-event-ledger/src/lib.rs` | 脱敏 JSONL、尾部修复和并发写入基础 |
| `code/crates/dtoken-compress/src/optical/engine.rs` | Hybrid prepare/active/fallback 执行编排 |
| `code/crates/dtoken-compress/src/optical/renderer.rs` | 确定性 PNG 和资源限制 |
| `code/crates/dtoken-compress/src/optical/patcher.rs` | typed patch |
| `code/crates/dtoken-compress/src/optical/post_validate.rs` | sealed mutation post-validation |
| `code/crates/dtoken-compress/src/optical/eval.rs` | capability/eval gate |
| `code/crates/dtoken-types/tests/hybrid_accounting.rs` | signed savings 和归因语义 |

## 4. d-token 当前事实文档

| 路径 | 用途 |
|---|---|
| `docs/PRD.md` | 唯一产品真理源 |
| `docs/PLAN.md` | 当前执行计划和发布阻塞 |
| `docs/product/IMPLEMENTATION_STATUS.md` | 当前代码事实快照 |
| `docs/ARCHITECTURE.md` | 系统架构 |
| `docs/TECH_STACK.md` | 技术栈与平台边界 |
| `docs/SECURITY_BOUNDARIES.md` | 安全、隐私和写入边界 |
| `docs/APP_FUNCTIONS_AND_PAGES.md` | 页面与功能映射 |
| `docs/optical/implementation-status.md` | Optical 当前实现 |
| `docs/optical/release-gates.md` | Optical 发布禁令和 Gate |
| `docs/legal/THIRD_PARTY_NOTICES.md` | AiMaMi 等第三方适配唯一声明 |

## 5. 当前本地验证结果

### 已实际运行并通过

- `npm test`：24 个测试文件、108 项测试通过。
- `npm run build`：Vite production build 通过。
- `cargo test --workspace`：Rust workspace 通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test -p dtoken-config-patcher`：事务和 17 项集成测试通过。

### 已实际运行并发现问题

- `npm run typecheck`：失败，原因是 `package.json` 没有该 script。
- 前端测试 stderr：Radix DialogOverlay 存在 function component ref/forwardRef warning。
- Vite output：主 chunk、Recharts 相关 chunk 和 InstallWorkbench chunk 较大。

### 本轮未取得成功证据

- `cargo fmt --all -- --check`：被 CodexPro safe bash allowlist 阻止。
- `python3 support/scripts/check_structure.py`：被 safe bash allowlist 阻止。
- 真实 OpenCode routed test：本轮未执行。
- macOS/Windows 签名安装：本轮未执行。

这些项目不得在本研究中写成已通过。

## 6. 关键事实与证据映射

| 事实 | 主要证据 |
|---|---|
| d-token 不允许静默路由猜测 | router/core-api tests、AGENTS.md |
| OpenCode 写入绑定审查 hash | `opencode_adapter.rs`、对应 Rust/React tests |
| 配置恢复保留原始 bytes/权限 | ConfigPatcher 与 patch_plan tests |
| Browser preview 不代表 runtime | Runtime Evidence 与页面 tests |
| Optical active 不是默认生产能力 | Optical policy、capability、eval、release-gates |
| d-token 已适配部分 AiMaMi UI 结构 | `THIRD_PARTY_NOTICES.md` 与相关源码注释 |
| AiMaMi 桌面系统集成更完整 | 固定 commit Tauri 配置、capabilities 和 lib.rs |
| d-token 发布仍被阻断 | `docs/PLAN.md`、`IMPLEMENTATION_STATUS.md`、当前缺失实机证据 |

## 7. 事实、验证和推断标记

### 源码事实

直接从文件结构、函数、配置和测试代码得到。例如：OpenCode adapter 核对 before/after hash。

### 本地验证

当前环境实际执行命令得到。例如：108 项前端测试通过。

### 架构推断

基于源码规模、依赖和职责作出的工程判断。例如：Repository 或 main.rs 具有中心模块风险。

### 产品建议

基于用户旅程和 d-token PRD提出，不代表已经实现。例如：引入托盘和单实例。

## 8. 更新本研究时的检查

1. AiMaMi fixed commit 是否变化。
2. d-token 当前 HEAD 和工作树是否变化。
3. 测试命令是否在当前候选 commit 重新执行。
4. `THIRD_PARTY_NOTICES.md` 是否新增适配文件。
5. PRD/PLAN 是否改变产品边界或顺序。
6. Optical release gate 是否仍保持默认关闭。
7. 本目录 README 链接是否完整。