# 前端动画与交互设计

## 1. 动画在桌面开发工具中的作用

动画不是装饰，主要承担四类信息：

1. **空间关系**：页面从哪里来，弹窗覆盖哪一层。
2. **状态变化**：服务启动、扫描、应用、验证和回退处于哪个阶段。
3. **操作反馈**：用户的点击是否被接收，动作是否仍在执行。
4. **注意力管理**：只强调需要处理的异常，不让整个界面持续运动。

AiMaMi 的优势是把页面、导航和系统状态做成统一桌面体验。d-token 已适配其 PageStage、分段控制器和侧栏结构，但必须继续遵守自己的安全与可访问性边界。

## 2. 动画原则

### 2.1 低幅度、短时长

推荐基线：

- 微交互：80–140ms。
- 状态切换：140–200ms。
- 页面进入：180–240ms。
- 危险确认：不使用弹簧式夸张运动。

位移通常控制在 4–12px，透明度和轻微缩放优先于大幅滑动。

### 2.2 动画不能掩盖真实性

状态卡从 loading 变为 verified 时可以过渡，但不能先显示绿色成功再等待真实结果。所有成功动效必须发生在后端 receipt 和 verify 完成之后。

### 2.3 动画不能改变操作语义

- 页面切换不能重置未提交表单。
- 折叠侧栏不能改变当前 page id。
- Dialog 关闭动画不能提前执行真实写操作。
- 列表重排不能造成焦点跳到其他危险按钮。

### 2.4 Reduce Motion 是产品能力

`prefers-reduced-motion` 下应关闭：

- 页面位移。
- 图标循环运动。
- 背景流场。
- 弹簧缩放。
- 无必要的数字滚动。

仍可保留瞬时颜色、边框和文本状态变化。

## 3. 页面转场

`PageStage.tsx` 应只负责页面级视觉生命周期，不负责数据加载。推荐：

```text
旧页面 80–120ms 淡出
→ 新页面 160–220ms 淡入并上移 4–8px
```

要求：

- `AnimatePresence` 使用稳定 page key。
- Suspense fallback 不应与页面转场形成两次明显闪烁。
- 页面进入时焦点保持在合理位置；键盘用户切换页面后应能感知新标题。
- 不使用 blur 作为主要动画，因为低性能机器和透明窗口下成本较高。

## 4. 侧栏交互

### 4.1 展开与折叠

侧栏宽度变化应统一由壳层控制。推荐：

- 展开宽度约 184–220px。
- 折叠宽度约 56–68px。
- 文字淡出略早于宽度收缩，避免挤压。
- 图标位置保持稳定，不在折叠时横向跳动。

### 4.2 导航图标

`AnimatedNavIcon` 应只在 hover、focus 或 active 时触发短动画，不应持续循环。状态变化要可由颜色、背景和文本同时识别。

### 4.3 键盘行为

- Tab 进入导航。
- 上下方向键可选做 roving focus，但不能破坏普通 Tab 顺序。
- Enter/Space 激活。
- 折叠按钮具有明确 `aria-expanded`。
- Tooltip 在键盘 focus 时同样出现。

## 5. 分段控制器

`AnimatedSegmentedControl` 适合主题、语言、时间范围和视图模式，但不适合危险操作。

正确实现包括：

- 指示器使用共享 layout id 或 transform 移动。
- 每项等宽或有稳定测量，避免文字变化时抖动。
- 支持方向键、Home 和 End。
- 焦点环不能被移动指示器遮挡。
- Reduce Motion 下直接切换，不做滑动。

路由 apply、删除凭据、停止服务等不能使用轻量 segmented control，因为会弱化风险感知。

## 6. 状态动画

### 6.1 服务启动

推荐状态序列：

```text
stopped
→ starting
→ verifying process identity
→ verifying listener
→ verifying health
→ running_verified
```

界面应显示当前阶段文本，而不是只有无限 spinner。失败时停止动画，并显示失败阶段和恢复结果。

### 6.2 配置事务

```text
planning
→ review_required
→ backing_up
→ applying
→ verifying
→ doctor_running
→ applied_verified
```

审查阶段禁止自动前进。用户确认后，步骤条可以逐步更新。若 verify 失败并成功恢复，应显示 `failed_restored`，而不是普通失败。

### 6.3 数据刷新

Runtime Evidence 每 2.5 秒轮询并指数退避。UI 不应每次刷新都闪烁 skeleton。推荐：

- 首次加载显示 skeleton。
- 后续刷新保留旧数据并显示小型更新时间指示。
- 数据过期后改变 freshness，不立刻清空全部内容。
- runtime unavailable 时保留本地 transaction/rollback evidence。

## 7. 图表动画

分析图表应谨慎：

- 首次进入可以短暂绘制。
- 后续轮询更新不应完整重播动画。
- 大量点更新时关闭逐点动画。
- `prefers-reduced-motion` 下禁用折线绘制和柱状增长。
- estimated、shadow 和 actual 使用不同线型或填充，而不仅是颜色。

## 8. 背景与装饰动画

d-token 已删除旧 `BackgroundBlobs` 并改用更克制的背景流场，这是合理方向。建议：

- 背景只在顶层壳存在一份。
- 低透明度、低频率、无持续重绘的复杂滤镜。
- 窗口失焦或隐藏时暂停。
- Reduce Motion 下静止。
- 不与状态颜色竞争注意力。

## 9. 危险动作的交互节奏

危险动作需要故意增加确认成本：

1. 先展示影响范围。
2. 展示精确 target 和 diff。
3. 展示当前 backup 状态。
4. 用户明确确认。
5. 后端重新校验 revision/hash。
6. 操作期间禁用重复提交。
7. 成功后展示 receipt 和 rollback id。

禁止：

- hover 即执行。
- toast 中只有“已保存”而没有验证结果。
- 使用乐观 UI 先把状态改成成功。
- 在动画结束回调中触发不可逆操作。

## 10. 性能预算

- 页面动画保持 transform/opacity，避免 layout thrashing。
- 减少 backdrop-filter 大面积叠加。
- 侧栏宽度动画可使用 CSS grid columns 或 transform，但要测试文本清晰度。
- 图表页面按路由懒加载。
- 大型图表库和安装工作台继续拆 chunk。
- 动画组件不应引入第二套重复动画库。

当前 d-token 使用 `motion/react`，应统一使用，不再混入其他动画框架。

## 11. 测试策略

### 单元测试

- Reduce Motion 分支。
- 分段控制器键盘行为。
- 侧栏折叠状态和焦点。
- 页面切换不会错误卸载共享 Runtime Evidence。
- 危险操作在 review 前不可执行。

### E2E

- 快速切换页面无白屏和重复加载。
- 窄窗口折叠导航可用。
- Dialog 打开、取消、确认和返回焦点正确。
- 动画过程中重复点击不会提交两次。
- runtime 状态变化后页面更新但不闪烁假数据。

### 真窗口验收

- macOS 透明度、拖拽区、窗口按钮和 Retina 清晰度。
- Windows 缩放 100%/125%/150%、标题栏和触控板滚动。
- Reduce Motion 系统设置真实生效。
- 低性能环境下无明显掉帧。

## 12. 当前缺陷与改进

1. Radix Dialog wrapper 未正确 forwardRef，已经在测试 stderr 中暴露，应优先修复。
2. 动画组件和壳层正在大规模变更，需补充真窗口焦点与布局回归。
3. 页面 chunk 较大时，转场可能掩盖加载延迟，应显示轻量且稳定的 fallback。
4. 状态刷新应避免每次触发入场动画。
5. 动画相关适配必须继续保留 Apache-2.0 来源与修改声明。