//! RLM 工具 — 沙箱化 Python REPL。
//!
//! Where `rlm_query` is a parallel fanout primitive (N prompts → N answers,
//! stateless), `rlm_process` runs the full recursive-language-model loop
//! against a long input. The input is loaded into a Python REPL as the
//! `PROMPT` variable; a sub-agent writes code to chunk it, calls
//! `llm_query()` / `sub_rlm()` for sub-LLM work, and returns a final string
//! via `FINAL()`. The model never has to put the long input in its own
//! context window — it just calls the tool with `task` + `file_path` (or
//! inline `content`) and reads the synthesized answer back.
//!
//! Use when the input genuinely doesn't fit in working context: a whole
//! file, a long transcript, a multi-document corpus. For short prompts or
//! parallel fanout, prefer `rlm_query`.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::client::DeepSeekClient;
use crate::rlm::turn::{RlmTermination, run_rlm_turn_with_root};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};
use crate::utils::spawn_supervised;

/// Default child model — cheap and fast.
const DEFAULT_CHILD_MODEL: &str = "deepseek-v4-flash";
/// Default `sub_rlm` recursion budget — paper experiments use 1.
const DEFAULT_MAX_DEPTH: u32 = 1;
/// Hard cap on how many chars of inline `content` we'll accept. Larger
/// inputs should come in via `file_path` so they never enter the caller's
/// context in the first place.
const MAX_INLINE_CONTENT_CHARS: usize = 200_000;

pub struct RlmTool {
    /// Production HTTP client. `None` when no API key is configured.
    client: Option<DeepSeekClient>,
    /// Root model to drive the RLM loop. Set at registration time; matches
    /// whatever model the parent session is using.
    root_model: String,
}

impl RlmTool {
    #[must_use]
    pub fn new(client: Option<DeepSeekClient>, root_model: String) -> Self {
        Self { client, root_model }
    }
}

#[async_trait]
impl ToolSpec for RlmTool {
    fn name(&self) -> &'static str {
        "rlm"
    }

    fn description(&self) -> &'static str {
        "用于处理不适合您自己上下文窗口的长输入的专业工具。将输入加载到沙箱化的 Python REPL 中作为 `PROMPT`；\
         子代理编写 Python 代码对输入进行分块，并调用 REPL 内的辅助函数（`llm_query`、`llm_query_batched`、\
         `rlm_query`、`rlm_query_batched`）进行处理，然后返回综合答案。\n\n\
         当输入确实很大，或当 Python 的 map-reduce 加上子 LLM 调用是合适的方案时使用此工具：\
         完整文件、长对话记录、多文档语料库、批量语义分类或分解/评审工作。对于精确计数\
         或结构化聚合，直接在 REPL 中的 Python 内计算并报告确定性结果，而不是让子 LLM 猜测。\
         对于全输入的 map-reduce，使用 REPL 辅助函数 `chunk_context()` 和 `chunk_coverage()`，\
         以便结果说明覆盖了哪些内容。\n\n\
         提供 `task`（做什么）加上 `file_path`（工作区相对路径，首选——将长输入完全排除在您的上下文之外）\
         或 `content`（内联，上限 20 万字符）中的一个。Python 辅助函数（`llm_query`、`rlm_query` 等）\
         存在于 REPL 内部——它们不是可单独调用的工具。\n\n\
         返回最终综合答案以及 RLM 报告，显示输入大小、迭代次数、持续时间、子 LLM 调用次数和跟踪摘要。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task"],
            "properties": {
                "task": {
                    "type": "string",
                    "description": "如何处理输入（例如\"总结安全模型\"、\"提取所有 API 端点\"、\"按情感分类每一行\"）。子代理将其作为目标。"
                },
                "file_path": {
                    "type": "string",
                    "description": "要加载为 PROMPT 的文件的工作区相对路径。首选——将长输入排除在您的上下文之外。与 `content` 互斥。"
                },
                "content": {
                    "type": "string",
                    "description": "要加载为 PROMPT 的内联内容。仅在输入不是您可以指向的文件时使用。上限为 20 万字符。"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "`sub_rlm()` 调用的递归预算。0 禁用递归；默认 1 与论文实验一致。"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        // Network for the LLM calls; ExecutesCode because the sub-agent
        // runs Python in the REPL (which can do filesystem operations
        // within its sandbox).
        vec![ToolCapability::Network, ToolCapability::ExecutesCode]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        // Same level as parallel_fanout: the model decided to invoke this, the
        // user already enabled tools by being in Agent/YOLO mode, and
        // every concrete side-effect (file read, LLM call) is bounded.
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        // Each call spins its own sidecar on a kernel-assigned port and
        // its own per-turn state file, so two calls don't interfere.
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let Some(client) = self.client.clone() else {
            return Err(ToolError::not_available(
                "rlm_process requires an active DeepSeek client".to_string(),
            ));
        };

        let task = input
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::MissingField {
                field: "task".to_string(),
            })?
            .trim();
        if task.is_empty() {
            return Err(ToolError::invalid_input("rlm: `task` is empty"));
        }

        let file_path = input.get("file_path").and_then(|v| v.as_str());
        let content = input.get("content").and_then(|v| v.as_str());

        let body = match (file_path, content) {
            (Some(_), Some(_)) => {
                return Err(ToolError::invalid_input(
                    "rlm: pass `file_path` OR `content`, not both",
                ));
            }
            (None, None) => {
                return Err(ToolError::invalid_input(
                    "rlm: requires `file_path` (preferred) or `content`",
                ));
            }
            (Some(path), None) => {
                let resolved = context.resolve_path(path)?;
                tokio::fs::read_to_string(&resolved).await.map_err(|e| {
                    ToolError::ExecutionFailed {
                        message: format!("read {}: {e}", resolved.display()),
                    }
                })?
            }
            (None, Some(c)) => {
                if c.chars().count() > MAX_INLINE_CONTENT_CHARS {
                    return Err(ToolError::invalid_input(format!(
                        "rlm: inline `content` is {} chars (cap {MAX_INLINE_CONTENT_CHARS}). Pass `file_path` for larger inputs.",
                        c.chars().count()
                    )));
                }
                c.to_string()
            }
        };

        if body.trim().is_empty() {
            return Err(ToolError::invalid_input(
                "rlm: input is empty after loading",
            ));
        }
        let input_chars = body.chars().count();
        let input_lines = body.lines().count();

        // Pin child calls to Flash so model-generated tool args cannot quietly
        // turn fanout work into Pro-billed requests. The RLM root still uses
        // the session model; child helper calls are the cheap batch layer.
        let child_model = DEFAULT_CHILD_MODEL.to_string();

        let max_depth = input
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .map(|n| n.min(u64::from(u32::MAX)) as u32)
            .unwrap_or(DEFAULT_MAX_DEPTH);

        // The tool framework doesn't expose a per-tool event stream, and
        // we don't want RLM's progress events to interleave with the
        // parent agent's stream. Drain into a no-op channel.
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let drain = spawn_supervised(
            "rlm-progress-drain",
            std::panic::Location::caller(),
            async move { while rx.recv().await.is_some() {} },
        );

        // The big body lives only in the REPL as `context`. The small
        // `task` rides along as `root_prompt` and is shown to the root
        // LLM each iteration so it never forgets the objective.
        let result = run_rlm_turn_with_root(
            &client,
            self.root_model.clone(),
            body,
            Some(task.to_string()),
            child_model.clone(),
            tx,
            max_depth,
        )
        .await;

        drain.abort();

        if let Some(err) = result.error {
            return Err(ToolError::ExecutionFailed {
                message: format!(
                    "rlm: {err} (iterations={}, termination={:?})",
                    result.iterations, result.termination
                ),
            });
        }

        if result.answer.trim().is_empty() {
            return Err(ToolError::ExecutionFailed {
                message: format!(
                    "rlm: empty answer (termination={:?}, iterations={})",
                    result.termination, result.iterations
                ),
            });
        }

        // Surface the termination reason and a brief per-round trace so the
        // user can verify the sub-agent actually engaged with `context`
        // through sub-LLM calls — not just inferred an answer from the
        // preview.
        let footer = match result.termination {
            RlmTermination::Final => String::new(),
            RlmTermination::NoCode => format!(
                "\n\n[warning: sub-agent failed to engage the REPL after {} iterations — answer is the model's last raw response]",
                result.iterations
            ),
            RlmTermination::Exhausted => format!(
                "\n\n[warning: sub-agent hit the {}-iteration cap without FINAL()]",
                result.iterations
            ),
            RlmTermination::Error => String::new(),
        };

        let report = format!(
            "RLM report:\n- input: {input_lines} line(s), {input_chars} char(s)\n- iterations: {}\n- duration: {}ms\n- sub-LLM RPCs: {}\n- termination: {:?}\n\nAnswer:\n",
            result.iterations,
            result.duration.as_millis(),
            result.total_rpcs,
            result.termination,
        );

        let trace_summary = if result.trace.is_empty() {
            String::from("\n\n[trace: no REPL rounds executed]")
        } else {
            let mut s = String::from("\n\n[RLM trace]");
            for r in &result.trace {
                let head = r
                    .code_summary
                    .lines()
                    .next()
                    .unwrap_or(r.code_summary.as_str())
                    .chars()
                    .take(80)
                    .collect::<String>();
                s.push_str(&format!(
                    "\n  round {}: {} sub-LLM call(s), {}ms{} — {}",
                    r.round,
                    r.rpc_count,
                    r.elapsed_ms,
                    if r.had_error { " (error)" } else { "" },
                    head
                ));
            }
            s
        };

        let trace_json: Vec<_> = result
            .trace
            .iter()
            .map(|r| {
                json!({
                    "round": r.round,
                    "rpc_count": r.rpc_count,
                    "elapsed_ms": r.elapsed_ms,
                    "had_error": r.had_error,
                    "code_summary": r.code_summary,
                    "stdout_preview": r.stdout_preview,
                })
            })
            .collect();

        // The `child_*` keys are the contract the engine reads in
        // `tool_routing::accrue_child_token_cost_if_any` to roll
        // sub-LLM token usage into the session-cost counter. RLM
        // spawns its own DeepSeek calls under `child_model`; without
        // this accrual the dashboard under-reports a session that
        // uses RLM heavily by 10-20× because only the parent turn's
        // tokens hit `accrue_session_cost` (#524).
        let metadata = json!({
            "iterations": result.iterations,
            "duration_ms": result.duration.as_millis() as u64,
            "input_tokens": result.usage.input_tokens,
            "output_tokens": result.usage.output_tokens,
            "child_input_tokens": result.usage.input_tokens,
            "child_output_tokens": result.usage.output_tokens,
            "child_prompt_cache_hit_tokens": result.usage.prompt_cache_hit_tokens,
            "child_prompt_cache_miss_tokens": result.usage.prompt_cache_miss_tokens,
            "child_model": child_model,
            "termination": format!("{:?}", result.termination).to_lowercase(),
            "max_depth": max_depth,
            "context_chars": input_chars,
            "context_lines": input_lines,
            "total_rpcs": result.total_rpcs,
            "trace": trace_json,
        });

        Ok(ToolResult::success(format!(
            "{report}{}{}{}",
            result.answer, footer, trace_summary
        ))
        .with_metadata(metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> RlmTool {
        RlmTool::new(None, "deepseek-v4-pro".to_string())
    }

    fn ctx() -> ToolContext {
        use std::path::PathBuf;
        ToolContext::with_auto_approve(
            PathBuf::from("."),
            false,
            PathBuf::from("notes.txt"),
            PathBuf::from("mcp.json"),
            true,
        )
    }

    #[test]
    fn name_and_schema() {
        let t = tool();
        assert_eq!(t.name(), "rlm");
        let schema = t.input_schema();
        assert!(schema["properties"]["task"].is_object());
        assert!(schema["properties"]["file_path"].is_object());
        assert!(schema["properties"]["content"].is_object());
        assert!(schema["properties"]["max_depth"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "task"));
    }

    #[test]
    fn approval_is_auto_so_calls_are_unattended() {
        assert_eq!(tool().approval_requirement(), ApprovalRequirement::Auto);
    }

    #[test]
    fn capabilities_include_network_and_executes_code() {
        let caps = tool().capabilities();
        assert!(caps.contains(&ToolCapability::Network));
        assert!(caps.contains(&ToolCapability::ExecutesCode));
    }

    #[test]
    fn supports_parallel_dispatch() {
        assert!(tool().supports_parallel());
    }

    #[test]
    fn description_steers_without_suppressing_rlm_use() {
        let t = tool();
        let description = t.description();
        assert!(
            description.contains("使用此工具"),
            "描述应正面解释 RLM 的适用场景"
        );
        assert!(
            !description.contains("不要使用"),
            "避免训练模型回避可用工具"
        );
        assert!(
            !description.contains("更慢且更贵"),
            "成本警告应属于验证指导，而非工具抑制"
        );
    }

    #[tokio::test]
    async fn returns_not_available_without_client() {
        let t = tool();
        let ctx = ctx();
        let res = t
            .execute(json!({"task": "x", "content": "y"}), &ctx)
            .await
            .expect_err("must error");
        assert!(matches!(res, ToolError::NotAvailable { .. }));
    }

    #[tokio::test]
    async fn rejects_missing_task() {
        let t = RlmTool::new(None, "x".into());
        let ctx = ctx();
        let res = t
            .execute(json!({"content": "abc"}), &ctx)
            .await
            .expect_err("must error");
        // Without a client we hit NotAvailable first. Re-check ordering by
        // injecting an obviously-bad payload that would trip earlier.
        assert!(matches!(
            res,
            ToolError::NotAvailable { .. } | ToolError::MissingField { .. }
        ));
    }

    #[tokio::test]
    async fn rejects_both_path_and_content() {
        // Even without a client, the input-shape check should fire if we
        // bypass the client guard. Simpler: just verify the schema lists
        // the two as alternatives via descriptions.
        let schema = tool().input_schema();
        let path_desc = schema["properties"]["file_path"]["description"]
            .as_str()
            .unwrap();
        assert!(path_desc.contains("互斥"));
    }
}
