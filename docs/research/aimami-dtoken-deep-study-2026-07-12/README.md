# AiMaMi 与 d-token 深度研究

更新时间：2026-07-12  
研究对象：[`borawong/AiMaMi`](https://github.com/borawong/AiMaMi) 与 d-token 本地活跃工作树  
AiMaMi 固定源码版本：[`297c7af56f10fb371b77bc9b6b65aa320afcbe7e`](https://github.com/borawong/AiMaMi/commit/297c7af56f10fb371b77bc9b6b65aa320afcbe7e)

本目录是完整的 Markdown 研究成果，不是 Word 报告的摘要。内容按产品、前端、动画、后端、状态、框架、算法、安全、优秀思路、差距、路线图、三轮审核和源码证据拆分，可直接作为后续开发输入。

本研究不替代：

- [`docs/PRD.md`](../../PRD.md)：唯一产品真理源。
- [`docs/PLAN.md`](../../PLAN.md)：当前执行计划。
- [`docs/product/IMPLEMENTATION_STATUS.md`](../../product/IMPLEMENTATION_STATUS.md)：当前代码事实。
- [`docs/legal/THIRD_PARTY_NOTICES.md`](../../legal/THIRD_PARTY_NOTICES.md)：第三方代码适配、许可证与修改声明唯一入口。

## 核心结论

1. **AiMaMi 是 Codex 本机工作流管理器**，优势是任务化产品、桌面系统集成、账号/额度/路由/会话/MCP/Skills 的完整日常闭环。
2. **d-token 是多 Agent 本地运行时与控制面**，核心资产是压缩、显式路由、协议桥接、安全配置事务、Event Ledger、Doctor、Evidence 和 Optical/Hybrid Context Optimization。
3. d-token 不应复制 AiMaMi 的产品边界，而应吸收其桌面体验、任务表达、懒加载、托盘、更新和单实例思路。
4. d-token 当前在配置安全、路由正确性、证据建模和自动化测试上更强，在桌面交付成熟度、系统能力闭环和普通用户旅程上落后。
5. 当前最高优先级不是继续堆页面，而是收口工作树、拆分 Tauri 中心模块、建立 Agent Adapter SPI、统一类型/状态，并完成双平台同候选 commit 发布证据。

## 完整目录

### 研究入口

| 文档 | 内容 |
|---|---|
| [`00_EXECUTIVE_SUMMARY.md`](00_EXECUTIVE_SUMMARY.md) | 总体结论、优劣势、P0/P1/P2 和推荐顺序 |
| [`01_PRODUCT_AND_USER_JOURNEY.md`](01_PRODUCT_AND_USER_JOURNEY.md) | AiMaMi 产品本质、首次启动、日常使用、配置修改、异常恢复和 d-token 用户旅程 |

### 前端设计

| 文档 | 内容 |
|---|---|
| [`02_FRONTEND_UI_DESIGN.md`](02_FRONTEND_UI_DESIGN.md) | 信息架构、Shell/Surface、页面设计、响应式、可访问性和 UI 验收 |
| [`03_FRONTEND_ANIMATION_AND_INTERACTION.md`](03_FRONTEND_ANIMATION_AND_INTERACTION.md) | 页面转场、侧栏、分段控制器、状态动画、危险动作、性能与 Reduce Motion |
| [`evidence/ui/README.md`](evidence/ui/README.md) | 用户提供的 AiMaMi 逐页真实界面截图、页面索引、隐私处理与采用边界 |

### 后端、状态与框架

| 文档 | 内容 |
|---|---|
| [`04_BACKEND_TAURI_RUST_ARCHITECTURE.md`](04_BACKEND_TAURI_RUST_ARCHITECTURE.md) | AiMaMi 后端结构、d-token Tauri/Rust 拆分、Agent Adapter SPI、ConfigPatcher 和进程生命周期 |
| [`05_STATE_UNIFICATION_AND_DATA_MODEL.md`](05_STATE_UNIFICATION_AND_DATA_MODEL.md) | Runtime Evidence、统一状态信封、领域 Store、状态机、Rust/TypeScript 合同和一致性策略 |
| [`06_FRAMEWORK_TECH_STACK_AND_MODULE_BOUNDARIES.md`](06_FRAMEWORK_TECH_STACK_AND_MODULE_BOUNDARIES.md) | 技术栈、依赖方向、前端/Rust 目录、Tauri command、sidecar、持久化和 CI Gate |

### 算法与安全

| 文档 | 内容 |
|---|---|
| [`07_ALGORITHMS_AND_CORE_MECHANISMS.md`](07_ALGORITHMS_AND_CORE_MECHANISMS.md) | 配额归一化、路由、协议适配、Hybrid Context、依赖图、风险、经济门、渲染、Patcher 和 Ledger |
| [`08_SECURITY_PRIVACY_AND_CONFIG_TRANSACTIONS.md`](08_SECURITY_PRIVACY_AND_CONFIG_TRANSACTIONS.md) | 威胁模型、Tauri 权限、CSP、凭据、配置事务不变量、Updater、隐私与发布安全 Gate |

### 借鉴、差距与开发计划

| 文档 | 内容 |
|---|---|
| [`09_EXCELLENT_IDEAS_AND_ADOPTION_MATRIX.md`](09_EXCELLENT_IDEAS_AND_ADOPTION_MATRIX.md) | 应直接吸收、改造吸收、不应照搬，以及 d-token 自身应强化的优秀思路 |
| [`10_DTOKEN_VS_AIMAMI_GAP_ANALYSIS.md`](10_DTOKEN_VS_AIMAMI_GAP_ANALYSIS.md) | 多维度对比、双方优缺点、d-token 十项核心缺陷和竞争力判断 |
| [`11_IMPLEMENTATION_ROADMAP.md`](11_IMPLEMENTATION_ROADMAP.md) | Phase 0–8、两周计划、四到八周计划、DoD 和禁止提前完成规则 |

### 审核与证据

| 文档 | 内容 |
|---|---|
| [`12_THREE_ROUND_REVIEW_AND_ITERATIONS.md`](12_THREE_ROUND_REVIEW_AND_ITERATIONS.md) | 使用相同十项维度进行三轮完整审核，每轮发现与优化记录 |
| [`13_SOURCE_EVIDENCE_INDEX.md`](13_SOURCE_EVIDENCE_INDEX.md) | AiMaMi/d-token 源码范围、本地测试结果、未验证项和事实/推断边界 |

## 推荐阅读路径

### 只看结论

```text
00 总览
→ 10 差距分析
→ 11 实施路线图
```

### 前端与产品

```text
01 产品与用户旅程
→ 02 UI 设计
→ 03 动画与交互
→ 09 优秀思路
```

### 后端与架构

```text
04 Tauri/Rust 后端
→ 05 状态统一
→ 06 框架边界
→ 08 安全事务
```

### 算法与 Optical

```text
07 核心算法
→ 08 安全事务
→ 10 差距分析
→ 11 Phase 8
```

### 审核研究是否可靠

```text
12 三轮审核
→ 13 源码证据索引
```

## 当前 d-token 本地验证

本次研究期间实际执行：

- 前端 Vitest：24 个测试文件、108 项测试通过。
- Vite production build：通过。
- Rust `cargo test --workspace`：通过。
- Rust Clippy `-D warnings`：通过。
- ConfigPatcher 专项测试：通过。

同时发现：

- `npm run typecheck` 不存在，当前缺少独立 TypeScript 门禁。
- Vitest stderr 有 Radix DialogOverlay `ref/forwardRef` 警告。
- 主 chunk、Recharts chunk 和 InstallWorkbench chunk 较大。
- 本轮未执行真实 OpenCode routed test 和双平台签名安装，不能写成通过。

详细证据见 [`13_SOURCE_EVIDENCE_INDEX.md`](13_SOURCE_EVIDENCE_INDEX.md)。

## d-token 已适配的 AiMaMi 范围

根据唯一法律入口：

- `PageStage.tsx`：页面低幅度转场与 Reduce Motion。
- `AnimatedSegmentedControl.tsx`：泛型分段控制器、键盘和产品样式。
- `Sidebar.tsx` 与 `shell.css`：inset shell、展开/折叠和主题分段结构。
- `Surface.tsx`：仅借鉴 grouped-panel 原则，独立实现。
- `AnimatedNavIcon.tsx`：d-token 自有实现。

未复制 AiMaMi 名称、Logo、应用图标、截图、品牌文案、人物、二维码、声音或 Type4Me 音频。详情以 [`THIRD_PARTY_NOTICES.md`](../../legal/THIRD_PARTY_NOTICES.md) 为准。

## 最重要的 P0

1. 收口当前大规模工作树并按模块提交。
2. 拆分 `src-tauri/src/main.rs`。
3. 建立 Agent Adapter SPI，将 OpenCode adapter 作为 golden implementation。
4. 建立版本化 EvidenceEnvelope 和自动生成 TypeScript DTO。
5. 增加 `typecheck` 和统一 `check`，修复前端 warning。
6. 完成当前候选 commit 的真实 OpenCode route、Ledger 和 rollback 证据。
7. 完成 macOS 与 Windows 的签名安装、sidecar hash、凭据和升级/回退证据。
8. Optical active 继续默认关闭，直到真实 Provider fidelity/cost/latency Gate 全部通过。

## 维护规则

- AiMaMi 上游版本变化时更新固定 commit，不静默混用多个版本。
- d-token 代码状态变化时先更新正式事实文档，再更新本研究。
- 借鉴范围变化时只更新唯一 `docs/legal/THIRD_PARTY_NOTICES.md`。
- 研究建议进入实施计划前必须与 PRD 和 PLAN 对齐。
- 当前测试计数属于 2026-07-12 阶段证据，后续不得长期沿用为当前事实。
