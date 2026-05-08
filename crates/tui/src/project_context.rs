//! DeepSeek TUI 的项目上下文加载。
//!
//! 本模块处理加载项目特定的上下文文件，为 AI 代理提供
//! 指令和上下文。包括：
//!
//! - `AGENTS.md` - 项目级代理指令（主要）
//! - `.claude/instructions.md` - Claude 风格的隐藏指令
//! - `CLAUDE.md` - Claude 风格的指令
//! - `.deepseek/instructions.md` - 隐藏指令文件（旧版）
//!
//! 加载的内容被注入系统提示词，以向代理提供关于项目约定、结构和要求的上下文。

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// 要查找的项目上下文文件名，按优先级排序。
const PROJECT_CONTEXT_FILES: &[&str] = &[
    "AGENTS.md",
    ".claude/instructions.md",
    "CLAUDE.md",
    ".deepseek/instructions.md",
];

/// 项目上下文文件的最大大小（防止加载巨大文件）
const MAX_CONTEXT_SIZE: usize = 100 * 1024; // 100KB

// === 错误类型 ===

#[derive(Debug, Error)]
enum ProjectContextError {
    #[error("Failed to read context metadata for {path}: {source}")]
    Metadata {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Context file {path} is too large ({size} bytes, max {max})")]
    TooLarge {
        path: PathBuf,
        size: u64,
        max: usize,
    },
    #[error("Failed to read context file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Context file {path} is empty")]
    Empty { path: PathBuf },
}

/// 加载项目上下文的结果
#[derive(Debug, Clone)]
pub struct ProjectContext {
    /// 已加载的指令内容
    pub instructions: Option<String>,
    /// 已加载文件的路径（用于显示）
    pub source_path: Option<PathBuf>,
    /// 加载过程中的任何警告
    pub warnings: Vec<String>,
    /// 项目根目录
    #[allow(dead_code)] // 属于 ProjectContext 公共接口的一部分
    pub project_root: PathBuf,
    /// 是否为受信任的项目
    pub is_trusted: bool,
}

impl ProjectContext {
    /// 创建一个空的项目上下文
    pub fn empty(project_root: PathBuf) -> Self {
        Self {
            instructions: None,
            source_path: None,
            warnings: Vec::new(),
            project_root,
            is_trusted: false,
        }
    }

    /// 检查是否加载了任何指令
    pub fn has_instructions(&self) -> bool {
        self.instructions.is_some()
    }

    /// 获取格式化的指令块，用于系统提示词
    pub fn as_system_block(&self) -> Option<String> {
        self.instructions.as_ref().map(|content| {
            let source = self
                .source_path
                .as_ref()
                .map_or_else(|| "project".to_string(), |p| p.display().to_string());

            format!(
                "<project_instructions source=\"{source}\">\n{content}\n</project_instructions>"
            )
        })
    }
}

/// 从工作区目录加载项目上下文。
///
/// 搜索已知的项目上下文文件并加载找到的第一个文件。
pub fn load_project_context(workspace: &Path) -> ProjectContext {
    let mut ctx = ProjectContext::empty(workspace.to_path_buf());

    // 搜索项目上下文文件
    for filename in PROJECT_CONTEXT_FILES {
        let file_path = workspace.join(filename);

        if file_path.exists() && file_path.is_file() {
            match load_context_file(&file_path) {
                Ok(content) => {
                    ctx.instructions = Some(content);
                    ctx.source_path = Some(file_path);
                    break;
                }
                Err(error) => {
                    ctx.warnings.push(error.to_string());
                }
            }
        }
    }

    // 检查信任文件
    ctx.is_trusted = check_trust_status(workspace);

    ctx
}

/// 也从父目录加载项目上下文。
///
/// 这允许在根目录 AGENTS.md 应用于所有子目录的单仓库设置。
pub fn load_project_context_with_parents(workspace: &Path) -> ProjectContext {
    let mut ctx = load_project_context(workspace);

    // 如果工作区未找到上下文，则检查父目录
    if !ctx.has_instructions() {
        let mut current = workspace.parent();

        while let Some(parent) = current {
            let parent_ctx = load_project_context(parent);
            ctx.warnings.extend(parent_ctx.warnings.iter().cloned());
            if parent_ctx.has_instructions() {
                ctx.instructions = parent_ctx.instructions;
                ctx.source_path = parent_ctx.source_path;
                break;
            }

            current = parent.parent();
        }
    }

    // 当任何位置都不存在上下文文件时，自动生成 .deepseek/instructions.md。
    // 这避免了 prompts.rs 中每轮文件系统扫描的回退，
    // 该回退会破坏 KV 前缀缓存的稳定性。
    if !ctx.has_instructions()
        && let Some(generated) = auto_generate_context(workspace)
    {
        ctx = load_project_context(workspace);
        if !ctx.has_instructions() {
            // 从刚写入的文件加载 — 使用生成的内容
            // 作为最后手段（正常情况下不应发生）。
            ctx.instructions = Some(generated);
            ctx.source_path = None;
        }
    }

    ctx
}

/// 从项目树和摘要生成上下文文件并写入
/// `.deepseek/instructions.md`。成功时返回生成的内容。
fn auto_generate_context(workspace: &Path) -> Option<String> {
    let deepseek_dir = workspace.join(".deepseek");
    let instructions_path = deepseek_dir.join("instructions.md");

    // 不要覆盖已存在的文件
    if instructions_path.exists() {
        return None;
    }

    let summary = crate::utils::summarize_project(workspace);
    let tree = crate::utils::project_tree(workspace, 2);

    let content = format!(
        "# Project Structure (Auto-generated)\n\n\
         > This file was automatically generated by DeepSeek TUI.\n\
         > You can edit or delete it at any time.\n\n\
         **Summary:** {summary}\n\n\
         **Tree:**\n```\n{tree}\n```"
    );

    // 如果需要，创建 .deepseek/ 目录
    if let Err(e) = std::fs::create_dir_all(&deepseek_dir) {
        tracing::warn!("Failed to create .deepseek/ directory: {e}");
        return None;
    }

    match std::fs::write(&instructions_path, &content) {
        Ok(()) => {
            tracing::info!("Auto-generated {}", instructions_path.display());
            Some(content)
        }
        Err(e) => {
            tracing::warn!("Failed to write {}: {e}", instructions_path.display());
            None
        }
    }
}

/// 加载上下文文件并进行大小检查
fn load_context_file(path: &Path) -> Result<String, ProjectContextError> {
    // 首先检查文件大小
    let metadata = fs::metadata(path).map_err(|source| ProjectContextError::Metadata {
        path: path.to_path_buf(),
        source,
    })?;

    if metadata.len() > MAX_CONTEXT_SIZE as u64 {
        return Err(ProjectContextError::TooLarge {
            path: path.to_path_buf(),
            size: metadata.len(),
            max: MAX_CONTEXT_SIZE,
        });
    }

    // 读取文件
    let content = fs::read_to_string(path).map_err(|source| ProjectContextError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    // 基本验证
    if content.trim().is_empty() {
        return Err(ProjectContextError::Empty {
            path: path.to_path_buf(),
        });
    }

    Ok(content)
}

/// 检查此项目是否标记为受信任
fn check_trust_status(workspace: &Path) -> bool {
    if crate::config::is_workspace_trusted(workspace) {
        return true;
    }

    // 检查信任标记
    let trust_markers = [
        workspace.join(".deepseek").join("trusted"),
        workspace.join(".deepseek").join("trust.json"),
    ];

    for marker in &trust_markers {
        if marker.exists() {
            return true;
        }
    }

    false
}

/// 为项目创建默认的 AGENTS.md 文件
pub fn create_default_agents_md(workspace: &Path) -> std::io::Result<PathBuf> {
    let agents_path = workspace.join("AGENTS.md");

    let default_content = r#"# Project Agent Instructions

This file provides guidance to AI agents (DeepSeek TUI, Claude Code, etc.) when working with code in this repository.

## File Location

Save this file as `AGENTS.md` in your project root so the CLI can load it automatically.

## Build and Development Commands

```bash
# Build
# cargo build              # Rust projects
# npm run build            # Node.js projects
# python -m build          # Python projects

# Test
# cargo test               # Rust
# npm test                 # Node.js
# pytest                   # Python

# Lint and Format
# cargo fmt && cargo clippy  # Rust
# npm run lint               # Node.js
# ruff check .               # Python
```

## Architecture Overview

<!-- Describe your project's high-level architecture here -->
<!-- Focus on the "big picture" that requires reading multiple files to understand -->

### Key Components

<!-- List and describe the main components/modules -->

### Data Flow

<!-- Describe how data flows through the system -->

## Configuration Files

<!-- List important configuration files and their purposes -->

## Extension Points

<!-- Describe how to extend the codebase (add new features, tools, etc.) -->

## Commit Messages

Use conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`
"#;

    fs::write(&agents_path, default_content)?;
    Ok(agents_path)
}

/// 合并多个项目上下文（例如来自嵌套目录）
#[allow(dead_code)] // 单仓库上下文合并的公共 API
pub fn merge_contexts(contexts: &[ProjectContext]) -> Option<String> {
    let non_empty: Vec<_> = contexts
        .iter()
        .filter_map(ProjectContext::as_system_block)
        .collect();

    if non_empty.is_empty() {
        None
    } else {
        Some(non_empty.join("\n\n"))
    }
}

// === 单元测试 ===

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_project_context_empty() {
        let tmp = tempdir().expect("tempdir");
        let ctx = load_project_context(tmp.path());

        assert!(!ctx.has_instructions());
        assert!(ctx.source_path.is_none());
    }

    #[test]
    fn test_load_project_context_agents_md() {
        let tmp = tempdir().expect("tempdir");
        let agents_path = tmp.path().join("AGENTS.md");
        fs::write(&agents_path, "# Test Instructions\n\nFollow these rules.").expect("write");

        let ctx = load_project_context(tmp.path());

        assert!(ctx.has_instructions());
        assert!(
            ctx.instructions
                .as_ref()
                .unwrap()
                .contains("Test Instructions")
        );
        assert_eq!(ctx.source_path, Some(agents_path));
    }

    #[test]
    fn test_load_project_context_priority() {
        let tmp = tempdir().expect("tempdir");

        // Create both files - AGENTS.md should take priority
        fs::write(tmp.path().join("AGENTS.md"), "AGENTS content").expect("write");
        let claude_dir = tmp.path().join(".claude");
        fs::create_dir(&claude_dir).expect("mkdir");
        fs::write(claude_dir.join("instructions.md"), "CLAUDE content").expect("write");

        let ctx = load_project_context(tmp.path());

        assert!(ctx.has_instructions());
        assert!(
            ctx.instructions
                .as_ref()
                .unwrap()
                .contains("AGENTS content")
        );
    }

    #[test]
    fn test_load_project_context_hidden_dir() {
        let tmp = tempdir().expect("tempdir");
        let hidden_dir = tmp.path().join(".deepseek");
        fs::create_dir(&hidden_dir).expect("mkdir");
        fs::write(hidden_dir.join("instructions.md"), "Hidden instructions").expect("write");

        let ctx = load_project_context(tmp.path());

        assert!(ctx.has_instructions());
        assert!(
            ctx.instructions
                .as_ref()
                .unwrap()
                .contains("Hidden instructions")
        );
    }

    #[test]
    fn test_as_system_block() {
        let tmp = tempdir().expect("tempdir");
        let agents_path = tmp.path().join("AGENTS.md");
        fs::write(&agents_path, "Test content").expect("write");

        let ctx = load_project_context(tmp.path());
        let block = ctx.as_system_block().expect("block");

        assert!(block.contains("<project_instructions"));
        assert!(block.contains("Test content"));
        assert!(block.contains("</project_instructions>"));
    }

    #[test]
    fn test_empty_file_warning() {
        let tmp = tempdir().expect("tempdir");
        let agents_path = tmp.path().join("AGENTS.md");
        fs::write(&agents_path, "   \n  \n  ").expect("write"); // Only whitespace

        let ctx = load_project_context(tmp.path());

        assert!(!ctx.has_instructions());
        assert!(!ctx.warnings.is_empty());
    }

    #[test]
    fn test_check_trust_status() {
        let tmp = tempdir().expect("tempdir");

        // Not trusted by default
        assert!(!check_trust_status(tmp.path()));

        // Create trust marker
        let deepseek_dir = tmp.path().join(".deepseek");
        fs::create_dir(&deepseek_dir).expect("mkdir");
        fs::write(deepseek_dir.join("trusted"), "").expect("write");

        assert!(check_trust_status(tmp.path()));
    }

    #[test]
    fn test_create_default_agents_md() {
        let tmp = tempdir().expect("tempdir");
        let path = create_default_agents_md(tmp.path()).expect("create");

        assert!(path.exists());
        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("Project Agent Instructions"));
    }

    #[test]
    fn test_load_with_parents() {
        let tmp = tempdir().expect("tempdir");

        // Create a nested structure
        let subdir = tmp.path().join("subproject");
        fs::create_dir(&subdir).expect("mkdir");

        // Put AGENTS.md in parent
        fs::write(tmp.path().join("AGENTS.md"), "Parent instructions").expect("write");
        // Also create .git to mark as repo root
        fs::create_dir(tmp.path().join(".git")).expect("mkdir .git");

        // Load from subdir should find parent's AGENTS.md
        let ctx = load_project_context_with_parents(&subdir);

        assert!(ctx.has_instructions());
        assert!(
            ctx.instructions
                .as_ref()
                .unwrap()
                .contains("Parent instructions")
        );
    }

    #[test]
    fn test_merge_contexts() {
        let mut ctx1 = ProjectContext::empty(PathBuf::from("/a"));
        ctx1.instructions = Some("Instructions A".to_string());
        ctx1.source_path = Some(PathBuf::from("/a/AGENTS.md"));

        let mut ctx2 = ProjectContext::empty(PathBuf::from("/b"));
        ctx2.instructions = Some("Instructions B".to_string());
        ctx2.source_path = Some(PathBuf::from("/b/AGENTS.md"));

        let merged = merge_contexts(&[ctx1, ctx2]).expect("merge");

        assert!(merged.contains("Instructions A"));
        assert!(merged.contains("Instructions B"));
    }

    #[test]
    fn test_load_with_parents_searches_above_git_root_when_needed() {
        let tmp = tempdir().expect("tempdir");

        // AGENTS.md exists above repository root.
        fs::write(tmp.path().join("AGENTS.md"), "Organization instructions").expect("write");

        // Mark repository root one level below.
        let repo_root = tmp.path().join("repo");
        fs::create_dir(&repo_root).expect("mkdir repo");
        fs::create_dir(repo_root.join(".git")).expect("mkdir .git");

        let workspace = repo_root.join("apps").join("client");
        fs::create_dir_all(&workspace).expect("mkdir workspace");

        let ctx = load_project_context_with_parents(&workspace);
        assert!(ctx.has_instructions());
        assert!(
            ctx.instructions
                .as_ref()
                .unwrap()
                .contains("Organization instructions")
        );
    }
}
