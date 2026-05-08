//! 进程级重试状态界面（#499）。
//!
//! `client::send_with_retry` 中的 HTTP 重试路径已经计时其等待
//! 并知道错误类别。本模块为 TUI 提供观察该状态的方式 —
//! `start`、`succeeded` 和 `failed` 翻转全局 `RetryState`，
//! 底部/状态面板每帧读取该状态。
//!
//! 为什么是进程级全局：面向用户的 TUI 每个进程运行一个引擎，
//! 我们想要呈现的唯一重试状态是用户正在关注的那个。
//! 后台任务中的子代理重试有意**不**点亮前台横幅 —
//! 它们应该是不可见的。如果未来的功能需要每引擎重试显示，
//! 将此替换为 `EngineHandle` 上携带的 `Arc<RwLock<...>>`；
//! 公共 API 保持不变。

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 一个正在进行的重试尝试。`deadline` 是下一个请求将触发
/// 的挂钟时间 — UI 从中减去 `Instant::now()` 以渲染实时倒计时。
#[derive(Debug, Clone)]
pub struct RetryBanner {
    /// 从 1 开始索引的重试尝试编号（第一次重试为尝试 1）。
    pub attempt: u32,
    /// 下一个请求将发送的时间。
    pub deadline: Instant,
    /// 简短的可读原因（"rate limited"、"server error"等）。
    pub reason: String,
}

/// 供 UI 渲染的重试界面快照。
#[derive(Debug, Clone, Default)]
pub enum RetryState {
    /// 没有正在进行的重试。横幅隐藏。
    #[default]
    Idle,
    /// 请求在重试前等待。显示倒计时横幅。
    Active(RetryBanner),
    /// 所有重试已耗尽；显示失败行直到下一轮次开始。
    /// `since` 记录行设置的时间，以便未来的优化可以自动过期；
    /// 目前引擎在 `TurnStarted` 时清除它。
    Failed {
        reason: String,
        #[allow(dead_code)]
        since: Instant,
    },
}

impl RetryState {
    /// Wall-clock seconds remaining on the active banner, or `None` if
    /// not active. Saturates at zero — the renderer should treat any
    /// negative remaining as "firing now".
    #[must_use]
    pub fn seconds_remaining(&self) -> Option<u64> {
        match self {
            Self::Active(banner) => Some(
                banner
                    .deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs(),
            ),
            _ => None,
        }
    }

    /// Whether the failure row should still be shown. Mirrors the
    /// "until next turn" rule in the issue spec; the engine clears it
    /// explicitly via [`clear`] on `TurnStarted`.
    #[cfg(test)]
    #[must_use]
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// Lazy-init the cell on first read so callers don't have to initialize
/// process-wide state at boot.
fn cell() -> &'static Mutex<RetryState> {
    static STATE: OnceLock<Mutex<RetryState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(RetryState::Idle))
}

/// Public read snapshot for renderers.
#[must_use]
pub fn snapshot() -> RetryState {
    cell().lock().map(|s| s.clone()).unwrap_or(RetryState::Idle)
}

/// Mark an in-flight retry. `attempt` is the number of the *upcoming*
/// retry (1 for the first); `delay` is how long the client will sleep
/// before firing.
pub fn start(attempt: u32, delay: Duration, reason: impl Into<String>) {
    let banner = RetryBanner {
        attempt,
        deadline: Instant::now() + delay,
        reason: reason.into(),
    };
    if let Ok(mut s) = cell().lock() {
        *s = RetryState::Active(banner);
    }
}

/// Mark the retry chain as having succeeded. Hides the banner.
pub fn succeeded() {
    if let Ok(mut s) = cell().lock() {
        *s = RetryState::Idle;
    }
}

/// Mark the retry chain as having exhausted retries. The renderer keeps
/// the failure row until [`clear`] (typically called on `TurnStarted`).
pub fn failed(reason: impl Into<String>) {
    if let Ok(mut s) = cell().lock() {
        *s = RetryState::Failed {
            reason: reason.into(),
            since: Instant::now(),
        };
    }
}

/// Reset to idle. Called on `TurnStarted` so the previous turn's
/// failure row doesn't bleed into the next turn.
pub fn clear() {
    if let Ok(mut s) = cell().lock() {
        *s = RetryState::Idle;
    }
}

/// Test helper: serialize tests that touch the global state so cargo's
/// parallel runner can't observe a torn read. The guard is exported so
/// tests in *other* modules (e.g. footer rendering tests) can hold the
/// same lock as the ones in `retry_status::tests`.
#[cfg(test)]
pub fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static GUARD: Mutex<()> = Mutex::new(());
    GUARD.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Acquire the cross-module test guard from [`super::test_guard`] and
    /// reset state to `Idle` before yielding to the test body.
    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let g = test_guard();
        clear();
        g
    }

    #[test]
    fn idle_by_default_after_clear() {
        let _g = setup();
        assert!(matches!(snapshot(), RetryState::Idle));
        assert_eq!(snapshot().seconds_remaining(), None);
    }

    #[test]
    fn start_then_succeeded_returns_to_idle() {
        let _g = setup();
        start(1, Duration::from_secs(5), "rate limited");
        let s = snapshot();
        assert!(matches!(s, RetryState::Active(_)));
        let remaining = s.seconds_remaining().unwrap();
        assert!(remaining <= 5, "{remaining}");
        succeeded();
        assert!(matches!(snapshot(), RetryState::Idle));
    }

    #[test]
    fn failed_persists_until_clear() {
        let _g = setup();
        failed("upstream 500");
        let s = snapshot();
        assert!(s.is_failed());
        if let RetryState::Failed { reason, .. } = s {
            assert_eq!(reason, "upstream 500");
        } else {
            panic!("expected Failed");
        }
        clear();
        assert!(matches!(snapshot(), RetryState::Idle));
    }

    #[test]
    fn deadline_in_past_yields_zero_remaining() {
        let _g = setup();
        // Bypass `start` so we can plant a deadline already in the past.
        if let Ok(mut s) = cell().lock() {
            *s = RetryState::Active(RetryBanner {
                attempt: 2,
                deadline: Instant::now() - Duration::from_secs(1),
                reason: "test".into(),
            });
        }
        assert_eq!(snapshot().seconds_remaining(), Some(0));
        clear();
    }
}
