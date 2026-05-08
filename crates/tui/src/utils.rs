//! `DeepSeek` CLI 的通用工具辅助函数。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::models::{ContentBlock, Message};
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use serde_json::Value;

// === Project Mapping Helpers ===

/// 判断文件是否为项目识别的"关键"文件。
#[must_use]
pub fn is_key_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };

    matches!(
        file_name.to_lowercase().as_str(),
        "cargo.toml"
            | "package.json"
            | "requirements.txt"
            | "build.gradle"
            | "pom.xml"
            | "readme.md"
            | "agents.md"
            | "claude.md"
            | "makefile"
            | "dockerfile"
            | "main.rs"
            | "lib.rs"
            | "index.js"
            | "index.ts"
            | "app.py"
    )
}

/// 基于关键文件生成项目的高级摘要。
///
/// 输出在多次调用间保持字节稳定：`WalkBuilder` 不排序兄弟项
///（操作系统的 readdir 顺序会透出），因此在不会预排序的文件系统上，
/// 连接的 `key_files` 列表会在运行间重新排序。
/// 仅在工作区没有 `AGENTS.md` / `CLAUDE.md` 时才有影响，
/// 因为系统提示词首先通过 `ProjectContext::as_system_block` 路由，
/// 仅当没有项目上下文文档时才回退到此函数。
#[must_use]
pub fn summarize_project(root: &Path) -> String {
    let mut key_files = Vec::new();

    let mut builder = WalkBuilder::new(root);
    builder.hidden(false).follow_links(true).max_depth(Some(2));
    let walker = builder.build();

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if is_key_file(entry.path())
            && let Ok(rel) = entry.path().strip_prefix(root)
        {
            key_files.push(rel.to_string_lossy().to_string());
        }
    }

    key_files.sort();

    if key_files.is_empty() {
        return "未知项目类型".to_string();
    }

    let mut types = Vec::new();
    if key_files
        .iter()
        .any(|f| f.to_lowercase().contains("cargo.toml"))
    {
        types.push("Rust");
    }
    if key_files
        .iter()
        .any(|f| f.to_lowercase().contains("package.json"))
    {
        types.push("JavaScript/Node.js");
    }
    if key_files
        .iter()
        .any(|f| f.to_lowercase().contains("requirements.txt"))
    {
        types.push("Python");
    }

    if types.is_empty() {
        format!("关键文件项目：{}", key_files.join(", "))
    } else {
        format!("{} 项目", types.join(" + "))
    }
}

/// 生成项目结构的树状视图。
///
/// 兄弟项顺序通过排序收集的路径来固定 — 底层的 `WalkBuilder` 遵循操作系统的
/// readdir 顺序，这在文件系统间是不确定的。按完整路径排序保留了树形结构
///（目录仍在其子项之前，因为 `"src" < "src/lib.rs"`）
/// 同时使渲染的输出在多次运行间保持字节稳定。
#[must_use]
pub fn project_tree(root: &Path, max_depth: usize) -> String {
    let mut entries: Vec<(PathBuf, bool)> = Vec::new();

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .follow_links(true)
        .max_depth(Some(max_depth + 1));

    for entry in builder.build().flatten() {
        let depth = entry.depth();
        if depth == 0 || depth > max_depth {
            continue;
        }
        let rel_path = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_path_buf();
        let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
        entries.push((rel_path, is_dir));
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut tree_lines = Vec::with_capacity(entries.len());
    for (rel_path, is_dir) in entries {
        let depth = rel_path.components().count();
        let indent = "  ".repeat(depth.saturating_sub(1));
        let prefix = if is_dir { "DIR: " } else { "FILE: " };
        tree_lines.push(format!(
            "{}{}{}",
            indent,
            prefix,
            rel_path.file_name().unwrap_or_default().to_string_lossy()
        ));
    }

    tree_lines.join("\n")
}

// === Filesystem Helpers ===

/// 使用临时文件 + fsync + 重命名原子化地将 `contents` 写入 `path`。
///
/// 1. 在与 `path` 相同的目录（同一文件系统）中创建 `NamedTempFile`。
/// 2. 将 `contents` 写入临时文件。
/// 3. 对临时文件调用 `sync_all()` 以确保持久性。
/// 4. 原子化地将临时文件重命名（持久化）覆盖 `path`。
///
/// 在支持的文件系统上（`ext4`、`apfs`、`ntfs`），重命名是原子操作 —
/// 并发读取者要么看到旧内容，要么看到新内容，绝不会看到部分写入。
/// `sync_all` 确保数据在元数据更改前已写入稳定存储，因此重命名期间的
/// 操作系统崩溃不会丢失数据。
///
/// # 错误
/// 如果无法确定父目录、无法创建临时文件、写入失败或重命名失败，返回 `io::Error`。
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("path has no parent directory: {}", path.display()),
        )
    })?;
    // Use parent directory so the rename is on the same filesystem.
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, contents)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}

/// 在 `path` 打开或创建用于追加的文件，可选择在每次写入后同步。
/// 用于仅追加日志，如 `audit.log`。
///
/// 返回的 `BufWriter<fs::File>` 包装了追加句柄。在每个批次后调用
/// `.flush()` 后跟 `.get_ref().sync_all()`。
pub fn open_append(path: &Path) -> std::io::Result<std::io::BufWriter<std::fs::File>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok(std::io::BufWriter::new(file))
}

/// 刷新包装 `File` 的 `BufWriter`，然后 `fsync` 底层文件。
pub fn flush_and_sync(writer: &mut std::io::BufWriter<std::fs::File>) -> std::io::Result<()> {
    writer.flush()?;
    writer.get_ref().sync_all()
}

/// 生成带 panic 监督的 tokio 任务。
///
/// 将 future 包装在 `AssertUnwindSafe` + `catch_unwind` 中。发生 panic 时：
/// 1. 通过 `tracing::error!` 记录 panic 的任务名称和调用位置。
/// 2. 将崩溃转储写入 `~/.deepseek/crashes/<timestamp>-<name>.log`。
///
/// 返回的 `JoinHandle` 解析为 `()` — panic 被捕获并内部处理，
/// 因此父进程保持存活。
pub fn spawn_supervised<F>(
    name: &'static str,
    location: &'static std::panic::Location<'static>,
    future: F,
) -> tokio::task::JoinHandle<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        use futures_util::FutureExt;
        let result = std::panic::AssertUnwindSafe(future).catch_unwind().await;
        if let Err(panic_info) = result {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!(
                target: "panic",
                "Task '{name}' panicked at {}: {msg}",
                location,
            );
            // Write crash dump (best-effort)
            let _ = write_panic_dump(name, location, &msg);
        }
    })
}

/// 将 panic 转储文件写入 `~/.deepseek/crashes/`。
///
/// 在需要时创建目录，并写入带有时间戳的日志，包含任务名称、
/// 调用位置和 panic 消息。
/// 尽力而为 — 失败会被静默忽略。
fn write_panic_dump(
    name: &str,
    location: &std::panic::Location<'_>,
    message: &str,
) -> std::io::Result<()> {
    let home = dirs::home_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "home directory not found")
    })?;
    let crash_dir = home.join(".deepseek").join("crashes");
    write_panic_dump_to(&crash_dir, name, location, message)
}

fn write_panic_dump_to(
    crash_dir: &Path,
    name: &str,
    location: &std::panic::Location<'_>,
    message: &str,
) -> std::io::Result<()> {
    use chrono::Utc;
    std::fs::create_dir_all(crash_dir)?;
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let filename = format!("{timestamp}-{name}.log");
    let path = crash_dir.join(&filename);
    let contents =
        format!("Task: {name}\nLocation: {location}\nTimestamp: {timestamp}\nPanic: {message}\n");
    std::fs::write(&path, contents)?;
    Ok(())
}

/// 带 panic 转储保护的"发后即忘" `spawn_blocking`。
///
/// 与 `spawn_supervised`（包装 `tokio::spawn` 用于异步任务）不同，
/// 此辅助函数包装 `tokio::task::spawn_blocking`。当 CPU 密集型或阻塞 I/O
/// 任务必须在异步运行时之外运行，并且其完成*不被*等待时使用 — 例如轮次后的
/// 磁盘快照或稍后通过共享数据结构轮询的文件树构建。如果闭包发生 panic，
/// 会将崩溃转储写入 `~/.deepseek/crashes/`，并以 ERROR 级别记录 panic，
/// 而不是被静默吞没。
#[track_caller]
pub fn spawn_blocking_supervised<F>(name: &'static str, f: F) -> tokio::task::JoinHandle<()>
where
    F: FnOnce() + Send + 'static,
{
    let location = std::panic::Location::caller();
    tokio::task::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if let Err(panic_info) = result {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!(
                target: "panic",
                "Blocking task '{name}' panicked at {location}: {msg}",
            );
            let _ = write_panic_dump(name, location, &msg);
        }
    })
}

#[allow(dead_code)]
pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("Failed to create directory: {}", path.display()))
}

/// 使用美化格式渲染 JSON，出错时回退到紧凑字符串。
#[must_use]
#[allow(dead_code)]
pub fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// 将字符串截断到最大长度，如果截断则添加省略号。
///
/// 使用字符边界避免在多字节 UTF-8 字符上 panic。
#[must_use]
pub fn truncate_with_ellipsis(s: &str, max_len: usize, ellipsis: &str) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let budget = max_len.saturating_sub(ellipsis.len());
    // Find the last char boundary that fits within the byte budget.
    let safe_end = s
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= budget)
        .last()
        .unwrap_or(0);
    format!("{}{}", &s[..safe_end], ellipsis)
}

/// 对字符串进行百分号编码，用于 URL 查询参数。
///
/// 编码除未保留字符（A-Z, a-z, 0-9, `-`, `_`, `.`, `~`）之外的所有字符。
/// 空格编码为 `+`。
#[must_use]
pub fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for ch in input.bytes() {
        match ch {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(ch as char)
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{ch:02X}")),
        }
    }
    encoded
}

/// 将路径渲染为**面向用户的显示**，家目录缩写为 `~`。在 TUI、doctor/setup
/// 标准输出以及任何观看者可能看到输出的地方（截图、视频、粘贴到 issue 的帮助）
/// 使用此函数。在 macOS/Linux 上，绝对路径 `/Users/<name>/...` 或
/// `/home/<name>/...` 会暴露操作系统账户名，这通常与公共用户名相同 —
/// 对于分享终端的用户来说不理想。
///
/// **不要**在持久化路径（会话、审计日志）或发送给 LLM 提供商的路径上使用 —
/// 那些需要完整保真度，以便跨进程正确解析。
#[must_use]
pub fn display_path(path: &Path) -> String {
    display_path_with_home(path, dirs::home_dir().as_deref())
}

/// 与 [`display_path`] 类似，但使用显式的家目录而不是读取 `$HOME` / `dirs::home_dir()`。
/// 在测试和调用方已有家目录路径可用时使用。
///
/// 家目录相对后缀通过遍历路径组件，使用平台分隔符（Windows 上为 `\`，
/// 其他平台为 `/`）重新连接，因此带有外来分隔符的输入不会透出。
#[must_use]
pub fn display_path_with_home(path: &Path, home: Option<&Path>) -> String {
    let Some(home) = home else {
        return path.display().to_string();
    };
    if let Ok(rest) = path.strip_prefix(home) {
        if rest.as_os_str().is_empty() {
            return "~".to_string();
        }
        let sep = std::path::MAIN_SEPARATOR_STR;
        let mut out = String::from("~");
        for component in rest.components() {
            out.push_str(sep);
            out.push_str(&component.as_os_str().to_string_lossy());
        }
        return out;
    }
    path.display().to_string()
}

/// 估算消息内容块中的总字符数。
#[must_use]
pub fn estimate_message_chars(messages: &[Message]) -> usize {
    let mut total = 0;
    for msg in messages {
        for block in &msg.content {
            match block {
                ContentBlock::Text { text, .. } => total += text.len(),
                ContentBlock::Thinking { thinking } => total += thinking.len(),
                ContentBlock::ToolUse { input, .. } => total += input.to_string().len(),
                ContentBlock::ToolResult { content, .. } => total += content.len(),
                ContentBlock::ServerToolUse { .. }
                | ContentBlock::ToolSearchToolResult { .. }
                | ContentBlock::CodeExecutionToolResult { .. } => {}
            }
        }
    }
    total
}

// 测试使用 `display_path_with_home`，因此它们从不修改全局 `HOME` 环境变量。
// 通过 `std::env::set_var` 修改 `HOME` 不是线程安全的；Cargo 默认并行运行测试，
// CI 运行器是多核的，因此任何覆盖 `HOME` 的测试将与*读取*它的测试竞争。
// 使用注入的辅助函数完全避免了竞争，并使测试无需额外平台脚手架即可移植到 Windows。
#[cfg(test)]
mod tests {
    use super::display_path_with_home;
    use std::path::PathBuf;

    fn home(s: &str) -> Option<PathBuf> {
        Some(PathBuf::from(s))
    }

    #[test]
    fn display_path_contracts_home_prefix() {
        let h = home("/Users/alice");
        assert_eq!(
            display_path_with_home(&PathBuf::from("/Users/alice/projects/foo"), h.as_deref()),
            format!(
                "~{}projects{}foo",
                std::path::MAIN_SEPARATOR,
                std::path::MAIN_SEPARATOR
            ),
        );
    }

    #[test]
    fn display_path_returns_bare_tilde_for_home_itself() {
        let h = home("/Users/alice");
        assert_eq!(
            display_path_with_home(&PathBuf::from("/Users/alice"), h.as_deref()),
            "~"
        );
    }

    #[test]
    fn display_path_leaves_unrelated_paths_alone() {
        let h = home("/Users/alice");
        // 不同用户 — 不得被重写或共享波浪线。
        assert_eq!(
            display_path_with_home(&PathBuf::from("/Users/bob/Code"), h.as_deref()),
            "/Users/bob/Code".to_string()
        );
        // 系统路径必须保持绝对。
        assert_eq!(
            display_path_with_home(&PathBuf::from("/etc/hosts"), h.as_deref()),
            "/etc/hosts"
        );
    }

    #[test]
    fn display_path_does_not_match_username_prefix() {
        // 回归防护：名称像用户家目录*前缀*但不在其下的目录不得被重写。
        let h = home("/Users/alice");
        assert_eq!(
            display_path_with_home(&PathBuf::from("/Users/alice2/work"), h.as_deref()),
            "/Users/alice2/work"
        );
    }

    #[test]
    fn display_path_with_no_home_returns_full_path() {
        assert_eq!(
            display_path_with_home(&PathBuf::from("/some/path"), None),
            "/some/path"
        );
    }
}

#[cfg(test)]
mod atomic_write_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn write_atomic_writes_content() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("test.json");
        let content = b"hello atomic world";

        write_atomic(&path, content).expect("write_atomic");
        assert!(path.exists());
        let read = fs::read_to_string(&path).expect("read");
        assert_eq!(read.as_bytes(), content);
    }

    #[test]
    fn write_atomic_replaces_existing_file() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("existing.json");
        fs::write(&path, b"old content").expect("write old");
        write_atomic(&path, b"new content").expect("write_atomic");
        let read = fs::read_to_string(&path).expect("read");
        assert_eq!(read, "new content");
    }

    #[test]
    fn write_atomic_no_temp_left_behind_on_success() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("clean.json");
        write_atomic(&path, b"clean").expect("write_atomic");
        // List files in dir — there should be no .tmp files left
        let entries: Vec<_> = fs::read_dir(tmp.path())
            .expect("read_dir")
            .filter_map(|e| e.ok())
            .collect();
        let tmp_files: Vec<_> = entries
            .iter()
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with('.')))
            .collect();
        assert!(
            tmp_files.is_empty(),
            "temp files left behind: {tmp_files:?}"
        );
    }

    #[test]
    fn flush_and_sync_writes_and_syncs() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("append.log");
        {
            let mut writer = open_append(&path).expect("open_append");
            writeln!(writer, "line 1").expect("write");
            flush_and_sync(&mut writer).expect("flush_and_sync");
            writeln!(writer, "line 2").expect("write");
            flush_and_sync(&mut writer).expect("flush_and_sync");
        }
        let content = fs::read_to_string(&path).expect("read");
        assert_eq!(content, "line 1\nline 2\n");
    }
}

#[cfg(test)]
mod spawn_supervised_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// 发生 panic 的生成任务不会将 panic 传播到父任务 — `spawn_supervised` 捕获它。
    /// 与磁盘上的崩溃转储路径隔离验证，因此测试可跨 macOS / Linux / Windows 移植
    ///（在 Windows 上 `dirs::home_dir()` 读取 `USERPROFILE` 而非 `HOME`，
    /// 因此环境变量修改技巧无法重定向转储）。
    #[tokio::test]
    async fn panicking_task_does_not_propagate_to_parent() {
        let parent_alive = Arc::new(AtomicBool::new(false));
        let parent_alive_clone = parent_alive.clone();

        let handle = spawn_supervised(
            "panic-test-fixture",
            std::panic::Location::caller(),
            async move {
                parent_alive_clone.store(true, Ordering::SeqCst);
                panic!("deliberate panic for catch-unwind test");
            },
        );

        let result = handle.await;
        assert!(
            result.is_ok(),
            "spawn_supervised must convert panic to a normal completion"
        );
        assert!(
            parent_alive.load(Ordering::SeqCst),
            "fixture task must have run before panicking"
        );
    }

    #[tokio::test]
    async fn panicking_blocking_task_does_not_propagate_to_parent() {
        let parent_alive = Arc::new(AtomicBool::new(false));
        let parent_alive_clone = parent_alive.clone();

        let handle = spawn_blocking_supervised("blocking-panic-test-fixture", move || {
            parent_alive_clone.store(true, Ordering::SeqCst);
            panic!("deliberate panic for spawn_blocking catch-unwind test");
        });

        let result = handle.await;
        assert!(
            result.is_ok(),
            "spawn_blocking_supervised must convert panic to a normal completion"
        );
        assert!(
            parent_alive.load(Ordering::SeqCst),
            "fixture blocking task must have run before panicking"
        );
    }

    /// `write_panic_dump_to` 将格式正确的崩溃日志写入提供的目录。
    /// 与 `spawn_supervised` 分开测试，因为通过环境变量修改重定向
    /// `dirs::home_dir()` 在 Windows 上不起作用。
    #[test]
    fn write_panic_dump_writes_named_log() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let crash_dir = tmp.path().join("crashes");
        let location = std::panic::Location::caller();
        write_panic_dump_to(&crash_dir, "panic-fixture", location, "boom").expect("write dump");

        let entries: Vec<_> = std::fs::read_dir(&crash_dir)
            .expect("crashes dir exists")
            .flatten()
            .collect();
        assert_eq!(entries.len(), 1, "exactly one crash dump expected");
        let dump = std::fs::read_to_string(entries[0].path()).expect("read dump");
        assert!(
            dump.contains("panic-fixture"),
            "dump must include the task name; got: {dump}"
        );
        assert!(
            dump.contains("boom"),
            "dump must include the panic message; got: {dump}"
        );
    }
}

#[cfg(test)]
mod project_mapping_tests {
    use super::{project_tree, summarize_project};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn project_tree_sorts_siblings_alphabetically() {
        // 跨平台 readdir 不能保证字母顺序 — 在带 htree 的 ext4 上是哈希顺序，
        // 在 APFS 上大致是插入顺序，在 ZFS 上取决于存储类。当工作区没有
        // AGENTS.md / CLAUDE.md 时，系统提示将此字符串嵌入缓存前缀中，
        // 因此函数必须在不同文件系统上保持字节稳定。
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        // 故意以打乱的顺序创建文件，使宿主文件系统的预排序（如果有）
        // 不太可能掩盖我们代码中缺失的排序。
        fs::write(root.join("zebra.txt"), "z").expect("write zebra");
        fs::write(root.join("apple.txt"), "a").expect("write apple");
        fs::write(root.join("mango.txt"), "m").expect("write mango");

        let tree = project_tree(root, 1);
        let lines: Vec<&str> = tree.lines().collect();
        let apple_pos = lines
            .iter()
            .position(|l| l.contains("apple.txt"))
            .expect("apple line");
        let mango_pos = lines
            .iter()
            .position(|l| l.contains("mango.txt"))
            .expect("mango line");
        let zebra_pos = lines
            .iter()
            .position(|l| l.contains("zebra.txt"))
            .expect("zebra line");

        assert!(apple_pos < mango_pos);
        assert!(mango_pos < zebra_pos);
    }

    #[test]
    fn project_tree_keeps_directory_before_its_children() {
        // 按完整路径排序兄弟项足以保留树形结构：
        // `"src" < "src/lib.rs"` 因为较短的字符串比较值较小。
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        let src = root.join("src");
        fs::create_dir_all(&src).expect("mkdir src");
        fs::write(src.join("lib.rs"), "lib").expect("write lib");
        fs::write(src.join("main.rs"), "main").expect("write main");

        let tree = project_tree(root, 2);
        let src_pos = tree.find("DIR: src").expect("src dir line");
        let lib_pos = tree.find("FILE: lib.rs").expect("lib file line");
        let main_pos = tree.find("FILE: main.rs").expect("main file line");

        assert!(src_pos < lib_pos, "directory must precede its children");
        assert!(lib_pos < main_pos, "siblings sorted by name");
    }

    #[test]
    fn project_tree_is_byte_stable_across_calls() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        fs::write(root.join("z.txt"), "z").expect("write");
        fs::write(root.join("a.txt"), "a").expect("write");

        assert_eq!(project_tree(root, 1), project_tree(root, 1));
    }

    #[test]
    fn summarize_project_sorts_key_files_in_fallback() {
        // 当 `summarize_project` 无法分类项目类型时，它会回退到列出发现的关键文件。
        // 该连接列表必须是确定性的，以便嵌入它的系统提示在按非字母顺序发出 readdir
        // 的文件系统上不会在运行间漂移。
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        // 使用不会触发任何类型检测器的关键文件（Cargo.toml / package.json / requirements.txt），
        // 以便函数进入"关键文件项目：…"分支。
        fs::write(root.join("Makefile"), "all:").expect("write makefile");
        fs::write(root.join("README.md"), "# x").expect("write readme");

        let summary = summarize_project(root);
        assert!(
            summary.starts_with("关键文件项目："),
            "期望回退分支；得到：{summary}"
        );
        let suffix = summary
            .strip_prefix("关键文件项目：")
            .expect("prefix");
        assert_eq!(suffix, "Makefile, README.md");
    }
}
