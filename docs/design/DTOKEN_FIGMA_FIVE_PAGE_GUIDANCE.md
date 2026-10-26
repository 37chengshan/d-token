# d-token 五页桌面设计实现说明

该交付把现有 React 的暖纸张、深墨色和橙色强调收敛为五个可评审页面：Configuration、Advanced Configuration、Analysis & Logs、Diagnostics、Settings。布局采用固定 184px 侧栏、32px 页面边距、12px 卡片圆角和 8px 控件圆角。页面标题、状态条、证据条和主内容按同一垂直节奏排列，避免每个 workbench 自己定义信息层级。

## 页面与组件

- **Configuration**：Compression Engine 与 Code Graph 是同级卡片；Agent setup 放在下方。模式选择必须先显示当前模式、fallback 和证据，再提供 Test/Scan/Apply。代码图谱未建立时使用 `unavailable`，不能用示例数字填空。
- **Advanced Configuration**：Jev 五 workflow 用同一行组件，字段顺序固定为标题、policy、fallback、状态；Smart learning、routing/cache、recovery/retention 分成三个次级卡片。不要把 provider/model/endpoint 再放到本页。
- **Analysis & Logs**：Overview/Requests/Changes/Logs 用 segmented control；请求时间线只显示脱敏 id、route、状态和 freshness。上下文节省没有 runtime evidence 时显示 `—` 与 `unavailable`。
- **Diagnostics**：Runtime、Provider、Jev、Agent、Security、Recovery 使用同一 diagnostic action card；每个可修复动作必须说明作用域、是否可回退和结果 correlation id。
- **Settings**：只放 appearance、privacy/retention、service lifecycle、about。路由、压缩策略、Jev workflow 不在这里出现。

## 交互与无障碍

1. 键盘顺序：页面标题 → tab/segmented control → 状态条（只读）→ 卡片内主控件 → evidence bar → 次级动作。Tab 组件使用 roving focus，左右键切换，Home/End 跳首尾。
2. 状态条和变更结果使用 `aria-live="polite"`；error/action-required 使用 `assertive`，但不得用 toast 代替持久错误说明。
3. 卡片不是按钮；只有真实动作使用 button，最小可点击区域 32px，focus-visible 使用 2px ring + 2px offset。状态不能只靠颜色，必须有文字标签。
4. Loading 保留卡片高度并显示 skeleton；Empty 给出原因和唯一主动作；Unavailable 说明 evidence source/freshness；Error 显示可恢复动作和 request/correlation id。
5. Apply/repair 打开变更预览对话框：焦点进入标题，Tab 顺序为 summary → diff → backup/evidence → Cancel → Apply；Escape 关闭且回到触发按钮。Apply 成功后焦点回到触发按钮并更新 live region。
6. 尊重 `prefers-reduced-motion`：取消页面切换位移、状态条 shimmer 和 toast transform，只保留颜色/文本变化。

## React 对齐约束

优先复用现有 `Surface`、`Badge`、`Button`、`Tabs`、`OperationDialog`、notification 和主题变量。仅在现有组件无法表达 evidence freshness、workflow row 或 diagnostic action 时新增局部组件，不新增顶级导航和新的颜色体系。把 `support/artifacts/figma-export/dtoken-design-tokens.json` 映射到 CSS custom properties；Figma 变量命名应保持 `color/*`、`space/*`、`radius/*`、`motion/*` 前缀。

## 待产品确认

- Analysis & Logs 默认时间窗口是 24h 还是最近 100 次请求。
- Jev workflow 的 shadow/active 切换是否需要在 Advanced 页面保留单独的确认步骤。
- Settings 的 service lifecycle 是否允许 stop，或只允许跳转 Diagnostics。
