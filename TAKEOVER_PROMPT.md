# v0.8.6 接管提示词——全新的 DeepSeek V4 会话

你将接管 `github.com/Hmbown/DeepSeek-TUI` 的 v0.8.6 冲刺。先前的一个 DeepSeek 会话不断被中断，因为在长时间运行的工作中，父会话变得过大。用户现在已经清理了本地保存的会话，但这只是暂时的缓解。你的工作是稳定分支并修复产品，使长时间运行的代理工作默认可以持续。

## 首要原则

不要将其作为一个长顺序的父会话来运行。

父会话是协调者。使用 `agent_spawn` 进行工具携带工作，使用 `rlm` 对较长的问题列表或文档进行批量分类/综合，并保持父会话记录小巧。如果你发现自己在为同一个主题逐个读取文件，停下来并委派。

## 紧急事项

从 #402 开始：

- `#402 P0: make long-running sessions survivable by default (Codex-style compaction + bounded transcript state)`

这是现在最高优先级，因为它导致了中断的控制权交接循环。问题正文指出了与 `/Volumes/VIXinSSD/codex-main` 相比的具体差距：

- DeepSeek TUI 保持无界的 `api_messages` 和可见的 `history`
- `auto_compact = false`，容量控制器默认关闭
- 保存的会话序列化完整的 `messages: Vec<Message>` 快照
- 关于压缩/子代理/并行执行的重要模拟引擎测试仍然被忽略，因为引擎需要具体的 `DeepSeekClient`
- Codex 有运行时前后压缩、替换历史记录、持久化的压缩列表项以及经过净化的最后-N 条子代理分支行为

不要将其视为文档或提示词调整。实施运行时护栏。

## 要验证的当前分支状态

分支应为 `feat/v0.8.6`。先前中断的会话有未完成的工作。在相信任何声明之前进行验证：

1. `git status --short --branch`
2. `cargo check --workspace --all-targets --locked`
3. 如果检查通过，运行 `cargo test --workspace --all-features --locked`
4. 阅读 `AGENTS.md`、`V086_BRIEF.md`、`docs/ARCHITECTURE.md` 和 issue #402

来自中断会话的已知部分工作：

- Goal 模式命令分发（`/goal`）——检查 `crates/tui/src/commands/goal.rs`
- 文件树面板——检查 `crates/tui/src/tui/file_tree.rs`
- 用户定义命令的管道——检查 `crates/tui/src/commands/user_commands.rs`
- `crates/tui/src/*` 中的本地化/侧边栏/渲染更改

不要覆盖不相关的脏文件。使用现有更改进行工作。

## 更新的 v0.8.6 问题集

最初的简报说有 23 个 issue，但实时的 v0.8.6 标签现在包含更多。使用以下命令刷新实时状态：

```bash
gh issue list --label v0.8.6 --state open --limit 100 --json number,title,body,labels
```

新的或特别相关的补充：

- `#402` P0 长会话可持续性：运行时压缩、有界记录/会话持久化
- `#401` 修剪过度防御性断言：删除脆弱的提示子串/快照风格测试
- `#400` 聊天/侧边栏文字渗漏：滚动时时间戳片段跨单元格残留
- `#399` 延迟/冻结审计：在 UI 线程上同步 git、无界历史 Vec、文件树阻塞遍历
- `#398` codex-mcp 对等：agent 风格 MCP 服务器工具加 `deepseek mcp add/list/get/remove`

现有的高优先级 v0.8.6 问题仍包括：

- `#397` Goal 模式
- `#396` 每轮缓存命中芯片
- `#395` 循环边界可视化
- `#394` 文件树面板
- `#393` 分享会话 URL
- `#392` `/model auto`
- `#391` 用户定义斜杠命令
- `#390` 配置（profile）热切换
- `#389` 内联 LSP 诊断
- `#388` 崩溃恢复提示
- `#387` 自更新
- `#386` `/init`
- `#385` `/diff`
- `#384` `/undo`
- `#383` `/edit`
- `#382` 折叠 Steer/Queue/Immediate
- `#380` 内联 diff 高亮
- `#379` 智能剪贴板
- `#378` 文档润色
- `#377` 缩小 App 状态
- `#376` 原生复制逃生
- `#375` 右键上下文菜单
- `#374` 可点击的 file:line
- `#373` 任务面板忽略 shell 作业

## 首小时执行计划

以扇出方式执行，而非串行调查。

1. 父级：创建一个包含以下分类的清单，然后运行一个批量的读取/状态轮次：`git status`、`gh issue list --label v0.8.6`、针对 compaction/session/history/capacity 的聚焦 `rg`，以及初始 cargo check。

2. 生成子代理 A：#402 运行时/会话可持续性。
   所有权：`crates/tui/src/core/engine.rs`、`crates/tui/src/compaction.rs`、`crates/tui/src/session_manager.rs`、`crates/tui/src/tui/app.rs`、`crates/tui/tests/integration_mock_llm.rs` 和相关配置文档。
   任务：设计和实施限制父模型历史/会话持久化并解除真实集成测试阻塞的最小运行时护栏切片。

3. 生成子代理 B：当前脏树编译修复。
   所有权：中断会话的部分 v0.8.6 文件：`commands/goal.rs`、`commands/user_commands.rs`、`tui/file_tree.rs`、`commands/mod.rs`、`localization.rs`、`tui/sidebar.rs`、`tui/ui.rs`。
   任务：使分支可编译，不扩大范围。

4. 生成子代理 C：UI 性能/渗漏分类（#399/#400/#394）。
   所有权：记录渲染/缓存、侧边栏渲染、文件树遍历。
   任务：修复回归问题，识别任何阻塞性的同步 UI 工作。

5. 生成子代理 D：问题/测试卫生分类（#401 加忽略的模拟测试）。
   所有权：脆弱测试、提示快照测试和被忽略的集成测试。
   任务：在适当的地方删除脆弱的断言，并将 #402 验收标准转化为真实测试。

6. 仅在需要时生成子代理 E：MCP 对等（#398）或命令表面跟进（#391/#397）。保持它与 #402 分离，这样 P0 修复就不会与功能工作纠缠在一起。

## RLM 使用

当输入大到在父级中粘贴/读取会使会话膨胀时，使用 `rlm`。这里适合使用 RLM 的任务：

- 将所有实时的 `v0.8.6` 问题正文分类为独立的实施分类；
- 通过给 RLM 提供两个仓库的摘录并询问有界的验收清单来比较 #402 与 Codex 文件；
- 批量审查与 #401 相关的较长测试列表中的脆弱断言；
- 将较长的 cargo/clippy 输出总结为文件所属的修复集群。

在 RLM 内部，对独立分类使用 `llm_query_batched()`，仅对递归批判/分解使用 `rlm_query()`。父级应该得到最终的综合，而不是每一个中间块。

## 会话生存规则

- 最多保持 5 个正在运行的子代理。
- 生成代理后，继续执行不重叠的本地协调工作。
- 仅在被结果阻塞时使用 `agent_wait`。
- 对已完成的代理使用 `agent_result`，并将结果总结到父级中。
- 在上下文使用率达到 60% 时建议 `/compact`，但不要将其依赖为产品级修复。
- 如果父级在同一个主题上达到 3 个连续轮次，则生成或使用 RLM。
- 不要将完整日志粘贴到父级中。将日志存储为工件，或让 RLM 总结它们。

## PR 工作流

使用 GitHub PR 作为额外的审查表面。不要让一个巨大的本地分支在没有外部检查的情况下堆积。

- 按 issue 或紧密相关的分类偏好小 PR：#402 可以有自己的 PR，编译修复可以有自己的 PR，UI 性能/回归修复可以有自己的 PR，命令表面功能可以是单独的 PR。
- 一旦每个切片可编译并有聚焦的测试，尽早推送工作分支并创建 PR。只有当 PR 实际满足 issue 时才包含 `Closes #...`。
- 让 CI 和任何 GitHub AI/代码审查代理检查代码。将审查评论视为真正的工作：用后续提交来回应，而不是挥手否决。
- 当 PR 通过检查时，将其合并到目标分支，并从更新后的分支继续。当 PR 需要修改时，进行修改，重新运行相关门禁，并在更新后的检查通过前等待。
- 使用 `gh pr view`、`gh pr checks` 和 `gh issue view` 保持父会话跟踪 PR 状态；除非验收已验证且合并没有自动关闭它们，否则不要手动关闭 issue。

## 验证门禁

在声明任何工作完成之前：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

对于 #402 特别地，还需要添加或启用聚焦测试，证明：

- 压缩/循环护栏在危险上下文增长之前运行；
- 实时的 `api_messages` 或等效的模型历史在压缩后是有界的；
- 可见的记录/会话持久化是有界或虚拟化的；
- 子代理结果注入父级被总结/有界；
- 子分支历史可以使用净化的最后-N 条行为；
- 会话保存/检查点不会重写任意大的完整记录。

## 最终报告格式

使用以下标题：

- 已实施
- 已验证
- 可安全关闭的 issue
- 仍打开的 issue 及原因
- 运行的命令
- 剩余风险

明确指出哪些是仅本地、哪些已提交、哪些已推送以及哪些仅是计划。除非验收标准已验证，否则不要关闭 issue。
