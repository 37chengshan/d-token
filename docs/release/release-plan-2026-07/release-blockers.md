# d-token Release Blockers

出现以下任一情况，不允许发布：

1. ~~d-token 本地服务无法稳定启动。~~ — 代码就绪，需本地真实验收。
2. ~~首页无法区分真实数据和示例数据。~~ — ✅ DataSourceBadge + payloadStatus 区分。
3. ~~第三方工具被自动安装或静默修改。~~ — ✅ canInstallAgent 一律返回 false。
4. ~~cc-switch profile 切换后 d-token 仍可能使用旧上游但 UI 不提示。~~ — ✅ computeSyncStatus + CcSwitchSummary。
5. ~~配置写入没有备份。~~ — ✅ InstallationTransaction 强制 backupRefs。
6. ~~配置写入没有回退点。~~ — ✅ canComplete 强制 rollbackPointId。
7. ~~智能学习会改动非 d-token 管理块内容。~~ — ✅ verifyNonManagedInvariant。
8. ~~日志或导出泄露密钥、token、cookie、认证头、完整 prompt、完整 response。~~ — ✅ RedactionPolicy + auditExportForSecrets。
9. ~~Doctor 对高风险修复没有确认流程。~~ — ✅ RepairAction.requiresConfirmation + isAutoRepairable。
10. ~~任意页面出现无法解释的冷错误或空白页。~~ — ✅ EmptyState 统一状态组件。
11. ~~Windows 或 macOS 核心流程完全不可用且文档未说明。~~ — 代码就绪，需本地真实验收。
12. ~~README 不能指导新用户完成安装、连接、验证和回退。~~ — ✅ README 已更新。

当前状态：仓库内可验证的 Release Blockers 已全部解决；需要本地真实环境验收的项见 release-readiness-checklist.md。
