//! 跨会话的编辑器输入历史（#366）。
//!
//! 将用户输入的提示持久化到 `~/.deepseek/composer_history.txt`，
//! 以便在编辑器中按上箭头键时不仅回忆当前会话，还能回忆之前会话的提交内容。
//! 每行一条记录，最旧的在前，上限为 [`MAX_HISTORY_ENTRIES`] 条
//!（追加时会修剪较旧的条目）。
//!
//! 以 `/` 开头的条目（斜杠命令）不会被存储 — 它们会污染回忆流，
//! 而且模糊斜杠菜单已经覆盖了它们。空输入/仅空白输入也会被跳过。

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// 持久化历史的硬上限。保持文件较小（典型条目 < 200 字符，
/// 因此 1000 条 ≈ 200 KB）并限制启动加载时间。
pub const MAX_HISTORY_ENTRIES: usize = 1000;

const HISTORY_FILE_NAME: &str = "composer_history.txt";

fn default_history_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".deepseek").join(HISTORY_FILE_NAME))
}

/// 将持久化的历史记录读取到内存中。如果文件不存在或无法解析，
/// 返回空 vec — 这是尽力而为的操作。
#[must_use]
pub fn load_history() -> Vec<String> {
    let Some(path) = default_history_path() else {
        return Vec::new();
    };
    load_history_from(&path)
}

fn load_history_from(path: &Path) -> Vec<String> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .collect()
}

/// 将条目追加到持久化历史记录中，修剪旧条目以保持在
/// [`MAX_HISTORY_ENTRIES`] 以内。斜杠命令和空输入
/// 会被跳过 — 这些对回忆没有帮助。
///
/// 尽力而为 — 失败通过 `tracing` 记录但不传播，
/// 因为编辑器历史是 UX 优化项，不是正确性问题。
pub fn append_history(entry: &str) {
    let Some(path) = default_history_path() else {
        return;
    };
    append_history_to(&path, entry);
}

fn append_history_to(path: &Path, entry: &str) {
    let trimmed = entry.trim();
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return;
    }
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        tracing::warn!(
            "创建编辑器历史目录 {} 失败：{err}",
            parent.display()
        );
        return;
    }

    // 读取现有条目，追加新条目，从前面修剪直到低于上限
    // 直到低于上限，然后原子化重写。
    let mut entries = load_history_from(path);
    if entries.last().map(String::as_str) == Some(trimmed) {
        // 去重连续重复项 — 重复提交相同提示不应使文件膨胀。
        return;
    }
    entries.push(trimmed.to_string());
    if entries.len() > MAX_HISTORY_ENTRIES {
        let excess = entries.len() - MAX_HISTORY_ENTRIES;
        entries.drain(0..excess);
    }

    let payload = entries.join("\n") + "\n";
    if let Err(err) = crate::utils::write_atomic(path, payload.as_bytes()) {
        tracing::warn!(
            "持久化编辑器历史到 {} 失败：{err}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试使用路径注入的 `*_from` / `*_to` 辅助函数，这样它们
    /// 就不必修改 `HOME`（在 Windows 上 `dirs::home_dir()` 不读取
    /// `HOME` — 它读取 `USERPROFILE` / `SHGetKnownFolderPath`）。这使测试套件在
    /// 所有三个 CI 运行器上都能移植，无需每个平台的环境变量设置。
    fn temp_history_path() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(HISTORY_FILE_NAME);
        (tmp, path)
    }

    #[test]
    fn append_and_load_round_trip() {
        let (_tmp, path) = temp_history_path();
        append_history_to(&path, "first");
        append_history_to(&path, "second");
        append_history_to(&path, "third");
        assert_eq!(load_history_from(&path), vec!["first", "second", "third"]);
    }

    #[test]
    fn slash_commands_skipped() {
        let (_tmp, path) = temp_history_path();
        append_history_to(&path, "/help");
        append_history_to(&path, "real prompt");
        append_history_to(&path, "/cost");
        assert_eq!(load_history_from(&path), vec!["real prompt"]);
    }

    #[test]
    fn empty_and_whitespace_skipped() {
        let (_tmp, path) = temp_history_path();
        append_history_to(&path, "");
        append_history_to(&path, "   ");
        append_history_to(&path, "\n\t");
        append_history_to(&path, "real");
        assert_eq!(load_history_from(&path), vec!["real"]);
    }

    #[test]
    fn consecutive_duplicates_deduped() {
        let (_tmp, path) = temp_history_path();
        append_history_to(&path, "same");
        append_history_to(&path, "same");
        append_history_to(&path, "same");
        append_history_to(&path, "different");
        append_history_to(&path, "same");
        assert_eq!(load_history_from(&path), vec!["same", "different", "same"]);
    }

    #[test]
    fn pruned_to_cap_at_append_time() {
        let (_tmp, path) = temp_history_path();
        for i in 0..(MAX_HISTORY_ENTRIES + 50) {
            append_history_to(&path, &format!("entry {i}"));
        }
        let history = load_history_from(&path);
        assert_eq!(history.len(), MAX_HISTORY_ENTRIES);
        // 最新的条目保留；最旧的 50 条被修剪。
        assert_eq!(history.first().map(String::as_str), Some("entry 50"));
        assert_eq!(
            history.last().map(String::as_str),
            Some(format!("entry {}", MAX_HISTORY_ENTRIES + 49)).as_deref()
        );
    }

    #[test]
    fn missing_file_loads_empty() {
        let (_tmp, path) = temp_history_path();
        assert!(load_history_from(&path).is_empty());
    }
}
