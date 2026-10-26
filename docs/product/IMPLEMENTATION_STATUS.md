# d-token 当前实现状态

更新时间：2026-09-07（发布前功能与用户体验收尾；当前未提交工作树）
状态类型：代码事实快照，不替代 `docs/PRD.md` 与 `docs/PLAN.md`

> **当前与历史证据边界**：下方「2026-09-06 当前工作树」是本轮代码事实与验证边界；后续 2026-08-14/08-07 快照用于追溯，不能充当当前候选的发布证据。执行顺序以 `docs/PLAN.md` 为准。


2026-09-07 功能优先修复（覆盖上轮候选结论）：

- 后续恢复复核：路由写入失败后，仅在重新读取并核对原配置字节摘要后恢复原托管服务。任一文件恢复或服务重启失败都显示恢复未完成；错误正文不再作为“已回退”的证据。零工具成功显示连接未完成，部分失败保留成功工具；自动刷新不抹掉本次终态。请求日志的限流/超时增加用户可执行说明。
- 已暂停视觉调整。Doctor 将明确未配置的 Optical / 未安装的 cc-switch 识别为待设置，而非未知数据；真实 Preview 已完成核心服务修复、启动和复检。缺失/损坏配置仍区分，未降低未知和过期结果的写入门槛。
- OpenCode 原先仅注册 provider，未设置默认请求模型；安装入口现统一使用受管 transport，将 provider、默认模型和私有的原选择记录作为一个事务。支持旧自有 provider 更新、幂等重试、清除恢复原默认值；配置/记录变动、同名外部 provider 和符号链接阻止覆盖。普通安装不再走旧的 provider-only command。
- 安装预检失败发生在停启服务和写路由前；混合批次只应用通过预检的工具。事务后发生失败时按本次实际回执精确恢复，不能用清除全部接入代替回退。transport 应用阶段已按工具隔离失败，后续 Hybrid 部分完成回执触发该工具恢复，其他成功工具保留；仍待真实 App 批量点击复验。
- 当前源码前端 506 项全量、类型检查通过；新增混合预检/应用失败隔离和部分写入恢复用例通过。Rust 配置 crate 全量、OpenCode 11 项定向及原生发现回归通过；当前模块严格 Clippy 通过。随后全工作区 1072 项通过、6 项忽略；最终安装定向 35 项、类型检查及独立 debug Preview 构建通过。旧 release DMG 不能代表这些修改。
- 真实免费 MiMo 再试 `real-routed-20260906T155332Z-38784.json` 仍为 6 个 HTTP 429；原配置字节已核对恢复。OpenCode 默认接入的最新真实 App 点击链尚未复验；桌面工具再次报告锁屏，已通知用户。Goal 保持 ACTIVE，不宣称全部功能可用。


## 1. 2026-09-06 当前工作树

- **后续原生复核（22:50）**：发现恢复后的 Changes 仍调用 runtime 统计接口；此前专用接口在后端存在，但前端接线实际缺失，现已接回 `get_config_change_records`。原生服务停止时已实际读取本机备份；旧记录明确显示未通过并保持锁定，不再被旧 verified 字段误标通过。分析原生无请求时已合并空图表墙，已有数据明细保持；图谱文件选择器打开/取消与现有安装识别通过。设置强调色切换/恢复及右下角保存 Toast 在独立 Preview 实看，成功不再新增顶部标签或推动布局。上述是 Preview 原生证据，不冒充最终 release 全链路验收。

- **最后一轮实测纠偏**：原生 Doctor 的五类产品模块加配置同步项被前端误拒，已修复为明确的混合合同，并显示第六项；实际只读扫描通过，1/6 正常、5/6 完成，缺失项仍锁定修复。安装整卡点击、选择保留、独立详情弹窗和关闭后焦点均已在 macOS 验证。按用户指出的空态布局问题，Doctor 未扫描页已收为单一说明/扫描入口；不再显示空圆环，常规桌面结果页左右分列。
- **当前发布准备**：前端 502 项、类型检查通过。Chromium/WebKit 全量先通过 26 项，另两项因旧测试要求两个重复标签失败；更新为当前一个服务状态标签后，两项复检通过，禁用原生写入的断言保持。macOS release App/DMG、包内容、ad-hoc 签名和临时安装启动审计通过；当前 DMG SHA-256 为 `226800cd111408c2bee156a4b12f82458bf173d88adec6c771fa6af307bb7746`。npm/Cargo 上轮同依赖扫描无已知漏洞（Cargo 18 条警告另列）。当前源码的独立 Preview 已验证本节列出的交互，最终 release 全部原生点击链仍未完成。
- **真实模型仍未通过**：间隔后复试 `real-routed-20260906T140243Z-80548.json` 仍为 6 个 `upstream_http_429` 失败请求。正常配置已精确恢复、临时凭据已删除；没有压缩成功数据。签名公证/Windows 实包按 PLAN §6.1 分期，不把本机 ad-hoc 包称为正式分发完成。

- **当前收敛结果**：全工作区 Rust 1065 项通过、6 项忽略；严格 Clippy 与格式检查通过。阶段前端 486 项、类型检查和 Chromium/WebKit 28 项通过；引导按需加载使初始 JS 从约 207 KiB 回到 158 KiB（门限 176 KiB）。这些结果早于最后的原生辅助功能、背景与重复标签纠偏，最终候选仍须重验。
- **原生实测后的修正**：安装清单完整显示 23 项工具，初次“查看连接状态”只导航，不自动写入。整卡 button 会让原生辅助功能隐藏内部详情，现改为分组容器、独立可聚焦复选框与详情按钮，整卡鼠标选择继续保留。诊断未扫描时不再声称“没有待处理项”。底部常驻栏收敛为服务/连接/诊断与处理入口，端口等信息移入详情；旧绿色诊断不再覆盖当前过期或不完整状态。
- **最新用户视觉纠偏**：加深浅色底纸（外层 #e9e5dc、工作区 #f0ede5、纸面 #fbf8f2），设置强调色与深色主题保持各自规则。删除概览/分析/高级设置/日志等页面重复来源标签，保留无法操作时的原因、过期数据提示和重试；本轮未重排概览内容。
- **图谱边界已补强**：非终态与失败 JSON 优先于正向字段，必须有完成计数；CLI 输出、运行时间、取消与进程清理有界。实际 v0.10.8 私有目录中完成 8 节点/8 边索引和隔离查询。平台验证仅 macOS arm64 实机，不能据此声称 Windows 通过。

- **最新阶段：按已对齐约定实施发布前收尾**。恢复基准源码为 `5572998` 加 `99a7289` 的七个文件，基准构建 46 个 JS/CSS 与已安装 App 同名同字节；基准 54 文件/473 项测试、TypeScript、构建通过。随后用户已授权 Goal 完成全部发布前任务，正在实现新的局部页面约定；新源码不再要求与旧安装包资源相同。完整备份保存在 `.ai-bridge/frontend-baseline-recovery/20260906T112121Z/`，已安装 App 保持原样。

- **统一通知与图谱前端**。新增仅本次操作产生的 Toast 队列，右下角最多三条、两秒后退出，悬停/聚焦暂停且按剩余时间继续；通知历史与临时提示分开。查看打开并展开对应记录，关闭提示不会删除失败记录；重复事件返回真实记录 ID，并清除上次失败的详情/回执。公共 `OperationDialog` 支持行内错误。图谱接回本机组件发现、官方压缩包选择/确认导入、目录选择和失败恢复；路径收进详情，删除无用说明和包围层，保留独立状态。通知/Shell 27 项与图谱/弹窗/通知 40 项专项通过，最终实际窗口验证未完成。

- **Hybrid 事务与真实服务**。部分失败 verify 不会晋升整体成功，撤销失败的 capability 继续保留，验证和配置竞争早退会保存恢复 manifest；17 项事务/10 项 native capability 测试和对应严格 Clippy 通过。生命周期失败来自测试旧配置缺失 `log_path`；修正 fixture、重建真实代理后，隔离状态下服务启动/停止、空配置启动、Doctor 启动修复、托管重载通过。cc-switch 的真实重载测试使用隔离 route 配置，不扩展为用户真实数据库全流程证明。

- **当前验证与缺口**。本轮 `cargo test --workspace` 为 1062 passed / 6 ignored；这是图谱新增审查修复前的快照。图谱已有官方 v0.10.8 来源核验和小项目真实索引/查询，但独立审查发现索引非终态误报与子进程输出/超时清理边界，尚在补强。安装卡片、诊断和分析正在实施；最终全量前端、原生窗口、候选包及平台发布证据未闭合。免费 MiMo 最新真实压缩准备仍返回 HTTP 429（`real-routed-20260906T124219Z-40014.json`），原配置精确恢复，无真实压缩成功结论。

### 恢复前的工作记录（仅追溯，不代表当前页面已完成）

- **恢复前的分工记录**：用户负责前端重构，Codex 推进后端与真实功能验收。停止前安装工具曾改为完整显示已检测项，批量选择仍仅限可参与路由的工具；概览重复状态合并。110 项相关前端回归通过；这些修改现已备份，不是本次恢复后的页面。

- **图谱组件发现与压缩包导入**：原生层已只读识别配置目录、PATH 和常用安装位置；兼容性缓存绑定文件标识，状态返回实际使用来源。用户可选择官方压缩包，校验同一份受限字节后解压到新的自有目录；同时校验二进制，失败恢复原位置与导入记录，恢复失败返回 `recovery_required` 并清除前端旧就绪状态。主界面仅保留组件结果、官方下载与导入，路径和已解压目录入口收进详情。组件 crate 25 项、桌面相关 12 项测试及 1 项实际官方压缩包导入测试通过；严格 Clippy、575 项前端测试、类型检查与规范 Tauri Preview 打包通过。最后的文件选择器点击与导入后窗口反馈因 Mac 锁屏未完成，不能据此宣称整个图谱链路已验收。

- **非首页编辑风重构进行中**：按用户最新授权，新增仅非 overview 使用的 `data-workbench` 样式范围；标题、表面、控件采用暖纸/碳墨体系，设置已取消用户否定的双列说明栏和包围卡片，改为标题下的紧凑全宽设置行，普通分组标题 14px、正文 13px、辅助说明 12px；安装页已移除重复大标题与说明段，导航仅保留文字标签。移除图谱主界面的来源标签、内部状态解释与隐私长段落，项目来源统计默认收起；保存失败等可操作错误仍直接显示。当前仅阶段实现，整组原生视觉和功能验收未完成，不代表用户已认可最终外观。

- **MiMo 真实链路**：基础单请求已通过且正常配置精确恢复，但没有可压缩历史工具输出。后续压缩准备已补充脱敏失败分类；两次真实请求均确认上游 HTTP 429，其中 `real-routed-20260906T100144Z-83232.json` 使用临时公共示例项目、正常 HOME/config/state，6 条请求均失败。配置精确恢复、私有备份与验证凭据清理均已确认；没有真实压缩成功证据，不能把限流失败写成引擎验收通过。

- **后端完整工具清单**：`discover_upstream_candidates.agent_catalog` 从同一 Rust 注册表返回全部 23 项工具，`agent_bindings` 仍只包含真实检测项。扫描失败的 `installed` 为 `null`，未安装/未知项不可执行配置或清除；2 项 catalog 测试与已检测范围回归通过，接口说明已写入 Tauri README。

- **压缩结果可解释**：压缩关闭、上下文未识别、不支持、没有安全候选、无收益及校验失败均返回固定 `fallback_reason_code`，由实际引擎结果进入请求与事件元数据；成功变换不附加失败原因。256 项压缩测试通过，core 路由元数据与 MCP 原因码回归通过。

- **模型能力来自实际证据**：不再按观测次数推算上下文/输出容量；数值只来自实际提示，冲突采用保守值。成功上游工具调用和完整流终态可积累能力观测，请求 schema、普通 HTTP 成功、失败/取消/截断不算证据；参数必须为对象且同一响应全部工具调用有效。观测以私有原子文件持久化，串行保存避免旧快照覆盖新事实，迟绑定路径会先加载已有数据；协议仍为解析提示而非已持久化的观测字段。registry 14 项、core 201 项单元测试及 29 项集成测试通过（另 1 项忽略）。

- **Doctor 与连接同步的后端结果**：完整且有效的 `repairable/manual` 报告可进入修复，部分/未知/过期报告继续拒绝。手动 cc-switch 同步即使不启用自动跟随，也必须检查本地服务重新加载结果；失败精确恢复旧配置。只有明确证明服务已停止，才报告下次启动使用新配置，未知运行状态不能当作离线成功。定向 7 项原生回归通过；重启失败后可能仍需用户显式启动，不能把配置已恢复宣称为服务已恢复。

- **图谱恢复重试入口**：新增仅恢复未验证成功配置的入口，保留已验证连接；确认框列出目标工具，恢复失败直接在框内显示错误并允许再次执行。新增混合成功/待恢复状态回归，验证首次恢复失败后重试成功、两次调用均只针对失败工具；图谱与文案 24 项、类型检查通过。未通过真实工具配置制造故障，原生完整恢复仍需最终事务验收。

- **图谱逐工具失败隔离**：配置或验证失败仅恢复当前工具，保留成功连接并继续后续工具；页面报告完成数量与各项失败原因，恢复失败的回执仍保留供重试。回归覆盖第一项和末项验证失败、失败配置返回恢复事务；全量前端 566 项及类型检查通过，规范 Tauri Preview 构建完成。真实工具配置事务未在本轮桌面检查中执行。

- **图谱工具选择范围**：原页面仅以静态图标展示自动纳入的工具，现提供真正的复选框；已安装且支持项默认选中，未安装/不支持项禁用。取消选择在重新扫描后保留，预览仅包含选中工具，修改选择清除旧预览。新增回归验证取消 Codex 后重扫仍不选中，并只为 OpenCode 生成预览；重建原生 App 已实际验证取消选择和重新扫描保留选择。

- **图谱终态桌面复验与视觉压缩语言**：重建 App 中重试独立项目索引，无手动刷新即从 busy 变为失败并通知版本冲突原因，处理入口恢复；后台状态与界面一致。视觉压缩主流程改为连接规则、启动插件、恢复功能与压缩预览等任务说明，保留 Codex `/hooks` 信任要求和不改变实际请求的 Beta 边界；该文案已重新打包，并在原生 App 验证显示。

- **图谱索引失败根因与回执**：固定组件 v0.9.1-rc.1 与本机已有 v0.10.8 会话发生同用户进程版本冲突；固定版本 IPC 使用共享 `/private/tmp/cbm-daemon-<uid>`，更换缓存目录不能解决。未停止外部会话或绕过协调限制。启动器将该错误转换为仅含原因码的失败回执，桌面保留失败状态并提供中英处理说明；退出码成功但回执失败不再记录为成功。重建启动器实测 `outcome=failed / upstream_version_conflict`，未泄露原始诊断内容。crate 19 项、native 9 项通过（1 项环境测试单独已验证），严格 clippy 通过；完整索引仍待无版本冲突环境复验。

- **图谱组件与索引实测**：本机默认组件哈希不匹配，保持原文件；下载当前清单固定的 v0.9.1-rc.1 到临时验证目录，压缩包及二进制哈希均匹配。真实 App 保存目录后显示已验证；真实组件校验缓存测试通过（295578848 字节，冷校验约 1495ms，热缓存约 24µs）。独立双文件示例项目索引返回 `process_failed`，完整索引尚未通过。实测同时发现原生任务结束后页面仍显示处理中，已补充仅在索引 busy 时的顺序状态刷新及结束通知；该改动待重建 App 复验，索引失败根因待继续定位。未更改编码工具配置。

- **Doctor 到期状态更新**：报告到期时主动更新为过期并禁用修复，窗口重新获得焦点时再次检查；不再依赖其他交互触发渲染。11 项 Doctor 回归通过，包含无额外用户动作的 TTL 到期检查；该变更尚未重新打包实测。

- **Doctor 摘要准确性与本地化**：没有自动修复项但仍有手动/阻止项时，不再显示绿色通过图标；只有完整、健康且没有待处理项的报告显示通过。已知原生服务、工具辅助功能、视觉压缩、启动与备份摘要映射为中英用户说明，避免中文页直接出现英文启动回执。

- **日志导出真实桌面验收**：使用独立测试状态目录启动无模型连接的本地服务，macOS Preview 的 JSON/CSV 按钮均在下载目录生成文件（6296/315 字节），JSON 可解析且 `redacted=true`，CSV 可解析。测试后通过 App 停止服务并确认 18001 不再监听；未配置编码工具、未发模型请求。日志筛选与空状态改用中英用户语言，日志/文案 5 项回归通过；这次文案变更已通过规范 Tauri 构建打入 Preview。

- **Doctor 后端修复条件**：准备与执行共用的报告检查补充本地来源、总体状态、逐项状态及覆盖数量一致性校验，不能只凭 TTL/revision 放行不完整报告。新增回归覆盖部分检查、未知状态、来源不符及覆盖数量矛盾；7 项原生 Doctor 回归和 desktop 严格 clippy 通过。加入该条件后重新运行 Rust 全工作区、全工作区严格 clippy、格式检查与规范 Tauri Preview 构建均通过；Chromium/WebKit 36 项检查通过。日志导出的真实桌面文件落盘已单独验收，见本节日志导出条目。

- **Doctor 处理入口**：诊断不完整时，专项工具的设置导航、备份查看和只读重扫保持可用；准备配置改动的操作仍要求完整有效检查。专项工具标题和说明改为中英用户任务，避免英文界面出现整段中文。回归覆盖不完整报告下打开连接设置和备份页，以及修复按钮仍禁用；全量前端 561 项通过。

- **Doctor 原生扫描兼容**：实际 App 复现原生已生成报告但前端误报扫描失败；原因是产品模块报告新增 `agent_cc_switch_sync` 后被前端格式校验拒绝。现接受该明确登记的补充项，仍拒绝未知模块；未完成检查保持修复锁定。重建 Preview 只读重扫确认显示 5/6 检查结果及完成通知。9 项 Doctor 回归通过；全量 560 项通过、1 项英文文案规则失败，调整文案后该规则 2 项复检通过。修复入口的完整实际执行验收仍待继续。

- **总体概览语言与单日趋势**：将连接/同步/额度说明中的工程术语改为当前情况和检查入口，仍保留过期、未确认与缺少价格时不估算的边界。单日趋势直接显示节省数值，日期居中，数据点保留键盘与悬停说明。概览相关 42 项及全量前端 560 项测试通过。

- **底部状态栏**：常驻区域显示服务、连接、诊断与处理入口，端口和数据状态移入可展开详情；服务停止明确提示启动后检查。旧诊断为正常但当前数据已过期、部分可用或不可用时，不再亮绿色诊断状态。14 项状态栏回归、类型检查及 36 项 Chromium/WebKit 检查通过，并在重建的 macOS Preview 中确认停止状态及详情展开。

- **安装与连接**：已重排为局部三列工具网格（按内容宽度降为两列/一列），工具名称、选择与状态集中在卡片内，扫描与批量配置共用一条工具栏；服务状态收为简短一行。详情改为弹出层，辅助集成单独折叠，路径/变更证据按需查看。未操作工具不显示「配置失败」。只读预检为每个工具提供准备情况；取消选择后仍保留其真实状态。
- **设置与可读性**：主题/图表根变量直接由设置色块数组投影，主按钮、选中标记和图表不再使用各自覆盖的调色板；按钮保留白字，主要操作文字加重，小字 12–13px、正文 14px。开屏路径绘制和图标弹入已恢复；减少动态效果仍可跳过开屏并关闭相关动效。
- **查看与配置的授权区分**：引导的查看状态/进入验证入口只打开安装页；只有明确的一键接入按钮设置自动配置标记。普通页面跳转清除遗留标记。新目录下实际点击查看入口，没有配置事务、active config 或受保护文件变更；此前复现导致的 Codex 三文件已精确恢复。
- **逐项结果准确性**：准备失败与预检失败也计入批次未完成数；部分工具成功时明确给出已完成/需处理数量并保留 warning。成功工具的原生英文回执不再直接占据中文列表主文案。
- **恢复工具配置**：已验证备份中、目标路径仍属于工具登记范围的配置使用 `managed-agent-config` 恢复类型，复用精确恢复与反向备份机制；当前文件变化、损坏备份、未登记路径与符号链接仍拒绝恢复。10 项原生回退回归通过。回退页的标题、时间和不能恢复的原因已本地化；文件路径/记录号按需展开，并提供返回当前配置的入口。成功说明只声称实际完成的文件检查，失败与未知历史不再被描述成完成。
- **离线恢复记录**：`get_config_change_records` 与 `useConfigChangeRecords` 独立读取本机恢复记录，避免 runtime 统计过期导致新鲜备份被锁定；读取失败保留旧记录但锁定写操作，加载/读取失败不显示为“没有备份”。服务停止时仍可预览恢复，实际应用仍重新检查 revision/hash。原生离线读取回归通过。
- **Codex 当前连接发现**：未选中的内联 profiles 与 companion 配置不再遮蔽根 config.toml 的当前设置；显式 legacy profile 选择仍拒绝猜测。针对本机实际形态的 12 项原生测试和严格 Clippy 已通过，重新构建 App 已实际显示可沿用当前连接。
- **通知与结果**：本次错误、警告与待处理通知不会定时消失；成功/普通信息 5 秒后收起，悬停/键盘聚焦暂停计时。详情打开对应历史记录；跨引导/工作台切换保留本次通知；历史通知不会在重启后自动重放。重复结果使用实际记录 ID，清除旧的详情/跳转/回执字段，失败保留重试入口。
- **真实操作反馈**：安装、Doctor、变更回退、设置、图谱、视觉压缩的主操作已补充进行中、成功、部分完成与失败状态。回退/停止/删除失败保持对话框可重试；刷新不再把部分读取失败报告为完整成功。
- **首次服务启动**：真实 active config 不存在时，仅创建独立、私有、无 Provider/Route 的设置用配置并启动服务；现有非法配置、符号链接、权限错误仍拒绝。设置服务可用于完成连接准备，但模型请求在无路由时返回 `503 dtoken_route_not_configured`，不宣称已经连接模型。
- **用户任务语言**：安装工作台为连接工具、复用现有配置、高级设置、操作记录；底部故障状态有安装/诊断入口；视觉压缩服务与接入卡用任务状态替代 raw enum，运行标识、端口和路径收至详情；图谱可通过系统目录选择器回填已解压目录，再调用原有来源与完整性校验。
- **核心输入边界**：保留工作树既有 Responses C1 历史 tool output 压缩；本轮为压缩请求体增加有界解压，超限返回 413、损坏返回 400，不进入路由或普通事件账本。C1 的本地回归不能替代真实 Provider 质量/延迟验收；历史「Responses 始终 Off」结论不适用于该工作树实现。
- **已观察的桌面行为**：macOS ad-hoc debug `.app` 的独立 Preview 标识避免与已安装旧 App 操作通道混淆。测试状态目录下启动/停止服务、未就绪工具提示、通知详情、原生目录选择器打开与取消均已实际验证。更紧凑的安装页已在同一窗口观察，最终事务 scope/失败隔离与全量验证仍在收尾。

**尚不能宣称可发布**：当前是包含既有修改的未提交工作树；最终同候选全量结果、真实 Agent 请求链路、Developer ID/公证和 Windows 实包证据须分别完成。当前 debug/ad-hoc macOS 包及浏览器测试不代替这些门禁。

## 1.1 历史结论（2026-08-14 复核）

当前候选（commit `066c22e` + 1b 计量闭环）已完成：

- **Agent 配置**：Onboarding 五段 + 真实门禁 + StepProgress；Install/Onboarding Agent 可取消 checkbox（P0.0.10.1）；一次点击授权（P0.0.10.3）；per-Agent 独立事务（失败仅回滚自身并继续，P0.0.10.5）；shared route 回滚时同步撤销指向已撤销 route 的 Agent 配置。
- **cc-switch**：app 级 DB 路径授权边界（同 DB 内 provider/endpoint/protocol 变化自动跟随）；永远只读；app_type 全量不丢失。
- **Codex**：顶层 `openai_base_url`/`model_catalog_json` 解析；真实链路 `Codex → d-token → ocxlocal → upstream` PASS（隔离 CODEX_HOME）。
- **Runtime Capability Registry（R2.E）**：seed/observed/learned 三层、CAS、LRU、未知模型不失败、持久化 JSON store；`compression_hint` 实际应用策略；真实请求后观测回写（store 落盘）；model_identity 现进 routed decision（learning loop 可达）。
- **凭据**：exact upstream identity 派生 keyring account（防 slug 碰撞）；keyring-only 无明文 fallback。
- **计量**：Dashboard/Analytics token 数字来自真实 runtime/ledger（本地估算明确标注 estimated）；Analytics 日志行补 provider actual/claimable 投影（仅证据存在时显示）。

**诚实未闭环（不能写 PASS）**：
- Native OpenAI Responses wire 路径压缩仍 `Off`（协议逐字节透传，Dashboard 显示 honest passthrough 而非压缩成功）——R2.D 的真实 Codex 链路是路由/ledger 证据，不是压缩证据。
- 八页真实 Tauri 验收（R3）尚未在当前候选重跑。
- 双平台同 commit release evidence（R5）未产生。
- `docs/PLAN.md` R4/R5 lane 尚未基于当前事实重新生成。

d-token 当前已形成可运行的 Rust-owned 本地 runtime、Tauri 2 桌面控制面、React 18/TypeScript UI 与薄 TypeScript SDK。当前工作树已补齐九类 production codec release evidence（id 列表由 compressor registry 编译期投影）、CCR lifecycle/reconciliation 与 Windows DACL 实现、Rust/HTTP/MCP/TS surface parity、23 项 Agent 检测/transport capability matrix、Claude/Codex/Cline 真实临时配置精确恢复、cc-switch A→B→A 只读静态识别以及 macOS managed RTK lifecycle probe；G4 Responses bridge 现已对非法 tool choice/消息角色/函数字段 fail-closed，并将流式 Ledger/CCR finalize 延迟到完整终态。Install 的 MCP/Hook 分支已接入 Rust-owned preview/apply/verify 事务：目标路径必须与 Agent 注册表精确匹配，preview 只返回脱敏摘要，apply 绑定双 hash、备份、receipt、复读验证与失败自动回退，且明确不提供模型上游 route evidence。G7 当前候选真实 OpenCode 已于 2026-08-04 通过（candidate_id=G7，报告 `support/artifacts/opencode-real-routed/real-routed-20260804T112429Z-76558.json`）。G7 macOS package audit 已通过（c124d9f，2026-08-04：DMG d-token_0.2.0-beta.1_aarch64.dmg，SHA-256 1fe3f68e05ffe8903d62551c146320c6c76556f655c2ea4bd85b546f82165216，app 12 文件 46MB，ad-hoc 签名，临时安装/启动/退出/卸载全链路验证通过，无 immediate crash，Window 可见）。2026-08-07 本轮新增：Codex 自动发现支持 `ocxlocal` openai-responses（`ProviderWireApi::OpenAiResponses` 将 `/v1/responses` 原样透传上游、不做 Chat 转换；Codex 排除显式 model 要求，`models=[]` 允许请求 model 动态透传；core native Responses 非流返回上游 body 原样、流 SSE body 原样且仅终态 finalize；Chat-wire 仍桥接并 fail-closed）；Overview/Analytics 从 Settings `model-pricing` 价格表（input/output 每百万 token 单价）估算 input cost before/after/delta，unknown/mixed/过期 fail-closed，`provider_cost_usd` 作为上游账单费用单独呈现；tray 面板 placement 统一在单一物理坐标空间（`monitor_from_point`/`work_area`/`scale_factor`，覆盖 mixed-DPI 与负原点多显示器），placement 单测 8/8。最新全仓复证已通过（`cargo test --workspace` 936 pass/5 ignored、workspace clippy strict pass、前端 Vitest 634 pass、`tsc --noEmit`/build pass、Chromium+WebKit E2E 各 13 pass），macOS rebuilt app+DMG 审计 ad-hoc 通过（SHA-256 60f7d063…，未签名/未公证），Windows 仅 source/CI 合同、无当前实机。它仍不是可发布候选：主工作区本轮 Rust/前端/structure 全量复证受 macOS File Provider `dataless` 文件阻塞，同一候选已于 2026-08-05 在普通副本完成 Cargo 与前端全量复证并全部通过（见 `docs/PLAN.md` 第 13 条）；G6 仅 macOS 本机可执行门通过（PowerShell .ps1 门 fail-closed 未执行），Windows MSVC runner、同 commit 双平台安装包以及 G8 尚未完成；unsigned/unnotarized 与新 ocxlocal 路径的真实 Codex routed acceptance 仍缺失。Headroom/G0-G8 未完成、真实 provider/raw-wire 未验证，公开未签名 prerelease 只有在这些硬门全绿后才允许创建。

### 1.2 历史发布门禁 G0-G8 状态（2026-08-05 门禁补齐轮）

- 门禁聚合：新增 `support/scripts/generate_release_gate_status.py`（跨平台、fail-closed）。任一 gate 非 `passed` 即默认 exit 1；`--report-only` 仅供诊断且仍报告 `release_ready=false`。契约测试 13 项位于 `support/tests/test_release_gate_status_aggregation.py`，并已接入 `run_release_gate_contract_test.ps1` 动态运行。
- G6 Ops：本机（macOS，无 pwsh）仅 Python 可执行门通过；PowerShell 门（verification/gate_contract/provenance/performance/codec/capability/redaction/cleanup/distribution/handoff/signing）未执行 → 聚合为 `unavailable`/`blocked`。`release-verification-*.json` 缺席即 G6 blocked；“脚本可解析 / metadata-only / unsigned 存在”均不构成通过。
- G8 Candidate：双平台同 commit 证据未产生 → G8 `blocked`。fan-in 契约（`run_fan_in_artifact_contract.ps1`）本轮修复两处：`critical_passed` 改为从 critical 列表计算（不再用全量总数，防止误报近全绿）；`same_commit_across_platforms` 拒绝空 `git_head`（两个空值不再视为同 commit）。source digest 由 publish job 独立 checkout 的 `git diff --binary HEAD` 锚定；`fan-in-artifact-contract-*.json` 缺席或 ok=false 即 G8 blocked。
- G3：`run_windows_dacl_acl_tests.ps1`（零测试视为失败、非 Windows exit 2）已接入真实 Windows release runner（windows-release.yml，windows-2022）；ci.yml 的 ubuntu 调用只能 exit 2 跳过，不能作为 Windows MSVC 实测证据。
- 结论：G6 PowerShell 与 G8 Windows runner/双平台同 commit 证据仍缺失，当前候选不可发布。

## 2. 代码所有权

| 范围 | 当前定位 |
|---|---|
| `code/apps/desktop/` | 唯一正式桌面 App；Tauri 2 + React 18 + TypeScript + Vite |
| `code/crates/` | 18 个 Rust 业务 crate，负责类型、配置、路由、压缩、学习、进程、Doctor、事件、策略、ContextStore、runtime pipeline、Agent bridge、Codebase Memory、API 与 proxy |
| `code/apps/desktop/src-tauri/` | 桌面 native command、凭据存储、配置草稿/应用、服务生命周期、安装事务、Doctor 与导出 |
| `code/runtimes/headroom/` | Headroom-derived 能力迁移与 provenance 边界，不是 Python sidecar 或外部服务依赖 |
| `code/UI design for project/` | 设计原型/历史 UI 输入，不是生产代码入口 |
| `code/apps/desktop/src-old/` | 旧前端实现，不得作为新功能落点 |
| `code/packages/sdk/` | 薄 TypeScript SDK；与 Rust/HTTP/MCP 共用 compression decision parity contract |

## 3. 当前 Rust workspace

`code/Cargo.toml` 当前注册 18 个业务 crate 与 1 个 Tauri app crate：

```text
dtoken-types
dtoken-config
dtoken-router
dtoken-compress
dtoken-context-store
dtoken-process
dtoken-doctor
dtoken-event-ledger
dtoken-preprocessor
dtoken-headroom-adapter
dtoken-project-policy
dtoken-runtime-pipeline
dtoken-config-patcher
dtoken-agent-bridge
dtoken-codebase-memory
dtoken-learning
dtoken-core-api
dtoken-proxy
d-token-desktop
```

当前生产主链路：

```text
AI coding agent
→ 127.0.0.1 d-token listener
→ dtoken-core-api / dtoken-proxy
→ compression + route selection
→ configured upstream
→ redacted runtime evidence / Event Ledger / Dashboard / Doctor / App
```

## 4. 已存在的本地 API

`dtoken-core-api` 当前提供以下主要能力：

- OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 入口。
- `/health`、`/dashboard/payload`、`/stats`、`/stats-history`、`/metrics`。
- Provider、Route、Instance、active config、chain status 与近期 route decision。
- Doctor、recent logs、JSON/CSV export、Event Ledger。
- CCR、memory/learn、output shaping、项目策略与兼容性信息。
- Headroom-derived 状态面，以及 d-token-owned managed RTK command filter/lifecycle evidence。
- Optical shadow preview、status、capabilities、lifecycle 与 recent events。

这些端点的存在只证明实现面存在，不自动等于已达到稳定 Beta。真实 release claim 仍需同一候选 commit 的构建、运行、路由、持久化和平台证据。

### Headroom 全能力迁移事实

当前固定对照基线为 `headroomlabs-ai/headroom` `v0.36.5` / `34a551756222b24ea4bc064f4250b8e7d169daf1`（2026-08-22 发布；2026-08-24 delta 扫描统一，见 `docs/research/HEADROOM_DELTA_LEDGER_2026-08-24.md`）；上一扫描记录 `0cb72f45b23bdf7129822b16dd1d4cb7d6e0b062`（2026-08-04）与历史 `1725cd1f`、`v0.33.0` 仅用于解释旧证据。当前事实如下：

- proxy、显式 route、Ledger、stats/history/metrics、Dashboard/export 已有真实 d-token-owned 基础，但 Provider/Agent 矩阵未完整验收。v0.33 delta 已开始吸收：九类 production codec 现由 Rust-owned declarative compressor registry 统一登记和门控，release evidence 的 codec id 列表由 registry 编译期投影（`production_registry_codec_ids()`），计数无法与登记表漂移；`POST /v1/compress` 使用显式 route/provider/model/protocol 生成 validated candidate body，不发送上游、不持久化请求，并与 MCP 和 TS SDK 共用合同。d-token 不加载 Python entry point 或任意第三方进程内插件，第三方中转只能通过显式 RouteProfile 接入。
- resolved production path 已接入 `FrozenPrefix → TransformableWindow → ProtectedTail`、route/provider/model-bound cache key、ContentRouter、manifest 与 post-validation。JSON、log、search、build、diff、text、HTML、config、code-compressor 九类 codec 在本地 release probe 中均为 active；每类至少 128 个 property/semantic/rollback cases，negative、budget/cancellation 门零失败。`config-codec` 已进入生产 evidence（registry 第 8 项），且混合/多类型内容已接入 IR-safe section router：`route_mixed_content` 逐 section 走同一 codec/secret/marker/负收益门并字节精确还原（统一 CRLF 保留、混合行尾 fail-closed），`compress_resolved` 与 `/v1/compress` 公共表面会把 closed historical tool result 中的 config 与 mixed 内容按 section 路由；manifest 以 `Mixed` codec 声明并用同一 section router 重推导校验。tool-schema PR-E1/PR-E2（工具表按名排序 + schema key 规范化）已接入 `compress_resolved` 与公共 surface：任一工具携带客户 `cache_control` 时跳过 E1（位置敏感），PR-E3 自动放置 `cache_control` 仍由 proxy 层在 fresh/exact provider cache evidence 门控下执行，engine 不擅自改变客户缓存意图。code-compressor corpus 覆盖 Rust/TS/JS/Python/Go/Java/C/C++。未知、secret、风险和负收益输入继续精确 passthrough。该 evidence 尚未绑定最终 clean candidate 或真实 Provider 质量/延迟，因此 G2 不能据此单独宣布完成。普通输入图片文件压缩明确不在产品范围。
- CCR 已实现 AES-256-GCM、TTL/quota、非破坏性 inspect、精确 retrieve/delete、`write_intent → pending_retained → terminal` 与 `delete_intent → deleted` journal、重启保守 reconciliation、JSONL 到 SQLite projection rebuild、幂等 tombstone 与 lifecycle lock。Unix 目录/DB/WAL/SHM/CCR 文件分别收紧为 `0700`/`0600`。Windows 实现 current user + SYSTEM、protected DACL、禁继承、`SetNamedSecurityInfoW` 与读回验证，并接入 DB/WAL/SHM 和 CCR root/temp/final；Windows GNU cross-check 通过，仍须 MSVC runner 实测。跨 JSONL/密文/Event/SQLite 原子 transaction 和自动 tool-call retrieve 仍是 G3 缺口。
- memory 已接入 typed `ContextStore` 写入、重启持久化、项目/Agent/session 精确 scope isolation、share grant/revoke 与敏感原始字段拒绝；v4 的 project-wide grant 迁移后保留审计但强制撤销。managed MCP capability 可注册 `headroom_memory_list/export/delete/write`，并将不透明授权、project/agent/session scope 绑定到 launcher，工具参数不能自报 scope；公开 HTTP summary 因缺少调用方授权 scope 而 fail-closed。route-aware output shaping、跨 Agent capability transaction 与本机授权审计仍未完成。
- `/headroom/mcp` 已提供有界 stdio JSON-RPC transport：`initialize`、`ping`、`tools/list`、`headroom_compress`、`headroom_stats`、受控 `headroom_retrieve`、返回 deletion receipt 的 `headroom_delete` 与 SharedContext 工具，取消账本和 payload 限制均有测试。Rust library、HTTP、MCP 与 TS SDK 对同一 fixture 的 decision/reason/artifact parity 定向回归已通过；完整协议覆盖、caller capability 分发与最终候选复证仍缺。
- MCP `headroom_compress` 现要求受限 `route_ref`，并从 d-token active config 重新解析 route/provider/model/protocol；缺失、未知、endpoint/model/policy 不匹配均 fail-closed 且不回显输入或凭据。OpenAI Chat → OpenAI Chat、OpenAI Responses → Chat/Responses SSE 与 direct Anthropic Messages → Anthropic Messages 已接 resolved compression；Responses 输入字段与流式终态均有 fail-closed 回归，跨协议 MCP capability 分发与完整 Streamable HTTP session 仍未完成，故 MCP 仍不是全协议完成态。
- `dtoken-proxy --mcp-stdio` 已成为 d-token-owned MCP stdio launcher，可复用同一持久化 runtime/config 而不绑定 HTTP listener。Agent MCP transaction 仍默认使用 metadata-only `dtoken-agent-bridge`；将 raw CCR retrieve capability 分发给独立 Agent MCP entry 前，不能把该 capability 写进常规 Agent 配置或日志。
- Desktop 的 managed Agent MCP command 已注册并接入 Install：operation 仅允许 configure/clear；请求路径需为 absolute/home-relative，并与 `resolve_mcp_config_path` 注册目标完全一致；symlink、非普通文件、未知父目录和 stale review 全部 fail-closed。preview 不返回完整配置或 secret-like diff；apply 复用 `ConfigPatcher` 的备份、hash-bound transaction、verify、rollback 与 receipt；clear 无 owned entry 时不创建空文件、不重排 JSON，目标文件缺失时 verify 为成功。该状态是工作树代码事实，已随同一候选在普通副本通过本轮 Cargo/前端全量复证（2026-08-05，见 `docs/PLAN.md` 第 13 条），主工作区实体化复证仍未完成。
- Smart Learning 已新增 `dtoken-learning` 有界安全项目索引、真实项目分析、Agent 会话 metadata 发现和 managed block transaction；草稿生成具备立即反馈、自动定位与顶部通知，保存必须先显示 diff 并确认，重启后可从持久化分析继续操作。失败模式挖掘、跨 Agent memory 仍未完成。
- Overview 已新增 Codex/Claude 5h 与 7d 额度摘要合同。Codex 优先消费当前 ChatGPT OAuth 请求按账号节流获取的 `/backend-api/wham/usage` allowlist 数字字段，并兼容旧式 `x-codex-*` 响应头；`ChatGPT-Account-Id` 与 credential 只用于生成不可逆的 account/context 引用。Codex cache 绑定完整 credential+account context，Claude cache 绑定 OAuth credential hash；两者均有 provider 级 single-flight、账户级 attempt floor、15 分钟硬 TTL 与 8 项上限，active context 消失或切换时禁止跨账户回落。Claude 仅在检测到真实 `sk-ant-oat*` OAuth 上下文时读取官方 usage endpoint。两条链均为只读后台刷新，不保存原始账号 ID、Authorization、OAuth credential、完整 headers 或原始 usage response；单窗口证据标为 partial，无证据时保持 unavailable。
- Analytics 已扩展 Model、Protocol、Project、relay chain、失败分类、缓存/浪费/流水线 availability 与请求明细；新增 `/analytics/requests` cursor API、Tauri 只读桥接和前端分页。Agent/Model/Protocol/Route/Provider/Chain/Error 与 30 日 daily 聚合由 Rust read model 提供，不再由最近 20 条摘要重建；分页独立显示 source/freshness/revision，支持 loading/error/invalid-cursor 自动恢复。当前 runtime 最多恢复/查询最近 10,000 条；Project、真实 cache alignment 与完整 route/upstream/end-to-end timing 尚无生产字段时保持 unavailable。`RequestLog.latency_ms` 只作为 compression-only 证据，route/provider/cc-switch/end-to-end 与 route overhead 不再用比例或 0 值推算。以上不能称为全量 lifetime analytics。
- 性能工作树已新增独立增量 Dashboard summary read model、Learning 锁外评分与原子取消、Native summary/detail single-flight、summary slice revision，以及 WebView/Native/core 的有界 metadata-only timing。release-profile core summary 在 0/1k/10k/100k fixture 下 p95 为 34/151/144/153 µs；最终重建的同一 macOS release `.app` 已验证全部正式页首次加载、深浅主题、服务启动和 Overview 从 stale 自动恢复为 live，真实运行中的 summary HTTP 50 次顺序请求 p95 为 7.643 ms，core summary build 56 样本 p95 为 5,936 µs。遍历全部懒页面后的 30 秒可见窗口样本为 App CPU 0.23% / RSS 113.37 MiB、runtime CPU 0.00% / RSS 11.87 MiB。以上仍是 dirty working-tree、ad-hoc signed macOS 证据，不能替代 clean candidate、冷启动/完整进程组/soak、Developer ID/notarization 或 Windows 证据。
- 前端 production build 已加入 manifest 递归 bundle gate。2026-07-15 最新干净 Tauri 候选的初始静态 JS 为 160,769 B（157.00 KiB），低于 160 KiB 门限；`charts-vendor` 仍只由 Analytics 动态加载。该门只证明产物体积合同，不证明 WebKit parse/evaluate 或真实冷启动达标。
- Native loopback GET 已记录 metadata-only connect/configure/write/read/validate/parse/total 阶段及 health cache hit/miss，并复用响应分配剥离 header。当前仍是 blocking `TcpStream + Connection: close + read_to_string`，没有 response cap 或共享 typed client，因此只算测量与小幅复制优化，不算 HTTP 架构收敛。
- terminal evidence 仍保持四个 durable JSONL sync 边界，未为速度删除 durability；当前已能记录各阶段 duration/sync count、ContextStore lock/projection 与 total 分位数。canonical commit、fault injection、双写/重放和持久化收敛仍未完成。
- Rust 检测注册表覆盖 23 个 Agent；前端不再维护第二份 Agent catalog。Claude Code、Codex、Cline CLI、Goose、Kilo Code、OpenCode、OpenClaw、Gemini CLI、Roo Code、Continue、Aider 有版本化 Configure/Clear adapter（2026-08-03 起）；CommandCode、Claude Desktop、Cursor、Hermes、OpenHands、Copilot CLI、VS Code Copilot Chat、Pi、Cortex Code、Windsurf、Antigravity、Mistral Vibe 有显式 detection-only/fail-closed decision。VS Code Copilot Chat 只有发现真实扩展目录才视为已安装，共享 VS Code `settings.json` 不能单独构成证据；其 `overrideProxyUrl` 属网络代理调试面，而当前 d-token runtime 只提供模型协议 HTTP 端点、不提供 HTTP CONNECT/forward-proxy，因此不得写入并宣称 routed compression。Cortex `connections.toml` 只做 size/mtime/state metadata fingerprint，不读取文件内容。Cursor 保持 fail-closed 的原因是官方确认所有 BYO-key 请求都经 Cursor 服务器转发，没有可验证的本地 upstream override；Gemini CLI 适配器写 `~/.gemini/settings.json` 的 `security.auth.selectedType=gemini-api-key` 与 `~/.gemini/.env` 的 `GOOGLE_GEMINI_BASE_URL/GEMINI_MODEL/GEMINI_API_KEY`（值只写 `dtoken-loopback` 标记，绝不写凭据）；Roo Code 写 `~/.roo/settings.json` 的 `openAiBaseUrl/openAiModelId/openAiApiKey/currentApiConfigName/listApiConfigMeta`；Continue 只写 `~/.continue/config.json`（存在权威 `config.yaml` 时 fail-closed）；Aider 写 `~/.aider.conf.yml` 的 `model/openai-api-base/openai-api-key`。未知 JSON 不再递归猜 upstream。支持 adapter 的 parser 只读取 provider/model/protocol/脱敏 endpoint 与 credential-present；Goose custom provider sidecar 参与 watcher/fingerprint。新 adapter 均带独立 ownership manifest、双 hash、revision-bound preview/apply、verify 与 exact clear；`cc_switch_fixture.rs` 与 in-crate 矩阵测试同步锁定该 11 项 Configure/Clear 矩阵。
- 2026-07-31 已在本机真实 Claude Code、Codex、Cline 配置上完成 preview → apply → reread verify → clear → exact restore，不启动 Agent、不发模型请求、不返回 credential value；最终 SHA-256 分别为 `341e468283ce253d7a31699dc6b514bb556a1da8b8ca71e9db6786f404460ed3`、`8af5fd43ea971d118cc533dc0c09eed23d34f7036c012c6b4777769bc9064c4b`、`9adf48a01f16427ed4c40fca78fcf8389fd06e387e263383459e4265c4d67a05`，权限均为 `0600`。2026-08-03 工作树新增 Gemini CLI/Roo Code/Continue/Aider 的 fixture 级 lifecycle 事务（configure → verify → clear → exact restore）并更新发布矩阵；真实本机候选 probe 与 Windows runner 证据仍未执行。OpenCode 已于 2026-08-04 在当前候选完成真实 routed compression（G7 通过，报告 `support/artifacts/opencode-real-routed/real-routed-20260804T112429Z-76558.json`）。
- cc-switch 保持只读：`CcSwitchSnapshot` 仅提供 schema/source 状态、app type、active upstream、脱敏 endpoint、protocol/model、credential-present、fingerprint/revision。生产实现位于 `code/crates/dtoken-config-patcher/src/cc_switch.rs`（`read_cc_switch_snapshot` 以 SQLite read-only 打开，仅读取 metadata 列，不读 `settings_config` payload、`private_metadata` 或凭据值；未知 schema 版本 fail-closed 为 `blocked`，多 active 为 `needs_selection`，缺必需字段不静默默认）。它监听已定位的 DB/WAL/SHM 与父目录，使用 750ms debounce、single-flight 处理和 polling fallback；该 watcher 是 byte-level polling（只哈希原始文件字节与目录列表，从不打开或解析 SQLite，因此也不可能读到凭据），模式始终显式报告 `polling_fallback`，不存在静默的 inotify/FSEvents 路径。pending 变更若回滚至稳定快照会被清除，之后再次出现必须完成新的完整 debounce。用户在一次 hash-bound 确认中授权后，高置信单一 upstream 的变化可自动生成并执行 d-token 自有 route sync/reload receipt；手动首轮 sync 只有 `runtime_reloaded` 或安全的 `runtime_not_running` receipt 才会持久化 auto-follow 授权，否则保留已应用 route 但不授予后续自动跟随。多 active、未知 schema、缺 model/新凭据一律 `needs-selection`/blocked，不写 cc-switch。
- d-token-owned managed RTK 已在 macOS candidate probe 通过 install/verify、ANSI/空行/重复行过滤、有界输出、成功与失败 exit propagation、usage metadata、exact uninstall 和 private root cleanup；receipt 明确 `command_argv_persisted=false`、`raw_output_returned=false`、`credential_value_read=false`。最新证据为 `support/artifacts/managed-rtk-candidate/candidate-20260731T223754Z-7536.json`。Windows/WSL、hook integrity 与最终候选 workflow 证据仍未完成，因此 integrated RTK 保持 partial。
- Doctor v3 已将 `agent_cc_switch_sync` 作为实际 domain module：同步、可修复 drift、需要来源修复和缺失证据分别由同一只读 discovery 产生。Doctor 尚未直接执行同步写入；用户仍从 Install 的 diff/确认流应用 d-token-owned route。
- 当前 gate 结论：G2 本地 codec release probe、G3 lifecycle/ACL 实现、G4 surface parity（含 Responses fail-closed/stream terminal regression）、G5 部分 Agent/cc-switch/macOS RTK 证据已推进；它们尚未绑定最终 clean candidate。G6 仅 macOS 本机可执行门通过（Python 契约 5 组、structure/scope/version、fixture/fake-bin no-touch smoke、SBOM/license/security 生成 exit 0），PowerShell 门因本机无 PowerShell fail-closed 未执行；G7 当前候选真实 OpenCode 已于 2026-08-04 通过；G8、Windows runner 和双平台产物仍阻断发布。Optical code/context → PNG 保持默认关闭与独立 fidelity/cost/latency gate。

逐能力缺口、顺序与发布 Gate 见 `docs/PLAN.md` §1C。任何 metadata-only endpoint、静态布尔值、fixture smoke 或外部 8787 对照都不算迁移完成。

## 5. 当前 Tauri 能力

生产版 invoke 当前包含：

- App 设置读写。
- Provider credential 的操作系统凭据库写入/删除，配置中只保存引用。
- Route/provider 草稿保存、应用和 active config 状态读取。
- d-token active config、OpenCode 和 Smart Learning 单文件 ConfigPatcher rollback point；回退预览区分完整性与当前可恢复性，绑定当前 SHA-256/`missing` 状态，恢复前创建可再次使用的反向安全备份并返回文件/runtime 复检状态。Agent Integration 多文件 manifest 已聚合为事务级回退点，Optical/Agent 根据 `executor_kind` 调用来源 executor。
- 工具检测、安装计划预览、d-token 服务状态、启动和停止。
- 安装事务、Doctor v3 统一快照/模块注册表/动态 freshness、自动扫描并修复、专项模块、Agent 自动纳入整批逐项事务与失败逆序回退、预览/确认/自动复检、产品 readiness 聚合、Dashboard 读取和脱敏导出。
- Agent route 配置验证与项目压缩策略推荐。
- OpenCode 配置事务：preview、脱敏 diff、双 SHA-256、backup、明确确认、apply、verify 和 exact rollback。
- Smart Learning：多 Agent 会话 metadata 项目发现、有界安全结构索引、项目分析、敏感路径排除、持久化结果恢复，以及带即时反馈、diff/confirm、managed block transaction 与 exact rollback 的文档动作。
- Codebase Memory（实验性）：固定上游 headless artifact、`dtoken-codebase-memory-launcher`、项目级私有 cache、MCP transaction、Claude Hook、d-token-owned index/UI host、五维状态，以及只读 graph open/start/stop/delete。上游原生 UI、运行时下载、repo persistence 与 Agent-owned 进程停止均保持禁止。
- Codebase Memory 真实验收：OpenCode 1.14.51 使用官方免费模型经项目事务连接 managed canonical MCP entry；metadata-only receipt 为 initialize=1、tools/list=1、allowed tools/call=3、denied=0，随后项目配置精确恢复，批准仓库未生成 `.codebase-memory/` 或 `artifact.json`。
- 统一 `OperationDialog` 与本地持久化 `NotificationCenter`；success/info/warning/error/action-required 均可即时占用顶栏 banner。普通成功/信息 5 秒后收起，error/action-required 保持可见直到用户处理；普通通知收起后仍保留在通知中心。安装与连接、Doctor、变更回退、视觉压缩、设置、Smart Learning、日志/安全等正式动作均使用页内中间态与顶部结果通知，失败详情经过统一脱敏并保持简短。

OpenCode workload、模型测试、Headroom capability smoke 等开发命令只在 debug build 注册；release build 不注册这些测试入口。

当前限制：

1. `install_tool` 对缺失的 RTK/Headroom 不执行真实下载，只做检测并安全失败。
2. OpenCode 已完成真实配置事务和恢复，但该授权与证据不扩展到 Claude Code、Codex、Cursor、Cline 或 cc-switch。
3. 平台自动启动控制面已接通 macOS LaunchAgent 与 Windows HKCU Run，具备快照、原子写入/注册表写入、写后校验与失败回滚；真实登录会话和覆盖升级后的自动启动证据仍待补齐。
4. 凭据库代码路径存在，但真实认证 Provider 的 macOS Keychain/Windows Credential Manager 候选版证据仍需补齐。
5. 通用回退页已直接执行普通 ConfigPatcher、Agent Integration 与 Optical 三类 executor；Agent 多文件事务聚合、反向安全备份和专项复检已有回归覆盖。Route/安装等尚无专项 executor 的 scope 仍必须锁定，双平台真实窗口与 runtime 离线回执证据待补。
6. Doctor v3 当前真正可写的专项 executor 是身份验证后的 d-token 服务生命周期与已有管理证据的 Claude Code/Codex/OpenCode Agent Integration；Route reload、listener TTL 清理、Agent transport、Optical Shadow、autostart 等模块已注册并显示能力状态，但未逐项完成 executor/双平台候选验收时保持锁定或返回来源工作台。

## 6. 当前桌面信息架构

正式一级导航与 PRD 一致：

1. 总体概览。
2. 安装与连接工作台。
3. 代码图谱（实验性）。
4. 分析。
5. 视觉压缩（Beta）。
6. Doctor。
7. 变更与回退。
8. 设置。

「视觉压缩（Beta）」页对齐安装工作台信息架构：一键启用（安全 Shadow 默认）+ 高级配置（全局/route 策略）+ 状态与预览。通过 Tauri `get_optical_observability` / `preview_optical_plan` 读取 d-token-owned Optical metadata-only API，并通过 `get_optical_config_editor` / `preview_optical_config_change` / `apply_optical_config_change` 以 ConfigPatcher 事务编辑 active config 的 optical 段（强制 `allow_active=false`，应用后重启 managed service）。页面 lazy 加载、仅打开时拉取，不进全局 dashboard 热路径。Windows/macOS 共享同一 React 页；估算节省不得标为账单。代码图谱作为独立实验页展示 package/integration/agent session/UI session/index 五维状态，使用受限 launcher、项目级私有 cache、配置事务和 d-token-owned 只读 loopback graph host；上游原生 UI、跨项目查询、运行时下载和 Agent-owned 进程停止均被禁止。Advanced、Logs/Security、Compatibility、Smart Learning 等仍作为安装工作台子页面或内部组件，不应再扩张为工程化顶级导航。Smart Learning 已接入“安装与连接”二级页，并使用真实 native scan/preview/apply/rollback action result。

同一页的一键区域已增加独立 Agent 接入工作台：展示 service PID/port、transport route 数、bridge 可用性、Claude Code/Codex/OpenCode 检测结果、全局/项目作用域和规则/Hook/Plugin/Skill/MCP 开关；应用前逐文件展示脱敏 diff，应用后提供校验、Codex Hook trust/new-session 提示与整笔回退。Agent 接入不会修改 Provider 凭据，也不会自动打开或关闭视觉 Shadow/Active。

正式桌面页面只接受 Tauri command 与 d-token-owned local service 的 source/freshness/action-result 证据；缺失数据保持 unavailable/partial/unknown。非 Tauri 环境仅用于开发预览并 fail-closed，不向正式页面注入示例数字。

当前 UI 基础层为 AiMaMi 信息层级取向的纯色 shell/surface：侧栏和顶部工作区连成一个整体，正式页面不使用玻璃卡片堆叠或动态背景；概览默认展示全年 heatmap，设置使用 grouped rows。侧栏一级图标、主题与折叠使用各自独立的 enter/leave 零件配方与点击容器 scale（Reduce Motion 关闭 transform）；首次无偏好时侧栏默认折叠。Windows/macOS 共用同一套 React UI，当前只完成 macOS 真实视觉验收。

## 7. 当前 macOS 真实候选证据

2026-07-13 已在精确最新 `.app` 上完成以下验证：

- 真实链路：`OpenCode -> d-token 127.0.0.1:18001 -> OpenCode Zen`。
- 关键 request `bc88fde0-3ce9-45ad-8e7f-b733096e9f0f`：压缩前 365,650、压缩后 331,945，物理减少 33,705；处理步骤为 `repeat_line_fold` 与 `semantic_closed_tool_result`。
- `tokens_saved/claimable=0` 是有意的 fail-closed 语义：没有上游归因证据时不把物理减少宣称为可计入节省。
- 重启后 runtime 仍保留 89 条 routed request、20 条可见 request record；物理减少汇总 279,270，约 4.0%，claimable 为 0。
- OpenCode 配置已恢复到 SHA-256 `47772c85307f02e1d910dc3cc93b85165a9fc37db5911b7d4201be766f85baab`；d-token active route 已恢复到 `37149d6c69561a4562a67d7d53bf03337e37e98104424c694aaa4a4038c45843`，当前 route 为 `Local verification`。
- 历史 Doctor/product-readiness 在真实窗口为 9/9；Headroom 缺失是正常可选状态，OpenCode 和 RTK 检测通过。该证据早于当前主动扫描/修复/复检工作台，不能替代新闭环的跨平台实机验收。
- Changes 旧页面曾显示 0 项待处理；当前工作树已移除顶级待批/Diff，默认展示备份与回滚，并从 desktop transaction ledger 投影持久变更记录。
- Smart Learning 在临时目录完成 managed block apply 与 exact rollback，没有改写用户项目。
- macOS 六页基线截图位于 `support/artifacts/macos-ui/final-candidate-2026-07-12T18-52-02-502Z/`；当前工作树新增 Overview、Settings autostart 与 920×640 实机证据位于 `support/artifacts/macos-ui/final-candidate-2026-07-13/`。本机 ad-hoc 哈希由候选专属 manifest 记录，不写死在源码状态文档中，也不等同于正式签名发布产物。

## 8. Optical / Hybrid Context Optimization

当前后端已实现默认关闭、证据门控的完整 Hybrid 执行链：

- 版本化 Capability Snapshot 与 revoked/superseded 生命周期。
- Anthropic/OpenAI Chat/OpenAI Responses 工具调用依赖图与 fail-closed 规则；Responses 只生成 shadow evidence。
- request-local TextArena、exact/decision/secret 风险分类、content/action/cache evidence、动态 geometry 与三点 economics 计划。
- raw-byte/canonical JSON fingerprint 计划证据。
- preview/status/capabilities/lifecycle/events API。
- route-resolved Protected Context IR 与仅批准 closed historical low-risk tool result 的 semantic/Optical codec。
- 确定性纯 Rust PNG renderer、动态末页、自解码、deadline/取消/并发/内存/像素/编码预算；active accounting 使用实际 artifact 页尺寸。
- Anthropic typed patcher、artifact-derived post-transform validator 和 pre-send 原文 fallback。
- Anthropic typed patcher 在每组 PNG 周围注入可验证的 `RenderedContextPointer`、可选 secret-first `ExactFactSheet` 与 `HistoryBoundaryGuard`；当前请求、近期六个闭合工具对及 open/orphan/malformed 状态仍保持原生文本。
- 同协议 Anthropic JSON/SSE 转发；shadow/passthrough 保留 ingress 原始 bytes，跨协议不授予 Optical active。
- Ed25519 route-bound Capability/eval attestation、signed multimodal accounting、typed Ledger action/cache/risk 聚合与 Doctor/status schema v2 evidence。

active 代码路径存在但生产默认仍关闭：必须同时满足全局/路由显式授权、同协议 Anthropic、精确签名 Capability、通过且绑定 renderer/font/corpus 的 eval、经济门、资源门、patch/post-validation 与 durable pre-send Ledger。当前没有真实 Provider fidelity/cost/latency 候选证据、OpenAI active patcher、history collapse 或 post-commit 自动 rollback；任何文档和 UI 都不得把本地估算或第三方数据宣称为已验证节省。

桌面 Beta UI 已接入 metadata-only 观测与 shadow preview；macOS/Windows 代码路径相同。Windows 实窗证据仍待补齐时不得把 macOS 单端截图写成双端完成。

## 9. 仍然阻断发布的事项

1. Headroom §8 仍未达到“完整迁移”发布声明：九类 codec 已 active，但最终候选真实 Provider/Agent 质量证据、G3 Windows runner/剩余 crash/tool-call、全协议 MCP/capability 分发、Agent/RTK 双平台矩阵与 G6 剩余 PowerShell 门、G8 尚未闭环（G7 当前候选真实 OpenCode 已于 2026-08-04 通过；G6 仅 macOS 本机可执行门通过，PowerShell 门 fail-closed 未执行，不据此宣称 G6 完成）。
2. GitHub public prerelease workflow 已改为 unsigned-only 聚合发布：同一 tag/同一 commit 的 macOS ARM64 与 Windows x64 reusable job 全部通过后，才创建 GitHub prerelease；它明确不宣称 Developer ID/notarization/Gatekeeper 或 Authenticode。
3. 生产级签名发布仍未完成：macOS 仍缺真实 Apple Developer ID/notarization/Gatekeeper 证据；Windows 仍缺具备签名凭据的 Authenticode 结果，以及 UTF-8/路径、Credential Manager、回退和视觉一致性证据。
   Linux CI、macOS release 与 Windows release 均会在 Rust workspace 门禁前从当前源码准备 host 对应的真实 `dtoken-proxy` externalBin，干净 checkout 不再依赖仓库内或开发机残留二进制。
4. macOS LaunchAgent 与 Windows HKCU Run 已接通，但尚未绑定同一候选完成两端真实登录、禁用和覆盖升级验证，不能宣称平台 autostart 已通过发布验收。
5. Claude Code/Codex/Cline 已完成真实临时配置事务并精确恢复，但按授权未发送模型请求；Gemini CLI/Roo Code/Continue/Aider 已完成 fixture 级事务与矩阵锁定，但真实本机候选 probe 尚未执行；OpenCode 已按当前候选完成真实 routed compression（G7 通过，2026-08-04）。Cursor 因官方路由架构（所有请求经 Cursor 服务器）保持 detection-only，其余未验证 schema Agent 必须继续显示 unavailable/detection-only，不能计入自动接入完成率。
6. 真实认证 Provider 的 Keychain/Credential Manager 引用、短生命周期出站读取和不泄露凭据的候选版实证仍缺失。
7. `code/UI design for project/` 与 `src-old/` 等旧实现需要在后续清理任务中决定保留、迁移或删除；它们不是生产入口。

## 10. 当前验证入口

```bash
python3 support/scripts/check_structure.py

cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

2026-07-13 历史候选门禁结果：

- `python3 support/scripts/check_structure.py`：通过。
- `npm run test:run`：125 项前端测试通过。
- `npx tsc --noEmit` 与 `npm run build`：通过。
- `npm run test:e2e`：20 项 Playwright E2E 通过。
- Rust route lifecycle、配置 patch/rollback 与 workspace gates 通过。
- `npm audit`：0 个漏洞。
- `npm run tauri:build`：macOS `.app` 与 DMG 生成成功；bundle `codesign --verify --deep --strict` 通过 ad-hoc 签名校验。该结果不等于 Developer ID/notarization。

2026-07-15 当前工作树最终本地门：Rust workspace `467 tests / 50 suites`、桌面 Vitest `35 files / 175 tests`、Chrome Playwright `10/10`、support Python `7/7` 全部通过；TypeScript、Vite production build、workspace fmt、严格 Clippy、structure、macOS sidecar dry-run 与 diff check 通过。WebKit 因缺少本机二进制未执行；Windows 仅有编译/脚本/workflow 合同证据。真实 Claude/Codex 用户配置未改写，本轮未执行新的真实 OpenCode routed flow，Provider fidelity/cost/latency 与签名 Capability 仍缺失。

2026-07-18 Codebase Memory 本地交付证据：Rust workspace `555 passed / 2 ignored`、桌面 Vitest `234/234`、TypeScript、Vite 与首屏预算、Chromium/WebKit Playwright `20/20`、structure、fmt、严格 Clippy 和 diff check 通过。macOS ARM/x64 与 Windows ARM64/x64 四个官方 archive 的 SHA-256、提取 binary SHA-256 与 Cosign blob bundle（固定 workflow identity/OIDC issuer）均验证通过；Windows GNU target 的领域/配置 crate `cargo check` 通过并包含私有 backup ACL 路径。Tauri debug `.app`/DMG 完成实构建、ad-hoc deep signature 校验，bundle 内 launcher 对签名后 upstream 完成 initialize、tools/list 与真实 tool call。真实 OpenCode 1.14.51 使用官方免费模型完成 3 次允许 tool call、0 次拒绝，事务后项目配置与仓库零写入边界复检通过。Windows workflow 已新增安装后 launcher MCP、索引、只读图页、安全端点、浏览器分发与仓库零写入 smoke gate，但尚无远程 runner 通过报告，因此不写成 Windows 实机通过。

2026-07-18 晚间按产品反馈调整分发与交互：正式 Tauri `externalBin`、macOS/Windows sidecar 准备、签名、安装审计与产物 manifest 已移除大型 upstream，App 仅保留受限 launcher 与法律资源；页面增加固定官方 `v0.9.0` 下载入口、默认/自定义导入目录与每秒 metadata 变化自动识别，执行前仍以 pinned platform/arch/SHA-256 fail closed。页面当时自动扫描 OpenCode/Codex/Claude，并经历过“默认多选全部已安装受支持目标”“自动纳入且不可取消”两种历史交互；这两种口径都已被 2026-08-12 P0.0.10 取代。当前合同是：完整 Agent 清单可见，已安装受支持项默认勾选，用户可以取消，未安装项不可勾选；逐 Agent 独立事务，单项失败不得逆序回滚其他成功项。2026-08-03 起 Cursor、Windsurf 等未被支持 schema 的 Agent 保持不可自动接入，而 Gemini CLI/Roo Code/Continue/Aider 已具备 Configure/Clear 能力。Shell 横幅只显示本次运行最新事件，历史保留通知中心；所有页面切换均不再主动聚焦 `<h1>`。实窗复核还发现图谱 Tauri command 的 Rust 参数为 `request: Payload`，而前端曾错误发送双层 `request`，导致批量配置按钮报 `missing field request`；现已统一为单层 payload，并用 Rust contract test 锁定 preview/apply/verify/rollback/start/index/delete/directory 命令。状态构建、配置预览/应用、UI host 启动与索引启动已移入 `spawn_blocking`，避免这些文件校验和进程启动工作占用窗口线程。

同日晚间最终本地门：Rust workspace `558 passed / 2 ignored`、桌面 Vitest `40 files / 239 tests`、Chromium/WebKit Playwright `20/20`、support Python `11/11` 全部通过；当时候选的 TypeScript、Vite production build、workspace fmt、严格 Clippy、structure、diff check 与 `npm audit`（0 vulnerabilities）通过。最终重打包时工作树还并发加入了其他 UI 改动，首屏递归 JS 为 `136,747 B / 180,224 B`。重新生成的 macOS arm64 release 候选 `.app` 为 32 MB、DMG 为 14 MB，bundle 资源清单只有 `dtoken-codebase-memory-launcher` 与 Codebase Memory LICENSE/NOTICE，不含 261 MB upstream；`codesign --verify --deep --strict` 通过 ad-hoc 签名校验。用户导入 pinned upstream 后，真实 OpenCode 1.14.51 使用官方免费模型完成 `codebase-memory-mcp_list_projects` tool event（completed）；事务随后精确回退，`code/opencode.json` 不存在，批准仓库没有新增 `.codebase-memory/`、`artifact.json` 或 `.gitattributes`。这些是 macOS 本机、dirty working-tree、ad-hoc signed 证据；Windows workflow 虽有静态和合同门禁，仍无真实 Windows runner 报告。上述门禁完成后，23:24 并发新增的未跟踪 `src/app/lib/copyPolicy.test.ts` 因缺少 Node 类型造成当前全量 `tsc --noEmit` 的 6 个错误；该文件不属于本轮代码图谱改动，未在本轮擅自修正，因此当前 dirty working-tree 不能标为全门通过。

Hybrid 发布前加固已落地：统一 SecretClassifier v4、三协议 recent-six、共享 in-flight byte budget、stream 终态 Ledger、原子持久 Agent transaction 与跨进程锁、共享 secret diff redaction、MCP 有界读取、UI 事务证据丢失 fail-closed、签名后 bridge identity workflow，以及任何 bridge sidecar 执行前的平台签名验证。Capability registry 为空；production Active 代码存在但当前不可达。

2026-07-19 桌面发布文案治理：`src/app/lib/i18n.ts` 已作为正式双语词典，Codebase Memory、Optical、Doctor 的稳定检查项和操作结果使用一致的中英文表达。`userCopy.ts` 将 native 失败映射为可执行的本地化说明和安全状态代码；设置、日志、智能学习、更新、视觉压缩与 Doctor 不再向页面、通知或弹窗直接回显原始异常、native summary 或路径/凭据相关文本。AST 文案策略测试同时阻止审核口吻、测试/演示词、AI 腔和可见原生错误直传；内部协议字段与测试源码保留。最终门禁结果以本次候选的桌面 Vitest、TypeScript、Vite build 与 E2E 实际输出为准。

2026-07-19 Codebase Memory 批量配置性能修复：261 MB pinned upstream 的 Rust debug 单次 SHA 校验原约 41.00 秒，且旧链路可能为每个 Agent 重复校验并并发占用窗口线程。当前实现将 SHA debug 热点优化、同一文件 identity 的 30 秒 single-flight 校验缓存和 verified launch 合并，实测冷校验 866 ms、缓存命中 25 us；缓存绑定 canonical path、size、mtime 与平台文件 identity，校验期间或随后发生变化时 fail-closed。Tauri 的状态、preview/apply/verify/rollback、UI/index 生命周期和 loopback 操作均移入 blocking worker；前端批量配置逐 Agent 串行调度，并在首个 Native 调用返回前显示 Agent、阶段和 current/total 进度。Code Graph Vitest `9/9`、TypeScript、Vite build、Chromium/WebKit Playwright `22/22`、workspace fmt/严格 Clippy、structure、support Python `11/11` 与 diff check 通过；全量 Rust 仅出现一次无关 Optical deadline timing failure，目标连续复跑 `2/2` 通过。最新 macOS release `.app`/DMG 已经 `build_and_run.sh --verify` 重建并启动。真实 OpenCode 1.14.51 回执为 initialize 1 次、tools/list 1 次、允许 tool call 2 次、拒绝 0 次，配置与批准仓库零写入边界随后精确回退；Windows 远程 runner 证据仍缺失。

同日完成安全与恢复一致性复审：运行时不再接受“同一签名者但 hash 不同”的 upstream，只允许 manifest 固定的平台 binary SHA-256 或明确登记的 macOS universal 派生 SHA-256；自动发现不再把 `.ai-bridge` 或相邻旧 bundle 作为供应链候选。页面明确标注 `Codebase Memory MCP v0.9.0`。Agent 批量配置状态由单一最后事务升级为 bounded v2 transaction set，刷新/重启后仍能恢复全部 active transaction 的批量回退入口；verify/rollback 从对应 manifest 恢复 Agent/scope/surface，写配置后若控制面状态落盘失败则自动补偿回退或返回 recovery handle。修复后 Code Graph Vitest `11/11`、全量桌面 Vitest `247/247`、Rust workspace `561 passed / 3 ignored`、Chromium/WebKit Playwright `22/22`、TypeScript、目标严格 Clippy 与 macOS release `.app`/DMG 重建启动通过。最新真实 OpenCode 验收回执为 initialize=1、tools/list=1、allowed tool call=1、denied=0，事务后 `code/opencode.json`、`.codebase-memory/`、`artifact.json`、`.gitattributes` 均为 absent。Windows 远程 runner 仍是 CBM-6 唯一外部证据缺口。

2026-08-06 Codebase Memory 版本核对与 untrusted 文案修复：用户报告“选择正确 upstream 目录后仍无法 Agent 配置”的根因是 pinned v0.9.0 校验与上游版本漂移的组合——上游最新为 `v0.9.1-rc.1`（pre-release，commit `ee9833df2c72b750378273897ffec9410fc2c4f2`，2026-07-30），其 archive/binary SHA-256 与 pinned manifest 不符，导入后 package 进入 `untrusted`/`upstream_untrusted`，而 UI 之前只显示误导性的“请先导入并通过校验图谱组件。”。决策：明确阻断 pre-release、不升级 pinned（保持 immutable v0.9.0 + Sigstore attestation 模式），不改写任何 hash/signature/平台校验。修复：`integrationBlockedReason` 对 untrusted 状态返回真实原因“已检测到图谱组件，但未通过固定版本校验（需要 v0.9.0）”，并新增 `cbm.packageUntrusted`/`cbm.detail.upstream_untrusted`/`cbm.detail.upstream_not_imported`/`cbm.detail.launcher_unavailable` 中英文案；先补失败测试再修复（Code Graph Vitest 新增 1 例后 `12/12`、全量桌面 Vitest `44 files / 266 tests`）。验证：`cargo test -p dtoken-codebase-memory` `18 passed`、desktop native `codebase_memory` `8 passed / 1 ignored`、tsc 仅剩既有 `discovery.ts` 2 错（非本轮改动）、Vite build 通过；用官方 v0.9.0 darwin-arm64 release 目录实测 archive/binary SHA-256 与 pinned 一致（通过），官方 v0.9.1-rc.1 资产 hash 不符（拒绝），全程未修改任何用户 Agent 配置。provenance 记录见 `code/runtimes/codebase-memory/README.md` 与 `.ai-bridge/release-iteration/release-codebase-memory.md`。

2026-08-06 晚间（第二轮）Codebase Memory pinned 升级到官方最新 immutable pre-release `v0.9.1-rc.1`（published `2026-07-30T03:15:15Z`，commit `ee9833df2c72b750378273897ffec9410fc2c4f2`，用户确认实际使用 v0.9.0 并批准升级）：manifest/`ARTIFACT_SPECS`/universal 派生 hash/下载 URL/UI 文案全部更新为 v0.9.1-rc.1（UI 标注 pre-release）；四平台 archive/binary SHA-256 经 GitHub API digest == 官方 checksums.txt == 本机实测三方一致（darwin-arm64 实测，其余平台有可审计来源），macOS universal 用真实两个 macOS 二进制 `lipo -create` 实测 `c18762b9...`，新 CDHash 标识来自 `codesign -dvv`；Cosign workflow identity/OIDC issuer 不变，prepare 脚本 manifest tag 校验同步更新，未放松任何 hash/signature/平台校验，源码 clone 不作为可执行 release。同时独立复现用户「正确 v0.9.0 目录配置失败」并确认真实旧版本 Bug：官方 v0.9.0 资产 hash 与旧 pinned 一致、upstream 校验本应通过，真实阻塞是 launcher 不可用时 `package_dimension` 返回 `unavailable`/`launcher_unavailable`，而 `integrationBlockedReason` 只区分 untrusted、仍误报“请先导入并通过校验图谱组件。”；修复为 `launcher_unavailable → cbm.launcherRequired`（“受限 launcher 未找到，图谱组件无法启用。请确认 d-token 安装完整后刷新。”），先补失败测试再修复（CodebaseMemory Vitest 新增 1 例后 `13/13`）。真实二进制验收：debug launcher + 官方 v0.9.1-rc.1 二进制在临时 HOME/project/cache/backups 上完成 preview→apply→verify→rollback，rollback 后 `opencode.json` 原始 bytes/hash/权限位（0o640）全部恢复。验证：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 通过（含 HEAD 既有 dtoken-core-api 2 处 rustfmt 漂移与 1 处 redundant-pattern-matching 的最小修复）；`cargo test -p dtoken-codebase-memory` `18 passed`、`dtoken-config-patcher` integration `11 passed / 2 ignored`、desktop native `codebase_memory` `8 passed / 1 ignored`、全量桌面 Vitest `44 files / 267 tests`、Vite build（递归 JS 144337 B / 180224 B）通过；tsc 仍仅剩既有 `discovery.ts` 2 错。Windows runner / Playwright e2e / release 重打包未在本轮执行。

2026-08-07 最新全仓复证与 macOS rebuilt audit：`cargo test --workspace` 936 pass/5 ignored、workspace clippy strict pass、前端 Vitest 634 pass、`tsc --noEmit` 与 `npm run build` pass、Chromium+WebKit Playwright E2E 各 13 pass；macOS rebuilt `.app`+DMG audit ad-hoc 通过（SHA-256 60f7d063…，未签名/未公证）。本轮新增：Codex 自动发现支持 `ocxlocal` openai-responses（`ProviderWireApi::OpenAiResponses` 原样透传 `/v1/responses`、不做 Chat 转换；Codex 排除显式 model 要求，`models=[]` 允许请求 model 动态透传；core native Responses 非流返回上游 body 原样、流 SSE body 原样且仅终态 finalize；Chat-wire 仍桥接并 fail-closed）、Overview/Analytics 设置价格表成本估算（input cost before/after/delta，unknown/mixed/过期 fail-closed，`provider_cost_usd` 单独呈现）、tray 物理坐标 placement（多显示器/DPI 单测 8/8）。Windows 仅 source/CI 合同、无当前实机；unsigned/unnotarized 与 Windows/G8 仍阻断，新 ocxlocal 路径真实 Codex routed acceptance 仍缺失。

Tauri 构建必须通过 `npm run tauri:dev` 或 `npm run tauri:build`，以便先准备对应平台的 `dtoken-proxy` external binary。
