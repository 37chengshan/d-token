# Optical Compression 方案概述

## 一句话

视觉压缩不是普通文本压缩，也不是直接把 prompt 画成图片。它是一个面向 AI Coding 长上下文的计划层系统：先判断哪些内容可以安全视觉化，再判断是否经济划算，最后才允许渲染和协议改写。

## 默认行为

```text
默认关闭。
第一阶段只做 shadow plan，不改写请求。
第一批 active 只允许 Anthropic /v1/messages 的大块 tool_result。
current user turn、recent tail、secret-like、schema validator、open tool-call、diff/patch 默认保持文本。
unknown model、unknown provider、unknown endpoint 只能 shadow 或 pass-through。
```

## 用户可见承诺

```text
d-token 可以把低精确性、稳定、过长、非当前任务核心的上下文块规划为视觉候选，并用 metadata 展示每个候选为什么可压、为什么跳过、预计节省多少、是否会破坏缓存稳定性。
```

## 禁止承诺

```text
无损压缩。
保证省钱。
所有模型都支持。
hash、路径、命令、ID 可以只靠图片读取。
可以绕过 provider 规则或计费模型。
```

## 核心原则

```text
1. Planner owns decisions. Renderer never decides eligibility.
2. Shadow plan must preserve body unchanged.
3. Active plan must pass validation before renderer runs.
4. Public API only returns summaries, never raw prompt/image/base64/exact values.
5. Estimator、renderer、UI 必须共用同一个 RenderGeometryProfile。
6. Cache-unstable candidates cannot active-transform.
7. Unknown model cannot active-transform.
8. Every candidate must end with exactly one CandidateAction and one CandidateReason.
9. Every failure path must fail closed to original body.
```
