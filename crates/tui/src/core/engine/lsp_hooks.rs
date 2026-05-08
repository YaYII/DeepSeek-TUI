//! 引擎工具执行的编辑后 LSP 诊断钩子。
//!
//! 轮次循环只需要问"成功的编辑是否产生了诊断？"
//! 本模块拥有工具输入路径提取和合成诊断消息注入，
//! 以便顶层引擎模块专注于会话编排。

use std::path::PathBuf;

use super::*;

/// #136：派生工具调用编辑的文件路径。对于不修改文件的工具
/// 返回空向量。我们故意只处理三个已知的编辑工具 —
/// 添加更多（例如专门的 refactor 工具）只需在此处添加一行。
pub(super) fn edited_paths_for_tool(tool_name: &str, input: &serde_json::Value) -> Vec<PathBuf> {
    match tool_name {
        "edit_file" | "write_file" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                vec![PathBuf::from(path)]
            } else {
                Vec::new()
            }
        }
        "apply_patch" => {
            // `apply_patch` 接受 `path` 覆盖或 `files` 列表
            //（每个 `{path, content}`）。我们尝试两种形状。
            let mut out = Vec::new();
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                out.push(PathBuf::from(path));
            }
            if let Some(files) = input.get("files").and_then(|v| v.as_array()) {
                for entry in files {
                    if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                        out.push(PathBuf::from(path));
                    }
                }
            }
            // 回退：从统一 diff 负载中解析 `---`/`+++` 头部。
            if out.is_empty()
                && let Some(patch) = input.get("patch").and_then(|v| v.as_str())
            {
                out.extend(parse_patch_paths(patch));
            }
            out
        }
        _ => Vec::new(),
    }
}

/// 统一 diff 中 `+++ b/<path>` 行的轻量解析器。当
/// `apply_patch` 以原始 `patch` 文本调用且没有 `path`/`files` 覆盖时
/// 用作回退。我们故意保持简单 — 真正的 `apply_patch` 工具已经验证了
/// patch 形状；我们只需要一个尽力而为的提示用于 LSP 钩子。
pub(super) fn parse_patch_paths(patch: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            let trimmed = rest.trim();
            // 按 git diff 约定去除前导 `b/`。
            let path = trimmed.strip_prefix("b/").unwrap_or(trimmed);
            // 跳过 `/dev/null`（删除）。
            if path == "/dev/null" {
                continue;
            }
            out.push(PathBuf::from(path));
        }
    }
    out
}

impl Engine {
    /// #136：编辑后钩子。检查工具名称 + 输入，派生编辑的文件路径，
    /// 并向 LSP 管理器请求诊断。渲染的块排队在
    /// `pending_lsp_blocks` 中，并在下一次 API 请求之前刷新到
    /// 会话消息流中。失败默认静默 — 缺失/崩溃的 LSP 服务器
    /// 绝不能阻塞代理。
    pub(super) async fn run_post_edit_lsp_hook(
        &mut self,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) {
        if !self.lsp_manager.config().enabled {
            return;
        }
        let paths = edited_paths_for_tool(tool_name, tool_input);
        for path in paths {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                self.session.workspace.join(&path)
            };
            // 使用基于现有轮次计数器的短编辑序列，以便
            // 日志输出保持关联，即使我们当前不按序列批处理。
            let seq = self.turn_counter;
            if let Some(block) = self.lsp_manager.diagnostics_for(&absolute, seq).await {
                self.pending_lsp_blocks.push(block);
            }
        }
    }

    /// 将 `pending_lsp_blocks` 排空到单个合成用户消息中，以便
    /// 模型在下一次请求时看到诊断。当没有待处理内容时跳过。
    /// 消息使用标准的 `text` 内容块形状（与工具后引导消息
    /// 相同的形状），因此我们不需要发明新的信封。
    pub(super) async fn flush_pending_lsp_diagnostics(&mut self) {
        if self.pending_lsp_blocks.is_empty() {
            return;
        }
        let blocks = std::mem::take(&mut self.pending_lsp_blocks);
        let rendered = crate::lsp::render_blocks(&blocks);
        if rendered.is_empty() {
            return;
        }
        self.add_session_message(self.user_text_message_with_turn_metadata(rendered))
            .await;
    }
}
