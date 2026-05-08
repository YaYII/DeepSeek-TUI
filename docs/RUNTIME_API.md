# 运行时 API 与集成契约

DeepSeek TUI 通过 `deepseek serve --http` 暴露本地运行时 API，并通过 `deepseek doctor --json` 提供机器可读的健康检查。它还通过 `deepseek serve --acp` 为支持 Agent Client Protocol（ACP）的编辑器客户端提供 stdio 接口。本文档是面向嵌入 DeepSeek 引擎而无需屏幕抓取终端输出的原生 macOS 工作台应用（和其他本地监管程序）的稳定集成契约。

## 架构

```
macOS 工作台（或任何本地监管程序）
        │
        ├─ deepseek doctor --json   → 机器可读的健康与能力信息
        ├─ deepseek serve --http    → HTTP/SSE 运行时 API
        ├─ deepseek serve --acp     → 面向 Zed 等编辑器的 ACP stdio 代理
        ├─ deepseek serve --mcp     → MCP stdio 服务器
        └─ deepseek [args]          → 交互式 TUI 会话
```

引擎作为仅限本地的进程运行。所有 API 默认绑定到 `localhost`。没有托管中继、没有提供方令牌托管、没有秘密泄露。

## ACP stdio 适配器：`deepseek serve --acp`

`deepseek serve --acp` 通过换行分隔的 stdio 使用 JSON-RPC 2.0 与兼容 ACP 的编辑器客户端通信。初始适配器实现了 ACP 基线：

- `initialize`
- `session/new`
- `session/prompt`
- `session/cancel`

提示请求通过配置的 DeepSeek 客户端和当前默认模型进行路由。响应以 `session/update` 代理消息块发布，后跟带有 `stopReason: "end_turn"` 的 `session/prompt` 响应。

适配器有意保守：它尚未通过 ACP 暴露 shell 工具、文件写入工具、检查点重放或会话加载。对于完整的本地运行时 API，请使用 `deepseek serve --http`；当其他客户端需要将 DeepSeek 的工具作为 MCP 工具使用时，请使用 `deepseek serve --mcp`。

## 能力端点：`deepseek doctor --json`

返回描述当前安装就绪状态的 JSON 对象。适用于 macOS 工作台的健康检查轮询。

```bash
deepseek doctor --json
```

### 响应模式（关键字段）

| 字段 | 类型 | 描述 |
|---|---|---|
| `version` | string | 已安装版本（例如 `"0.8.9"`） |
| `config_path` | string | 解析后的配置文件路径 |
| `config_present` | bool | 配置文件是否存在 |
| `workspace` | string | 默认工作区目录 |
| `api_key.source` | string | `env`、`config` 或 `missing` |
| `base_url` | string | API 基础 URL |
| `default_text_model` | string | 默认模型 |
| `memory.enabled` | bool | 记忆功能是否开启 |
| `memory.path` | string | 记忆文件路径 |
| `memory.file_present` | bool | 记忆文件是否存在 |
| `mcp.config_path` | string | MCP 配置文件路径 |
| `mcp.present` | bool | MCP 配置是否存在 |
| `mcp.servers` | array | 每个服务器的健康状态：`{name, enabled, status, detail}` |
| `skills.selected` | string | 解析后的技能目录 |
| `skills.global.path` / `.present` / `.count` | — | DeepSeek 全局技能目录（`~/.deepseek/skills`） |
| `skills.agents.path` / `.present` / `.count` | — | 工作区 `.agents/skills/` 目录 |
| `skills.agents_global.path` / `.present` / `.count` | — | agentskills.io 全局技能目录（`~/.agents/skills`） |
| `skills.local.path` / `.present` / `.count` | — | `skills/` 目录 |
| `skills.opencode.path` / `.present` / `.count` | — | `.opencode/skills/` 目录 |
| `skills.claude.path` / `.present` / `.count` | — | `.claude/skills/` 目录 |
| `tools.path` / `.present` / `.count` | — | 全局工具目录 |
| `plugins.path` / `.present` / `.count` | — | 全局插件目录 |
| `sandbox.available` | bool | 此 OS 是否支持沙箱 |
| `sandbox.kind` | string 或 null | 沙箱类型（例如 `"macos_seatbelt"`） |
| `storage.spillover.path` / `.present` / `.count` | — | 工具输出溢出目录 |
| `storage.stash.path` / `.present` / `.count` | — | Composer 暂存目录 |

### 示例

```json
{
  "version": "0.8.9",
  "config_path": "/Users/you/.deepseek/config.toml",
  "config_present": true,
  "workspace": "/Users/you/projects/deepseek-tui",
  "api_key": {
    "source": "env"
  },
  "base_url": "https://api.deepseek.com/beta",
  "default_text_model": "deepseek-v4-pro",
  "memory": {
    "enabled": false,
    "path": "/Users/you/.deepseek/memory.md",
    "file_present": true
  },
  "mcp": {
    "config_path": "/Users/you/.deepseek/mcp.json",
    "present": true,
    "servers": [
      {"name": "filesystem", "enabled": true, "status": "ok", "detail": "ready"}
    ]
  },
  "sandbox": {
    "available": true,
    "kind": "macos_seatbelt"
  }
}
```

## HTTP/SSE 运行时 API：`deepseek serve --http`

```bash
deepseek serve --http [--host 127.0.0.1] [--port 7878] [--workers 2] [--auth-token TOKEN]
```

默认值：host `127.0.0.1`、port `7878`、2 个工作进程（限制在 1-8）。

服务器默认绑定到 `localhost`。配置通过 CLI 标志进行——没有 `[app_server]` 配置节。

默认情况下，现有本地行为不变，`/v1/*` 路由不进行认证。要使 `/v1/*` 路由需要 bearer token，请传递 `--auth-token TOKEN` 或在启动服务器前设置 `DEEPSEEK_RUNTIME_TOKEN=TOKEN`。`/health` 仍保持公开，用于本地进程监管和就绪检查。

已认证的客户端可以通过 `Authorization: Bearer TOKEN`、`X-DeepSeek-Runtime-Token: TOKEN` 或 `?token=TOKEN`（适用于无法设置自定义头的 EventSource 客户端）提供 token。

### 端点

**健康检查**
- `GET /health`

**会话**（遗留会话管理器）
- `GET /v1/sessions?limit=50&search=<子字符串>`
- `GET /v1/sessions/{id}`
- `DELETE /v1/sessions/{id}`
- `POST /v1/sessions/{id}/resume-thread`

**线程**（持久化运行时数据模型）
- `GET /v1/threads?limit=50&include_archived=false&archived_only=false`
- `GET /v1/threads/summary?limit=50&search=<可选>&include_archived=false&archived_only=false`
- `POST /v1/threads`
- `GET /v1/threads/{id}`
- `PATCH /v1/threads/{id}`（见下面的请求体结构）
- `POST /v1/threads/{id}/resume`
- `POST /v1/threads/{id}/fork`

`archived_only=true` 仅返回已归档的线程（相互覆盖 `include_archived`）。默认行为不变：`include_archived=false` 和 `archived_only=false` 返回活动线程。在 v0.8.10 中添加（#563）。

`PATCH /v1/threads/{id}` 请求体——每个字段都是可选的，缺失表示"无变更"。至少必须有一个字段存在。`title` 和 `system_prompt` 接受空字符串以清除先前设置的值。在 v0.8.10 中添加（#562）：

```json
{
  "archived": true,
  "allow_shell": false,
  "trust_mode": false,
  "auto_approve": false,
  "model": "deepseek-v4-pro",
  "mode": "agent",
  "title": "用户设置的线程标题",
  "system_prompt": "你是一个有用的助手。"
}
```

**轮次**（在线程内）
- `POST /v1/threads/{id}/turns`
- `POST /v1/threads/{id}/turns/{turn_id}/steer`
- `POST /v1/threads/{id}/turns/{turn_id}/interrupt`
- `POST /v1/threads/{id}/compact`（手动压缩）

**事件**（SSE 重放 + 实时流）
- `GET /v1/threads/{id}/events?since_seq=<u64>`

**兼容流**（一次性，向后兼容）
- `POST /v1/stream`

**任务**（持久化后台工作）
- `GET /v1/tasks`
- `POST /v1/tasks`
- `GET /v1/tasks/{id}`
- `POST /v1/tasks/{id}/cancel`

**自动化**（定时重复工作）
- `GET /v1/automations`
- `POST /v1/automations`
- `GET /v1/automations/{id}`
- `PATCH /v1/automations/{id}`
- `DELETE /v1/automations/{id}`
- `POST /v1/automations/{id}/run`
- `POST /v1/automations/{id}/pause`
- `POST /v1/automations/{id}/resume`
- `GET /v1/automations/{id}/runs?limit=20`

**内省**
- `GET /v1/workspace/status`
- `GET /v1/skills`
- `GET /v1/apps/mcp/servers`
- `GET /v1/apps/mcp/tools?server=<可选>`

**用量统计**（跨线程的 token/成本聚合）
- `GET /v1/usage?since=<rfc3339>&until=<rfc3339>&group_by=<day|model|provider|thread>`

`since` / `until` 是包含边界的 RFC 3339 时间戳，可以省略（无边界）。`group_by` 默认为 `day`。桶按键的升序排序。空时间范围产生空的 `buckets`（绝不会是 404）。成本通过模型→定价映射计算；模型没有定价条目的轮次贡献 token 但成本为 `0.0`。在 v0.8.10 中添加（#564）。

```json
{
  "since": "2026-04-01T00:00:00Z",
  "until": "2026-04-30T23:59:59Z",
  "group_by": "day",
  "totals": {
    "input_tokens": 12345,
    "output_tokens": 6789,
    "cached_tokens": 0,
    "reasoning_tokens": 0,
    "cost_usd": 0.012,
    "turns": 42
  },
  "buckets": [
    {
      "key": "2026-04-30",
      "input_tokens": 1234,
      "output_tokens": 678,
      "cached_tokens": 0,
      "reasoning_tokens": 0,
      "cost_usd": 0.001,
      "turns": 3
    }
  ]
}
```

## 运行时数据模型

运行时使用持久化的线程/轮次/项目生命周期。

- **ThreadRecord** — `id`、`created_at`、`updated_at`、`model`、`workspace`、`mode`、`task_id`、`coherence_state`、`system_prompt`、`latest_turn_id`、`latest_response_bookmark`、`archived`
- **TurnRecord** — `id`、`thread_id`、`status`（`queued|in_progress|completed|failed|interrupted|canceled`）、时间戳、持续时间、用量、错误摘要
- **TurnItemRecord** — `id`、`turn_id`、`kind`（`user_message|agent_message|tool_call|file_change|command_execution|context_compaction|status|error`）、生命周期 `status`、`metadata`

事件是追加写入的，具有全局单调递增的 `seq` 用于重放/恢复。

### 重启语义

- 如果进程在轮次或项目处于 `queued` 或 `in_progress` 状态时重启，恢复的记录被标记为 `interrupted`，并带有 `"被进程重启中断"` 的错误。
- 任务执行在同一持久化的线程/轮次存储之上执行自己的恢复。

### 审批模型

- `auto_approve` 标志适用于运行时审批桥接和引擎工具上下文。当为线程/轮次/任务启用时，需要审批的工具在非交互式运行时路径中被自动批准，shell 安全检查以自动批准模式运行，生成的子代理继承该设置。
- 省略时，`auto_approve` 默认为 `false`。

### SSE 事件流

SSE 事件负载结构：

```json
{
  "seq": 42,
  "timestamp": "2026-02-11T20:18:49.123Z",
  "thread_id": "thr_1234abcd",
  "turn_id": "turn_5678efgh",
  "item_id": "item_90ab12cd",
  "event": "item.delta",
  "payload": {
    "delta": "部分输出",
    "kind": "agent_message"
  }
}
```

常见事件名称：`thread.started`、`thread.forked`、`turn.started`、`turn.lifecycle`、`turn.steered`、`turn.interrupt_requested`、`turn.completed`、`item.started`、`item.delta`、`item.completed`、`item.failed`、`item.interrupted`、`approval.required`、`sandbox.denied`、`coherence.state`。

## 安全边界

- **仅限 localhost**。服务器默认绑定到 `127.0.0.1`。仅在您有执行认证的反向代理/VPN 时设置 `--host 0.0.0.0`。运行时不提供用户隔离或 TLS。
- **可选的 token 保护**。`--auth-token` 或 `DEEPSEEK_RUNTIME_TOKEN` 要求 `/v1/*` 路由使用匹配的 bearer token。这是一个本地便利防护，不能替代公共网络上的 TLS、VPN 或可信反向代理。
- **无提供方令牌托管**。服务器从不返回 API 密钥。`api_key.source` 能力字段报告 `env`、`config` 或 `missing`——从不返回密钥本身。
- **无托管中继**。应用服务器是一个受用户控制的本地进程。没有云组件。
- **能力响应**从不泄露秘密、文件内容或会话消息体。它们报告*元数据*：存在性、计数、状态标志。

### CORS 允许列表

运行时 API 带有内置的开发源允许列表：`http://localhost:3000`、`http://127.0.0.1:3000`、`http://localhost:1420`、`http://127.0.0.1:1420`、`tauri://localhost`。要添加额外的源（例如在 Vite 的默认 `:5173` 上开发 UI 时），请使用以下任一方式：

- CLI 标志（可重复）：`deepseek serve --http --cors-origin http://localhost:5173`
- 环境变量（逗号分隔）：`DEEPSEEK_CORS_ORIGINS="http://localhost:5173,http://localhost:8080"`
- 配置（`~/.deepseek/config.toml`）：
  ```toml
  [runtime_api]
  cors_origins = ["http://localhost:5173"]
  ```

用户提供的源在**内置默认值之上堆叠**；它们不会替换默认值。不支持通配符源——保留显式允许列表模型。在 v0.8.10 中添加（#561）。

## 会话生命周期（原生 UI 监管）

| 操作 | 端点 |
|---|---|
| 列出会话 | `GET /v1/sessions` |
| 获取会话 | `GET /v1/sessions/{id}` |
| 删除会话 | `DELETE /v1/sessions/{id}` |
| 恢复到线程 | `POST /v1/sessions/{id}/resume-thread` |
| 创建线程 | `POST /v1/threads` |
| 列出线程 | `GET /v1/threads` |
| 附加到事件 | `GET /v1/threads/{id}/events?since_seq=0` |
| 发送消息 | `POST /v1/threads/{id}/turns` |
| 引导 | `POST /v1/threads/{id}/turns/{turn_id}/steer` |
| 中断 | `POST /v1/threads/{id}/turns/{turn_id}/interrupt` |
| 压缩 | `POST /v1/threads/{id}/compact` |

## 兼容性测试

契约快照保存在 `crates/protocol/tests/` 中。运行：

```bash
cargo test -p deepseek-protocol --test parity_protocol --locked
```

这会验证应用服务器的事件模式没有偏离文档化的契约。CI 在每次推送到 `main` 和发布标签时运行此测试。