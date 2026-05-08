# 工具表面

为什么选用这些特定的工具，为什么这样分组，以及每种工具在什么情况下应优先于可用的 shell 等价物。与 `crates/tui/src/prompts/agent.txt` 配套使用。

## 设计立场

- **当专用工具能返回结构化输出时，优先使用专用工具而非 `exec_shell`。** Bash 转义容易出错，且平台行为各异（GNU 与 BSD 的 `grep` 不同，`rg` 并非总是已安装）。结构化输出还能让模型免于重新解析自由格式文本。
- **其余一切使用 `exec_shell`。** 构建、测试、格式化、lint、临时命令，以及任何平台相关操作。我们不会试图包装所有长尾场景。
- **淘汰那些不优于 shell 等价物的工具。** 为同一底层操作设置两个工具别名对模型是一种陷阱——LLM 会在它们之间交替使用，导致缓存命中率下降。

## 当前表面（v0.7.5）

### 文件操作

| 工具 | 适用场景 |
|---|---|
| `read_file` | 读取 UTF-8 文件。PDF 在可用时通过 `pdftotext`（poppler）自动提取；`pages: "1-5"` 可分割大型文档。 |
| `list_dir` | 结构化的、感知 gitignore 的目录列表。优先于 `exec_shell("ls")`。 |
| `write_file` | 创建或覆盖文件。 |
| `edit_file` | 在单个文件中执行搜索替换。比全面重写更轻量。 |
| `apply_patch` | 应用统一格式的 diff。适用于多块编辑。 |
| `retrieve_tool_result` | 读取先前大型工具输出被溢出到 `~/.deepseek/tool_outputs/` 的摘要或切片；使用 `summary`、`head`、`tail`、`lines` 或 `query`，而非重放整个结果。 |

### 搜索

| 工具 | 适用场景 |
|---|---|
| `grep_files` | 在工作区内使用正则表达式搜索文件内容；返回结构化匹配结果和上下文行。纯 Rust 实现（`regex` crate），无需调用 `rg`/`grep` shell。 |
| `file_search` | 模糊匹配文件名（而非内容）。当你大概知道文件名时使用。 |
| `web_search` | DuckDuckGo（备选 Bing）；返回排序后的摘要和用于引用的 `ref_id`。 |
| `fetch_url` | 对已知 URL 直接发起 HTTP GET。当链接已知时比 `web_search` 更快。默认将 HTML 转为纯文本。 |

### Shell

| 工具 | 适用场景 |
|---|---|
| `exec_shell` | 运行 shell 命令。前台运行可取消，但仅用于有界命令；超时会杀死进程并返回后台重跑提示。 |
| `exec_shell_wait` | 轮询后台任务的增量输出。取消本轮对话会停止等待，但不会杀死任务。 |
| `exec_shell_interact` | 向正在运行的后台任务发送 stdin 并读取增量输出。 |
| `exec_shell_cancel` | 按 ID 取消一个正在运行的后台 shell 任务，或在明确请求时取消所有正在运行的后台 shell 任务。 |
| `task_shell_start` | 在后台启动一个长时间运行的命令并立即返回。对于可能运行数分钟的诊断、测试、搜索和服务器，优先于前台 shell。 |
| `task_shell_wait` | 轮询后台命令。如果完成后提供了 `gate`，则在活动的持久化任务上记录结构化的门控证据。 |

当前台 shell 命令超时时，进程不会静默继续。工具结果会告知模型使用 `task_shell_start` 或带 `background = true` 的 `exec_shell` 重新运行长时间工作，然后使用 `task_shell_wait` 或 `exec_shell_wait` 进行轮询。

交互式 shell 任务也可通过 `/jobs` 查看。TUI 任务中心由与 `exec_shell`/`task_shell_start` 相同的 shell 管理器驱动，显示命令、cwd、已用时间、状态、输出尾部、进程本地 shell ID，以及（如果可用）关联的持久化任务 ID。`/jobs show`、`/jobs poll`、`/jobs wait`、`/jobs stdin` 和 `/jobs cancel` 提供对实时任务的查看、轮询、stdin 和取消控制。任务是进程本地的；重启后，实时进程状态不会重新关联，任何记住的已分离条目必须标记为过时，而不是作为实时进程呈现。

### MCP 管理器和命令面板发现

MCP 服务器配置通过 `/mcp` 和 `/config` 中的 `mcp_config_path` 行在 TUI 中展示。`/mcp` 显示已解析的配置路径、服务器启用/禁用状态、传输方式、命令或 URL、超时时间、连接错误以及已发现的工具/资源/提示词。它支持狭窄的管理器操作：init、add、enable、disable、remove、validate 和 reload/reconnect。配置编辑会立即写入，但模型可见的 MCP 工具池需要重启后才能生效。

命令面板包含按服务器分组的 MCP 条目。禁用和失败的服务器仍然可见，已发现的工具/提示词使用向模型展示的运行时名称，例如 `mcp_<server>_<tool>`。

### Git / 诊断 / 测试

| 工具 | 适用场景 |
|---|---|
| `git_status` | 在不运行 shell 的情况下检查仓库状态。 |
| `git_diff` | 检查工作树或暂存区的 diff。 |
| `diagnostics` | 一次调用获取工作区、git、沙箱和工具链信息。 |
| `run_tests` | `cargo test` 带可选参数。 |

### 任务管理和持久化工作

| 工具 | 适用场景 |
|---|---|
| `update_plan` | 用于复杂多步骤工作的结构化检查清单。 |
| `task_create` | 通过 `TaskManager` 创建/入队一个持久化后台任务。这是长时间运行的代理工作的实际可执行工作对象。 |
| `task_list` | 列出持久化任务及其状态和关联的运行时 ID。 |
| `task_read` | 读取持久化任务的详细信息：线程/轮次关联、时间线、检查清单、门控、工件、PR 尝试、GitHub 事件。 |
| `task_cancel` | 取消一个已入队或正在运行的持久化任务。需要审批。 |
| `checklist_write` | 在活动线程/任务下进行细粒度进度记录。检查清单状态从属于持久化任务。 |
| `checklist_add` / `checklist_update` / `checklist_list` | 单条检查清单操作。 |
| `todo_write` / `todo_add` / `todo_update` / `todo_list` | 检查清单工具的兼容性别名。现有会话继续工作，但新提示应使用 `checklist_*`。 |
| `note` | 供以后使用的单次重要事实记录。 |

### 验证门控和工件

| 工具 | 适用场景 |
|---|---|
| `task_gate_run` | 运行一个已审批的验证命令，并将结构化证据附加到活动的持久化任务上：命令、cwd、退出码、持续时间、分类、摘要和日志工件。 |

大型日志和命令输出应作为工件处理，并在转录中保留紧凑摘要。`task_gate_run` 对活动的持久化任务自动处理此操作。

### GitHub 上下文和受保护写入

| 工具 | 适用场景 |
|---|---|
| `github_issue_context` | 通过 `gh issue view` 获取只读 issue 上下文；大篇幅内容在可能时作为任务工件。 |
| `github_pr_context` | 通过 `gh pr view` 获取只读 PR 上下文；通过 `gh pr diff --patch` 可选捕获 diff；大篇幅内容/diff 在可能时作为任务工件。 |
| `github_comment` | 在 issue/PR 上发表带有结构化证据的评论，需要审批。 |
| `github_close_issue` | 关闭 issue，需要审批。要求非空的验收标准和证据；拒绝脏工作树，除非明确允许。切勿仅仅因为代理停止就关闭 issue。 |

### PR 尝试

| 工具 | 适用场景 |
|---|---|
| `pr_attempt_record` | 将当前 git diff 捕获为尝试元数据，并将补丁工件附加到持久化任务上。 |
| `pr_attempt_list` | 列出一个任务上记录的尝试。 |
| `pr_attempt_read` | 检查一条记录的尝试及其工件引用。 |
| `pr_attempt_preflight` | 对尝试补丁运行 `git apply --check`。不修改工作树。 |

### 自动化

| 工具 | 适用场景 |
|---|---|
| `automation_create` | 创建定时自动化任务。需要审批。 |
| `automation_list` / `automation_read` | 检查持久化自动化和最近的运行记录。 |
| `automation_update` | 更新提示词、调度、cwd 或状态。需要审批。 |
| `automation_pause` / `automation_resume` / `automation_delete` | 生命周期控制。需要审批。 |
| `automation_run` | 立即运行一个自动化任务；运行会入队一个正常的持久化任务。需要审批。 |

### 子代理

`agent_spawn` 及配套工具（`agent_result` / `wait` / `send_input` / `agent_assign` / `agent_cancel` / `resume_agent` / `agent_list`）。委托协议参见 `agent.txt`，角色分类参见 [`SUBAGENTS.md`](SUBAGENTS.md)（`general` / `explore` / `plan` / `review` / `implementer` / `verifier` / `custom`）。

`agent_spawn` 默认启动一个全新的子对话。传递 `fork_context: true` 用于需要继承父级系统提示和消息前缀以复用 DeepSeek 前缀缓存的延续式工作。已弃用的 `delegate_to_agent` 兼容性包装器通过 `agent_spawn` 路由，并默认将 `fork_context` 设为 `true`。

### 并行扇出：成本等级限制

有两个工具提供并行扇出能力，但具有反映截然不同成本等级的不同并发限制：

| 工具 | 每个子任务的行为 | 墙钟时间 | Token 开销 | 上限 |
|---|---|---|---|---|
| `agent_spawn` | 完整子代理循环（规划、工具调用、多轮流式传输、可产生子代理） | 分钟 | 数千 token | 默认 10 个并发（`[subagents].max_concurrent`，硬上限 20） |
| `rlm` 辅助工具 `llm_query_batched` | 固定到 `deepseek-v4-flash` 的一次性非流式 Chat Completions 调用 | 秒 | 约数百 token | 每次调用 16 个 |

上限信息出现在每个工具的描述和错误消息中，以便模型（和用户）选择适合工作的工具。如果一个子代理就够用但需要并行查找，优先使用带 `llm_query_batched` 的 `rlm`；如果每个任务需要自己的带工具调用能力的代理循环，则使用 `agent_spawn`（并取消已完成的以释放槽位）。

## 近期整合（v0.5.1）

以下工具已从提示中移除，作为等价工具的重复项（底层分发器仍能解析它们，因此现有会话不会中断——它们只是不再污染模型的工具列表）：

- `spawn_agent` → 使用 `agent_spawn`。
- `close_agent` → 使用 `agent_cancel`。
- `assign_agent` → 使用 `agent_assign`。

## 弃用计划（v0.6.2 → v0.8.0）

下面的别名工具仍能成功执行，但现在会在返回的每个结果上附加一个 `_deprecation` 块。模型应在 v0.8.0 之前迁移到规范名称，届时这些别名将被移除。

| 已弃用的别名 | 规范名称 | 自版本起警告 | 移除版本 |
|---|---|---|---|
| `spawn_agent` | `agent_spawn` | v0.6.2 | v0.8.0 |
| `delegate_to_agent` | `agent_spawn` | v0.6.2 | v0.8.0 |
| `close_agent` | `agent_cancel` | v0.6.2 | v0.8.0 |
| `send_input` | `agent_send_input` | v0.6.2 | v0.8.0 |

`_deprecation` 块的形状：

```json
{
  "_deprecation": {
    "this_tool": "spawn_agent",
    "use_instead": "agent_spawn",
    "removed_in": "0.8.0",
    "message": "工具 'spawn_agent' 已弃用；请在 v0.8.0 之前切换到 'agent_spawn'。"
  }
}
```

该块会合并到工具结果的 `metadata` 对象中，与任何其他元数据键（例如 `status`、`timed_out`）并存，不会替换现有元数据。每次别名被调用时，还会在审计日志中以 `tracing::warn` 级别输出一行弃用警告。

## 为什么我们不提供一个单一的 `bash` 工具

单一的 `bash` 代理（如 Claude Code 的设计）虽然强大，但将 shell 脚本的所有隐患都交给了模型：引号问题、平台差异、因误读 cwd 导致的副作用、`cd` 在调用之间不持久等。我们的文件工具在转录中渲染的成本也显著更低（结构化的 JSON 格式输出比 `ls -la` 文本墙更紧凑）。

当某些功能缺失时，模型始终可以回退到 `exec_shell`。专用工具只是将常见的 80% 场景从 shell 逃生口中剥离出来。
