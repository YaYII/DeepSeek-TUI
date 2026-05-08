# 竞品分析：DeepSeek TUI vs OpenCode vs Codex CLI

对三个 AI 编程智能体的能力分析：OpenCode（`/Volumes/VIXinSSD/opencode`）、Codex CLI（`/Volumes/VIXinSSD/codex-main`）和 DeepSeek TUI（`/Volumes/VIXinSSD/deepseek-tui`）。

## 工具矩阵

| 能力 | OpenCode | Codex CLI | DeepSeek TUI |
|---|---|---|---|
| 文件读取 | ✅ Read | ✅ | ✅ file |
| 文件写入 | ✅ Write | ✅ | ✅ file |
| 文件编辑 | ✅ Edit（字符串替换） | ✅ apply_patch（diff 格式） | ✅ edit_file + apply_patch |
| 文件搜索 | ✅ Glob | ✅ | ✅ file_search |
| 代码搜索 | ✅ Grep + CodeSearch（Exa） | ✅ | ✅ grep_files + search |
| Shell 执行 | ✅ Bash | ✅ exec/shell | ✅ shell |
| Web 抓取 | ✅ WebFetch | ✅ | ✅ fetch_url |
| Web 搜索 | ✅ WebSearch | ✅ WebSearchRequest | ✅ web_search |
| Web 浏览 | ❌ | ❌ | ✅ web_run |
| LSP | ✅ Lsp（实验性） | ❌ | ✅ 编辑后诊断（自动） |
| 任务/待办跟踪 | ✅ TodoWrite | ✅ | ✅ todo_write |
| 子代理生成 | ✅ Task | ✅ Collab/SpawnCsv | ✅ agent_spawn |
| 技能系统 | ✅ Skill（多位置发现） | ✅ core-skills | ⚠️ 部分（.deepseek/skills/） |
| 计划模式 | ✅ plan-enter/exit | ✅ Plan 模式 | ✅ Plan 模式 |
| 用户提问 | ✅ Question | ✅ request_user_input | ✅ user_input |
| 补丁应用 | ✅ apply_patch（自定义格式） | ✅ apply_patch（diff 格式） | ✅ apply_patch |
| 数据验证 | ❌ | ❌ | ✅ validate_data |
| 金融数据 | ❌ | ❌ | ✅ finance |
| Git 操作 | 通过 Bash 工具 | ✅ git-utils | ✅ git 模块 |
| GitHub 操作 | 通过 Bash（gh） | ✅ | ✅ github |
| 测试运行 | ❌ | ✅ | ✅ test_runner |
| 自动化 | ❌ | ❌ | ✅ automation |
| 代码审查 | ❌ | ✅ GuardianApproval | ✅ review |
| 回溯/归档 | ❌ | ❌ | ✅ recall_archive |
| 诊断 | ❌ | ✅ | ✅ diagnostics |
| 轮次回滚 | ❌ | ❌ | ✅ revert_turn |
| 图像生成 | ❌ | ✅ ImageGeneration | ❌ |
| 浏览器使用 | ❌ | ✅ BrowserUse | ❌（web_run 是无头） |
| 计算机使用 | ❌ | ✅ ComputerUse | ❌ |
| 实时语音 | ❌ | ✅ RealtimeConversation | ❌ |

---

## 高优先级差距

这些是能最直接提高 DeepSeek TUI 作为编程智能体有效性的能力。

### 1. LSP 集成 — ✅ 已实施（编辑后诊断）

**状态：** 在 `crates/tui/src/lsp/` + `crates/tui/src/core/engine/lsp_hooks.rs` 中实施。作为自动编辑后诊断注入已发布。

**DeepSeek TUI 拥有：**

- **编辑后诊断钩子：** 每次成功的 `edit_file`、`write_file` 或 `apply_patch` 后，引擎自动从相应的 LSP 服务器请求诊断，并将编译器错误作为合成消息注入到模型的上下文中。
- **自定义 JSON-RPC stdio 客户端**（`client.rs`）：实现了 LSP 线协议，没有 `tower-lsp` 依赖。将 LSP 服务器作为子进程生成，处理 `Content-Length` 帧，路由 `publishDiagnostics` 通知。
- **语言注册表**（`registry.rs`）：从文件扩展名检测语言并映射到内置默认值：
  - Rust → `rust-analyzer`
  - Go → `gopls serve`
  - Python → `pyright-langserver --stdio`
  - TypeScript/JavaScript → `typescript-language-server --stdio`
  - C/C++ → `clangd`
- **可配置**：通过 `~/.deepseek/config.toml` 中的 `[lsp]` 表：`enabled`、`poll_after_edit_ms`（默认 5000）、`max_diagnostics_per_file`（默认 20）、`include_warnings`（默认 false），以及每种语言的 `[lsp.servers]` 覆盖。
- **非阻塞设计：** 缺少 LSP 二进制、服务器崩溃或超时会静默降级为"本轮无诊断。"服务器在每种语言首次编辑时惰性生成。
- **测试基础设施：** 用于无需真实 LSP 服务器的 CI 测试的 `FakeTransport` 接缝。

**与 OpenCode 的剩余差距：** OpenCode 将 LSP 暴露为**模型可调用工具**，具有 9 个操作（goToDefinition、findReferences、hover、documentSymbol、workspaceSymbol、goToImplementation、prepareCallHierarchy、incomingCalls、outgoingCalls）。DeepSeek TUI 的 LSP 目前是被动的（编辑后自动触发），而非主动的（模型可按需查询导航）。

**DeepSeek TUI 仍可以添加的：**

一个模型可调用的 `lsp` 工具，在 `crates/tui/src/tools/` 中暴露交互式 LSP 操作（goToDefinition、findReferences、hover、documentSymbol、workspaceSymbol）。传输基础设施已经存在——差距仅在于工具包装器和 LSP 方法（超出 `didOpen`/`didChange`/`publishDiagnostics` 的）请求/响应周期。

### 2. 细粒度权限系统

**是什么：** 基于工具名称 × 文件路径模式的允许/拒绝/询问规则，支持通配符、主目录扩展以及级联到待处理请求。

**为什么重要：** 当前全有或全无的审批模型造成了摩擦。用户无法表达"始终允许读取 `src/` 但始终询问 `.env` 文件。"在长时间会话中，永久批准一个模式的能力可以将审批疲劳减少 60-80%。

**OpenCode 的实现：** `packages/opencode/src/permission/index.ts` 实现：

- `Action`：`allow | deny | ask`
- `Rule`：`{ permission: string, pattern: string, action: Action }`
- `Ruleset`：具有最后匹配获胜语义的有序规则列表
- 针对 `~/`、`$HOME/` 的模式扩展
- 权限名称和路径模式上的通配符匹配
- 回复模式：`once`（批准这一次调用）、`always`（永久批准该模式）、`reject`（拒绝这一次）
- 自动级联：一个"always"回复自动解决同一会话的待处理请求
- 不同的错误类型：`DeniedError`（基于规则）、`RejectedError`（用户说否）、`CorrectedError`（用户说否并附反馈）

代理定义继承可被用户覆盖的权限规则集：

```typescript
build: {
  permission: merge(defaults, { question: "allow", plan_enter: "allow" }, user),
}
plan: {
  permission: merge(defaults, { edit: { "*": "deny" } }, user),
}
explore: {
  permission: merge(defaults, { "*": "deny", grep: "allow", read: "allow", ... }, user),
}
```

**DeepSeek TUI 需要什么：** 一个具有相同维度（工具名称 × 路径模式 × 动作）、持久化到磁盘以及钩子集成（使审批决策可以级联）的权限规则引擎。

### 3. 生命周期钩子

**是什么：** 用户定义的 shell 命令或插件函数，在特定生命周期事件上触发——工具执行前、完成后、请求权限时、会话启动时、用户提交提示时和会话停止时。

**为什么重要：** 钩子是让用户能够在不污染系统提示的情况下强制实施不变性的逃生舱。"写入 `.rs` 文件后始终运行 `cargo fmt`。""在任何 `rm -rf` 之前警告我。""将每个 shell 命令记录到文件。"它们是可组合、可审计的，并且不消耗上下文窗口令牌。

**Codex CLI 的实现：** `codex-rs/hooks/` 定义了六个具有类型化请求/响应负载的事件类型：

| 事件 | 何时触发 | 负载 |
|---|---|---|
| `PreToolUse` | 工具执行前 | 工具名称、输入参数、沙箱状态 |
| `PostToolUse` | 工具执行后 | 工具名称、输入、成功/失败、持续时间、输出预览 |
| `PermissionRequest` | 模型请求权限时 | 权限类型、理由 |
| `SessionStart` | 新会话开始时 | 会话 ID、cwd、来源（新建/恢复） |
| `UserPromptSubmit` | 用户发送消息时 | 提示文本 |
| `Stop` | 会话结束时 | 原因 |

每个钩子处理器支持：
- `matcher`：可选的正则表达式，用于过滤哪些工具调用触发钩子
- `command`：要运行的 shell 命令
- `timeout_sec`：最大运行时间
- `status_message`：钩子运行时向用户显示的消息
- `source_path` + `source`：跟踪钩子定义位置（项目 hooks.json、用户配置、插件）
- 钩子可以返回 `Success`、`FailedContinue` 或 `FailedAbort`（阻塞操作）

**DeepSeek TUI 需要什么：** 扩展 `crates/hooks/` 以支持完整的事件表面、添加基于匹配器的过滤，并提供类似于 Codex CLI 的 `hooks.json` 发现机制。

### 4. 持久化记忆

**是什么：** 从对话中自动提取用户偏好、项目约定和过去的决策，作为可检索的记忆存储，这些记忆被注入新会话中。

**为什么重要：** 在一个长时间的调试会话中，代理重新发现相同的事实："这个项目使用 Rust edition 2024，" "测试使用 `cargo test --workspace` 运行，" "用户更喜欢 4 空格缩进。"记忆系统使价值复合——每个会话建立在前知识的基础上，而不是从零开始。

**Codex CLI 的实现：** `MemoryTool` 功能（实验性，在 `/experimental` 菜单后面）启用：
- 记忆生成：模型从对话内容创建结构化记忆
- 记忆检索：相关的记忆被注入新的对话上下文
- `Chronicle` 功能通过一个辅助进程添加被动的屏幕上下文记忆
- 记忆存储在 SQLite 中，通过 `/memories` 命令在 TUI 中展示

**DeepSeek TUI 需要什么：** 一个记忆提取提示、一个基于向量或关键词的检索系统，以及现有会话/状态基础设施中的存储。

### 5. 技能自动发现

**是什么：** 自动扫描多个位置以查找提供领域特定指令、脚本和引用的 `SKILL.md` 文件。技能通过 `skill` 工具按需注入对话中。

**为什么重要：** 技能是社区打包专业知识的方式。一个"Rust 重构"技能、一个"Docker 部署"技能、一个"GitHub Actions"技能——每个都提供专门的指令，而不膨胀主系统提示。OpenCode 的多位置发现意味着技能可以是项目本地、用户全局或从 URL 拉取的。

**OpenCode 的实现：** `packages/opencode/src/skill/index.ts` 扫描：

1. `~/.claude/skills/**/SKILL.md`（Claude Code 兼容性）
2. `~/.agents/skills/**/SKILL.md`（Agents SDK 兼容性）
3. 从 cwd 到工作区根目录的父目录中的 `.claude/skills/` 和 `.agents/skills/`
4. 项目配置目录中的 `{skill,skills}/**/SKILL.md`
5. 用户配置的路径（支持 `~/` 扩展）
6. 用户配置的 URL（通过发现模块拉取）

技能被解析为 YAML frontmatter（`name`、`description`）和 Markdown 内容。重复名称会警告但不报错。技能遵守代理权限——代理只能加载其权限规则集允许的技能。

**DeepSeek TUI 需要什么：** 扩展现有的 `~/.deepseek/skills/` 发现机制，支持父目录遍历、Claude Code 兼容路径和基于 URL 的技能源。添加 YAML frontmatter 解析。

---

## 中等优先级差距

这些会显著改善代理体验，但不太紧迫。

### 6. 代理配置文件（profile）与权限继承

**是什么：** 命名代理类型（build、plan、general、explore）继承不同的工具权限集。用户可以定义具有特定模型、温度、系统提示和权限规则的自定义代理。

**OpenCode 的实现：** `packages/opencode/src/agent/agent.ts`：

- `build`：完全访问，敏感路径上询问
- `plan`：所有编辑工具被拒绝，允许 plan-exit，允许在 `.opencode/plans/` 中写入计划文件
- `general`：仅子代理，拒绝 todo-write
- `explore`：只读，允许 grep/glob/read/bash/webfetch/websearch
- 加上用于内部任务（压缩、标题生成、总结）的隐藏代理

每个代理携带自己的 `model`、`temperature`、`topP`、`prompt` 和 `permission` 规则集。一个 `generate` 函数从用户描述动态创建新的代理配置。

**DeepSeek TUI 需要什么：** 扩展模式系统（Plan/Agent/YOLO）以支持命名代理配置文件，每个配置文件具有按配置文件过滤的工具和模型配置。

### 7. Shell 沙箱

**是什么：** shell 命令的操作系统级沙箱强制——网络限制、文件系统只读挂载、允许/禁止的路径。

**Codex CLI 的实现：** `codex-rs/sandboxing/`：

- macOS：Seatbelt（`sandboxing/src/seatbelt.rs`）带 `.sbpl` 策略文件
- Linux：bubblewrap（默认）或 Landlock（遗留回退）
- Windows：受限令牌
- 每个命令可配置的沙箱策略
- 集成测试可以检测它们是否在沙箱下运行并提前退出

**DeepSeek TUI 需要什么：** 扩展 `crates/execpolicy/` 以支持平台特定的沙箱强制。从 macOS Seatbelt 开始（大多数 DeepSeek TUI 用户在 macOS 上）。

### 8. 工具搜索 / 延迟 MCP 工具暴露

**是什么：** 不将所有 MCP 工具倾倒在系统提示中（膨胀上下文），而是暴露一个 `tool_search` 函数，模型调用它通过名称或描述发现相关工具。

**Codex CLI 的实现：** `ToolSearch` 功能（稳定，默认启用）。`ToolSearchAlwaysDeferMcpTools` 更进一步——从不直接暴露 MCP 工具，始终需要搜索。当 MCP 服务器暴露数百个工具时，这至关重要。

**DeepSeek TUI 需要什么：** `tool_search_tool_regex` 和 `tool_search_tool_bm25` 已经作为延迟工具发现机制存在。扩展它们以将 MCP 工具暴露限制在按需搜索之后。

### 9. 执行策略 / 命令审批规则

**是什么：** 一个策略引擎，根据用户定义的规则评估 shell 命令——前缀允许列表、网络限制、模式匹配——并自动批准、拒绝或升级。

**Codex CLI 的实现：** `codex-rs/execpolicy/src/`：

- `Policy`：有序的 `Rule` 条目列表
- `Rule`：前缀模式（例如，允许 `cargo build*`，拒绝 `rm *`）
- `NetworkRule`：协议级别的网络限制
- `MatchOptions`：控制规则评估行为
- `Evaluation`：针对命令的策略评估结果

规则可以在运行时通过 `blocking_append_allow_prefix_rule` 修改。

**DeepSeek TUI 需要什么：** 扩展 `crates/execpolicy/` 以支持前缀规则、网络规则和运行时策略修改。

### 10. 动态代理生成

**是什么：** 从自然语言描述即时生成新的代理配置。

**OpenCode 的实现：** `agent.ts` 中的 `generate` 函数接收一个描述，如 "只读取文件和报告问题的代码审查者"，并使用结构化 LLM 调用返回一个 `{ identifier, whenToUse, systemPrompt }` 对象。生成的代理尊重现有的代理名称冲突。

**DeepSeek TUI 需要什么：** 一个模型可调用的工具或斜杠命令，从描述生成代理配置并在会话中注册它们。

### 11. 流式补丁事件

**是什么：** 在模型生成 `apply_patch` 输入时流式传输结构化进度事件，让用户实时了解哪些文件将发生变化。

**Codex CLI 的实现：** `ApplyPatchStreamingEvents` 功能（开发中）在模型生成补丁块时流式传输文件级进度。`apply-patch/src/streaming_parser.rs` 中的 `StreamingPatchParser` 处理增量解析。

**DeepSeek TUI 需要什么：** 扩展 `apply_patch.rs` 以在流式模型输出期间发出进度事件。

---

## 较低优先级差距

专业功能，对核心编码工作流有价值但不太关键。

| 能力 | 来源 | 备注 |
|---|---|---|
| 图像生成 | Codex CLI `ImageGeneration` | 对编码来说是个 niche；对文档图表有用 |
| 浏览器使用 | Codex CLI `BrowserUse` | 交互式浏览器自动化（点击、输入、截图）。DeepSeek TUI 有 `web_run` 用于无头操作 |
| 计算机使用 | Codex CLI `ComputerUse` | 完整的桌面自动化。受桌面应用门控 |
| 实时语音 | Codex CLI `RealtimeConversation` | 语音对话模式。实验性 |
| 统一 PTY 执行 | Codex CLI `UnifiedExec` | 单一 PTY 支持的 shell，跨轮次的状态快照 |
| 工件（Artifacts） | Codex CLI `Artifact` | 原生工件渲染工具 |
| 目标（Goals） | Codex CLI `Goals` | 持久化线程目标，在压缩和会话重启后仍然存在 |
| Git 提交归属 | Codex CLI `CodexGitCommit` | 正确提交归属的模型指令 |
| CSV 代理生成 | Codex CLI `SpawnCsv` | CSV 支持的并行代理作业分发 |
| Shell 快照 | Codex CLI `ShellSnapshot` | 跨轮次保存/恢复 shell 状态 |
| 防止空闲休眠 | Codex CLI `PreventIdleSleep` | 在长时间运行的代理任务期间保持机器唤醒 |

---

## 架构模式

### OpenCode

**客户端/服务器架构：** TUI 是一个客户端；服务器可以从移动应用、桌面应用或 Web 控制台远程驱动。这使代理运行时与 UI 层解耦。

**插件系统：** `packages/opencode/src/plugin/` 支持热加载的 JS/TS 插件，可以添加工具、模型、认证提供者和聊天中间件。插件接收带有工具执行、认证和文件系统访问的类型化上下文。

**多提供者：** 不耦合于任何单一 AI 提供者。模型用提供者 ID 配置，并通过提供者注册表解析。`plugin/codex.ts` 中支持 OpenAI Codex 的 OAuth 认证（ChatGPT 订阅集成）。

**配置分层：** 配置从多个源（全局、项目、环境变量）加载，并以明确定义的优先级合并。

### Codex CLI

**应用-服务器协议：** `codex-rs/app-server-protocol/` 定义了 TUI 前端和代理后端之间的版本化 RPC 协议（v2）。所有新的 API 开发通过 v2 进行，具有严格命名约定（`*Params`/`*Response`/`*Notification`、`resource/method` RPC 命名）。

**功能标志系统：** `codex-rs/features/` 集中管理 60 多个功能标志，具有生命周期阶段（UnderDevelopment、Experimental、Stable、Deprecated、Removed）。功能有元数据（菜单名称、描述、公告文本）并可以携带自定义配置结构体。

**Bazel + Cargo 双构建：** Codex CLI 同时使用 Cargo（用于开发）和 Bazel（用于 CI/发布）。`find_resource!` 宏和 `cargo_bin()` 辅助函数抽象了运行文件差异。

**快照测试：** `codex-rs/tui/` 广泛使用 `insta` 进行 UI 快照测试。任何 UI 更改都需要相应的快照覆盖。

**核心模块化：** 明确抵制向 `codex-core` 添加代码。新功能进入专门构建的 crate（`codex-apply-patch`、`codex-memories`、`codex-sandboxing`），而不是增长核心 crate。

### DeepSeek TUI

**RLM（递归语言模型）：** 在此领域独一无二。一个沙箱化的 Python REPL，子 LLM 可以在其中调用辅助函数（`llm_query`、`llm_query_batched`、`rlm_query`）进行批处理、分块和递归批判。两个竞争对手都没有等效的功能。

**持久化任务：** 可重启感知的持久化任务对象，带有证据跟踪（门禁运行、PR 尝试、时间线）。设计用于长时间运行的自主工作，可以承受重启。

**自动化：** 具有 cron 风格 RRULE 重复的定时重复任务。在这三者中独一无二。

---

## DeepSeek TUI 已经擅长的方面

- **LSP 诊断** — 编辑后自动的编译器/linter 反馈注入到模型上下文中；两个竞争对手都没有被动的 LSP 集成（OpenCode 的仅是可模型调用的）
- **RLM** — Python 沙箱中的批量/批量 LLM 处理；两个竞争对手都没有等效功能
- **金融数据** — 实时股票/加密货币报价；在此领域独一无二
- **自动化** — 具有 cron 规则的定时重复任务
- **持久化任务** — 可重启感知，带有证据跟踪和门禁验证
- **轮次回滚** — 通过 side-git 快照每个轮次撤销工作区更改
- **数据验证** — JSON/TOML 验证工具
- **Web 运行** — 无头浏览器交互（Codex CLI 有 Browser Use 但受桌面应用门控）
- **并行工具执行** — 显式建模为基础架构
- **Git/GitHub 操作** — 全面的 git 模块，具有 blame、log、diff、status 加上通过 gh 的完整 GitHub API
- **项目映射** — 高级项目结构生成

---

## 推荐的实施顺序

1. ~~**LSP 工具**~~ — ✅ **已完成**（编辑后诊断）。剩余：模型可调用的导航工具。
2. **路径模式权限** — 在长时间会话中将审批疲劳减少 60-80%。
3. **持久化记忆** — 跨会话复合价值；对于长期运行的项目至关重要。
4. **工具前后钩子** — 用户定义护栏的逃生舱，无需膨胀系统提示。
5. **技能自动发现** — 启用社区技能生态系统和 Claude Code 兼容性。
6. **LSP 导航工具** — 将 goToDefinition/findReferences/hover 暴露为模型可调用工具。基础设施已存在；添加请求/响应方法 + 工具包装器。
7. **代理配置文件（profile）** — 具有模型/权限继承的命名代理类型。
8. **MCP 工具搜索** — 在连接到具有许多工具的 MCP 服务器时保持上下文窗口可管理。
9. **Shell 沙箱** — 安全性改进，从 macOS Seatbelt 开始。