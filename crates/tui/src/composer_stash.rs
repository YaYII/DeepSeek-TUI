//! 编辑器的暂存草稿（#440）。
//!
//! 暂存区是历史记录的侧通道：它保存用户有意存放的草稿（Ctrl+S），
//! 而不是过去的提交内容（存在于 `composer_history.rs` 中）。
//! 弹出语义使其成为 LIFO — 最新的暂存最先取出。
//!
//! ## 磁盘格式
//!
//! `~/.deepseek/composer_stash.jsonl` — 每行一个 JSON 对象：
//!
//! ```jsonl
//! {"ts":"2026-05-04T01:23:45Z","text":"draft here"}
//! ```
//!
//! 自我修复解析器：格式错误的行被静默跳过，因此单个错误写入
//! 不会损坏暂存区的其余部分。解析器不要求任何特定字段顺序；
//! 只有 `text` 是必需的。
//!
//! ## 为什么使用 JSONL 而不是纯文本文件？
//!
//! 草稿可以包含换行符（它们是提示，不是单行命令），
//! 因此以 `\n` 分隔的纯文件会破坏多行草稿。
//! JSONL 在 JSON 字符串内无歧义地转义换行符，
//! 且时间戳/未来字段也能干净地存放。

use std::fs;
use std::io;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const STASH_FILE_NAME: &str = "composer_stash.jsonl";

/// 硬上限，防止失控的脚本用存放的草稿填满用户的家目录。
/// 当暂存超过此数量时，会在推送时修剪较旧的条目。
pub const MAX_STASH_ENTRIES: usize = 200;

/// 一个存放的草稿。字段使用 `#[serde(default)]`，以便旧版/
/// 截断的记录仍能被解析，而不会破坏暂存区。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashedDraft {
    /// RFC 3339 时间戳；旧版记录上省略。
    #[serde(default)]
    pub ts: String,
    /// 存放的文本。必需 — 加载时丢弃没有 `text` 的条目（视为格式错误）。
    pub text: String,
}

fn default_stash_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".deepseek").join(STASH_FILE_NAME))
}

/// 从磁盘加载所有暂存的草稿，按写入顺序排列（最旧的在前）。
/// 自我修复：格式错误的行被静默丢弃。当文件不存在时返回空 vec。
#[must_use]
pub fn load_stash() -> Vec<StashedDraft> {
    let Some(path) = default_stash_path() else {
        return Vec::new();
    };
    load_stash_from(&path)
}

fn load_stash_from(path: &Path) -> Vec<StashedDraft> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<StashedDraft>(&line).ok())
        .filter(|draft| !draft.text.is_empty())
        .collect()
}

/// 将一个草稿推入暂存区。空文本/仅空白文本会被静默丢弃，
/// 这样在空编辑器中误按 Ctrl+S 不会污染文件。失败会记录日志但永不
/// 传播 — 暂存区是 UX 优化项，不是正确性问题。
pub fn push_stash(text: &str) {
    let Some(path) = default_stash_path() else {
        return;
    };
    push_stash_to(&path, text);
}

fn push_stash_to(path: &Path, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        tracing::warn!(
            "Failed to create composer stash dir {}: {err}",
            parent.display()
        );
        return;
    }

    let mut entries = load_stash_from(path);
    entries.push(StashedDraft {
        ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        text: text.to_string(),
    });
    if entries.len() > MAX_STASH_ENTRIES {
        let excess = entries.len() - MAX_STASH_ENTRIES;
        entries.drain(0..excess);
    }
    write_stash_to(path, &entries);
}

/// 移除并返回最近推入的草稿（如果有）。
/// 用剩余条目重写磁盘上的文件。
#[must_use]
pub fn pop_stash() -> Option<StashedDraft> {
    let path = default_stash_path()?;
    pop_stash_from(&path)
}

/// 完全清除暂存文件。返回被丢弃的条目数量（以便调用者报告）。当文件
/// 不存在或没有条目时返回 0。
pub fn clear_stash() -> io::Result<usize> {
    let Some(path) = default_stash_path() else {
        return Ok(0);
    };
    clear_stash_at(&path)
}

fn clear_stash_at(path: &Path) -> io::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let entries = load_stash_from(path);
    let count = entries.len();
    if count == 0 {
        return Ok(0);
    }
    crate::utils::write_atomic(path, b"")?;
    Ok(count)
}

fn pop_stash_from(path: &Path) -> Option<StashedDraft> {
    let mut entries = load_stash_from(path);
    let popped = entries.pop()?;
    write_stash_to(path, &entries);
    Some(popped)
}

fn write_stash_to(path: &Path, entries: &[StashedDraft]) {
    let mut payload = String::new();
    for entry in entries {
        match serde_json::to_string(entry) {
            Ok(line) => {
                payload.push_str(&line);
                payload.push('\n');
            }
            Err(err) => {
                // 经过 serde 往返序列化的草稿不应序列化失败，
                // 但以防万一，`text` 中的奇异码位不会在写入中途毁掉文件。
                tracing::warn!("因序列化失败跳过暂存条目：{err}");
            }
        }
    }
    if let Err(err) = crate::utils::write_atomic(path, payload.as_bytes()) {
        tracing::warn!(
            "持久化暂存区到 {} 失败：{err}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_stash_path() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("composer_stash.jsonl");
        (tmp, path)
    }

    #[test]
    fn push_and_load_round_trip() {
        let (_tmp, path) = temp_stash_path();
        push_stash_to(&path, "first draft");
        push_stash_to(&path, "second draft");
        let entries = load_stash_from(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "first draft");
        assert_eq!(entries[1].text, "second draft");
        assert!(!entries[1].ts.is_empty(), "timestamp stamped on push");
    }

    #[test]
    fn pop_returns_lifo_and_rewrites_file() {
        let (_tmp, path) = temp_stash_path();
        push_stash_to(&path, "first");
        push_stash_to(&path, "second");
        let popped = pop_stash_from(&path).expect("non-empty stash");
        assert_eq!(popped.text, "second");
        let remaining = load_stash_from(&path);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].text, "first");
    }

    #[test]
    fn pop_on_empty_stash_returns_none() {
        let (_tmp, path) = temp_stash_path();
        assert!(pop_stash_from(&path).is_none());
    }

    #[test]
    fn empty_text_is_dropped() {
        let (_tmp, path) = temp_stash_path();
        push_stash_to(&path, "");
        push_stash_to(&path, "   \n  ");
        assert!(load_stash_from(&path).is_empty());
    }

    #[test]
    fn multiline_drafts_are_preserved_intact() {
        let (_tmp, path) = temp_stash_path();
        let multiline = "first line\nsecond line\n  third line";
        push_stash_to(&path, multiline);
        let entries = load_stash_from(&path);
        assert_eq!(entries.len(), 1);
        // 多行文本能往返传输是因为 JSON 转义了换行符。
        assert_eq!(entries[0].text, multiline);
    }

    #[test]
    fn malformed_lines_are_skipped_and_valid_lines_survive() {
        let (_tmp, path) = temp_stash_path();
        // 混合有效的 JSON、垃圾数据和部分写入截断。
        let raw = "\
{\"ts\":\"2026-05-04T01:23:45Z\",\"text\":\"good one\"}
this is not json
{\"text\":\"good two\"}
{\"ts\":\"2026-05-04T01:24:00Z\"
{\"text\":\"\"}
{}
";
        std::fs::write(&path, raw).unwrap();
        let entries = load_stash_from(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "good one");
        assert_eq!(entries[1].text, "good two");
    }

    #[test]
    fn clear_returns_zero_when_file_is_absent() {
        let (_tmp, path) = temp_stash_path();
        // 路径尚不存在。
        assert_eq!(clear_stash_at(&path).unwrap(), 0);
    }

    #[test]
    fn clear_returns_zero_when_file_is_empty() {
        let (_tmp, path) = temp_stash_path();
        std::fs::write(&path, "").unwrap();
        assert_eq!(clear_stash_at(&path).unwrap(), 0);
    }

    #[test]
    fn clear_drops_entries_and_reports_count() {
        let (_tmp, path) = temp_stash_path();
        push_stash_to(&path, "first");
        push_stash_to(&path, "second");
        push_stash_to(&path, "third");
        let dropped = clear_stash_at(&path).expect("clear succeeds");
        assert_eq!(dropped, 3);
        // 文件仍然存在但为空，因此后续加载返回空内容。
        assert!(load_stash_from(&path).is_empty());
    }

    #[test]
    fn cap_prunes_oldest_at_push_time() {
        let (_tmp, path) = temp_stash_path();
        for i in 0..(MAX_STASH_ENTRIES + 5) {
            push_stash_to(&path, &format!("draft {i}"));
        }
        let entries = load_stash_from(&path);
        assert_eq!(entries.len(), MAX_STASH_ENTRIES);
        // 最旧保留的是 `5..`，因为前 5 条被修剪了。
        assert_eq!(entries[0].text, "draft 5");
        assert_eq!(
            entries[entries.len() - 1].text,
            format!("draft {}", MAX_STASH_ENTRIES + 5 - 1)
        );
    }
}
