# No-touch Testing Guide

日期：2026-07-01

## 1. 核心原则

No-touch 只适用于 **本地开发测试阶段**，目的是保护开发者机器上已经部署好、正在使用的 RTK、Headroom 和 AI agents。

它不是成品安装策略。成品安装器可以覆盖安装、升级、配置 hooks、修改 shell/profile 和 provider routes，但必须走 InstallationTransaction：diff、backup、confirm、apply、doctor、rollback point。

No-touch testing 的目标是：

```text
测试 d-token 的解析、合并、展示、归因、配置计划能力
但不测试、不修改、不重启开发者真实环境
```

## 2. 适用范围

适用于：

1. 本地开发。
2. CI。
3. GUI demo。
4. parser tests。
5. adapter tests。
6. event ledger tests。
7. dashboard mock/fixture tests。

不适用于：

1. 成品一键安装。
2. 用户主动选择 Managed Mode。
3. 用户主动点击 Connect Existing Setup。
4. 用户主动确认配置 patch。
5. 发布版 installer 覆盖安装流程。

## 3. 开发期禁止行为

在默认开发测试模式下禁止：

1. 不调用真实 RTK binary。
2. 不调用真实 Headroom binary。
3. 不读取真实 agent 配置。
4. 不修改真实 agent hook。
5. 不修改真实 shell profile。
6. 不修改真实 RTK config。
7. 不修改真实 Headroom config。
8. 不读取真实 RTK tracking DB。
9. 不读取真实 RTK tee 完整文件。
10. 不读取真实 Headroom CCR 原文缓存。
11. 不连接真实 provider key。
12. 不占用真实 proxy 端口。
13. 不启动、停止、重启开发者真实服务。

## 4. 测试模式

### 4.1 fixture mode

完全读取 `support/fixtures/`，不调用任何真实命令。

```bash
DTOKEN_PROFILE=fixture cargo run -p dtoken-proxy -- --port 38101 --config configs/config.example.json
```

适合测试：

1. RTK parser。
2. Headroom parser。
3. API response。
4. Dashboard cards。
5. Event ledger。
6. Layered savings attribution。

### 4.2 fake-binary mode

把 `support/fake-bin` 放到 PATH 最前面，让 d-token 调到假的 `rtk` / `headroom`。

```bash
PATH="$PWD/../support/fake-bin:$PATH" cargo run -p dtoken-proxy -- --port 38101 --config configs/config.example.json
```

适合测试：

1. command runner。
2. stdout parser。
3. missing / found / error boundary。
4. exit code handling。

### 4.3 shadow mode

只读检测真实环境，但不修改任何东西。

```bash
DTOKEN_PROFILE=shadow cargo run -p dtoken-proxy -- --port 38101 --config configs/config.example.json
```

注意：shadow mode 仍可能调用 `rtk --version` / `headroom --version` 等只读命令。只有开发者主动选择时才使用。

### 4.4 managed install mode

这是成品安装模式，不属于 no-touch testing。

Managed install 可以做：

1. install。
2. update。
3. uninstall。
4. hook setup。
5. shell/profile patch。
6. provider route patch。
7. config apply。

但必须走：

```text
scan → plan → diff → backup → confirm → apply → doctor → rollback point
```

## 5. 推荐开发测试顺序

```text
cargo check/test
  ↓
fixture mode
  ↓
fake-binary mode
  ↓
shadow mode（开发者主动）
  ↓
真实 external connect（开发者主动）
```

## 6. 本地命令

从仓库根目录运行：

```bash
bash support/scripts/run_fixture_tests.sh
bash support/scripts/run_fake_binary_tests.sh
```

这两个脚本不应该碰开发者真实 RTK / Headroom / agent。

## 7. 端口规则

开发测试端口必须使用 381xx 系列：

```text
38101 d-token fixture proxy
38102 fake headroom fixture endpoint, if needed later
38103 reserved
```

不要使用真实环境的 proxy 端口。

## 8. GUI 开发测试规则

开发期 GUI 默认应该进入 Fixture Demo Mode。

入口顺序：

1. Fixture Demo Mode：只看假数据。
2. Fake Binary Test Mode：测试命令执行边界。
3. Shadow Detect Mode：开发者主动选择，只读真实环境。
4. Connect Existing Setup：开发者主动选择，仍然不修改。
5. Managed Install Mode：成品安装路径，开发期必须显式开启。

## 9. 成品 GUI 规则

成品 GUI 可以提供一键安装和覆盖安装，但不能无提示写配置。成品安装器必须展示：

1. 即将安装什么。
2. 即将覆盖什么。
3. 即将修改哪些配置。
4. diff。
5. backup 路径。
6. rollback 入口。
7. doctor 结果。

## 10. 验收标准

No-touch 开发测试通过的标准：

1. `DTOKEN_PROFILE=fixture` 下不调用真实 `rtk`。
2. `DTOKEN_PROFILE=fixture` 下不调用真实 `headroom`。
3. fake-binary mode 下调用的是 `support/fake-bin`。
4. 测试端口不等于真实 proxy 端口。
5. 任何 config patch 都只生成 plan，不 apply。
6. 所有 diff/backup/rollback 都指向临时目录或 fixture 目录。
7. Dashboard 可用 fixture 数据完整渲染。

成品安装通过的标准：

1. 可以覆盖安装。
2. 可以升级。
3. 可以修改 hooks/config。
4. 可以卸载。
5. 所有写操作都有 backup。
6. 所有写操作可 rollback。
7. 安装失败能恢复。

## 11. 最重要结论

```text
开发期：真实环境保持稳定，测试环境完全隔离
成品期：可以覆盖安装，但必须事务化、可备份、可回滚
```
