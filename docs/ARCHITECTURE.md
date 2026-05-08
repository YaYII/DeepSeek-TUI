# DeepSeek TUI 架构

本文档为开发者和贡献者提供 DeepSeek TUI 架构的概览。

当前边界说明（v0.8.6）：
- `crates/tui` 仍然是 TUI、运行时 API、任务管理器和工具执行循环的活跃终端用户运行时。
- 其他工作区 crate 正在逐步拆分，但它们尚未成为运行时的事实来源。
- LSP 子系统（`crates/tui/src/lsp/`）已完全接入引擎的工具后执行路径
  （`core/engine/lsp_hooks.rs`），在每次 edit_file/apply_patch/write_file 后提供内联诊断。
- 集群代理系统在 v0.8.5 中被移除，取而代之的是子代理（agent_spawn）和 RLM（rlm_query）。
  活动代码库中不再保留模型可见的集群工具。

## 高层概览

```
┌─────────────────────────────────────────────────────────────────┐
│                         用户界面                                │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐  │
│  │   TUI (ratatui) │  │  单次模式       │  │  配置/CLI      │  │
│  └────────┬────────┘  └────────┬────────┘  └────────┬───────┘  │
└───────────┼─────────────────────┼────────────────────┼──────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                        核心引擎                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    代理循环 (core/engine.rs)             │   │
│  │  ┌─────────┐  ┌─────────────┐  ┌──────────────────────┐ │   │
│  │  │ 会话    │  │ 回合管理    │  │ 工具编排             │ │   │
│  │  └─────────┘  └─────────────┘  └──────────────────────┘ │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                     工具与扩展层                                │
│  ┌──────────┐  ┌──────────┐  ┌─────────┐  ┌────────────────┐   │
│  │  工具    │  │  技能    │  │  钩子   │  │  MCP 服务器     │   │
│  │ (shell,  │  │ (插件)   │  │ (前/    │  │  (外部)        │   │
│  │  file)   │  │          │  │  后)    │  │                │   │
│  └──────────┘  └──────────┘  └─────────┘  └────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
            │                     │                    │
            ▼                     ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                  运行时 API + 任务管理                          │
│  ┌─────────────────────────────┐  ┌──────────────────────────┐  │
│  │ HTTP/SSE 运行时 API         │  │ 持久任务管理器           │  │
│  │ (runtime_api.rs)            │  │ (task_manager.rs)        │  │
│  └─────────────────────────────┘  └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
            │                     │
            ▼                     ▼
┌─────────────────────────────────────────────────────────────────┐
│                        LLM 层                                   │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              LLM 客户端抽象 (llm_client.rs)               │  │
│  │  ┌─────────────────┐  ┌─────────────────────────────┐    │  │
│  │  │  DeepSeek 客户端 │  │  兼容客户端 (DeepSeek)       │    │  │
│  │  │   (client.rs)   │  │       (client.rs)           │    │  │
│  │  └─────────────────┘  └─────────────────────────────┘    │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## 模块组织

### 入口点

- **`main.rs`** - CLI 参数解析（clap）、配置加载、入口点路由

### 核心组件

- **`core/`** - 主要引擎组件
  - `engine.rs` - 引擎状态、操作处理、消息处理
  - `engine/turn_loop.rs` - 流式回合循环和工具执行编排
  - `engine/capacity_flow.rs` - 容量护栏检查点和干预
  - `session.rs` - 会话状态管理
  - `turn.rs` - 基于回合的对话处理
  - `events.rs` - UI 更新的事件系统
  - `ops.rs` - 核心操作

### 配置

- **`config.rs`** - 配置加载、配置文件、环境变量
- **`settings.rs`** - 运行时设置管理

### 工作区 Crates

- **`crates/tools`** - 共享工具调用原语，包括 TUI 运行时使用的工具结果/错误/能力类型。
- **`crates/agent`** - 模型/提供者注册表（ModelRegistry），用于将模型 ID 解析到提供者端点。
- **`crates/app-server`** - HTTP/SSE + JSON-RPC 应用服务器传输，用于无头代理工作流。
- **`crates/config`** - 配置加载、配置文件、环境变量优先级、CLI 运行时覆盖。
- **`crates/core`** - 代理循环、会话管理、回合编排、容量流护栏。
- **`crates/execpolicy`** - 工具执行决策的审批/沙箱策略引擎。
- **`crates/hooks`** - 工具前后事件的生命周期钩子（stdout、jsonl、webhook）。
- **`crates/mcp`** - MCP 客户端 + stdio 服务器，用于模型上下文协议工具服务器。
- **`crates/protocol`** - 请求/响应框架和协议类型。
- **`crates/secrets`** - 用于 API 密钥存储的操作系统密钥环集成。
- **`crates/state`** - SQLite 线程/会话持久层。
- **`crates/tui-core`** - 事件驱动的 TUI 状态机框架。

### LLM 集成

- **`client.rs`** - DeepSeek 记录的 OpenAI 兼容 Chat Completions API 的 HTTP 客户端
- **`llm_client.rs`** - 具有重试逻辑的抽象 LLM 客户端 trait
- **`models.rs`** - API 请求/响应的数据结构

#### DeepSeek API 端点

DeepSeek 公开了 OpenAI 兼容的端点。CLI 使用：
- `https://api.deepseek.com/beta/chat/completions` - 默认 v0.8.16 DeepSeek 模型回合
- `https://api.deepseek.com/beta/models` - 默认 v0.8.16 实时模型发现和健康检查

`https://api.deepseek.com/v1` 被接受用于 OpenAI SDK 兼容性，并且
仍可显式配置以选择退出仅限 beta 的功能，如
严格工具模式、聊天前缀补全和 FIM 补全。公开的
DeepSeek 文档没有为此工作流记录 Responses API 路径；引擎
通过 Chat Completions 驱动回合。

### 工具系统

- **`tools/`** - 内置工具实现
  - `mod.rs` - 工具注册表和常见类型
  - `shell.rs` - Shell 命令执行
  - `file.rs` - 文件读/写操作
  - `todo.rs` - 检查列表工具加上遗留的 todo 别名
  - `tasks.rs` - 模型可见的持久任务、门控、后台 shell 和 PR 尝试工具
  - `github.rs` - 只读 GitHub 上下文和由 `gh` 支持的受保护评论/关闭工具
  - `automation.rs` - `AutomationManager` 上模型可见的调度工具
  - `plan.rs` - 规划工具
  - `subagent.rs` - 子代理生成（替换已移除的 `agent_swarm` 表面）
  - `spec.rs` - 工具规范
  - `rlm.rs` - 递归语言模型（RLM）工具 — 带有 `llm_query()` 辅助函数的沙箱化 Python REPL

### 扩展系统

- **`mcp.rs`** - 用于外部工具服务器的模型上下文协议客户端
- **`skills.rs`** - 插件/技能加载和执行
- **`hooks.rs`** - 带有条件的前后执行钩子

### 用户界面

- **`tui/`** - 终端 UI 组件（基于 ratatui）
  - `app.rs` - 应用状态和消息处理
  - `ui.rs` - 事件处理、流式状态和渲染逻辑
  - `approval.rs` - 工具审批对话框
  - `clipboard.rs` - 剪贴板处理
  - `streaming.rs` - 流式文本收集器

- **`ui.rs`** - 遗留/简单 UI 工具

### LSP 集成

- **`lsp/`** - 编辑后诊断注入（#136）
  - `mod.rs` - `LspManager` — 惰性每语言传输池 + 配置
  - `client.rs` - `StdioLspTransport` — 基于 stdio 的 JSON-RPC，带有 `didOpen`/`didChange`/`publishDiagnostics`
  - `diagnostics.rs` - 诊断类型、严重性和 HTML 块渲染器
  - `registry.rs` - 语言检测和默认服务器映射（rust-analyzer、pyright、gopls、clangd、typescript-language-server）
  - 通过 `core/engine/lsp_hooks.rs` 接入引擎 — 在每次成功编辑后调用

### 安全性

- **`sandbox/`** - 平台沙箱策略准备和拒绝报告
  - `mod.rs` - 沙箱类型定义
  - `policy.rs` - 沙箱策略配置
  - `seatbelt.rs` - macOS Seatbelt 配置文件生成
  - `landlock.rs` - Linux Landlock 检测和未来辅助合约
  - `windows.rs` - Windows 辅助合约；在 Job
    Object 进程包含辅助存在之前不做宣传

### 工具

- **`utils.rs`** - 通用工具
- **`logging.rs`** - 日志基础设施
- **`compaction.rs`** - 长对话的上下文压缩
- **`pricing.rs`** - 成本估算
- **`prompts.rs`** - 系统提示模板
- **`project_doc.rs`** - 项目文档处理
- **`session.rs`** - 会话序列化
- **`runtime_api.rs`** - HTTP/SSE 运行时 API（`deepseek serve --http`）
- **`runtime_threads.rs`** - 持久线程/回合/项存储 + 可重放事件时间线
- **`task_manager.rs`** - 持久队列、工作池、任务时间线和工件

## 数据流

### 交互式会话

1. TUI 接收用户输入
2. `core/engine.rs` 处理输入
3. 通过 `llm_client.rs` 发送消息到 LLM
4. 响应流式返回，在 `client.rs` 中解析
5. 通过 `tools/` 提取并执行工具调用
6. 工具执行前后触发钩子
7. 结果聚合并发送回 LLM
8. 最终响应在 TUI 中渲染

### 崩溃恢复 + 离线队列

1. 发送用户输入前，TUI 将检查点快照写入 `~/.deepseek/sessions/checkpoints/latest.json`
2. 启动时默认保持新鲜；先前的会话通过 `--resume`/`--continue`（或 TUI 中的 `Ctrl+R`）显式恢复
3. 降级/离线时，新提示在内存中排队并镜像到 `~/.deepseek/sessions/checkpoints/offline_queue.json`
4. 队列编辑（`/queue ...`）持续持久化，因此草稿和排队的提示在重启后仍然存在
5. 成功完成回合后清除活动检查点并写入持久会话快照
6. 代理/Yolo 回合还在 `~/.deepseek/snapshots/<project_hash>/<worktree_hash>/.git` 下获取回合前后的 side-git 工作区快照；`/restore N` 和 `revert_turn` 恢复文件状态而不改变对话历史或用户的 `.git`

### 工具执行

1. LLM 通过 `tool_use` 内容块请求工具
2. 工具注册表查找处理程序
3. 运行预执行钩子
4. 如果需要则请求审批（非 yolo 模式）
5. 工具执行（在 macOS 上可能被沙箱化）
6. 运行预执行钩子
7. 结果元数据保留在运行时项记录上
8. **LSP 编辑后钩子**（v0.8.6）：如果工具是 `edit_file`/`apply_patch`/`write_file` 且启用了 LSP，引擎运行 `run_post_edit_lsp_hook()` 收集诊断
9. **诊断刷新**（v0.8.6）：在下一个 API 请求之前，`flush_pending_lsp_diagnostics()` 将收集的错误作为合成用户消息注入
10. 结果返回给代理循环

### 后台任务

1. 客户端排队任务（`/task add ...` 或 `POST /v1/tasks`）
2. `task_manager.rs` 在 `~/.deepseek/tasks` 下持久化任务 + 队列条目
3. 工作器选择排队任务（有界池），转换为 `running`
4. 任务创建/使用运行时线程并启动运行时回合
5. `runtime_threads.rs` 持久化线程/回合/项记录 + 单调事件序列
6. 时间线/工具摘要/工件引用增量持久化
7. 检查列表状态、验证器门控、PR 尝试和受保护的 GitHub 事件从工具元数据应用到活动任务
8. 最终状态（`completed|failed|canceled`）是持久的，可通过 TUI/API 查询

模型可见的持久任务工具是此同一管理器的表面。它们不
引入并行工作系统：`task_create` 排队正常任务，
`checklist_*` 更新任务本地进度，`task_gate_run` 和已完成的
`task_shell_wait` 附加验证证据，自动化运行排队
普通持久任务。

### 运行时线程/回合时间线

1. API/TUI 创建或恢复线程（`/v1/threads*`）
2. 回合在线程上开始（`/v1/threads/{id}/turns`）
3. 引擎事件映射到项生命周期事件（`item.started|item.delta|item.completed`）
4. 中断/转向操作仅应用于活动回合
5. 压缩（自动/手动）作为 `context_compaction` 项生命周期发出
6. 客户端重放历史并通过 `/v1/threads/{id}/events?since_seq=<n>` 恢复

### 持久模式门

- `session_manager.rs`、`runtime_threads.rs` 和 `task_manager.rs` 在持久化记录上嵌入 `schema_version`。
- 加载时，较新的模式版本会被显式错误拒绝，而不是静默截断/覆盖数据。
- 这允许安全的前向迁移，并防止二进制文件和存储状态不同步时的损坏。

## 扩展点

### 添加新工具

1. 在 `tools/` 中创建处理程序
2. 在 `tools/registry.rs` 中注册
3. 添加工具规范（名称、描述、输入模式）

### 添加 MCP 服务器

1. 在 `~/.deepseek/mcp.json` 中配置
2. 启动时自动发现服务器
3. 工具自动暴露给 LLM

### 创建技能

1. 创建带有 `SKILL.md` 的技能目录
2. 定义技能提示和可选脚本
3. 放置在 `~/.deepseek/skills/` 中

### 添加钩子

在 `~/.deepseek/config.toml` 中配置：

```toml
[[hooks]]
event = "tool_call_before"
command = "echo '运行工具：$TOOL_NAME'"
```

## 关键设计决策

1. **流式优先**：所有 LLM 响应流式传输以提高响应速度
2. **工具安全**：非 YOLO 模式需要审批破坏性操作，包括有副作用的 MCP 工具
3. **可扩展性**：MCP、技能和钩子允许无需代码更改的自定义
4. **跨平台**：核心在 Linux/macOS/Windows 上工作。沙箱保证
   是平台特定的：macOS Seatbelt 是活动策略路径；Linux 和
   Windows 需要辅助执行才能被视为完整的操作系统
   沙箱。
5. **最小依赖**：谨慎选择依赖以加快构建速度
6. **本地优先运行时 API**：HTTP/SSE 端点旨在用于受信任的 localhost 访问，目前由 `crates/tui` 运行时提供服务

## 配置文件

- `~/.deepseek/config.toml` - 主要配置
- `/etc/deepseek/managed_config.toml` - 可选的托管默认层（Unix）
- `/etc/deepseek/requirements.toml` - 可选的允许策略约束（Unix）
- `~/.deepseek/mcp.json` - MCP 服务器配置
- `~/.deepseek/skills/` - 用户技能目录
- `~/.deepseek/sessions/` - 会话历史
- `~/.deepseek/sessions/checkpoints/` - 崩溃检查点 + 离线队列持久化
- `~/.deepseek/snapshots/` - 用于 `/restore` 和 `revert_turn` 的 side-git 回合前后工作区快照
- `~/.deepseek/tasks/` - 后台任务记录、队列、时间线、工件
- `~/.deepseek/audit.log` - 凭据 + 审批/提升操作的追加_only 审计事件
