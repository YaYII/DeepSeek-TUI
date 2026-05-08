# DeepSeek TUI 运维手册

本手册涵盖本地 CLI/TUI 运行时的实用调试和事件响应。

## 快速排查

1. 确认二进制文件 + 配置：
   - `cargo run -- --version`
   - `cat ~/.deepseek/config.toml`（或检查已配置的 profile）
2. 启用详细日志：
   - `RUST_LOG=deepseek_cli=debug cargo run`
   - 如需 HTTP 重试/重连：`RUST_LOG=deepseek_cli::client=debug cargo run`
3. 捕获当前状态：
   - `ls ~/.deepseek/sessions`
   - `ls ~/.deepseek/sessions/checkpoints`
   - `ls ~/.deepseek/tasks`

## 事件：轮次挂起或流停止

症状：
- TUI 停留在加载状态
- 助手输出不完整，未完成

检查：
1. 检查重试/健康日志（`deepseek_cli::client`）
2. 验证端点连通性：
   - `curl -sS https://api.deepseek.com/beta/models -H "Authorization: Bearer $DEEPSEEK_API_KEY"`
3. 确认工具输出中没有本地沙箱/权限死锁

操作：
1. 如果前台 shell 命令正在运行，按 `Ctrl+B` 并选择将其后台运行或取消当前轮次。
2. 如果命令已在后台启动，要求助手使用 `exec_shell_cancel` 及返回的任务 ID 来取消它。
3. 当你想要停止请求本身时，使用 `Esc` 或 `Ctrl+C` 中断当前轮次。
4. 重试提示；如果仍然失败，重启 TUI。
5. 重启后，验证之前排队/运行中的轮次显示为已中断，而非停留在运行状态。

## 事件：网络中断 / 离线行为

预期行为：
- 离线模式激活时，新提示会被排队
- 队列状态持久保存到 `~/.deepseek/sessions/checkpoints/offline_queue.json`

检查：
1. 在 TUI 中打开队列：`/queue list`
2. 确认持久化的队列文件存在且时间戳已更新

操作：
1. 恢复网络连接
2. 重新发送队列中的条目（通过 `/queue edit <n>` + Enter，或常规输入流程）
3. 确保队列清空后队列文件已清除

## 事件：需要崩溃恢复

预期行为：
- 检查点存储在 `~/.deepseek/sessions/checkpoints/latest.json`
- 除非提供 `--resume`/`--continue` 参数，否则启动时开始新会话

操作：
1. 通过 `deepseek --resume <id>` 或 TUI 中的 `Ctrl+R` 显式恢复之前的工作
2. 如需检查检查点，检查 `latest.json` 中的 schema 不匹配/详情
3. 如果 schema 版本高于二进制文件支持版本，升级二进制文件或删除过时的检查点

## 事件：持久化状态 Schema 错误

症状：
- 错误信息如 `schema vX is newer than supported vY`

受影响的存储：
- 会话（`~/.deepseek/sessions/*.json`）
- 运行时线程/轮次/消息记录
- 任务（`~/.deepseek/tasks/tasks/*.json`）

操作：
1. 确认二进制文件版本和迁移预期
2. 在编辑之前备份状态目录
3. 采取以下任一措施：
   - 使用更新且兼容的二进制文件运行，或
   - 归档不兼容的记录并重新生成状态

## 事件：MCP/工具执行失败

检查：
1. 验证 `~/.deepseek/mcp.json` 的 schema 和服务器命令路径
2. 确认服务器进程可以手动启动
3. 检查 TUI 历史记录/日志中的沙箱拒绝信息

操作：
1. 使用所需的审批重试（或仅在适当时使用 YOLO 模式）
2. 临时禁用有问题的 MCP 服务器并隔离问题
3. 通过 `/mcp` 诊断工具验证后重新启用

## 事件后检查清单

1. 保存日志和相关状态文件
2. 记录触发条件、影响范围和缓解措施
3. 添加或更新回归测试（重试/恢复/schema）
4. 如果行为发生变化，更新本手册和架构文档
