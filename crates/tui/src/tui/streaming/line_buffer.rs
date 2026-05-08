//! 行缓冲区 — 管理流式文本行以进行增量渲染。
//!
//! `LineBuffer` 是分块器上游的安全层，它会保留最后一个 `\n` 之后的
//! 所有文本，直到下一个换行符到达。这可以防止部分多字符 markdown —
//! 最重要的是防止部分代码围栏
//!（` ``` `）其含义取决于同一行后续内容——
//! from ever becoming visible state in the renderer.
//!
//! Mental model:
//! - `push(delta)` 将原始流文本追加到内部待处理缓冲区。
//! - `take_committable()` 仅返回到（并包含）最后一个 `\n` 的前缀，
//!   并清除此前缀。最后一个 `\n` 之后的内容会留在缓冲区中等待下一次 push。
//! - `flush()` 返回剩余所有内容，在模型表示轮次结束时使用。
//!   （分块器上游的约定是：只有完整行文本才会被提交；`flush()` 是
//!   当我们知道不会再有文本到达时的显式逃生口。）
//!
//! See `cx5_chx5_newline_gate.md` in the task brief for full rationale.

/// 持有流式文本，直到到达换行符边界。
///
/// 在流式管道中位于 [`StreamChunker`](super::commit_tick::StreamChunker) 上游：
///
/// ```text
/// raw delta -> LineBuffer.push -> take_committable -> StreamChunker.push_delta -> commit tick
/// ```
///
/// 分块器也在其待处理缓冲区上强制实施"排空至最后一个换行符"规则，
/// 但 `LineBuffer` 作为一个*独立*层存在，目的是：
/// 1. 约定清晰且可在本地测试。
/// 2. 未来的下游消费者（例如乐观地渲染排队行的实时预览）
///    不会意外看到部分代码围栏。
/// 3. 轮次结束时的刷新语义由门控层拥有，而非策略层。
#[derive(Debug, Default, Clone)]
pub struct LineBuffer {
    /// 自上次提交以来尚未释放的待处理文本，因为尚未看到终止的 `\n`。
    pending: String,
}

impl LineBuffer {
    /// 创建一个空缓冲区。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加原始数据块。
    pub fn push(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.pending.push_str(delta);
    }

    /// 返回待处理缓冲区中直到（并包含）最后一个 `\n` 的前缀。
    /// 该换行符之后的内容（如果有）仍保留在缓冲区中。
    ///
    /// 当缓冲区为空或尚未包含换行符时返回空字符串——
    /// 调用方可以将空字符串情况视为"本次 push 没有可提交内容"。
    pub fn take_committable(&mut self) -> String {
        let Some(last_nl) = self.pending.rfind('\n') else {
            return String::new();
        };
        // Drain everything up to and including the last newline. The remaining
        // tail (post-newline) stays in `pending` and is concatenated with the
        // next `push` before the next commit decision is made.
        self.pending.drain(..=last_nl).collect()
    }

    /// Return whatever is left in the buffer, even if it is not newline
    /// terminated. Used when the stream ends so we don't strand the final
    /// partial line.
    pub fn flush(&mut self) -> String {
        std::mem::take(&mut self.pending)
    }

    /// Whether the buffer holds any uncommitted text.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Length of the pending tail in bytes (testing/observability).
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Reset the buffer (e.g. on stream restart).
    pub fn reset(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_without_newline_holds_everything() {
        // Cornerstone invariant: nothing escapes the gate until a newline
        // terminates the line. This is what protects partial code fences
        // (e.g. ``` arriving in chunk N, language tag in chunk N+1).
        let mut buf = LineBuffer::new();
        buf.push("hello");
        assert_eq!(buf.take_committable(), "");
        assert_eq!(buf.pending_len(), 5);
        assert!(!buf.is_empty());
    }

    #[test]
    fn push_with_trailing_partial_returns_only_prefix() {
        let mut buf = LineBuffer::new();
        buf.push("hello\nwo");
        assert_eq!(buf.take_committable(), "hello\n");
        // Tail is held for next call.
        assert_eq!(buf.pending_len(), 2);
        assert!(!buf.is_empty());
    }

    #[test]
    fn next_push_is_concatenated_with_held_tail() {
        let mut buf = LineBuffer::new();
        buf.push("hello\nwo");
        assert_eq!(buf.take_committable(), "hello\n");
        // The held "wo" is concatenated with "rld\n", and the whole line
        // becomes committable.
        buf.push("rld\n");
        assert_eq!(buf.take_committable(), "world\n");
        assert!(buf.is_empty());
    }

    #[test]
    fn flush_returns_unterminated_tail() {
        let mut buf = LineBuffer::new();
        buf.push("trailing without newline");
        // No newline → nothing committable.
        assert_eq!(buf.take_committable(), "");
        // End-of-stream flush returns it raw.
        assert_eq!(buf.flush(), "trailing without newline");
        assert!(buf.is_empty());
    }

    #[test]
    fn flush_is_empty_when_buffer_drained() {
        let mut buf = LineBuffer::new();
        buf.push("a\n");
        assert_eq!(buf.take_committable(), "a\n");
        assert_eq!(buf.flush(), "");
    }

    #[test]
    fn multi_line_burst_returns_prefix_through_last_newline() {
        // Multiple newlines in one push: the entire prefix up through the
        // last newline is committable in one go; only the unterminated tail
        // is held.
        let mut buf = LineBuffer::new();
        buf.push("a\nb\nc\nd");
        assert_eq!(buf.take_committable(), "a\nb\nc\n");
        assert_eq!(buf.pending_len(), 1);
        // Finishing "d" with a newline releases it on the next take.
        buf.push("\n");
        assert_eq!(buf.take_committable(), "d\n");
    }

    #[test]
    fn partial_code_fence_never_escapes_the_gate() {
        // Acceptance scenario from CX#5: a fenced code block whose opener
        // arrives split across deltas must never expose "foo```rust" without
        // a terminating newline. We assert that on every intermediate
        // commit, the *committed* text either contains a newline or is empty
        // — i.e. the pre-language partial fence never leaks.
        let mut buf = LineBuffer::new();

        // Chunk 1: a paragraph fragment ending with the fence opener.
        buf.push("foo```");
        let c1 = buf.take_committable();
        assert!(
            c1.is_empty() || c1.ends_with('\n'),
            "partial fence leaked: {c1:?}"
        );
        assert!(
            !c1.contains("foo```"),
            "fence opener escaped without newline: {c1:?}"
        );

        // Chunk 2: language tag + start of body. The fence line is now
        // newline-terminated, so it can commit; the post-newline body is
        // held.
        buf.push("rust\nlet x");
        let c2 = buf.take_committable();
        assert!(
            c2.ends_with('\n'),
            "expected newline-terminated commit: {c2:?}"
        );
        assert_eq!(c2, "foo```rust\n");

        // Chunk 3: rest of body and the fence closer.
        buf.push("= 1;\n```\n");
        let c3 = buf.take_committable();
        assert_eq!(c3, "let x= 1;\n```\n");
        assert!(buf.is_empty());
    }

    #[test]
    fn empty_push_is_a_noop() {
        let mut buf = LineBuffer::new();
        buf.push("");
        assert!(buf.is_empty());
        assert_eq!(buf.take_committable(), "");
    }

    #[test]
    fn reset_clears_pending_tail() {
        let mut buf = LineBuffer::new();
        buf.push("partial");
        assert_eq!(buf.pending_len(), 7);
        buf.reset();
        assert!(buf.is_empty());
        assert_eq!(buf.flush(), "");
    }
}
