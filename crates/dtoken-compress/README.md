# dtoken-compress

压缩引擎模块。

## 职责

- 请求消息压缩。
- 工具输出压缩。
- 日志/JSON/代码/长文本内容识别。
- 原文缓存与 retrieval id。
- token before/after 估算。

## 策略

- `off`：不压缩，仅记录日志。
- `safe`：低风险压缩，删除重复和无关噪声。
- `balanced`：默认策略，压缩长上下文并保留关键路径、错误、符号和接口。
- `aggressive`：高压缩率策略，必须绑定原文可检索缓存。

## v0.2 MVP

先实现 stub：

```text
compress(input, policy) -> output
estimate_tokens(before) -> number
estimate_tokens(after) -> number
```
