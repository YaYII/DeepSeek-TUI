//! 工具调度 — 每轮工具批次的计划/执行辅助函数。
//!
//! 从 `core/engine.rs` 提取（P1.3）。高层排序仍然
//! 在 `Engine::handle_deepseek_turn` 中；本模块拥有：
//!
//! * 流式缓冲区解析为最终化的 `serde_json::Value` 工具输入
//!   （`final_tool_input`、`parse_tool_input`、代码块/JSON 段辅助函数）。
//! * `multi_tool_use.parallel` 负载解析器。
//! * 轮次循环咨询的策略谓词 — 批次何时可以并行运行，
//!   何时 `update_plan` 步骤应停止轮次，何时 Plan
//!   提示应强制先计划跳转，以及可安全并行运行的少量只读
//!   MCP 工具。
//! * 批次驱动程序传递的工具执行计划/结果类型。
//!
//! 所有项仅为 `pub(super)`：公共引擎表面（Op/Event、
//! `EngineHandle`、`spawn_engine`）保留在 `core/engine.rs` 中。

use serde_json::json;

use crate::models::{Tool, ToolCaller};
use crate::tools::spec::{ToolError, ToolResult};
use crate::tui::app::AppMode;

use super::ToolUseState;

// === Types ============================================================

#[allow(dead_code)] // `index` 镜像批次顺序以便诊断。
pub(super) struct ToolExecOutcome {
    pub(super) index: usize,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) input: serde_json::Value,
    pub(super) started_at: std::time::Instant,
    pub(super) result: Result<ToolResult, ToolError>,
}

#[derive(Debug, Clone)]
pub(super) struct ToolExecutionPlan {
    pub(super) index: usize,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) input: serde_json::Value,
    pub(super) caller: Option<ToolCaller>,
    pub(super) interactive: bool,
    pub(super) approval_required: bool,
    pub(super) approval_description: String,
    pub(super) supports_parallel: bool,
    pub(super) read_only: bool,
    pub(super) blocked_error: Option<ToolError>,
    pub(super) guard_result: Option<ToolResult>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct ParallelToolResultEntry {
    pub(super) tool_name: String,
    pub(super) success: bool,
    pub(super) content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct ParallelToolResult {
    pub(super) results: Vec<ParallelToolResultEntry>,
}

// 在工具执行期间持有锁守卫。
// 内部守卫为 RAII 目的持有（守卫丢弃时释放）。
pub(super) enum ToolExecGuard<'a> {
    Read(#[allow(dead_code)] tokio::sync::RwLockReadGuard<'a, ()>),
    Write(#[allow(dead_code)] tokio::sync::RwLockWriteGuard<'a, ()>),
}

// === 调用者策略和错误 ========================================

pub(super) fn caller_type_for_tool_use(caller: Option<&ToolCaller>) -> &str {
    caller.map_or("direct", |c| c.caller_type.as_str())
}

pub(super) fn caller_allowed_for_tool(
    caller: Option<&ToolCaller>,
    tool_def: Option<&Tool>,
) -> bool {
    let requested = caller_type_for_tool_use(caller);
    if let Some(def) = tool_def
        && let Some(allowed) = &def.allowed_callers
    {
        if allowed.is_empty() {
            return requested == "direct";
        }
        return allowed.iter().any(|item| item == requested);
    }
    requested == "direct"
}

pub(super) fn format_tool_error(err: &ToolError, tool_name: &str) -> String {
    match err {
        ToolError::InvalidInput { message } => {
            format!("Invalid input for tool '{tool_name}': {message}")
        }
        ToolError::MissingField { field } => {
            format!("Tool '{tool_name}' is missing required field '{field}'")
        }
        ToolError::PathEscape { path } => format!(
            "Path escapes workspace: {}. Use a workspace-relative path or enable trust mode.",
            path.display()
        ),
        ToolError::ExecutionFailed { message } => message.clone(),
        ToolError::Timeout { seconds } => format!(
            "Tool '{tool_name}' timed out after {seconds}s. Try a narrower scope or a longer timeout."
        ),
        ToolError::NotAvailable { message } => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("current tool catalog") || lower.contains("did you mean:") {
                message.clone()
            } else {
                format!(
                    "Tool '{tool_name}' is not available: {message}. Check mode, feature flags, or tool name."
                )
            }
        }
        ToolError::PermissionDenied { message } => format!(
            "Tool '{tool_name}' was denied: {message}. Adjust approval mode or request permission."
        ),
    }
}

// === 流式缓冲区解析 =========================================

/// 将流式 `ToolUseState` 提升为最终化的 JSON 输入。
///
/// 优先级顺序：
///
///   1. `input_buffer`（原始流式增量拼接）— 解析为
///      JSON。这是最权威的，因为它是模型实际发出的内容。
///   2. `input`（每个增量的尽力解析镜像）— 当
///      缓冲区为空时使用（预流式工具调用走此路径）。
///   3. `input_buffer` 非空但无法解析 → 回退到 `input`
///      （每个增量的解析器已将最近有效的部分解析
///      镜像到 `tool_state.input` 中）。
pub(super) fn final_tool_input(state: &ToolUseState) -> serde_json::Value {
    if !state.input_buffer.trim().is_empty()
        && let Some(parsed) = parse_tool_input(&state.input_buffer)
    {
        return parsed;
    }
    state.input.clone()
}

pub(super) fn parse_tool_input(buffer: &str) -> Option<serde_json::Value> {
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        return None;
    }
    // 首先尝试确定性的参数修复阶梯（处理尾随逗号、
    // 未闭合的花括号、嵌入的控制字符等）
    if let Ok(value) = crate::tools::arg_repair::repair(trimmed) {
        return Some(value);
    }
    // 回退到修复阶梯未覆盖的代码块、双重编码和
    // 段提取模式的现有策略。
    if let Some(stripped) = strip_code_fences(trimmed)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&stripped)
    {
        return Some(value);
    }
    if let Ok(serde_json::Value::String(inner)) = serde_json::from_str::<serde_json::Value>(trimmed)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&inner)
    {
        return Some(value);
    }
    extract_json_segment(trimmed)
        .and_then(|segment| serde_json::from_str::<serde_json::Value>(&segment).ok())
}

fn strip_code_fences(text: &str) -> Option<String> {
    if !text.contains("```") {
        return None;
    }
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        lines.push(line);
    }
    let stripped = lines.join("\n");
    let stripped = stripped.trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

fn extract_json_segment(text: &str) -> Option<String> {
    extract_balanced_segment(text, '{', '}').or_else(|| extract_balanced_segment(text, '[', ']'))
}

fn extract_balanced_segment(text: &str, open: char, close: char) -> Option<String> {
    let start = text.find(open)?;
    let mut depth = 0i32;
    let mut end = None;
    for (offset, ch) in text[start..].char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                end = Some(start + offset + ch.len_utf8());
                break;
            }
        }
    }
    end.map(|end_idx| text[start..end_idx].to_string())
}

fn normalize_parallel_tool_name(raw: &str) -> String {
    let mut name = raw.trim();
    for prefix in ["functions.", "tools.", "tool."] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            name = stripped;
            break;
        }
    }
    name.to_string()
}

pub(super) fn parse_parallel_tool_calls(
    input: &serde_json::Value,
) -> Result<Vec<(String, serde_json::Value)>, ToolError> {
    let tool_uses = input
        .get("tool_uses")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ToolError::missing_field("tool_uses"))?;
    if tool_uses.is_empty() {
        return Err(ToolError::invalid_input(
            "multi_tool_use.parallel requires at least one tool call",
        ));
    }

    let mut calls = Vec::with_capacity(tool_uses.len());
    for item in tool_uses {
        let name = item
            .get("recipient_name")
            .or_else(|| item.get("tool_name"))
            .or_else(|| item.get("name"))
            .or_else(|| item.get("tool"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::missing_field("recipient_name"))?;
        let params = item
            .get("parameters")
            .or_else(|| item.get("input"))
            .or_else(|| item.get("args"))
            .or_else(|| item.get("arguments"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        calls.push((normalize_parallel_tool_name(name), params));
    }

    Ok(calls)
}

// === Dispatch policy ==================================================

pub(super) fn should_parallelize_tool_batch(plans: &[ToolExecutionPlan]) -> bool {
    !plans.is_empty()
        && plans.iter().all(|plan| {
            plan.read_only && plan.supports_parallel && !plan.approval_required && !plan.interactive
        })
}

pub(super) fn should_stop_after_plan_tool(
    mode: AppMode,
    tool_name: &str,
    result: &Result<ToolResult, ToolError>,
) -> bool {
    mode == AppMode::Plan && tool_name == "update_plan" && result.is_ok()
}

pub(super) fn should_force_update_plan_first(mode: AppMode, content: &str) -> bool {
    if mode != AppMode::Plan {
        return false;
    }

    let lower = content.to_ascii_lowercase();
    let asks_for_direct_plan = [
        "quick plan",
        "short plan",
        "simple plan",
        "3-step plan",
        "3 step plan",
        "three-step plan",
        "three step plan",
        "high-level plan",
        "high level plan",
        "give me a plan",
        "make a plan",
        "outline a plan",
        "draft a plan",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    if !asks_for_direct_plan {
        return false;
    }

    let asks_for_repo_exploration = [
        "inspect the repo",
        "inspect the code",
        "explore the repo",
        "search the repo",
        "read the code",
        "review the code",
        "analyze the code",
        "investigate",
        "look through",
        "understand the current",
        "ground it in the codebase",
        "based on the codebase",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    !asks_for_repo_exploration
}

pub(super) fn mcp_tool_is_parallel_safe(name: &str) -> bool {
    matches!(
        name,
        "list_mcp_resources"
            | "list_mcp_resource_templates"
            | "mcp_read_resource"
            | "read_mcp_resource"
            | "mcp_get_prompt"
    )
}

pub(super) fn mcp_tool_is_read_only(name: &str) -> bool {
    matches!(
        name,
        "list_mcp_resources"
            | "list_mcp_resource_templates"
            | "mcp_read_resource"
            | "read_mcp_resource"
            | "mcp_get_prompt"
    )
}

pub(super) fn mcp_tool_approval_description(name: &str) -> String {
    if mcp_tool_is_read_only(name) {
        format!("Read-only MCP tool '{name}'")
    } else {
        format!("MCP tool '{name}' may have side effects")
    }
}
