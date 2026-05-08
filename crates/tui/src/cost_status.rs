//! 进程级成本累积侧通道（#526）。
//!
//! 主轮次完成路径之外的背景 LLM 调用（压缩摘要、接缝重新压缩、
//! 周期简报）以前会丢弃其令牌使用量 — 仪表盘的会话成本
//! 只看到父轮次的令牌，因此触发压缩或周期重启的长会话
//! 会低估这些后台调用消耗的令牌数。
//!
//! 镜像 [`crate::retry_status`] 模式：后台调用者在每次
//! `client.create_message` 后调用 [`report`]，TUI 渲染循环
//! 每帧调用 [`drain`]，任何排出的金额被归入
//! `App::accrue_subagent_cost_estimate`。
//!
//! 为什么是侧通道而不是插接回调：泄漏的调用者
//!（`compaction.rs`、`seam_manager.rs`、`cycle_manager.rs`）
//! 是引擎内部机制，没有直接访问 `App` 或引擎事件通道的句柄。
//! 侧通道使变更面保持微小 — 每个调用点新增一行 `report` —
//! 任何未来的后台调用者（摘要器、检索助手）无需额外插接即可自动累积。

use std::sync::{Mutex, OnceLock};

use crate::models::Usage;
use crate::pricing::CostEstimate;

static PENDING: OnceLock<Mutex<CostEstimate>> = OnceLock::new();

fn cell() -> &'static Mutex<CostEstimate> {
    PENDING.get_or_init(|| Mutex::new(CostEstimate::default()))
}

/// 后台调用者在此报告其 LLM 使用量。通过
/// [`crate::pricing::calculate_turn_cost_estimate_from_usage`] 计算成本
/// 并将其添加到待处理池中。轻量级；获取短生命周期的锁并返回。
/// 对定价表未知的模型无操作。
pub fn report(model: &str, usage: &Usage) {
    let Some(cost) = crate::pricing::calculate_turn_cost_estimate_from_usage(model, usage) else {
        return;
    };
    if !cost.is_positive() {
        return;
    }
    if let Ok(mut pending) = cell().lock() {
        pending.usd += cost.usd;
        pending.cny += cost.cny;
    }
}

/// 排出待处理成本。返回累积金额并将池重置为零。
/// 由 TUI 渲染/事件循环每帧调用；任何非零结果归入
/// `accrue_subagent_cost_estimate`。
pub fn drain() -> CostEstimate {
    let Ok(mut pending) = cell().lock() else {
        return CostEstimate::default();
    };
    std::mem::take(&mut *pending)
}

/// Reset the pool to zero without consuming. Test-only helper for
/// suites that share the static and need to start from a known
/// state. Production code should always use [`drain`].
#[cfg(test)]
pub fn reset_for_tests() {
    if let Ok(mut pending) = cell().lock() {
        *pending = CostEstimate::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_usage() -> Usage {
        Usage {
            input_tokens: 1_000,
            output_tokens: 500,
            ..Default::default()
        }
    }

    /// Tests run in parallel and share the static — serialize the
    /// ones that touch the pool through this mutex so concurrent
    /// `report`/`drain` doesn't make assertions racy.
    fn serial_lock() -> std::sync::MutexGuard<'static, ()> {
        static M: OnceLock<Mutex<()>> = OnceLock::new();
        M.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn report_adds_to_pool_and_drain_returns_then_resets() {
        let _g = serial_lock();
        reset_for_tests();
        report("deepseek-v4-flash", &small_usage());
        let first = drain();
        assert!(first.usd > 0.0, "expected positive USD cost, got {first:?}");
        assert!(first.cny > 0.0, "expected positive CNY cost, got {first:?}");
        let second = drain();
        assert_eq!(second, CostEstimate::default(), "drain must zero the pool");
    }

    #[test]
    fn report_skips_unknown_models() {
        let _g = serial_lock();
        reset_for_tests();
        // NIM-hosted models intentionally have no DeepSeek pricing.
        report("deepseek-ai/deepseek-v4-pro", &small_usage());
        assert_eq!(drain(), CostEstimate::default());
    }

    #[test]
    fn report_accumulates_across_multiple_calls() {
        let _g = serial_lock();
        reset_for_tests();
        report("deepseek-v4-flash", &small_usage());
        report("deepseek-v4-flash", &small_usage());
        let total = drain();
        // Two equal reports — total must be 2× a single report.
        let single = crate::pricing::calculate_turn_cost_estimate_from_usage(
            "deepseek-v4-flash",
            &small_usage(),
        )
        .unwrap();
        assert!((total.usd - 2.0 * single.usd).abs() < 1e-12);
        assert!((total.cny - 2.0 * single.cny).abs() < 1e-12);
    }
}
