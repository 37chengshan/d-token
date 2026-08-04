# support/ AGENTS.md

本文件是 `support/` 目录的 per-directory 入口。通用规则以根 `AGENTS.md` 为准。

## 1. support/ 职责

测试、脚本、fixture、fake-bin、CI 与审计账本。

```text
support/
├── scripts/                    构建/检查/发布/审计脚本
├── fake-bin/                   测试用 fake 二进制 shim
├── fixtures/                   测试 fixture（cc-switch 等）
├── artifacts/                  本地产物（gitignored）
└── git-operations.json         Git 操作审计账本
```

## 2. 脚本分类

### 结构检查

- `check_structure.py` — 仓库结构检查（新增/移动/删除文档后必须更新）
- `check_release_scope.py` — 发布范围检查
- `check_release_version.py` — 发布版本检查

### Release gate

- `run_release_gate_contract_test.ps1` — release gate 契约测试
- `run_release_handoff_manifest.ps1` — release handoff manifest
- `run_release_verification.ps1` — release 验证
- `run_release_performance_gate.ps1` — release 性能 gate
- `run_runtime_performance_gate.py` — runtime 性能 gate
- `run_macos_release_package_audit.sh` — macOS 发布包审计
- `run_windows_signing_readiness_audit.ps1` — Windows 签名就绪审计

### Fixture 生成

- `generate_cc_switch_fixture_db.py` — 生成 cc-switch fixture db
- `generate_headroom_capability_snapshot.py` — 生成 headroom 能力快照
- `generate_release_metadata.py` — 生成发布元数据
- `generate_tray_template_icons.py` — 生成托盘模板图标

### 测试脚本

- `run_fixture_tests.sh` / `.ps1` — fixture 测试
- `run_opencode_real_routed_test_macos.sh` — OpenCode 真实路由测试
- `run_opencode_workload_benchmark.ps1` — OpenCode workload benchmark
- `run_managed_rtk_candidate_test_macos.sh` — managed RTK candidate 测试
- `run_agent_transport_candidate_test_macos.sh` — agent transport candidate 测试
- `run_codec_candidate_evidence.ps1` — codec candidate evidence
- `run_candidate_capability_registry_snapshot.ps1` — candidate capability registry 快照

### 审计与对照

- `run_headroom_capability_coverage_audit.ps1` — headroom 能力覆盖审计
- `run_release_artifact_redaction_audit.ps1` — release artifact 脱敏审计
- `test_codec_candidate_evidence_contract.py` — codec candidate evidence 契约测试
- `test_generate_headroom_capability_snapshot.py` — headroom 能力快照生成测试
- `test_headroom_capability_coverage_audit_contract.py` — headroom 能力覆盖审计契约测试
- `test_runtime_performance_gate.py` — runtime 性能 gate 测试

## 3. fake-bin 边界

1. **fake-bin 只用于测试**：`support/fake-bin/dtoken-agent-bridge` 是测试用 shim，不是生产二进制。
2. **fake-bin 必须可区分**：用 `.cmd` 后缀（Windows）或无后缀（Unix），且内容中标注 `FAKE BIN - FOR TESTING ONLY`。
3. **禁止把 fake-bin 放入生产 bundle**。

## 4. fixture 边界

1. **fixture 是测试证据**：`support/fixtures/cc-switch/cc-switch.db` 是二进制 fixture。
2. **fixture 不含真实凭据**：fixture db 中的 auth 字段是测试占位符。
3. **修改 fixture 必须同步**：对应的 `tests/` 文件、`generate_*.py` 脚本。

## 5. Git 操作审计（硬约束）

所有 commit、branch create/switch/delete/rename、merge、rebase 和 push 操作，执行前必须：

```bash
python3 support/scripts/git_operation_audit.py record \
  --operation <commit|branch-create|push|...> \
  --branch <branch-name> \
  --head-before <HEAD-sha> \
  --summary "一句话说明"
```

写入 `support/git-operations.json`。**禁止删除或改写既有记录**。

## 6. 验证

```bash
# 结构检查
python3 support/scripts/check_structure.py

# Fixture 测试
bash support/scripts/run_fixture_tests.sh
# 或
powershell -File support/scripts/run_fixture_tests.ps1

# 审计脚本测试
python3 support/scripts/test_codec_candidate_evidence_contract.py
python3 support/scripts/test_runtime_performance_gate.py
```

## 7. 常见坑

1. **`git-operations.json` 是 append-only 账本**：不要删记录，不要改写记录。
2. **fixture db 是二进制**：不要用文本编辑器打开，用 `sqlite3` 或专用工具。
3. **脚本跨平台**：Windows 用 `.ps1`，macOS/Linux 用 `.sh`，Python 脚本用 `python3`。
