# No-touch Testing 多维度审查报告

日期：2026-07-01  
审查对象：

1. `docs/TESTING_NO_TOUCH.md`
2. `support/fixtures/*`
3. `support/fake-bin/*`
4. `support/scripts/run_fixture_tests.sh`
5. `support/scripts/run_fake_binary_tests.sh`
6. `code/crates/dtoken-preprocessor`
7. `code/crates/dtoken-headroom-adapter`

## 1. 总结论

No-touch 方案方向正确，已经解决了最重要的问题：**默认不碰用户已经部署好、已经跑通的 RTK / Headroom / agents**。

当前设计已经具备：

1. 明确的 no-touch 文档边界。
2. RTK fixtures。
3. Headroom fixtures。
4. fake RTK binary。
5. fake Headroom binary。
6. fixture mode smoke script。
7. fake-binary mode smoke script。
8. RTK adapter `DTOKEN_PROFILE=fixture` 分支。
9. Headroom adapter `DTOKEN_PROFILE=fixture` 分支。

但它还不是完全严密的方案，主要缺口包括：

1. 缺少“真实 binary 绝不会被调用”的自动断言。
2. 缺少 Windows / PowerShell no-touch 脚本。
3. Smoke script 使用 `sleep 2`，存在 flaky 风险。
4. 失败时不自动打印 server log，排障体验不够好。
5. fake-binary mode 当前只测 version/status，没有覆盖 gain/discover/session/stats parser。
6. fixture mode 只靠环境变量切换，缺少更强的 compile/test guard。
7. GUI 入口还没有实现“默认 Fixture Demo Mode”的强制约束。
8. CI 目前只跑 cargo check/test，不跑 no-touch smoke。

最终判断：

```text
当前方案：可接受的 M1 no-touch foundation
成熟度：约 8.2/10
进入 M2 前应补强：poison binary、retry wait、Windows script、CI no-touch smoke、parser fixture tests
```

## 2. 安全隔离维度

评分：8.8/10。

### 已做对

`docs/TESTING_NO_TOUCH.md` 明确禁止调用真实 RTK、真实 Headroom、真实 agent 配置、真实 hook、真实 shell profile、真实 config、真实 tracking DB、真实 tee、真实 CCR cache、真实 provider key、真实 proxy 端口以及真实服务 restart。

RTK adapter 在 `DTOKEN_PROFILE=fixture` 下直接返回 fixture status，不调用 `Command::new("rtk")`。

Headroom adapter 在 `DTOKEN_PROFILE=fixture` 下直接返回 fixture status，不调用 `Command::new("headroom")`。

### 主要缺口

当前还没有自动证明“真实 binary 没有被调用”。

更强方案：新增 poison-bin。

```text
support/poison-bin/rtk
support/poison-bin/headroom
```

poison binary 行为：只要被调用就 `exit 99`，并打印：

```text
ERROR: real/fake command path should not be called in fixture mode
```

然后在 fixture smoke 里设置：

```bash
PATH="$ROOT/support/poison-bin:$PATH"
DTOKEN_PROFILE=fixture
```

如果 fixture mode 仍然误调用 `rtk` 或 `headroom`，测试会立刻失败。

## 3. 可执行性维度

评分：7.8/10。

### 已做对

`run_fixture_tests.sh` 会执行：

1. structure check。
2. cargo check。
3. cargo test。
4. 启动 d-token proxy。
5. 检查 `/health`、`/rtk/status`、`/rtk/gain`、`/headroom/status`、`/headroom/stats`、`/projects/default-policy`、`/config/patches/plan`。

`run_fake_binary_tests.sh` 使用临时 `HOME` 并把 `support/fake-bin` 放到 PATH 前面。

### 主要缺口

#### 缺口 1：`sleep 2` 不稳定

当前脚本使用：

```bash
sleep 2
```

这在低性能机器或 CI 上可能失败。

建议改成 retry wait：

```bash
for i in {1..30}; do
  if curl -fsS "http://127.0.0.1:${PORT}/health" >/dev/null; then
    break
  fi
  sleep 1
done
```

#### 缺口 2：失败时不打印日志

当前脚本把 log 写到 `/tmp`，但失败时不自动输出。

建议增加：

```bash
trap 'cat "$LOG" >&2 || true; cleanup' ERR
```

#### 缺口 3：端口冲突没有提前检测

虽然测试使用 381xx 端口，但仍可能被占用。

建议启动前检测：

```bash
if lsof -i TCP:${PORT} >/dev/null 2>&1; then
  echo "port ${PORT} already in use" >&2
  exit 1
fi
```

或让 d-token 支持 `--port 0` 随机端口并输出实际端口。

## 4. 跨平台维度

评分：5.8/10。

### 已做对

当前 bash 脚本适合 macOS/Linux/WSL。

### 主要缺口

Native Windows 下不够好：

1. `bash` 不一定存在。
2. `chmod +x` 不适合 Windows native。
3. fake binary 无 `.cmd` / `.ps1` 版本。
4. 端口检测命令不同。
5. 临时 HOME 变量对 Windows 工具并不一定等价。

建议新增：

```text
support/fake-bin/rtk.cmd
support/fake-bin/headroom.cmd
support/scripts/run_fixture_tests.ps1
support/scripts/run_fake_binary_tests.ps1
```

Windows 原生 no-touch 测试必须覆盖：

1. PowerShell。
2. cmd PATH precedence。
3. TEMP test home。
4. 381xx 端口。
5. 不读取真实 `%USERPROFILE%` agent config。

## 5. 测试覆盖维度

评分：7.0/10。

### 已覆盖

1. Status API。
2. Basic fixture stats。
3. Project default policy。
4. Config patch plan。
5. Event ledger indirectly。

### 未覆盖

1. RTK gain parser 单元测试。
2. RTK discover parser 单元测试。
3. RTK session parser 单元测试。
4. Headroom stats-history parser。
5. Headroom metrics parser。
6. Headroom doctor parser。
7. layered savings attribution calculation。
8. config patch 不 apply 的强断言。
9. GUI fixture mode default。
10. fake-binary exit code failure path。

建议补：

```text
code/crates/dtoken-preprocessor/tests/fixture_gain.rs
code/crates/dtoken-preprocessor/tests/fake_binary.rs
code/crates/dtoken-headroom-adapter/tests/fixture_stats.rs
code/crates/dtoken-event-ledger/tests/attribution.rs
code/crates/dtoken-config-patcher/tests/no_apply.rs
```

## 6. 架构维度

评分：8.4/10。

### 已做对

1. fixture mode 在 adapter 层处理，避免 API 层硬编码。
2. fake-binary mode 通过 PATH precedence 测 command runner。
3. fixture data 与 fake binaries 放在 `support/`，没有污染 `code/`。
4. `DTOKEN_FIXTURE_ROOT` 允许自定义 fixture 根目录。

### 主要缺口

#### 缺口 1：Profile 模型还只是环境变量

目前使用：

```text
DTOKEN_PROFILE=fixture
```

建议后续提升为正式配置：

```ts
interface RuntimeProfile {
  mode: 'fixture' | 'fake_binary' | 'shadow' | 'external' | 'managed';
  no_touch: boolean;
  allow_command_exec: boolean;
  allow_config_read: boolean;
  allow_config_write: boolean;
  allow_service_control: boolean;
}
```

#### 缺口 2：缺少 NoTouchGuard

建议在 Rust core 增加：

```text
dtoken-safety-guard
```

职责：

1. 根据 profile 决定是否允许 command exec。
2. 根据 profile 决定是否允许 config read/write。
3. 根据 profile 决定是否允许 service start/stop。
4. 在 fixture mode 下拒绝所有外部命令调用。

## 7. GUI 维度

评分：6.5/10。

文档已经要求 GUI 默认 Fixture Demo Mode，但代码还没实现。

M3 GUI 必须强制：

1. 第一次启动默认 fixture。
2. 不自动扫描真实环境。
3. “Shadow Detect Mode” 需要用户点击。
4. “Connect Existing Setup” 需要用户点击。
5. “Managed Mode” 默认隐藏或关闭。
6. 每个可能触碰真实环境的按钮必须标记风险。

建议 GUI 首页顺序：

```text
Fixture Demo Mode        默认
Fake Binary Test Mode    开发测试
Shadow Detect Mode       只读真实环境，用户主动
Connect Existing Setup   用户主动连接现有环境
Managed Mode             新用户可选
```

## 8. CI 维度

评分：6.8/10。

### 已做对

已有 `.github/workflows/ci.yml`，覆盖 structure check、cargo check、cargo test。

### 缺口

CI 没有跑：

```bash
bash support/scripts/run_fixture_tests.sh
bash support/scripts/run_fake_binary_tests.sh
```

建议拆分：

```text
ci-core:
  structure check
  cargo check
  cargo test

ci-no-touch:
  run_fixture_tests.sh
  run_fake_binary_tests.sh
```

注意：CI 跑 fake-binary mode 时，本来就没有真实 RTK/Headroom，所以很适合作为安全回归测试。

## 9. 优先级排序

### P0：必须补

1. poison-bin：证明 fixture mode 不会执行任何外部命令。
2. retry wait：替换 `sleep 2`。
3. error log dump：失败时打印 server log。
4. CI 跑 no-touch smoke。

### P1：强烈建议

1. Windows PowerShell no-touch scripts。
2. `.cmd` fake binaries。
3. parser unit tests。
4. RuntimeProfile / NoTouchGuard。

### P2：后续

1. GUI default fixture mode enforcement。
2. shadow mode confirmation UX。
3. managed mode risk labels。
4. fixture generator：从脱敏样本生成 fixture。

## 10. 最终结论

当前 no-touch 方案已经满足 M1 的基础需求：它能让 d-token 在不动用户真实环境的前提下进行开发验证。

但如果要进入 M2/M3，它必须补强为：

```text
不只是“理论上不碰真实环境”
而是“测试能自动证明没有碰真实环境”
```

下一步最应该实现的是：

```text
support/poison-bin/*
retry wait helper
failure log dump
CI no-touch smoke
Windows scripts
```
