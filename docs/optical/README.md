# d-token Optical Compression — 方案汇总索引

本文件夹汇总 d-token Optical Compression（视觉压缩）的全部方案、实现状态、数据结构和接口合同，供实施和审查使用。

## 真理源

- 核心算法与框架计划：`docs/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`（1658 行，V6 完整规格）
- Phase A 实现报告：`docs/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md`

## 本文件夹文件

1. `README.md`：本索引。
2. `overview.md`：方案一句话摘要、产品边界、默认行为、禁止承诺。
3. `architecture.md`：总体架构管线、模块框架、依赖分层、核心原则。
4. `data-structures.md`：全部公开数据结构（enums/structs）汇总。
5. `implementation-status.md`：Phase A-E 实现状态矩阵。
6. `test-matrix.md`：测试矩阵（gate 测试 + plan 层测试）。
7. `release-gates.md`：发布门槛与完整性标准。

## 产品边界一句话

视觉压缩是一个面向 AI Coding 长上下文的**计划层系统**：先判断哪些内容可以安全视觉化，再判断是否经济划算，最后才允许渲染和协议改写。默认关闭，第一阶段只做 shadow plan，不改写请求。

## 当前分支

```text
optical
```
