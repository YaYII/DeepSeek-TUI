# v0.8.6 待办清单 — AI 代理工作简报

这是一份结构化的简报，供另一个 AI（Claude Opus、DeepSeek V4 或类似模型）理解完整的 v0.8.6 范围并开始实施。仓库为
`github.com/Hmbown/DeepSeek-TUI` — Rust 工作空间，面向 DeepSeek V4 的 TUI 编码代理。

**分支**：从 `main`（当前 HEAD 在 v0.8.5 标签处）创建 `feat/v0.8.6`。  
**全部 23 个 issue 均已标记 `v0.8.6`**，存放在仓库的 GitHub Issues 中。  
**此列表之外无未解决的 issue**——面板是干净的。

## 项目背景

DeepSeek TUI 是一个终端原生编码代理。关键架构要点：
- **Dispatcher 二进制**（`deepseek`）委托给 TUI 二进制（`deepseek-tui`）
- **Crate 映射**：`crates/tui` 是主 crate；`crates/cli` 处理 CLI 入口；
  `crates/config`、`crates/core`、`crates/tools` 等是子 crate
- **引擎模式**：`core/engine.rs` 运行代理循环，处理工具调用
- **TUI**：基于 ratatui，替代屏幕，编辑器在底部，侧边栏在右侧
- **配置**：`~/.deepseek/config.toml`，配置文件、提供者、设置
- **首先阅读的关键文件**：`docs/ARCHITECTURE.md`、`crates/tui/src/main.rs`、
  `crates/tui/src/tui/app.rs`、`crates/tui/src/core/engine.rs`

在仓库根目录阅读 `AGENTS.md` 和 `CLAUDE.md` 获取构建/测试命令。

---

## v0.8.6 Issues — 按主题分组

### 组 A：UX 打磨 — 转录与剪贴板（5 个 issues）

| # | 标题 | 简述 |
|---|------|------|
| 380 | 内联差异高亮 | 在 apply_patch/edit_file 结果中为 +/- 着色 |
| 379 | 智能剪贴板 Ctrl+Y | 将焦点单元格复制到系统剪贴板 |
| 375 | 右键上下文菜单 | 每个单元格的菜单：复制、在编辑器中打开、重新运行、隐藏 |
| 374 | 可点击的 file:line | 在工具输出的 path:line 上添加 OSC-8 超链接 |
| 376 | 原生复制转义 | 按住 Shift 绕过替代屏幕以便终端选择 |

### 组 B：工作区 UX — 导航与可见性（4 个 issues）

| # | 标题 | 简述 |
|---|------|------|
| 394 | 文件树面板 | Ctrl+E 切换左侧工作区导航器 |
| 395 | 轮次边界可视化 | 在连贯性周期之间插入内联分隔线 |
| 396 | 每次轮次缓存命中标签 | 底部栏在每次轮次后显示缓存命中百分比 |
| 388 | 崩溃恢复提示 | 重启时，提供恢复中断轮次的选项 |

### 组 C：会话与历史（3 个 issues）

| # | 标题 | 简述 |
|---|------|------|
| 383 | /edit — 修改后重新提交 | 将最后一条消息拉回编辑器，重新运行轮次 |
| 384 | /undo — 撤销上次补丁 | 对 apply_patch/edit_file/write_file 进行手术式撤销 |
| 385 | /diff — 会话变更 | 显示从会话开始以来的 git diff |

### 组 D：工具与智能（4 个 issues）

| # | 标题 | 简述 |
|---|------|------|
| 389 | 内联 LSP 诊断 | 在每个补丁后显示 rust-analyzer 错误 |
| 386 | /init — 引导 AGENTS.md | 自动检测项目类型，编写初始 AGENTS.md |
| 391 | 用户定义的斜杠命令 | ~/.deepseek/commands/<name>.md 模板 |
| 392 | /model auto | 每次轮次的启发式 Pro 与 Flash 路由选择 |

### 组 E：基础设施与共享（4 个 issues）

| # | 标题 | 简述 |
|---|------|------|
| 390 | /profile — 热切换配置 | 在会话内切换配置配置文件，无需重启 |
| 393 | /share — 会话 URL |将会话导出为静态 HTML，上传到 gist/S3 |
| 387 | 应用内自更新 | deepseek update 获取并替换二进制文件 |
| 397 | 目标模式 | 明确的目标、token 预算、自我验证工具 |

### 组 F：质量与修复（3 个 issues）

| # | 标题 | 简述 |
|---|------|------|
| 382 | 合并 Steer/Queue/Immediate | 单一心智模型——一切皆排队，Ctrl+Enter 引导 |
| 373 | 侧边栏任务面板忽略 shell 作业 | 将 shell 作业接入任务面板 |
| 377 | 缩减 App 状态 | 将约 200 个字段分组为类型化子状态 |
| 378 | 文档：精简 README + ARCHITECTURE | 面向外部读者的润色 |

---

## 建议的实施顺序

### 第 1 波：基础（从这里开始）
1. **#377（重构 App 状态）** — 首先做这个。在添加更多字段之前，将字段分组为子状态结构体。
   后续每个功能都会触及 App。
2. **#382（合并 Steer/Queue）** — UX 清晰度修复，实施风险低。
3. **#373（任务面板 shell 作业）** — bug 修复，风险低。

### 第 2 波：转录 UX
4. **#380（内联差异高亮）** — 对工具输出进行解析处理，可见价值高。
5. **#374（可点击的 file:line）** — OSC-8 超链接，可发现性高。
6. **#379（智能剪贴板 Ctrl+Y）** — 小功能，大大改善人体工程学。
7. **#375（右键上下文菜单）** — 依赖于鼠标事件管道。
8. **#376（原生复制转义）** — 终端选择修复。

### 第 3 波：会话工具
9. **#383（/edit）** — 需要引擎截断路径。
10. **#384（/undo）** — 依赖于快照基础设施。
11. **#385（/diff）** — 使用快照仓库，依赖于 #380 进行渲染。
12. **#388（崩溃恢复提示）** — 使用现有的检查点基础设施。

### 第 4 波：智能
13. **#386（/init）** — 项目检测 + AGENTS.md 生成。
14. **#389（LSP 诊断）** — 轮询现有 LSP 客户端，维护成本低。
15. **#391（用户定义命令）** — 技能加载器复用。
16. **#392（/model auto）** — 启发式路由器，DeepSeek 特有。

### 第 5 波：可见性与共享
17. **#394（文件树面板）** — 工作区导航器。
18. **#395（轮次边界可视化）** — 连贯性周期分隔线。
19. **#396（缓存命中标签）** — 底部栏标签，简单添加。
20. **#393（/share）** — HTML 导出，gist/S3 后端。
21. **#387（自更新）** — 二进制获取 + 验证 + 替换。

### 第 6 波：文档与目标模式
22. **#378（文档润色）** — README + ARCHITECTURE 刷新。
23. **#397（目标模式）** — 最大的功能，最后做（受益于之前所有工作）。

---

## 工作模式

- **每个 issue 对应一个 PR**（或小集群）。每个合并的 PR 关闭一个 issue。
- **先分解**：阅读 issue 正文，识别需要变更的文件，
  创建 `checklist_write`，然后实施。
- **测试关卡**：`cargo test --workspace --all-features` 必须在每个 PR 前通过。
- **Lint 关卡**：`cargo clippy --workspace --all-targets --all-features -- -D warnings` 保持干净。
- **格式化**：提交前运行 `cargo fmt --all`。
- **GitHub**：推送到 `feat/v0.8.6`，向 `main` 创建 PR。使用 `gh` CLI。
- **无未解决的 issue**，除了 v0.8.6 列表——如果出现新 issue，创建它们但不阻塞当前工作。

## 关键资源

- 仓库：`https://github.com/Hmbown/DeepSeek-TUI`
- 架构文档：`docs/ARCHITECTURE.md`
- 配置参考：`docs/CONFIGURATION.md`
- CLI：`gh issue list --label v0.8.6 --json number,title,body` 获取完整的 issue 文本
