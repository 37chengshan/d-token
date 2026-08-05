# d-token 发布前综合审查与最佳迭代方案

- 综合日期：2026-07-16
- 定位：对三份并行深度审查报告的**交叉综合、把关与统一迭代路线图**。这是给决策者的顶层文档，细节见下列子报告。
- 审查方式：三个独立审查 agent 基于**当前源码事实**（非文档宣称）审查，主审查者通读三份报告后做交叉验证、亲自核实关键存疑点、收敛跨领域根因。

## 0. 三份子报告索引

| 报告 | 文件 | 就绪度 | CRITICAL/HIGH |
|---|---|---|---|
| 前端 UI 与前后端对接 | `claude report/FRONTEND_AND_INTEGRATION_REVIEW_2026-07-16.md` | A- / 88 | 0 / 3 |
| 后端各层能力 | `claude report/BACKEND_CAPABILITIES_REVIEW_2026-07-16.md` | 62 / 100（§8 迁移 ~45-50%） | 3 / 7 |
| 用户流程与整体性能 | `claude report/UX_FLOW_AND_PERFORMANCE_REVIEW_2026-07-16.md` | 体验 8/10、性能 6.5/10 | 0 / 4 |

---

## 1. 核心判断：前端成熟度与后端完整度的结构性倒挂

d-token 呈现一个清晰且重要的结构性特征：

> **精装修的房子，但很多房间没通水电。**

- **前端外壳（88 分）** 是纪律性极强、契约驱动的工程：`EvidenceSlice` 统一状态模型真实落地、单一所有者、event-first、诚实空态（`—`/verified-empty/unavailable 严格区分）、无假数据/假时间/工程术语泄漏、可访问性达标。
- **后端能力（62 分）** 呈现"深度好、广度不足"：安全边界、事务/回退、进程身份验证、CCR 加密、类型化发送管线是**可发布级别**；但 memory/SharedContext、MCP、output shaping、大部分 codec、integrated RTK、chain_bridge 二次转发仍是**占位或仅存储层**，Headroom §8 全能力迁移只完成约 45-50%。

**关键洞察**：前端的"诚实"（无数据即显示 unavailable）恰好**掩盖了后端的"空洞"**——用户看到的是精致的界面，但相当一部分数据格显示"暂不可用"，因为它对接的后端能力尚未通电。这不是 bug，而是**发布标准与实现进度之间的差距被诚实地呈现了出来**。

**由此推出全局最重要的决策**（见 §3）：**用哪个标准发布**——PRD 把 §8 全能力设为发布门槛（很远），但如果收敛为"封闭内测 + 明确已知限制"（很近）。

---

## 2. 交叉验证：高置信度共识（多份报告互相印证）

以下问题被 ≥2 份报告独立发现，置信度最高，应优先处理：

### 2.1 单文件巨型化（前端 + 后端 + 性能三方一致）
- 后端：`main.rs` **13,007 行**（PRD/PLAN 记为 ~6000，已翻倍）、`core-api` 12,080、`compress` 14,801、`config-patcher` 4,356。
- 前端：`Doctor.tsx` 1773、`VisualCompression.tsx` 1612、`Advanced.tsx` 1332、`Install.tsx` 1191、`Overview.tsx` 980（5 个页面超 800 行自定上限）。
- **统一根因**：逻辑与呈现/职责耦合。后端把安全关键逻辑（进程身份、服务生命周期、Doctor executor）堆在 Tauri `main.rs` 而独立的 `dtoken-process` crate 是死桩；前端把 payload→view-model 派生逻辑堆在页面组件。**违反 PLAN §1C.0 "Tauri 不承载业务规则" 的单向分层。**

### 2.2 对接边界未类型化（前端 H-2 + 后端 C-2 两侧）
- 前端：`get_desktop_data`/`get_desktop_summary` 返回 `Record<string, any>`，全前端 109 处 `any`，无 schema 校验；后端契约变更时前端编译期零告警，只会运行时静默降级。
- 后端：同一 payload 由 13k 行 `main.rs` 手工组装，两个 `invoke_handler` 列表（debug/release）手工维护易漂移。
- **这是前后端对接最大的共同技术债**：契约两侧都缺强类型约束。

### 2.3 单一大 Context 无 selector（前端 H-3 + 性能 HIGH-3）
- 运行证据走单一 `RuntimeEvidenceContext`，任一 slice 变化触发全体消费者（Overview/StatusBar/当前页）重渲染。PLAN §1B.6 Gate3 自认 partial。数据量增大后是首要卡顿来源。native 侧 `slice_revisions` 已就绪，缺前端 selector 订阅。

### 2.4 长列表无虚拟化只做硬截断（性能 HIGH-1，前端隐含）
- 日志 `.slice(0,80)`、学习候选 `.slice(0,8/16)`，全仓无虚拟化库。既是性能预算 PLAN §1B.4 #10 未达标，也是**功能缺陷**（用户看不全日志，无"加载更多"）。

### 2.5 "已优化 vs 已实测"背离（性能 HIGH-4，全局）
- 性能架构重构扎实（串行退出动画已消除、Advanced 3s 轮询已删、summary/detail 双层拆分、event-first 调度器、bundle 157KiB 过门），但冷/暖启动、双平台、CPU/RSS/IPC、soak 全无 release 实测证据。按 PRD §19 #40 **当前不得对外宣称"流畅/低占用/高性能"**。

---

## 3. 把关补充：主审查者亲自核实的两处关键背离

### 3.1 memory / SharedContext / output shaping 存储层孤立（已确认，发布阻断）
后端报告 C-1：`dtoken-context-store` 有完整 memory schema v4（`upsert_memory`/`list_visible_memory`/`grant_memory_share` 等），但 `dtoken-core-api` **零调用**——这些函数仅被 context-store 自身与其测试引用。`/memory/*`、`/output-shape` 全部 metadata-only，注释明写 "not yet applied to outbound requests"。**"盖了房子没通电"最典型的一处。** PRD §8 将这些列为发布门槛。

### 3.2 cc-switch 二次转发（chain_bridge）有定义无执行（主审查者已核实确认）
后端报告将此标为"需进一步验证"。**我亲自核实 `dtoken-core-api/src/lib.rs` 确认**：
- `ChainMode::ChainBridge` 仅出现在日志（:299/:5900）、CSV 导出（:5865）、route 过滤（:7694）和状态报告端点（:7710 `"mode": "compress_then_chain_bridge"`、:7708 `"external_cc_switch_mutation": false`），以及测试构造。
- 转发函数只有 4 个**单跳** forward（`forward_openai_chat`/`forward_anthropic_messages` + stream），**无任何 `forward_to_cc_switch`/`second_hop`/`chain_forward` 二次转发执行**。
- **结论**：PRD §5.2 声称的"先压缩、再转发到 cc-switch、由 cc-switch 路由到 final"作为一等 `chain_mode` 能力**没有专门执行路径**，只有 route 定义与状态报告。它若能工作，只能退化为"把 cc-switch 本地端口当作普通 provider base_url 做单跳转发"。这需要**产品澄清 cc-switch 用法定位**（见 §4 决策 B）。

> 说明：cc-switch 的**只读发现/同步**（读 cc-switch SQLite、UI 显示 profile/upstream/synced 状态）是真实的（`upstream_discovery.rs`）。缺的是**请求侧的链式二次转发执行**。

---

## 4. 需要产品拍板的关键决策

以下三点决定发布范围与工期，建议优先拍板：

### 决策 A（最高优先）：发布标准 —— 路线甲 vs 路线乙
- **路线甲（按 PRD 原标准）**：补齐 §8 全能力（memory 接入、MCP crate、Code AST、output shaping、integrated RTK、chain_bridge 执行）才发布。工期长，且逼迫为"凑全能力"而降低已实现部分质量。
- **路线乙（诚实收敛，强烈推荐）**：改 PRD §8，把 memory/MCP/Code/RTK/output/chain_bridge 明确移入"后续版本"，内测只承诺已真实的能力（proxy/五类路由中的单跳/压缩子集/CCR 门控/Ledger/Doctor/OpenCode 接入）。用"缩小承诺范围"而非"假装完成"来满足 PRD §P0.2(15)"未完成能力计入发布阻断"。
- **建议**：先做路线乙的诚实盘点（低成本），再按资源决定甲的范围。三份报告的独立结论一致指向乙。

### 决策 B：cc-switch 用法定位
- 选项 1（低成本）：官方文档 + UI 明确 cc-switch = "把其本地端口配置为普通 provider，单跳转发"，删除/降级独立的 `chain_bridge` chain_mode 宣称。当前代码即可支撑。
- 选项 2（高成本）：补真正的 chain_bridge 执行路径（协议转换 + 二次转发 + Ledger 两跳记录）。
- **建议**：内测选选项 1，把 chain_bridge 一等能力列入后续版本。

### 决策 C：性能声明口径
- 在 release build 冷/暖启动、双平台、CPU/RSS/soak 实测闭环前，所有对外材料按 PRD §19 #40 **避免"流畅/低占用/高性能"绝对表述**，改用"本地优先、事件驱动、按需加载"等可核验描述。

---

## 5. 统一发布前迭代路线图

整合三份报告的修改计划，跨领域去重、按依赖与优先级排序。**"内测"指封闭内测 + 明确已知限制**（假设采纳路线乙）。

### 第 0 波：诚实盘点与承诺收敛（0.5-1 天，无代码风险，最先做）
| 项 | 内容 | 依赖 |
|---|---|---|
| W0-1 | 执行决策 A/B：改 PRD §8 将 memory/MCP/Code/RTK/output/chain_bridge 移入后续版本；PRD/PLAN/STATUS 三处口径一致 | 用户拍板 |
| W0-2 | 更新发布阻断清单与"已知限制"公开文案；性能声明按决策 C 收口 | W0-1 |

### 第 1 波：发布前必做（体验闭环 + 安全可测性，多数可并行）
| 项 | 内容 | 来源 | 依赖 |
|---|---|---|---|
| W1-1 | 一键安装 needs-credential **内联补录弹窗**（不换页），复用 `store_provider_credential` | UX HIGH-2 | 无 |
| W1-2 | 日志/事件"加载更多"或游标分页（复用 `get_analytics_request_page` 范式） | UX HIGH-1 | 无 |
| W1-3 | 进程身份/服务生命周期从 `main.rs` **回迁 `dtoken-process`** 替换死桩 + 补单测 | 后端 C-2 | 无（可并行） |
| W1-4 | 压缩/协议/CCR 核心引入 **property/fuzz/fault 测试**（ContentRouter/JSON minify/hybrid IR/CCR envelope 不变量 + malformed corpus + crash recovery） | 后端 C-3 | 无（可并行） |
| W1-5 | 补 custom_openai + custom_anthropic + cc_switch(单跳) 三类 **真实 routed 实证**（Ledger 可重建） | 后端 H-4 | W1-3 稳定后 |
| W1-6 | 清理：删无引用 281KB PNG、移除 `PageStage` 死 `exit`、修 Compatibility 英文 `payload` 串、`_mock`/`fixture` 方法移入 `#[cfg(test)]` | 三报告 LOW | 无（可并行） |

### 第 2 波：发布前强烈建议（结构性治理，降低后续回归）
| 项 | 内容 | 来源 | 依赖 |
|---|---|---|---|
| W2-1 | desktop payload **建立强类型 + Zod 边界校验**，收敛核心 `any` | 前端 H-2 | 无 |
| W2-2 | 抽离超大页面 view-model（以 Overview 为样板→Doctor/VisualCompression/Advanced/Install），主文件 ≤550 行 | 前端 H-1 | W2-1 协同 |
| W2-3 | **selector store / slice 订阅**：优先迁 StatusBar + Overview（复用 native `slice_revisions`） | 前端 H-3 / UX HIGH-3 | W2-1 |
| W2-4 | VisualCompression 独立轮询迁入 `appRefreshScheduler`；discovery 激活收敛到安装页 | UX MEDIUM-1/2 | 无（可并行） |

### 第 3 波：公开"高性能"声明前必做（release 实测闭环）
| 项 | 内容 | 来源 | 依赖 |
|---|---|---|---|
| W3-1 | 引入轻量虚拟列表（`@tanstack/react-virtual`，route-lazy，守住 160KiB 门）替换硬截断 | UX HIGH-1 后半 | W2-3 |
| W3-2 | 同一候选 commit 的 macOS release build 冷/暖启动 p50/p95、完整进程组 CPU/RSS/IPC、WebKit Long Task、30 分钟 soak | UX HIGH-4 | W1-W2 定稿 |
| W3-3 | Windows 平台性能与 E2E 基线 + 签名/NSIS 实机 gate | UX MEDIUM-6 / PLAN M5 | W3-2 |
| W3-4 | macOS Developer ID/notarization/staple 实机执行 | PLAN M5.S1 | 凭据 |

### 并行机会总结
- 第 1 波：W1-1/W1-2/W1-6（前端体验+清理）‖ W1-3/W1-4（后端可测性+测试）三组互不依赖，可三线并行。
- 第 2 波：W2-1 是 W2-2/W2-3 的前置；W2-4 独立可并行。
- selector（W2-3）与虚拟化（W3-1）都依赖 slice 边界，宜同一波做，避免二次重构。

---

## 6. 后续版本（不阻塞内测，路线乙移出的能力）
memory/SharedContext 运行时接入、`dtoken-mcp` crate（stdio + compress/retrieve/stats handler）、Code AST 逐语言压缩、通用文本压缩、output shaping 执行链、integrated RTK 内部能力、Library/SDK、chain_bridge 真正二次转发、Optical production active（保持实验轨道默认关闭）。每项以逐能力 runtime+eval+Agent 证据（G2/G4/G5）验收。

---

## 7. 一句话结论
d-token 的**前端与用户流程已达封闭内测水准，核心链路（OpenCode→d-token→upstream）真实可用**；阻碍"按 PRD 原标准公开发布"的是**后端 §8 全能力广度（~45-50%）与占位/背离能力**。最优路径不是继续堆占位凑"全能力"，而是**诚实收敛发布承诺（W0）+ 加固已实现部分（W1）+ 结构性治理（W2）+ release 实测（W3）**——把"计入发布阻断"通过缩小承诺范围而非假装完成来解决。
