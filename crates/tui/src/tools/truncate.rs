//! 工具输出溢出写入器（#422）。
//!
//! 当工具产生的输出太大而无法放入模型的上下文预算时，
//! 我们希望同时做两件事：
//!
//! 1. 转录/工具单元格渲染一个有边界的预览，以便 UI 保持可浏览性。
//! 2. 完整的原始输出保存在磁盘上，以便模型在以后需要被省略的尾部时
//!    可以通过 `read_file` 读回，也方便用户在 `$EDITOR` 中打开。
//!
//! 本模块负责磁盘端。文件存放在 `~/.deepseek/tool_outputs/<清理后的id>.txt`。
//! id 是引擎分配的工具调用 id；我们保守地进行清理（仅允许 ASCII 字母数字 +
//! `-`/`_`），以防止恶意 id 通过 `..` 或绝对路径技巧逃逸目录。
//!
//! 启动时清理会删除 mtime 早于 [`SPILLOVER_MAX_AGE`]（7 天）的文件。
//! 清理失败会被记录但不会致命——用户不应因为一个过时的工具输出文件而看到启动卡住。
//!
//! ## 实时调用者
//!
//! * [`apply_spillover`] — 从引擎的工具执行路径（`turn_loop.rs`）调用，
//!   任何超过 [`SPILLOVER_THRESHOLD_BYTES`] 的成功工具结果都会被溢出到磁盘，
//!   模型会收到一个 [`SPILLOVER_HEAD_BYTES`] 大小的头部和一个指针页脚。
//! * 启动清理在 `main.rs` 中删除早于 [`SPILLOVER_MAX_AGE`] 的文件。
//!
//! UI 端的内联 `full output: <path>` 注释的渲染由
//! `tui/history.rs::render_spillover_annotation` 负责。工具详情分页器在用户
//! 在已溢出的工具单元格上按 `Alt+V`（或在空编辑器上按 `v`）时打开溢出文件。

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::tools::spec::ToolResult;

// `Path` is only referenced from helpers gated to test builds.
#[cfg(test)]
use std::path::Path;

/// `~/.deepseek/` 下溢出目录的名称。
pub const SPILLOVER_DIR_NAME: &str = "tool_outputs";

/// 默认阈值，超过此值的工具结果将成为溢出候选。
/// 镜像了我们在其他地方用于"太大而无法内联"的 `MAX_MEMORY_SIZE` 上限，
/// 使规则保持一致。如果某个工具族有不同的经济考量，调用者可以传递不同的值。
pub const SPILLOVER_THRESHOLD_BYTES: usize = 100 * 1024; // 100 KiB

/// 默认启动清理期限。超过此期限的溢出文件在启动时被删除，
/// 以防止 `~/.deepseek/tool_outputs/` 无限制增长。镜像了工作区快照的 7 天默认值。
pub const SPILLOVER_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[cfg(test)]
static TEST_SPILLOVER_ROOT: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) static TEST_SPILLOVER_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 解析 `~/.deepseek/tool_outputs/`。如果无法确定 home 目录则返回 `None`
///（CI 容器偶尔会遇到此情况）。调用者应将 `None` 视为"溢出不可用"并优雅降级，
/// 而不是使工具调用失败。
#[must_use]
pub fn spillover_root() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(root) = TEST_SPILLOVER_ROOT
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clone()
    {
        return Some(root);
    }

    Some(dirs::home_dir()?.join(".deepseek").join(SPILLOVER_DIR_NAME))
}

/// 覆盖测试的溢出根目录，无需修改 `$HOME`。
#[cfg(test)]
pub(crate) fn set_test_spillover_root(root: Option<PathBuf>) -> Option<PathBuf> {
    let mut guard = TEST_SPILLOVER_ROOT
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    std::mem::replace(&mut *guard, root)
}

/// 解析工具调用 id 的溢出文件路径。对 id 进行清理，防止恶意值逃逸存储目录。
/// 对空或完全无效的 id 返回 `None`；调用者应将其视为"溢出不可用"并跳过写入。
#[must_use]
pub fn spillover_path(id: &str) -> Option<PathBuf> {
    let sanitised = sanitise_id(id)?;
    Some(spillover_root()?.join(format!("{sanitised}.txt")))
}

/// 将 `content` 写入 `id` 的溢出文件。如果需要则创建父目录。
/// 成功时返回解析后的路径。
///
/// 通过底层操作系统的 `write` + 文件系统重命名保证实现原子性——
/// 文件首先以临时名称创建，然后重命名为最终名称。
/// 失败以 `io::Error` 形式冒泡，调用者可决定是否将其展示。
pub fn write_spillover(id: &str, content: &str) -> io::Result<PathBuf> {
    let path = spillover_path(id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "could not resolve spillover path (empty/invalid id or missing home directory)",
        )
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::utils::write_atomic(&path, content.as_bytes())?;
    Ok(path)
}

/// 删除早于 `max_age` 的溢出文件。返回已删除的文件数。
/// 非致命：目录不存在返回 0；每个文件的错误会被记录并跳过。
/// 镜像了 [`crate::session_manager::prune_workspace_snapshots`]。
pub fn prune_older_than(max_age: Duration) -> io::Result<usize> {
    let Some(root) = spillover_root() else {
        return Ok(0);
    };
    if !root.exists() {
        return Ok(0);
    }
    let cutoff = SystemTime::now()
        .checked_sub(max_age)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut pruned = 0usize;
    for entry in fs::read_dir(&root)? {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(target: "spillover", ?err, "skipping unreadable dir entry");
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let modified = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(target: "spillover", ?err, ?path, "skipping unreadable mtime");
                continue;
            }
        };
        if modified < cutoff {
            if let Err(err) = fs::remove_file(&path) {
                tracing::warn!(target: "spillover", ?err, ?path, "spillover prune skipped a file");
                continue;
            }
            pruned += 1;
        }
    }
    Ok(pruned)
}

/// 通用"太长？溢出"模式的便利函数。如果 `content` 小于或等于 `threshold` 字节，
/// 返回 `None`，调用者保留内联内容。超过阈值时，将完整内容写入溢出文件并返回
/// `Some((head, path))`，其中 `head` 是调用者可以内联显示的前导切片。
/// 尾部不返回——`path` 是规范引用。
///
/// `head_bytes` 控制调用者希望保留的内联内容量。
/// 传入 `threshold` 表示"尽可能多地内联"，传入较小值（如 `4 * 1024`）表示"显示预览"。
pub fn maybe_spillover(
    id: &str,
    content: &str,
    threshold: usize,
    head_bytes: usize,
) -> io::Result<Option<(String, PathBuf)>> {
    if content.len() <= threshold {
        return Ok(None);
    }
    let path = write_spillover(id, content)?;
    // Don't slice mid-utf8: walk back to a char boundary if needed.
    let cut = head_bytes.min(content.len());
    let cut = (0..=cut)
        .rev()
        .find(|&i| content.is_char_boundary(i))
        .unwrap_or(0);
    Ok(Some((content[..cut].to_string(), path)))
}

/// 当 [`apply_spillover`] 截断工具结果时保留的内联头部。
/// 32 KiB 足够模型保留有意义的上下文（长堆栈跟踪、`git diff` 头部、
/// 典型深度的目录列表），而不会消耗每轮上下文预算的大部分。
/// 完整输出保存在磁盘上；模型在需要尾部时可以通过 `read_file` 读回。
pub const SPILLOVER_HEAD_BYTES: usize = 32 * 1024;

/// 就地应用溢出到工具结果。如果结果的内容超过 [`SPILLOVER_THRESHOLD_BYTES`]，
/// 将完整内容写入 `~/.deepseek/tool_outputs/` 下的姐妹文件，将 `result.content`
/// 替换为 [`SPILLOVER_HEAD_BYTES`] 大小的头部加上指向溢出文件的页脚，
/// 并标记 `metadata.spillover_path` 以便 UI 渲染其"完整输出：…"注释。
///
/// 成功时返回溢出路径，如果没有溢出发生则返回 `None`（内容足够小、错误结果、
/// 写入失败）。失败会被记录但不会冒泡——生成结果的工具不应因为溢出写入器无法
/// 写入磁盘而被标记为失败；我们降级为无操作，模型将获得原始的（大）内容。
///
/// 错误结果（`success == false`）会被跳过：错误消息通常很短，将其转为"查看文件"
/// 指针只会向模型隐藏错误信息。
pub fn apply_spillover(result: &mut ToolResult, tool_id: &str) -> Option<PathBuf> {
    if !result.success {
        return None;
    }
    if result.content.len() <= SPILLOVER_THRESHOLD_BYTES {
        return None;
    }
    let total = result.content.len();
    let outcome = match maybe_spillover(
        tool_id,
        &result.content,
        SPILLOVER_THRESHOLD_BYTES,
        SPILLOVER_HEAD_BYTES,
    ) {
        Ok(Some(pair)) => pair,
        Ok(None) => return None,
        Err(err) => {
            tracing::warn!(
                target: "spillover",
                ?err,
                tool_id,
                "spillover write failed; passing original content through"
            );
            return None;
        }
    };
    let (head, path) = outcome;
    let path_str = path.display().to_string();
    let footer = format!(
        "\n\n[Output truncated: {head_kib} KiB of {total_kib} KiB shown. \
         Full output saved to {path_str}. Use \
         `retrieve_tool_result ref={tool_id} mode=tail` or \
         `retrieve_tool_result ref={tool_id} mode=query query=<text>` \
         if you need the elided output.]",
        head_kib = head.len() / 1024,
        total_kib = total / 1024,
    );
    result.content = format!("{head}{footer}");
    let metadata = result.metadata.get_or_insert_with(|| serde_json::json!({}));
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("spillover_path".into(), serde_json::Value::String(path_str));
    } else {
        // Pre-existing metadata that wasn't a JSON object (rare,
        // possibly an array). Replace with an object so we can
        // attach our key without losing prior data — wrap it under
        // a `_prior` field so callers that introspect can recover.
        let prior = std::mem::replace(metadata, serde_json::json!({}));
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert("_prior".into(), prior);
            obj.insert(
                "spillover_path".into(),
                serde_json::Value::String(path.display().to_string()),
            );
        }
    }
    Some(path)
}

/// 清理工具调用 id 以用作文件名。保留 ASCII 字母数字、`-` 和 `_`；
/// 拒绝 `.` 以防止 `..` 遍历，拒绝空结果。如果输入中不包含可接受的字符则返回 `None`。
fn sanitise_id(id: &str) -> Option<String> {
    let cleaned: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// 为测试覆盖溢出根目录，以免污染用户的真实 `~/.deepseek/` 目录。
/// 使用临时 `HOME` 覆盖来包装主体，该覆盖在 drop 时恢复。
#[cfg(test)]
fn with_test_home<F, R>(home: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    // SAFETY: tests in this module serialize through `TEST_GUARD`
    // because they share process-wide `$HOME`. Without the guard,
    // parallel tests could observe each other's overrides.
    let prior = std::env::var_os("HOME");
    // SAFETY: caller holds the test guard.
    unsafe {
        std::env::set_var("HOME", home);
    }
    let out = f();
    // SAFETY: caller holds the test guard.
    unsafe {
        if let Some(p) = prior {
            std::env::set_var("HOME", p);
        } else {
            std::env::remove_var("HOME");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Tests in this module serialize through this guard because
    /// they mutate process-global `$HOME`. Without it, cargo's
    /// parallel runner would observe interleaved overrides.
    fn setup() -> std::sync::MutexGuard<'static, ()> {
        super::TEST_SPILLOVER_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn sanitise_id_keeps_safe_chars_and_drops_dangerous() {
        assert_eq!(super::sanitise_id("abc-123_x"), Some("abc-123_x".into()));
        // `.` is dropped to keep `..` out of the path.
        assert_eq!(super::sanitise_id("../etc"), Some("etc".into()));
        assert_eq!(super::sanitise_id("/etc/passwd"), Some("etcpasswd".into()));
        // Empty-after-sanitise → None.
        assert!(super::sanitise_id("...").is_none());
        assert!(super::sanitise_id("").is_none());
    }

    #[test]
    fn write_spillover_creates_directory_and_writes_file() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            let path = write_spillover("call-abc", "hello world").expect("write");
            assert!(path.exists(), "{path:?} missing");
            let body = fs::read_to_string(&path).unwrap();
            assert_eq!(body, "hello world");
            // Directory landed under `<HOME>/.deepseek/tool_outputs/`.
            // Compare components instead of a substring on `to_string_lossy`
            // — Windows uses `\` as the separator so a `/` substring match
            // would falsely fail there.
            let components: Vec<&str> = path
                .components()
                .filter_map(|c| c.as_os_str().to_str())
                .collect();
            assert!(
                components.contains(&".deepseek") && components.contains(&"tool_outputs"),
                "spillover path missing expected `.deepseek/tool_outputs/...` segments: {path:?}"
            );
        });
    }

    #[test]
    fn write_spillover_rejects_empty_id() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            let err = write_spillover("...", "x").unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        });
    }

    #[test]
    fn maybe_spillover_returns_none_below_threshold() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            let out = maybe_spillover("call-1", "tiny content", 100 * 1024, 4 * 1024).expect("ok");
            assert!(out.is_none());
        });
    }

    #[test]
    fn maybe_spillover_writes_and_returns_head_above_threshold() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            // Content larger than the threshold.
            let big = "A".repeat(2_000);
            let (head, path) = maybe_spillover("call-2", &big, 1_000, 256)
                .expect("ok")
                .expect("should have spilled");
            // Head is bounded.
            assert_eq!(head.len(), 256);
            // Full content on disk.
            let body = fs::read_to_string(&path).unwrap();
            assert_eq!(body.len(), 2_000);
        });
    }

    #[test]
    fn maybe_spillover_does_not_split_inside_a_codepoint() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            // 4 byte chars; ask for 3 bytes of head → walks back to
            // the previous char boundary (0).
            let s = "🐳🐳🐳🐳"; // 4 × 4-byte codepoints
            assert_eq!(s.len(), 16);
            let (head, _) = maybe_spillover("call-3", s, 1, 3)
                .expect("ok")
                .expect("spilled");
            // 3 isn't a char boundary in this string; walk back → 0.
            assert_eq!(head, "");
            // Asking for 4 bytes lands on the first char boundary.
            let (head, _) = maybe_spillover("call-3b", s, 1, 4)
                .expect("ok")
                .expect("spilled");
            assert_eq!(head, "🐳");
        });
    }

    #[test]
    fn prune_older_than_handles_missing_root() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            // Nothing has ever written; root doesn't exist; that's fine.
            let count = prune_older_than(SPILLOVER_MAX_AGE).expect("ok");
            assert_eq!(count, 0);
        });
    }

    // The mtime backdate uses utimensat (Unix-only). On Windows the
    // filetime_set_modified helper is a no-op, so the prune wouldn't see
    // any stale files. Gate the whole test on `cfg(unix)` instead of
    // testing a no-op path that can't fail meaningfully.
    #[test]
    #[cfg(unix)]
    fn prune_older_than_keeps_fresh_files_drops_stale_ones() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            let fresh = write_spillover("fresh", "x").unwrap();
            let stale = write_spillover("stale", "y").unwrap();

            // Backdate `stale` to 30 days ago.
            let thirty_days = SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60);
            filetime_set_modified(&stale, thirty_days);

            let pruned = prune_older_than(SPILLOVER_MAX_AGE).unwrap();
            assert_eq!(pruned, 1);
            assert!(fresh.exists());
            assert!(!stale.exists());
        });
    }

    /// Set the mtime on a file. The workspace doesn't pull the
    /// `filetime` crate, so we reach for `utimensat` directly on
    /// Unix. Windows is a no-op — the prune semantics are the same
    /// and the per-cycle stress test lives on the Unix path.
    #[cfg(unix)]
    fn filetime_set_modified(path: &Path, when: SystemTime) {
        let secs = when
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as libc::time_t;
        let times = [
            libc::timespec {
                tv_sec: secs,
                tv_nsec: 0,
            },
            libc::timespec {
                tv_sec: secs,
                tv_nsec: 0,
            },
        ];
        let path_c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: path_c is a valid CString; times is a 2-element array
        // matching utimensat's signature.
        let rc = unsafe { libc::utimensat(libc::AT_FDCWD, path_c.as_ptr(), times.as_ptr(), 0) };
        assert_eq!(
            rc,
            0,
            "utimensat failed: {}",
            std::io::Error::last_os_error()
        );
    }

    // Windows stub removed in v0.8.8 — the only caller of
    // `filetime_set_modified` is `prune_older_than_keeps_fresh_files_drops_stale_ones`,
    // which is now `#[cfg(unix)]` because mtime backdating requires
    // `utimensat` and a Windows no-op stub can't make the assertion pass
    // anyway. Keeping the stub triggered `-D dead-code` on Windows builds
    // (the prune test was the only caller) and broke `Test (windows-latest)`.

    #[test]
    fn apply_spillover_is_noop_below_threshold() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            let mut result = ToolResult::success("small payload");
            let path = apply_spillover(&mut result, "call-small");
            assert!(path.is_none());
            assert_eq!(result.content, "small payload");
            assert!(result.metadata.is_none());
        });
    }

    #[test]
    fn apply_spillover_is_noop_for_error_results() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            // Even very large error messages are passed through —
            // truncating an error would hide it from the model.
            let big_err = "boom\n".repeat(50_000);
            let mut result = ToolResult::error(big_err.clone());
            let path = apply_spillover(&mut result, "call-err");
            assert!(path.is_none());
            assert_eq!(result.content, big_err);
        });
    }

    #[test]
    fn apply_spillover_truncates_and_stamps_metadata_above_threshold() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            // 200 KiB body — well above the 100 KiB threshold.
            let big = "X".repeat(200 * 1024);
            let mut result = ToolResult::success(big.clone());
            let path = apply_spillover(&mut result, "call-big").expect("should spill");

            // Inline content shrunk to head + footer.
            assert!(result.content.len() < big.len());
            assert!(
                result.content.contains("Output truncated:"),
                "footer missing: {}",
                &result.content[result.content.len().saturating_sub(200)..]
            );
            assert!(result.content.contains("retrieve_tool_result ref=call-big"));

            // Full bytes are on disk at the returned path.
            assert!(path.exists(), "spillover file missing: {path:?}");
            let body = fs::read_to_string(&path).unwrap();
            assert_eq!(body.len(), 200 * 1024);

            // metadata.spillover_path stamped for the UI to find.
            let metadata = result.metadata.expect("metadata stamped");
            let stamped = metadata
                .get("spillover_path")
                .and_then(serde_json::Value::as_str)
                .expect("spillover_path key present");
            assert_eq!(stamped, path.display().to_string());
        });
    }

    #[test]
    fn apply_spillover_preserves_existing_metadata() {
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            let big = "Y".repeat(200 * 1024);
            let mut result = ToolResult::success(big)
                .with_metadata(serde_json::json!({"prior_key": "prior_value"}));
            let path = apply_spillover(&mut result, "call-meta").expect("should spill");

            let metadata = result.metadata.expect("metadata present");
            // Prior keys survive.
            assert_eq!(
                metadata
                    .get("prior_key")
                    .and_then(serde_json::Value::as_str),
                Some("prior_value")
            );
            // New key added alongside.
            assert_eq!(
                metadata
                    .get("spillover_path")
                    .and_then(serde_json::Value::as_str),
                Some(path.display().to_string().as_str())
            );
        });
    }

    #[test]
    fn apply_spillover_wraps_non_object_metadata_under_prior_key() {
        // Defends against a tool whose `metadata` is something
        // other than a JSON object (rare — most use the `json!({})`
        // pattern — but legal per `serde_json::Value`). The
        // spillover writer must add `spillover_path` without losing
        // the prior payload.
        let _g = setup();
        let tmp = tempdir().unwrap();
        with_test_home(tmp.path(), || {
            let big = "Z".repeat(200 * 1024);
            let mut result = ToolResult::success(big).with_metadata(serde_json::json!([
                "unexpected",
                "array",
                "payload"
            ]));
            let path = apply_spillover(&mut result, "call-arr").expect("should spill");

            let metadata = result.metadata.expect("metadata stamped");
            // Prior payload re-homed under `_prior`.
            let prior = metadata.get("_prior").expect("_prior wrap key present");
            assert_eq!(
                prior,
                &serde_json::json!(["unexpected", "array", "payload"]),
                "prior array should round-trip under _prior"
            );
            // New key alongside.
            assert_eq!(
                metadata
                    .get("spillover_path")
                    .and_then(serde_json::Value::as_str),
                Some(path.display().to_string().as_str())
            );
        });
    }
}
