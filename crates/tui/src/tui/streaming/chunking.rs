//! 将流式文本分割为可渲染的块。
//!
//! 从 `codex-rs/tui/src/streaming/chunking.rs` 移植，针对 deepseek-tui 的
//! 基于文本的流式管道进行了适配。策略是队列压力驱动的，且与来源无关。
//!
//! # 思维模型
//!
//! 两个档位：
//! - [`ChunkingMode::Smooth`]：正常压力。
//! - [`ChunkingMode::CatchUp`]：压力升高。
//!
//! 正常模式的调用方排空所有当前可用的块，使显示跟随上游 SSE 数据块节奏。
//! 低动态模式的调用方保持在 Smooth 模式，每节拍排空一个块以减少视觉变化。
//!
//! # 滞后
//!
//! - 当 `queued_lines >= ENTER_QUEUE_DEPTH_LINES` 或最旧的排队块
//!   至少达到 [`ENTER_OLDEST_AGE`] 时，进入 `CatchUp`。
//! - 仅在压力保持在 [`EXIT_QUEUE_DEPTH_LINES`] 和 [`EXIT_OLDEST_AGE`]
//!   以下至少 [`EXIT_HOLD`] 时间后，才退出 `CatchUp`。
//! - 退出后，在 [`REENTER_CATCH_UP_HOLD`] 内抑制立即重新进入，
//!   除非积压"严重"（队列 >= [`SEVERE_QUEUE_DEPTH_LINES`] 或
//!   最旧块 >= [`SEVERE_OLDEST_AGE`]）。

use std::time::Duration;
use std::time::Instant;

/// Queue-depth threshold that allows entering catch-up mode.
pub(crate) const ENTER_QUEUE_DEPTH_LINES: usize = 160;

/// Oldest-chunk age threshold that allows entering catch-up mode.
pub(crate) const ENTER_OLDEST_AGE: Duration = Duration::from_millis(1_200);

/// Queue-depth threshold used when evaluating catch-up exit hysteresis.
pub(crate) const EXIT_QUEUE_DEPTH_LINES: usize = 32;

/// Oldest-chunk age threshold used when evaluating catch-up exit hysteresis.
pub(crate) const EXIT_OLDEST_AGE: Duration = Duration::from_millis(300);

/// Minimum duration queue pressure must stay below exit thresholds to leave catch-up mode.
pub(crate) const EXIT_HOLD: Duration = Duration::from_millis(250);

/// Cooldown window after a catch-up exit that suppresses immediate re-entry.
pub(crate) const REENTER_CATCH_UP_HOLD: Duration = Duration::from_millis(250);

/// Queue-depth cutoff that marks backlog as severe (bypasses re-entry hold).
pub(crate) const SEVERE_QUEUE_DEPTH_LINES: usize = 640;

/// Oldest-line age cutoff that marks backlog as severe.
pub(crate) const SEVERE_OLDEST_AGE: Duration = Duration::from_millis(4_000);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChunkingMode {
    /// 每个基准提交节拍排空一个显示块。
    #[default]
    Smooth,
    /// 根据队列压力排空排队积压。
    CatchUp,
}

/// 捕获自适应分块决策所使用的队列压力输入。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueSnapshot {
    /// 等待显示的排队流块数量。
    pub queued_lines: usize,
    /// 决策时最旧排队块的年龄。
    pub oldest_age: Option<Duration>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrainPlan {
    /// 在此节拍发出所有可用的排队块。
    Available,
    /// 精确发出一个排队行。
    Single,
}

/// 表示针对特定队列快照的一个策略决策。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkingDecision {
    /// 应用滞后转换后的模式。
    pub mode: ChunkingMode,
    /// 此决策是否从 `Smooth` 转换为 `CatchUp`。
    pub entered_catch_up: bool,
    /// 当前提交节拍要执行的排空计划。
    pub drain_plan: DrainPlan,
}

/// 跨节拍维护自适应分块模式和滞后状态。
#[derive(Debug, Default, Clone)]
pub struct AdaptiveChunkingPolicy {
    mode: ChunkingMode,
    below_exit_threshold_since: Option<Instant>,
    last_catch_up_exit_at: Option<Instant>,
    /// 为 true 时，策略从不进入 `CatchUp` — 无论队列压力如何都保持在 `Smooth`，
    /// 为偏好减少视觉变化的用户保持显示平静。
    low_motion: bool,
}

impl AdaptiveChunkingPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回最近决策所使用的策略模式。
    pub fn mode(&self) -> ChunkingMode {
        self.mode
    }

    /// 将状态重置为基准平滑模式。
    pub fn reset(&mut self) {
        self.mode = ChunkingMode::Smooth;
        self.below_exit_threshold_since = None;
        self.last_catch_up_exit_at = None;
    }

    /// 为 true 时，策略从不进入 `CatchUp` — 无论队列压力如何都保持在 `Smooth`。
    pub fn set_low_motion(&mut self, low_motion: bool) {
        self.low_motion = low_motion;
        if low_motion {
            self.mode = ChunkingMode::Smooth;
            self.below_exit_threshold_since = None;
            self.last_catch_up_exit_at = None;
        }
    }

    /// 根据当前队列快照计算排空决策。
    pub fn decide(&mut self, snapshot: QueueSnapshot, now: Instant) -> ChunkingDecision {
        // 低动态模式下，无论队列压力如何都使用 Smooth 节奏
        // — 用户要求平静、稳定的显示。
        if self.low_motion {
            self.mode = ChunkingMode::Smooth;
            self.below_exit_threshold_since = None;
            return ChunkingDecision {
                mode: self.mode,
                entered_catch_up: false,
                drain_plan: DrainPlan::Single,
            };
        }

        if snapshot.queued_lines == 0 {
            self.note_catch_up_exit(now);
            self.mode = ChunkingMode::Smooth;
            self.below_exit_threshold_since = None;
            return ChunkingDecision {
                mode: self.mode,
                entered_catch_up: false,
                drain_plan: DrainPlan::Available,
            };
        }

        let entered_catch_up = match self.mode {
            ChunkingMode::Smooth => self.maybe_enter_catch_up(snapshot, now),
            ChunkingMode::CatchUp => {
                self.maybe_exit_catch_up(snapshot, now);
                false
            }
        };

        ChunkingDecision {
            mode: self.mode,
            entered_catch_up,
            drain_plan: DrainPlan::Available,
        }
    }

    fn maybe_enter_catch_up(&mut self, snapshot: QueueSnapshot, now: Instant) -> bool {
        if !should_enter_catch_up(snapshot) {
            return false;
        }
        if self.reentry_hold_active(now) && !is_severe_backlog(snapshot) {
            return false;
        }
        self.mode = ChunkingMode::CatchUp;
        self.below_exit_threshold_since = None;
        self.last_catch_up_exit_at = None;
        true
    }

    fn maybe_exit_catch_up(&mut self, snapshot: QueueSnapshot, now: Instant) {
        if !should_exit_catch_up(snapshot) {
            self.below_exit_threshold_since = None;
            return;
        }

        match self.below_exit_threshold_since {
            Some(since) if now.saturating_duration_since(since) >= EXIT_HOLD => {
                self.mode = ChunkingMode::Smooth;
                self.below_exit_threshold_since = None;
                self.last_catch_up_exit_at = Some(now);
            }
            Some(_) => {}
            None => {
                self.below_exit_threshold_since = Some(now);
            }
        }
    }

    fn note_catch_up_exit(&mut self, now: Instant) {
        if self.mode == ChunkingMode::CatchUp {
            self.last_catch_up_exit_at = Some(now);
        }
    }

    fn reentry_hold_active(&self, now: Instant) -> bool {
        self.last_catch_up_exit_at
            .is_some_and(|exit| now.saturating_duration_since(exit) < REENTER_CATCH_UP_HOLD)
    }
}

/// 返回当前队列压力是否值得进入 catch-up 模式。
fn should_enter_catch_up(snapshot: QueueSnapshot) -> bool {
    snapshot.queued_lines >= ENTER_QUEUE_DEPTH_LINES
        || snapshot
            .oldest_age
            .is_some_and(|oldest| oldest >= ENTER_OLDEST_AGE)
}

/// 返回队列压力是否足够低以开始退出滞后。
fn should_exit_catch_up(snapshot: QueueSnapshot) -> bool {
    snapshot.queued_lines <= EXIT_QUEUE_DEPTH_LINES
        && snapshot
            .oldest_age
            .is_some_and(|oldest| oldest <= EXIT_OLDEST_AGE)
}

/// 返回积压是否严重到足以绕过重新进入保持。
fn is_severe_backlog(snapshot: QueueSnapshot) -> bool {
    snapshot.queued_lines >= SEVERE_QUEUE_DEPTH_LINES
        || snapshot
            .oldest_age
            .is_some_and(|oldest| oldest >= SEVERE_OLDEST_AGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(queued_lines: usize, oldest_age_ms: u64) -> QueueSnapshot {
        QueueSnapshot {
            queued_lines,
            oldest_age: Some(Duration::from_millis(oldest_age_ms)),
        }
    }

    fn empty_snap() -> QueueSnapshot {
        QueueSnapshot {
            queued_lines: 0,
            oldest_age: None,
        }
    }

    #[test]
    fn smooth_only_burst_drains_available_chunks_in_normal_motion() {
        // 五条缓慢到达的行，每条都远低于进入阈值，永远不会
        // 将策略从 `Smooth` 切换出去。正常模式仍会排空
        // 已有的内容，使显示节奏跟随上游数据块。
        let mut policy = AdaptiveChunkingPolicy::new();
        let t0 = Instant::now();

        for i in 0..5 {
            // 1 个排队行，年龄 10 毫秒 — 远低于 ENTER 阈值。
            let decision = policy.decide(snap(1, 10), t0 + Duration::from_millis(50 * i));
            assert_eq!(decision.mode, ChunkingMode::Smooth);
            assert!(!decision.entered_catch_up);
            assert_eq!(decision.drain_plan, DrainPlan::Available);
        }
    }

    #[test]
    fn deep_burst_flips_to_catch_up_and_drains_backlog() {
        // 跨越 ENTER_QUEUE_DEPTH_LINES 的突发进入 CatchUp。
        // 使用单字素块，阈值保持足够高，使得普通散文在 catch-up 生效前
        // 仍能以可见方式滴入。策略应进入 `CatchUp`，同时正常模式排空
        // 仍保留已到达的上游突发。
        let mut policy = AdaptiveChunkingPolicy::new();
        let now = Instant::now();

        let decision = policy.decide(snap(ENTER_QUEUE_DEPTH_LINES, 10), now);
        assert_eq!(decision.mode, ChunkingMode::CatchUp);
        assert!(decision.entered_catch_up);
        assert_eq!(decision.drain_plan, DrainPlan::Available);

        // 下次节拍请求更大积压：仍为 CatchUp，批次随之增长。
        let larger_backlog = ENTER_QUEUE_DEPTH_LINES + 80;
        let decision = policy.decide(snap(larger_backlog, 30), now + Duration::from_millis(10));
        assert_eq!(decision.mode, ChunkingMode::CatchUp);
        assert!(!decision.entered_catch_up, "没有第二次转换信号");
        assert_eq!(decision.drain_plan, DrainPlan::Available);
    }

    #[test]
    fn age_threshold_alone_triggers_catch_up() {
        // 队列深度很小，但最旧的块已超过年龄阈值。
        // 任一条件都足以进入 catch-up。
        let mut policy = AdaptiveChunkingPolicy::new();
        let now = Instant::now();

        let decision = policy.decide(snap(2, ENTER_OLDEST_AGE.as_millis() as u64), now);
        assert_eq!(decision.mode, ChunkingMode::CatchUp);
        assert!(decision.entered_catch_up);
        assert_eq!(decision.drain_plan, DrainPlan::Available);
    }

    #[test]
    fn catch_up_exits_after_low_activity_hold() {
        // Enter CatchUp via depth burst, then drop pressure below exit
        // thresholds. Policy must hold for >=EXIT_HOLD before returning to Smooth.
        let mut policy = AdaptiveChunkingPolicy::new();
        let t0 = Instant::now();

        let _ = policy.decide(snap(ENTER_QUEUE_DEPTH_LINES, 20), t0);
        assert_eq!(policy.mode(), ChunkingMode::CatchUp);

        // Pressure drops to the exit thresholds.
        // Hold begins; not yet 250ms.
        let pre_hold = policy.decide(
            snap(EXIT_QUEUE_DEPTH_LINES, EXIT_OLDEST_AGE.as_millis() as u64),
            t0 + Duration::from_millis(50),
        );
        assert_eq!(pre_hold.mode, ChunkingMode::CatchUp);

        // Still under hold.
        let mid_hold = policy.decide(
            snap(EXIT_QUEUE_DEPTH_LINES, EXIT_OLDEST_AGE.as_millis() as u64),
            t0 + Duration::from_millis(200),
        );
        assert_eq!(mid_hold.mode, ChunkingMode::CatchUp);

        // Past EXIT_HOLD (250 ms) → return to Smooth.
        let post_hold = policy.decide(
            snap(EXIT_QUEUE_DEPTH_LINES, EXIT_OLDEST_AGE.as_millis() as u64),
            t0 + Duration::from_millis(320),
        );
        assert_eq!(post_hold.mode, ChunkingMode::Smooth);
        assert_eq!(post_hold.drain_plan, DrainPlan::Available);
    }

    #[test]
    fn idle_resets_to_smooth_immediately() {
        // An empty queue forces Smooth regardless of prior mode.
        let mut policy = AdaptiveChunkingPolicy::new();
        let now = Instant::now();

        let _ = policy.decide(snap(ENTER_QUEUE_DEPTH_LINES, 20), now);
        assert_eq!(policy.mode(), ChunkingMode::CatchUp);

        let decision = policy.decide(empty_snap(), now + Duration::from_millis(10));
        assert_eq!(decision.mode, ChunkingMode::Smooth);
        assert_eq!(decision.drain_plan, DrainPlan::Available);
    }

    #[test]
    fn reentry_hold_blocks_immediate_flip_back() {
        // After exiting CatchUp via idle, a threshold-sized burst that arrives within
        // the re-entry hold window should not immediately re-enter CatchUp.
        let mut policy = AdaptiveChunkingPolicy::new();
        let t0 = Instant::now();

        let _ = policy.decide(snap(ENTER_QUEUE_DEPTH_LINES, 20), t0);
        let _ = policy.decide(empty_snap(), t0 + Duration::from_millis(10));

        // Within REENTER_CATCH_UP_HOLD (250 ms): hold blocks re-entry.
        let held = policy.decide(
            snap(ENTER_QUEUE_DEPTH_LINES, 20),
            t0 + Duration::from_millis(100),
        );
        assert_eq!(held.mode, ChunkingMode::Smooth);
        assert_eq!(held.drain_plan, DrainPlan::Available);

        // Past the hold: re-entry permitted.
        let reentered = policy.decide(
            snap(ENTER_QUEUE_DEPTH_LINES, 20),
            t0 + Duration::from_millis(400),
        );
        assert_eq!(reentered.mode, ChunkingMode::CatchUp);
        assert_eq!(reentered.drain_plan, DrainPlan::Available);
    }

    #[test]
    fn severe_backlog_bypasses_reentry_hold() {
        // Even within the hold window, a "severe" backlog bypasses
        // the gate so display lag doesn't unbounded-grow.
        let mut policy = AdaptiveChunkingPolicy::new();
        let t0 = Instant::now();

        let _ = policy.decide(snap(ENTER_QUEUE_DEPTH_LINES, 20), t0);
        let _ = policy.decide(empty_snap(), t0 + Duration::from_millis(10));

        let severe = policy.decide(
            snap(SEVERE_QUEUE_DEPTH_LINES, 20),
            t0 + Duration::from_millis(100),
        );
        assert_eq!(severe.mode, ChunkingMode::CatchUp);
        assert_eq!(severe.drain_plan, DrainPlan::Available);
    }

    #[test]
    fn low_motion_always_smooth_regardless_of_pressure() {
        let mut policy = AdaptiveChunkingPolicy::new();
        policy.set_low_motion(true);
        let t0 = Instant::now();

        // Queue depth far above ENTER threshold.
        let d1 = policy.decide(snap(ENTER_QUEUE_DEPTH_LINES + 80, 10), t0);
        assert_eq!(d1.mode, ChunkingMode::Smooth);
        assert!(!d1.entered_catch_up);
        assert_eq!(d1.drain_plan, DrainPlan::Single);

        // Oldest age far above ENTER threshold.
        let d2 = policy.decide(
            snap(5, ENTER_OLDEST_AGE.as_millis() as u64),
            t0 + Duration::from_millis(100),
        );
        assert_eq!(d2.mode, ChunkingMode::Smooth);
        assert!(!d2.entered_catch_up);
        assert_eq!(d2.drain_plan, DrainPlan::Single);

        // Severe backlog — still Smooth.
        let d3 = policy.decide(
            snap(
                SEVERE_QUEUE_DEPTH_LINES + 80,
                SEVERE_OLDEST_AGE.as_millis() as u64,
            ),
            t0 + Duration::from_millis(200),
        );
        assert_eq!(d3.mode, ChunkingMode::Smooth);
        assert_eq!(d3.drain_plan, DrainPlan::Single);
    }

    #[test]
    fn low_motion_reset_resumes_normal_operation() {
        let mut policy = AdaptiveChunkingPolicy::new();
        policy.set_low_motion(true);
        let t0 = Instant::now();

        // Low motion blocks catch-up.
        let d1 = policy.decide(snap(ENTER_QUEUE_DEPTH_LINES + 80, 10), t0);
        assert_eq!(d1.mode, ChunkingMode::Smooth);

        // Turn off low motion — next burst should enter CatchUp.
        policy.set_low_motion(false);
        let d2 = policy.decide(
            snap(ENTER_QUEUE_DEPTH_LINES + 80, 10),
            t0 + Duration::from_millis(10),
        );
        assert_eq!(d2.mode, ChunkingMode::CatchUp);
        assert!(d2.entered_catch_up);
        assert_eq!(d2.drain_plan, DrainPlan::Available);
    }
}
