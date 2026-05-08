# PTY/帧捕获 TUI QA 测试框架

用于集成测试的小型辅助工具，这些测试需要像真实用户在真实终端中打字一样驱动 `deepseek-tui`——包括按键、粘贴、调整大小，以及对解析后的终端帧和工作区文件系统的断言。

## 何时使用

当某个 bug 仅在 **交互式** 终端中显现时，请使用此测试框架：粘贴行为、斜杠菜单、模式切换、视口渲染、引导流程、调整大小、鼠标捕获。任何使用 `TestBackend` 或对底层状态机进行单元测试与用户实际所见内容相去甚远的情况。

对于 `App`、`SkillRegistry`、引擎的 `Op` / `Event` 管道等的纯逻辑测试，请继续使用 `crates/tui/src/.../tests` 风格的单元测试。不要仅仅为了断言一个函数返回了正确的值而启动一个 PTY。

## 结构

- `pty.rs` — `PtySession`。在真实的 PTY 中启动一个二进制文件（通过 `portable-pty`），将子进程的 stdout 泵入后台线程的缓冲区，暴露 `write_bytes`、`resize`、`drain`、`shutdown` 方法。
- `frame.rs` — `Frame`。包装了 `vt100::Parser`。输入字节，然后查询输出：`text()`、`row(y)`、`contains(s)`、`cursor()`、`debug_dump()`。
- `keys.rs` — 按键的字节序列构造器（`key::ctrl('c')`、`key::enter()`、`key::tab()`、……）以及粘贴的构造器（`paste::bracketed(s)`、`paste::unbracketed(s)`）。
- `harness.rs` — `Harness`。组合上述组件。提供 `wait_for`、`wait_for_text`、`wait_for_idle`，以及用于临时目录 HOME 的 `make_sealed_workspace()`。

## 添加新场景

1. 选择能复现用户可见行为的最小输入集。如果缺少真实 LLM 对话轮次就无法复现，则该场景可能更适合放在单元测试（或 `wiremock` 驱动的对话测试）中。

2. 构建一个密封的工作区，使场景不会看到开发者的真实 `~/.deepseek/` 或 API 密钥：

   ```rust
   let ws = qa_harness::harness::make_sealed_workspace()?;
   std::fs::write(ws.user_skills_dir().join("foo/SKILL.md"), "...")?;
   ```

3. 启动：

   ```rust
   let mut h = Harness::builder(Harness::cargo_bin("deepseek-tui"))
       .cwd(ws.workspace())
       .seal_home(ws.home())
       .env("DEEPSEEK_API_KEY", "ci-test-key")
       .args(["--workspace", ws.workspace().to_str().unwrap(),
              "--no-project-config", "--skip-onboarding"])
       .size(40, 120)
       .spawn()?;
   ```

4. 驱动它：

   ```rust
   h.wait_for_text("Composer", Duration::from_secs(10))?;
   h.send(keys::key::ch('/'))?;
   h.wait_for_text("/skills", Duration::from_secs(2))?;
   ```

5. 断言：

   ```rust
   let f = h.frame();
   assert!(f.contains("local-skill"), "frame:\n{}", f.debug_dump());
   ```

6. 最后始终要干净地关闭，以便即使断言失败时 PTY 清理也能运行：

   ```rust
   let _ = h.shutdown();
   ```

## 约定

- **始终使用密封环境。** 任何场景都不应看到真实的 `$HOME/.deepseek/` 或连接到 `api.deepseek.com`。如果某个场景必须进行真实的模型对话，请通过本地的 `wiremock` 或 `tiny_http` 伪造提供商，并设置 `DEEPSEEK_BASE_URL=<localhost>`。
- **失败时发出响亮警告。** 当断言失败时，打印 `frame.debug_dump()`，以便 CI 日志显示渲染的屏幕，而不仅仅是 `assertion failed`。
- **优先使用 `wait_for_text` 而非 `sleep`。** 在断言前休眠 500ms 的场景在 CI 负载下会产生不稳定结果。而使用 10 秒超时进行轮询的场景则很稳健。
- **首次启动时输出可能较慢。** TUI 在显示编辑器之前会进行配置探测、技能安装和快照清理。为启动过程预留至少 10–15 秒，然后再判定超时。

## 平台

`portable-pty` 支持 macOS、Linux 和 Windows（ConPTY）。目前场景仅针对 Unix——测试二进制文件通过 `#![cfg(unix)]` 进行门控，直到 Windows 特定的输入管道在同一测试框架下完成审查。
