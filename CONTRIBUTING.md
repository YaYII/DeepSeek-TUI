# 参与 DeepSeek TUI 项目

感谢您有兴趣参与 DeepSeek TUI 项目！本文档提供了贡献指南和说明。

## 开始

### 前置条件

- Rust 1.88 或更高版本（edition 2024）
- Cargo 包管理器
- Git

### 设置开发环境

1. Fork 并克隆仓库：
   ```bash
   git clone https://github.com/YOUR_USERNAME/DeepSeek-TUI.git
   cd DeepSeek-TUI
   ```

2. 构建项目：
   ```bash
   cargo build
   ```

3. 运行测试：
   ```bash
   cargo test
   ```

4. 使用开发设置运行：
   ```bash
   cargo run
   ```

## 开发工作流

### 代码风格

- 提交前运行 `cargo fmt`，确保格式一致
- 运行 `cargo clippy` 并处理所有警告
- 遵循 Rust 命名规范（函数/变量使用 snake_case，类型使用 CamelCase）
- 为公共 API 添加文档注释

### 测试

- 为新功能编写测试
- 确保所有现有测试通过：`cargo test --workspace --all-features`
- 单元测试放在被测试代码旁边（标准 Rust `#[cfg(test)]` 模块），集成测试放在对应 crate 的 `tests/` 目录下（例如 `crates/tui/tests/` 或 `crates/state/tests/`）。仓库根目录的 `tests/` 目录不使用

### 提交信息

使用清晰、描述性的提交信息，遵循 conventional commits 规范：

- `feat:` 新功能
- `fix:` 错误修复
- `docs:` 文档变更
- `refactor:` 代码重构
- `test:` 添加或更新测试
- `chore:` 维护任务

示例：`feat: add doctor subcommand for system diagnostics`

## 项目结构

DeepSeek TUI 是一个 Cargo 工作区。实时代码和大部分 TUI、引擎及工具代码目前位于 `crates/tui/src/` 中。较小的工作区 crate 提供正在逐步提取的共享抽象。

```
crates/
├── tui/           deepseek-tui 二进制（交互式 TUI + 运行时 API）
├── cli/           deepseek 二进制（调度器外观）
├── app-server/    HTTP/SSE + JSON-RPC 传输
├── core/          智能体循环 / 会话 / 轮次管理
├── protocol/      请求/响应帧
├── config/        配置加载、配置文件（profile）、环境变量优先级
├── state/         SQLite 线程/会话持久化
├── tools/         类型化工具定义和生命周期
├── mcp/           MCP 客户端 + stdio 服务器
├── hooks/         生命周期钩子（stdout/jsonl/webhook）
├── execpolicy/    审批/沙箱策略引擎
├── agent/         模型/提供方注册表
└── tui-core/      事件驱动 TUI 状态机构架
```

有关这些 crate 的实时数据流，请参阅 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)；构建顺序请参阅 [DEPENDENCY_GRAPH.md](DEPENDENCY_GRAPH.md)。

## 提交变更

1. 从 `main` 创建功能分支：
   ```bash
   git checkout -b feat/your-feature
   ```

2. 进行修改并提交

3. 确保 CI 通过：
   ```bash
   cargo fmt --check
   cargo clippy
   cargo test
   ```

4. 推送分支并创建 Pull Request

5. 在 PR 描述中清晰说明您的变更

## Pull Request 指南

- 保持 PR 聚焦于单一变更
- 必要时更新文档
- 为新功能添加测试
- 确保在请求审查前 CI 通过

## 典型 PR 的结构

一个结构良好的 PR 遵循一致的模式。近期示例包括：

- **#386** — `/init` 命令：新建 `crates/tui/src/commands/init.rs` 模块、项目类型检测、AGENTS.md 生成、命令注册到 `commands/mod.rs`、本地化字符串
- **#389** — 内联 LSP 诊断：`crates/tui/src/lsp/` 中的 LSP 子系统、`core/engine/lsp_hooks.rs` 中的引擎钩子、配置开关、测试覆盖
- **#387** — 自更新：新建 `crates/cli/src/update.rs` 模块、CLI 子命令注册、HTTP 下载 + SHA256 校验 + 原子二进制替换
- **#393** — `/share` 会话 URL：新建 `crates/tui/src/commands/share.rs`、HTML 渲染、`gh gist create` 集成、命令注册
- **#343/#346** —（v0.8.5）运行时线程/轮次时间线和持久化任务管理器重构

通常每个 PR 涉及 1-3 个新文件，修改 2-5 个现有文件用于接入（注册表、分发匹配、本地化），并添加或更新测试。变更范围限于单一功能或修复——如果您发现有需要做的相关工作，请单独开 issue，而不是扩大 PR 范围。

提交前，运行：
```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features 2>&1 | head -50
cargo check
```

## 报告问题

报告问题时，请包含：

- 操作系统及版本
- Rust 版本（`rustc --version`）
- DeepSeek TUI 版本（`deepseek --version`）
- 重现问题的步骤
- 预期行为与实际行为
- 相关错误信息或日志

## 行为准则

请保持尊重和包容。我们欢迎各种背景和经验水平的贡献者。

## 许可

通过为 DeepSeek TUI 贡献代码，您同意您的贡献将根据 MIT 许可证进行许可。

## 有问题？

如有关于贡献的任何问题，请随时创建 issue。