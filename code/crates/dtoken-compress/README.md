# dtoken-compress

Compression interface 与策略模块。

职责：

1. 定义 compression policy：off / safe / balanced / aggressive。
2. 对接 Headroom-compatible runtime。
3. 标准化 transforms applied。
4. 统计 tokens before / after / saved。
5. 提供 CCR retrieve 接口占位。
6. 为 Rust 侧逐步实现压缩能力预留边界。
