# v0.7.6 遗留 Rust 审计

状态日期：2026-04-29

本次审计特意采取非破坏性的方式。在 v0.7.6 中，除非测试证明公共 CLI、已保存会话、工具模式和文档中记载的命令路径不再依赖某段兼容性代码，否则不会删除它。

## 摘要

| 影响面 | 所属模块 | 当前使用者 | 参考检查 | 兼容原因 | 当前警告 | 建议操作 |
|---|---|---|---|---|---|---|
| 遗留 MCP 同步 API（`McpServerInput`、`list`、`add`、`remove`、`call_tool`、`load_legacy`） | `crates/tui/src/mcp.rs` | 未接入当前 `/mcp` 命令路径；以 `#[allow(dead_code)]` 保留 | 已检查直接 Rust 引用和当前 MCP 命令路径；已保存/配置 JSON 兼容性仍需专门的冒烟测试 | 保留旧的 JSON 结构，包括 `mcpServers` 别名和同步调用辅助函数，同时异步 MCP 管理器是活动路径 | 仅有代码 TODO | 在 CLI/运行时对等测试证明没有调用者后，将其置于显式遗留模块下或移除。由 #218 跟踪。 |
| 遗留提示常量/函数（`AGENT_PROMPT`、`YOLO_PROMPT`、`PLAN_PROMPT`、`base_system_prompt`、`normal_system_prompt` 等） | `crates/tui/src/prompts.rs` | 测试和仍直接导入提示常量的旧调用者 | 仍存在直接 Rust 引用；未证明公共 crate 和旧测试工具不再导入 | 分层提示 API 取代了单体提示，但旧调用点可能仍能编译这些常量 | 无 | 在 v0.7.6 中保留；仅在内置调用者迁移完成后添加弃用注解。由 #219 跟踪。 |
| `/compact` 斜杠命令定位 | `crates/tui/src/commands/mod.rs` | 公共斜杠命令注册表和帮助覆盖层 | 公共命令注册表/文档路径仍引用它 | 当前 cycle/seam 策略更倾向于重启/cycle 流程，但用户可能仍手动运行 `/compact` | 描述称遗留并指向 cycle 重启 | 作为手动兼容命令保留；在上下文/令牌问题解决前不删除。 |
| `todo_*` 兼容工具 | `crates/tui/src/tools/todo.rs` | 仍使用 `todo_add`、`todo_update`、`todo_list`、`todo_write` 的工具注册表/模型调用 | 工具注册表兼容性和已保存工具调用风险仍然存在 | `checklist_*` 是规范名称，但旧工具名称可能出现在已保存的提示、跟踪或模型先验知识中 | 元数据标记 `compat_alias: true`；描述称兼容别名 | 添加带有目标版本的显式弃用元数据，然后在工具模式迁移证据就绪后移除。由 #220 跟踪。 |
| 已弃用的子代理别名工具（`spawn_agent`、`send_input`、delegate 别名） | `crates/tui/src/tools/subagent/mod.rs` | 工具注册表和模型/工具调用兼容性 | 工具注册表兼容性和已保存工具调用风险仍然存在 | 规范名称是 `agent_spawn`、`agent_send_input` 等；别名名称保留旧工具调用兼容性 | `_deprecation` 元数据和 tracing 警告；移除目标为 `v0.8.0` | 在 v0.7.x 期间保留；移除已有元数据。由 #221 跟踪。 |
| 遗留根/提供方 TOML `api_key` 兼容性 | `crates/tui/src/config.rs`、`crates/config/src/lib.rs` | 配置解析器；在配置文件中使用现有 `api_key` 的用户 | 公共配置加载和文档仍提及迁移行为 | 密钥环迁移是首选，但破坏现有配置将阻止启动/认证 | Tracing 警告指向 `deepseek auth set` / `deepseek auth migrate` | 保留；警告对用户可操作。移除应等待迁移命令和发布说明窗口。 |
| 模型别名规范化（`deepseek-chat`、`deepseek-reasoner`、旧版 V3/R1 别名） | `crates/tui/src/config.rs`、`crates/config/src/lib.rs` | 配置/环境/模型选择器规范化 | 公共文档和现有配置可能仍使用别名 | 保留旧的已文档化 DeepSeek 别名，并将其映射到 `deepseek-v4-flash` | 按设计静默别名处理 | 保留；删除别名会破坏配置而没有实质收益。 |
| 已弃用的调色板常量和别名 | `crates/tui/src/palette.rs`、`crates/tui/tests/palette_audit.rs` | 现有调用点加审计测试 | 调色板审计强制剩余的允许列表 | 语义别名是首选，但旧常量存在以防止广泛的样式变动 | 调色板审计阻止在允许列表之外直接使用弃用项 | 保留别名；继续逐步将调用点移至语义角色。 |

## 后续移除候选

这些在 v0.7.6 中移除不安全：

1. #218 遗留 MCP 同步 API：需要调用图检查以及 `/mcp`、`deepseek mcp` 和 MCP 服务器验证流程的显式 CLI/运行时对等测试。
2. #219 遗留提示常量/函数：需要证明没有公共 crate 或旧测试工具再导入它们。
3. #220 `todo_*` 工具别名：需要弃用元数据和已保存跟踪/工具模式迁移窗口。
4. #221 已弃用的子代理别名工具：移除目标已编码为 `v0.8.0`，但实际移除应单独跟踪和测试。

## 验证清单

在移除任何兼容性影响面之前：

1. 使用 `rg` 搜索直接的 Rust 引用。
2. 搜索文档和 README 命令示例。
3. 使用所有功能运行工作区测试。
4. 如果影响面涉及工具模式或持久化历史记录，运行已保存会话/工具调用兼容性冒烟测试。
5. 保留发布说明条目，对于用户可见的配置/工具更改，至少在一个次版本发布中保留迁移提示。