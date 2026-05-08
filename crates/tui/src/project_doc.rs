//! 项目文档发现和加载
//!
//! 支持类似 Claude Code 的项目指令自动发现。
//! 优先级：AGENTS.md > .claude/instructions.md > CLAUDE.md > .deepseek/instructions.md

use std::path::{Path, PathBuf};

/// 要搜索的文档文件名（按优先级排序）
pub const DOC_FILENAMES: &[&str] = &[
    "AGENTS.md",
    ".claude/instructions.md",
    "CLAUDE.md",
    ".deepseek/instructions.md",
];

/// 从项目文档读取的最大字节数（默认：32KB）
#[allow(dead_code)] // 由 read_project_docs 使用
pub const DEFAULT_MAX_BYTES: usize = 32768;

/// 已发现的项目文档
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProjectDoc {
    pub path: PathBuf,
    pub content: String,
}

/// 从当前工作目录向上遍历到 git 根目录，收集所有项目文档
pub fn discover_paths(cwd: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let git_root = find_git_root(cwd);

    let mut current = cwd.to_path_buf();
    loop {
        for filename in DOC_FILENAMES {
            let doc_path = current.join(filename);
            if doc_path.exists() && doc_path.is_file() {
                paths.push(doc_path);
            }
        }

        // 在 git 根目录或文件系统根目录停止
        if let Some(ref root) = git_root
            && current == *root
        {
            break;
        }

        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => break,
        }
    }

    // 反转，使父文档优先（将被子文档覆盖）
    paths.reverse();
    paths
}

/// 从当前工作目录查找 git 根目录
fn find_git_root(cwd: &Path) -> Option<PathBuf> {
    let mut current = cwd.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => return None,
        }
    }
}

/// 读取并连接项目文档，带有字节限制
#[allow(dead_code)] // 公共 API；project_context.rs 提供活动代码路径
pub fn read_project_docs(paths: &[PathBuf], max_bytes: usize) -> Option<String> {
    if paths.is_empty() {
        return None;
    }

    let mut combined = String::new();
    let mut total_bytes = 0;

    for path in paths {
        if total_bytes >= max_bytes {
            break;
        }

        if let Ok(content) = std::fs::read_to_string(path) {
            let remaining = max_bytes.saturating_sub(total_bytes);
            let content = if content.len() > remaining {
                // 如果可能，在单词边界截断到剩余字节数
                let truncated: String = content.chars().take(remaining).collect();
                format!("{truncated}\n\n[...truncated...]")
            } else {
                content
            };

            if !combined.is_empty() {
                combined.push_str("\n\n---\n\n");
            }
            combined.push_str(&format_instructions(path, &content));
            total_bytes += content.len();
        }
    }

    if combined.is_empty() {
        None
    } else {
        Some(combined)
    }
}

/// 格式化项目指令以注入系统提示词
#[allow(dead_code)] // 由 read_project_docs 使用
pub fn format_instructions(path: &Path, content: &str) -> String {
    format!(
        "# Project instructions from {}\n\n<INSTRUCTIONS>\n{}\n</INSTRUCTIONS>",
        path.display(),
        content.trim()
    )
}

/// 使用默认设置从工作区加载项目文档
#[allow(dead_code)] // 便利函数；project_context.rs 提供活动代码路径
pub fn load_from_workspace(workspace: &Path) -> Option<String> {
    let paths = discover_paths(workspace);
    read_project_docs(&paths, DEFAULT_MAX_BYTES)
}
