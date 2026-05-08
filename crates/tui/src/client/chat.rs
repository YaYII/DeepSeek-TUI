//! 面向 DeepSeek OpenAI 兼容端点的 Chat Completions API 辅助工具。
//!
//! 这是生产代码路径。流式处理（`create_message_stream`）、
//! 请求构建（`build_chat_messages*`）和 SSE 解析（`parse_sse_chunk`）
//! 都包含在此处。

use std::collections::HashSet;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tokio::time::timeout as tokio_timeout;

/// SSE 流读取的默认空闲超时时间（300 秒 = 5 分钟）。
/// 在此时间内没有收到数据，则认为流已停滞
/// 并产生可恢复的错误，供调用者重试。
const DEFAULT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// 初始流式响应头的默认超时时间。
///
/// `doctor` 使用有界的非流式请求，但正常的 TUI 轮次首先等待
/// SSE 响应打开。在某些 Windows/代理路径上，这个等待可能会在任何流数据块
/// 存在之前挂起，导致 UI 卡在"工作中..."状态。
const DEFAULT_STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(45);

/// 读取 `DEEPSEEK_STREAM_OPEN_TIMEOUT_SECS` 作为响应头等待的有界覆盖值。
/// 这有意比每个数据块的空闲超时更短，因为它只覆盖连接建立
/// 和上游头返回，不覆盖流开始后的模型思考时间。
fn stream_open_timeout() -> Duration {
    stream_open_timeout_from_env(
        std::env::var("DEEPSEEK_STREAM_OPEN_TIMEOUT_SECS")
            .ok()
            .as_deref(),
    )
}

fn stream_open_timeout_from_env(value: Option<&str>) -> Duration {
    let secs = value
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_STREAM_OPEN_TIMEOUT.as_secs())
        .clamp(5, 300);
    Duration::from_secs(secs)
}

/// 读取 `DEEPSEEK_STREAM_IDLE_TIMEOUT_SECS` 环境变量，若未设置则
/// 回退到默认的 300 秒。解析后的值被限制在 [1, 3600] 秒范围内。
fn stream_idle_timeout() -> Duration {
    let secs = std::env::var("DEEPSEEK_STREAM_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_STREAM_IDLE_TIMEOUT.as_secs())
        .clamp(1, 3600);
    Duration::from_secs(secs)
}

use crate::llm_client::StreamEventBox;
use crate::logging;
use crate::models::{
    ContentBlock, ContentBlockStart, Delta, Message, MessageDelta, MessageRequest, MessageResponse,
    StreamEvent, SystemPrompt, Tool, ToolCaller, Usage,
};

use super::{
    DeepSeekClient, ERROR_BODY_MAX_BYTES, SSE_BACKPRESSURE_HIGH_WATERMARK,
    SSE_BACKPRESSURE_SLEEP_MS, SSE_MAX_LINES_PER_CHUNK, acquire_stream_buffer, api_url,
    apply_reasoning_effort, bounded_error_text, from_api_tool_name, parse_usage,
    release_stream_buffer, system_to_instructions, to_api_tool_name,
};

impl DeepSeekClient {
    pub(super) async fn create_message_chat(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse> {
        let messages = build_chat_messages_for_request(request);
        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
        });

        if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = json!(top_p);
        }
        if let Some(tools) = request.tools.as_ref() {
            body["tools"] = json!(
                tools
                    .iter()
                    .map(|tool| tool_to_chat_for_base_url(tool, &self.base_url))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(choice) = request.tool_choice.as_ref()
            && let Some(mapped) = map_tool_choice_for_chat(choice)
        {
            body["tool_choice"] = mapped;
        }
        apply_reasoning_effort(
            &mut body,
            request.reasoning_effort.as_deref(),
            self.api_provider,
        );

        let url = api_url(&self.base_url, "chat/completions");
        let open_timeout = stream_open_timeout();
        let response = match tokio_timeout(
            open_timeout,
            self.send_with_retry(|| self.http_client.post(&url).json(&body)),
        )
        .await
        {
            Ok(result) => result?,
            Err(_elapsed) => {
                anyhow::bail!(
                    "SSE stream request did not receive response headers after {}s. \
                     `deepseek doctor` can still pass when non-streaming requests work; \
                     on Windows or proxy networks, try `DEEPSEEK_FORCE_HTTP1=1` and rerun `deepseek`.",
                    open_timeout.as_secs()
                );
            }
        };

        let status = response.status();
        if !status.is_success() {
            let error_text = bounded_error_text(response, ERROR_BODY_MAX_BYTES).await;
            anyhow::bail!("Failed to call DeepSeek Chat API: HTTP {status}: {error_text}");
        }

        let response_text = response.text().await.unwrap_or_default();
        let value: Value =
            serde_json::from_str(&response_text).context("Failed to parse Chat API JSON")?;
        parse_chat_message(&value)
    }
}

impl DeepSeekClient {
    pub(super) async fn handle_chat_completion_stream(
        &self,
        request: MessageRequest,
    ) -> Result<StreamEventBox> {
        // 尝试通过聊天补全实现真正的 SSE 流式传输（广泛支持）
        let messages = build_chat_messages_for_request(&request);
        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "stream": true,
            "stream_options": {
                "include_usage": true
            },
        });

        if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = json!(top_p);
        }
        if let Some(tools) = request.tools.as_ref() {
            body["tools"] = json!(
                tools
                    .iter()
                    .map(|tool| tool_to_chat_for_base_url(tool, &self.base_url))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(choice) = request.tool_choice.as_ref()
            && let Some(mapped) = map_tool_choice_for_chat(choice)
        {
            body["tool_choice"] = mapped;
        }
        apply_reasoning_effort(
            &mut body,
            request.reasoning_effort.as_deref(),
            self.api_provider,
        );

        // 最终防弹清理器：遍历线路负载，为任何有 tool_calls
        // 但没有 reasoning_content 的助手消息强制添加 `reasoning_content`。
        // DeepSeek 的思考模式 API 会拒绝此类消息并返回 400。
        // 这是在引擎端和构建端替换之后的最后一道防线；
        // 如果任一上游路径遗漏了某种情况（例如从磁盘恢复的会话、
        // 子代理直接添加消息，或缓存前缀不匹配），此遍仍能产生有效请求。
        let replay_input_tokens = sanitize_thinking_mode_messages(
            &mut body,
            &request.model,
            request.reasoning_effort.as_deref(),
        );

        let url = api_url(&self.base_url, "chat/completions");
        let response = self
            .send_with_retry(|| self.http_client.post(&url).json(&body))
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = bounded_error_text(response, ERROR_BODY_MAX_BYTES).await;
            // 如果 DeepSeek 尽管经过清理器仍因缺少 reasoning_content 而拒绝，
            // 则转储违规索引，以便在下一次失败时诊断其来源。
            if error_text.contains("reasoning_content") {
                log_thinking_mode_violations(&body);
            }
            anyhow::bail!("SSE stream request failed: HTTP {status}: {error_text}");
        }

        let model = request.model.clone();

        // 在将 `response` 消费为 `bytes_stream()` 之前捕获传输层头部。
        // 它们会在解码错误日志路径中暴露，以便我们在调查 #103 时
        // 区分 HTTP/2 RST_STREAM、分块编码损坏和 gzip 压缩器故障。
        let response_headers = format_stream_headers(response.headers());
        let byte_stream = response.bytes_stream();

        let stream = async_stream::stream! {
            use futures_util::StreamExt;

            // 发送一个合成的 MessageStart
            yield Ok(StreamEvent::MessageStart {
                message: MessageResponse {
                    id: String::new(),
                    r#type: "message".to_string(),
                    role: "assistant".to_string(),
                    content: Vec::new(),
                    model: model.clone(),
                    stop_reason: None,
                    stop_sequence: None,
                    container: None,
                    usage: Usage {
                        input_tokens: 0,
                        output_tokens: 0,
                        ..Usage::default()
                    },
                },
            });

            let mut line_buf = String::new();
            let mut byte_buf = acquire_stream_buffer();
            let mut content_index: u32 = 0;
            let mut text_started = false;
            let mut thinking_started = false;
            let mut tool_indices: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
            let is_reasoning_model = requires_reasoning_content(&model);

            let mut byte_stream = std::pin::pin!(byte_stream);
            let idle = stream_idle_timeout();

            // #103 流解码诊断的遥测数据：自流开始以来的接收字节数和
            // 上次成功事件的时间。在 reqwest 产生块错误时出现在错误日志中，
            // 以便在调查不稳定会话时区分 HTTP/2 RST_STREAM、
            // 块解码失败和 gzip 损坏。
            let stream_start = std::time::Instant::now();
            let mut last_event_at = std::time::Instant::now();
            let mut bytes_received: usize = 0;

            loop {
                let chunk_result = match tokio_timeout(idle, byte_stream.next()).await {
                    Ok(Some(result)) => result,
                    Ok(None) => break, // 流正常结束
                    Err(_elapsed) => {
                        yield Err(anyhow::anyhow!(
                            "SSE stream idle timeout after {}s — no data received",
                            idle.as_secs(),
                        ));
                        break;
                    }
                };
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        // 遍历错误源链，使 reqwest 底层的 hyper/h2/io 错误可见
                        // — 没有这个，外层的"解码响应体错误"消息
                        // 无法告诉我们流为何死亡。
                        let mut error_chain = format!("{e}");
                        let mut current: Option<&(dyn std::error::Error + 'static)> =
                            std::error::Error::source(&e);
                        while let Some(source) = current {
                            error_chain.push_str(&format!(" -> {source}"));
                            current = std::error::Error::source(source);
                        }
                        crate::logging::warn(format!(
                            "Stream read error: {error_chain} \
                             (elapsed: {}ms, bytes_received: {}, ms_since_last_event: {}, headers: {})",
                            stream_start.elapsed().as_millis(),
                            bytes_received,
                            last_event_at.elapsed().as_millis(),
                            response_headers,
                        ));
                        yield Err(anyhow::anyhow!("Stream read error: {e}"));
                        break;
                    }
                };

                bytes_received = bytes_received.saturating_add(chunk.len());
                last_event_at = std::time::Instant::now();
                byte_buf.extend_from_slice(&chunk);

                // 防止缓冲区无限增长（例如，没有换行符的畸形流）
                const MAX_SSE_BUF: usize = 10 * 1024 * 1024; // 10 MB
                if byte_buf.len() > MAX_SSE_BUF {
                    yield Err(anyhow::anyhow!("SSE buffer exceeded {MAX_SSE_BUF} bytes — aborting stream"));
                    break;
                }

                if byte_buf.len() > SSE_BACKPRESSURE_HIGH_WATERMARK {
                    tokio::time::sleep(Duration::from_millis(SSE_BACKPRESSURE_SLEEP_MS)).await;
                }

                // 处理缓冲区中完整的 SSE 行
                let mut lines_processed = 0usize;
                while let Some(newline_pos) = byte_buf.iter().position(|&b| b == b'\n') {
                    let mut end = newline_pos;
                    if end > 0 && byte_buf[end - 1] == b'\r' {
                        end -= 1;
                    }
                    let line = String::from_utf8_lossy(&byte_buf[..end]).into_owned();
                    byte_buf.drain(..newline_pos + 1);

                    if line.is_empty() {
                        // 空行 = 事件边界，处理累积的数据
                        if !line_buf.is_empty() {
                            let data = std::mem::take(&mut line_buf);
                            if data.trim() == "[DONE]" {
                                // 流已完成
                            } else if let Ok(chunk_json) = serde_json::from_str::<Value>(&data) {
                                // 将 SSE 块解析为流事件
                                for mut event in parse_sse_chunk(
                                    &chunk_json,
                                    &mut content_index,
                                    &mut text_started,
                                    &mut thinking_started,
                                    &mut tool_indices,
                                    is_reasoning_model,
                                ) {
                                    // 在最终 usage 上盖戳客户端侧的重放令牌估计值，
                                    // 以便 UI 可以显示它（#30）。
                                    // 我们在请求前计算它，
                                    // 并在流完成时覆盖服务器报告的 usage。
                                    if let Some(tokens) = replay_input_tokens
                                        && let StreamEvent::MessageDelta {
                                            usage: Some(usage),
                                            ..
                                        } = &mut event
                                    {
                                        usage.reasoning_replay_tokens = Some(tokens);
                                    }
                                    yield Ok(event);
                                }
                            }
                        }
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        line_buf.push_str(data);
                    }
                    // 忽略其他 SSE 字段（event:, id:, retry:）

                    lines_processed = lines_processed.saturating_add(1);
                    if lines_processed >= SSE_MAX_LINES_PER_CHUNK {
                        // 通过让步提供背压缓解，避免饿死下游消费者。
                        break;
                    }
                }
            }

            // 关闭任何打开的块
            if thinking_started {
                yield Ok(StreamEvent::ContentBlockStop { index: content_index.saturating_sub(1) });
            }
            if text_started {
                yield Ok(StreamEvent::ContentBlockStop { index: content_index.saturating_sub(1) });
            }

            release_stream_buffer(byte_buf);
            yield Ok(StreamEvent::MessageStop);
        };

        Ok(Pin::from(Box::new(stream)
            as Box<
                dyn futures_util::Stream<Item = Result<StreamEvent>> + Send,
            >))
    }
}

/// 强制性的中文推理语言指令。
///
/// 此指令在每次 API 请求中作为独立的系统消息注入，
/// 独立于主系统提示词。它确保即使系统提示词被压缩、
/// 修改或丢失，模型仍然始终接收到用中文进行思考的
/// 硬性要求。内联正则表达式的书写方式使其
/// 在 DeepSeek V4 的潜在语言漂移场景下更具鲁棒性。
const CHINESE_REASONING_INSTRUCTION: &str = "\
## 推理语言强制指令（硬性要求 — 不可协商）

### 你必须遵守以下规则：

1. **思考过程必须使用中文。** 你的所有 `reasoning_content`（推理过程、
   思维链、计划步骤、工具选择原因、内部分析）必须使用中文书写。
   这是强制性的，即使你认为用英文思考更自然。

2. **最终回复也必须使用中文。** 当用户使用中文时，你的文本回复
   必须使用中文。代码、文件路径、工具名称和标识符保持原始形式。

3. **语言跟随用户。** 用户使用什么自然语言，你的思考和回复
   就必须使用什么自然语言。用户使用简体中文时，思考和回复
   必须使用简体中文。

4. **优先级最高。** 本条指令优先级高于系统提示词中任何其他
   与语言相关的指令。这是硬性要求，不可协商。

**违背后果：** 如果模型使用非用户语言的思考过程，
将被视为违反核心指令，必须纠正。

---

> 注：上述指令仅影响自然语言散文。代码、标识符、路径、
> 工具名称、URL 和日志行始终保留原始形式。";

// === 聊天补全辅助函数 ===

#[cfg(test)]
pub(super) fn build_chat_messages(
    system: Option<&SystemPrompt>,
    messages: &[Message],
    model: &str,
) -> Vec<Value> {
    build_chat_messages_with_reasoning(
        system,
        messages,
        model,
        should_replay_reasoning_content(model, None),
    )
}

pub(super) fn build_chat_messages_for_request(request: &MessageRequest) -> Vec<Value> {
    build_chat_messages_with_reasoning(
        request.system.as_ref(),
        &request.messages,
        &request.model,
        should_replay_reasoning_content(&request.model, request.reasoning_effort.as_deref()),
    )
}

fn build_chat_messages_with_reasoning(
    system: Option<&SystemPrompt>,
    messages: &[Message],
    _model: &str,
    include_reasoning: bool,
) -> Vec<Value> {
    let mut out = Vec::new();
    let mut pending_tool_calls: HashSet<String> = HashSet::new();

    if let Some(instructions) = system_to_instructions(system.cloned())
        && !instructions.trim().is_empty()
    {
        out.push(json!({
            "role": "system",
            "content": instructions,
        }));
    }

    // 注入强制中文推理语言指令。
    // 作为独立的系统消息发送，以确保即使主系统提示词
    // 被压缩或修改，模型仍然接收到用中文进行思考的硬性要求。
    // 此指令独立于系统提示词且优先级更高。
    out.push(json!({
        "role": "system",
        "content": CHINESE_REASONING_INSTRUCTION,
    }));

    for (message_index, message) in messages.iter().enumerate() {
        let role = message.role.as_str();
        let mut text_parts = Vec::new();
        let mut thinking_parts = Vec::new();
        let mut tool_calls = Vec::new();
        let mut tool_call_ids = Vec::new();
        let mut tool_results: Vec<(String, Value)> = Vec::new();
        let later_user_turn = messages[message_index + 1..]
            .iter()
            .any(message_starts_user_turn);

        for block in &message.content {
            match block {
                ContentBlock::Text { text, .. } => text_parts.push(text.clone()),
                ContentBlock::Thinking { thinking } => thinking_parts.push(thinking.clone()),
                ContentBlock::ToolUse {
                    id,
                    name,
                    input,
                    caller,
                    ..
                } => {
                    let args = serde_json::to_string(input).unwrap_or_else(|_| input.to_string());
                    let mut call = json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": to_api_tool_name(name),
                            "arguments": args,
                        }
                    });
                    if let Some(caller) = caller {
                        call["caller"] = json!({
                            "type": caller.caller_type,
                            "tool_id": caller.tool_id,
                        });
                    }
                    tool_calls.push(call);
                    tool_call_ids.push(id.clone());
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    tool_results.push((
                        tool_use_id.clone(),
                        json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": content,
                        }),
                    ));
                }
                ContentBlock::ServerToolUse { .. }
                | ContentBlock::ToolSearchToolResult { .. }
                | ContentBlock::CodeExecutionToolResult { .. } => {}
            }
        }

        if role == "assistant" {
            let content = text_parts.join("\n");
            let mut reasoning_content = thinking_parts.join("\n");
            let has_text = !content.trim().is_empty();
            let has_tool_calls = !tool_calls.is_empty();
            // DeepSeek 思考模式的工具调用必须在后续请求中重放 `reasoning_content`。
            // 非工具的助手推理在后续真实用户文本消息开始新轮次后可以省略。
            let include_reasoning_for_turn =
                include_reasoning && (has_tool_calls || !later_user_turn);
            let mut has_reasoning =
                include_reasoning_for_turn && !reasoning_content.trim().is_empty();
            if include_reasoning_for_turn && has_tool_calls && !has_reasoning {
                logging::warn(
                    "Substituting placeholder reasoning_content for DeepSeek tool-call assistant message",
                );
                reasoning_content = String::from("（推理已省略）");
                has_reasoning = true;
            }

            // DeepSeek 拒绝 `content` 和 `tool_calls` 都缺失/为 null 的助手消息。
            // 跳过此类条目，即使它们携带仅包含推理的元数据，
            // 除非我们可以发送一个非 null 的占位内容字段。
            if !has_text && !has_tool_calls && !has_reasoning {
                pending_tool_calls.clear();
                continue;
            }

            let mut msg = json!({
                "role": "assistant",
                "content": if has_text {
                    json!(content)
                } else if has_reasoning {
                    json!("")
                } else {
                    Value::Null
                },
            });
            if has_reasoning {
                msg["reasoning_content"] = json!(reasoning_content);
            }
            if has_tool_calls {
                msg["tool_calls"] = json!(tool_calls);
                pending_tool_calls = tool_call_ids.into_iter().collect();
            } else {
                pending_tool_calls.clear();
            }
            out.push(msg);
        } else if role == "system" {
            let content = text_parts.join("\n");
            if !content.trim().is_empty() {
                out.push(json!({
                    "role": "system",
                    "content": content,
                }));
            }
        } else if role == "user" {
            let content = text_parts.join("\n");
            if !content.trim().is_empty() {
                out.push(json!({
                    "role": "user",
                    "content": content,
                }));
            }
        }

        if !tool_results.is_empty() {
            if pending_tool_calls.is_empty() {
                logging::warn("Dropping tool results without matching tool_calls");
            } else {
                for (tool_id, tool_msg) in tool_results {
                    if pending_tool_calls.remove(&tool_id) {
                        out.push(tool_msg);
                    } else {
                        logging::warn(format!(
                            "Dropping tool result for unknown tool_call_id: {tool_id}"
                        ));
                    }
                }
            }
        } else if role != "assistant" {
            pending_tool_calls.clear();
        }
    }

    // 安全网：压缩后，助手消息可能包含其结果已被摘要化的 tool_calls。
    // API 会拒绝这些，因此剥离 tool_calls（降级为普通助手消息）
    // 并移除现在已孤立的工具结果消息。
    let mut i = 0;
    while i < out.len() {
        let is_assistant_with_tools = out[i].get("role").and_then(Value::as_str)
            == Some("assistant")
            && out[i].get("tool_calls").is_some();

        if is_assistant_with_tools {
            let expected_ids: HashSet<String> = out[i]
                .get("tool_calls")
                .and_then(Value::as_array)
                .map(|calls| {
                    calls
                        .iter()
                        .filter_map(|c| c.get("id").and_then(Value::as_str).map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            // 收集紧接在此助手消息之后的工具结果 ID。
            let mut found_ids: HashSet<String> = HashSet::new();
            let mut tool_result_end = i + 1;
            while tool_result_end < out.len() {
                if out[tool_result_end].get("role").and_then(Value::as_str) == Some("tool") {
                    if let Some(id) = out[tool_result_end]
                        .get("tool_call_id")
                        .and_then(Value::as_str)
                    {
                        found_ids.insert(id.to_string());
                    }
                    tool_result_end += 1;
                } else {
                    break;
                }
            }

            // 也扫描到下一个助手消息为止的非连续工具结果，
            // 以防压缩留下了间隙。
            let mut scan = tool_result_end;
            while scan < out.len() {
                if out[scan].get("role").and_then(Value::as_str) == Some("assistant") {
                    break;
                }
                if out[scan].get("role").and_then(Value::as_str) == Some("tool")
                    && let Some(id) = out[scan].get("tool_call_id").and_then(Value::as_str)
                {
                    found_ids.insert(id.to_string());
                }
                scan += 1;
            }

            if !expected_ids.is_subset(&found_ids) {
                let missing: Vec<_> = expected_ids.difference(&found_ids).collect();
                logging::warn(format!(
                    "Stripping orphaned tool_calls from assistant message \
                     (expected {} tool results, found {}, missing: {:?})",
                    expected_ids.len(),
                    found_ids.len(),
                    missing
                ));
                if let Some(obj) = out[i].as_object_mut() {
                    obj.remove("tool_calls");
                }
                // 如果 tool_calls 是唯一的助手内容，则完全移除现在无效的
                // 助手消息（DeepSeek 要求必须有 content 或 tool_calls）。
                let assistant_content_empty = out[i]
                    .get("content")
                    .is_none_or(|v| v.is_null() || v.as_str().is_some_and(str::is_empty));
                if assistant_content_empty {
                    // 移除与此被剥离的助手调用集相关的孤立工具结果。
                    let mut j = out.len();
                    while j > i + 1 {
                        j -= 1;
                        if out[j].get("role").and_then(Value::as_str) == Some("tool")
                            && let Some(id) = out[j].get("tool_call_id").and_then(Value::as_str)
                            && expected_ids.contains(id)
                        {
                            out.remove(j);
                        }
                    }
                    out.remove(i);
                    i = i.saturating_sub(1);
                    continue;
                }
                // 先移除连续的工具结果
                if tool_result_end > i + 1 {
                    out.drain((i + 1)..tool_result_end);
                }
                // 移除任何引用 expected_ids 的剩余非连续工具结果
                //（反向扫描以避免索引移位问题）
                let mut j = out.len();
                while j > i + 1 {
                    j -= 1;
                    if out[j].get("role").and_then(Value::as_str) == Some("tool")
                        && let Some(id) = out[j].get("tool_call_id").and_then(Value::as_str)
                        && expected_ids.contains(id)
                    {
                        out.remove(j);
                    }
                }
            }
        }
        i += 1;
    }

    out
}

fn message_starts_user_turn(message: &Message) -> bool {
    message.role == "user"
        && message.content.iter().any(|block| match block {
            ContentBlock::Text { text, .. } => !text.trim().is_empty(),
            _ => false,
        })
}

pub(super) fn tool_to_chat(tool: &Tool) -> Value {
    let mut value = json!({
        "type": "function",
        "function": {
            "name": to_api_tool_name(&tool.name),
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    });
    if let Some(allowed_callers) = &tool.allowed_callers {
        value["allowed_callers"] = json!(allowed_callers);
    }
    if let Some(defer_loading) = tool.defer_loading {
        value["defer_loading"] = json!(defer_loading);
    }
    if let Some(input_examples) = &tool.input_examples {
        value["input_examples"] = json!(input_examples);
    }
    if let Some(strict) = tool.strict
        && let Some(function) = value.get_mut("function")
    {
        function["strict"] = json!(strict);
    }
    value
}

pub(super) fn tool_to_chat_for_base_url(tool: &Tool, base_url: &str) -> Value {
    let mut value = tool_to_chat(tool);
    if !deepseek_base_url_supports_strict_tools(base_url)
        && let Some(function) = value.get_mut("function")
        && let Some(obj) = function.as_object_mut()
    {
        obj.remove("strict");
    }
    value
}

fn deepseek_base_url_supports_strict_tools(base_url: &str) -> bool {
    let trimmed = base_url.trim_end_matches('/').to_ascii_lowercase();
    let is_deepseek = trimmed == "https://api.deepseek.com"
        || trimmed == "https://api.deepseek.com/v1"
        || trimmed == "https://api.deepseek.com/beta"
        || trimmed == "https://api.deepseeki.com"
        || trimmed == "https://api.deepseeki.com/v1"
        || trimmed == "https://api.deepseeki.com/beta";
    !is_deepseek || trimmed.ends_with("/beta")
}

fn map_tool_choice_for_chat(choice: &Value) -> Option<Value> {
    if let Some(choice_str) = choice.as_str() {
        return Some(json!(choice_str));
    }
    let Some(choice_type) = choice.get("type").and_then(Value::as_str) else {
        return Some(choice.clone());
    };

    match choice_type {
        "auto" | "none" => Some(json!(choice_type)),
        "any" => Some(json!("auto")),
        "tool" => choice.get("name").and_then(Value::as_str).map(|name| {
            json!({
                "type": "function",
                "function": { "name": to_api_tool_name(name) }
            })
        }),
        _ => Some(choice.clone()),
    }
}

/// 对即将发出的聊天补全 JSON 负载执行最终清理。
/// 当模型 + 推理努力度组合需要时，为携带 `tool_calls` 的助手消息
/// 强制添加非空的 `reasoning_content`。DeepSeek 的思考模式 API
/// 会拒绝此类消息并返回 400 错误；替换占位符可保持对话链完整。
/// 非工具的助手推理在后续用户文本轮次开始后可以保持省略。
///
/// 同时统计所有重放的 `reasoning_content` 大小并记录日志，
/// 方便使用 `RUST_LOG=deepseek_tui=debug` 的用户查看
/// 输入预算中有多少被用于重新发送先前的思考轨迹。
pub(super) fn sanitize_thinking_mode_messages(
    body: &mut Value,
    model: &str,
    effort: Option<&str>,
) -> Option<u32> {
    if !should_replay_reasoning_content(model, effort) {
        return None;
    }
    let messages = body.get_mut("messages").and_then(Value::as_array_mut)?;
    let mut substitutions: u32 = 0;
    let mut replay_chars: u64 = 0;
    let mut replay_messages: u32 = 0;
    for (idx, msg) in messages.iter_mut().enumerate() {
        if msg.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let has_tool_calls = msg.get("tool_calls").is_some();
        let needs_placeholder = msg
            .get("reasoning_content")
            .and_then(Value::as_str)
            .is_none_or(|s| s.trim().is_empty());
        if has_tool_calls && needs_placeholder {
            msg["reasoning_content"] = json!("(reasoning omitted)");
            substitutions = substitutions.saturating_add(1);
            logging::warn(format!(
                "Final sanitizer: forced reasoning_content placeholder on assistant[{idx}]",
            ));
        }
        if let Some(reasoning) = msg.get("reasoning_content").and_then(Value::as_str) {
            let len = reasoning.len() as u64;
            if len > 0 {
                replay_chars = replay_chars.saturating_add(len);
                replay_messages = replay_messages.saturating_add(1);
            }
        }
    }
    if substitutions > 0 {
        logging::warn(format!(
            "Final sanitizer: {substitutions} assistant message(s) needed reasoning_content placeholder",
        ));
    }
    if replay_messages == 0 {
        return None;
    }
    // ~4 字符/令牌是标准的粗略估计；DeepSeek 令牌在中文/代码上略短，但这是数量级信息。
    let approx_tokens = (replay_chars / 4).min(u64::from(u32::MAX)) as u32;
    logging::info(format!(
        "Reasoning-content replay: {replay_messages} assistant message(s), ~{approx_tokens} input tokens ({replay_chars} chars) being re-sent in this request",
    ));
    Some(approx_tokens)
}

/// 对传出聊天补全体中所有助手消息的 `reasoning_content` 进行字节长度求和。
/// 由测试使用；生产环境中的清理器内联计算相同数值并记录日志。
#[cfg(test)]
pub(super) fn count_reasoning_replay_chars(body: &Value) -> u64 {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return 0;
    };
    messages
        .iter()
        .filter(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))
        .filter_map(|m| m.get("reasoning_content").and_then(Value::as_str))
        .map(|s| s.len() as u64)
        .sum()
}

/// 渲染我们关心的 #103 诊断的传输层头部。
/// 始终返回可打印的内容，以便解码错误日志行可解析，即使服务器去掉了我们期望的头部。
fn format_stream_headers(headers: &reqwest::header::HeaderMap) -> String {
    const FIELDS: &[&str] = &[
        "content-encoding",
        "transfer-encoding",
        "connection",
        "server",
    ];
    let mut parts: Vec<String> = Vec::with_capacity(FIELDS.len());
    for field in FIELDS {
        let rendered = headers
            .get(*field)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(absent)");
        parts.push(format!("{field}={rendered}"));
    }
    parts.join(", ")
}

/// 当 DeepSeek 尽管经过清理仍拒绝请求时触发的诊断日志器。
/// 遍历请求体，记录哪些助手消息有 tool_calls 但没有 `reasoning_content`
/// — 有助于追踪完全绕过清理器的代码路径。
fn log_thinking_mode_violations(body: &Value) {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        logging::warn("400-after-sanitizer: body has no `messages` array");
        return;
    };
    let mut violations: Vec<String> = Vec::new();
    for (idx, msg) in messages.iter().enumerate() {
        if msg.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let reasoning = msg
            .get("reasoning_content")
            .and_then(Value::as_str)
            .unwrap_or("");
        let has_tc = msg.get("tool_calls").is_some();
        if reasoning.trim().is_empty() {
            violations.push(format!(
                "assistant[{idx}] (reasoning_content missing, tool_calls={})",
                has_tc
            ));
        }
    }
    if violations.is_empty() {
        logging::warn(
            "400-after-sanitizer: all assistant messages have reasoning_content — DeepSeek rejected for a different reason",
        );
    } else {
        logging::warn(format!(
            "400-after-sanitizer: {} assistant message(s) lack reasoning_content despite sanitizer: {}",
            violations.len(),
            violations.join(", ")
        ));
    }
}

fn requires_reasoning_content(model: &str) -> bool {
    let lower = model.to_lowercase();
    lower.contains("deepseek-v4")
        || lower.contains("reasoner")
        || lower.contains("-reasoning")
        || lower.contains("-thinking")
        || has_deepseek_r_series_marker(&lower)
}

fn should_replay_reasoning_content(model: &str, effort: Option<&str>) -> bool {
    if effort
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "off" | "disabled" | "none" | "false"
            )
        })
        .unwrap_or(false)
    {
        return false;
    }

    requires_reasoning_content(model)
}

fn has_deepseek_r_series_marker(model_lower: &str) -> bool {
    const PREFIX: &str = "deepseek-r";
    model_lower.match_indices(PREFIX).any(|(idx, _)| {
        model_lower[idx + PREFIX.len()..]
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit())
    })
}

fn reasoning_field(value: &Value) -> Option<&str> {
    value
        .get("reasoning_content")
        .or_else(|| value.get("reasoning"))
        .and_then(Value::as_str)
}

pub(super) fn parse_chat_message(payload: &Value) -> Result<MessageResponse> {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("chatcmpl")
        .to_string();
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let choices = payload
        .get("choices")
        .and_then(Value::as_array)
        .context("Chat API response missing choices")?;
    let choice = choices
        .first()
        .context("Chat API response missing first choice")?;
    let message = choice
        .get("message")
        .context("Chat API response missing message")?;

    let mut content_blocks = Vec::new();
    if let Some(reasoning) =
        reasoning_field(message).filter(|reasoning| !reasoning.trim().is_empty())
    {
        content_blocks.push(ContentBlock::Thinking {
            thinking: reasoning.to_string(),
        });
    }
    if let Some(text) = message.get("content").and_then(Value::as_str)
        && !text.trim().is_empty()
    {
        content_blocks.push(ContentBlock::Text {
            text: text.to_string(),
            cache_control: None,
        });
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in tool_calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("tool_call")
                .to_string();
            let function = call.get("function");
            let name = function
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let arguments = function
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
                .map(|raw| serde_json::from_str(raw).unwrap_or(Value::String(raw.to_string())))
                .unwrap_or(Value::Null);
            let caller = call.get("caller").and_then(|v| {
                v.get("type")
                    .and_then(Value::as_str)
                    .map(|caller_type| ToolCaller {
                        caller_type: caller_type.to_string(),
                        tool_id: v
                            .get("tool_id")
                            .and_then(Value::as_str)
                            .map(std::string::ToString::to_string),
                    })
            });

            content_blocks.push(ContentBlock::ToolUse {
                id,
                name: from_api_tool_name(&name),
                input: arguments,
                caller,
            });
        }
    }

    let usage = parse_usage(payload.get("usage"));

    Ok(MessageResponse {
        id,
        r#type: "message".to_string(),
        role: "assistant".to_string(),
        content: content_blocks,
        model,
        stop_reason: choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .map(str::to_string),
        stop_sequence: None,
        container: None,
        usage,
    })
}

// === 流式处理辅助函数 ===

/// 从非流式响应构建合成的流事件（用作回退）。
#[allow(dead_code)]
fn build_stream_events(response: &MessageResponse) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    let mut index = 0u32;

    events.push(StreamEvent::MessageStart {
        message: response.clone(),
    });

    for block in &response.content {
        match block {
            ContentBlock::Text { text, .. } => {
                events.push(StreamEvent::ContentBlockStart {
                    index,
                    content_block: ContentBlockStart::Text {
                        text: String::new(),
                    },
                });
                if !text.is_empty() {
                    events.push(StreamEvent::ContentBlockDelta {
                        index,
                        delta: Delta::TextDelta { text: text.clone() },
                    });
                }
                events.push(StreamEvent::ContentBlockStop { index });
            }
            ContentBlock::Thinking { thinking } => {
                events.push(StreamEvent::ContentBlockStart {
                    index,
                    content_block: ContentBlockStart::Thinking {
                        thinking: String::new(),
                    },
                });
                if !thinking.is_empty() {
                    events.push(StreamEvent::ContentBlockDelta {
                        index,
                        delta: Delta::ThinkingDelta {
                            thinking: thinking.clone(),
                        },
                    });
                }
                events.push(StreamEvent::ContentBlockStop { index });
            }
            ContentBlock::ToolUse {
                id, name, input, ..
            } => {
                events.push(StreamEvent::ContentBlockStart {
                    index,
                    content_block: ContentBlockStart::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                        caller: None,
                    },
                });
                events.push(StreamEvent::ContentBlockStop { index });
            }
            ContentBlock::ToolResult { .. } => {}
            ContentBlock::ServerToolUse { id, name, input } => {
                events.push(StreamEvent::ContentBlockStart {
                    index,
                    content_block: ContentBlockStart::ServerToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    },
                });
                events.push(StreamEvent::ContentBlockStop { index });
            }
            ContentBlock::ToolSearchToolResult { .. }
            | ContentBlock::CodeExecutionToolResult { .. } => {}
        }
        index = index.saturating_add(1);
    }

    events.push(StreamEvent::MessageDelta {
        delta: MessageDelta {
            stop_reason: response.stop_reason.clone(),
            stop_sequence: response.stop_sequence.clone(),
        },
        usage: Some(response.usage.clone()),
    });
    events.push(StreamEvent::MessageStop);

    events
}

// === SSE 块解析器 ===

/// 将来自聊天补全流式 API 的单个 SSE 块解析为我们的内部 `StreamEvent` 表示。
pub(super) fn parse_sse_chunk(
    chunk: &Value,
    content_index: &mut u32,
    text_started: &mut bool,
    thinking_started: &mut bool,
    tool_indices: &mut std::collections::HashMap<u32, u32>,
    is_reasoning_model: bool,
) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
        // 仅包含用量信息的块（在末尾随 stream_options 发送）
        if let Some(usage_val) = chunk.get("usage") {
            let usage = parse_usage(Some(usage_val));
            events.push(StreamEvent::MessageDelta {
                delta: MessageDelta {
                    stop_reason: None,
                    stop_sequence: None,
                },
                usage: Some(usage),
            });
        }
        return events;
    };

    if choices.is_empty() {
        if let Some(usage_val) = chunk.get("usage") {
            let usage = parse_usage(Some(usage_val));
            events.push(StreamEvent::MessageDelta {
                delta: MessageDelta {
                    stop_reason: None,
                    stop_sequence: None,
                },
                usage: Some(usage),
            });
        }
        return events;
    }

    for choice in choices {
        let delta = choice.get("delta");
        let finish_reason = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .map(str::to_string);

        if let Some(delta) = delta {
            // 处理 reasoning_content / reasoning 思考增量。
            if is_reasoning_model
                && let Some(reasoning) = reasoning_field(delta)
                && !reasoning.is_empty()
            {
                if !*thinking_started {
                    events.push(StreamEvent::ContentBlockStart {
                        index: *content_index,
                        content_block: ContentBlockStart::Thinking {
                            thinking: String::new(),
                        },
                    });
                    *thinking_started = true;
                }
                events.push(StreamEvent::ContentBlockDelta {
                    index: *content_index,
                    delta: Delta::ThinkingDelta {
                        thinking: reasoning.to_string(),
                    },
                });
            }

            // 处理常规内容
            if let Some(content) = delta.get("content").and_then(Value::as_str)
                && !content.is_empty()
            {
                // 如果在过渡到文本时关闭思考块
                if *thinking_started {
                    events.push(StreamEvent::ContentBlockStop {
                        index: *content_index,
                    });
                    *content_index += 1;
                    *thinking_started = false;
                }
                if !*text_started {
                    events.push(StreamEvent::ContentBlockStart {
                        index: *content_index,
                        content_block: ContentBlockStart::Text {
                            text: String::new(),
                        },
                    });
                    *text_started = true;
                }
                events.push(StreamEvent::ContentBlockDelta {
                    index: *content_index,
                    delta: Delta::TextDelta {
                        text: content.to_string(),
                    },
                });
            }

            // 处理工具调用
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for tc in tool_calls {
                    let tc_index = tc.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
                    let tool_block_index = match tool_indices.entry(tc_index) {
                        std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            // Close text block if transitioning to tool use
                            if *text_started {
                                events.push(StreamEvent::ContentBlockStop {
                                    index: *content_index,
                                });
                                *content_index += 1;
                                *text_started = false;
                            }
                            if *thinking_started {
                                events.push(StreamEvent::ContentBlockStop {
                                    index: *content_index,
                                });
                                *content_index += 1;
                                *thinking_started = false;
                            }

                            let block_index = *content_index;
                            let id = tc
                                .get("id")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                                // 某些上游网关（以及 responses-API 桥接器）
                                // 会在工具调用的第一个块上省略 `id`。
                                // 如果回退到常量字符串，当模型在同一 delta 中发出
                                // 并行工具调用时会发生冲突——每个调用最终具有相同的 id，
                                // 下游工具结果路由会两次匹配第一个调用。
                                // 通过内容块位置索引以保持回退在响应内唯一。
                                .unwrap_or_else(|| format!("call_{block_index}"));
                            let name = tc
                                .get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let caller = tc.get("caller").and_then(|v| {
                                v.get("type").and_then(Value::as_str).map(|caller_type| {
                                    ToolCaller {
                                        caller_type: caller_type.to_string(),
                                        tool_id: v
                                            .get("tool_id")
                                            .and_then(Value::as_str)
                                            .map(std::string::ToString::to_string),
                                    }
                                })
                            });

                            events.push(StreamEvent::ContentBlockStart {
                                index: block_index,
                                content_block: ContentBlockStart::ToolUse {
                                    id,
                                    name: from_api_tool_name(&name),
                                    input: json!({}),
                                    caller,
                                },
                            });
                            *content_index = (*content_index).saturating_add(1);
                            entry.insert(block_index);
                            block_index
                        }
                    };

                    // 流式传输工具调用参数
                    if let Some(args) = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(Value::as_str)
                        && !args.is_empty()
                    {
                        events.push(StreamEvent::ContentBlockDelta {
                            index: tool_block_index,
                            delta: Delta::InputJsonDelta {
                                partial_json: args.to_string(),
                            },
                        });
                    }
                }
            }
        }

        // 处理结束原因
        if let Some(reason) = finish_reason {
            // Close any open blocks
            if *text_started {
                events.push(StreamEvent::ContentBlockStop {
                    index: *content_index,
                });
                *text_started = false;
            }
            if *thinking_started {
                events.push(StreamEvent::ContentBlockStop {
                    index: *content_index,
                });
                *thinking_started = false;
            }
            // 关闭工具块
            let mut open_tool_indices: Vec<u32> =
                tool_indices.drain().map(|(_, idx)| idx).collect();
            open_tool_indices.sort_unstable();
            for tool_block_index in open_tool_indices {
                events.push(StreamEvent::ContentBlockStop {
                    index: tool_block_index,
                });
            }

            // 如果可用，从块中发出用量信息
            let chunk_usage = chunk.get("usage").map(|u| parse_usage(Some(u)));
            events.push(StreamEvent::MessageDelta {
                delta: MessageDelta {
                    stop_reason: Some(reason),
                    stop_sequence: None,
                },
                usage: chunk_usage,
            });
        }
    }

    events
}

// === #103 阶段 1：流解码诊断 ===================================

#[cfg(test)]
mod stream_diagnostics_tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    #[test]
    fn stream_open_timeout_defaults_and_clamps_env_values() {
        assert_eq!(stream_open_timeout_from_env(None), Duration::from_secs(45));
        assert_eq!(
            stream_open_timeout_from_env(Some("not-a-number")),
            Duration::from_secs(45)
        );
        assert_eq!(
            stream_open_timeout_from_env(Some("1")),
            Duration::from_secs(5)
        );
        assert_eq!(
            stream_open_timeout_from_env(Some("120")),
            Duration::from_secs(120)
        );
        assert_eq!(
            stream_open_timeout_from_env(Some("999")),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn format_stream_headers_renders_all_fields_when_present() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", HeaderValue::from_static("gzip"));
        headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        headers.insert("connection", HeaderValue::from_static("keep-alive"));
        headers.insert("server", HeaderValue::from_static("openresty/1.25.3.1"));

        let rendered = format_stream_headers(&headers);
        // 顺序由辅助函数中的 FIELDS 固定；断言每个字段都出现。
        assert!(
            rendered.contains("content-encoding=gzip"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains("transfer-encoding=chunked"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains("connection=keep-alive"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains("server=openresty/1.25.3.1"),
            "got: {rendered}"
        );
    }

    #[test]
    fn format_stream_headers_marks_missing_fields_as_absent() {
        // DeepSeek 在不压缩时经常省略 content-encoding。
        // 诊断仍必须产生可解析的行，以便日志抓取器不会丢失该槽位。
        let headers = HeaderMap::new();
        let rendered = format_stream_headers(&headers);
        assert!(
            rendered.contains("content-encoding=(absent)"),
            "missing field must be explicitly marked; got: {rendered}"
        );
        assert!(
            rendered.contains("transfer-encoding=(absent)"),
            "missing field must be explicitly marked; got: {rendered}"
        );
    }

    #[test]
    fn format_stream_headers_handles_non_ascii_value_gracefully() {
        // 如果头部值不是 UTF-8，`.to_str()` 会失败 — 我们不能 panic，
        // 并且仍应产生可解析的行。
        let mut headers = HeaderMap::new();
        // 0xFF 是有效字节，但属于无效的 UTF-8 起始字节。
        headers.insert(
            "server",
            HeaderValue::from_bytes(b"\xff\xfemystery").expect("header value"),
        );
        let rendered = format_stream_headers(&headers);
        assert!(
            rendered.contains("server=(absent)"),
            "non-UTF8 header values fall back to (absent); got: {rendered}"
        );
    }
}

// === #103 阶段 4：SSE 解码器在预设块序列上的行为 ============

#[cfg(test)]
mod stream_decoder_tests {
    //! 在预设的块序列上驱动 `parse_sse_chunk`（就地 SSE 事件提取器）。
    //! 完整的 `handle_chat_completion_stream` 路径需要实时的 `reqwest::Response`，
    //! 因此没有模拟 HTTP 框架就无法进行单元测试（问题 #69 跟踪此问题）。
    //! 对于 #103，我们直接测试块解码器，以验证引擎依赖的每种"流失败类别"。
    use super::*;
    use crate::models::{ContentBlockStart, Delta, StreamEvent};

    /// 将原始 SSE 数据 JSON 块解码为内部事件，镜像 `handle_chat_completion_stream` 使用的每个事件调用形状。
    fn decode_chunk(json_text: &str) -> Vec<StreamEvent> {
        let chunk: Value = serde_json::from_str(json_text).expect("valid SSE JSON");
        let mut content_index = 0u32;
        let mut text_started = false;
        let mut thinking_started = false;
        let mut tool_indices = std::collections::HashMap::new();
        parse_sse_chunk(
            &chunk,
            &mut content_index,
            &mut text_started,
            &mut thinking_started,
            &mut tool_indices,
            true,
        )
    }

    #[test]
    fn decoder_emits_text_delta_for_content_chunk() {
        // "快乐"的第一个块：一个普通的内容增量。引擎将其视为
        // `any_content_received = true`，并且在后续错误时不会透明重试。
        let events = decode_chunk(r#"{"choices":[{"delta":{"content":"hello"}}]}"#);
        assert!(
            matches!(
                events.first(),
                Some(StreamEvent::ContentBlockStart {
                    content_block: ContentBlockStart::Text { .. },
                    ..
                })
            ),
            "first event should open a text block; got {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ContentBlockDelta {
                    delta: Delta::TextDelta { text },
                    ..
                } if text == "hello")),
            "should yield a TextDelta carrying 'hello'; got {events:?}"
        );
    }

    #[test]
    fn decoder_emits_thinking_delta_for_reasoning_chunk() {
        // V4 思考模型首先展示 reasoning_content — 引擎也将其计为已接收内容
        // （因此后续流错误会直接暴露，而不是透明重试）。
        let events = decode_chunk(r#"{"choices":[{"delta":{"reasoning_content":"plan..."}}]}"#);
        assert!(
            matches!(
                events.first(),
                Some(StreamEvent::ContentBlockStart {
                    content_block: ContentBlockStart::Thinking { .. },
                    ..
                })
            ),
            "first event should open a thinking block; got {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ContentBlockDelta {
                    delta: Delta::ThinkingDelta { thinking },
                    ..
                } if thinking == "plan...")),
            "should yield a ThinkingDelta carrying 'plan...'; got {events:?}"
        );
    }

    #[test]
    fn decoder_yields_no_events_for_keepalive_chunk() {
        // DeepSeek 在发出真实内容之前经常发送 `{"choices":[]}` 保活块。
        // 引擎必须将这些视为"未收到内容"，并有资格进行透明重试
        // — 在此断言解码器不产生任何负载事件。
        let events = decode_chunk(r#"{"choices":[]}"#);
        assert!(
            events.is_empty(),
            "empty-choices chunk must produce no events; got {events:?}"
        );
    }

    #[test]
    fn decoder_emits_tool_use_block_for_tool_call_delta() {
        // 工具调用增量也是内容 — 一旦到达，透明重试必须关闭
        // （模型已提交到 DeepSeek 已计费的工具调用路径）。
        let events = decode_chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"grep_files","arguments":"{\"pattern\":\"foo\"}"}}]}}]}"#,
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                StreamEvent::ContentBlockStart {
                    content_block: ContentBlockStart::ToolUse { name, .. },
                    ..
                } if name == "grep_files"
            )),
            "should open a ToolUse block for grep_files; got {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                StreamEvent::ContentBlockDelta {
                    delta: Delta::InputJsonDelta { partial_json },
                    ..
                } if partial_json.contains("\"pattern\"")
            )),
            "should yield InputJsonDelta carrying the tool args; got {events:?}"
        );
    }

    /// 无 ID 并行工具调用冲突的回归测试（审计发现 8）：
    /// 当上游块省略 `id` 字段时，回退值以前对所有并行调用都是
    /// 字面字符串 `"tool_call"`，因此同一个 delta 中的两个工具调用
    /// 最终共享一个 id。下游路由随后两次匹配第一个调用的 tool_result，
    /// 而第二个调用挂起。现在的回退值通过内容块位置索引，
    /// 保持每个调用在响应内唯一。
    #[test]
    fn decoder_assigns_unique_fallback_ids_to_parallel_tool_calls_missing_id() {
        let events = decode_chunk(
            r#"{"choices":[{"delta":{"tool_calls":[
                {"index":0,"function":{"name":"grep_files","arguments":"{\"pattern\":\"a\"}"}},
                {"index":1,"function":{"name":"read_file","arguments":"{\"path\":\"x\"}"}}
            ]}}]}"#,
        );

        let ids: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::ContentBlockStart {
                    content_block: ContentBlockStart::ToolUse { id, .. },
                    ..
                } => Some(id.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(
            ids.len(),
            2,
            "expected two tool-use blocks for parallel tool calls; got {events:?}"
        );
        assert_ne!(
            ids[0], ids[1],
            "parallel tool calls without upstream `id` must get distinct fallback ids; got {ids:?}"
        );
    }

    #[test]
    fn decoder_preserves_upstream_tool_call_id_when_present() {
        // 回退回归测试的反向验证：当上游块确实包含 `id` 时，
        // 我们原样传递它 — 不能因为有回退路径就悄悄重写 API 给我们的 id。
        let events = decode_chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_xyz","function":{"name":"grep_files","arguments":"{}"}}]}}]}"#,
        );
        let id = events
            .iter()
            .find_map(|e| match e {
                StreamEvent::ContentBlockStart {
                    content_block: ContentBlockStart::ToolUse { id, .. },
                    ..
                } => Some(id.as_str()),
                _ => None,
            })
            .expect("tool-use block present");
        assert_eq!(id, "call_xyz");
    }

    #[test]
    fn request_builder_preserves_internal_system_messages() {
        let messages = vec![Message {
            role: "system".to_string(),
            content: vec![ContentBlock::Text {
                text: "internal runtime event".to_string(),
                cache_control: None,
            }],
        }];

        let built = build_chat_messages(None, &messages, "deepseek-v4-flash");

        assert_eq!(built.len(), 1);
        assert_eq!(built[0]["role"], "system");
        assert_eq!(built[0]["content"], "internal runtime event");
    }
}
