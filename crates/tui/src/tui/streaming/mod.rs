#![allow(dead_code)]

//! 用于实时微块渲染的 Markdown 流收集器。
//!
//! 此模块实现了来自 codex-rs 的模式：
//! - 流式文本被拆分为小的字素对齐块
//! - 提交节拍在提供者数据块之间将块滴入转录本
//! - 流结束时发出最终内容

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

use crate::palette;

pub mod chunking;
pub mod commit_tick;
pub mod line_buffer;

pub use chunking::{AdaptiveChunkingPolicy, ChunkingMode};
pub use commit_tick::{StreamChunker, run_commit_tick};
pub use line_buffer::LineBuffer;
/// 收集流式文本并提交完整行。
#[derive(Debug, Clone)]
pub struct MarkdownStreamCollector {
    /// 传入文本的缓冲区
    buffer: String,
    /// 已提交的行数
    committed_line_count: usize,
    /// 用于换行的终端宽度
    width: Option<usize>,
    /// 流是否仍处于活动状态
    is_streaming: bool,
    /// 是否为思考块
    is_thinking: bool,
}

impl Default for MarkdownStreamCollector {
    fn default() -> Self {
        // `is_streaming: true` 与 `MarkdownStreamCollector::new` 一致，
        // 因此新默认块的行为类似于新启动的流。
        Self::new(None, false)
    }
}

impl MarkdownStreamCollector {
    /// 创建一个新的收集器
    pub fn new(width: Option<usize>, is_thinking: bool) -> Self {
        Self {
            buffer: String::new(),
            committed_line_count: 0,
            width,
            is_streaming: true,
            is_thinking,
        }
    }

    /// 向缓冲区推送新内容
    pub fn push(&mut self, content: &str) {
        self.buffer.push_str(content);
    }

    /// 获取当前缓冲区内容（用于流式传输期间的显示）
    pub fn current_content(&self) -> &str {
        &self.buffer
    }

    /// 检查是否有完整行可提交
    pub fn has_complete_lines(&self) -> bool {
        self.buffer.contains('\n')
    }

    /// 提交完整行并返回它们。
    /// 仅以 '\n' 结尾的行会被提交。
    /// 返回自上次调用以来新提交的行。
    pub fn commit_complete_lines(&mut self) -> Vec<Line<'static>> {
        let committed = self.commit_complete_text();
        if committed.is_empty() {
            return Vec::new();
        }
        self.render_lines(&committed)
    }

    /// 提交以换行符结尾的完整文本块。
    /// 返回自上次调用以来变为可见的原始文本。
    pub fn commit_complete_text(&mut self) -> String {
        if self.buffer.is_empty() {
            return String::new();
        }

        // 找到最后一个换行符 — 只处理到那里
        let Some(last_newline_idx) = self.buffer.rfind('\n') else {
            return String::new(); // 还没有完整行
        };

        // 提取完整部分（直到并包含最后一个换行符）
        let complete_portion = self.buffer[..=last_newline_idx].to_string();

        // 从缓冲区中移除已提交部分，以便 finalize 只发出剩余部分
        self.buffer = self.buffer[last_newline_idx + 1..].to_string();
        self.committed_line_count = 0;

        complete_portion
    }

    /// 结束流并返回任何剩余内容。
    /// 当流结束时调用此方法以发出最终的不完整行。
    pub fn finalize(&mut self) -> Vec<Line<'static>> {
        let remaining = self.finalize_text();
        if remaining.is_empty() {
            return Vec::new();
        }
        self.render_lines(&remaining)
    }

    /// 结束流并返回任何剩余的原始文本。
    pub fn finalize_text(&mut self) -> String {
        self.is_streaming = false;

        if self.buffer.is_empty() {
            return String::new();
        }

        let remaining = self.buffer.clone();
        self.buffer.clear();
        self.committed_line_count = 0;
        remaining
    }

    /// 获取所有渲染行（用于流结束后最终显示）
    pub fn all_lines(&self) -> Vec<Line<'static>> {
        self.render_lines(&self.buffer)
    }

    /// 将内容渲染为样式化行
    fn render_lines(&self, content: &str) -> Vec<Line<'static>> {
        let width = self.width.unwrap_or(80);
        let style = if self.is_thinking {
            Style::default()
                .fg(palette::STATUS_WARNING)
                .add_modifier(Modifier::DIM | Modifier::ITALIC)
        } else {
            Style::default()
        };

        let mut lines = Vec::new();

        for line in content.lines() {
            // 对长行进行换行
            let wrapped = wrap_line(line, width);
            for wrapped_line in wrapped {
                lines.push(Line::from(Span::styled(wrapped_line, style)));
            }
        }

        // 处理尾部换行符（添加空行）
        if content.ends_with('\n') {
            lines.push(Line::from(""));
        }

        lines
    }

    /// 检查流是否仍处于活动状态
    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    /// 获取原始缓冲区长度
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// 清除缓冲区
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.committed_line_count = 0;
    }
}

/// 将单行换行以适合给定的宽度
fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for word in line.split_whitespace() {
        let word_width = word.width();

        if current_width == 0 {
            // 行中的第一个词
            current_line = word.to_string();
            current_width = word_width;
        } else if current_width + 1 + word_width <= width {
            // 单词和空格都能放下
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            // 单词放不下，开始新行
            result.push(current_line);
            current_line = word.to_string();
            current_width = word_width;
        }
    }

    if !current_line.is_empty() {
        result.push(current_line);
    }

    if result.is_empty() {
        vec![String::new()]
    } else {
        result
    }
}

/// 每块流式子状态：可选的线路缓冲区为收集器 + 分块器/策略提供
/// 两档速率的输入。
///
/// 管道：
/// ```text
/// raw delta -> LineBuffer.push -> take_committable -> collector + chunker -> commit tick
/// ```
///
/// [`LineBuffer`] 仍然可用于对换行敏感的模式。普通助手散文和思考块
/// 绕过它，以便文本可以以实时微块的形式流式传输，而不是等待换行边界。
#[derive(Debug, Default)]
struct BlockState {
    /// 换行门控：在数据块之间保留尾部的不完整行文本。
    /// 当 `bypass_gate` 为 true 时绕过（思考块）。
    line_buffer: LineBuffer,
    /// 是否绕过 [`LineBuffer`]（思考块实时流式传输）。
    bypass_gate: bool,
    collector: MarkdownStreamCollector,
    chunker: StreamChunker,
    policy: AdaptiveChunkingPolicy,
}

/// 管理多个流收集器的状态（每个内容块一个）
#[derive(Debug, Default)]
pub struct StreamingState {
    /// 按索引的每块状态（收集器 + 分块器 + 策略）。
    blocks: Vec<Option<BlockState>>,
    /// 是否有任何流当前处于活动状态
    pub is_active: bool,
    /// 用于显示的累积文本
    pub accumulated_text: String,
    /// 用于显示的累积思考内容
    pub accumulated_thinking: String,
}

impl StreamingState {
    /// 创建一个新的流状态
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动一个新的文本块。助手散文以微块形式实时流式传输，
    /// 以便用户可以在答案形成时直观地跟踪，而不是等待换行终止的行。
    pub fn start_text(&mut self, index: usize, width: Option<usize>) {
        self.ensure_capacity(index);
        self.blocks[index] = Some(BlockState {
            line_buffer: LineBuffer::new(),
            bypass_gate: true,
            collector: MarkdownStreamCollector::new(width, false),
            chunker: StreamChunker::new(),
            policy: AdaptiveChunkingPolicy::new(),
        });
        self.is_active = true;
    }

    /// 启动一个新的思考块。思考数据块绕过换行门控，
    /// 以便它们在视觉上保持实时 — 长推理通常作为单个段落到达，
    /// 没有中间换行符，门控会导致用户长时间看不到任何内容。
    pub fn start_thinking(&mut self, index: usize, width: Option<usize>) {
        self.ensure_capacity(index);
        self.blocks[index] = Some(BlockState {
            line_buffer: LineBuffer::new(),
            bypass_gate: true,
            collector: MarkdownStreamCollector::new(width, true),
            chunker: StreamChunker::new(),
            policy: AdaptiveChunkingPolicy::new(),
        });
        self.is_active = true;
    }

    /// 向块推送内容。路由取决于块类型：
    ///
    /// - 助手文本块：传入字节通常绕过 [`LineBuffer`]
    ///   并在下游拆分为小的显示块。
    /// - 思考块：字节绕过门控并直接进入收集器/分块器，
    ///   以便推理在视觉上保持实时（长思考通常没有中间换行符）。
    ///
    /// `accumulated_text` / `accumulated_thinking` 始终跟踪完整的原始流，
    /// 以便构建 API 消息或进行重试的调用方确切看到模型发出什么，
    /// 不受 UI 门控的影响。
    pub fn push_content(&mut self, index: usize, content: &str) {
        if let Some(Some(block)) = self.blocks.get_mut(index) {
            // 始终先更新原始累加器 — UI 门控不得
            // 影响我们在重试/继续时发送回模型的内容。
            if block.collector.is_thinking {
                self.accumulated_thinking.push_str(content);
            } else {
                self.accumulated_text.push_str(content);
            }

            // 确定此推送中哪些字节可以安全地暴露给下游。
            let downstream: String = if block.bypass_gate {
                // 思考：逐字转发给收集器 + 分块器。
                content.to_string()
            } else {
                // 助手文本：在最后一个换行符边界处门控。
                block.line_buffer.push(content);
                block.line_buffer.take_committable()
            };

            if downstream.is_empty() {
                return;
            }

            if block.bypass_gate {
                block.chunker.push_delta(&downstream);
            } else {
                block.collector.push(&downstream);
                let committed = block.collector.commit_complete_text();
                if !committed.is_empty() {
                    block.chunker.push_delta(&committed);
                }
            }
        }
    }

    /// 从块获取新提交的行。（映射到分块器的旧入口点。）
    pub fn commit_lines(&mut self, index: usize) -> Vec<Line<'static>> {
        let text = self.commit_text(index);
        if text.is_empty() {
            return Vec::new();
        }
        // Re-render the text through the same path the collector used.
        let style = if self
            .blocks
            .get(index)
            .and_then(|b| b.as_ref())
            .is_some_and(|b| b.collector.is_thinking)
        {
            Style::default()
                .fg(palette::STATUS_WARNING)
                .add_modifier(Modifier::DIM | Modifier::ITALIC)
        } else {
            Style::default()
        };
        let mut lines = Vec::new();
        for line in text.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), style)));
        }
        if text.ends_with('\n') {
            lines.push(Line::from(""));
        }
        lines
    }

    /// 运行一个分块器策略的提交节拍，并返回此节拍上安全刷新到转录本的任何文本。
    /// 可能为空（空队列上的 Smooth 模式节拍）或包含从一行到整个积压的内容
    ///（CatchUp 模式突发排空）。
    pub fn commit_text(&mut self, index: usize) -> String {
        if let Some(Some(block)) = self.blocks.get_mut(index) {
            let now = Instant::now();
            let out = run_commit_tick(&mut block.policy, &mut block.chunker, now);
            out.committed_text
        } else {
            String::new()
        }
    }

    /// 检查块当前的分块模式（测试/可观测性）。
    pub fn chunking_mode(&self, index: usize) -> Option<ChunkingMode> {
        self.blocks
            .get(index)
            .and_then(|b| b.as_ref())
            .map(|b| b.policy.mode())
    }

    /// 分块器是否有排队等待下一个提交节拍刷新的内容。
    /// 对于希望在 Smooth 模式节奏下队列排空时驱动额外节拍的调用方很有用。
    pub fn has_pending_chunker_lines(&self, index: usize) -> bool {
        self.blocks
            .get(index)
            .and_then(|b| b.as_ref())
            .is_some_and(|b| b.chunker.queued_lines() > 0)
    }

    /// 结束一个块并获取剩余行
    pub fn finalize_block(&mut self, index: usize) -> Vec<Line<'static>> {
        let text = self.finalize_block_text(index);
        if text.is_empty() {
            return Vec::new();
        }
        let style = if self
            .blocks
            .get(index)
            .and_then(|b| b.as_ref())
            .is_some_and(|b| b.collector.is_thinking)
        {
            Style::default()
                .fg(palette::STATUS_WARNING)
                .add_modifier(Modifier::DIM | Modifier::ITALIC)
        } else {
            Style::default()
        };
        let mut lines = Vec::new();
        for line in text.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), style)));
        }
        if text.ends_with('\n') {
            lines.push(Line::from(""));
        }
        lines
    }

    /// Finalize a block and get remaining raw text. Drains the full pipeline
    /// in upstream-to-downstream order:
    ///
    /// 1. [`LineBuffer::flush`] returns any post-newline tail held by the gate.
    ///    For gated blocks this is critical — without it, a final partial
    ///    line (e.g. text the model emitted without a trailing newline before
    ///    the turn ended) would otherwise be stranded in the gate.
    /// 2. The collector's `finalize_text` releases any partial line it still
    ///    holds (relevant for the bypass path where the collector receives
    ///    raw deltas directly).
    /// 3. The chunker's `drain_remaining` releases queued whole-line text
    ///    that the policy hadn't yet committed.
    pub fn finalize_block_text(&mut self, index: usize) -> String {
        if let Some(Some(block)) = self.blocks.get_mut(index) {
            // Flush the gate first so any held tail rejoins the stream
            // before the collector/chunker drain. For thinking blocks the
            // gate is unused, so this is a no-op.
            let gate_tail = block.line_buffer.flush();
            if !gate_tail.is_empty() {
                block.collector.push(&gate_tail);
            }
            // Any newly committable text after the gate flush feeds the
            // chunker so drain order remains "queued-lines, then partial-tail".
            let post_flush = block.collector.commit_complete_text();
            if !post_flush.is_empty() {
                block.chunker.push_delta(&post_flush);
            }
            // Any unterminated tail still in the collector is returned raw.
            let tail = block.collector.finalize_text();
            // Any whole-line text held by the chunker is safe to emit now.
            let mut out = block.chunker.drain_remaining();
            if !tail.is_empty() {
                out.push_str(&tail);
            }
            self.check_active();
            out
        } else {
            String::new()
        }
    }

    /// Finalize all blocks
    pub fn finalize_all(&mut self) -> Vec<(usize, Vec<Line<'static>>)> {
        let mut result = Vec::new();
        let len = self.blocks.len();
        for i in 0..len {
            let lines = self.finalize_block(i);
            if !lines.is_empty() {
                result.push((i, lines));
            }
        }
        self.is_active = false;
        result
    }

    /// Propagate the low-motion flag to every block's chunking policy.
    /// When true, all policies stay in `Smooth` regardless of queue pressure,
    /// preventing CatchUp burst drains that would create sudden visual jumps.
    pub fn set_low_motion(&mut self, low_motion: bool) {
        for block in self.blocks.iter_mut().flatten() {
            block.policy.set_low_motion(low_motion);
        }
    }

    /// Check if any stream is still active
    fn check_active(&mut self) {
        self.is_active = self.blocks.iter().any(|b| {
            b.as_ref()
                .is_some_and(|state| state.collector.is_streaming())
        });
    }

    /// Ensure capacity for the given index
    fn ensure_capacity(&mut self, index: usize) {
        while self.blocks.len() <= index {
            self.blocks.push(None);
        }
    }

    /// Reset the streaming state
    pub fn reset(&mut self) {
        self.blocks.clear();
        self.is_active = false;
        self.accumulated_text.clear();
        self.accumulated_thinking.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_complete_lines() {
        let mut collector = MarkdownStreamCollector::new(Some(80), false);

        // Push incomplete line
        collector.push("Hello ");
        let lines = collector.commit_complete_lines();
        assert!(lines.is_empty()); // No complete lines yet

        // Complete the line
        collector.push("World\n");
        let lines = collector.commit_complete_lines();
        assert_eq!(lines.len(), 2); // "Hello World" + empty line from trailing \n

        // Push more content
        collector.push("Second line");
        let lines = collector.commit_complete_lines();
        assert!(lines.is_empty()); // No new complete lines

        // Finalize
        let lines = collector.finalize();
        assert_eq!(lines.len(), 1); // "Second line"
    }

    #[test]
    fn test_wrap_line() {
        let result = wrap_line("This is a long line that should be wrapped", 20);
        assert!(result.len() > 1);
    }

    #[test]
    fn assistant_text_streams_before_newline() {
        let mut state = StreamingState::new();
        state.start_text(0, None);
        state.push_content(0, "hello world");

        assert_eq!(state.commit_text(0), "hello world");
        assert!(!state.has_pending_chunker_lines(0));
    }

    #[test]
    fn thinking_text_streams_before_newline() {
        let mut state = StreamingState::new();
        state.start_thinking(0, None);
        state.push_content(0, "thinking deeply");

        assert_eq!(state.commit_text(0), "thinking deeply");
        assert!(!state.has_pending_chunker_lines(0));
    }

    #[test]
    fn finalize_preserves_uncommitted_micro_chunks() {
        let mut state = StreamingState::new();
        state.start_text(0, None);
        state.set_low_motion(true);
        state.push_content(0, "abc");
        assert_eq!(state.commit_text(0), "a");

        assert_eq!(state.finalize_block_text(0), "bc");
    }
}
