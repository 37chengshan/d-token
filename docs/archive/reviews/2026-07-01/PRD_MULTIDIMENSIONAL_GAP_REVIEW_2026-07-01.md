# PRD 多维度遗漏审查报告

日期：2026-07-01  
审查对象：`docs/PRD.md`  
结论：PRD 主线正确，但在本轮审查前存在若干工程化落地遗漏；已通过直接修改 `docs/PRD.md` 补齐，不另起 PRD 版本。

## 1. 总结论

PRD 的核心方向是正确的：

```text
RTK = Command Execution Layer / Shell Output Layer
Headroom = API Gateway Layer / LLM Context Layer
d-token = Desktop orchestration + routing + observability + policy + installer
```

但原 PRD 对“用户如何一键装好、如何按项目管理 Headroom 压缩等级、如何防止双层 savings 重复计算、如何安全修改配置、如何实现之前问题修复”写得不够完整。

本轮已补入：

1. One-click installer / setup wizard。
2. Selective tool installation。
3. ProjectProfile。
4. ProjectCompressionPolicy。
5. CompressionRecommendationEngine。
6. Config diff / backup / rollback。
7. Headroom Doctor checks。
8. RTK + Headroom double-count prevention。
9. CI gate：structure check、cargo check、cargo test。
10. M1 新增 `dtoken-project-policy` 与 config rollback skeleton。

## 2. 维度一：产品完整性审查

评分：从 8.4/10 提升到 9.3/10。

### 原遗漏

1. 已有 RTK + Headroom 分层压缩，但没有完整描述用户如何安装和配置。
2. 已有 Dashboard 指标，但没有项目级策略管理。
3. 已有 savings 分层，但没有明确防重复统计公式。
4. 已有 RTK 集成模式，但没有 Headroom runtime 的可选安装策略。

### 已补齐

1. 新增“一键安装与配置向导”。
2. 新增“工具选择矩阵”。
3. 新增“Headroom 项目级压缩策略”。
4. 新增“推荐等级引擎”。
5. 新增“防重复统计”。

## 3. 维度二：用户体验审查

评分：从 7.8/10 提升到 9.2/10。

### 原遗漏

用户虽然能看到 RTK、Headroom、provider、route、instance，但没有一个明确的“从零到可用”的新手路径。

### 已补齐

新手路径已改为：

```text
打开 Desktop
  ↓
系统检测
  ↓
选择安装组件
  ↓
选择目标 agent
  ↓
显示配置 diff
  ↓
创建 backup
  ↓
执行安装
  ↓
运行 Doctor
  ↓
生成 rollback point
  ↓
进入 Dashboard
```

用户可以选择：

1. 只装 d-token core。
2. 只接入外部 RTK。
3. 只装 Headroom-compatible runtime。
4. 同时启用 RTK + Headroom。
5. 只用 built-in fallback preprocessor。

## 4. 维度三：技术架构审查

评分：从 8.5/10 提升到 9.1/10。

### 原遗漏

1. 缺少项目级配置模型。
2. 缺少推荐算法输入信号。
3. 缺少配置修改事务模型。
4. M1 里没有体现 `dtoken-project-policy`。

### 已补齐

新增模型：

1. `ProjectProfile`。
2. `ProjectCompressionPolicy`。
3. `CompressionRecommendationEngine`。
4. `LayeredCompressionStage` 增加 `project_id`。
5. `PreprocessorProfile` 保留 RTK external / managed / builtin / custom 四种模式。

新增 M1 内容：

1. `dtoken-preprocessor` crate。
2. `dtoken-project-policy` crate。
3. RTK binary/version detection。
4. Headroom runtime/proxy detection。
5. config diff/backup/rollback skeleton。

## 5. 维度四：安全与隐私审查

评分：从 8.8/10 提升到 9.4/10。

### 原遗漏

1. 只写了不上传用户数据，但没有明确 Headroom CCR 原文缓存的读取边界。
2. 只写了不静默修改 RTK config，但没有覆盖 Headroom config 和 agent config。
3. 没有明确 backup/rollback 是安全要求。

### 已补齐

新增要求：

1. 修改 agent/RTK/Headroom 配置前必须显示 diff。
2. 修改前必须创建 backup。
3. 必须支持 rollback。
4. 默认不读取 Headroom CCR 原文缓存，只显示摘要、大小、数量和检索状态。
5. 默认不读取 RTK tee 完整内容，只显示路径和摘要。

## 6. 维度五：数据统计审查

评分：从 8.0/10 提升到 9.3/10。

### 原遗漏

PRD 写了 `rtk_savings`、`headroom_savings`、`end_to_end_savings`，但没有明确如何避免重复统计。

### 已补齐

新增计算规则：

```text
raw_cli_tokens
  → rtk_cleaned_tokens
  → headroom_final_tokens

rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
headroom_savings = headroom_input_tokens - headroom_final_tokens
end_to_end_savings = raw_cli_tokens + non_cli_raw_context_tokens - final_llm_input_tokens
```

并明确禁止直接把 `rtk_savings + headroom_savings` 当作总 savings，除非 baseline 已统一。

## 7. 维度六：工程落地审查

评分：从 7.6/10 提升到 8.8/10。

### 原问题

1. M1 已写 Rust skeleton，但还没有远端运行 `cargo check`。
2. 旧 `.github/workflows` 清理存在阻塞。
3. CI gate 不明确。
4. M1 与新 PRD 中 RTK/Headroom adapter 要求不完全一致。

### 已补齐

PRD M5 新增：

1. CI gate：structure check。
2. CI gate：cargo check。
3. CI gate：cargo test。

M1 新增：

1. `dtoken-preprocessor`。
2. `dtoken-project-policy`。
3. Headroom runtime/proxy detection。
4. config diff/backup/rollback skeleton。

## 8. 剩余风险

1. RTK 的 hook 行为依赖不同 agent 的 JSON 格式和 hook 能力，必须做 compatibility matrix。
2. Native Windows 下 RTK 自动 rewrite 能力有限，必须优先提示 WSL 路径。
3. Headroom 的 wrap/proxy/MCP/runtime 依赖 Python 环境，Windows/macOS 安装体验必须重点打磨。
4. Headroom 项目级压缩推荐需要真实数据训练/启发式规则，M1/M2 只能先 mock。
5. 双层 savings attribution 需要统一 request_id、command_id、project_id，否则统计会漂移。
6. 配置 diff/rollback 必须覆盖 agent config、RTK config、Headroom config、d-token config 四类文件。

## 9. 最终判断

PRD 已经从“架构正确”升级为“可落地规划”。

下一步开发优先级应从原来的单纯 M1 core skeleton，调整为：

```text
M1A: 修正 cargo check / cargo test / structure check
M1B: 新增 dtoken-preprocessor crate
M1C: 新增 dtoken-project-policy crate
M1D: 新增 RTK/Headroom detection endpoints
M1E: 新增 config diff/backup/rollback skeleton
M2: Desktop one-click installer + Dashboard skeleton
```
