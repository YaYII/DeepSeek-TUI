#![allow(dead_code)]
//! 审批缓存 — 缓存工具审批决策。
//!
//! 不是仅按工具名称缓存（那样会让已批准的 `exec_shell "cat foo"` 静默通过
//! `exec_shell "rm -rf /"`），而是使用**调用指纹**作为缓存键——即工具名称
//! 及其参数的语义相关部分的摘要。
//!
//! ## 指纹形状
//!
//! | 工具           | 键                                        |
//! |---------------|------------------------------------------|
//! | `apply_patch`  | `patch:<文件路径的哈希>`                    |
//! | `exec_shell`   | `shell:<命令前缀（前 3 个令牌）>`           |
//! | `fetch_url`    | `net:<主机名>`                            |
//! | 其他           | `tool:<工具名称>`                         |
//!
//! 缓存是**会话键控的**：条目带有一个 `ApprovedForSession` 标志。当为 true 时，
//! 该批准将在会话的剩余时间内重用；当为 false 时，是一次性授权（后续具有相同
//! 指纹的调用仍会提示）。

use std::collections::HashMap;
use std::time::Instant;

use crate::command_safety::classify_command;

/// 工具调用的指纹——足够稳定以匹配重复调用，但又足够具体以避免权限混淆。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApprovalKey(pub String);

/// 先前作出的批准决策的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalCacheStatus {
    /// 调用指纹匹配且会话级标志指示可重用。
    Approved,
    /// 调用指纹匹配但授权是一次性的（已消耗）。
    Denied,
    /// 不匹配——需要重新批准。
    Unknown,
}

/// 单个缓存条目。
#[derive(Debug, Clone)]
struct ApprovalCacheEntry {
    /// 此条目创建的时间。
    created: Instant,
    /// 批准是否应在整个会话中重用。
    approved_for_session: bool,
}

/// 一个由工具调用指纹支持的批准缓存。
#[derive(Debug, Default)]
pub struct ApprovalCache {
    entries: HashMap<ApprovalKey, ApprovalCacheEntry>,
}

impl ApprovalCache {
    /// 构造一个空缓存。
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// 查找先前作出的批准决策。
    pub fn check(&self, key: &ApprovalKey) -> ApprovalCacheStatus {
        let Some(entry) = self.entries.get(key) else {
            return ApprovalCacheStatus::Unknown;
        };
        if entry.approved_for_session {
            ApprovalCacheStatus::Approved
        } else {
            ApprovalCacheStatus::Denied
        }
    }

    /// 在给定指纹下记录批准决策。
    ///
    /// 当 `approved_for_session` 为 true 时，后续具有相同键的调用将
    /// 在会话的剩余时间内自动批准。
    pub fn insert(&mut self, key: ApprovalKey, approved_for_session: bool) {
        self.entries.insert(
            key,
            ApprovalCacheEntry {
                created: Instant::now(),
                approved_for_session,
            },
        );
    }

    /// 清除所有条目。
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 缓存条目数量。
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 缓存是否为空。
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Fingerprint helpers ────────────────────────────────────────────

/// 构建工具调用的批准缓存键。
///
/// 该键包含工具名称和参数的有损摘要，使缓存能够区分
/// `exec_shell "ls"` 和 `exec_shell "rm -rf /"`，同时仍然识别
/// 相同无害命令的重复调用。
#[must_use]
pub fn build_approval_key(tool_name: &str, input: &serde_json::Value) -> ApprovalKey {
    let fingerprint = match tool_name {
        "apply_patch" => {
            let paths_hash = hash_patch_paths(input);
            format!("patch:{paths_hash}")
        }
        "exec_shell"
        | "exec_shell_wait"
        | "exec_shell_interact"
        | "exec_wait"
        | "exec_interact" => {
            let prefix = command_prefix(input);
            format!("shell:{prefix}")
        }
        "fetch_url" | "web.fetch" | "web_fetch" => {
            let host = parse_host(input);
            format!("net:{host}")
        }
        _ => format!("tool:{tool_name}"),
    };
    ApprovalKey(fingerprint)
}

/// 返回 `input` 中 shell 命令的规范命令前缀。
///
/// 使用 arity 字典中的 [`classify_command`]，以便 `auto_allow = ["git status"]`
/// 正确匹配 `git status -s` 和 `git status --porcelain`，但不匹配 `git push`。
fn command_prefix(input: &serde_json::Value) -> String {
    let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    if tokens.is_empty() {
        return "<empty>".to_string();
    }
    classify_command(&tokens)
}

/// 对补丁输入中引用的排序后的文件路径集合进行哈希。
fn hash_patch_paths(input: &serde_json::Value) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut paths: Vec<&str> = Vec::new();

    if let Some(changes) = input.get("changes").and_then(|v| v.as_array()) {
        for change in changes {
            if let Some(path) = change.get("path").and_then(|v| v.as_str()) {
                paths.push(path);
            }
        }
    } else if let Some(patch_text) = input.get("patch").and_then(|v| v.as_str()) {
        for line in patch_text.lines() {
            if let Some(rest) = line.strip_prefix("+++ b/") {
                paths.push(rest.trim());
            }
        }
    }

    paths.sort();
    paths.dedup();

    if paths.is_empty() {
        return "no_files".to_string();
    }

    let mut hasher = DefaultHasher::new();
    for path in &paths {
        path.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

/// 从 URL 输入中解析主机部分。
fn parse_host(input: &serde_json::Value) -> String {
    let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");

    if let Ok(parsed) = reqwest::Url::parse(url) {
        parsed.host_str().unwrap_or(url).to_string()
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cache_hit_returns_approved_for_session() {
        let mut cache = ApprovalCache::new();
        let key = build_approval_key("exec_shell", &json!({"command": "ls -la"}));
        cache.insert(key.clone(), true);
        assert_eq!(cache.check(&key), ApprovalCacheStatus::Approved);
    }

    #[test]
    fn cache_one_shot_is_not_reused() {
        let mut cache = ApprovalCache::new();
        let key = build_approval_key("exec_shell", &json!({"command": "cargo build"}));
        cache.insert(key.clone(), false);
        assert_eq!(cache.check(&key), ApprovalCacheStatus::Denied);
    }

    #[test]
    fn cache_miss_is_unknown() {
        let cache = ApprovalCache::new();
        let key = build_approval_key("exec_shell", &json!({"command": "ls"}));
        assert_eq!(cache.check(&key), ApprovalCacheStatus::Unknown);
    }

    #[test]
    fn different_commands_different_keys() {
        let key_a = build_approval_key("exec_shell", &json!({"command": "ls"}));
        let key_b = build_approval_key("exec_shell", &json!({"command": "rm -rf /tmp"}));
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn same_command_same_key() {
        let key_a = build_approval_key("exec_shell", &json!({"command": "cargo build --release"}));
        let key_b = build_approval_key("exec_shell", &json!({"command": "cargo build --release"}));
        assert_eq!(key_a, key_b);
    }

    #[test]
    fn command_prefix_drops_flags() {
        let key_a = build_approval_key("exec_shell", &json!({"command": "cargo build"}));
        let key_b = build_approval_key("exec_shell", &json!({"command": "cargo build --release"}));
        assert_eq!(key_a, key_b);
    }

    #[test]
    fn patch_keys_differ_by_path() {
        let key_a = build_approval_key(
            "apply_patch",
            &json!({"changes": [{"path": "a.rs", "content": "x"}]}),
        );
        let key_b = build_approval_key(
            "apply_patch",
            &json!({"changes": [{"path": "b.rs", "content": "x"}]}),
        );
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn net_keys_differ_by_host() {
        let key_a = build_approval_key("fetch_url", &json!({"url": "https://example.com"}));
        let key_b = build_approval_key("fetch_url", &json!({"url": "https://other.org"}));
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn generic_tool_uses_tool_name() {
        let key_a = build_approval_key("read_file", &json!({"path": "a.txt"}));
        let key_b = build_approval_key("read_file", &json!({"path": "b.txt"}));
        assert_eq!(key_a, key_b);
        assert_eq!(key_a.0, "tool:read_file");
    }
}
