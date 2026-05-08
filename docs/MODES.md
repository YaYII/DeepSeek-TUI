# 模式与审批

DeepSeek TUI 有两个相关概念：

- **TUI 模式**：你处于何种可见交互类型（Plan/Agent/YOLO）。
- **审批模式**：UI 在执行工具前询问的积极程度。

## TUI 模式

按 `Tab` 可以补全编辑器菜单、在轮次运行中将草稿排队为下一轮跟进，或在编辑器空闲时循环切换可见模式：**Plan → Agent → YOLO → Plan**。
按 `Shift+Tab` 循环切换推理努力级别。

- **Plan**：设计优先的提示方式。只读调查工具保持可用；shell 和补丁执行保持关闭。当你希望边思考边输出，并将计划交给人类（之后的你自己，或审查者）时使用此模式。
- **Agent**：多步骤工具使用。Shell 和付费工具需要审批（文件写入无需提示即可执行）。
- **YOLO**：启用 shell + 信任模式并自动批准所有工具。仅在受信任的仓库中使用。

所有三种模式都可以使用 `rlm` 工具。在其 Python REPL 内部，`llm_query_batched` 可以扇出 1–16 个廉价的并行子调用，固定使用 `deepseek-v4-flash`。当工作可分解时，模型会使用此工具。

## 兼容性说明

- `/normal` 是一个隐藏的兼容性别名，会切换到 `Agent`。
- 带有 `default_mode = "normal"` 的旧配置文件仍然会作为 `agent` 加载；保存时会重写为标准化值。

## Escape 键行为

`Esc` 是一个取消堆栈，而非模式开关。

- 首先关闭斜杠菜单或瞬态 UI。
- 如果轮次正在运行，取消当前请求。
- 如果编辑器为空，丢弃已排队的草稿。
- 如果存在文本，清除当前输入。
- 否则为无操作。

## 审批模式

你可以在运行时覆盖审批行为：

```text
/config
# 将 approval_mode 行编辑为：suggest | auto | never
```

历史说明：`/set approval_mode ...` 已被弃用，改用 `/config`。

- `suggest`（默认）：使用上述的按模式规则。
- `auto`：自动批准所有工具（类似于 YOLO 的审批行为，但不强制进入 YOLO 模式）。
- `never`：阻止任何不被视为安全/只读的工具。

## 小屏幕状态行为

当终端高度受限时，状态区域会首先压缩，以便头部/聊天/编辑器/底部栏保持可见：

- 加载中和排队中的状态行会根据可用高度进行预算。
- 当完整预览无法容纳时，排队预览会折叠为紧凑摘要。
- `/queue` 工作流仍然可用；紧凑状态仅影响渲染密度。

## 工作区边界与信任模式

默认情况下，文件工具被限制在 `--workspace` 目录内。启用信任模式以允许文件访问工作区之外：

```text
/trust
```

YOLO 模式会自动启用信任模式。

## MCP 行为

MCP 工具以 `mcp_<server>_<tool>` 形式暴露，并使用与内置工具相同的审批流程。只读的 MCP 辅助工具在建议性审批模式下可能会自动运行；可能有副作用的 MCP 工具需要审批。

参见 `MCP.md`。

## 相关 CLI 标志

运行 `deepseek --help` 获取权威列表。常用标志：

- `-p, --prompt <TEXT>`：一次性提示模式（打印并退出）
- `--model <MODEL>`：使用 `deepseek` 外观时，向 TUI 转发 DeepSeek 模型覆盖
- `--workspace <DIR>`：文件工具的工作区根目录
- `--yolo`：以 YOLO 模式启动
- `-r, --resume <ID|PREFIX|latest>`：恢复已保存的会话
- `-c, --continue`：恢复此工作区中最近的会话
- `--max-subagents <N>`：限制为 `1..=20`
- `--mouse-capture` / `--no-mouse-capture`：选择启用或禁用内部鼠标滚动、转录本选择和右键点击上下文操作。鼠标捕获在非 Windows 终端上默认启用，因此拖动选择仅复制用户/助手转录文本；拖动时按住 Shift 或使用 `--no-mouse-capture` 进行原始终端选择。在 Windows（CMD/终端鼠标转义序列泛滥）和 JetBrains JediTerm 内部——PyCharm/IDEA/CLion 等——默认关闭，因为终端宣称支持鼠标但将 SGR 鼠标事件作为原始文本转发（#878, #898）。在任何默认关闭的地方使用 `--mouse-capture` 选择启用。
- `--profile <NAME>`：选择配置档案
- `--config <PATH>`：配置文件路径
- `-v, --verbose`：详细日志输出