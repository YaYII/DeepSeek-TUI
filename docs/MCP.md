# MCP（外部工具服务器）

DeepSeek TUI 可以通过 MCP（模型上下文协议，Model Context Protocol）加载额外工具。MCP 服务器是本地进程，TUI 会启动这些进程并通过 stdio 与之通信。

浏览说明：
- `web.run` 是规范的內建浏览工具。
- `web_search` 作为兼容性别名保留，用于支持较旧的提示和集成。

服务端模式说明：
- `deepseek-tui serve --mcp` 运行 MCP stdio 服务器。
- `deepseek-tui serve --http` 运行运行时 HTTP/SSE API（独立模式）。
- `deepseek` 调度器将 `deepseek mcp-server` 暴露为等价的 stdio 入口点，供拆分 CLI 使用。

## 初始 MCP 配置

在你解析后的 MCP 路径下创建一个初始 MCP 配置：

```bash
deepseek-tui mcp init
```

`deepseek-tui setup --mcp` 在完成技能初始化的同时执行相同的 MCP 引导设置。

常用的管理命令：

```bash
deepseek-tui mcp list
deepseek-tui mcp tools [server]
deepseek-tui mcp add <name> --command "<cmd>" --arg "<arg>"
deepseek-tui mcp add <name> --url "http://localhost:3000/mcp"
deepseek-tui mcp enable <name>
deepseek-tui mcp disable <name>
deepseek-tui mcp remove <name>
deepseek-tui mcp validate
```

## TUI 内管理器

在交互式 TUI 中，`/mcp` 会打开一个针对已解析 MCP 配置路径的紧凑管理器。它会显示每个已配置的服务器、其启用或禁用状态、传输方式、命令或 URL、超时值、连接错误，以及在运行过服务发现后找到的工具/资源/提示。

TUI 内支持的操作：

```text
/mcp init
/mcp init --force
/mcp add stdio <name> <command> [args...]
/mcp add http <name> <url>
/mcp enable <name>
/mcp disable <name>
/mcp remove <name>
/mcp validate
/mcp reload
```

`/mcp validate` 和 `/mcp reload` 会重新连接以进行 UI 发现并刷新管理器快照。从 TUI 进行的配置编辑会立即写入，但模型可见的 MCP 工具池不会热重载；管理器会将其标记为需要重启，直到 TUI 重启为止。

## 配置文件位置

默认路径：

- `~/.deepseek/mcp.json`

覆盖方式：

- 配置项：`mcp_config_path = "/path/to/mcp.json"`
- 环境变量：`DEEPSEEK_MCP_CONFIG=/path/to/mcp.json`

`deepseek-tui mcp init`（以及 `deepseek-tui setup --mcp`）会写入此解析后的路径。

交互式 `/config` 编辑器也暴露了 `mcp_config_path` 配置项。在 TUI 中更改它会更新 `/mcp` 所使用的路径，并且需要重启后才能重建模型可见的 MCP 工具池。

编辑文件或更改 `mcp_config_path` 后，请重启 TUI。

## 工具命名

发现到的 MCP 工具会按以下命名方式暴露给模型：

- `mcp_<server>_<tool>`

示例：一个名为 `git` 的服务器，其中包含名为 `status` 的工具，将变为 `mcp_git_status`。

命令面板会按服务器分组的 MCP 条目。它会显示已禁用和出现故障的服务器，而不是隐藏它们，并使用与展示给模型时相同的运行时工具名称。

## 资源和提示辅助工具

当 MCP 启用时，CLI 还会暴露以下辅助工具：

- `list_mcp_resources`（可选的 `server` 过滤器）
- `list_mcp_resource_templates`（可选的 `server` 过滤器）
- `mcp_read_resource` / `read_mcp_resource`（别名）
- `mcp_get_prompt`

## 最小示例

```json
{
  "timeouts": {
    "connect_timeout": 10,
    "execute_timeout": 60,
    "read_timeout": 120
  },
  "servers": {
    "example": {
      "command": "node",
      "args": ["./path/to/your-mcp-server.js"],
      "env": {},
      "disabled": false
    }
  }
}
```

你也可以使用 `mcpServers` 代替 `servers`，以与其他客户端保持兼容。

## 将 DeepSeek 作为 MCP 服务器运行

你可以将本地 DeepSeek 二进制文件注册为 MCP 服务器，以便其他 DeepSeek 会话（或任何 MCP 客户端）调用其工具。

### 快速设置

```bash
deepseek-tui mcp add-self
```

该命令会解析当前二进制文件路径，生成一个运行 `deepseek-tui serve --mcp` 的配置条目，并将其写入你的 MCP 配置文件中。默认服务器名称为 `deepseek`。

选项：

- `--name <NAME>` — 自定义服务器名称（默认：`deepseek`）
- `--workspace <PATH>` — 服务器的工作目录

### 手动配置

`~/.deepseek/mcp.json` 中等价的手动条目：

```json
{
  "servers": {
    "deepseek": {
      "command": "/path/to/deepseek",
      "args": ["serve", "--mcp"],
      "env": {}
    }
  }
}
```

`deepseek-tui` 二进制文件直接支持 `serve --mcp`。`deepseek` 调度器提供了等价的 `deepseek mcp-server` stdio 入口点。使用你在 `PATH` 中的那个即可（运行 `which deepseek` 或 `which deepseek-tui` 来查找完整路径）。`mcp add-self` 命令会自动解析正确的二进制文件。

### 前提条件

- `command` 中引用的二进制文件必须存在且可执行。
- MCP 服务器作为子进程通过 stdio 运行——无需网络端口。
- 每个 MCP 客户端会话会启动自己的服务器进程。

### 工具命名

自托管 DeepSeek 服务器中的工具遵循标准的命名约定：

- `mcp_deepseek_<tool>`（如果服务器名为 `deepseek`）

例如，`shell` 工具将变为 `mcp_deepseek_shell`。

### MCP 服务器 vs HTTP/SSE API vs ACP

| | `deepseek-tui serve --mcp` | `deepseek-tui serve --http` | `deepseek-tui serve --acp` |
|---|---|---|---|
| **协议** | MCP stdio | HTTP/SSE JSON-RPC | ACP stdio |
| **用途** | 面向 MCP 客户端的工具服务器 | 面向应用的运行时 API | 面向 Zed/自定义 ACP 客户端的编辑器代理 |
| **配置** | `~/.deepseek/mcp.json` 条目 | 直接 URL 连接 | 编辑器 `agent_servers` 自定义命令 |
| **生命周期** | 每个客户端会话启动一个实例 | 长时间运行的守护进程 | 每个编辑器代理会话启动一个实例 |

当你希望其他 MCP 客户端可以使用 DeepSeek 工具时，请使用 `mcp add-self`。
当你构建直接使用 API 的应用时，请使用 `serve --http`。
当编辑器希望作为 ACP 代理与 DeepSeek 通信时，请使用 `serve --acp`。

### 验证

添加后，测试连接：

```bash
deepseek-tui mcp validate
deepseek-tui mcp tools deepseek
```

## 服务器字段

每个服务器的设置项：

- `command`（字符串，必需）
- `args`（字符串数组，可选）
- `env`（对象，可选）
- `connect_timeout`、`execute_timeout`、`read_timeout`（秒，可选）
- `disabled`（布尔值，可选）
- `enabled`（布尔值，可选，默认 `true`）
- `required`（布尔值，可选）：如果此服务器无法初始化，则启动/连接验证失败。
- `enabled_tools`（字符串数组，可选）：此服务器的工具允许列表。
- `disabled_tools`（字符串数组，可选）：在 `enabled_tools` 之后应用的禁用列表。

## 安全说明

MCP 工具现在与内建工具一样，遵循相同的工具审批框架。只读的 MCP 辅助工具（资源/提示列表和读取）在建议性审批模式下可以在无需提示的情况下运行，而具有副作用的 MCP 工具则需要审批。

你仍然应仅配置你信任的 MCP 服务器，并将 MCP 服务器配置视为等同于在你的机器上运行代码。

## 故障排除

- 运行 `deepseek-tui doctor` 来确认 MCP 配置路径的解析结果以及该路径是否存在。
- 在 TUI 中运行 `/mcp validate` 来刷新可见的服务器/工具快照。
- 如果 MCP 配置缺失，运行 `deepseek-tui mcp init --force` 来重新生成。
- 如果工具没有出现，请验证服务器命令能否在你的 shell 中正常运行，以及服务器是否支持 MCP `tools/list`。
