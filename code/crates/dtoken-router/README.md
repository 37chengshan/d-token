# dtoken-router

ProviderProfile / RouteProfile 决策模块。

职责：

1. 读取 provider profiles。
2. 读取 route profiles。
3. 根据 instance、agent、protocol、model 做 route decision。
4. 记录 route decision metadata。
5. 不做静默 fallback。
6. 所有 fallback 必须显式配置并进入日志。
