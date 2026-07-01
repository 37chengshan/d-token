# Headroom Dashboard 完整迁移清单

状态：Draft  
日期：2026-07-01

## 1. 顶部状态区

必须迁移：

- Version。
- Session / Historical 双视图切换。
- Health status。
- Updated time。
- Light / Dark theme。
- Live Feed toggle。
- Telemetry badge。
- Savings profile。

## 2. 核心指标区

必须迁移：

- Money saved。
- Token savings。
- Output tokens saved。
- Overhead。
- Throughput：input、compression、forward、generation。

## 3. Agent 使用分析区

必须迁移：

- Agent Usage。
- Before tokens。
- After tokens。
- Tokens saved。
- Savings percent。
- Agent rows。
- Coverage label。

## 4. 缓存与压缩质量区

必须迁移：

- Prefix cache by provider。
- Cache reads。
- Cache writes。
- Cache busts。
- Removed content breakdown。
- Savings over time。

## 5. Token 与性能区

必须迁移：

- Token usage。
- Before compression。
- Removed / filtered tokens。
- After compression。
- Output tokens。
- Provider breakdown。
- Performance。
- TTFB。
- Failed requests。
- Pipeline timing breakdown。

## 6. 明细表格区

必须迁移：

- Per-model token savings。
- Per-project savings。
- Recent requests table。
- Expandable request details。
- Original tokens。
- Compressed tokens。
- Tokens removed。
- Optimization time。
- Transforms applied。
- Waste signals per request。

## 7. 历史视图区

必须迁移：

- Historical proxy compression。
- JSON export。
- CSV export。
- Lifetime money saved。
- Lifetime tokens saved。
- Active days。
- Average saved per day。
- Hourly rollups。
- Daily rollups。
- Weekly rollups。
- Monthly rollups。

## 8. d-token 新增维度

每个 Dashboard 统计项都应尽量增加：

- ProviderProfile。
- RouteProfile。
- Instance。
- Chain mode。
- OS process status。
- Config source。

## 9. 最小验收标准

M2 完成时，即使使用 mock data，也必须有全部卡片和页面占位。

未接入真实数据的卡片只能显示：

- `Not connected yet`
- `No data yet`
- `Deferred to Mx`

不得因为暂未实现后端数据而删除 UI 区块。
