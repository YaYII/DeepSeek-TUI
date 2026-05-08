//! 差异格式 — 统一 diff 格式化和解析。
//!
//! `edit_file` 和 `write_file` 捕获变更前后的文件内容，并在其 `ToolResult`
//! 输出的头部发出统一差异。TUI 的 `output_looks_like_diff` 检测器然后将负载
//! 路由到 `diff_render::render_diff`，后者使用行号和彩色 `+`/`-` 边线渲染差异（#505）。
//!
//! 差异对于模型来说也是一项严格的 UX 升级——它确切地看到哪些行发生了变化，
//! 而不是一行摘要。

use similar::TextDiff;

/// 构建 `old` 和 `new` 之间的统一差异，以 `path` 为键。
///
/// 当输入字节相同时返回空字符串，以便调用者可跳过"无变更"标头。
/// 输出使用 git 风格的 `--- a/...` / `+++ b/...` 标头和三行上下文——
/// 匹配 TUI 的 `diff_render::render_diff` 已经理解的格式。
#[must_use]
pub fn make_unified_diff(path: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let a = format!("a/{path}");
    let b = format!("b/{path}");
    let diff = TextDiff::from_lines(old, new);
    diff.unified_diff()
        .context_radius(3)
        .header(&a, &b)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_inputs_emit_empty_diff() {
        let s = "hello\nworld\n";
        assert!(make_unified_diff("foo.txt", s, s).is_empty());
    }

    #[test]
    fn replacement_emits_minus_plus_pair() {
        let old = "alpha\nbeta\ngamma\n";
        let new = "alpha\nBETA\ngamma\n";
        let diff = make_unified_diff("foo.txt", old, new);
        assert!(diff.contains("--- a/foo.txt"), "{diff}");
        assert!(diff.contains("+++ b/foo.txt"), "{diff}");
        assert!(diff.contains("-beta"), "{diff}");
        assert!(diff.contains("+BETA"), "{diff}");
    }

    #[test]
    fn new_file_renders_against_empty_old() {
        let new = "first line\nsecond line\n";
        let diff = make_unified_diff("new.txt", "", new);
        assert!(diff.contains("--- a/new.txt"), "{diff}");
        assert!(diff.contains("+++ b/new.txt"), "{diff}");
        assert!(diff.contains("+first line"), "{diff}");
        assert!(diff.contains("+second line"), "{diff}");
    }

    #[test]
    fn diff_contains_hunk_header_so_tui_renders_it() {
        // The TUI detector scans the first 5 lines for `@@`. Make sure the
        // unified diff puts a hunk header within that window so the
        // diff-aware renderer kicks in (#505).
        let diff = make_unified_diff("foo.txt", "a\n", "b\n");
        let head: Vec<&str> = diff.lines().take(5).collect();
        assert!(
            head.iter().any(|line| line.starts_with("@@")),
            "expected hunk header in first 5 lines; got {head:?}"
        );
    }
}
