//! `/stash` 命令 — 暂存会话状态。
//!
//! 有关磁盘格式和持久化规则，请参见 `crates/tui/src/composer_stash.rs`。
//! 斜杠命令是面向用户的界面；编辑器中的 Ctrl+S 是相应的推送入口点。

use crate::composer_stash;
use crate::tui::app::App;

use super::CommandResult;

/// `/stash` 的顶级调度。子命令：
///
/// * `/stash`        — 等同于 `/stash list`。
/// * `/stash list`   — 显示暂存的草稿，最早的在前。
/// * `/stash pop`    — 将最近暂存的草稿恢复到
///   编辑器中；弹出的条目将从磁盘删除。
/// * `/stash clear`  — 清除整个暂存文件。报告删除了
///   多少条目，以便用户知道删除了什么。
pub fn stash(app: &mut App, arg: Option<&str>) -> CommandResult {
    let sub = arg.map(str::trim).unwrap_or("list").to_ascii_lowercase();
    match sub.as_str() {
        "" | "list" | "ls" | "show" => list(),
        "pop" | "restore" => pop(app),
        "clear" | "wipe" | "drop" => clear(),
        other => CommandResult::error(format!(
            "unknown subcommand `{other}`. Try `/stash list`, `/stash pop`, or `/stash clear`."
        )),
    }
}

fn list() -> CommandResult {
    let entries = composer_stash::load_stash();
    if entries.is_empty() {
        return CommandResult::message(
            "Stash empty. Press Ctrl+S in the composer to park the current draft.",
        );
    }
    let mut out = String::new();
    out.push_str(&format!("{} parked draft(s):\n\n", entries.len()));
    for (idx, entry) in entries.iter().enumerate() {
        let preview = preview_first_line(&entry.text, 80);
        let ts = if entry.ts.is_empty() {
            "(no ts)".to_string()
        } else {
            entry.ts.clone()
        };
        out.push_str(&format!("  {idx}. [{ts}] {preview}\n"));
    }
    out.push_str("\nUse `/stash pop` to restore the most recent draft.");
    CommandResult::message(out)
}

fn clear() -> CommandResult {
    match composer_stash::clear_stash() {
        Ok(0) => CommandResult::message("Stash already empty — nothing to clear."),
        Ok(n) => CommandResult::message(format!("Cleared {n} parked draft(s) from the stash.")),
        Err(err) => CommandResult::error(format!("Failed to clear stash: {err}")),
    }
}

fn pop(app: &mut App) -> CommandResult {
    match composer_stash::pop_stash() {
        Some(entry) => {
            // Replace the current composer contents with the popped
            // draft. We don't merge — replacing is the predictable
            // behaviour and matches the "restore the parked draft"
            // mental model. Mirror the queue-edit pattern for the
            // cursor reset.
            app.input = entry.text.clone();
            app.cursor_position = app.input.len();
            let preview = preview_first_line(&entry.text, 60);
            // Tell the user how many drafts remain so they can plan
            // whether to keep popping or move on. Matches the
            // confirmation pattern used by the queue surface.
            let remaining = composer_stash::load_stash().len();
            let suffix = match remaining {
                0 => " (stash now empty)".to_string(),
                1 => " (1 more parked)".to_string(),
                n => format!(" ({n} more parked)"),
            };
            CommandResult::message(format!("Restored stashed draft: {preview}{suffix}"))
        }
        None => CommandResult::message("Stash empty — nothing to pop."),
    }
}

/// 获取 `text` 的单行预览，限制在 `max_chars` 字符。
/// 多行草稿将获得单行摘要，使列表保持可扫描性。
fn preview_first_line(text: &str, max_chars: usize) -> String {
    let head = text.lines().next().unwrap_or("").trim();
    if head.chars().count() <= max_chars {
        return head.to_string();
    }
    let mut out: String = head.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_first_line_truncates_to_cap() {
        let body = "x".repeat(200);
        let p = preview_first_line(&body, 10);
        assert_eq!(p.chars().count(), 10);
        assert!(p.ends_with('…'));
    }

    #[test]
    fn preview_first_line_keeps_short_input_intact() {
        assert_eq!(preview_first_line("short", 50), "short");
    }

    #[test]
    fn preview_first_line_only_uses_first_line_of_multiline() {
        let body = "first line of the draft\nsecond line that's longer\nthird";
        assert_eq!(preview_first_line(body, 80), "first line of the draft");
    }

    #[test]
    fn preview_first_line_handles_empty_input() {
        assert_eq!(preview_first_line("", 50), "");
        assert_eq!(preview_first_line("   ", 50), "");
    }
}
