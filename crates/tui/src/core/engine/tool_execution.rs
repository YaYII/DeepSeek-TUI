//! 引擎轮次循环的底层工具执行辅助函数。
//!
//! 本模块将 MCP 调度、执行锁定和并行工具分发的
//! 机制从 `engine.rs` 中分离出来；轮次循环仍然拥有计划、
//! 审批以及工具结果如何写回会话状态的所有权。

use std::{fs::OpenOptions, io::Write, sync::Arc, time::Duration};

use super::*;

/// RAII 守卫，在交互式工具执行期间暂停 TUI 的终端状态所有权，
/// 然后在 drop 时恢复。
///
/// 背景：交互式工具（任何需要原始 TTY 的工具 — 外部编辑器、
/// 带 stdin 的 `exec_shell` 等）需要 TUI 离开备用屏幕、
/// 禁用原始模式并释放鼠标捕获，以便子进程看到正常的
/// 终端。TUI 监听 `Event::PauseEvents` / `Event::ResumeEvents`
/// 并相应地运行 `pause_terminal` / `resume_terminal`。
///
/// 早期代码在工具执行前发送 `PauseEvents`，在工具执行后
/// 发送 `ResumeEvents`。这在正常路径上有效，但如果工具的
/// future 被丢弃 — Ctrl+C 取消、子代理中止、在工具等待时
/// 父任务被取消 — 则第二个 `await` 永远不会到达，
/// `ResumeEvents` 也永远不会被发送。它还允许交互式子进程
/// 在 UI 实际离开备用屏幕/原始模式之前启动。这两种失败都
/// 会使 TUI 陷入普通的 shell 回滚：父 shell 滚动条接管，
/// 鼠标滚轮滚动主机终端而不是对话记录，TUI 在 cooked 模式
/// 输出的底部渲染。
///
/// `Drop` 同步运行且无法 await，因此我们首先在**事件通道的
/// 克隆**上使用 `try_send` 以非阻塞方式推送 `ResumeEvents`。
/// 如果通道已满，我们将恢复操作排队到活动的 Tokio 运行时上，
/// 而不是丢弃它；否则引擎事件的突发可能使 UI 陷入暂停的
/// 终端状态。
pub(super) struct InteractiveTerminalGuard {
    tx: Option<mpsc::Sender<Event>>,
}

impl InteractiveTerminalGuard {
    /// 发送 `PauseEvents` 并装备守卫。如果 `interactive` 为 false，
    /// 则守卫为空操作 — `Drop` 将跳过恢复。
    pub(super) async fn engage(tx: mpsc::Sender<Event>, interactive: bool) -> Self {
        if !interactive {
            return Self { tx: None };
        }
        // Best-effort: if the receiver is gone the TUI has already shut down
        // and there's nothing to restore. If the event is delivered, wait for
        // the UI to actually release the terminal before starting the child.
        let ack = Arc::new(tokio::sync::Notify::new());
        match tx
            .send(Event::PauseEvents {
                ack: Some(ack.clone()),
            })
            .await
        {
            Ok(()) => {
                if tokio::time::timeout(Duration::from_millis(750), ack.notified())
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        target: "engine.tool_execution",
                        "InteractiveTerminalGuard: timed out waiting for terminal pause ack; \
                         continuing with interactive tool"
                    );
                }
            }
            Err(err) => {
                tracing::debug!(
                    target: "engine.tool_execution",
                    ?err,
                    "InteractiveTerminalGuard: event channel closed before PauseEvents"
                );
            }
        }
        Self { tx: Some(tx) }
    }
}

impl Drop for InteractiveTerminalGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            match tx.try_send(Event::ResumeEvents) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(event)) => {
                    match tokio::runtime::Handle::try_current() {
                        Ok(handle) => {
                            handle.spawn(async move {
                                if let Err(err) = tx.send(event).await {
                                    tracing::warn!(
                                        target: "engine.tool_execution",
                                        ?err,
                                        "InteractiveTerminalGuard: async send(ResumeEvents) failed; \
                                         terminal may stay in paused state until the next \
                                         pause/resume cycle"
                                    );
                                }
                            });
                        }
                        Err(err) => {
                            tracing::warn!(
                                target: "engine.tool_execution",
                                ?err,
                                "InteractiveTerminalGuard: event channel full and no Tokio runtime \
                                 available to queue ResumeEvents; terminal may stay paused until \
                                 the next pause/resume cycle"
                            );
                        }
                    }
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    tracing::debug!(
                        target: "engine.tool_execution",
                        "InteractiveTerminalGuard: event channel closed before ResumeEvents"
                    );
                }
            }
        }
    }
}

pub(super) fn emit_tool_audit(event: serde_json::Value) {
    let Some(path) = std::env::var_os("DEEPSEEK_TOOL_AUDIT_LOG") else {
        return;
    };
    let line = match serde_json::to_string(&event) {
        Ok(line) => line,
        Err(_) => return,
    };
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

impl Engine {
    pub(super) async fn execute_mcp_tool_with_pool(
        pool: Arc<AsyncMutex<McpPool>>,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let mut pool = pool.lock().await;
        let result = pool
            .call_tool(name, input)
            .await
            .map_err(|e| ToolError::execution_failed(format!("MCP tool failed: {e}")))?;
        let content = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
        Ok(ToolResult::success(content))
    }

    pub(super) async fn execute_parallel_tool(
        &mut self,
        input: serde_json::Value,
        tool_registry: Option<&crate::tools::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
    ) -> Result<ToolResult, ToolError> {
        let calls = parse_parallel_tool_calls(&input)?;
        let mcp_pool = if calls.iter().any(|(tool, _)| McpPool::is_mcp_tool(tool)) {
            Some(self.ensure_mcp_pool().await?)
        } else {
            None
        };
        let Some(registry) = tool_registry else {
            return Err(ToolError::not_available(
                "tool registry unavailable for multi_tool_use.parallel",
            ));
        };

        let mut tasks = FuturesUnordered::new();
        for (tool_name, tool_input) in calls {
            if tool_name == MULTI_TOOL_PARALLEL_NAME {
                return Err(ToolError::invalid_input(
                    "multi_tool_use.parallel cannot call itself",
                ));
            }
            if McpPool::is_mcp_tool(&tool_name) {
                if !mcp_tool_is_parallel_safe(&tool_name) {
                    return Err(ToolError::invalid_input(format!(
                        "Tool '{tool_name}' is an MCP tool and cannot run in parallel. \
                         Allowed MCP tools: list_mcp_resources, list_mcp_resource_templates, \
                         mcp_read_resource, read_mcp_resource, mcp_get_prompt."
                    )));
                }
            } else {
                let Some(spec) = registry.get(&tool_name) else {
                    return Err(ToolError::not_available(format!(
                        "tool '{tool_name}' is not registered"
                    )));
                };
                if !spec.is_read_only() {
                    return Err(ToolError::invalid_input(format!(
                        "Tool '{tool_name}' is not read-only and cannot run in parallel"
                    )));
                }
                if spec.approval_requirement() != ApprovalRequirement::Auto {
                    return Err(ToolError::invalid_input(format!(
                        "Tool '{tool_name}' requires approval and cannot run in parallel"
                    )));
                }
                if !spec.supports_parallel() {
                    return Err(ToolError::invalid_input(format!(
                        "Tool '{tool_name}' does not support parallel execution"
                    )));
                }
            }

            let registry_ref = registry;
            let lock = tool_exec_lock.clone();
            let tx_event = self.tx_event.clone();
            let mcp_pool = mcp_pool.clone();
            tasks.push(async move {
                let result = Engine::execute_tool_with_lock(
                    lock,
                    true,
                    false,
                    tx_event,
                    tool_name.clone(),
                    tool_input.clone(),
                    Some(registry_ref),
                    mcp_pool,
                    None,
                )
                .await;
                (tool_name, result)
            });
        }

        let mut results = Vec::new();
        while let Some((tool_name, result)) = tasks.next().await {
            match result {
                Ok(output) => {
                    let mut error = None;
                    if !output.success {
                        error = Some(output.content.clone());
                    }
                    results.push(ParallelToolResultEntry {
                        tool_name,
                        success: output.success,
                        content: output.content,
                        error,
                    });
                }
                Err(err) => {
                    let message = format!("{err}");
                    results.push(ParallelToolResultEntry {
                        tool_name,
                        success: false,
                        content: format!("Error: {message}"),
                        error: Some(message),
                    });
                }
            }
        }

        ToolResult::json(&ParallelToolResult { results })
            .map_err(|e| ToolError::execution_failed(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_tool_with_lock(
        lock: Arc<RwLock<()>>,
        supports_parallel: bool,
        interactive: bool,
        tx_event: mpsc::Sender<Event>,
        tool_name: String,
        tool_input: serde_json::Value,
        registry: Option<&crate::tools::ToolRegistry>,
        mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
        context_override: Option<crate::tools::ToolContext>,
    ) -> Result<ToolResult, ToolError> {
        let _guard = if supports_parallel {
            ToolExecGuard::Read(lock.read().await)
        } else {
            ToolExecGuard::Write(lock.write().await)
        };

        // RAII pause/resume: ensures `Event::ResumeEvents` always fires on
        // drop, even if the tool future is cancelled mid-await. See
        // `InteractiveTerminalGuard` doc-comment for the regression this
        // closes (parent terminal scrollback hijacking the TUI after a
        // cancelled interactive tool).
        let _terminal = InteractiveTerminalGuard::engage(tx_event, interactive).await;

        if McpPool::is_mcp_tool(&tool_name) {
            if let Some(pool) = mcp_pool {
                Engine::execute_mcp_tool_with_pool(pool, &tool_name, tool_input).await
            } else {
                Err(ToolError::not_available(format!(
                    "tool '{tool_name}' is not registered"
                )))
            }
        } else if let Some(registry) = registry {
            registry
                .execute_full_with_context(&tool_name, tool_input, context_override.as_ref())
                .await
        } else {
            Err(ToolError::not_available(format!(
                "tool '{tool_name}' is not registered"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{sync::Mutex, time::Duration};

    /// Tests in this module mutate `DEEPSEEK_TOOL_AUDIT_LOG` which is
    /// process-global; serialise through this guard so the parallel
    /// runner doesn't observe interleaved env mutations.
    static AUDIT_TEST_GUARD: Mutex<()> = Mutex::new(());

    fn audit_test_guard() -> std::sync::MutexGuard<'static, ()> {
        AUDIT_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[tokio::test]
    async fn terminal_guard_queues_resume_when_event_channel_is_full() {
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send(Event::status("filler")).expect("fill channel");

        drop(InteractiveTerminalGuard { tx: Some(tx) });

        assert!(matches!(rx.recv().await, Some(Event::Status { .. })));
        let resumed = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("queued resume event")
            .expect("event channel still open");
        assert!(matches!(resumed, Event::ResumeEvents));
    }

    #[tokio::test]
    async fn terminal_guard_waits_for_pause_ack_before_returning() {
        let (tx, mut rx) = mpsc::channel(4);
        let task = tokio::spawn(InteractiveTerminalGuard::engage(tx, true));

        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("pause event")
            .expect("event channel still open");
        let ack = match event {
            Event::PauseEvents { ack: Some(ack) } => ack,
            other => panic!("expected PauseEvents with ack, got {other:?}"),
        };

        tokio::task::yield_now().await;
        assert!(!task.is_finished(), "guard returned before pause ack");

        ack.notify_one();
        let guard = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("guard returned after ack")
            .expect("guard task joined");

        drop(guard);
        let resumed = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("resume event")
            .expect("event channel still open");
        assert!(matches!(resumed, Event::ResumeEvents));
    }

    #[test]
    fn emit_tool_audit_writes_jsonl_line_when_env_var_set() {
        let _g = audit_test_guard();
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("audit.log");
        // SAFETY: serialised by the guard above.
        unsafe {
            std::env::set_var("DEEPSEEK_TOOL_AUDIT_LOG", &path);
        }

        emit_tool_audit(json!({
            "event": "tool.spillover",
            "tool_id": "call-abc",
            "tool_name": "exec_shell",
            "path": "/tmp/foo.txt",
        }));
        emit_tool_audit(json!({
            "event": "tool.result",
            "tool_id": "call-xyz",
            "success": true,
        }));

        let body = std::fs::read_to_string(&path).expect("audit log written");
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2, "two emits → two lines");

        // Each line round-trips as JSON, has the expected event key.
        let first: serde_json::Value = serde_json::from_str(lines[0]).expect("first line is JSON");
        assert_eq!(
            first.get("event").and_then(|v| v.as_str()),
            Some("tool.spillover")
        );
        assert_eq!(
            first.get("tool_id").and_then(|v| v.as_str()),
            Some("call-abc")
        );

        let second: serde_json::Value =
            serde_json::from_str(lines[1]).expect("second line is JSON");
        assert_eq!(
            second.get("event").and_then(|v| v.as_str()),
            Some("tool.result")
        );

        // SAFETY: cleanup under the guard.
        unsafe {
            std::env::remove_var("DEEPSEEK_TOOL_AUDIT_LOG");
        }
    }

    #[test]
    fn emit_tool_audit_is_noop_when_env_var_unset() {
        let _g = audit_test_guard();
        // SAFETY: serialised by the guard above.
        unsafe {
            std::env::remove_var("DEEPSEEK_TOOL_AUDIT_LOG");
        }
        // Should not panic and should not create any file. We can't
        // assert "no file written" without knowing where one might be
        // written, but the contract is "do nothing", which we verify
        // by ensuring the call returns without error.
        emit_tool_audit(json!({"event": "noop", "x": 1}));
        // Successful return is the assertion.
    }

    #[test]
    fn emit_tool_audit_creates_parent_directory() {
        let _g = audit_test_guard();
        let tmp = tempfile::tempdir().expect("tempdir");
        // Path with a parent that doesn't exist yet — the writer
        // should create it.
        let nested = tmp.path().join("nested").join("dir").join("audit.log");
        // SAFETY: serialised by the guard above.
        unsafe {
            std::env::set_var("DEEPSEEK_TOOL_AUDIT_LOG", &nested);
        }
        emit_tool_audit(json!({"event": "test"}));
        assert!(nested.exists(), "writer should mkdir -p the parent chain");

        // SAFETY: cleanup under the guard.
        unsafe {
            std::env::remove_var("DEEPSEEK_TOOL_AUDIT_LOG");
        }
    }
}
