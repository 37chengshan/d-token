# No-touch Testing Guide

日期：2026-07-01

## 1. 核心原则

本项目默认不能触碰用户已经部署好、已经跑通的 RTK、Headroom 和 AI agents。

No-touch testing 的目标是：

```text
测试 d-token 的解析、合并、展示、归因、配置计划能力
但不测试、不修改、不重启用户真实环境
```

## 2. 禁止行为

在默认测试模式下禁止：

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
13. 不启动、停止、重启用户真实服务。

## 3. 测试模式

### 3.1 fixture mode

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

### 3.2 fake-binary mode

把 `support/fake-bin` 放到 PATH 最前面，让 d-token 调到假的 `rtk` / `headroom`。

```bash
PATH="$PWD/../support/fake-bin:$PATH" cargo run -p dtoken-proxy -- --port 38101 --config configs/config.example.json
```

适合测试：

1. command runner。
2. stdout parser。
3. missing / found / error boundary。
4. exit code handling。

### 3.3 shadow mode

只读检测真实环境，但不修改任何东西。

```bash
DTOKEN_PROFILE=shadow cargo run -p dtoken-proxy -- --port 38101 --config configs/config.example.json
```

注意：shadow mode 仍可能调用 `rtk --version` / `headroom --version` 等只读命令。只有用户主动选择时才使用。

## 4. 推荐测试顺序

```text
cargo check/test
  ↓
fixture mode
  ↓
fake-binary mode
  ↓
shadow mode（用户主动）
  ↓
真实 external connect（用户主动）
```

## 5. 本地命令

从仓库根目录运行：

```bash
bash support/scripts/run_fixture_tests.sh
bash support/scripts/run_fake_binary_tests.sh
```

这两个脚本不应该碰用户真实 RTK / Headroom / agent。

## 6. 端口规则

测试端口必须使用 381xx 系列：

```text
38101 d-token fixture proxy
38102 fake headroom fixture endpoint, if needed later
38103 reserved
```

不要使用用户真实环境的 proxy 端口。

## 7. GUI 测试规则

GUI 默认应该进入 Fixture Demo Mode。

入口顺序：

1. Fixture Demo Mode：只看假数据。
2. Fake Binary Mode：测试命令执行边界。
3. Shadow Detect Mode：用户主动选择，只读真实环境。
4. Connect Existing Setup：用户主动选择，仍然不修改。
5. Managed Mode：只给新用户或主动托管用户使用。

## 8. 验收标准

No-touch 测试通过的标准：

1. `DTOKEN_PROFILE=fixture` 下不调用真实 `rtk`。
2. `DTOKEN_PROFILE=fixture` 下不调用真实 `headroom`。
3. fake-binary mode 下调用的是 `support/fake-bin`。
4. 测试端口不等于用户真实 proxy 端口。
5. 任何 config patch 都只生成 plan，不 apply。
6. 所有 diff/backup/rollback 都指向临时目录或 fixture 目录。
7. Dashboard 可用 fixture 数据完整渲染。

## 9. 最重要结论

对于已经部署好 RTK/Headroom/agent 的用户，d-token 的测试必须先验证自身逻辑，而不是验证用户真实环境。

```text
真实环境保持稳定
测试环境完全隔离
GUI 默认展示 fixture
用户主动选择后才读取真实状态
```
