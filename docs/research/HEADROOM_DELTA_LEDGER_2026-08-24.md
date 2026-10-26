# Headroom 上游 Delta Ledger（2026-08-24 扫描）

状态：ACTIVE — R3 可用性收敛轮 F1 产物
对照基线：`headroomlabs-ai/headroom`
- 我方固定对照基线：`0cb72f45b23bdf7129822b16dd1d4cb7d6e0b062`（main 快照，2026-08-04 获取，≈v0.33.0 之后）
- 本次扫描新基线（吸收完成后的统一 pin）：**`v0.36.5` / `34a551756222b24ea4bc064f4250b8e7d169daf1`**（2026-08-22 发布）
- Delta 规模：ahead 246 commits，覆盖 v0.34.0 / v0.35.0 / v0.36.0–v0.36.5 及 Unreleased

处置口径（2026-08-23 用户确认）：安全修复直接吸收；压缩算法/策略变更必须先过本仓 codec 评测集（每类 ≥128 property/semantic/rollback cases + 负收益/budget/cancel 门）再采纳；非压缩面变更只记录。

## 1. 处置分类

### 1.1 直接吸收（安全/正确性，随本轮 F2 落实，每条带回归测试与 provenance 记录）

| 上游项 | 内容 | d-token 对应面 |
|---|---|---|
| #2908 | CCR：advertise marker 前先验证 hash | dtoken-compress CCR marker 面（防伪造 marker） |
| #3094 | CCR：post-processing 失败时仍放行成功上游 turn | core-api 流式终态处理对照 |
| #2694/#2703 | CCR：不把 retrieval marker 持久化为原文 | CCR journal 写入面核对 |
| #2716/#2740 | kompress：v2 checkpoint 加载与 artifact 拒载 | 不适用（无 Python 推理栈），只记录模式 |
| 插件路径逃逸修复 | plugins/ 沙箱拒绝 `.`/`..`/NUL | dtoken-agent-bridge 路径校验已有等价（absolute/home-relative + symlink fail-closed），核对补漏 |
| Dashboard 回环门控 | request metadata 仅 loopback/trusted CIDR | d-token 全服务本就 127.0.0.1-only，核对即可 |
| telemetry opt-in | 默认关闭遥测 | d-token 无遥测，无需动作 |

### 1.2 评测门控后吸收（算法/策略，F3）

| 上游项 | 内容 | 评测门 |
|---|---|---|
| TextCrusher + CJK 分段 | 确定性抽取式散文压缩，CJK QA 保留率上游宣称 34%→91% | 在本仓 text codec 评测集上先写死阈值（压缩率/还原/CJK 样本/负收益），上游数字不作为验收 |
| #2718 `frozen_message_count` on /v1/compress | 已于 v0.33 轮吸收（`resolve_frozen_message_count` parity），核对 v0.34+ 语义漂移 | 现有 frozen-prefix proptest |
| PHP codec（CodeAwareCompressor） | 新语言覆盖 | 本轮只记录不吸收（code corpus 现为 9 语言，扩语言需新 corpus + tree-sitter gate） |
| #2691 lossless provider seam + marker-free /v1/compress 默认 | 输出形态变化 | 评估对本仓 marker 合同的影响后决定；不盲从 |
| SmartCrusher audit-safe mode | 受保护 pattern + fail-closed | 与本仓 secret/负收益门对齐后评估 |
| tokenizer per model / 计价修正（#2760/#2761/#2762） | 计量面 | 随 F2 验证后吸收到 capability registry seed/计价投影 |

### 1.3 协议面对照（C1 的输入）

| 上游项 | 内容 | 对 C1 的意义 |
|---|---|---|
| Codex `/v1/responses` 字节保真转发（zstd body 400 修复） | 上游是**转发保真**，不是 Responses 压缩 | 与 C1（Responses wire 压缩）不冲突：C1 在转发前对 input 做 resolved compression；字节保真约束保留（未压缩字段逐字节透传） |
| /v1/responses 输出塑形 + Codex WS 后帧修复 | 输出侧 | C1 保持响应侧透传不变，仅压缩请求 input |
| `store=false` 语义 | memory 注入遵守 | C1 无 memory 注入面，不适用 |
| Anthropic [1m] tier + HEADROOM_1M_MODEL | 1M 上下文档位 | capability registry seed 补充 [1m] 变体记录（记录级） |

### 1.4 只记录不跟随

- Podman/userns、容器部署、CLI 扩展 seam、beacon/routing summary、VS Code Copilot 透明代理（#2687/#3077/#3112）——上游 host-side 集成形态，d-token 无对应运行形态；其中 Copilot 相关仅作为 B3 阶段 VS Code Copilot Chat 调研的参考证据。
- 性能优化模式（#2838 token 计数 memo、27% 热路径削减）：Rust 侧已有 summary read model 单飞/缓存，模式记录不移植。

## 2. Responses 压缩：自研 vs 吸收决策（C1 前置）

**决策：自研实现，参考上游约束。** 理由：
1. 上游（Python proxy）对 `/v1/responses` 只做字节保真转发与输出塑形，**没有实现 Responses wire 的请求侧压缩**——吸收无可吸收的实现，只有合同约束可借鉴。
2. d-token 已有全部构件：`build_openai_responses_inventory`（shadow 解析已覆盖 input/tools/保护节点）、`patch_path_for` 缺口已定位（`/input/{i}/output` 映射）、`compress_for_route_with_frozen` 管线与负收益门成熟。
3. C1 改动面维持评审定稿的 6 处；新增两条硬约束：
   - 负收益门 parity（压缩后 token 不减则不压缩）必须与 Chat/Anthropic 同级；
   - Responses usage 语义（`output_tokens` 含 `reasoning_tokens`）下的 tokens_saved/claimable 投影需按 Responses 字段重验，无证据保持 fail-closed。
4. 字节保真约束保留：未被批准转换的 input item 与全部响应侧字段逐字节透传。

## 3. Provenance 统一（阶段 0.1 收口）

- 旧不一致：`code/runtimes/headroom/README.md` 记「参考 01df245 / v0.34.0 tag 未评估」，`docs/product/IMPLEMENTATION_STATUS.md` 记「固定对照基线 0cb72f4（2026-08-04）」——两个 commit 分属不同日期、不同口径。
- 统一结论：**单一基线定义 = 「最近一次完成 delta 扫描并完成处置的 upstream tag」**。本轮起基线为 `v0.36.5 / 34a551756222b24ea4bc064f4250b8e7d169daf1`（2026-08-22）；`0cb72f4` 降级为历史扫描记录。两处文档已同步（见下）。
- codebase-memory：pin `v0.9.1-rc.1 / ee9833d` → 上游已到 `v0.10.8 / 46ae198fc11cda80e817acbc5f5908d7c2de7032`（2026-08-19，stable，图谱正确性修复为主）。升级评估见 F4：需走既有 manifest/ARTIFACT_SPECS/cosign 流程并绑定图谱页验收，本轮内执行。

## 4. 验证记录

- 扫描方式：`gh api`（compare 0cb72f4...main = 246 commits；releases v0.34.0–v0.36.5 逐版本 body）+ CHANGELOG Unreleased 段抓取。
- C3 压缩管线健康基线（同日）：`support/artifacts/opencode-real-routed/real-routed-20260824T012956Z-28974.json` — 9/9 请求成功、semantic_active、物理节省 1,258 tokens、CCR 生命周期 PASS、配置精确恢复。F3 的 TextCrusher/CJK 评测以该管线为健康基线。
