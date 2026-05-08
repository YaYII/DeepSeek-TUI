//! 流式响应状态和护栏。
//!
//! 本模块拥有解码一个模型流时使用的本地状态：
//! 内容块类型追踪、流式工具使用缓冲区、透明重试策略，
//! 以及看起来像伪造工具调用包装器的文本清理器。

use crate::models::ToolCaller;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ContentBlockKind {
    Text,
    Thinking,
    ToolUse,
}

#[derive(Debug, Clone)]
pub(super) struct ToolUseState {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) input: serde_json::Value,
    pub(super) caller: Option<ToolCaller>,
    pub(super) input_buffer: String,
}

/// 等待单个流块的最长时间，超过则假定停滞。
/// **这是空闲超时** — 它在每个 SSE 块上重置，因此确实在产生
/// reasoning_content 的长思考轮次保持存活。只有真正的
/// `chunk_timeout` 静默窗口会终止流。
const DEFAULT_STREAM_CHUNK_TIMEOUT_SECS: u64 = 300;
const MIN_STREAM_CHUNK_TIMEOUT_SECS: u64 = 1;
const MAX_STREAM_CHUNK_TIMEOUT_SECS: u64 = 3600;
const STREAM_IDLE_TIMEOUT_ENV: &str = "DEEPSEEK_STREAM_IDLE_TIMEOUT_SECS";

/// 读取 SSE 客户端使用的共享流空闲超时覆盖。
pub(super) fn stream_chunk_timeout_secs() -> u64 {
    stream_chunk_timeout_secs_from_env(std::env::var(STREAM_IDLE_TIMEOUT_ENV).ok().as_deref())
}

fn stream_chunk_timeout_secs_from_env(value: Option<&str>) -> u64 {
    value
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_STREAM_CHUNK_TIMEOUT_SECS)
        .clamp(MIN_STREAM_CHUNK_TIMEOUT_SECS, MAX_STREAM_CHUNK_TIMEOUT_SECS)
}
/// 在终止流之前文本/思考内容的最大总字节数。
pub(super) const STREAM_MAX_CONTENT_BYTES: usize = 10 * 1024 * 1024; // 10 MB
/// 总流挂钟持续时间的健全性后盾。**不是**常规的终止开关 —
/// 流块空闲超时是主要的停滞检测器。挂钟上限仅用于限制
/// 病态情况（例如服务器永远发送心跳而没有进展）。
///
/// 历史：此值曾为 300 秒（5 分钟），过于激进 — V4
/// 思考轮次在困难提示上合法地超过 5 分钟挂钟时间，
/// 同时全程发出 reasoning_content 块。在 v0.6.6 中提升到
/// 30 分钟以解决 `TODO_FIXES.md` #1。Codex 默认使用每块空闲
/// 300 秒且无挂钟上限；我们保留两层但给挂钟一个宽松的窗口，
/// 使其在实践中永远不会触发。
pub(super) const STREAM_MAX_DURATION_SECS: u64 = 1800; // 30 分钟（曾为 300 秒；#103/#1）
/// 在我们将轮次失败呈现给用户之前，连续可恢复流错误的硬上限。
/// 在 v0.6.7 中从 3 提升到 5，同时引入 HTTP/2 keepalive 默认值
///（#103）— keepalive 应使虚假解码错误更少，因此我们可以容忍
/// 更长的连续失败序列再放弃轮次。
pub(super) const MAX_STREAM_ERRORS_BEFORE_FAIL: u32 = 5;
/// 透明流级重试的上限 — 这些仅在连接在流式传输任何内容之前
/// 中断时发生，因此 DeepSeek 尚未向我们收费且用户尚未看到任何内容。
/// 两次尝试足以度过不稳定的边缘节点，而不会放大真正的故障（#103）。
pub(super) const MAX_TRANSPARENT_STREAM_RETRIES: u32 = 2;

/// 决定流错误是否有资格进行透明重试。
///
/// 仅当所有三个条件都满足时为 true：
/// 1. 当前尝试未收到任何内容 — 否则 DeepSeek
///    已为输出令牌向我们收费且用户已看到部分增量；
///    重新发送将双重计费并使 UI 不同步。
/// 2. 我们仍有透明重试预算剩余。
/// 3. 轮次尚未被取消。
///
/// 提取为纯函数，以便四个 #103 重试情况可以在单元测试中
/// 练习，而无需启动完整的引擎状态机。
pub(super) fn should_transparently_retry_stream(
    any_content_received: bool,
    transparent_attempts: u32,
    cancelled: bool,
) -> bool {
    !any_content_received && transparent_attempts < MAX_TRANSPARENT_STREAM_RETRIES && !cancelled
}

pub(crate) const TOOL_CALL_START_MARKERS: [&str; 5] = [
    "[TOOL_CALL]",
    "<deepseek:tool_call",
    "<tool_call",
    "<invoke ",
    "<function_calls>",
];

pub(crate) const TOOL_CALL_END_MARKERS: [&str; 5] = [
    "[/TOOL_CALL]",
    "</deepseek:tool_call>",
    "</tool_call>",
    "</invoke>",
    "</function_calls>",
];

/// 当模型尝试在纯文本中伪造工具调用包装器而不是使用 API 工具通道时
/// 发出的一次性通知。可见内容仍然被清理；此通知存在是为了让用户
/// 看到为什么他们的文本缩小了。
pub(crate) const FAKE_WRAPPER_NOTICE: &str =
    "从模型输出中剥离了非 API 工具调用包装器（请使用 API 工具通道）";

/// 如果 `text` 包含任何已知的伪造包装器开始标记，则为 true。
/// 由流式循环用于决定是否发出 `FAKE_WRAPPER_NOTICE`。
pub(crate) fn contains_fake_tool_wrapper(text: &str) -> bool {
    TOOL_CALL_START_MARKERS.iter().any(|m| text.contains(m))
}

fn find_first_marker(text: &str, markers: &[&str]) -> Option<(usize, usize)> {
    markers
        .iter()
        .filter_map(|marker| text.find(marker).map(|idx| (idx, marker.len())))
        .min_by_key(|(idx, _)| *idx)
}

pub(crate) fn filter_tool_call_delta(delta: &str, in_tool_call: &mut bool) -> String {
    if delta.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    let mut rest = delta;

    loop {
        if *in_tool_call {
            let Some((idx, len)) = find_first_marker(rest, &TOOL_CALL_END_MARKERS) else {
                break;
            };
            rest = &rest[idx + len..];
            *in_tool_call = false;
        } else {
            let Some((idx, len)) = find_first_marker(rest, &TOOL_CALL_START_MARKERS) else {
                output.push_str(rest);
                break;
            };
            output.push_str(&rest[..idx]);
            rest = &rest[idx + len..];
            *in_tool_call = true;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_chunk_timeout_defaults_and_clamps_env_values() {
        assert_eq!(stream_chunk_timeout_secs_from_env(None), 300);
        assert_eq!(
            stream_chunk_timeout_secs_from_env(Some("not-a-number")),
            300
        );
        assert_eq!(stream_chunk_timeout_secs_from_env(Some("0")), 1);
        assert_eq!(stream_chunk_timeout_secs_from_env(Some("90")), 90);
        assert_eq!(stream_chunk_timeout_secs_from_env(Some("99999")), 3600);
    }
}
