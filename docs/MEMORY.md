# 用户记忆

用户记忆（user-memory）功能为模型提供一个小的持久化笔记文件，该文件在每次对话轮次中被注入到系统提示词中。它是存放偏好和约定的地方，这些内容应跨会话保留——例如"我更喜欢 pytest 而非 unittest"、"此代码库使用 4 空格缩进"、"提交前始终运行 `cargo fmt`"——而无需在每个对话中重复。

记忆功能是**主动加入（opt-in）**的。当禁用时（默认状态），不会加载任何内容，不会拦截任何操作，`remember` 工具也不会暴露给模型。这为未启用该功能的用户保持零开销行为。

## 启用记忆

设置环境变量：

```bash
export DEEPSEEK_MEMORY=on
```

接受的真值包括 `1`、`on`、`true`、`yes`、`y` 和 `enabled`。

……或添加到 `~/.deepseek/config.toml`：

```toml
[memory]
enabled = true
```

切换后重启 TUI。禁用的方式与之相反。

默认情况下，记忆文件位于 `~/.deepseek/memory.md`；可通过 `config.toml` 中的 `memory_path` 或环境变量 `DEEPSEEK_MEMORY_PATH` 覆盖。当两者都设置时，`DEEPSEEK_MEMORY_PATH` 优先于配置文件。

## 快速示例

```text
# 记住：此仓库在提交前更喜欢 cargo fmt
/memory
/memory path
/memory edit
/memory help
```

- 在输入框中键入 `# 记住：此仓库在提交前更喜欢 cargo fmt`，即可添加一条带时间戳的条目，而无需触发一次对话轮次。
- 运行 `/memory` 确认功能当前写入的位置以及已存储的内容。
- 当你想手动在编辑器中整理文件时，运行 `/memory edit`。

## 注入内容

当记忆功能启用且文件存在时，每次对话轮次的系统提示词会携带一个额外的块：

```xml
<user_memory source="/Users/you/.deepseek/memory.md">
- (2026-05-03 22:14 UTC) prefer pytest over unittest
- (2026-05-03 22:31 UTC) this codebase uses 4-space indentation
…
</user_memory>
```

该块位于提示词组装中的易变内容边界之上，因此它能保持在 DeepSeek 的跨轮次前缀缓存中。文件在每个提示词构建调用时被读取——通过 `/memory` 或外部编辑器进行的编辑会在下一轮次生效，无需重启。

大于 100 KiB 的文件会被加载但被截断，并追加一个标记以便你看到截断位置。

## 三种添加到记忆的方式

### 1. `# ` 输入框前缀 (#492)

在输入框中键入以 `#` 开头（但不是 `##` 或 `#!`）的单行内容：

```
# 记住在此仓库中使用 4 空格缩进
```

TUI 会拦截该输入，并在你的记忆文件中追加一条带时间戳的条目。**不会触发对话轮次**——你的输入被消耗，状态行会确认写入的路径，你可以继续键入真正的问题。

多 `#` 前缀会故意放行，正常提交对话轮次，这样你可以粘贴 Markdown 标题而不会感到意外。

### 2. `/memory` 斜杠命令 (#491)

检查、清除或获取编辑文件的提示：

| 子命令              | 效果                                                 |
|---------------------|------------------------------------------------------|
| `/memory`           | 显示解析后的路径和当前内容（内联）                    |
| `/memory show`      | 无参数形式的别名                                      |
| `/memory path`      | 仅打印解析后的路径                                    |
| `/memory clear`     | 将文件替换为空标记                                    |
| `/memory edit`      | 打印 `${VISUAL:-${EDITOR:-vi}} <path>` shell 命令行   |
| `/memory help`      | 显示命令特定的帮助信息和当前路径                      |

`/memory edit` 形式特意只打印命令，而不是在进程中启动编辑器——这使斜杠命令处理程序保持简单且一致，无论你使用哪个编辑器。

你也可以从通用帮助界面发现此功能：

- `/help memory` 显示斜杠命令摘要和用法行。
- `/memory help` 打印记忆特定的子命令以及解析后的路径。

### 3. `remember` 工具（自动更新，#489）

当记忆启用时，模型会获得一个 `remember` 工具，其形式如下：

```json
{
  "name": "remember",
  "description": "向用户记忆文件追加一条持久化的笔记...",
  "input_schema": {
    "type": "object",
    "properties": {
      "note": { "type": "string", ... }
    },
    "required": ["note"]
  }
}
```

当模型注意到一个值得跨会话保留的持久偏好、约定或事实时，它会使用此工具。该工具是自动批准的，因为写入操作仅限于用户自己的记忆文件——将其置于标准写入批准流程之后会违背自动记忆捕获的目的。

如果模型将 `remember` 用于临时任务状态（"我正在编辑 foo.rs"），结果是无害的，但会浪费上下文。该工具的描述明确告诉模型**不要**这样做——仅限持久的、单句式的笔记。

## 文件格式

记忆是带有时间戳条目的纯 Markdown：

```markdown
- (2026-05-03 22:14 UTC) prefer pytest over unittest
- (2026-05-03 22:31 UTC) this codebase uses 4-space indentation
- (2026-05-04 09:02 UTC) all PRs need 2 reviewers before merge
```

你可以在任何编辑器中手动编辑该文件——加载器不关心时间戳的格式；它只是将整个文件作为记忆块读取。时间戳是一种约定，这样你在整理文件时能知道每条笔记的添加时间。

## 层次结构与导入

记忆是有意设计为**用户范围**而非仓库范围的。它与项目指令源（如 `AGENTS.md`、`.deepseek/instructions.md` 和 `instructions = [...]`）并列存在，而不是包含在其中。

- 使用**记忆**来存放应随你跨仓库和会话的持久个人偏好。
- 使用**项目指令**来存放应随代码库一起传播的仓库特定约定。

记忆加载器目前按原样读取一个已解析的文件路径。`@path` 导入/包含目前**不**支持；如果你需要更大的可复用指令包，请将其放在项目指令文件或技能（skill）中。

## 不应放入记忆的内容

记忆用于存放**持久**信号。以下内容**不**应放在其中：

- **秘密信息**——没有 API 密钥、令牌、密码。该文件是磁盘上的纯文本，会按原样注入到系统提示词中。
- **临时任务状态**——"我正在处理解析器"每次会话都会变化；它不属于跨会话记忆。
- **对话片段**——引用风格的笔记属于笔记工具（`note`），而非记忆。
- **长指令**——任何超过几句话的内容应放在 `AGENTS.md`（项目级别）或 [skill](../crates/tui/src/skills/mod.rs)（可复用的指令包）中。

## 隐私与范围

记忆文件完全保存在你的机器上，位于 `~/.deepseek/`。它永远不会上传到任何云服务——TUI 仅将其内联包含在发送给 LLM 提供商的系统提示词中，且仅在记忆启用时如此。如果你切换提供商（DeepSeek / NVIDIA NIM / Fireworks 等），会使用同一个记忆文件；该文件与提供商无关。

该文件是每个用户的，而非每个项目的。如果你想要项目特定的记忆，请改用项目级别的 `AGENTS.md` 或 `.deepseek/instructions.md` 文件——这些文件由 `project_context` 加载，并存在于仓库中（或你提交它们的任何地方）。

## 配置参考

```toml
# ~/.deepseek/config.toml
[memory]
enabled = true                    # 默认为 false；或设置 DEEPSEEK_MEMORY=on
# 路径在顶级配置（与 skills_dir、notes_path 同级）：
memory_path = "~/.deepseek/memory.md"
```

| 设置               | 默认值                       | 覆盖方式                              |
|-------------------|-------------------------------|---------------------------------------|
| 记忆启用           | `false`                       | `[memory] enabled = true` 或 `DEEPSEEK_MEMORY=on` |
| 记忆文件路径       | `~/.deepseek/memory.md`       | `memory_path = "..."` 或 `DEEPSEEK_MEMORY_PATH=`  |
| 最大文件大小       | 100 KiB                       | （目前无；截断标记会显示截断位置）     |

## 相关文档

- `docs/SUBAGENTS.md` — 子代理继承记忆，也可以使用 `remember` 工具。
- `docs/CONFIGURATION.md` — 完整配置参考。
- Issue [#489](https://github.com/Hmbown/DeepSeek-TUI/issues/489) — 阶段-1 EPIC 跟踪此工作。
