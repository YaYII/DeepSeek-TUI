# 更新日志

本项目所有值得注意的变更都将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
本项目遵循[语义化版本](https://semver.org/lang/zh-CN/)。

## [0.8.20] - 2026-05-08

### 修复
- **中文思考保持中文** - 在确定性环境提示使其退化后，恢复了 #588 的语言合约。最新用户消息现在为 `reasoning_content` 和最终回复选择自然语言；解析后的 `lang` 字段仅当用户轮次不明确时作为回退。

## [0.8.19] - 2026-05-08

### 修复
- **DeepSeek beta 端点保持为中国地区默认** - 旧的 `deepseek-cn` 运行时路径不再将用户路由到非 beta 的 `https://api.deepseek.com` base URL。它现在作为常规 `deepseek` 提供方默认值 `https://api.deepseek.com/beta` 的向后兼容别名，因此严格工具模式和其他 beta 门控功能在全球范围内保持可用。
- **提供商文档停止宣传 `deepseek-cn` 作为独立提供商** - 运行时文档现在仅将其描述为遗留配置别名。DeepSeek 在全球使用相同的官方主机；使用私有镜像的用户应显式设置 `base_url`。

## [0.8.18] - 2026-05-07

这是 v0.8.17 的后续发布：更严密的 TUI/运行时/安装改进，具有更安全的会话启动语义，Docker 镜像升级为受支持的安装路径，以及几个社区 PR 被摘入发布分支。VS Code 和飞书/移动端配套工作不在本发布范围内。

### 新增
- **GHCR 上的预构建 Docker 镜像** - 发布构建现在发布 `ghcr.io/hmbown/deepseek-tui`，带有 `latest`、语义版本和 `vX.Y.Z` 标签，GitHub 发布说明包含 Docker 安装片段。Docker 发布现在是一个发布门禁，而不是尽力而为的检查。
- **可拖拽的记录滚动条**（#1075、#1076）- 启用鼠标捕获后，可以拖拽记录滚动条浏览长会话。实现还在调整大小和新点击时清除旧拖拽状态。感谢 @Oliver-ZPLiu。
- **视口漂移的 PTY 回归测试**（#1085）- QA 测试工具现在覆盖失败/长轮次后的顶部空行问题，使未来的布局更改能捕获终端视口漂移。

### 变更
- **直接运行 `deepseek` 启动新会话** - 在同一个文件夹中打开第二个 `deepseek` 不再静默附加到同一个飞行中的检查点。崩溃/中断的检查点保留为已保存会话，并通过 `deepseek --continue` 显式恢复。
- **npm postinstall 对临时下载失败可恢复**（#1059）- 安装时的 GitHub 下载/解压错误是非阻塞性的并有文档记录，而不支持的平台、校验和不匹配、glibc 预检失败和运行时包装器失败仍然是致命错误。感谢 @Fire-dtx。
- **Docker Buildx cargo 缓存按平台隔离并锁定** - 注册表、git 和目标缓存现在使用平台特定的缓存 ID 加锁定共享，以避免发布检查中的 `.cargo-ok File exists` 解压竞争。
- **长会话调色板更易读**（#1070、#936 部分）- 默认正文文本略微柔和，推理/思考文本使用更暖的强调色，`/theme` 现在更新终端颜色适配器，使浅色模式在会话内切换后保持这些对比度一致。感谢 @bevis-wong 和 @oooyuy92 的可读性报告。
- **安装文档添加第二个 rustup 镜像回退**（#1011）- `rsproxy.cn` 被记录为替代 rustup 镜像，旧的 Debian/Ubuntu Cargo `edition2024` 失败现在引导用户使用 rustup stable。感谢 @wuwuzhijing。

### 修复
- **中文破坏性审批对话框保留显式风险措辞**（#1087、#1091）- zh-Hans 破坏性审批文案现在本地化操作标签、标题、提示和破坏性风险警告，而不改变英文默认行为。感谢 @qinxianyuzou 和 @axobase001。
- **重绘前重置终端视口**（#1085）- TUI 现在在恢复、调整大小和轮次完成后的关键重绘前清除滚动边界/origin 模式，防止备用屏幕内容向下漂移并在顶部留下空行。
- **交互式子进程等待终端释放**（#1085）- shell/编辑器交接现在等待 UI 实际离开备用屏幕/原始模式后再启动子进程，防止交互式工具使用后 TUI 重绘到主机回滚中。
- **浅色主题推理块保持浅色**（#1070、#936 部分）- 思考/推理背景色调现在映射到浅色推理表面，而不是在 `/theme light` 后保持深色模式色调。
- **FreeBSD 可以编译 secrets crate**（#1089）- 没有原生 `keyring` 依赖的平台现在干净地失败 OS 密钥环探测并回退到文件支持的安全存储，而不是引用缺失的 crate。感谢 @avysk 的 FreeBSD 报告。
- **Windows 沙箱文档不再夸大保证**（#1015、#1058）- 文档和代码注释现在将未来的 Windows 辅助功能描述为仅进程树包含，直到文件系统、网络、注册表或 AppContainer 隔离实际实现。感谢 @axobase001。

## [0.8.17] - 2026-05-07

一个专注于可靠性的发布版本，几乎完全由社区贡献构建。修复了 Plan 模式安全、粘贴-回车自动提交、斜杠菜单技能覆盖、`deepseek-cn` 端点预设以及一些平台/流式/网关兼容性问题。还引入了一个小的 PTY 驱动的 QA 测试工具，使下一轮 TUI 修复可以针对真实终端行为进行验证。

### 新增
- **`/theme` 命令**（#1057）— 内联切换深色和浅色主题，无需通过 `/config` 往返。感谢 @MengZ-super。
- **PTY/帧捕获 TUI QA 测试工具** — 新的 `crates/tui/tests/support/qa_harness/` 让集成测试可以在真实伪终端中生成 `deepseek-tui`，发送脚本化的按键/粘贴/调整大小，并根据解析的终端帧加上工作区文件系统进行断言。初始场景覆盖启动冒烟测试和 #1073 粘贴回归。添加场景的指南在 `crates/tui/tests/support/qa_harness/README.md` 中。
- **Whalescale 桌面运行时桥接** — 本地运行时 API 现在暴露 `POST /v1/approvals/{id}`、`GET /v1/runtime/info`、`GET /v1/skills` 上的 `enabled` 标志以及 `POST /v1/skills/{name}` 开关。运行时线程事件还携带 `agent_reasoning` 项目，使桌面客户端可以分别渲染思考内容和助手文本。

### 变更
- **`deepseek-cn` 提供商预设现在默认为官方 `https://api.deepseek.com` 主机**（#1079、#1084）— 匹配 [api-docs.deepseek.com](https://api-docs.deepseek.com/)。旧的拼写错误主机 `api.deepseeki.com` 仍在 URL 启发式规则和聊天客户端规范化中被识别，以便现有用户配置继续工作。感谢 @Jefsky。
- **Plan 模式在只读沙箱中运行 shell 命令**（#1077）— 之前是 `WorkspaceWrite`，将工作区作为可写根目录，这允许 `python -c "open('f','w').write('x')"` 在工作区内部修改文件。现在使用 `SandboxPolicy::ReadOnly`：文件系统上没有写入权限，没有网络。只读检查命令（`ls`、`git log`、`grep`、`cargo metadata` 等）继续通过各平台沙箱工作；对于创建或修改文件的操作，请切换到 Agent 模式（`/agent`）。感谢 @DI-HUO-MING-YI。

### 修复
- **粘贴多行文本并带有尾随换行符不再自动提交**（#1073）— composer 的 Enter 处理程序现在查询粘贴突发抑制状态，并将 `\n` 追加到飞行中的突发缓冲区或直接插入 composer 文本中，而不是回退到 `submit_input()`。从原始 Windows / PowerShell 症状复现；修复覆盖了 bracketed-paste 和快速按键检测路径。感谢 @bevis-wong 的精确复现。
- **斜杠菜单、`/skills` 和 `/skill <name>` 显示项目本地 AND 全局技能**（#1068、#1083）— 将缓存切换到 `discover_in_workspace`，使 UI 表面与系统提示技能块保持同步。额外修复：`SKILL.md` frontmatter 值现在去除周围的 YAML 引号，因此 `name: "hud"` 注册为 `hud` 并匹配前缀查找。感谢 @AlphaGogoo / @Duducoco。
- **Windows shell 输出在非 UTF-8 系统代码页上仍解码为 UTF-8**（#982、#1018）— Windows shell 命令现在使用 `chcp 65001 >NUL & ` 包装，使子进程输出 UTF-8 而非 GBK 或其他 ANSI 代码页。`display_command` 去除前缀，使记录和审批提示保持整洁。感谢 @chnjames。
- **启动时清理过期的快照 `tmp_pack_*` 文件**（#975、#1055）— 中断的 side-repo git pack 操作不再泄漏孤立的临时文件；`prune_unreachable_objects` 在常规修剪周期中运行，以删除回滚快照中的松散对象。解决了约 30 GB+ 的磁盘使用报告。感谢 @axobase001。
- **macOS Terminal.app 和 Windows ConHost 上的窗口调整大小残留问题已解决**（#993）— 在调整大小后的绘制期间强制使用调整大小事件的大小，这样 ratatui 的内部 `autoresize()` 无法将视口收缩回过期维度，让新扩展的区域充满过时内容。与 #582 同类，针对额外的模拟器系列。感谢 @ArronAI007。
- **流式思考块在流错误和重启时干净地终结**（#861 部分、#1078）— 引擎错误处理器现在将飞行中的思考块排入记录，而不是将部分推理留在 `StreamingState` 中成为孤儿。重构将思考生命周期提取到命名辅助函数中（`start_streaming_thinking_block`、`finalize_current_streaming_thinking`、`stash_reasoning_buffer_into_last_reasoning`）。感谢 @reidliu41。
- **OpenRouter 和其他自定义端点提供商保留显式模型 ID**（#1066）— 当提供商具有显式模型 AND 自定义 `base_url`（不同于提供商默认值）时，模型名称不再被提供商特定的规范化重写。让 OpenAI 兼容网关接受裸 ID，如 `deepseek/deepseek-v4-pro`、`accounts/fireworks/models/...` 或 `glm-5`。感谢 @THINKER-ONLY。
- **自动生成的 `.deepseek/instructions.md` 稳定 KV 前缀缓存**（#1080）— 替换了 `prompts.rs` 中每轮文件系统扫描的回退，当没有上下文文件存在时创建一个真实的磁盘工件，使系统提示的前缀在轮次间保持字节稳定，从而提高前缀缓存命中率。自动生成的文件有明显标签，用户可以自由编辑或删除。感谢 @lloydzhou。
- **压缩网关后的 SSE 响应正确解码**（#1061）— 启用 reqwest 的 `gzip` 和 `brotli` 功能，使通过压缩响应的代理的流式数据能正常通过，而不是作为协议损坏。静默了某些"卡在 working"报告背后的一个失败模式。感谢 @MengZ-super。
- **NVIDIA NIM 提供商配置使用自己的 API 密钥，即使存在旧的根 DeepSeek 密钥**（#1081）— `[providers.nvidia_nim] api_key` 现在对 NIM 请求获胜，避免了可能因意外发送顶层 DeepSeek 凭据到 NVIDIA 而导致 401 错误。感谢 @wlon 的精准诊断。
- **npm 安装说明在 GitHub Releases 被屏蔽时解释发布镜像逃生路径**（#1051、#1056）— 网络/DNS 失败现在指向现有的 `DEEPSEEK_TUI_RELEASE_BASE_URL` 覆盖和所需的校验和清单/二进制布局，而不是停留在原始的 `ENOTFOUND github.com`。感谢 @axobase001。

### 贡献者须知

本发布改变了项目的 PR 处理理念：每项贡献都有其价值所在；维护者的工作是找到它、使用它并感谢贡献者——绝不要在未采纳任何内容的情况下关闭 PR。如果 PR 过大或范围混杂而无法完整合并，直接摘取有用的提交/文件/创意，而不是要求贡献者拆分。凭据、沙箱、提供商、发布、遥测、赞助、品牌和全局提示的信任边界仍然需要维护者显式签署，但达到这一点的责任在我们这里。详见 `AGENTS.md`。

## [0.8.16] - 2026-05-07

一个针对 v0.8.15 在 RLM、子代理可见性和终端所有权方面的回归问题的热修复版本。此版本保持 v0.8.15 的功能集完整，同时使长时间运行的委派工作更易于检查和更安全地运行。

### 变更
- **RLM 没有固定的 180 秒墙上时钟超时**（#955）— RLM 轮次可以在旧硬限制之后继续，只要长输入 REPL 仍在取得进展。
- **RLM 输出更易于审计**（#955）— 最终报告现在包括紧凑的执行元数据：输入大小、迭代次数、已用时间、子 LLM RPC 计数和终止状态。
- **RLM 分块指南对精确工作更严格**（#955）— 提示现在告诉子代理对计数/聚合使用确定性 Python 处理完整 `context`，并在拆分整个输入时报告块覆盖范围。
- **工具指南不再那么防御性**（#955）— 系统提示现在解释何时使用工具，而不是劝阻模型使用实际可用的能力。

### 修复
- **活动的 RLM 工作保持可见**（#955）— 前台 RLM 调用在活动任务/右侧面板状态中显示，而不是让任务面板显示"无活动任务"。
- **`/subagents` 不再报告虚假的空状态**（#955）— 子代理覆盖层现在在管理器缓存尚未刷新时包括实时的仅进度代理和记录扇出工作者。
- **子代理卡片更安静且更有用**（#955）— 低信号调度器行如 `step 1/100: requesting model response` 被隐藏，而紧凑的工具活动保持可见。
- **子代理完成协议保持内部**（#955）— 完成标记作为内部运行时事件而不是用户消息路由，因此父代理不会向用户解释原始的协议 XML。
- **子代理不能接管父终端**（#955）— 后台代理拒绝带有 `interactive=true` 的 `exec_shell`；它们仍可以使用非交互式 shell、后台 shell、`tty=true` 和任务 shell 工具。
- **终端回滚所有权已恢复**（#955）— TUI 在前台/子代理工作清空后重新进入备用屏幕模式，防止主机终端滚动条接管实时界面。

## [0.8.15] - 2026-05-06

一个认证、Windows、编辑器集成和设置稳定性发布。此版本保持现有的 DeepSeek V4 架构完整，同时落地使首次运行设置、终端行为、技能、成本显示和恢复路径更可信赖的小型社区修复。

### 新增
- **面向 Zed/自定义代理的 ACP stdio 适配器**（#782）— `deepseek serve --acp` 通过 stdio 启动本地 Agent Client Protocol 服务器。第一个切片支持通过用户现有的 DeepSeek 配置/API 密钥创建新会话和提示响应；工具支持的编辑和检查点重放暂时不在 ACP 表面范围内。
- **人民币/CNY 成本显示**（#806）— `cost_currency = "cny"`（也接受 `yuan` / `rmb`）将底部面板、上下文面板、`/cost`、`/tokens` 和长轮次通知摘要从 USD 切换为 CNY。
- **技能的斜杠自动补全**（#808）— 已安装的技能在斜杠命令自动补全菜单中可见。
- **`/rename` 会话标题**（#836）— 无需手动编辑保存文件即可重命名会话。

### 变更
- **轮次元数据中的当前本地日期**（#893，关闭 #865）— 真实用户轮次现在在 `<turn_meta>` 中包含当前本地日期，而不更改稳定的系统提示/缓存前缀。
- **Doctor 端点诊断**（#823）— `deepseek doctor` 显示解析后的提供商/API 端点，使代理、中国端点和继承环境调试更具体。
- **更保守的请求大小**（#826）— API 请求在分发前将 `max_tokens` 与活动模型/上下文预算对齐。
- **更安全的配置和密钥文件写入**（#833、#837）— 生成的配置文件使用限制性权限和改进的密钥编辑。

### 修复
- **仅环境变量 API 密钥失败恢复**（#892）— 运行时认证失败现在说明拒绝的密钥来自继承的 `DEEPSEEK_API_KEY` 且没有已保存的配置密钥，与更清晰的 `deepseek doctor` 指导匹配。
- **Windows Unicode 输出**（#887，关闭 #872）— TUI 启动现在尽力将 Windows 控制台输入/输出代码页切换到 UTF-8，改善中文和其他非 ASCII 渲染。
- **Windows 恢复选择器**（#886，关闭 #866）— 调度器在 Windows 上保留恢复选择器路径，而不是绕过它。
- **Windows 剪贴板回退**（#850）— 复制操作在主剪贴板后端不可用时具有回退路径。
- **工作区信任持久化**（#870）— 审批/信任选择在全局配置中持久化，而不是在下次启动时让用户惊讶。
- **Ctrl+E composer 行为**（#883，关闭 #876）— 纯 Ctrl+E 再次移动到 composer 末尾；文件树切换移到 Shift 快捷键。
- **纯 Markdown 技能**（#869）— 没有 frontmatter 的 `SKILL.md` 文件现在回退到第一个 `# Heading` 而不是被忽略。
- **工作区范围的最新恢复**（#830，关闭 #779）— `resume --last`、`--continue` 和 fork/resume 辅助函数选择当前工作区/仓库的最新会话，而不是全局最新保存的会话。
- **Npm 包装器版本回退**（#885）— `deepseek --version` / `-v` 可以在原生二进制尚未下载时报告包版本。
- **TUI 退出恢复提示**（#863，关闭 #682）— 退出 TUI 现在引导用户使用相关的恢复命令。
- **启动和终端可靠性** — 包括有界的流打开等待（#847）、`@` 提及的光标延迟减少（#849）、SSH 的 OSC52 剪贴板回退（#845）、旧版 Ctrl+V 粘贴识别（#786）、Windows 鼠标捕获默认关闭（#785）和保留 UTF-8 的 ANSI 剥离（#784）。
- **安装和策略可靠性** — 避免不稳定的 Rust 文件锁定 API（#821）、在 `web_run` 中强制执行网络策略（#800）、修复 API 密钥设置后的重复设置语言提示（#844），以及解释调度器 TUI 生成失败（#853）。
- **工作区安全** — 拒绝 `$HOME` 或不安全工作区的危险快照（#798、#804）、修复名称中双点的路径逃逸误报（#824）、限定快照内置排除项（#854），以及将提供商 `unreachable!()` 路径替换为正确错误（#835）。
- **技能发现** — 递归读取技能目录（#811）、忽略选定安装根目录外的符号链接（#814）、发现全局 Agents 技能（#848），并包括 `.cursor/skills`（#817）。
- **提供商/模型兼容性** — 恢复自动模型路由（#772）、完成 vLLM 提供商集成（#737）、接受提供商前缀的 DeepSeek 模型 ID（#794）、保留请求的模型 ID 大小写（#733），以及将 RLM 子调用固定到 Flash（#832）。

### 致谢
- 感谢 [@reidliu41](https://github.com/reidliu41) 对恢复提示和工作区信任的修复（#863、#870）。
- 感谢 [@Oliver-ZPLiu](https://github.com/Oliver-ZPLiu) 对 Windows 剪贴板回退的贡献（#850）。
- 感谢 [@xieshutao](https://github.com/xieshutao) 对纯 Markdown 技能回退的贡献（#869）。
- 感谢 [@GK012](https://github.com/GK012) 对 npm 包装器版本回退的贡献（#885）。
- 感谢所有提交 Windows、中文设置、认证和首次运行报告的用户。这些具体的复现塑造了本发布。

## [0.8.13] - 2026-05-05

DeepSeek V4 运行时和 TUI 可靠性的稳定版发布。v0.8.13 里程碑范围缩小到直接的运行时/TUI 修复；提示卫生、轨迹日志、Anthropic 线协议支持和更大的 UI 清理被移出本发布。

### 新增
- **压缩前的无 LLM 工具结果修剪**（#710）— 旧的冗长工具结果在付费总结步骤前被机械总结。重复读取保留最新的完整正文，并将旧副本替换为一行摘要；如果这将会话带回到压缩阈值以下，则完全跳过 LLM 总结调用。
- **重复工具反循环保护**（#714）— 引擎现在每用户轮次跟踪 `(tool_name, args)` 对。在第三次相同调用时，它插入合成的修正工具结果，而不是再次运行相同的工具不变；每工具失败在三次时警告，在八次时停止。
- **V4 缓存命中遥测回退**（#721）— 用量解析现在识别 `usage.prompt_tokens_details.cached_tokens`，因此现有的底部面板缓存命中芯片可以使用 DeepSeek V4 的自动前缀缓存遥测以及旧的显式命中/未命中字段。

### 修复
- **无效工具调用 JSON 修复**（#712）— 格式错误的流式工具参数现在在分发前通过确定性修复阶梯。
- **幻觉工具名称恢复**（#713）— 常见的非规范工具名称在引擎报告缺失工具前通过注册表解析。
- **工具模式清理**（#715）— 模式在 API 发送前规范化，使提供商严格的 JSON Schema 处理不会拒绝有效工具。
- **大小写敏感的模型 ID**（#717、#729）— 有效的配置模型 ID 保持调用方提供的大小写，而紧凑的 DeepSeek 别名仍然规范化。
- **分发失败后的过期 `working...` 状态**（#738）— 如果 UI 在轮次开始前未能向引擎发送消息，composer 加载状态被清除，而不是将后续输入困在 pending 状态。
- **无提示的 doctor 密钥检查** — `deepseek doctor` 不再读取 OS 密钥环，避免了诊断期间的 macOS 密钥链提示。
- **macOS 终端颜色兼容性** — `xterm-256color` 会话现在接收 256 色调色板索引而不是真彩色 SGR，防止 Apple Terminal 将鲸鱼蓝渲染为绿色/青色块。
- **Responses 清理后的聊天客户端修复** — 在移除已废弃的实验性 Responses 回退路径后，恢复了聊天客户端正文和回归测试覆盖。
- **composer 为空时 Up/Down 箭头滚动记录** — 裸 Up/Down 箭头现在在 composer 输入为空（或仅空白）时滚动记录；有文本时它们仍然导航 composer 历史。以前该门控硬编码为 false，使虚拟终端（Ghostty、Codex、Kitty 协议）中无法使用修改键快捷键滚动的用户陷入困境。

## [0.8.11] - 2026-05-04

### 变更
- **DeepSeek V4 的缓存最大化提示路径** — 引擎现在在组装的稳定提示不变时跳过系统提示重新分配，将易变的仓库工作集摘要从系统提示中移除，并作为每轮元数据注入到最新用户消息中。
- **工具目录缓存锚点** — 模型可见的工具数组现在用 `cache_control: ephemeral` 标记最终的本地工具，使 DeepSeek 可以显式锚定稳定的工具前缀。
- **V4 规模的自动压缩默认值** — 自动压缩保持 500K token 的硬地板和回退压缩阈值现在反映 V4 规模的延迟触发策略，而不是旧的 50K 时代默认值。
- **仅 token 压缩触发器** — 消息计数压缩触发器是一个 128K 时代的启发式规则，会在小消息的长会话上触发——正是重写 V4 前缀缓存最浪费的情况。移除了 `CompactionConfig::message_threshold` 和 `should_compact` 中的消息计数分支；token 预算现在是唯一的自动触发器（由 500K 地板门控）。手动 `/compact` 不变。

### 修复
- **旧版 128K 上下文命名** — 128K 回退现在被命名并记录为仅旧版 DeepSeek 行为，减少了与 1M token DeepSeek V4 默认值的歧义。
- **`npm install` 对慢速/防火墙网络的弹性** — 来自 GitHub Releases 的 postinstall 二进制获取现在在瞬时错误时重试（5 次尝试，1-16 秒指数退避加抖动），强制每次尝试超时（默认 5 分钟，可通过 `DEEPSEEK_TUI_DOWNLOAD_TIMEOUT_MS` 配置）加 30 秒停滞检测器，尊重 `HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY` 环境变量（纯 Node CONNECT 隧道，无新依赖），并打印下载进度行到 stderr，让用户知道没有挂起。可通过 `DEEPSEEK_TUI_QUIET_INSTALL=1` 抑制。由来自中国的社区用户报告，其通过 CN npm 镜像的安装花了 18 分钟——瓶颈是 GitHub 获取，CN npm 镜像不代理该操作。
- **YOLO 沙箱降级为 DangerFullAccess** — YOLO 模式之前仍通过 WorkspaceWrite 沙箱路由 shell 命令，这拦截了合法的工作区外写入（包安装、子代理工作区、`~/.cache`、brew、`npm install -g`、pipx）并强制审批往返——与"无护栏"契约矛盾。YOLO 已自动批准所有工具并启用信任模式；沙箱是最后的残留限制。现在使用 DangerFullAccess（无沙箱），与完整的 YOLO 姿态一致。
- **跨渲染解析保持滚动位置锁定** — 实时流式传输期间用户向上滚动在下一个块时被拉到实时尾部。`user_scrolled_during_stream` 锁在内容短暂适合一个屏幕时被过早清除，或在记录在渲染之间收缩时（例如子代理卡片折叠）。通过快照 `resolve_top` 前的先前尾部状态修复，仅当用户有意在底部时才清除锁。
- **容量控制器默认禁用** — 容量控制器基于 slack 的 `p_fail` 计算安静地清除记录（`messages.clear()`），独立于 token 利用率或 `auto_compact` 设置。这与 v0.8.11 默认的 `auto_compact = false` 矛盾——用户选择信任完整的 1M token V4 窗口，而控制器代为管理前缀。控制器现在默认为 `enabled = false`；高级用户可以通过 `capacity.enabled = true` 加入。

### 文档
- **README 清晰度改进**（#685）— 标题大小写章节标题、`npm install -g` 片段前的显式 Node + npm 前提条件块、中国友好的 `--registry=https://registry.npmmirror.com` 安装变体、用于 AI 辅助仓库浏览的 DeepWiki 徽章以及标题上的 🐳 标记。*感谢 [@Agent-Skill-007](https://github.com/Agent-Skill-007) 提交此 PR。*

## [0.8.12] - 2026-05-05

基于 v0.8.11 缓存最大化基础的功能发布：合并了 20 个社区 PR，涵盖推理强度自动化、V4 FIM 编辑、bash 参数执行策略、技能注册表同步、vim composer 模式、大工具输出路由、可插拔沙箱后端、分层权限规则集和缓存感知常驻子代理。无破坏性更改。

### 新增
- **推理强度自动模式**（#669）— `reasoning_effort = "auto"` 检查最新用户消息中的关键词（debug/error → Max、search/lookup → Low、default → High）并在每次 API 请求前解析档位。子代理始终使用 Low。
- **面向 V4 /beta 的 FIM 编辑工具**（#668）— `fim_edit` 工具向 DeepSeek 的 `/beta` 端点发送 fill-in-the-middle 请求，用于外科手术式代码编辑。
- **Bash 参数词典**（#655）— `auto_allow = ["git status"]` 现在匹配 `git status -s` 但不匹配 `git push`。参数词典了解 git、cargo、npm、yarn、pnpm、docker、kubectl、aws、make 等的命令结构。未列出的命令仍使用旧的平面前缀匹配。
- **统一斜杠命令命名空间**（#661）— `~/.deepseek/commands/` 中的用户定义命令支持 `$1`、`$2`、`$ARGUMENTS` 模板替换。用户命令覆盖内置命令。
- **技能注册表同步**（#654）— `/skills sync` 获取社区技能注册表并安装/更新所有列出的技能。受现有 `[network]` 策略网络门控。
- **Composer 中的 Vim 模态编辑**（#659）— `vim.insert_mode` / `vim.normal_mode` 设置在消息 composer 中启用模态编辑，支持标准 Vim 键绑定。
- **独立的 tui.toml**（#657）— 主题颜色和键绑定覆盖可以放在 `~/.deepseek/tui.toml` 中，与主 `config.toml` 并列。*注意：文件格式已定义但启动时尚未加载——接线推迟到 v0.8.13。*
- **大工具输出路由**（#658）— 超过可配置 token 阈值的工具结果通过工作坊路由，带有截断预览，保护父上下文窗口。综合目前仅为截断；V4-Flash 子代理综合推迟到后续版本。
- **可插拔沙箱后端**（#645）— `SandboxBackend` 特性和阿里云 OpenSandbox HTTP 适配器让 `exec_shell` 将命令路由到远程沙箱，而不是本地生成。配置键：`sandbox_backend`、`sandbox_url`、`sandbox_api_key`。
- **分层权限规则集**（#653）— `ExecPolicyEngine` 支持内置、代理和用户优先级层，用于允许/拒绝前缀规则。拒绝始终获胜语义。
- **缓存感知常驻子代理**（#660）— 使用 `resident_file` 生成的子代理将文件内容前置到其系统前缀中，以实现 V4 前缀缓存局部性。全局租约表防止两个代理同时持有同一文件的常驻租约。代理完成时释放租约。
- **上下文限制交接**（#667）— 引擎级支持，当上下文压力触发时用 `.deepseek/handoff.md` 文件写入替换常规压缩。*注意：配置旋钮在实现前已移除。*
- **LSP 自动附加诊断**（#656）— 编辑结果现在通过引擎级 LSP 钩子路径包括编辑后诊断。

### 文档
- **README 安装部分重写**（#672）— 之前的标题声称"不需要 Node.js 或 Python 运行时"，但紧接着告诉读者在继续之前安装 Node。替换为三路径安装块（npm / cargo / 直接下载），明确说明 npm 包装器的角色：它下载预构建二进制文件，但 `deepseek` 本身在运行时不依赖 Node。中文 README 同步更新。
- **Windows Scoop 安装说明**（#696）— README 和中文 README 现在为 Windows 用户记录了 `scoop install deepseek-tui`。*感谢 [@woyxiang](https://github.com/woyxiang) 提交此 PR。*
- **DeepSeek Pro 折扣窗口延长**（#692）— 定价脚注从 2026 年 5 月 5 日更新到 2026 年 5 月 31 日，以匹配平台侧促销。*感谢 [@wangfeng](mailto:wangfengcsu@qq.com) 提交此 PR。*
- **`deepseek resume <SESSION_ID>` 在用法中展示** — 该命令自 v0.7 以来存在但未文档化。通过 #682 报告。
- **SECURITY.md**（#648）— 漏洞报告策略和支持版本。
- **CODE_OF_CONDUCT.md**（#686）— Contributor Covenant v2.1。*感谢 [@zichen0116](https://github.com/zichen0116) 提交此 PR。*
- **zh-Hans 语言环境激活文档**（#652）— README.zh-CN.md 和 config.example.toml 现在记录了 `locale = "zh-Hans"`。

### 修复
- **跨工作区会话泄漏（安全）** — 从任何目录启动 `deepseek` 静默自动恢复最近中断的会话，即使该会话源自完全不同的工作区。然后工具在前一个工作区的文件路径上操作，而状态栏显示*当前*工作区名称——这是一个令人困惑的信任边界违规，可能将前一个会话积累的 `api_messages`、`working_set` 条目和任何秘密泄漏到从未打算看到它们的新终端中。`try_recover_checkpoint()` 现在将已保存会话的工作区与 `std::env::current_dir()` 比较（规范化，规范化失败时使用严格相等回退），仅在匹配时自动恢复。不匹配时，检查点保留为常规会话（因此用户可以通过 `deepseek sessions` / `deepseek resume` 找到它），而不是静默泄漏到不同的工作区。
- **空闲进程上的 SIGTSTP 崩溃** — 在 `deepseek` 空闲（等待输入）时按 Ctrl+Z 并运行 `bg`、`fg` 或其他作业控制命令会导致段错误。`app.rs` 中的 `wait_for_composer_input` 路径在 SIGTSTP 后在 `[read].read_line()` 上返回 `Ok(0)`（EOF），然后导致未初始化的状态访问。现在检测 `read_line` 返回 0 并跳过该轮次的输入处理。由 #674 报告。
- **Ctrl+C 在批处理 shell 命令退出后没有返回到 TUI** — 在子进程退出后立即按 Ctrl+C 被 `signal_hook` 在 `exec_shell` 中吞噬，使用户在备用屏幕重新激活前按两次。修复：只对跨子进程的 `interactive` shell 命令安装信号钩子；非交互式命令直接等待子进程退出而不安装信号处理器。由 #673 和 #670 报告。
- **`/compact` 移除后助手消息丢失** — 压缩保留了用户消息和系统提示，但丢弃了所有不包含工具结果或推理块的纯助手消息。在压缩前有用户消息→助手消息→工具结果序列的会话中，助手消息被丢弃，工具结果作为孤立的后续出现——冻结模型无法继续。`compact_messages` 现在保留用户和助手消息之间的一对一关系，并发出相关的跟踪日志。由 #675 报告。
- **Windows 英文 README 标签** — 项目描述中错误使用了 "终端编码" 而非 "编码智能体"。
- **`cargo test` 在 `-p deepseek-cli` 上** — CLI tests/ 目录在 Cargo.toml 中被列出但不存在。移除了测试目标声明。由 #684 报告。

### 性能
- **会话管理器中移除了冗余的 `serde_json::from_slice`** — `SessionManager` 中的 `get_session` 对每个会话路径执行了双反序列化（一个用于获取 session_id，另一个用于完整解析）。将数据库路径上的操作合并为单个反序列化调用。先前使用 `serde_json::from_slice` 的 DurableTask 和 CachedSkills 路径未受影响。测量：v0.8.10 每个会话加载约 900μs → 约 50μs。

### 工程
- **提升 `unreachable` 模式以鼓励 RUSTSEC-2024-0399 缓解** — `clippy.toml` 中的 `allow-exhaustive-unreachable-patterns` 允许未命名的 wild 模式用于 `features` 和 `models` 匹配。将 `#[expect(unreachable_patterns)]` 替换为 `#[allow(unreachable_patterns)]` 匹配，使得将 `features.rs` / `models.rs` 中的枚举变体重命名或移除不会在没有 `deny` 警告的情况下默默通过。
- **`atty` 依赖移除** — `atty` crate（RUSTSEC-2024-0371，未维护/悬空）被 std 的 `std::io::IsTerminal` 特性替换。

## [0.8.10] - 2026-05-04

一个补丁发布：热修复、小型 UX 改进和四个解除 Whalescale 阻塞的运行时 API 新增。无破坏性更改。

### 新增
- **`GET /v1/usage` 端点**（#564）— 跨线程的 token/成本聚合，支持 `since`/`until`/`group_by`。使用情况面板和开销概览。见 `docs/RUNTIME_API.md`。
- **`PATCH /v1/threads/{id}` 端点**（#562）— 线程属性的运行时更新（`archived`、 `allow_shell`、`trust_mode`、`auto_approve`、`model`、`mode`、`title`、`system_prompt`）。所有字段可选；标题和系统提示接受空字符串以清除。这是 Whalescale 桌面运行时桥接所需的。
- **CORS 来源配置**（#561）— `--cors-origin` CLI 标志（可重复）、`DEEPSEEK_CORS_ORIGINS` 环境变量和 `[runtime_api] cors_origins` 配置项允许额外的开发来源。内置默认值：`localhost:3000`、`localhost:1420`、`tauri://localhost`。用户来源在默认值之上堆叠，不替换它们。无通配符。
- **`v1/runtime/info` API 端点**（#560）— 运行时能力自省，用于执行摘要和对齐检测。见 `docs/RUNTIME_API.md`。
- **`archived_only=true` 线程筛选**（#563）— `GET /v1/threads` 现在接受 `archived_only=true`（覆盖 `include_archived`），使 Whalescale 桌面归档视图可以拉取严格的已归档列表。

### 变更
- **`sed` 在 macOS 上成功退出**（#536，关闭 #527）— 在 macOS 上运行 `sed -i 's///g'` 在一个不存在的文件上产生 `exit 0`，因此非零退出码不是错误检测的可靠信号。将文档从强调 `exit code != 0` 转移到读取和解析 stderr。
- **`/accept` 现在接受第一轮**（#549）— 以前 `/accept` 跳过第一轮并在第二轮开始接受，使消息在首次使用时似乎"丢失"。修复来自 `turn_id` 偏移错误。
- **`/tokens` 输出对齐和数字格式化**（#551）— 表格渲染现在将整数右对齐，对较长的用户/系统 token 计数添加千位分隔符，并修复缓存行中 `n/a` 的对齐。
- **Windows 滚动恢复**（#552）— `cmd.exe` 在 Windows 上的 `MODE CON` 输出解析在 `COLUMNS=` 之前有一个前导空格；`parse_mode_con` 在拆分前修剪该行。在 #550 中恢复。
- **`--trace` / `DEEPSEEK_LOG_LEVEL=trace` 的 HTTP 流量日志**（#539）— `tracing-log` 标志从 `info` 提升到 `trace`，以便在正常调试级别下请求/响应摘要不再淹没日志。已记录的 `--trace` 警告更新为提及 `DEEPSEEK_LOG_LEVEL`。

### 修复
- **V4 自动补全问题**（#548）— `/` 和 `@` 自动补全弹出在空匹配时溢出。添加了空匹配保护。
- **滚动时选择泄漏**（#555）— 当用户向上滚动然后开始新选择时，起始锚点可能是一个屏幕外位置，导致选择区域泄漏到视口外。选择锚点现在在开始新选择时重新计算，作为屏幕可见行的上限。
- **文本在面板之间渗漏**（#557）— 侧边栏/记录垂直分割的调整大小处理程序在迭代面板索引时使用了有符号/无符号不匹配。UI 线程不再在动画期间并发调整大小。
- **正则表达式性能退化**（#566）— `regex` crate 的自动加速有时会将 `^` 锚定的模式转换为极慢的线性扫描。模式在编译前用 `(?-u:...)` 包裹以禁用 Unicode 感知优化，从而启用 DFA 加速。

## [0.8.8] - 2026-05-03

规模最大的 DeepSeek TUI 发布——一次架构上的跨领域工作，为 OpenCode 共享基础设施、agent 式副驾驶 UX、应用服务器遥测、Web UI、VS Code 扩展以及 23 个延续问题奠定了基础，全部采用 git worktree 隔离开发。

源代码树增加了 `crates/app-server/`（HTTP/SSE 运行时 API）、`crates/tools/`（类型化工具生命周期）、`crates/execpolicy/`（审批/沙箱策略引擎）和 `crates/agent/`（模型/提供商注册表）。总代码行数：~32,000 新增。

**前期注意事项**：这是一个超大版本，我们打破惯例提前发布此变更日志条目，以便社区了解共享工具基础设施（`crates/tools/`、`crates/execpolicy/`）和运行时 API（`crates/app-server/`）的进展，这些是 v0.8.9 计划中的全部需要。

本版本的架构变更摘要：`docs/ARCHITECTURE.md`。

### 新增（用户可见）

#### 配置和本地化
- **项目级配置覆盖**（#485）— `./.deepseek/config.toml` 合并到 `~/.deepseek/config.toml` 之上。仓库可以锁定其自己的提供商、模型、沙箱策略和审批策略。限制的键集记录在 `docs/CONFIGURATION.md` 中。`--no-project-config` 跳过覆盖。
- **`locale = "auto"`**（#496）— 新的默认设置按 `LC_ALL` → `LC_MESSAGES` → `LANG` 顺序检查环境变量。不受支持的语言环境回退到英语。
- **每轮每语言推理/回复语言**（#488）— 当最新用户消息为简体中文时，V4 的 `reasoning_content` 和最终回复被提示保持简体中文，无需在系统提示中设置语言环境。系统提示 `lang` 字段仅在不明确时作为回退。`reasoning_content` 在系统提示更改时稳定，英文/日文用户消息不受影响。
- **`cost_currency` 配置**（#489）— `cost_currency = "cny"`（或 `"yuan"`、`"rmb"`）切换底部成本显示为人民币。
- **`background_color` 配置键**（#493）— 可选的主 TUI 背景色，带有可访问性检查。在 `/config` 中编辑。
- **`DEEPSEEK_FORCE_HTTP1` 环境变量**（#498）— 用于在 HTTP/2 有问题的代理后面或 Windows 上调试。设置后客户端使用 HTTP/1.1。
- **已解析但当前未使用的 `tools_file` 配置键**（#497）— 为未来版本保留。

#### TUI 和 UX
- **完整快捷键参考**（#491）— `/keys` 在可搜索的叠加层中显示上下文绑定的快捷键（composer、记录、全局）。`docs/KEYBINDINGS.md` 是规范来源。
- **`/rename` 命令**（#486）— 内联重命名当前会话而不编辑保存文件。
- **`/config` 编辑器**（#494）— 用于常用设置（主题、语言环境、模型、成本货币、审批、沙箱）的交互式设置编辑器，带有自动补全、输入验证和即时保存。
- **`/models` 命令**（#495）— 从已配置的 API 端点发现实时模型 ID。列出模型、能力（上下文窗口）和定价（如果已知）。
- **`/theme` 内联切换**（#492）— 无需 `/config` 往返即可切换深色/浅色/鲸鱼主题。草稿保持不变。
- **拖拽记录滚动条**（#500）— 启用鼠标捕获后，抓取并拖动滚动条拇指。
- **`Copied` toast 通知**（#502）— 记录文本选择在复制时显示短暂的 "Copied" 通知。（仅在顶部鼠标选择复制上显示；Ctrl+C、Shift+Insert 和文本选择后按 Enter 不显示 toast。）
- **回滚后显示 `/restore` 回退提示**（#504）— 在撤销/恢复后明确打印 "use `/restore` to undo"。

#### 引擎和运行时
- **每轮本地化语言检测**（#488 核心）— 语言在每用户轮次确定。系统提示的 `lang` 字段仅在不明确时使用。前 N 条用户消息没有特殊处理——第一条消息的语言决定第一次回复的语言。
- **Thinking ContentBlock 支持在流式传输中** — 引擎将 `ContentBlock::Thinking` 块流式传输到 TUI，TUI 渲染一个专用的思考面板而不是将它们混合到助手文本中。`show_thinking` 设置控制可见性。
- **深度链接（`deepseek://` URL 方案）** — `deepseek://resume/<session_id>` 和 `deepseek://fork/<session_id>` 在注册的方案处理程序中打开 TUI 并指向正确的会话。macOS 需要用户在系统偏好设置中批准 `deepseek`。
- **运行时 API v1 端点（`deepseek serve --http`）** — 完整记录在 `docs/RUNTIME_API.md` 中。用于线程管理、轮次提交、SSE 事件流、任务队列和自动化的 REST + SSE 端点。设计用于 Whalescale 桌面运行时桥接。
- **ACP stdio 适配器（`deepseek serve --acp`）** — 用于 Zed 等编辑器的 Agent Client Protocol 服务器。初始实现：initialize、session/new、session/prompt、session/cancel。无 shell 或文件工具。
- **Whalescale 桥接基础设施** — `POST /v1/approvals/{id}`、`GET /v1/runtime/info`、技能启用标志、推理事件的项目类型。

### 变更
- **`Normal` 模式已移除** — 之前有两种方式进入 Agent 模式：`Agent` 和 `Normal`。`Normal` 现在已移除。旧的 `default_mode = "normal"` 设置加载为 `Agent`；隐藏的 `/normal` 命令切换到 Agent 模式。
- **`/deepseek` 命令重命名为 `/links`** — 别名包括 `/dashboard` 和 `/api`。旧的 `/deepseek` 现在显示一个弃用警告。
- **`/set model <name>` 已弃用** — 使用 `/config` 替代。`/set` 现在重定向到 `/config`。
- **配置键弃用** — `DEEPSEEK_DEFAULT_TEXT_MODEL` 被 `DEEPSEEK_MODEL` 取代。旧的名称被接受但触发弃用警告。
- **错误消息更加友好** — 引擎生成的错误使用更少的技术术语，并包括人性化的解释。API 错误现在说 "DeepSeek API 返回了一个错误" 而不是纯粹的 JSON。
- **启动擦除和恢复** — `deepseek` 启动时现在擦除任何先前的中断检查点，除非传递 `--continue`。中断的会话通过 `deepseek sessions` / `deepseek resume` 仍然可用。
- **`/undo` 变基行为** — `/undo` 现在正确变基，使得撤消更早的轮次不会使之后的轮次成为孤儿。撤消的轮次被标记为 `rolled_back`，工具结果被保留但不重新提交。
- **修剪清单中的工具结果** — `compact_messages` 在压缩期间保留工具结果，因此压缩不会将后续助手消息变为孤儿。
- **`doctor` 终端探测超时** — `tui.terminal_probe_timeout_ms` 设置（默认 500ms，限制在 100-5000ms）防止启动在模糊的终端上挂起。超时记录警告并中止启动。
- **`doctor` 错误使用人类可读行** — JSON 和文本输出都更一致。`doctor --json` 跳过 API 连接探测。
- **Node.js 18+ 需要用于 npm install** — npm 包装器的 `fetch` 需要在 Node 18+ 中全局可用。如果检测到旧版本，包装器会打印一个有用的错误。
- **`deepseek-author` 从 npm 包装器中移除** — npm 卸载脚本不再尝试调用 `deepseek-author`。包范围缩小到仅下载和二进制暴露。
- **npm 卸载脚本更安全** — 卸载脚本在从 PATH 中移除前验证目录所有权。
- **免费层的定价是一个明确的 "free" 标签** — 成本显示不再显示 "$0.00000"。
- **内部 crate 重构** — 见下文 "架构变更"。

### 修复
- **Ctrl+C 在批处理 shell 命令退出后正确返回到 TUI**（#537）— 在子进程退出后立即按 Ctrl+C 被 `signal_hook` 在 `exec_shell` 中吞噬。修复移除了非交互式命令的信号钩子。
- **会话加载时间** — 冗余的 `serde_json::from_slice` 在会话管理器中使加载速度变慢。合并为单次反序列化，在 `SessionManager` 中将会话加载时间从约 900μs 减少到约 50μs。
- **`/compact` 助手消息丢失** — 压缩丢弃了纯助手消息，使工具结果成为孤儿。修复保留了用户和助手消息之间的一对一关系。
- **sed macOS 兼容性**（#527）— 在 macOS 上对不存在的文件运行 `sed -i` 产生 `exit 0`。工具文档已更新。
- **`/accept` 跳过第一轮**（#549）— 第一轮被跳过，所以 `/accept` 感觉像什么都没做。已修复。
- **`/tokens` 对齐**（#551）— 长 token 计数在不对齐的列中溢出。添加了右对齐和千位分隔符。
- **Windows `cmd.exe` CPG 解析**（#552）— `MODE CON` 输出中的前导空格导致列解析偏移。已修复。
- **HTTP 流量日志级别**（#539）— 请求/响应日志从 `info` 移到 `trace`。
- **V4 自动补全空匹配**（#548）— 在无匹配时溢出。添加了空保护。
- **滚动时选择泄漏**（#555）— 选择锚点现在重新计算为屏幕可见上限。
- **面板之间的文本渗漏**（#557）— 分割调整大小处理程序中的有符号/无符号不匹配导致文本跨面板边界渗漏。
- **正则表达式性能退化**（#566）— `^` 锚定的模式变为慢速线性扫描。用 `(?-u:...)` 包裹以禁用 Unicode 感知并启用 DFA 加速。
- **alt 键绑定在 Wayland 上被吞噬** — 终端中 alt 键修饰符在发送前被某些 Wayland 合成器捕获。添加了 `meta_sends_escape` 配置键（默认 `true`）以支持 `Alt` 前缀在 Kitty 协议中正确发送。
- **`Ctrl+K` 后 KeyEvent 传播** — `/keys` 帮助叠加层在 Ctrl+K 后保持打开，因为 KeyEvent 在第一个处理程序处理后没有被标记为已消费。修复：`handle_key_event` 在叠加层打开时返回 `Handled`。
- **输入过多时的滚动跳跃** — 当粘贴大块文本时，记录滚动到末尾然后弹回。修复：在粘贴突发期间延迟滚动位置更新。
- **`Tab` 完成高亮在空匹配时闪烁** — 当没有完成项时，自动补全弹出窗口在关闭前短暂闪烁。添加了空匹配提前返回。
- **`NO_COLOR` 尊重** — 当 `NO_COLOR` 设置时，TUI 在写入日志文件时不再发出 ANSI 转义。`deepseek doctor --json` 输出始终不包含 ANSI。
- **`deepseek serve --http` 端口绑定** — 当端口已被占用时，服务器因模糊错误而崩溃。添加了清晰的 "端口 7878 已被占用" 消息。
- **`/skills` 路径扩展** — `skills_dir` 配置中的 `~` 现在正确扩展到主目录。
- **`/stash` 文件权限** — composer_stash.jsonl 使用更严格的权限创建（0o600）。
- **`--help` 文本中缺少 `--auth-token` 文档** — 用于 `deepseek serve --http` 的标志已添加。
- **`doctor` 中的 `sandbox` 字段在 macOS 上为 `null`** — macOS Seatbelt 检测被一个无声的 `if let` 失败破坏。修复恢复为 `macos_seatbelt`。
- **Windows 路径中的 `~` 扩展** — `expand_path` 现在在 Windows 上将 `~` 解析为 `%USERPROFILE%`。
- **`--no-mouse-capture` 在 Wayland 上** — 在 Wayland 上鼠标捕获始终关闭，即使显式请求。修复：仅在没有 Wayland 检测时默认关闭鼠标捕获；`--mouse-capture` 显式覆盖。
- **`model` 大小写规范化** — 用户配置的模型 ID 现在保留其大小写。例如，`deepseek-V4-Pro` 不会规范化为 `deepseek-v4-pro`。
- **通过 `deepseek doctor --json` 报告 V4 能力** — 能力字段现在为 `deepseek-v4-pro` 正确报告 `context_window: 1048576` 和 `thinking_supported: true`。
- **`[subagents] max_concurrent` 在 `max_subagents` 之下** — 子代理并发控制尊重两个设置中较小的值。

### 文档
- **新的 `docs/KEYBINDINGS.md`** — 完整、权威的快捷键目录。
- **更新的 `docs/CONFIGURATION.md`** — 重构，添加了项目覆盖、指令源、通知、用户记忆和关键参考表。
- **新的 `docs/RUNTIME_API.md`** — 完整的 HTTP/SSE API 参考。
- **更新的 `docs/INSTALL.md`** — 添加了 Linux ARM64、交叉编译、Windows 源码构建、npm 镜像、故障排除部分。
- **`README.md` 安装部分更新** — 添加了 Docker、Scoop、中国镜像路径。
- **`README.zh-CN.md` 同步** — 简体中文翻译与英文版本保持同步。

### 架构变更
- **提取的 `crates/tools/`** — 工具定义（描述、模式、审批要求、缓存策略）现在在 `crates/tools/` 中集中定义。运行时工具注册表从那里生成。这使得工具元数据和运行时之间的解耦成为可能。
- **提取的 `crates/execpolicy/`** — 审批/沙箱策略引擎现在是一个独立的 crate。Plan 模式强制执行 `SandboxPolicy::ReadOnly`，YOLO 使用 `DangerFullAccess`，Agent 使用 `WorkspaceWrite`。
- **提取的 `crates/agent/`** — 模型/提供商注册表现在是一个独立的 crate，具有提供商特定的端点解析、模型 ID 映射和能力自省。
- **提取的 `crates/app-server/`** — HTTP/SSE 运行时 API 现在是一个独立的 crate，有自己的 Cargo.toml 和依赖树。
- **提取的 `crates/hooks/`** — 生命周期钩子系统提取到 `crates/hooks/` 中，具有事件类型、负载模式和发现机制。
- **提取的 `crates/protocol/`** — 运行时 API 的类型化契约类型提取到 `crates/protocol/` 中，实现完全的序列化/反序列化，并通过对等协议测试锁定。
- **crates/tui/ 精简** — 引擎/运行时核心现在引用提取的 crate 作为依赖项。`crates/tui/` 中的代码行数减少了约 15%。
- **git worktree 开发流程** — 每个工作流（TUI 错误修复、OPENCODE 基础设施、代理/UX、应用服务器、Web UI、VS Code）都在单独的 git worktree 中开发。协调者代理（`docs/v0.8.8-coordinator-prompt.md`）管理工作流生成。

### 贡献者
本版本代表着一个里程碑，从社区驱动的错误修复转向开放的共享基础设施开发。在 30 多个贡献者的 200 多个 PR 中，工作流程使 10 多个并发的子代理能够同时开发独立的功能流。v0.8.8 中的每个新 crate 都为将来的维护和扩展奠定了基础。

## [0.8.7] - 2026-05-03

### 修复
- **跨记录单元格类型的选择** — v0.8.6（#383）中的选择收紧将复制/选择限制为用户和助手消息正文。选择单元格需要包括时间戳单元格和记录状态行。放宽了选择边界以匹配用户期望。时间戳和系统消息现在可以是选择的一部分。
- **`deepseek mcp validate` 输出** — 以正确的退出码和人类可读的输出验证 MCP 配置/连接。
- **提供者 API 密钥环境变量优先级** — 当同时设置 `DEEPSEEK_API_KEY` 和提供者特定的 `NVIDIA_API_KEY` 时，正在使用 `DEEPSEEK_API_KEY`。提供者密钥现在正确优先。
- **`--provider ollama` 模型 ID 传递** — Ollama 模型标签（例如 `deepseek-coder:1.3b`）被提供者规范化重写为 `deepseek-v4-flash`。Ollama 模型 ID 现在按原样传递。

### 变更
- **`[lsp]` 配置节已弃用** — LSP 集成现在默认启用且始终打开。`enabled` 标志仍被解析但不执行任何操作。删除特定于 LSP 的配置支持的计划。
- **Header/UI 小部件重构** — 内部清理，无用户可见的行为更改。

## [0.8.6] - 2026-05-03

### 新增
- **系统提示分析文档** — `PROMPT_ANALYSIS.md` 记录了 "管理不当的天才" 假说，即当前提示将 RLM 和子代理定位为 "专业逃生舱" 而非 "默认战略工具"。见文件本身以获取完整的差距分析和建议的提示更改。
- **编译器错误 `unused_imports` 的 LSP 诊断** — LSP 钩子现在解析并报告未使用导入作为诊断。以前这些被 `include_warnings = false` 门控过滤掉。
- **`DEEPSEEK_CAPACITY_*` 环境变量** — 容量控制器设置现在可以通过环境变量覆盖。完整列表见 `docs/CONFIGURATION.md`。

### 变更
- **`cargo test --workspace --all-features` 是标准测试命令** — 所有测试文档都更新为使用 `--all-features`。
- **`model` 配置键现在保留大小写** — 用户指定的模型 ID 保持其大小写；只有已知的 DeepSeek 别名（`deepseek-chat` → `deepseek-v4-flash`）被规范化。
- **HTML 剥离后的自动补全换行** — `Ctrl+P` / `Ctrl+N` 自动补全导航在 HTML 被剥离后正确包裹。
- **速度？没有减速。** — 引擎热路径优化：减少 alloc 和克隆，更早退出。

### 修复
- **`/stash list` 崩溃** — 空的 composer_stash.jsonl 导致解析恐慌。现在处理为空列表。
- **`/undo` 撤消错误的轮次** — 轮次索引在 `/undo N` 中偏移了一。N=1 撤消了当前轮次，而不是上一个。已修复。
- **`deepseek doctor` 报告 `api_key.source: "env"` 即使密钥来自配置** — 源标签在配置路径上被错误标记。已修复。
- **Windows: `%USERPROFILE%\.cargo\bin` 未在 `deepseek doctor` 中检查** — 医生检查 PATH 上的二进制文件但现在仅建议 Windows 用户手动添加目录。
- **macOS: `deepseek doctor` 中的密钥链权限提示** — 医生在探测密钥链条目时触发 macOS 权限弹出。现在跳过密钥链探测，除非明确使用 `--keychain` 标志。
- **`/skills sync` 在无网络访问时挂起** — 同步现在受 `[network]` 策略门控并在块连接时超时（默认 10 秒）。

### 文档
- **README 中的 `--model auto` 文档** — 新的 "Auto Mode" 部分解释了 `auto` 如何为每轮选择模型和思考级别。
- **修复了 `docs/SUBAGENTS.md` 中的术语一致性问题** — "subagent" → "sub-agent"（全文档）。
- **在 `docs/ARCHITECTURE.md` 中添加了中文翻译** — 中文架构文档保持同步。

---

> **注意**：以下版本（v0.8.4 及更早）的内容已从英文翻译为简体中文。这些是较早期的发布历史记录，主要包含初始功能开发信息。

## [0.8.4] - 2026-05-02

### Added
- **RLM 多轮支持**：RLM REPL 现在可以在同一轮次的多次 `rlm_query` 调用之间保持状态。在工具响应中使用 `rlm_session_active` 标志来指示 REPL 应保持打开状态。
- **RLM 的 `retain` 工具**：显式保持 RLM 会话活动而不产生响应。适用于长时间运行的批处理操作。
- **`rlm_query_batched` 并行扇出**：在单个 RLM 调用中批量向 `deepseek-v4-flash` 提交最多 16 个独立查询。每个查询获取自己的 API 请求；结果以 JSON 数组形式返回。
- **`DEEPSEEK_CAPACITY_*` 环境变量**（从 v0.8.3 继续）：所有容量控制器设置现在都可以通过环境变量覆盖。
- **`[notifications].include_summary` 配置**：当为 `true` 时，通知正文包含已用时间和以配置显示货币计价的成本。
- **`tui.osc8_links` 配置**（默认为 `true`）：在记录输出中的 URL 周围发送 OSC 8 转义序列。如果您的终端无法正确渲染该序列，请设置为 `false`。

### Changed
- **RLM 默认模型固定为 Flash**：所有 RLM 子调用现在默认使用 `deepseek-v4-flash`，而不是继承父模型。这防止了批处理分类工作产生昂贵的 Pro 调用。
- **容量控制器默认为 `enabled = false`**：控制器之前无论 token 利用率如何都在清除记录（`messages.clear()`）。现在必须显式启用。
- **`auto_compact` 默认为 `false`**：V4 默认路径保留稳定的消息前缀以利于缓存重用。当您明确需要替换式压缩时，请使用手动 `/compact` 或启用 `auto_compact`。
- **压缩现在保留工具结果**：`compact_messages` 不再从压缩输出中剥离 tool-call/tool-result 对，修复了孤儿助手消息的问题。

### Fixed
- **RLM 子代理上下文泄露**：当 RLM 子代理写入 `stdout` 时，输出被父 REPL 捕获而不是丢弃。通过关闭子进程的 stdout 管道修复。
- **`/compact` 移除助手消息**：压缩保留了用户消息和系统提示，但丢弃了不包含工具结果或推理块的纯助手消息。现在保留用户-助手消息配对。
- **会话保存竞争**：引擎和 TUI 的并发会话保存可能损坏会话文件。通过每会话写入锁修复。
- **Windows `DEEPSEEK_HOME` 路径解析**：Windows 上的 `~/.deepseek` 现在正确解析为 `%USERPROFILE%\.deepseek`。
- **`doctor --json` 字段 `api_key.source`**：当密钥来自 `config.toml` 时，现在正确报告 `"config"`。
- **`doctor` MCP 连接检查**：修复了当 MCP 服务器的 `command` 字段在配置中缺失时的崩溃。
- **Linux 上 `Ctrl+Z` 使终端停留在原始模式**：SIGTSTP 处理现在在挂起前将终端恢复到烹饪模式。

## [0.8.3] - 2026-05-01

### Added
- **`/stash` 命令**：按 Ctrl+S 将当前草稿暂存到 `~/.deepseek/composer_stash.jsonl`。支持 `/stash list`、`/stash pop`（后进先出）、`/stash clear`。上限为 200 条。
- **`/attach` 命令**：将本地媒体路径（图片/视频）作为显式路径引用附加。
- **`paste_burst_detection` 设置键**：为不支持 bracketed-paste 的终端提供回退快速按键粘贴检测。默认开启。
- **`[snapshots].max_age_days` 配置**：控制工作区快照的保留天数（默认为 7 天）。
- **`DEEPSEEK_FORCE_HTTP1`**：将 HTTP 客户端固定到 HTTP/1.1 以调试代理问题。

### Changed
- **快照存储位置已移动**：从 `<workspace>/.deepseek_snapshots/` 移动到 `~/.deepseek/snapshots/<project_hash>/<worktree_hash>/.git`。这防止了快照 git 仓库杂乱工作区。
- **快照修剪在启动时运行**：`prune_unreachable_objects` 在常规修剪周期中删除回滚快照中的松散对象。
- **`deepseek doctor` 现在检查伴生二进制文件**：验证 `deepseek-tui` 与 `deepseek` 在同一 PATH 上。

### Fixed
- **快照孤儿清理**：中断的 side-repo git pack 操作不再泄漏 `tmp_pack_*` 文件（报告了高达 30 GB+ 的磁盘使用，#975）。
- **Windows shell 输出编码**：命令使用 `chcp 65001 >NUL & ` 包装以在非 UTF-8 系统代码页上输出 UTF-8。
- **Plan 模式 shell 沙箱绕过**：`python -c "open('f','w').write('x')"` 在 Plan 模式下可能写入工作区。现在正确使用 `SandboxPolicy::ReadOnly`。
- **`Ctrl+K` 帮助叠加层保持打开**：KeyEvent 传播现在在叠加层打开时将事件标记为已处理。
- **Windows 上粘贴-回车自动提交**：粘贴文本中的尾随换行符不再触发立即提交。使用粘贴突发抑制状态。

## [0.8.2] - 2026-05-01

### Added
- **`/skills sync`**：获取社区技能注册表并安装/更新所有列出的技能。
- **`deepseek mcp list` / `deepseek mcp validate`**：用于 MCP 服务器管理的 CLI 子命令。
- **`[execpolicy]` 配置节**：Shell 命令的细粒度允许/拒绝/询问规则。
- **`show_tool_details` 设置键**：切换记录中详细工具调用渲染的开关。

### Changed
- **`/mcp` 现在使用解析后的 `mcp_config_path`**：在 `/config` 中更改路径会立即对 `/mcp` 生效，但重建 MCP 工具池需要重启 TUI。
- **`doctor` 中检查 MCP 服务器健康状态**：每服务器状态，显示 `ok`/`error`/`timeout`。
- **`deepseek mcp-server` 现在接受 `--config`**：将 MCP stdio 服务器指向自定义配置文件。

### Fixed
- **MCP 服务器关闭挂起**：拒绝在 SIGTERM 上退出的进程现在在 3 秒宽限期后被 SIGKILL 杀死。
- **`/skills` 重复条目**：从多个路径（例如同时从 `~/.deepseek/skills/` 和 `./skills/`）发现的技能被列出两次。现在按名称去重。
- **YOLO 模式沙箱逃逸**：YOLO 模式中的 shell 命令仍被 `WorkspaceWrite` 沙箱化。现在使用 `DangerFullAccess`（无沙箱）。
- **Windows 上的 `Ctrl+R` 恢复选择器**：调度器现在在 Windows 上保留恢复选择器路径。

## [0.8.1] - 2026-05-01

### Fixed
- **热修复：当 npm 包装器尚未下载二进制文件时 `deepseek --version` 崩溃**：包装器现在报告 npm 包版本作为回退。
- **热修复：会话自动恢复跨越工作区边界**：自动恢复现在将已保存会话的工作区与 `std::env::current_dir()` 比较。不匹配时，检查点保留为常规会话。

## [0.8.0] - 2026-05-01

### Added
- **用户记忆功能**：可选的持久笔记文件（`~/.deepseek/memory.md`）注入到系统提示中。通过 `DEEPSEEK_MEMORY=on` 或 `[memory].enabled = true` 启用。支持 composer 中的 `# foo` 快速捕获、`/memory` 斜杠命令和 `remember` 工具。
- **通知**：在超过可配置阈值的完成轮次上发送桌面通知（OSC 9 / BEL）。`[notifications]` 配置节。
- **Composer 暂存**：Ctrl+S 将草稿暂存到 JSONL 文件。支持 `/stash list`、`/stash pop`、`/stash clear`。
- **`/restore` 命令**：撤销上一次 `/restore`（工作区操作的两级撤销）。
- **`revert_turn` 工具**：通过 side-git 快照实现模型可调用的轮次回滚。
- **`note` 工具**：模型可调用的持久笔记写入器。
- **`diagnostics` 工具**：报告工作区信息、git 检测、沙箱可用性和 Rust 工具链。
- **`validate_data` 工具**：JSON/TOML 验证。
- **`finance` 工具**：实时股票/加密货币报价。
- **`project_map` 工具**：高级项目结构生成。
- **`web_run` 工具**：无头浏览器交互。
- **`automation` 工具**：使用 cron 风格 RRULE 的定时重复任务。
- **`review` 工具**：结构化代码审查。
- **`github_*` 工具**：GitHub issue/PR 读取、评论和关闭。

### Changed
- **默认模型从 `deepseek-chat` 改为 `deepseek-v4-pro`**：V4 模型现在是默认模型。`deepseek-chat` 和 `deepseek-reasoner` 保持为兼容性别名。
- **API 端点默认为 `https://api.deepseek.com/beta`**：beta 门控功能（严格工具模式、FIM、聊天前缀补全）无需额外配置即可使用。
- **配置文件位置**：默认配置现在为 `~/.deepseek/config.toml`（之前为 `~/.config/deepseek/config.toml`）。旧位置仍作为回退读取。
- **设置文件位置**：`~/.config/deepseek/settings.toml`（未更改）。
- **`deepseek login` 替换为 `deepseek auth set`**：新命令将凭据保存到共享配置文件中。旧的 `deepseek login --api-key` 仍可作为别名使用。
- **`deepseek doctor` 输出格式**：JSON 输出使用 `doctor --json`；文本输出更具结构性。
- **会话格式已更新**：会话现在使用 SQLite 后端存储（`crates/state/`）。旧的 JSON 会话文件在首次启动时迁移。

### Fixed
- *整个代码库中的数百个修复。详见下方各版本的详细信息。*

## [0.7.9] - 2026-05-02

### Added
- `DEEPSEEK_HOME` 环境变量，用于覆盖基本数据目录。
- `DEEPSEEK_AUTOMATIONS_DIR` 环境变量，用于自动化存储位置。
- `[snapshots].enabled` 和 `[snapshots].max_age_days` 配置键。

### Changed
- 会话保存/检查点现在使用原子文件写入。

### Fixed
- 快照 git 仓库在工作区目录中积累的问题。
- `doctor` 对配置来源的密钥报告错误的 `api_key.source`。

## [0.7.8] - 2026-05-01

### Added
- `deepseek mcp init` 用于搭建 MCP 配置。
- `deepseek setup --all` 用于引导 MCP、技能、工具和插件。

### Changed
- `deepseek doctor` 现在遵循配置的 `mcp_config_path` 和 `skills_dir`。
- 技能发现现在包括 `.cursor/skills` 和 `.agents/skills`。
- 配置解析器现在支持 `instructions = [...]` 作为额外的系统提示来源。

### Fixed
- 带有 YAML frontmatter 引号的技能：`name: "hud"` 现在注册为 `hud`。
- 没有 frontmatter 的纯 Markdown 技能回退到第一个 `# Heading`。
- Linux 上 `watch` 模式的文件描述符泄漏。

## [0.7.7] - 2026-04-30

### Added
- **RLM（递归语言模型）**：沙箱化的 Python REPL，用于使用子 LLM 辅助函数（`llm_query`、`llm_query_batched`、`rlm_query`）进行批处理、分块和递归批评。
- **持久任务队列**：重启感知的后台任务执行，带有证据追踪（关卡运行、PR 尝试、时间线）。
- **自动化**：使用 RRULE 重复规则的定时重复任务。
- **`deepseek serve --http`**：用于无头代理工作流的本地 HTTP/SSE 运行时 API。
- **`deepseek serve --acp`**：用于编辑器集成的 ACP stdio 适配器。
- **`deepreview` 工具**：带有多视角分析的结构化代码审查。
- **`dashboard` 斜杠命令**：会话和任务概览。

### Changed
- **工具注册表重构**：工具现在在 `crates/tools/` 中定义为类型化规范。
- **配置全面改造**：新的 `[profiles]` 节、托管配置、需求验证。
- **提示系统重新设计**：分层提示（`base.md` + 模式覆盖 + 个性 + 审批策略）。
- **引擎架构**：核心引擎提取到 `crates/core/`。

### Fixed
- *跨所有子系统的大量测试和错误修复。*

## [0.7.1] - 2026-04-28

### Fixed
- JSON schema 工具参数对于嵌套对象类型的反序列化。

## [0.7.0] - 2026-04-28

### Added
- 子代理生命周期管理（`agent_spawn`、`agent_result`、`agent_wait` 等）。
- MCP 客户端集成，支持 stdio 服务器。
- 会话保存/恢复，带有检查点持久化。
- 配置档案（`[profiles]` 节）。
- 多提供商支持（NVIDIA NIM、Fireworks、SGLang、vLLM、Ollama）。

### Changed
- 默认引擎切换到 V4 架构。
- UI 刷新，采用鲸鱼品牌主题。
- MCP 配置格式更新以支持多服务器。

## [0.6.5] - 2026-04-27

### Added
- `deepseek doctor` CLI 命令，用于系统诊断。
- `deepseek mcp-server` 命令，用于运行调度器 MCP stdio 服务器。
- Web 搜索和获取能力，通过 `web_search`、`fetch_url`、`web_run` 工具。

### Changed
- 工具审批模型：工具现在声明自己的审批要求。
- 配置文件格式扩展，支持提供商特定设置。
- 会话持久化格式已更新。

### Fixed
- `deepseek doctor` 在发布模式下无需 native-tls 即可构建。
- macOS 密钥链集成用于凭据存储。

## [0.6.1] - 2026-04-26

### Fixed
- macOS 密钥链集成用于凭据存储。
- `deepseek doctor` 在发布模式下无需 native-tls 即可构建。

## [0.6.0] - 2026-04-25

### Added
- V4 模型支持（`deepseek-v4-pro`、`deepseek-v4-flash`），具有 1M 上下文窗口。
- 思考模式流式传输和在 TUI 中渲染。
- 前缀缓存感知的成本追踪。
- 模型+思考选择的自动模式（`--model auto`）。

### Changed
- 为 V4 架构进行了重大引擎重写。
- 提示系统已为 V4 能力更新。

## [0.5.2] - 2026-04-25

### Fixed
- Shell 工具 `cwd` 强制执行。
- `deepseek doctor` 端点诊断。

## [0.5.1] - 2026-04-25

### Added
- 初始的 npm 包装器，用于二进制分发。
- 自更新命令（`deepseek update`）。

## [0.5.0] - 2026-04-25

### Added
- Plan/Agent/YOLO 模式系统。
- 工作区快照和回滚（`/restore`、`revert_turn`）。
- 用于仓库操作的 Git/GitHub 工具。
- 本地化支持（`en`、`ja`、`zh-Hans`、`pt-BR`）。

## [0.4.9] - 2026-04-27

### Added
- `deepseek doctor --json` 用于机器可读的诊断输出。
- `deepseek mcp list / validate` 子命令。
- `[snapshots]` 配置节，用于工作区快照控制。

### Fixed
- `doctor` 在边缘情况下报告错误的 `api_key.source`。
- Linux 上 `watch` 模式的文件描述符泄漏。

## [0.4.8] - 2026-04-26

### Added
- 带锁感知调度的并行工具执行。
- 带终端暂停/恢复的交互式 shell 模式。
- `deepseek mcp add/list/get/remove` MCP 服务器管理。

### Fixed
- 多工具轮次的工具审批路由。
- 使用 `--watch` 的配置文件监视。

## [0.3.33] - 2026-02-04

### Added
- DeepSeek V3.2 模型支持。
- 推理强度控制（`off`、`low`、`medium`、`high`、`max`）。

## [0.3.32] - 2026-02-04

### Fixed
- `cargo release` 在干运行时跳过发布 npm 包。
- 改进了编译器错误消息。

## [0.3.31] - 2026-02-04

### Added
- MCP 工具 `get` 命令，用于按名称发现工具。

### Fixed
- 针对 DuckDuckGo 速率限制的 Web 搜索结果解析。

## [0.3.28] - 2026-02-04

### Added
- `read` 工具，支持通过 `pdftotext` 自动提取 PDF。
- `rlm` 工具，用于长文档分析。
- `/stats` 中的工具使用统计。

## [0.3.27] - 2026-02-04

### Changed
- 从默认工具集中移除了 `multimedia` 工具。
- 更新了提示和文档以适应纯文本 DeepSeek API。

## [0.3.23] - 2026-02-03

### Added
- `deepseek` CLI 入口点，带有子命令调度。
- TUI 伴生二进制文件（`deepseek-tui`）。
- 配置文件支持（`config.toml`）。

## [0.3.22] - 2026-02-03

### Added
- `doctor` 中的 MCP 服务器健康检查。
- 技能目录发现（`.deepseek/skills`）。

## [0.3.21] - 2026-02-03

### Fixed
- 包含 unicode 字符的工具结果的 JSON 解析。
- 并发保存时的会话文件损坏。

## [0.3.17] - 2026-02-02

### Added
- `Ctrl+K` 命令面板，带有可搜索的帮助叠加层。
- `F1` 键盘快捷键帮助叠加层。
- `/keys` 命令，用于显示键盘快捷键参考。

## [0.3.16] - 2026-02-02

### Added
- 技能系统，支持 `SKILL.md` frontmatter 发现。
- 用户记忆，支持在 composer 中通过 `# foo` 快速捕获。
- 会话注释和笔记本式组织。

## [0.3.14] - 2026-02-01

### Added
- `deepseek doctor` 命令，用于系统诊断（初始版本）。
- 文件树面板，用于工作区浏览。
- 带会话元数据的侧边栏。

## [0.3.13] - 2026-02-01

### Added
- 编辑模式，带有内联差异高亮。
- `/edit`、`/diff`、`/undo` 命令，用于工作区编辑。

## [0.3.12] - 2026-01-31

### Added
- 多模型支持，通过 `/model` 命令。
- 来自已知 DeepSeek 模型的模型 ID 自动补全。

## [0.3.11] - 2026-01-31

### Added
- 工具审批策略（`on-request`、`untrusted`、`never`）。
- 可配置的沙箱模式（`read-only`、`workspace-write`、`danger-full-access`）。

## [0.3.10] - 2026-01-30

### Added
- Web 搜索和获取工具。
- 通过 `web_run` 进行 Web 浏览。
- 用于股票/加密货币报价的金融工具。

## [0.3.6] - 2026-01-30

### Added
- 会话保存/恢复，使用 JSON 持久化。
- `/sessions` 命令，用于列出已保存的会话。

## [0.3.5] - 2026-01-30

### Added
- Apply patch 工具，用于结构性编辑。
- 文件搜索，支持 glob 和模糊匹配。
- Git 模块，支持 blame、log、diff、status。

## [0.3.4] - 2026-01-29

### Fixed
- 针对更新后的 `McpServerConfig` 结构体形状的 MCP 测试编译错误。

## [0.3.3] - 2026-01-28

### Added
- TUI 打磨：Kimi 风格的页脚，显示模式/模型/token。
- 流式思考块，带有专用渲染。
- 加载动画改进。

## [0.3.2] - 2026-01-28

### Fixed
- 在压缩期间保留 tool-call + tool-result 配对，以避免无效的工具消息序列。
- 在请求构建器中丢弃孤立的工具结果作为安全网，以防止 API 400 错误。

## [0.3.1] - 2026-01-27

### Added
- `deepseek setup` 用于引导 MCP 配置和技能目录。
- `deepseek mcp init` 用于在配置路径生成模板 `mcp.json`。

### Changed
- `deepseek doctor` 现在遵循解析后的配置路径和从配置派生的 MCP/技能位置。

### Fixed
- 当路径通过配置或环境变量覆盖时，Doctor 不再报告缺失的 MCP/技能。

## [0.3.0] - 2026-01-27

### Added
- 仓库感知的工作集追踪，对活动路径进行提示注入。
- 工作集信号现在在自动压缩期间固定相关消息。
- 离线评估框架（`deepseek eval`），在测试任务中具有 CI 覆盖。
- Shell 工具现在发出 stdout/stderr 摘要和截断元数据。
- 依赖感知的 `agent_swarm` 工具，用于编排多个子代理。
- 扩展了子代理工具访问权限（apply_patch、web_search、file_search）。

### Changed
- 自动压缩现在考虑固定预算并保留工作集上下文。
- Apply patch 工具验证补丁形状，报告每文件摘要，并改进了块不匹配诊断。
- 评估框架 shell 步骤现在使用 Windows 安全的默认命令。
- 将 `max_subagents` 限制增加到 `1..=20`。

## [0.2.2] - 2026-01-22

### Fixed
- 会话保存不再在序列化错误时崩溃。
- Web 搜索正则表达式模式现在被缓存以提高性能。
- 改进了正则表达式编译失败的崩溃消息。

## [0.2.1] - 2026-01-22

### Fixed
- 解决 Rust 1.92 的 clippy 警告。

## [0.2.0] - 2026-01-20

### Changed
- 移除了 npm 包分发；现在仅通过 Cargo。
- 为公开发布进行清理。

### Fixed
- 禁用了自动 RLM 模式切换；使用 /rlm 或 /aleph 进入 RLM 模式。
- 修复了 cargo fmt 格式化问题。

## [0.0.2] - 2026-01-20

### Fixed
- 禁用了自动 RLM 模式切换；使用 /rlm 或 /aleph 进入 RLM 模式。

## [0.0.1] - 2026-01-19

### Added
- DeepSeek Responses API 客户端，带有 chat-completions 回退。
- CLI 对等命令：login/logout、exec、review、apply、mcp、sandbox。
- 恢复/分支会话工作流，带有选择器回退。
- DeepSeek 蓝色品牌刷新 + 鲸鱼指示器。
- Responses API 代理子命令，用于密钥隔离转发。
- Execpolicy 检查工具和功能标志 CLI。
- 代理执行模式（`deepseek exec --auto`），带有自动审批。

### Changed
- 移除了多媒体工具，并为纯文本 DeepSeek API 调整了提示和文档。

## [0.1.9] - 2026-01-17

### Added
- `deepseek doctor` 命令中的 API 连接测试。
- 针对常见 API 失败（密钥无效、超时、网络问题）的有用错误诊断。

## [0.1.8] - 2026-01-16

### Added
- 可渲染小部件抽象和模态视图栈，用于 TUI 组合。
- 带锁感知调度的并行工具执行。
- 带终端暂停/恢复处理的交互式 shell 模式。

### Changed
- 工具审批要求移入工具规范中。
- 工具结果按原始请求顺序记录。

## [0.1.7] - 2026-01-15

### Added
- Duo 模式（选手-教练自动编码工作流）。
- 字符级记录选择。

### Fixed
- 审批流程工具使用 ID 路由。
- 记录选择的游标位置同步。

## [0.1.6] - 2026-01-14

### Added
- 针对大型粘贴块的自动 RLM，带有上下文自动加载。
- `chunk_auto` 和 `rlm_query` 的 `auto_chunks`，用于快速文档扫描。
- 页脚中带有预算警告的 RLM 使用徽章。

### Changed
- 自动 RLM 现在即使对于较小的文件也尊重显式的 RLM 文件请求。

## [0.1.5] - 2026-01-14

### Added
- RLM 提示，带有外部上下文指南和 REPL 工具。
- RLM 工具，用于上下文加载、执行、状态和子查询（rlm_load、rlm_exec、rlm_status、rlm_query）。
- RLM 查询使用追踪和变量缓冲区。
- RLM 加载的工作区相对 `@path` 支持。
- 当用户请求大文件分析（或最大文件）时自动切换到 RLM。

### Changed
- 移除了编辑模式；RLM 聊天成为默认，通过 /repl 切换。

## [0.1.0] - 2026-01-12

### Added
- DeepSeek TUI 的初始 Alpha 版本。
- 交互式 TUI 聊天界面。
- DeepSeek API 集成（OpenAI 兼容的 Responses API）。
- 工具执行（shell、文件操作）。
- MCP（模型上下文协议）支持。
- 带历史记录的会话管理。
- 技能/插件系统。
- 成本追踪和估算。
- 钩子系统和配置档案。
- 示例技能和启动资源。

[Unreleased]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.7.9...v0.8.0
[0.7.9]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.7.8...v0.7.9
[0.7.8]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.7.7...v0.7.8
[0.7.7]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.7.6...v0.7.7
[0.7.6]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.7.5...v0.7.6
[0.6.1]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.4.9...v0.6.0
[0.4.9]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.4.8...v0.4.9
[0.4.8]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.33...v0.4.8
[0.3.33]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.32...v0.3.33
[0.3.32]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.31...v0.3.32
[0.3.31]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.28...v0.3.31
[0.3.28]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.27...v0.3.28
[0.3.23]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.22...v0.3.23
[0.3.22]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.21...v0.3.22
[0.3.21]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.17...v0.3.21
[0.3.17]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.16...v0.3.17
[0.3.16]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.14...v0.3.16
[0.3.14]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.13...v0.3.14
[0.3.13]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.12...v0.3.13
[0.3.12]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.11...v0.3.12
[0.3.11]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.10...v0.3.11
[0.3.10]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.6...v0.3.10
[0.3.6]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.5...v0.3.6
[0.3.5]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.0.2...v0.2.0
[0.0.2]: https://github.com/Hmbown/DeepSeek-TUI/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/Hmbown/DeepSeek-TUI/releases/tag/v0.0.1

<!-- generated by git-cliff -->