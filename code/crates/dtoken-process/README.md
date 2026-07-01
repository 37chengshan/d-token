# dtoken-process

本地进程与实例管理模块。

职责：

1. 管理 instance lifecycle。
2. 检查端口占用。
3. 启动、停止、重启本地服务。
4. 收集 stdout/stderr。
5. 写入进程状态和日志路径。
6. 支持 Windows/macOS 差异化处理。

原则：不允许用户看不到的后台进程长期残留。
