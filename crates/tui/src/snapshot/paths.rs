//! 每个工作区快照侧仓库的路径解析。
//!
//! 快照存放在 `~/.deepseek/snapshots/<project_hash>/<worktree_hash>/`。
//! 两级哈希拆分使我们能够独立快照同一项目的多个工作树 —
//! `git worktree list` 用户不会在功能分支之间产生串扰。

use std::io;
use std::path::{Path, PathBuf};

/// 计算给定工作区路径的快照目录。
///
/// 返回 `~/.deepseek/snapshots/<project_hash>/<worktree_hash>/`。
/// 调用者负责在磁盘上创建它；我们特意不在此处接触文件系统，
/// 因此重复调用此函数成本低廉。
///
/// `project_hash` 派生自规范化后的工作区路径，去除任何
/// `.worktrees/<name>` 后缀 — 同一仓库的多个工作树共享相同的
/// `project_hash`，因此用户可以在需要时跨工作树浏览快照，
/// 但 `worktree_hash` 默认保持提交隔离。
pub fn snapshot_dir_for(workspace: &Path) -> PathBuf {
    snapshot_dir_with_home(workspace, dirs::home_dir())
}

/// 与 [`snapshot_dir_for`] 相同，但带有可注入的家目录。
/// 由测试使用，以便我们从不接触用户真实的 `~/.deepseek/`。
pub fn snapshot_dir_with_home(workspace: &Path, home: Option<PathBuf>) -> PathBuf {
    let home = home.unwrap_or_else(|| PathBuf::from("."));
    let canonical = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let project_root = strip_worktree_suffix(&canonical);
    let project_hash = stable_hex(&project_root);
    let worktree_hash = stable_hex(&canonical);
    home.join(".deepseek")
        .join("snapshots")
        .join(project_hash)
        .join(worktree_hash)
}

/// 解析快照目录内的 `.git` 目录。
pub fn snapshot_git_dir(workspace: &Path) -> PathBuf {
    snapshot_dir_for(workspace).join(".git")
}

/// 确保快照目录在磁盘上存在并返回其路径。
pub fn ensure_snapshot_dir(workspace: &Path) -> io::Result<PathBuf> {
    let dir = snapshot_dir_for(workspace);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 去除尾部的 `.worktrees/<name>` 段，使同一检出的所有工作树
/// 共享一个 `project_hash`。如果路径看起来不像工作树，则原样返回。
fn strip_worktree_suffix(path: &Path) -> PathBuf {
    let mut components: Vec<_> = path.components().collect();
    if components.len() >= 2
        && let Some(parent) = components.get(components.len() - 2)
        && parent.as_os_str() == ".worktrees"
    {
        components.truncate(components.len() - 2);
        let mut p = PathBuf::new();
        for c in components {
            p.push(c.as_os_str());
        }
        return p;
    }
    path.to_path_buf()
}

/// Hex-encoded deterministic FNV-1a digest. This is only a directory tag, not
/// a security boundary, but it must remain stable across process launches.
fn stable_hex(path: &Path) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn snapshot_dir_layout_two_levels_under_deepseek() {
        let tmp = tempdir().expect("tempdir");
        let dir = snapshot_dir_with_home(tmp.path(), Some(tmp.path().to_path_buf()));
        let mut iter = dir.strip_prefix(tmp.path()).unwrap().components();
        assert_eq!(iter.next().unwrap().as_os_str(), ".deepseek");
        assert_eq!(iter.next().unwrap().as_os_str(), "snapshots");
        assert!(iter.next().is_some()); // project_hash
        assert!(iter.next().is_some()); // worktree_hash
        assert!(iter.next().is_none());
    }

    #[test]
    fn worktree_suffix_stripped_for_project_hash() {
        let tmp = tempdir().expect("tempdir");
        let main_path = tmp.path().join("repo");
        let wt_path = tmp.path().join("repo").join(".worktrees").join("featX");
        std::fs::create_dir_all(&main_path).unwrap();
        std::fs::create_dir_all(&wt_path).unwrap();

        let main_dir = snapshot_dir_with_home(&main_path, Some(tmp.path().to_path_buf()));
        let wt_dir = snapshot_dir_with_home(&wt_path, Some(tmp.path().to_path_buf()));

        // Same project_hash (parent component before the worktree-specific tail).
        let main_components: Vec<_> = main_dir.components().collect();
        let wt_components: Vec<_> = wt_dir.components().collect();
        assert_eq!(
            main_components[main_components.len() - 2],
            wt_components[wt_components.len() - 2],
            "worktrees should share project_hash",
        );
        // But different worktree_hash (the tail).
        assert_ne!(main_components.last(), wt_components.last());
    }

    #[test]
    fn ensure_snapshot_dir_creates_path() {
        let tmp = tempdir().expect("tempdir");
        // Use scoped HOME so we don't pollute the real one.
        let dir = snapshot_dir_with_home(tmp.path(), Some(tmp.path().to_path_buf()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(dir.exists());
    }

    #[test]
    fn snapshot_git_dir_appends_dot_git() {
        let tmp = tempdir().expect("tempdir");
        let git_dir = snapshot_git_dir(tmp.path());
        assert_eq!(git_dir.file_name().unwrap(), ".git");
    }
}
