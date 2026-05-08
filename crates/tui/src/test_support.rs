//! 共享的仅测试辅助工具。

use std::sync::{Mutex, MutexGuard, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 获取进程级环境变量互斥锁。
///
/// 如果之前的测试在持有锁时 panic，则恢复守卫，
/// 而不是让故障级联到不相关的测试。
pub(crate) fn lock_test_env() -> MutexGuard<'static, ()> {
    match env_lock().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// 查找两个字符串之间第一个差异的字节位置，
/// 返回一个窗口视图（差异周围的 `±32 字节`），
/// 以便缓存前缀稳定性测试显示*哪些*字节发生了变化，
/// 而不仅仅是它们发生了变化。当字符串字节完全相同时返回 `None`。
pub(crate) fn first_divergence(a: &str, b: &str) -> Option<(usize, String, String)> {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let max = a_bytes.len().min(b_bytes.len());
    for i in 0..max {
        if a_bytes[i] != b_bytes[i] {
            let lo = i.saturating_sub(32);
            let a_hi = (i + 32).min(a_bytes.len());
            let b_hi = (i + 32).min(b_bytes.len());
            let a_ctx = String::from_utf8_lossy(&a_bytes[lo..a_hi]).into_owned();
            let b_ctx = String::from_utf8_lossy(&b_bytes[lo..b_hi]).into_owned();
            return Some((i, a_ctx, b_ctx));
        }
    }
    if a_bytes.len() != b_bytes.len() {
        return Some((
            max,
            format!("(len={})", a_bytes.len()),
            format!("(len={})", b_bytes.len()),
        ));
    }
    None
}

/// 断言两个字符串字节相同，当不相同时在第一个差异处
/// 以窗口化 diff 的形式 panic。由前缀缓存稳定性测试套件
///（#263, #280）用于固定落入 DeepSeek KV 缓存前缀的构造表面。
#[track_caller]
pub(crate) fn assert_byte_identical(label: &str, a: &str, b: &str) {
    if let Some((pos, a_ctx, b_ctx)) = first_divergence(a, b) {
        panic!(
            "{label}：prompt 构造非确定性——首个差异位于字节 {pos}\n\
             ── 侧 A (±32B) ──\n{a_ctx:?}\n── 侧 B (±32B) ──\n{b_ctx:?}",
        );
    }
}
