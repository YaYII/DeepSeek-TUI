# 子代理（Sub-Agents）

子代理是代理循环的后台实例。父级代理生成一个子代理并分配一个专注的任务，立即获得一个 `agent_id`，
然后在子代理运行至完成的同时继续工作。子代理默认继承父级的工具注册表，并使用
`CancellationToken::child_token()` 运行，因此取消父级代理会取消所有后代代理。

本文档涵盖角色分类。有关编排工具接口
（`agent_spawn` / `agent_wait` / `agent_result` / `agent_cancel` /
`agent_list` / `agent_send_input` / `agent_resume` / `agent_assign`）
请参见 `prompts/base.md` 中的"子代理策略"及内联工具描述。

## 角色分类（Role taxonomy）

`agent_spawn` 上的 `agent_type` 字段为子代理选择系统提示姿态。
每个角色对应一种截然不同的工作立场——不仅仅是标签不同。

| 角色（Role）   | 立场（Stance）                            | 可写入？ | 可运行 shell？ | 典型用途                                      |
|---------------|-------------------------------------------|---------|--------------|----------------------------------------------|
| `general`     | 灵活；完全按父级要求执行                     | 是      | 是            | 默认选项；多步骤任务                            |
| `explore`     | 只读；快速映射相关代码                      | 否      | 是（读取）    | "查找 `Foo` 的每个调用点"                      |
| `plan`        | 分析并生成策略                              | 最少    | 最少          | "设计迁移方案；不要执行"                        |
| `review`      | 阅读并评分，给出严重性等级                   | 否      | 否            | "审计这个 PR 是否有 bug"                       |
| `implementer` | 以最小编辑落地特定变更                       | 是      | 是            | "重写 `bar.rs::Foo::bar` 以实现 X"             |
| `verifier`    | 运行测试/验证，报告结果                      | 否      | 是（测试）    | "运行 cargo test --workspace，报告结果"        |
| `custom`      | 显式限定狭窄的工具允许列表                   | 视情况  | 视情况        | 使用精选工具的锁定分发                          |

每个角色的完整系统提示位于
`crates/tui/src/tools/subagent/mod.rs`（搜索 `*_AGENT_PROMPT`）。
提示前缀在子代理启动时自动加载；父级的生成提示成为第一个轮次的用户消息。

## 上下文分叉（Context forking）

默认情况下，`agent_spawn` 以全新状态启动：子代理获得其角色提示加上您传递的任务。
当子代理应该从父级的当前请求前缀继续执行时，使用 `fork_context: true`。
在分叉模式下，子代理请求保持父级系统提示和消息历史的字节一致性，追加一个结构化的状态快照，
然后在末尾添加子代理角色指令和任务。这样既能保持 DeepSeek 前缀缓存的高复用率，
又能为子代理提供继续执行、审查、总结或压缩工作所需的上下文。

对于独立的探索性工作，使用全新生成。当任务依赖于父级转录中已有的决策、文件、
待办事项或计划状态时，使用分叉生成。

### 如何选择角色

- **`general`** — 当任务是"完成这整件事"，而不是"去查看"、"设计"或"验证"时使用。
  这是正确的默认选项；只有当立场确实重要时才选择更具体的角色。
- **`explore`** — 当父级在做下一步决策前需要证据时使用。探索者廉价且快速；
  对于独立区域，可同时并行生成 2–3 个。
- **`plan`** — 当父级有目标但没有可执行的分解方案时使用。规划者撰写工件
  （`update_plan` 行、`checklist_write` 条目），但不执行它们。
- **`review`** — 当已经存在变更，父级希望对其进行评分时使用。审查者不打补丁——
  他们在发现中描述修复方法，这样如果判定结果是"修复它"，父级可以调度一个实现者。
- **`implementer`** — 当变更已经明确，只需要落地时使用。实现者严格限定范围：
  最小编辑，不进行附带重构，在交回前运行快速验证。
- **`verifier`** — 当父级需要在测试套件或其他验证上获得权威的通过/失败结论时使用。
  验证者不修复失败；他们捕获失败的断言和堆栈，并将修复候选方案放在 RISKS 下。
- **`custom`** — 仅在父级需要显式限定工具集时使用。通过 `agent_spawn` 的
  `allowed_tools` 字段传递允许列表。

### 别名

模型可以通过多种方式拼写每个角色：

| 规范名称       | 别名                                                              |
|---------------|------------------------------------------------------------------|
| `general`     | `worker`, `default`, `general-purpose`                           |
| `explore`     | `explorer`, `exploration`                                        |
| `plan`        | `planning`, `awaiter`                                            |
| `review`      | `reviewer`, `code-review`                                        |
| `implementer` | `implement`, `implementation`, `builder`                         |
| `verifier`    | `verify`, `verification`, `validator`, `tester`                  |
| `custom`      | （无；需要显式的 `allowed_tools` 数组）                             |

所有匹配均不区分大小写。未知值会产生一个类型化错误，列出可接受的集合，
以便模型在下一轮自行纠正。

## 并发上限

调度器默认将并发子代理数量上限设为 10（可通过 `~/.deepseek/config.toml`
中的 `[subagents].max_concurrent` 配置，硬上限为 20）。
当父级达到上限时，`agent_spawn` 返回带有上限值的错误；
父级应在重试前调用 `agent_wait` 等待完成或 `agent_cancel` 以释放一个槽位。

上限仅统计**运行中**的代理——已完成/失败/已取消的记录保留以供检查，
但不占用槽位。丢失了 `task_handle` 的代理（例如跨进程重启）也不计入上限。

## 生命周期

每次生成都会创建一个记录，其状态按以下顺序流转：

```
待定（Pending）→ 运行中（Running）→（已完成（Completed）| 失败（Failed(reason)）| 已取消（Cancelled）| 已中断（Interrupted(reason)））
```

当管理器检测到某个 `Running` 状态的代理的 task_handle 已消失时——通常是在
从 `~/.deepseek/subagents.v1.json` 加载代理的进程重启之后——会触发 `Interrupted` 状态。
父级可以调用 `agent_resume` 尝试继续执行，或将其视为终止状态。

### 会话边界（#405）

每个 `SubAgentManager` 实例在构造时都会为自己分配一个全新的
`session_boot_id`。每次生成都会用该 id 标记代理；持久化的状态文件
在重启后仍然保留该信息。

`agent_list` 默认**仅显示当前会话**：来自先前会话且当前未运行的代理会被过滤掉。
传递 `include_archived=true` 可以显示所有记录，并附带 `from_prior_session: true`
标志，以便模型区分存档记录和活动记录。

从 #405 之前的状态文件（没有 `session_boot_id` 字段）加载的记录被归类为
先前会话，因为管理器无法将它们与当前启动匹配。

## 输出契约

每个子代理都会生成一个包含五个部分的最终结果字符串，按以下顺序排列：

```
SUMMARY:    一段落；你做了什么以及发生了什么
CHANGES:    修改过的文件，附带一行描述；如果只读则为"None."
EVIDENCE:   路径:行范围的引用和关键发现；每条一行
RISKS:      可能出问题的地方 / 父级应该再次检查的内容
BLOCKERS:   阻止你完成的因素；如果顺利完成则为"None."
```

确切的格式位于 `crates/tui/src/prompts/subagent_output_format.md`。
父级将 `EVIDENCE` 作为下一个轮次的工作集读取，因此探索者和审查者在此处应精确详实。

## 记忆与 `remember` 工具（#489）

当记忆功能启用时（`[memory] enabled = true` 或 `DEEPSEEK_MEMORY=on`），
子代理继承父级的记忆文件。它们可以通过 `remember` 工具追加持久的笔记——
这对于发现值得跨会话保留的项目约定的探索者，或者了解到"这个测试不稳定"的验证者，
都非常有用。

记忆写入限定在用户自己的 `memory.md` 文件范围内；不经过标准的写入审批流程。

## 实现说明

- 源代码：`crates/tui/src/tools/subagent/mod.rs`（约 3500 行）。
- 持久化状态：`~/.deepseek/subagents.v1.json`。模式版本号 `1`
  （向前兼容——新的可选字段使用 `#[serde(default)]`）。
- `is_running` 检查会忽略 `task_handle` 为 `None` 的代理；这可以避免将
  持久化但已分离的记录计入并发上限（#509）。
- `SharedSubAgentManager` 是 `Arc<RwLock<...>>` —— 读路径使用读锁，
  以便 `/agents` 和侧边栏投影在多代理扇出期间不会阻塞主循环（#510）。
