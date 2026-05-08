# `crates/tui/tests/`

TUI 二进制文件的集成测试。根据 `CONTRIBUTING.md`，每个 crate 的
集成测试位于各自的 `tests/` 目录中；仓库根目录下的 `tests/`
目录未被使用。

## Mock LLM 客户端（`integration_mock_llm.rs`）

`crates/tui/src/llm_client/mock.rs` 提供了 `MockLlmClient`，它通过重放队列驱动的预置响应并捕获所有发出的 `MessageRequest` 来实现 `LlmClient` trait。测试在 **trait 边界** 处进行 mock——绝不在 `reqwest` HTTP 层进行——因为 trait 是运行时所应依赖的持久抽象。

当前的覆盖范围端到端地覆盖了 trait 的表面：

- 流式对话循环
- 跨工具调用轮次的推理内容重放（V4 §5.1.1，该 bug 影响了 v0.4.9-v0.5.1）
- 带分块输入 JSON 的工具调用往返
- 单轮内的多工具调用排序
- 压缩风格的非流式 `create_message`
- 子代理风格的独立父/子 mock
- 流排空前的容量门控请求捕获观察

四个全引擎测试（`engine_full_*`）被标记为 `#[ignore]`。当 `core::engine::Engine` 被重构为接受 `Arc<dyn LlmClient>` 而非具体的 `Option<DeepSeekClient>` 时，这些测试将被解除阻塞。有关确切的重构范围，请参见 `integration_mock_llm.rs` 底部的注释块。

## `deepseek eval` 的 `--record` 模式

离线 `deepseek eval` 测试框架现在接受 `--record <DIR>`。设置后，每个工具步骤会将一条 JSON Lines 记录追加到 `<DIR>/<scenario>.jsonl`（默认场景名称：`offline-tool-loop.jsonl`）。每行是一个自包含的 JSON 对象，遵循以下 schema：

```json
{ "request":  { "step": "list_dir", "kind": "List" },
  "response_events": [ { "type": "ok", "output": "…" } ] }
```

Mock LLM 客户端（`crate::llm_client::mock`）通过将每个 `response_events` 数组映射到预置的 `Vec<StreamEvent>` 来重放这些测试夹具。将生成的夹具放入 `crates/tui/tests/fixtures/` 目录，使其随仓库一起提交并在 CI 中提供给 mock 使用。

快速示例：

```bash
cargo run --bin deepseek -- eval --record crates/tui/tests/fixtures
cat crates/tui/tests/fixtures/offline-tool-loop.jsonl | jq .
```

场景名称在形成文件名之前会被清理为 `[A-Za-z0-9_-]`，以确保特殊的场景字符串在不同平台上保持可移植性。
