//! 引擎的上下文预算和提示塑形辅助函数。
//!
//! 这些函数由流式轮次循环、容量流和引擎会话维护代码共享。
//! 将它们保留在此处可防止顶层引擎模块积累不相关的上下文策略细节。

use crate::compaction::estimate_tokens;
use crate::error_taxonomy::ErrorCategory;
use crate::models::{Message, SystemPrompt, context_window_for_model};
use crate::tools::spec::ToolResult;

/// 为普通代理轮次请求的最大输出令牌数。故意设置得很大：
/// V4 思考模型在困难提示上可以在可见回复之前产生数万个推理令牌，
/// 而 DeepSeek V4 提供 1M 上下文窗口。v0.7.5 将此上限保持固定，
/// 而不是在接近压力时静默降低 `max_tokens`；
/// 硬循环/预检检查在发送下一个请求之前保留此预算加上安全余量。
pub(super) const TURN_MAX_OUTPUT_TOKENS: u32 = 262_144;

/// API 请求中发送的安全最大输出令牌数。此值必须足够低，
/// 以兼容上下文限制小于模型原生窗口的提供商
/// （例如，使用 `--max-model-len 131072` 的自托管 vLLM/SGLang）。
/// DeepSeek 的 API 仍会根据需要生成尽可能多的思考令牌；
/// 此上限仅防止上下文限制较紧的提供商返回 HTTP 400。
const API_MAX_OUTPUT_TOKENS: u32 = 65_536;

/// 计算给定模型在 API 请求中发送的有效 `max_tokens`。
/// 使用 `API_MAX_OUTPUT_TOKENS`（64K），适用于大多数提供商限制（128K+ 总量）。
/// 对于上下文窗口较小的非 V4 模型，上限为上下文窗口的一半。
pub(super) fn effective_max_output_tokens(model: &str) -> u32 {
    let window = context_window_for_model(model).unwrap_or(128_000);
    if window >= 500_000 {
        // 大上下文提供商上的 V4 级模型：使用 64K，
        // 对大多数部署安全，同时仍允许大量输出。
        API_MAX_OUTPUT_TOKENS
    } else {
        // 较小模型：上限为上下文窗口的一半（留出输入空间）
        let capped = window / 2;
        capped.min(API_MAX_OUTPUT_TOKENS)
    }
}
/// 在需要紧急修剪时保留最近的消息数。
pub(super) const MIN_RECENT_MESSAGES_TO_KEEP: usize = 4;
/// 在轮次失败前允许的紧急恢复尝试次数。
pub(super) const MAX_CONTEXT_RECOVERY_ATTEMPTS: u8 = 2;
/// 保留额外余量以避免触及提供商硬限制。
const CONTEXT_HEADROOM_TOKENS: usize = 1024;
/// 插入模型上下文的任何工具输出的硬上限。
const TOOL_RESULT_CONTEXT_HARD_LIMIT_CHARS: usize = 12_000;
/// 插入模型上下文的已知嘈杂工具的软上限。
const TOOL_RESULT_CONTEXT_SOFT_LIMIT_CHARS: usize = 2_000;
/// 压缩工具输出用于模型上下文时保留的片段长度。
const TOOL_RESULT_CONTEXT_SNIPPET_CHARS: usize = 900;
/// 插入大上下文模型的工具输出的硬上限。
const LARGE_CONTEXT_TOOL_RESULT_HARD_LIMIT_CHARS: usize = 180_000;
/// 插入大上下文模型的已知嘈杂工具的软上限。
const LARGE_CONTEXT_TOOL_RESULT_SOFT_LIMIT_CHARS: usize = 60_000;
/// 压缩大上下文工具输出时保留的片段长度。
const LARGE_CONTEXT_TOOL_RESULT_SNIPPET_CHARS: usize = 40_000;
/// 可以放宽工具输出限制的上下文窗口大小。
const LARGE_CONTEXT_WINDOW_TOKENS: u32 = 500_000;
/// 从元数据提供的输出摘要中保留的最大字符数。
const TOOL_RESULT_METADATA_SUMMARY_CHARS: usize = 320;

pub(super) const COMPACTION_SUMMARY_MARKER: &str = "对话摘要（自动生成）";

#[derive(Debug, Clone, Copy)]
struct ToolResultContextLimits {
    hard_limit_chars: usize,
    noisy_soft_limit_chars: usize,
    snippet_chars: usize,
}

pub(super) fn summarize_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let take = limit.saturating_sub(3);
    let mut out: String = text.chars().take(take).collect();
    out.push_str("...");
    out
}

fn summarize_text_head_tail(text: &str, limit: usize) -> String {
    let total = text.chars().count();
    if total <= limit {
        return text.to_string();
    }
    if limit <= 20 {
        return summarize_text(text, limit);
    }

    let marker = "\n\n[... output truncated for context ...]\n\n";
    let marker_len = marker.chars().count();
    if limit <= marker_len + 20 {
        return summarize_text(text, limit);
    }

    let remaining = limit - marker_len;
    let head_len = remaining.saturating_mul(2) / 3;
    let tail_len = remaining.saturating_sub(head_len);
    let head: String = text.chars().take(head_len).collect();
    let tail_vec: Vec<char> = text.chars().rev().take(tail_len).collect();
    let tail: String = tail_vec.into_iter().rev().collect();
    format!("{head}{marker}{tail}")
}

fn tool_result_is_noisy(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "exec_shell"
            | "exec_shell_wait"
            | "exec_shell_interact"
            | "multi_tool_use.parallel"
            | "web_search"
    )
}

fn tool_result_metadata_summary(metadata: Option<&serde_json::Value>) -> Option<String> {
    let obj = metadata?.as_object()?;
    for key in ["summary", "stdout_summary", "stderr_summary", "message"] {
        if let Some(text) = obj.get(key).and_then(serde_json::Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(summarize_text(trimmed, TOOL_RESULT_METADATA_SUMMARY_CHARS));
            }
        }
    }
    None
}

fn summarize_subagent_status(status: &serde_json::Value) -> String {
    if let Some(raw) = status.as_str() {
        return raw.to_string();
    }
    if let Some(obj) = status.as_object()
        && let Some((kind, value)) = obj.iter().next()
    {
        if let Some(reason) = value.as_str().filter(|s| !s.trim().is_empty()) {
            return format!("{kind}({})", summarize_text(reason.trim(), 120));
        }
        return kind.to_string();
    }
    status.to_string()
}

fn summarize_subagent_snapshot(snapshot: &serde_json::Value, index: usize) -> String {
    let Some(obj) = snapshot.as_object() else {
        return format!(
            "- item {index}: {}",
            summarize_text(&snapshot.to_string(), 240)
        );
    };

    let agent_id = obj
        .get("agent_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let agent_type = obj
        .get("agent_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("agent");
    let status = obj
        .get("status")
        .map(summarize_subagent_status)
        .unwrap_or_else(|| "unknown".to_string());
    let objective = obj
        .get("assignment")
        .and_then(|assignment| assignment.get("objective"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| summarize_text(s, 220));
    let result = obj
        .get("result")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| summarize_text(s, 1_600));
    let steps = obj.get("steps_taken").and_then(serde_json::Value::as_u64);
    let duration_ms = obj.get("duration_ms").and_then(serde_json::Value::as_u64);

    let mut lines = vec![format!("- {agent_id} ({agent_type}) status={status}")];
    if let Some(objective) = objective {
        lines.push(format!("  objective: {objective}"));
    }
    match result {
        Some(result) => lines.push(format!("  result: {result}")),
        None => lines.push("  result: not available yet".to_string()),
    }
    if steps.is_some() || duration_ms.is_some() {
        let steps = steps
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        let duration_ms = duration_ms
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        lines.push(format!("  stats: steps={steps}, duration_ms={duration_ms}"));
    }
    lines.join("\n")
}

fn compact_subagent_tool_result_for_context(tool_name: &str, raw: &str) -> Option<String> {
    if !matches!(tool_name, "agent_result" | "agent_wait" | "wait") {
        return None;
    }

    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let snapshots: Vec<&serde_json::Value> = match &parsed {
        serde_json::Value::Array(items) => items.iter().collect(),
        serde_json::Value::Object(_) => vec![&parsed],
        _ => return None,
    };

    let mut out = String::from("[子代理结果已汇总到父上下文]\n");
    out.push_str("仅在需要完整原始负载时再次使用 `agent_result`。\n");
    for (idx, snapshot) in snapshots.iter().enumerate() {
        if idx >= 8 {
            out.push_str(&format!(
                "- ... 还有 {} 个子代理结果因上下文摘要限制被省略\n",
                snapshots.len().saturating_sub(idx)
            ));
            break;
        }
        out.push_str(&summarize_subagent_snapshot(snapshot, idx + 1));
        out.push('\n');
    }
    Some(out.trim_end().to_string())
}

fn tool_result_context_limits_for_model(model: &str) -> ToolResultContextLimits {
    let is_large_context =
        context_window_for_model(model).is_some_and(|window| window >= LARGE_CONTEXT_WINDOW_TOKENS);

    if is_large_context {
        ToolResultContextLimits {
            hard_limit_chars: LARGE_CONTEXT_TOOL_RESULT_HARD_LIMIT_CHARS,
            noisy_soft_limit_chars: LARGE_CONTEXT_TOOL_RESULT_SOFT_LIMIT_CHARS,
            snippet_chars: LARGE_CONTEXT_TOOL_RESULT_SNIPPET_CHARS,
        }
    } else {
        ToolResultContextLimits {
            hard_limit_chars: TOOL_RESULT_CONTEXT_HARD_LIMIT_CHARS,
            noisy_soft_limit_chars: TOOL_RESULT_CONTEXT_SOFT_LIMIT_CHARS,
            snippet_chars: TOOL_RESULT_CONTEXT_SNIPPET_CHARS,
        }
    }
}

pub(crate) fn compact_tool_result_for_context(
    model: &str,
    tool_name: &str,
    output: &ToolResult,
) -> String {
    let raw = output.content.trim();
    if raw.is_empty() {
        return String::new();
    }

    if let Some(summary) = compact_subagent_tool_result_for_context(tool_name, raw) {
        return summary;
    }

    let limits = tool_result_context_limits_for_model(model);
    let raw_chars = raw.chars().count();
    let should_compact = raw_chars > limits.hard_limit_chars
        || (tool_result_is_noisy(tool_name) && raw_chars > limits.noisy_soft_limit_chars);
    if !should_compact {
        return raw.to_string();
    }

    let snippet = summarize_text_head_tail(raw, limits.snippet_chars);
    let omitted = raw_chars.saturating_sub(snippet.chars().count());
    let summary = tool_result_metadata_summary(output.metadata.as_ref());

    if let Some(summary) = summary {
        format!(
            "[{tool_name} output compacted to protect context]\nSummary: {summary}\nSnippet: {snippet}\n(Original: {raw_chars} chars, omitted: {omitted} chars.)"
        )
    } else {
        format!(
            "[{tool_name} output compacted to protect context]\nSnippet: {snippet}\n(Original: {raw_chars} chars, omitted: {omitted} chars.)"
        )
    }
}

pub(super) fn extract_compaction_summary_prompt(
    prompt: Option<SystemPrompt>,
) -> Option<SystemPrompt> {
    match prompt {
        Some(SystemPrompt::Blocks(blocks)) => {
            let summary_blocks: Vec<_> = blocks
                .into_iter()
                .filter(|block| block.text.contains(COMPACTION_SUMMARY_MARKER))
                .collect();
            if summary_blocks.is_empty() {
                None
            } else {
                Some(SystemPrompt::Blocks(summary_blocks))
            }
        }
        Some(SystemPrompt::Text(text)) => {
            if text.contains(COMPACTION_SUMMARY_MARKER) {
                Some(SystemPrompt::Text(text))
            } else {
                None
            }
        }
        None => None,
    }
}

fn estimate_text_tokens_conservative(text: &str) -> usize {
    text.chars().count().div_ceil(3)
}

fn estimate_system_tokens_conservative(system: Option<&SystemPrompt>) -> usize {
    match system {
        Some(SystemPrompt::Text(text)) => estimate_text_tokens_conservative(text),
        Some(SystemPrompt::Blocks(blocks)) => blocks
            .iter()
            .map(|block| estimate_text_tokens_conservative(&block.text))
            .sum(),
        None => 0,
    }
}

pub(super) fn estimate_input_tokens_conservative(
    messages: &[Message],
    system: Option<&SystemPrompt>,
) -> usize {
    let message_tokens = estimate_tokens(messages).saturating_mul(3).div_ceil(2);
    let system_tokens = estimate_system_tokens_conservative(system);
    let framing_overhead = messages.len().saturating_mul(12).saturating_add(48);
    message_tokens
        .saturating_add(system_tokens)
        .saturating_add(framing_overhead)
}

pub(super) fn context_input_budget(model: &str, requested_output_tokens: u32) -> Option<usize> {
    let window = usize::try_from(context_window_for_model(model)?).ok()?;
    let output = usize::try_from(requested_output_tokens).ok()?;
    window
        .checked_sub(output)
        .and_then(|v| v.checked_sub(CONTEXT_HEADROOM_TOKENS))
}

pub(super) fn turn_response_headroom_tokens() -> u64 {
    u64::from(TURN_MAX_OUTPUT_TOKENS).saturating_add(CONTEXT_HEADROOM_TOKENS as u64)
}

pub(super) fn is_context_length_error_message(message: &str) -> bool {
    crate::error_taxonomy::classify_error_message(message) == ErrorCategory::InvalidInput
}
