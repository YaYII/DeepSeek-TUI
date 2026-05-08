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
//! 完整原理请参见任务简报中的 `cx5_chx5_newline_gate.md`。

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
        // 排空到最后一个换行符为止的所有内容。剩余的尾部（换行符之后）
        // 保留在 `pending` 中，并在下次提交决策前与下一次 `push` 的内容拼接。
        self.pending.drain(..=last_nl).collect()
    }

    /// 返回缓冲区中剩余的所有内容，即使没有换行符终止。
    /// 在流结束时使用，这样我们就不会遗漏最后的不完整行。
    pub fn flush(&mut self) -> String {
        std::mem::take(&mut self.pending)
    }

    /// 缓冲区是否持有任何未提交的文本。
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// 待处理尾部的字节长度（测试/可观测性）。
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// 重置缓冲区（例如在流重新启动时）。
    pub fn reset(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_without_newline_holds_everything() {
        // 基石不变性：在没有换行符终止行之前，任何内容都不会逃逸门控。
        // 这保护了部分代码围栏（例如 ``` 在块 N 中到达，语言标签在块 N+1 中）。
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
        // 尾部保留给下一次调用。
        assert_eq!(buf.pending_len(), 2);
        assert!(!buf.is_empty());
    }

    #[test]
    fn next_push_is_concatenated_with_held_tail() {
        let mut buf = LineBuffer::new();
        buf.push("hello\nwo");
        assert_eq!(buf.take_committable(), "hello\n");
        // 保留的 "wo" 与 "rld\n" 拼接，整行变为可提交。
        buf.push("rld\n");
        assert_eq!(buf.take_committable(), "world\n");
        assert!(buf.is_empty());
    }

    #[test]
    fn flush_returns_unterminated_tail() {
        let mut buf = LineBuffer::new();
        buf.push("trailing without newline");
        // 没有换行符 → 无可提交内容。
        assert_eq!(buf.take_committable(), "");
        // 流结束时 flush 原样返回。
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
        // 一次 push 中有多个换行符：直到最后一个换行符的整个前缀
        // 可以一次性提交；只有未终止的尾部被保留。
        let mut buf = LineBuffer::new();
        buf.push("a\nb\nc\nd");
        assert_eq!(buf.take_committable(), "a\nb\nc\n");
        assert_eq!(buf.pending_len(), 1);
        // 用换行符结束 "d" 后，下一次 take 会将其释放。
        buf.push("\n");
        assert_eq!(buf.take_committable(), "d\n");
    }

    #[test]
    fn partial_code_fence_never_escapes_the_gate() {
        // 来自 CX#5 的验收场景：代码围栏的起始标记在多个数据块中到达时，
        // 绝不能暴露没有终止换行符的 "foo```rust"。
        // 我们断言在每次中间提交中，*已提交*的文本要么包含换行符，要么为空
        // ——即语言标签前的部分围栏永远不会泄漏。
        let mut buf = LineBuffer::new();

        // 块 1：以代码围栏起始符结尾的段落片段。
        buf.push("foo```");
        let c1 = buf.take_committable();
        assert!(
            c1.is_empty() || c1.ends_with('\n'),
            "部分围栏泄漏了：{c1:?}"
        );
        assert!(
            !c1.contains("foo```"),
            "围栏起始符在无换行符时逃逸了：{c1:?}"
        );

        // 块 2：语言标签 + 正文开始。代码围栏行现在由换行符终止，
        // 因此可以提交；换行符后的正文被保留。
        buf.push("rust\nlet x");
        let c2 = buf.take_committable();
        assert!(
            c2.ends_with('\n'),
            "期望以换行符终止的提交：{c2:?}"
        );
        assert_eq!(c2, "foo```rust\n");

        // 块 3：正文其余部分和代码围栏结束符。
        buf.push("= 1;\n```\n");
        let c3 = buf.take_committable();
        assert_eq!(c3, "let x= 1;\n```\n");
        assert!(buf.is_empty());
    }

    #[test]
    fn empty_push_is_a_noop() {
        let mut buf = LineBuffer::new();
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
