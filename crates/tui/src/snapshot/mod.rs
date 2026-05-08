//! 工作区快照 — 轮次前后的安全网。
//!
//! 每个轮次，引擎在 `~/.deepseek/snapshots/<project_hash>/<worktree_hash>/.git`
//! 的侧边 git 仓库中拍摄用户工作区的 `pre-turn:<seq>` 快照，然后
//! 在轮次完成时拍摄对应的 `post-turn:<seq>` 快照。用户可以通过
//! `/restore N`（斜杠命令）或在模型识别到"撤销我上次编辑"意图时
//! 通过 `revert_turn` 工具来回滚。
//!
//! ## 为什么使用侧仓库？
//!
//! - 用户的 `.git` 永远不会被触及。当我们调用 git 时，
//!   `--git-dir` 和 `--work-tree` *始终*一起设置；
//!   这一不变性保证了快照和用户仓库完全独立。
//! - 没有 git 的工作区仍然可以获得快照。
//! - `git` 自身的去重（对象包文件）使磁盘占用可控 —
//!   典型的 100 MB 工作区 × 12 轮次 ≈ 1.2 GB 未压缩，
//!   但 git 的内容寻址存储通常将其降低 10-30 倍。我们进一步缓解：
//!     - 7 天默认保留期（`session_manager` 在会话开始时通过
//!       [`prune::prune_older_than`] 进行修剪）。
//!     - 侧仓库上设置 `gc.auto = 0`（我们不希望后台 gc 在轮次中触发）
//!       加上修剪后的显式 `git gc --prune=now`。
//!     - 启动时清理中断的 git 包操作留下的过时 `tmp_pack_*` 文件。
//!
//! ## 失败模型
//!
//! 轮次前/后的快照调用是**非致命的**。如果 `git` 缺失、磁盘已满、
//! 或工作区位于只读文件系统上，轮次继续进行，引擎记录警告。
//! 快照是安全网，不是正确性门控。

pub mod paths;
pub mod prune;
pub mod repo;

#[allow(unused_imports)]
pub use paths::{snapshot_dir_for, snapshot_git_dir};
pub use prune::{DEFAULT_MAX_AGE, prune_older_than};
#[allow(unused_imports)]
pub use repo::{Snapshot, SnapshotId, SnapshotRepo};
