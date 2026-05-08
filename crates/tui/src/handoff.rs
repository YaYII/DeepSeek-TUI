// Used by the deferred context-limit handoff feature (#667). The implementation
// path is staged but not yet wired from the engine; suppress dead-code warnings
// rather than delete the table until the follow-up feature consumes it.
#[allow(dead_code)]
pub const THRESHOLDS: [(f32, &str); 3] = [
    (
        0.9,
        "上下文已达 90%：停止并立即将交接写入 .deepseek/handoff.md",
    ),
    (0.8, "上下文已达 80%：将交接草稿写入 .deepseek/handoff.md"),
    (0.7, "上下文已达 70%：考虑结束当前子任务"),
];
#[allow(dead_code)]
pub fn threshold_message(ratio: f32) -> Option<&'static str> {
    THRESHOLDS
        .iter()
        .find(|(t, _)| ratio >= *t)
        .map(|(_, m)| *m)
}
