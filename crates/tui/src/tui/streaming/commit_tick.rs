//! 提交节拍调度器 — 根据策略排空流式分块器。
//!
//! 桥接 [`AdaptiveChunkingPolicy`] 和具体的 [`StreamChunker`] 队列。
//! 调用方通过 [`StreamChunker::push_delta`] 提供原始文本数据块，
//! 然后在每个提交节拍调用 [`run_commit_tick`] 获取要刷新到
//! 转录本中的文本。正常模式下会排空自上次节拍以来接收的所有文本，
//! 使显示跟随上游数据块节奏。低动态模式保持旧的单字素滴流以减少视觉变化。
//!
//! 分块器是流式传输的单位 — 每个活动块一个（助手/思考）。
//! 工具输出无缓冲，绕过此路径。

use std::collections::VecDeque;
use std::time::Duration;
use std::time::Instant;

use unicode_segmentation::UnicodeSegmentation;

use super::chunking::AdaptiveChunkingPolicy;
use super::chunking::ChunkingDecision;
use super::chunking::DrainPlan;
use super::chunking::QueueSnapshot;

const GRAPHEMES_PER_MICRO_CHUNK: usize = 1;
/// 缓冲原始流数据块，并以小显示块的形式发出已提交文本。
#[derive(Debug, Default)]
pub struct StreamChunker {
    /// 已接收但尚未拆分为显示块的字节。通常为空；
    /// 保留以便 `drain_remaining` 在需要时有无损来源可用，
    /// 如果我们决定为未来的 markdown 敏感模式保留尾部的话。
    pending: String,
    /// 等待刷新到转录本的小型字素对齐块。
    queue: VecDeque<QueuedChunk>,
}

#[derive(Debug, Clone)]
struct QueuedChunk {
    text: String,
    enqueued_at: Instant,
}

impl StreamChunker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加原始模型数据块。返回是否至少有一个新的显示块已入队。
    pub fn push_delta(&mut self, delta: &str) -> bool {
        if delta.is_empty() {
            return false;
        }
        self.pending.push_str(delta);

        let now = Instant::now();
        let committed = std::mem::take(&mut self.pending);
        let mut produced = false;
        for chunk in split_into_micro_chunks(&committed) {
            if chunk.is_empty() {
                continue;
            }
            self.queue.push_back(QueuedChunk {
                text: chunk,
                enqueued_at: now,
            });
            produced = true;
        }
        produced
    }

    /// 当前排队等待提交的显示块数量。
    pub fn queued_lines(&self) -> usize {
        self.queue.len()
    }

    /// 最旧的排队块的年龄（如果有）。
    pub fn oldest_queued_age(&self, now: Instant) -> Option<Duration> {
        self.queue
            .front()
            .map(|q| now.saturating_duration_since(q.enqueued_at))
    }

    /// 队列是否为空且没有缓冲的部分行残留。
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.pending.is_empty()
    }

    /// 策略决策的快照。
    pub fn snapshot(&self, now: Instant) -> QueueSnapshot {
        QueueSnapshot {
            queued_lines: self.queue.len(),
            oldest_age: self.oldest_queued_age(now),
        }
    }

    /// 排空 `max_lines` 个排队块并将它们作为拼接文本返回。
    pub fn drain_lines(&mut self, max_lines: usize) -> String {
        let n = max_lines.min(self.queue.len());
        let mut out = String::new();
        for queued in self.queue.drain(..n) {
            out.push_str(&queued.text);
        }
        out
    }

    /// 排空任何剩余的待处理字节（在流结束时调用）。
    /// 这包括排队的完整行和尾部的不完整行。
    pub fn drain_remaining(&mut self) -> String {
        let mut out = String::new();
        while let Some(q) = self.queue.pop_front() {
            out.push_str(&q.text);
        }
        if !self.pending.is_empty() {
            out.push_str(&self.pending);
            self.pending.clear();
        }
        out
    }

    /// 重置内部状态。
    pub fn reset(&mut self) {
        self.pending.clear();
        self.queue.clear();
    }
}

/// 一个提交节拍决策加上此节拍应刷新的文本。
pub struct CommitTickOutput {
    pub committed_text: String,
    pub decision: ChunkingDecision,
    pub is_idle: bool,
}

/// 运行单个提交节拍：询问策略，相应地排空分块器。
pub fn run_commit_tick(
    policy: &mut AdaptiveChunkingPolicy,
    chunker: &mut StreamChunker,
    now: Instant,
) -> CommitTickOutput {
    let snapshot = chunker.snapshot(now);
    let prior_mode = policy.mode();
    let decision = policy.decide(snapshot, now);

    if decision.mode != prior_mode {
        tracing::trace!(
            prior_mode = ?prior_mode,
            new_mode = ?decision.mode,
            queued_lines = snapshot.queued_lines,
            oldest_queued_age_ms = snapshot.oldest_age.map(|age| age.as_millis() as u64),
            entered_catch_up = decision.entered_catch_up,
            "stream chunking mode transition"
        );
    }

    let max = match decision.drain_plan {
        DrainPlan::Available => snapshot.queued_lines,
        DrainPlan::Single => 1,
    };

    // 通过分块器排空；Smooth 模式下的空队列产生 ""。
    let committed_text = chunker.drain_lines(max);

    CommitTickOutput {
        committed_text,
        decision,
        is_idle: chunker.is_idle(),
    }
}

/// 将文本拆分为字素对齐的块。换行符强制形成边界，
/// 以便 markdown 布局仍能快速确定，但散文不再需要等待整行才可见。
fn split_into_micro_chunks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut graphemes = 0usize;

    for grapheme in UnicodeSegmentation::graphemes(text, true) {
        current.push_str(grapheme);
        graphemes += 1;

        if grapheme == "\n" || graphemes >= GRAPHEMES_PER_MICRO_CHUNK {
            out.push(std::mem::take(&mut current));
            graphemes = 0;
        }
    }

    if !current.is_empty() {
        out.push(current);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::streaming::chunking::ChunkingMode;

    #[test]
    fn prose_streams_before_newline() {
        let mut chunker = StreamChunker::new();
        let mut policy = AdaptiveChunkingPolicy::new();
        let now = Instant::now();

        chunker.push_delta("hello world");
        let out = run_commit_tick(&mut policy, &mut chunker, now);
        assert_eq!(out.committed_text, "hello world");
        assert!(
            chunker.is_idle(),
            "正常模式应保持上游节奏"
        );

        let out = run_commit_tick(&mut policy, &mut chunker, now + Duration::from_millis(5));
        assert_eq!(out.committed_text, "");
    }

    #[test]
    fn low_motion_keeps_smooth_micro_chunk_pacing() {
        let mut chunker = StreamChunker::new();
        let mut policy = AdaptiveChunkingPolicy::new();
        policy.set_low_motion(true);
        let now = Instant::now();

        chunker.push_delta("hello world");
        let out = run_commit_tick(&mut policy, &mut chunker, now);
        assert_eq!(out.committed_text, "h");
        assert!(!chunker.is_idle(), "低动态模式应保持滴流");

        let out = run_commit_tick(&mut policy, &mut chunker, now + Duration::from_millis(20));
        assert_eq!(out.committed_text, "e");
    }

    #[test]
    fn normal_motion_burst_drains_available_backlog() {
        let mut chunker = StreamChunker::new();
        let mut policy = AdaptiveChunkingPolicy::new();
        let t0 = Instant::now();

        chunker.push_delta("abc");
        let out1 = run_commit_tick(&mut policy, &mut chunker, t0);
        assert_eq!(out1.decision.mode, ChunkingMode::Smooth);
        assert_eq!(out1.committed_text, "abc");
        assert!(out1.is_idle);

        let out2 = run_commit_tick(&mut policy, &mut chunker, t0 + Duration::from_millis(20));
        assert_eq!(out2.committed_text, "");
    }

    #[test]
    fn low_motion_stream_keeps_combining_marks_with_base_letter() {
        let mut chunker = StreamChunker::new();
        let mut policy = AdaptiveChunkingPolicy::new();
        policy.set_low_motion(true);
        let t0 = Instant::now();

        chunker.push_delta("e\u{301}x");
        let out1 = run_commit_tick(&mut policy, &mut chunker, t0);
        assert_eq!(out1.committed_text, "e\u{301}");
        let out2 = run_commit_tick(&mut policy, &mut chunker, t0 + Duration::from_millis(20));
        assert_eq!(out2.committed_text, "x");
    }

    #[test]
    fn large_burst_preserves_upstream_burst_in_normal_motion() {
        // "一次性"到达的大文本突发应以相同节奏显示，
        // 而不是被合成滴流后又在轮次结束时刷新。
        let mut chunker = StreamChunker::new();
        let mut policy = AdaptiveChunkingPolicy::new();
        let now = Instant::now();

        let burst = "abcdefghijklmnopqrstuvwxyz".repeat(8);
        chunker.push_delta(&burst);
        let out = run_commit_tick(&mut policy, &mut chunker, now);
        assert_eq!(out.decision.mode, ChunkingMode::CatchUp);
        assert_eq!(out.committed_text, burst);
        assert!(out.is_idle);
    }

    #[test]
    fn finalize_drains_partial_tail() {
        // 最后可能不完整的行必须由 drain_remaining 刷新。
        let mut chunker = StreamChunker::new();
        chunker.push_delta("done\nno-newline-here");
        let drained = chunker.drain_remaining();
        assert_eq!(drained, "done\nno-newline-here");
        assert!(chunker.is_idle());
    }
}
