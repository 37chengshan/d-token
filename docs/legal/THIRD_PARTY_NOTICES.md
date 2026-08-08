# 第三方版权与修改声明

更新时间：2026-08-06

本文件保存维护者使用的详细版权、来源与修改记录；面向源码与安装包分发的权威声明位于仓库根目录 [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md)。产品边界仍以 [`docs/PRD.md`](../PRD.md) 为准；两份声明都不授权使用任何第三方商标或品牌资产。

## Headroom 行为对照基线

- 项目：[`headroomlabs-ai/headroom`](https://github.com/headroomlabs-ai/headroom)
- 当前固定对照：[`main`](https://github.com/headroomlabs-ai/headroom/tree/main) / [`01df2452529a86c689cf226fecd5918cc5d19676`](https://github.com/headroomlabs-ai/headroom/commit/01df2452529a86c689cf226fecd5918cc5d19676)（2026-08-02 获取；截至 2026-08-06 仍为本仓库评估基线）
- 最新稳定 tag：[`v0.34.0`](https://github.com/headroomlabs-ai/headroom/releases/tag/v0.34.0) / [`9fd5ae3d530b5d9eb9e9127e413235a5a3b2faf6`](https://github.com/headroomlabs-ai/headroom/commit/9fd5ae3d530b5d9eb9e9127e413235a5a3b2faf6)（2026-08-05 已发布，晚于当前固定对照；其增量尚未评估，未纳入能力矩阵，不作为完成口径）
- 前一发布对照：[`v0.33.0`](https://github.com/headroomlabs-ai/headroom/releases/tag/v0.33.0) / [`28aa53dc7ca51e687cc719c3fe160f3be50c6570`](https://github.com/headroomlabs-ai/headroom/commit/28aa53dc7ca51e687cc719c3fe160f3be50c6570)
- 历史对照：[`1725cd1f8376828849c0e48fd9008afa25d24fff`](https://github.com/headroomlabs-ai/headroom/commit/1725cd1f8376828849c0e48fd9008afa25d24fff)，仅用于解释 2026-07 的旧证据
- 上游许可证：Apache License 2.0；固定 commit 包含根目录 `NOTICE`
- 当前使用边界：只进行公开行为、接口和测试语义对照；`dtoken-learning`、Rust compressor registry、route-bound gateway compression 与现有 d-token runtime 均为本仓库独立实现，没有从该 commit 复制源码
- 后续边界：若迁移或修改上游源码，必须在代码进入仓库前补充许可证副本、NOTICE、逐文件来源、修改范围和保留版权声明；仅有同名接口、metadata endpoint 或布尔字段不得算作能力迁移

## LeanCTX / Context Bus / Context Relay 行为对照基线

- 项目：[`yvgude/lean-ctx`](https://github.com/yvgude/lean-ctx)（独立开源项目，非 Headroom 附属实现）
- 当前固定对照：[`v3.9.17`](https://github.com/yvgude/lean-ctx/releases/tag/v3.9.17) / [`cf8fce6bb1759a845088988cca189beaf70b05a9`](https://github.com/yvgude/lean-ctx/commit/cf8fce6bb1759a845088988cca189beaf70b05a9)（2026-08-04 发布）
- 上游版权声明：`NOTICE` 记录 `lean-ctx`，Copyright 2026 Yves Gugger，含第三方贡献者列表
- 上游许可证：Apache-2.0（项目总许可；根 `LICENSE`、README、Rust `Cargo.toml` 与 Claude plugin `manifest.json` 均声明 Apache-2.0）
- 子包例外（声明 MIT，单独列出，不并入项目总许可）：`clients/python`（`pyproject.toml`）、`rust/crates/grammar-addons/lua`（`Cargo.toml`）
- 本地只读研究副本：`/Users/cc/Documents/d-token-external/lean-ctx/`（git clone，HEAD 固定 v3.9.17 / `cf8fce6b`；官方 source archive SHA-256 `a1178c682afcc4cbea3021338ac878761d8e8df4da03bf40bdd6f807bbdeac25` 已核验）
- 与 Headroom 的关系：headroom 上游 [`#2677`](https://github.com/headroomlabs-ai/headroom/pull/2677)（2026-07-31 合并）已移除对 rtk/lean-ctx 的下载、安装与配置，二者不再有集成关系；d-token 的 LeanCTX 对齐层按 `docs/PRD.md` P0.0.5/§4.1.1 由 d-token-owned Rust service 提供
- 当前使用边界：仅公开行为/接口对照，不复制源码、不引入运行依赖；未实现前状态必须为 `unavailable` 或 `partial`，不得宣称完成
- 后续边界：若迁移或修改上游源码，必须在代码进入仓库前补充许可证副本、NOTICE、逐文件来源、修改范围和保留版权声明

## CCR 来源判定

- 判定：CCR **不是独立开源项目**。按 headroom 官方源码与文档（README "Reversible (CCR)"、`headroom_retrieve` MCP 工具、`<<ccr:` 内联 marker），CCR 是 `headroomlabs-ai/headroom` 仓库内的可逆压缩存储能力（CacheAligner → ContentRouter → CCR 管线的一部分），没有独立上游仓库。
- 来源：随 Headroom 基线登记（见上文固定对照，Apache-2.0 + NOTICE），无需单独克隆。
- d-token 差异：d-token 的 CCR 为 Rust-owned 实现（加密、TTL、配额、权限、retrieve/delete、崩溃恢复、精确删除），与 LeanCTX 职责分离（`docs/PRD.md` P0.0.5 表格）；普通日志/Dashboard/Ledger/export 只保存 metadata，不保存 CCR 原文。

## AiMaMi

- 项目：[`borawong/AiMaMi`](https://github.com/borawong/AiMaMi)
- 固定来源：[`297c7af56f10fb371b77bc9b6b65aa320afcbe7e`](https://github.com/borawong/AiMaMi/commit/297c7af56f10fb371b77bc9b6b65aa320afcbe7e)
- 上游版权声明：`Copyright 2025-2026 borawong`
- 上游许可证：Apache License 2.0
- 本仓库保留的许可证副本：[`AIMAMI_APACHE_2_0_LICENSE.txt`](AIMAMI_APACHE_2_0_LICENSE.txt)
- 核验证据：固定 commit 的 [`LICENSE`](https://github.com/borawong/AiMaMi/blob/297c7af56f10fb371b77bc9b6b65aa320afcbe7e/LICENSE)（Apache-2.0，SHA-256 `777e0c280d6264292c86f48a365626717aec5c31a8a4b740b1fd5c37a88ea28a`，与仓库内 `AIMAMI_APACHE_2_0_LICENSE.txt` 逐字节一致）；该 commit 没有根目录 `NOTICE` 文件。

### 适配范围

以下 d-token 文件含有从固定版本适配或保守按衍生实现管理的结构与交互：

| d-token 文件 | AiMaMi 来源 | d-token 修改边界 |
| --- | --- | --- |
| `code/apps/desktop/src/app/components/shell/PageStage.tsx` | `src/components/layout/page-stage.tsx` | 改为 d-token 的 CSS 属性驱动包装（`shell-page-stage` + `data-page-motion`/`data-reduced-motion`，由 shell.css 消费）；上游内联 140–240ms 状态转场改走 d-token motion token 体系；加入显式 Reduce Motion 数据状态、d-token 间距及惰性内容标题就绪度测量。 |
| `code/apps/desktop/src/app/components/shell/AnimatedSegmentedControl.tsx` | `src/components/ui/animated-segmented-control.tsx` | 改为 d-token 泛型 API、等宽指示器和产品样式；重写焦点移动与键盘语义；加入 Reduce Motion 数据状态。 |
| `code/apps/desktop/src/app/components/shell/Sidebar.tsx` 与 `code/apps/desktop/src/styles/shell.css` | `src/components/layout/sidebar.tsx`、`src/components/ui/sidebar.tsx` 的 inset shell、展开/折叠与主题分段结构 | 改为 d-token 八个一级页面（含代码图谱实验页与视觉压缩 Beta）、184/64px 布局、48px 顶部基线、跨平台窗口边界、独立导航动效、d-token 设计 token 与响应式规则。 |

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
- 使用边界：参考 768×1932、Spleen 5×8、reflow/newline marker、IDS、factsheet、recent-six 保护，以及“本地代理在转发前将旧上下文替换为图片并保留当前请求”的公开行为；d-token 的 Rust renderer、灰度 AA、请求 IR/patcher、Agent bridge、Hybrid 分区、grader和运行器为独立实现，不是 pxpipe 的像素级或 TypeScript 实现复制。
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
# codebase-memory-mcp

d-token integrates an explicit user-import flow for the pinned headless
`DeusData/codebase-memory-mcp` v0.9.1-rc.1 binary (commit
`ee9833df2c72b750378273897ffec9410fc2c4f2`) under the MIT License. Platform
archive and extracted-binary hashes are recorded in
`code/runtimes/codebase-memory/artifact-manifest.json`. The d-token installer
contains the restricted launcher and upstream legal text, but not the upstream
executable. When a user imports the pinned archive, d-token verifies the
platform, architecture and SHA-256 and keeps the upstream `LICENSE` and
`THIRD_PARTY_NOTICES.md` with the imported component. The upstream installer,
updater and native web UI are not used.
