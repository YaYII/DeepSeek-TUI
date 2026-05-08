//! Git 历史 — 查看 git 提交历史。
//!
//! These tools provide read-only access to commit history and attribution
//! without exposing arbitrary shell execution.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use async_trait::async_trait;
use serde_json::{Value, json};

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_str, optional_u64, required_str,
};

const MAX_OUTPUT_CHARS: usize = 40_000;
const DEFAULT_LOG_MAX_COUNT: u64 = 20;
const MAX_LOG_MAX_COUNT: u64 = 200;
const DEFAULT_UNIFIED: u64 = 3;
const MAX_UNIFIED: u64 = 50;
const DEFAULT_BLAME_START_LINE: u64 = 1;
const DEFAULT_BLAME_MAX_LINES: u64 = 200;
const MAX_BLAME_MAX_LINES: u64 = 2_000;

/// Tool for reading recent commit history.
pub struct GitLogTool;

#[async_trait]
impl ToolSpec for GitLogTool {
    fn name(&self) -> &'static str {
        "git_log"
    }

    fn description(&self) -> &'static str {
        "在工作区运行 `git log`，支持可选的路径和作者/日期过滤器。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "限定的可选子目录或文件路径。"
                },
                "max_count": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_LOG_MAX_COUNT,
                    "default": DEFAULT_LOG_MAX_COUNT,
                    "description": "返回的最大提交数。"
                },
                "author": {
                    "type": "string",
                    "description": "可选的 git 作者过滤器（与 `git log --author` 语义相同）。"
                },
                "since": {
                    "type": "string",
                    "description": "可选的起始日期，例如 '2 weeks ago' 或 ISO 日期。"
                },
                "until": {
                    "type": "string",
                    "description": "可选的结束日期，例如 'yesterday' 或 ISO 日期。"
                }
            },
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let git_ctx = resolve_git_context(context, optional_str(&input, "path"))?;
        let max_count =
            optional_u64(&input, "max_count", DEFAULT_LOG_MAX_COUNT).clamp(1, MAX_LOG_MAX_COUNT);
        let author = optional_str(&input, "author").map(ToOwned::to_owned);
        let since = optional_str(&input, "since").map(ToOwned::to_owned);
        let until = optional_str(&input, "until").map(ToOwned::to_owned);

        let mut args = vec![
            "log".to_string(),
            "--no-color".to_string(),
            format!("--max-count={max_count}"),
            "--date=iso-strict".to_string(),
            "--pretty=format:%H%nAuthor: %an <%ae>%nDate: %ad%nSubject: %s%n".to_string(),
        ];
        if let Some(author) = &author {
            args.push(format!("--author={author}"));
        }
        if let Some(since) = &since {
            args.push(format!("--since={since}"));
        }
        if let Some(until) = &until {
            args.push(format!("--until={until}"));
        }
        if let Some(pathspec) = &git_ctx.pathspec {
            args.push("--".to_string());
            args.push(pathspec.display().to_string());
        }

        let command_str = format_command(&git_ctx.working_dir, &args);
        let output = run_git_command(&git_ctx.working_dir, &args)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(
                ToolResult::error(format!("git log failed: {}", stderr.trim())).with_metadata(
                    json!({
                        "command": command_str,
                        "exit_code": output.status.code(),
                        "stderr": stderr.trim(),
                    }),
                ),
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (content, truncated, omitted_chars) = truncate_with_note(&stdout, MAX_OUTPUT_CHARS);
        Ok(ToolResult::success(content).with_metadata(json!({
            "command": command_str,
            "working_dir": git_ctx.working_dir,
            "pathspec": git_ctx.pathspec,
            "max_count": max_count,
            "author": author,
            "since": since,
            "until": until,
            "truncated": truncated,
            "omitted_chars": omitted_chars,
        })))
    }
}

/// Tool for showing a specific commit with optional patch/stat output.
pub struct GitShowTool;

#[async_trait]
impl ToolSpec for GitShowTool {
    fn name(&self) -> &'static str {
        "git_show"
    }

    fn description(&self) -> &'static str {
        "运行 `git show` 查看特定修订版，可选的补丁和统计信息。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "rev": {
                    "type": "string",
                    "description": "要显示的修订版本（提交 SHA、标签、分支或引用表达式）。"
                },
                "path": {
                    "type": "string",
                    "description": "限定的可选子目录或文件路径。"
                },
                "patch": {
                    "type": "boolean",
                    "default": true,
                    "description": "包含补丁片段（默认为 true）。"
                },
                "stat": {
                    "type": "boolean",
                    "default": true,
                    "description": "包含 --stat 摘要（默认为 true）。"
                },
                "unified": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": MAX_UNIFIED,
                    "default": DEFAULT_UNIFIED,
                    "description": "patch=true 时补丁输出的上下文行数。"
                }
            },
            "required": ["rev"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let rev = required_str(&input, "rev")?;
        let git_ctx = resolve_git_context(context, optional_str(&input, "path"))?;
        let patch = optional_bool(&input, "patch", true);
        let stat = optional_bool(&input, "stat", true);
        let unified = optional_u64(&input, "unified", DEFAULT_UNIFIED).min(MAX_UNIFIED);

        let mut args = vec![
            "show".to_string(),
            "--no-color".to_string(),
            "--no-ext-diff".to_string(),
        ];
        if patch {
            args.push(format!("--unified={unified}"));
        } else {
            args.push("--no-patch".to_string());
        }
        if stat {
            args.push("--stat".to_string());
        }
        args.push(rev.to_string());
        if let Some(pathspec) = &git_ctx.pathspec {
            args.push("--".to_string());
            args.push(pathspec.display().to_string());
        }

        let command_str = format_command(&git_ctx.working_dir, &args);
        let output = run_git_command(&git_ctx.working_dir, &args)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(ToolResult::error(format!(
                "git show failed for '{rev}': {}",
                stderr.trim()
            ))
            .with_metadata(json!({
                "command": command_str,
                "exit_code": output.status.code(),
                "stderr": stderr.trim(),
            })));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (content, truncated, omitted_chars) = truncate_with_note(&stdout, MAX_OUTPUT_CHARS);
        Ok(ToolResult::success(content).with_metadata(json!({
            "command": command_str,
            "working_dir": git_ctx.working_dir,
            "pathspec": git_ctx.pathspec,
            "rev": rev,
            "patch": patch,
            "stat": stat,
            "unified": if patch { Some(unified) } else { None },
            "truncated": truncated,
            "omitted_chars": omitted_chars,
        })))
    }
}

/// Tool for attributing lines in a file to commits and authors.
pub struct GitBlameTool;

#[async_trait]
impl ToolSpec for GitBlameTool {
    fn name(&self) -> &'static str {
        "git_blame"
    }

    fn description(&self) -> &'static str {
        "对文件运行 `git blame`，支持可选的修订版本和行范围控制。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "工作区内被跟踪文件的路径。"
                },
                "rev": {
                    "type": "string",
                    "description": "可选的指责目标修订版本（默认：HEAD）。"
                },
                "start_line": {
                    "type": "integer",
                    "minimum": 1,
                    "default": DEFAULT_BLAME_START_LINE,
                    "description": "在 blame 输出中包含的起始行号。"
                },
                "max_lines": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_BLAME_MAX_LINES,
                    "default": DEFAULT_BLAME_MAX_LINES,
                    "description": "包含的最大行数。"
                },
                "porcelain": {
                    "type": "boolean",
                    "default": false,
                    "description": "为 true 时，输出 `--line-porcelain` 格式。"
                }
            },
            "required": ["path"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = required_str(&input, "path")?;
        let resolved_path = context.resolve_path(path_str)?;
        let metadata = fs::metadata(&resolved_path).map_err(|e| {
            ToolError::invalid_input(format!(
                "Path does not exist or is not accessible: {path_str} ({e})"
            ))
        })?;
        if !metadata.is_file() {
            return Err(ToolError::invalid_input(format!(
                "Path must point to a file: {path_str}"
            )));
        }

        let working_dir = resolved_path.parent().ok_or_else(|| {
            ToolError::invalid_input(format!("Path has no parent directory: {path_str}"))
        })?;
        let pathspec = pathspec_from(working_dir, &resolved_path);
        let rev = optional_str(&input, "rev").unwrap_or("HEAD");
        let start_line = optional_u64(&input, "start_line", DEFAULT_BLAME_START_LINE).max(1);
        let max_lines = optional_u64(&input, "max_lines", DEFAULT_BLAME_MAX_LINES)
            .clamp(1, MAX_BLAME_MAX_LINES);
        let end_line = start_line.saturating_add(max_lines.saturating_sub(1));
        let porcelain = optional_bool(&input, "porcelain", false);

        let mut args = vec![
            "blame".to_string(),
            "--date=iso".to_string(),
            format!("-L{start_line},{end_line}"),
        ];
        if porcelain {
            args.push("--line-porcelain".to_string());
        }
        args.push(rev.to_string());
        args.push("--".to_string());
        args.push(pathspec.display().to_string());

        let command_str = format_command(working_dir, &args);
        let output = run_git_command(working_dir, &args)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(ToolResult::error(format!(
                "git blame failed for '{path_str}' at '{rev}': {}",
                stderr.trim()
            ))
            .with_metadata(json!({
                "command": command_str,
                "exit_code": output.status.code(),
                "stderr": stderr.trim(),
            })));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (content, truncated, omitted_chars) = truncate_with_note(&stdout, MAX_OUTPUT_CHARS);
        Ok(ToolResult::success(content).with_metadata(json!({
            "command": command_str,
            "working_dir": working_dir,
            "pathspec": pathspec,
            "rev": rev,
            "start_line": start_line,
            "max_lines": max_lines,
            "porcelain": porcelain,
            "truncated": truncated,
            "omitted_chars": omitted_chars,
        })))
    }
}

struct GitContext {
    working_dir: PathBuf,
    pathspec: Option<PathBuf>,
}

fn resolve_git_context(context: &ToolContext, path: Option<&str>) -> Result<GitContext, ToolError> {
    let workspace = canonical_or_workspace(&context.workspace);
    let mut working_dir = workspace.clone();
    let mut pathspec = None;

    if let Some(raw) = path {
        let resolved = context.resolve_path(raw)?;
        let metadata = fs::metadata(&resolved).map_err(|e| {
            ToolError::invalid_input(format!(
                "Path does not exist or is not accessible: {raw} ({e})"
            ))
        })?;

        if metadata.is_dir() {
            working_dir = resolved;
            pathspec = Some(PathBuf::from("."));
        } else {
            let parent = resolved.parent().ok_or_else(|| {
                ToolError::invalid_input(format!("Path has no parent directory: {raw}"))
            })?;
            working_dir = parent.to_path_buf();
            pathspec = Some(pathspec_from(&working_dir, &resolved));
        }
    }

    if !working_dir.exists() {
        return Err(ToolError::invalid_input(format!(
            "Working directory does not exist: {}",
            working_dir.display()
        )));
    }

    Ok(GitContext {
        working_dir,
        pathspec,
    })
}

fn canonical_or_workspace(workspace: &Path) -> PathBuf {
    workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf())
}

fn pathspec_from(working_dir: &Path, resolved: &Path) -> PathBuf {
    match resolved.strip_prefix(working_dir) {
        Ok(rel) if rel.as_os_str().is_empty() => PathBuf::from("."),
        Ok(rel) => rel.to_path_buf(),
        Err(_) => PathBuf::from("."),
    }
}

fn run_git_command(working_dir: &Path, args: &[String]) -> Result<Output, ToolError> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(working_dir);
    cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::not_available("git is not installed or not in PATH")
        } else {
            ToolError::execution_failed(format!("Failed to run git: {e}"))
        }
    })
}

fn format_command(working_dir: &Path, args: &[String]) -> String {
    format!(
        "git -C {} {}",
        working_dir.display(),
        args.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn truncate_with_note(text: &str, max_chars: usize) -> (String, bool, usize) {
    if text.chars().count() <= max_chars {
        return (text.to_string(), false, 0);
    }
    let end = char_boundary_index(text, max_chars);
    let truncated = &text[..end];
    let omitted_chars = text
        .chars()
        .count()
        .saturating_sub(truncated.chars().count());
    let note = format!(
        "\n\n[output truncated to {max_chars} characters; {omitted_chars} characters omitted]"
    );
    (format!("{truncated}{note}"), true, omitted_chars)
}

fn char_boundary_index(text: &str, max_chars: usize) -> usize {
    if max_chars == 0 {
        return 0;
    }
    for (count, (idx, _)) in text.char_indices().enumerate() {
        if count == max_chars {
            return idx;
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use tempfile::tempdir;

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn run_git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git should spawn");
        assert!(status.success(), "git {:?} failed", args);
    }

    fn init_git_repo(root: &Path) {
        run_git(root, &["init", "-q"]);
        run_git(root, &["config", "user.email", "test@example.com"]);
        run_git(root, &["config", "user.name", "Test User"]);
    }

    fn commit_all(root: &Path, message: &str) {
        run_git(root, &["add", "."]);
        run_git(root, &["commit", "-q", "-m", message]);
    }

    #[tokio::test]
    async fn git_log_lists_recent_commits() {
        if !git_available() {
            return;
        }

        let tmp = tempdir().expect("tempdir");
        init_git_repo(tmp.path());
        fs::write(tmp.path().join("file.txt"), "one\n").expect("write");
        commit_all(tmp.path(), "first");
        fs::write(tmp.path().join("file.txt"), "two\n").expect("write");
        commit_all(tmp.path(), "second");

        let ctx = ToolContext::new(tmp.path());
        let result = GitLogTool
            .execute(json!({ "max_count": 1 }), &ctx)
            .await
            .expect("execute");
        assert!(result.success);
        assert!(result.content.contains("Subject: second"));
    }

    #[tokio::test]
    async fn git_show_returns_patch_for_revision() {
        if !git_available() {
            return;
        }

        let tmp = tempdir().expect("tempdir");
        init_git_repo(tmp.path());
        fs::write(tmp.path().join("file.txt"), "one\n").expect("write");
        commit_all(tmp.path(), "first");
        fs::write(tmp.path().join("file.txt"), "one\ntwo\n").expect("write");
        commit_all(tmp.path(), "second");

        let ctx = ToolContext::new(tmp.path());
        let result = GitShowTool
            .execute(json!({ "rev": "HEAD", "stat": false }), &ctx)
            .await
            .expect("execute");
        assert!(result.success);
        assert!(result.content.contains("diff --git"));
        assert!(result.content.contains("+two"));
    }

    #[tokio::test]
    async fn git_blame_reports_author_for_range() {
        if !git_available() {
            return;
        }

        let tmp = tempdir().expect("tempdir");
        init_git_repo(tmp.path());
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).expect("mkdir");
        let file = src.join("lib.rs");
        fs::write(&file, "pub fn one() -> i32 { 1 }\n").expect("write");
        commit_all(tmp.path(), "first");
        fs::write(&file, "pub fn one() -> i32 { 2 }\n").expect("write");
        commit_all(tmp.path(), "second");

        let ctx = ToolContext::new(tmp.path());
        let result = GitBlameTool
            .execute(
                json!({
                    "path": "src/lib.rs",
                    "start_line": 1,
                    "max_lines": 1
                }),
                &ctx,
            )
            .await
            .expect("execute");
        assert!(result.success);
        assert!(result.content.contains("Test User"));
    }

    #[tokio::test]
    async fn git_blame_errors_for_non_file_path() {
        if !git_available() {
            return;
        }

        let tmp = tempdir().expect("tempdir");
        init_git_repo(tmp.path());

        let ctx = ToolContext::new(tmp.path());
        let result = GitBlameTool
            .execute(json!({ "path": "." }), &ctx)
            .await
            .expect_err("directory path should fail");
        assert!(matches!(result, ToolError::InvalidInput { .. }));
    }
}
