# dtoken-router

路由决策模块。

## 职责

- 加载 provider profiles。
- 加载 route profiles。
- 根据 agent、protocol、端口、instance_id 匹配目标上游。
- 判断 direct / via_cc_switch。
- 生成 route decision。
- 禁止第三方 provider 静默回落到官方 provider。

## RouteDecision 草案

```ts
interface RouteDecision {
  requestId: string;
  routeProfileId: string;
  targetProviderId: string;
  chain: 'direct' | 'via_cc_switch';
  inputProtocol: 'anthropic' | 'openai';
  outputProtocol: 'anthropic' | 'openai';
  compressionPolicy: 'off' | 'safe' | 'balanced' | 'aggressive';
}
```
