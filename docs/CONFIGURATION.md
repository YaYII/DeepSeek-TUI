# 配置

DeepSeek TUI 从 TOML 文件和环境变量读取配置。进程启动时，如果存在工作区本地的 `.env` 文件，也会加载它。使用受跟踪的 `.env.example` 作为模板；将其复制到 `.env`，然后编辑您需要的提供方和安全设置。

## 查找路径

默认配置路径：

- `~/.deepseek/config.toml`

覆盖方式：

- CLI：`deepseek --config /path/to/config.toml`
- 环境变量：`DEEPSEEK_CONFIG_PATH=/path/to/config.toml`

如果两者都设置，`--config` 优先。环境变量覆盖在文件加载后应用。

### 每项目覆盖（#485）

当 TUI 在包含 `<workspace>/.deepseek/config.toml` 文件的工作区启动时，该文件中声明的值会合并到全局配置之上。这使得仓库可以锁定自己的提供方、模型、沙箱策略或审批策略，而无需触及用户的 `~/.deepseek/config.toml`。传递 `--no-project-config` 可在一次启动中跳过覆盖。

项目覆盖中受支持的键（仅限顶层字段）：

| 键 | 效果 |
|---|---|
| `provider` | 切换后端（例如企业仓库使用 `"nvidia-nim"`） |
| `model` | 覆盖 `default_text_model` |
| `api_key` | 使用每个仓库的密钥（通常从 `.env` 读取，**不提交**） |
| `base_url` | 指向自托管端点 |
| `reasoning_effort` | 为复杂仓库强制设为 `"high"` / `"max"` |
| `approval_policy` | 对有意见的仓库设为 `"never"` / `"on-request"` / `"untrusted"` |
| `sandbox_mode` | `"read-only"` / `"workspace-write"` / `"danger-full-access"` |
| `mcp_config_path` | 每个仓库的 MCP 服务器集 |
| `notes_path` | 在仓库内保留笔记 |
| `max_subagents` | 为受限仓库限制并发（限制在 1..20） |
| `allow_shell` | 在 `false` 时关闭 shell 工具访问 |

覆盖范围有意较窄——它涵盖了仓库维护者最可能在贡献者之间标准化的字段。其他设置（skills_dir、hooks、capacity、retry 等）保持用户全局。如果您的仓库需要更多，请提交 issue 描述具体用例。

`deepseek` 外观程序和 `deepseek-tui` 二进制文件共享相同的配置文件，用于 DeepSeek 认证和模型默认值。`deepseek auth set --provider deepseek`（以及旧的 `deepseek login --api-key ...` 别名）将密钥保存到 `~/.deepseek/config.toml`，`deepseek --model deepseek-v4-flash` 作为 `DEEPSEEK_MODEL` 转发给 TUI。

凭据查找使用 `config -> keyring -> env` 的顺序（在任何显式 CLI `--api-key` 之后）。运行 `deepseek auth status` 检查活动提供方的配置文件、OS 密钥环后端、环境变量、获胜源和最后四个标签，而不打印密钥本身。该命令仅探测活动提供方的密钥环条目。

对于托管、通用 OpenAI 兼容或自托管的提供方，设置 `provider = "nvidia-nim"`、`"openai"`、`"fireworks"`、`"sglang"`、`"vllm"` 或 `"ollama"`，或传递 `deepseek --provider <name>`。外观程序将提供方凭据保存到共享用户配置，并将解析后的密钥、base URL、提供方和模型转发给 TUI 进程。使用 `deepseek auth set --provider nvidia-nim --api-key "YOUR_NVIDIA_API_KEY"` 或 `deepseek auth set --provider openai --api-key "YOUR_OPENAI_COMPATIBLE_API_KEY"` 或 `deepseek auth set --provider fireworks --api-key "YOUR_FIREWORKS_API_KEY"` 通过外观程序保存提供方密钥。通用 `openai` 提供方默认为 `https://api.openai.com/v1`，接受 `OPENAI_BASE_URL`，并按原样传递模型 ID 给 OpenAI 兼容网关。SGLang、vLLM 和 Ollama 是自托管的，默认情况下可以在没有 API 密钥的情况下运行。Ollama 默认为 `http://localhost:11434/v1`，并按原样发送模型标签，如 `deepseek-coder:1.3b` 或 `qwen2.5-coder:7b`。

需要额外请求头的第三方 OpenAI 兼容网关可以在顶层或提供方表（如 `[providers.deepseek]`）下设置 `http_headers = { "X-Model-Provider-Id" = "your-model-provider" }`。配置后，DeepSeek TUI 在模型 API 请求上发送这些自定义头。等效的环境变量覆盖是 `DEEPSEEK_HTTP_HEADERS`，使用逗号分隔的 `name=value` 对，如 `X-Model-Provider-Id=your-model-provider,X-Gateway-Route=dev`。`Authorization` 和 `Content-Type` 由客户端管理，不会被此设置覆盖。

要引导 MCP 和技能目录到其解析后的路径，运行 `deepseek-tui setup`。要仅搭建 MCP，运行 `deepseek-tui mcp init`。

注意：setup、doctor、mcp、features、sessions、resume/fork、exec、review 和 eval 是 `deepseek-tui` 二进制文件的子命令。`deepseek` 调度器暴露一组不同的命令（`auth`、`config`、`model`、`thread`、`sandbox`、`app-server`、`mcp-server`、`completion`），并将纯提示转发给 `deepseek-tui`。

## 配置文件（Profiles）

您可以在同一个文件中定义多个配置文件：

```toml
api_key = "PERSONAL_KEY"
default_text_model = "deepseek-v4-pro"

[profiles.work]
api_key = "WORK_KEY"
base_url = "https://api.deepseek.com/beta"

[profiles.nvidia-nim]
provider = "nvidia-nim"
api_key = "NVIDIA_KEY"
base_url = "https://integrate.api.nvidia.com/v1"
default_text_model = "deepseek-ai/deepseek-v4-pro"

[profiles.fireworks]
provider = "fireworks"
default_text_model = "accounts/fireworks/models/deepseek-v4-pro"

[profiles.openai-compatible]
provider = "openai"

[profiles.openai-compatible.providers.openai]
base_url = "https://openai-compatible.example/v4"
model = "glm-5"

[profiles.sglang]
provider = "sglang"
base_url = "http://localhost:30000/v1"
default_text_model = "deepseek-ai/DeepSeek-V4-Pro"

[profiles.vllm]
provider = "vllm"
base_url = "http://localhost:8000/v1"
default_text_model = "deepseek-ai/DeepSeek-V4-Pro"

[profiles.ollama]
provider = "ollama"
base_url = "http://localhost:11434/v1"
default_text_model = "deepseek-coder:1.3b"
```

选择配置文件的方式：

- CLI：`deepseek --profile work`
- 环境变量：`DEEPSEEK_PROFILE=work`

如果选择了配置文件但缺失，DeepSeek TUI 会退出并显示可用配置文件列表的错误信息。

## 环境变量

大多数运行时环境变量会覆盖配置值。API 密钥变量在已保存的配置和密钥环凭据之后作为回退：

- `DEEPSEEK_API_KEY`
- `DEEPSEEK_BASE_URL`
- `DEEPSEEK_HTTP_HEADERS`（自定义模型请求头，逗号分隔的 `name=value` 对）
- `DEEPSEEK_PROVIDER`（`deepseek|nvidia-nim|openai|openrouter|novita|fireworks|sglang|vllm|ollama`）
- `DEEPSEEK_MODEL` 或 `DEEPSEEK_DEFAULT_TEXT_MODEL`
- `DEEPSEEK_STREAM_IDLE_TIMEOUT_SECS`（流空闲超时秒数；默认 `300`，限制在 `1..3600`）
- `DEEPSEEK_STREAM_OPEN_TIMEOUT_SECS`（连接建立 + 响应头等待秒数；默认 `45`，限制在 `5..300`；与逐块空闲超时不同）
- `NVIDIA_API_KEY` 或 `NVIDIA_NIM_API_KEY`（提供方为 `nvidia-nim` 时优先；回退到 `DEEPSEEK_API_KEY`）
- `NVIDIA_NIM_BASE_URL`、`NIM_BASE_URL` 或 `NVIDIA_BASE_URL`
- `NVIDIA_NIM_MODEL`
- `OPENAI_API_KEY`
- `OPENAI_BASE_URL`
- `OPENAI_MODEL`
- `OPENROUTER_API_KEY`
- `OPENROUTER_BASE_URL`
- `NOVITA_API_KEY`
- `NOVITA_BASE_URL`
- `FIREWORKS_API_KEY`
- `FIREWORKS_BASE_URL`
- `SGLANG_BASE_URL`
- `SGLANG_MODEL`
- `SGLANG_API_KEY`（可选；许多 localhost SGLang 服务器不需要认证）
- `VLLM_BASE_URL`
- `VLLM_MODEL`
- `VLLM_API_KEY`（可选；许多 localhost vLLM 服务器不需要认证）
- `OLLAMA_BASE_URL`
- `OLLAMA_MODEL`
- `OLLAMA_API_KEY`（可选；许多 localhost Ollama 服务器不需要认证）
- `DEEPSEEK_LOG_LEVEL` 或 `RUST_LOG`（`info`/`debug`/`trace` 启用轻量级详细日志）
- `DEEPSEEK_SKILLS_DIR`
- `DEEPSEEK_MCP_CONFIG`
- `DEEPSEEK_NOTES_PATH`
- `DEEPSEEK_MEMORY`（`1|on|true|yes|y|enabled` 开启用户记忆）
- `DEEPSEEK_MEMORY_PATH`
- `DEEPSEEK_ALLOW_SHELL`（`1`/`true` 启用）
- `DEEPSEEK_APPROVAL_POLICY`（`on-request|untrusted|never`）
- `DEEPSEEK_SANDBOX_MODE`（`read-only|workspace-write|danger-full-access|external-sandbox`）
- `DEEPSEEK_MANAGED_CONFIG_PATH`
- `DEEPSEEK_REQUIREMENTS_PATH`
- `DEEPSEEK_MAX_SUBAGENTS`（限制在 `1..20`）
- `DEEPSEEK_TASKS_DIR`（运行时任务队列/工件存储，默认 `~/.deepseek/tasks`）
- `DEEPSEEK_ALLOW_INSECURE_HTTP`（`1`/`true` 允许非本地 `http://` base URL；默认拒绝）
- `DEEPSEEK_FORCE_HTTP1`（`1|true|yes|on` 将 HTTP 客户端固定到 HTTP/1.1，禁用 HTTP/2；在 Windows 或处理长寿命 H2 流有问题的代理后面有用）
- `DEEPSEEK_HOME`（覆盖基础数据目录；默认为 `~/.deepseek`）
- `DEEPSEEK_AUTOMATIONS_DIR`（覆盖自动化存储目录；默认为 `~/.deepseek/automations`）
- `DEEPSEEK_CAPACITY_ENABLED`
- `DEEPSEEK_CAPACITY_LOW_RISK_MAX`
- `DEEPSEEK_CAPACITY_MEDIUM_RISK_MAX`
- `DEEPSEEK_CAPACITY_SEVERE_MIN_SLACK`
- `DEEPSEEK_CAPACITY_SEVERE_VIOLATION_RATIO`
- `DEEPSEEK_CAPACITY_REFRESH_COOLDOWN_TURNS`
- `DEEPSEEK_CAPACITY_REPLAN_COOLDOWN_TURNS`
- `DEEPSEEK_CAPACITY_MAX_REPLAY_PER_TURN`
- `DEEPSEEK_CAPACITY_MIN_TURNS_BEFORE_GUARDRAIL`
- `DEEPSEEK_CAPACITY_PROFILE_WINDOW`
- `DEEPSEEK_CAPACITY_PRIOR_CHAT`
- `DEEPSEEK_CAPACITY_PRIOR_REASONER`
- `DEEPSEEK_CAPACITY_PRIOR_V4_PRO`
- `DEEPSEEK_CAPACITY_PRIOR_V4_FLASH`
- `DEEPSEEK_CAPACITY_PRIOR_FALLBACK`
- `NO_ANIMATIONS`（`1|true|yes|on` 在启动时强制 `low_motion = true` 和 `fancy_animations = false`，无论已保存的设置如何；参见 [`docs/ACCESSIBILITY.md`](./ACCESSIBILITY.md)）。
- `SSL_CERT_FILE` — 企业代理 / TLS 检查 MITM 用户将其指向一个 PEM 包（或单个 DER 证书），证书会与平台的系统信任存储一起添加。失败会记录警告并继续——现有的系统根仍然适用。

### 指令源（`instructions = [...]`, #454）

添加一个附加的系统提示源列表，按声明的顺序与自动加载的 `AGENTS.md` 拼接：

```toml
instructions = [
    "./AGENTS.md",
    "~/.deepseek/global.md",
    "~/team/agents-shared.md",
]
```

规则：

- 路径经过 `expand_path`，因此 `~` 和环境变量都能正常工作。
- 每个文件限制为 100 KiB；过大的文件会用 `[…elided]` 标记截断而不是跳过。
- 缺失的文件会以 tracing 警告跳过，因此过期的条目不会导致启动失败。
- 项目配置（`<workspace>/.deepseek/config.toml`）**整体替换**用户数组而不是合并。如果两者都需要，在项目数组中列出 `~/global.md`。在项目中设置 `instructions = []` 可清除该仓库的用户列表。

### `/hooks` 列表

在 TUI 内运行 `/hooks`（或 `/hooks list`）以查看按事件分组的每个已配置生命周期钩子，包括每个钩子的名称、命令预览、超时和条件。`[hooks].enabled` 标志的状态显示在顶部，因此可以清楚地看到钩子何时被全局禁用。钩子在 `[[hooks.hooks]]` 条目下配置——有关完整模式，请参阅现有的钩子系统文档。

### Composer 暂存（`/stash`、Ctrl+S）

在 composer 中按 **Ctrl+S** 将当前草稿暂存到 `~/.deepseek/composer_stash.jsonl`。`/stash list` 显示带有一行预览和时间戳的已暂存草稿；`/stash pop` 恢复最近暂存的草稿（LIFO）；`/stash clear` 清空文件。限制为 200 条；多行草稿完整往返。

## 设置文件（持久化 UI 偏好）

DeepSeek TUI 还将用户偏好存储在：

- `~/.config/deepseek/settings.toml`

值得注意的设置包括 `auto_compact`（默认 `false`），它选择仅在接近活动模型限制时进行替换式压缩。默认的 V4 路径保留稳定的消息前缀以进行缓存重用；仅在您明确想要自动替换压缩时使用手动 `/compact` 或启用 `auto_compact`。您可以通过 TUI 中的 `/settings` 和 `/config`（交互式编辑器）检查或更新这些设置。

常用设置键：

- `theme`（default、dark、light、whale）
- `auto_compact`（on/off，默认 off）
- `paste_burst_detection`（on/off，默认 on）：对于不发出 bracketed-paste 事件的终端，提供回退的快速按键粘贴检测。这与终端的 bracketed-paste 模式无关。
- `show_thinking`（on/off）
- `show_tool_details`（on/off）
- `locale`（`auto`、`en`、`ja`、`zh-Hans`、`pt-BR`；默认 `auto`）：UI 界面语言。`auto` 依次检查 `LC_ALL`、`LC_MESSAGES` 和 `LANG`；不支持或缺失的语言环境回退到英语。运行时还将解析后的语言环境暴露在系统提示中，作为最新用户消息不明确时 V4 推理和回复的回退自然语言。清晰的语言指示仍然优先；即使解析后的语言环境是英语，中文用户轮次也应产生中文的 `reasoning_content` 和中文的最终回复。
- `background_color`（`#RRGGBB`、`RRGGBB` 或 `default`）：可选的 TUI 主背景色，应用于根、头部、记录和底部面板，同时保持面板对比度。
- `cost_currency`（`usd`、`cny`；默认 `usd`）：底部面板、上下文面板、`/cost`、`/tokens` 和长轮次通知摘要使用的货币。别名 `rmb` 和 `yuan` 归一化为 `cny`。
- `default_mode`（agent、plan、yolo；旧的 `normal` 被接受并归一化为 `agent`）
- `max_history`（已提交的输入历史记录数量；清除的草稿也会本地保留用于 composer 历史搜索）
- `default_model`（模型名称覆盖）

只有 `agent`、`plan` 和 `yolo` 是 UI 中可见的模式。为了兼容性，包含 `default_mode = "normal"` 的旧设置文件仍会作为 `agent` 加载，隐藏的 `/normal` 斜杠命令会切换到 Agent 模式。

本地化范围在 [LOCALIZATION.md](LOCALIZATION.md) 中跟踪。v0.7.6 核心包仅覆盖高可见性的 TUI 界面；提供方/工具模式、个性提示和完整文档保持英文，除非稍后显式翻译。

可读性语义：

- 选择在记录、composer 菜单和模态框中使用统一风格。
- 底部提示使用专用语义角色（`FOOTER_HINT`），使提示文本在所有主题中保持可读。
- 底部包含一个紧凑的 `coherence` 芯片，描述当前会话的稳定和专注程度。可能的状态有 `healthy`、`crowded`、`refreshing`、`verifying` 和 `resetting`；这些源自容量和压缩事件，在正常 UI 中不暴露内部公式。

### Token 数量和驱动条件

DeepSeek V4 前缀缓存使得 token 标签至关重要。这些数量保持分开：

| 数量 | 含义 | 允许驱动 |
|---|---|---|
| 活动请求输入估计 | 下一个请求的实时系统提示和记录负载的保守估计 | 头部/底部上下文百分比、硬循环触发、可选 Flash seam 触发和紧急溢出预检 |
| 保留响应余量 | 内部轮次预算加安全余量。v0.8.16 将正常轮次保持在 `262144` 保留输出 token，并添加 `1024` 安全 token 用于上下文窗口检查，即使 V4 能力元数据报告官方 `384000` 最大输出 | 仅用于硬循环和紧急溢出预算检查 |
| 累计 API 用量 | 提供方报告的输入加输出 token，跨已完成 API 调用求和；多工具轮次可能多次计数相同的稳定前缀 | 仅用于会话用量和近似成本遥测 |
| 提示缓存命中/未命中 | 最近的 API 调用的提供方缓存遥测（可用时） | 仅用于缓存命中显示和成本估算；绝不用于压缩、seam 或循环触发 |
| 上下文百分比 | 活动请求输入估计除以模型上下文窗口 | 仅用于显示；它反映上下文安全机制使用的活动输入基础 |
| 成本估算 | 根据提供方用量和已配置的 DeepSeek 费率的近似支出 | 仅用于显示 |

对于默认的 V4 路径，当活动输入达到配置的循环阈值（`768000`）和模型窗口减去保留响应余量中较小的值时触发硬循环。替换压缩保持可选（`auto_compact = false` 默认），Flash seam 管理器保持可选（`[context].enabled = false`），容量控制器保持禁用，除非已配置。

### 命令迁移说明

如果您从旧版本升级：

- 旧版：`/deepseek`
  新版：`/links`（别名：`/dashboard`、`/api`）
- 旧版：`/set model deepseek-reasoner`
  新版：`/config` 并将 `model` 行编辑为 `deepseek-v4-pro` 或 `deepseek-v4-flash`
- 旧版：可见的 `Normal` 模式或 `default_mode = "normal"`
  新版：使用 `Agent` / `default_mode = "agent"`；旧的 `normal` 仍映射到 `agent`
- 旧版：在斜杠 UX/帮助中发现 `/set`
  新版：使用 `/config` 进行编辑，`/settings` 进行只读检查

## 键参考

### 核心键（TUI/引擎使用）

- `provider`（字符串，可选）：`deepseek`（默认）、`nvidia-nim`、`openai`、`openrouter`、`novita`、`fireworks`、`sglang`、`vllm` 或 `ollama`。旧的 `deepseek-cn` 配置仍被接受为 `deepseek` 的别名；DeepSeek 在全球使用相同的官方主机 [`https://api.deepseek.com`](https://api-docs.deepseek.com/)。`nvidia-nim` 通过 `https://integrate.api.nvidia.com/v1` 定位 NVIDIA 的 NIM 托管 DeepSeek 端点；`openai` 定位通用 OpenAI 兼容端点，默认为 `https://api.openai.com/v1`；`fireworks` 定位 `https://api.fireworks.ai/inference/v1`；`sglang` 定位自托管 OpenAI 兼容端点，默认为 `http://localhost:30000/v1`；`vllm` 定位自托管 vLLM OpenAI 兼容端点，默认为 `http://localhost:8000/v1`；`ollama` 定位 Ollama 的 OpenAI 兼容端点，默认为 `http://localhost:11434/v1`。
- `api_key`（字符串，托管提供方必需）：对于 DeepSeek/托管提供方必须非空（或设置提供方 API 密钥环境变量）。自托管的 SGLang、vLLM 和 Ollama 可以省略。
- `base_url`（字符串，可选）：对于 DeepSeek 的 OpenAI 兼容 Chat Completions API 默认为 `https://api.deepseek.com/beta`，包括旧的 `provider = "deepseek-cn"` 配置、`provider = "openai"` 的 `https://api.openai.com/v1`，或托管/自托管提供方的特定端点。显式设置 `https://api.deepseek.com` 或 `https://api.deepseek.com/v1` 可选择退出 DeepSeek 的 beta 功能。
- `default_text_model`（字符串，可选）：对于 DeepSeek 默认为 `deepseek-v4-pro`，对于 NVIDIA NIM 为 `deepseek-ai/deepseek-v4-pro`，对于通用 OpenAI 兼容端点为 `gpt-4.1`，对于 Fireworks 为 `accounts/fireworks/models/deepseek-v4-pro`，对于 SGLang/vLLM 为 `deepseek-ai/DeepSeek-V4-Pro`，对于 Ollama 为 `deepseek-coder:1.3b`。当前公开的 DeepSeek ID 是 `deepseek-v4-pro` 和 `deepseek-v4-flash`，两者都有 1M 上下文窗口、384K 最大输出和默认启用的思考模式。旧的 `deepseek-chat` 和 `deepseek-reasoner` 作为 `deepseek-v4-flash` 的兼容性别名保留到 2026 年 7 月 24 日。提供方特定的映射在支持时将 `deepseek-v4-pro` / `deepseek-v4-flash` 转换为每个提供方的模型 ID。通用 `openai` 和 Ollama 模型 ID 按原样传递。带有自定义 `base_url` 的 OpenRouter 提供方配置也保留显式模型值，这允许 OpenAI 兼容网关接受裸模型 ID。使用 `/models` 或 `deepseek models` 从您配置的端点发现实时 ID。`DEEPSEEK_MODEL` 在单次进程中覆盖此设置。
- `reasoning_effort`（字符串，可选）：`off`、`low`、`medium`、`high` 或 `max`；默认为已配置的 UI 档位。DeepSeek Platform 接收顶层 `thinking` / `reasoning_effort` 字段。NVIDIA NIM 通过 `chat_template_kwargs` 接收等效设置。
- `allow_shell`（布尔值，可选）：默认为 `true`（沙箱化）。
- `approval_policy`（字符串，可选）：`on-request`、`untrusted` 或 `never`。运行时 `/config` 中的 `approval_mode` 编辑也接受 `on-request` 和 `untrusted` 别名。
- `sandbox_mode`（字符串，可选）：`read-only`、`workspace-write`、`danger-full-access`、`external-sandbox`。
  平台支持不完全相同。macOS 使用 Seatbelt 进行策略强制。Linux 支持通过 Landlock 辅助功能门控。Windows 目前不提供 OS 沙箱；计划的 Windows 辅助功能合约仅以进程树包含开始，在实现之前不得描述为只读文件系统隔离、工作区写入强制、网络阻塞、注册表隔离或 AppContainer 隔离。
- `managed_config_path`（字符串，可选）：在用户/环境配置后加载的托管配置文件。
- `requirements_path`（字符串，可选）：用于强制允许的审批/沙箱值的要求文件。
- `max_subagents`（整数，可选）：默认为 `10`，限制在 `1..20`。
- `subagents.*`（可选）：`agent_spawn` 和相关子代理工具的每角色/类型模型默认值。显式工具 `model` 值优先，然后是角色/类型覆盖，最后是父运行时模型。支持的便捷键是 `default_model`、`worker_model`、`explorer_model`、`awaiter_model`、`review_model`、`custom_model` 和 `max_concurrent`。`[subagents] max_concurrent` 值覆盖顶层 `max_subagents`，也限制在 `1..20`。`[subagents.models]` 接受小写角色或类型键，如 `worker`、`explorer`、`general`、`explore`、`plan` 和 `review`。值必须在生成代理之前归一化为受支持的 DeepSeek 模型 ID。
- `skills_dir`（字符串，可选）：默认为 `~/.deepseek/skills`（每个技能是一个包含 `SKILL.md` 的目录）。工作区本地的 `.agents/skills` 或 `./skills` 在存在时优先；运行时还会发现全局的 agentskills.io 兼容 `~/.agents/skills` 和更广泛的 Claude 生态系统 `~/.claude/skills`。
- `mcp_config_path`（字符串，可选）：默认为 `~/.deepseek/mcp.json`。在 `/config` 中可见，可以从 TUI 更改。新路径会被 `/mcp` 立即使用，但重建模型可见的 MCP 工具池需要重启 TUI。
- `notes_path`（字符串，可选）：默认为 `~/.deepseek/notes.txt`，由 `note` 工具使用。
- `[memory].enabled`（布尔值，可选）：默认为 `false`。当为 `true` 时，TUI 将用户记忆文件加载到 `<user_memory>` 提示块中，在 composer 中启用 `# foo` 快速捕获，展示 `/memory` 斜杠命令，并注册 `remember` 工具。也可以通过 `DEEPSEEK_MEMORY=on` 使用相同的开关。
- `memory_path`（字符串，可选）：默认为 `~/.deepseek/memory.md`。启用时由用户记忆功能使用——参见 [`MEMORY.md`](MEMORY.md) 以了解完整功能表面（`# foo` composer 前缀、`/memory` 斜杠命令、`remember` 工具、可选开关）。
- `snapshots.*`（可选）：用于文件回滚的 side-git 工作区快照：
  - `[snapshots].enabled`（布尔值，默认 `true`）
  - `[snapshots].max_age_days`（整数，默认 `7`）
  - 快照位于 `~/.deepseek/snapshots/<project_hash>/<worktree_hash>/.git` 下，从不使用工作区自己的 `.git` 目录
- `context.*`（可选）：追加写入的 Flash seam 管理器，目前可选。阈值使用活动请求输入估计，而非生命周期累计 API 用量：
  - `[context].enabled`（布尔值，默认 `false`）
  - `[context].verbatim_window_turns`（整数，默认 `16`）
  - `[context].l1_threshold`（整数，默认 `192000`）
  - `[context].l2_threshold`（整数，默认 `384000`）
  - `[context].l3_threshold`（整数，默认 `576000`）
  - `[context].cycle_threshold`（整数，默认 `768000`）
  - `[context].seam_model`（字符串，默认 `deepseek-v4-flash`）
- `retry.*`（可选）：API 请求的重试/退避设置：
  - `[retry].enabled`（布尔值，默认 `true`）
  - `[retry].max_retries`（整数，默认 `3`）
  - `[retry].initial_delay`（浮点秒数，默认 `1.0`）
  - `[retry].max_delay`（浮点秒数，默认 `60.0`）
  - `[retry].exponential_base`（浮点数，默认 `2.0`）
- `capacity.*`（可选）：运行时上下文容量控制器。由于其主动干预可能会重写实时记录，因此是可选启用。
  - `[capacity].enabled`（布尔值，默认 `false`）
  - `[capacity].low_risk_max`（浮点数，默认 `0.50`）
  - `[capacity].medium_risk_max`（浮点数，默认 `0.62`）
  - `[capacity].severe_min_slack`（浮点数，默认 `-0.25`）
  - `[capacity].severe_violation_ratio`（浮点数，默认 `0.40`）
  - `[capacity].refresh_cooldown_turns`（整数，默认 `6`）
  - `[capacity].replan_cooldown_turns`（整数，默认 `5`）
  - `[capacity].max_replay_per_turn`（整数，默认 `1`）
  - `[capacity].min_turns_before_guardrail`（整数，默认 `4`）
  - `[capacity].profile_window`（整数，默认 `8`）
  - `[capacity].deepseek_v3_2_chat_prior`（浮点数，默认 `3.9`）
  - `[capacity].deepseek_v3_2_reasoner_prior`（浮点数，默认 `4.1`）
  - `[capacity].deepseek_v4_pro_prior`（浮点数，默认 `3.5`）
  - `[capacity].deepseek_v4_flash_prior`（浮点数，默认 `4.2`）
  - `[capacity].fallback_default_prior`（浮点数，默认 `3.8`）
- `[notifications].method`（字符串，可选）：`auto`、`osc9`、`bel` 或 `off`。默认为 `auto`。TUI 在已完成的（成功的）轮次（其已用时间达到 `threshold_secs`）上触发此通知；失败和取消的轮次静默。`auto` 对 `iTerm.app`、`Ghostty` 和 `WezTerm` 解析为 `osc9`（通过 `$TERM_PROGRAM` 检测）。否则在 macOS / Linux 上回退到 `bel`，在 Windows 上回退到 `off`（在 Windows 上 BEL 映射到系统错误提示音——请参阅[通知](#通知)部分了解完整原因，#583）。
- `[notifications].threshold_secs`（整数，可选）：默认为 `30`。只有已用时间达到或超过此值的已完成轮次才会触发通知。
- `[notifications].include_summary`（布尔值，可选）：默认为 `false`。当为 `true` 时，通知正文包括已用持续时间和轮次在配置的显示货币中的成本。
- `tui.alternate_screen`（字符串，可选）：`auto`、`always` 或 `never`。为配置兼容性而保留，但交互式会话现在始终使用 TUI 拥有的备用屏幕，因此主机终端回滚无法劫持视口。
- `tui.mouse_capture`（布尔值，可选，在非 Windows 终端上当备用屏幕活动时默认 `true`；在 Windows 上和 JetBrains JediTerm 内部——PyCharm/IDEA/CLion 等——默认 `false`，因为鼠标事件转义序列会作为乱码文本泄漏到输入流中，参见 #878 / #898）：启用内部鼠标滚动、记录选择和右键上下文操作。TUI 拥有的拖拽选择仅复制用户/助手记录文本。设置为 `false` 或使用 `--no-mouse-capture` 运行以使用原始终端选择；设置为 `true` 或使用 `--mouse-capture` 运行以在默认关闭的任何地方启用。
- `tui.terminal_probe_timeout_ms`（整数，可选，默认 `500`）：启动时终端模式探测超时（毫秒）。值限制在 `100..5000`；超时会发出警告并中止启动，而不是无限挂起。
- `tui.osc8_links`（布尔值，可选，默认 `true`）：在记录输出中的 URL 周围发出 OSC 8 转义序列，以便支持它们的终端（iTerm2、Terminal.app 13+、Ghostty、Kitty、WezTerm、Alacritty、最新的 gnome-terminal/konsole）将其呈现为 Cmd+点击超链接。不支持 OSC 8 的终端会渲染纯 URL 并忽略转义。设置为 `false` 用于渲染该序列有问题的终端；选择/剪贴板输出始终剥离转义。
- `hooks`（可选）：生命周期钩子配置（参见 `config.example.toml`）。
- `features.*`（可选）：功能标志覆盖（见下文）。

### 用户记忆

用户记忆在单个顶层路径设置和一个可选开关表之间分割：

```toml
memory_path = "~/.deepseek/memory.md"

[memory]
enabled = true
```

注意：

- `memory_path` 保持在顶层，与 `notes_path` 和 `skills_dir` 并列；它不嵌套在 `[memory]` 下。
- `DEEPSEEK_MEMORY_PATH` 从环境变量覆盖文件路径。
- `DEEPSEEK_MEMORY=on`（也接受 `1`、`true`、`yes`、`y` 或 `enabled`）翻转功能开关，无需编辑 `config.toml`。
- 禁用时功能处于惰性状态：不注入文件，`# foo` 回退到正常消息提交，模型看不到 `remember` 工具。
- 参见 [`MEMORY.md`](MEMORY.md) 获取示例和完整的 `/memory` 命令表面。

### 通知

TUI 可以在轮次**成功完成**且耗时超过阈值时发出桌面通知（OSC 9 转义或纯 BEL），因此您可以在长任务运行时切换离开。失败或取消的轮次有意保持静默——通知是"您的任务已准备好"的提示，不是一般的提示音。配置位于 `[notifications]` 下：

```toml
[notifications]
method          = "auto"  # auto | osc9 | bel | off
threshold_secs  = 30      # 仅当轮次耗时 >= 此秒数时通知
include_summary = false   # 在通知正文中包含已用时间和成本
```

方法语义：

- `auto`（默认）——为 `iTerm.app`、`Ghostty` 和 `WezTerm` 选择 `osc9`（通过 `$TERM_PROGRAM` 检测）。在 macOS 和 Linux 上回退到 `bel`。**在 Windows 上回退到 `off`** 而不是 `bel`，因为 Windows 音频栈将 `\x07` 映射到 `SystemAsterisk` / `MB_OK` 提示音——与应用错误弹窗使用相同的声音，因此成功轮次的通知听起来像错误（#583）。
- `osc9`——发出 `\x1b]9;<msg>\x07`。在 tmux 内部，序列被包裹在 DCS 传递中，以便到达外部终端。
- `bel`——发出单个 `\x07` 字节。仅在您主动想要提示音时在 Windows 上使用。
- `off`——完全禁用轮次后通知。

在已知的 OSC-9 终端（例如 Windows 上的 WezTerm）内运行的 Windows 用户仍然可以收到 OSC-9 通知；`off` 回退仅在没有识别到的 `TERM_PROGRAM` 时适用。

### 已解析但当前未使用（为未来版本保留）

这些键被配置加载器接受，但当前没有被交互式 TUI 或内置工具使用：

- `tools_file`

## 功能标志

功能标志位于 `[features]` 表下，并在各个配置文件之间合并。默认对内置工具启用，因此您只需要设置要强制开启或关闭的条目。

```toml
[features]
shell_tool = true
subagents = true
web_search = true # 启用规范的 web.run 加上兼容性 web_search 别名
apply_patch = true
mcp = true
exec_policy = true
```

您还可以针对单次运行覆盖功能：

- `deepseek-tui --enable web_search`
- `deepseek-tui --disable subagents`

使用 `deepseek-tui features list` 检查已知标志及其有效状态。

## 本地媒体附件

在 composer 中使用 `@path/to/file` 向下一条消息添加本地文本文件或目录上下文。使用 `/attach <path>` 添加本地图像/视频媒体路径，或使用 `Ctrl+V` 从剪贴板附加图像。DeepSeek 的公共 Chat Completions API 目前接受文本消息内容，因此媒体附件以显式本地路径引用而非原生图像/视频负载发送。附件行在提交前出现在 composer 上方；移动到 composer 开头，按 `↑` 选择附件行，然后按 `Backspace` 或 `Delete` 将其移除，无需手动编辑占位符文本。

## 托管配置和要求

DeepSeek TUI 支持策略分层模型：

1. 用户配置 + 配置文件 + 环境变量覆盖
2. 托管配置（如存在）
3. 要求验证（如存在）

在 Unix 上默认：
- 托管配置：`/etc/deepseek/managed_config.toml`
- 要求：`/etc/deepseek/requirements.toml`

要求文件结构：

```toml
allowed_approval_policies = ["on-request", "untrusted", "never"]
allowed_sandbox_modes = ["read-only", "workspace-write"]
```

如果配置的值违反要求，启动会失败并显示描述性错误。

有关公式、干预行为和遥测，请参阅 `docs/capacity_controller.md`。

## 关于 `deepseek-tui doctor` 的说明

`deepseek-tui doctor` 遵循与 TUI 其余部分相同的配置解析规则。这意味着 `--config` / `DEEPSEEK_CONFIG_PATH` 被尊重，MCP/技能检查使用解析后的 `mcp_config_path` / `skills_dir`（包括环境变量覆盖）。

要引导缺失的 MCP/技能路径，运行 `deepseek-tui setup --all`。您也可以运行 `deepseek-tui setup --skills --local` 创建工作区本地的 `./skills` 目录。

`deepseek-tui doctor --json` 打印机器可读的报告，跳过实时 API 连接探测。顶层键：`version`、`config_path`、`config_present`、`workspace`、`api_key.source`、`base_url`、`default_text_model`、`mcp`、`skills`、`tools`、`plugins`、`sandbox`、`platform`、`api_connectivity`、`capability`。CI 消费者应依赖 `api_key.source`（`env`/`config`/`missing`），而不是解析人类可读的 `doctor` 文本。

`capability` 键包含从静态知识（发布文档、API 指南）派生的每提供方能力信息，而非实时 API 探测。顶层子键：`resolved_provider`、`resolved_model`、`context_window`、`max_output`、`thinking_supported`、`cache_telemetry_supported` 和 `request_payload_mode`。

在 CI 脚本中使用 `capability.context_window` 和 `capability.max_output` 进行模型限制检查；不要将 `capability.max_output` 视为每轮请求预算。使用 `capability.thinking_supported` 决定是否配置推理强度。

## 设置状态、清理和扩展目录

`deepseek-tui setup` 接受除现有 `--mcp`、`--skills`、`--local`、`--all` 和 `--force` 之外的几个标志：

- `--status` — 打印紧凑的一屏状态（API 密钥、base URL、模型、MCP/技能/工具/插件计数、沙箱、`.env` 存在性）。只读且无需网络；在 CI 中运行安全。如果 `.env` 缺失且 `.env.example` 存在于工作区中，状态输出会指向 `cp .env.example .env`。
- `--tools` — 搭建 `~/.deepseek/tools/`，包含描述自描述 frontmatter 约定的 `README.md`（`# name:` / `# description:` / `# usage:`）和遵循该约定的 `example.sh`。该目录有意不自动加载；通过 MCP、钩子或技能将单个脚本接入代理。
- `--plugins` — 搭建 `~/.deepseek/plugins/`，包含 `README.md` 和使用与 `SKILL.md` 相同 frontmatter 结构的 `example/PLUGIN.md` 占位符。插件也不会自动加载；当您希望它们活动时，从技能或 MCP 包装器中引用它们。
- `--all` 现在同时搭建 MCP + 技能 + 工具 + 插件。
- `--clean` — 列出 `~/.deepseek/sessions/checkpoints/latest.json` 和 `offline_queue.json`（如果存在）。传递 `--force` 实际删除它们。这从不触及真实的会话历史或任务队列。

`--status` 和 `--clean` 与搭建标志互斥。

## 为什么引擎会剥离 XML/`[TOOL_CALL]` 文本

DeepSeek TUI 仅通过 API 工具通道（结构化的 `tool_use` / `tool_call` 项）发送和接收工具调用。`crates/tui/src/core/engine.rs` 中的流式循环识别一组固定的虚假包装器起始标记——`[TOOL_CALL]`、`<deepseek:tool_call`、`<tool_call`、`<invoke `、`<function_calls>`——并将它们从可见的助手文本中清除，而从不将它们转换为结构化的工具调用。当包装器被剥离时，循环每轮发出一个紧凑的 `status` 通知，以便用户可以看到他们的可见文本为何缩减。将任何重新启用基于文本的工具执行的更改视为回归；`crates/tui/tests/protocol_recovery.rs` 中的协议恢复测试锁定了这个契约。