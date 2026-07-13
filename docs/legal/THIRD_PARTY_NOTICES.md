# 第三方版权与修改声明

更新时间：2026-07-12

本文件是 d-token 当前代码中直接适配第三方实现的唯一版权、来源与修改声明入口。产品边界仍以 [`docs/PRD.md`](../PRD.md) 为准；本文件不授权使用任何第三方商标或品牌资产。

## AiMaMi

- 项目：[`borawong/AiMaMi`](https://github.com/borawong/AiMaMi)
- 固定来源：[`297c7af56f10fb371b77bc9b6b65aa320afcbe7e`](https://github.com/borawong/AiMaMi/commit/297c7af56f10fb371b77bc9b6b65aa320afcbe7e)
- 上游版权声明：`Copyright 2025-2026 borawong`
- 上游许可证：Apache License 2.0
- 本仓库保留的许可证副本：[`AIMAMI_APACHE_2_0_LICENSE.txt`](AIMAMI_APACHE_2_0_LICENSE.txt)
- 核验证据：固定 commit 的 [`LICENSE`](https://github.com/borawong/AiMaMi/blob/297c7af56f10fb371b77bc9b6b65aa320afcbe7e/LICENSE)；该 commit 没有根目录 `NOTICE` 文件。

### 适配范围

以下 d-token 文件含有从固定版本适配或保守按衍生实现管理的结构与交互：

| d-token 文件 | AiMaMi 来源 | d-token 修改边界 |
| --- | --- | --- |
| `code/apps/desktop/src/app/components/shell/PageStage.tsx` | `src/components/layout/page-stage.tsx` | 改用 `motion/react` 与 `AnimatePresence` 生命周期；保留 140–240ms 低幅度转场意图；加入显式 Reduce Motion 分支及 d-token 间距。 |
| `code/apps/desktop/src/app/components/shell/AnimatedSegmentedControl.tsx` | `src/components/ui/animated-segmented-control.tsx` | 改为 d-token 泛型 API、等宽指示器和产品样式；重写焦点移动与键盘语义；加入 Reduce Motion 数据状态。 |
| `code/apps/desktop/src/app/components/shell/Sidebar.tsx` 与 `code/apps/desktop/src/styles/shell.css` | `src/components/layout/sidebar.tsx`、`src/components/ui/sidebar.tsx` 的 inset shell、展开/折叠与主题分段结构 | 改为 d-token 六个一级页面、184/64px 布局、48px 顶部基线、跨平台窗口边界、独立导航动效、d-token 设计 token 与响应式规则。 |

`Surface.tsx` 只借鉴 AiMaMi 的 grouped-panel 视觉原则并由 d-token 独立实现；`AnimatedNavIcon.tsx` 的图形与动画配方为 d-token 自有实现。它们仍在源码注释中明确参考边界，但不声明复制 AiMaMi 品牌或图标资产。

### 明确未包含

d-token 未复制或分发 AiMaMi 的名称、Logo、应用图标、截图、人物、二维码、品牌文案、商标资产、声音资源或上游第三方 Type4Me 音频。AiMaMi 固定 commit 的 `LICENSES/THIRD_PARTY.md` 只声明 Type4Me 声音资产；由于这些资产没有进入 d-token，该项不属于本衍生范围。

### 修改声明

上述 d-token 文件均已修改，不是 AiMaMi 原文件的未改副本。d-token 保留自己的产品信息架构、字体、品牌、双语文案、真实数据合同、安全边界与 Windows/macOS 共享实现；Apache-2.0 不授予使用 AiMaMi 商标或品牌资产的权利。

## pxpipe 评测配方参考

- 项目：[`teamchong/pxpipe`](https://github.com/teamchong/pxpipe)
- 固定来源：[`7b7d6efa42926a73b1bd709b5f6d1af1ae679947`](https://github.com/teamchong/pxpipe/commit/7b7d6efa42926a73b1bd709b5f6d1af1ae679947)
- 上游版权声明：`Copyright (c) 2026 claude-image-proxy contributors`
- 上游许可证：MIT
- 使用边界：只参考 768×1932、Spleen 5×8、reflow/newline marker、IDS、factsheet 和 recent-six 保护等评测配方；d-token 的 Rust renderer、Hybrid分区、grader和运行器为独立实现，没有复制上游TypeScript实现。
- 1928×1928 profile只作为历史headline研究轨道，不声明为当前pxpipe生产profile。

## Spleen 5×8

- 项目：[`fcambus/spleen`](https://github.com/fcambus/spleen)
- 来源文件：[`spleen-5x8.bdf`](https://github.com/fcambus/spleen/blob/master/spleen-5x8.bdf)，版本2.2.0
- 上游版权声明：`Copyright (c) 2018-2026, Frederic Cambus`
- 上游许可证：BSD-2-Clause
- 分发范围：`code/crates/dtoken-compress/src/optical/spleen5x8.rs`只包含可打印ASCII 32–126的5×8位图；R/B profile对同一位图进行确定性整数缩放。Unifont没有进入本实现。

完整BSD-2-Clause声明如下：

```text
Copyright (c) 2018-2026, Frederic Cambus
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

* Redistributions of source code must retain the above copyright
  notice, this list of conditions and the following disclaimer.

* Redistributions in binary form must reproduce the above copyright
  notice, this list of conditions and the following disclaimer in the
  documentation and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS
BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```
