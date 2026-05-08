//! FIM 工具 — 填充中间（Fill-in-the-Middle）补全。
//!
//! Reads a file, finds `prefix_anchor` and `suffix_anchor`, calls the
//! DeepSeek `/beta/completions` FIM endpoint, and writes the generated
//! middle content back into the file.

use std::fs;

use async_trait::async_trait;
use serde_json::{Value, json};
use thiserror::Error;

use crate::client::DeepSeekClient;

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_u64, required_str,
};

/// Result of a FIM edit operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FimEditResult {
    pub success: bool,
    pub path: String,
    pub generated_text: String,
    pub prefix_end: usize,
    pub suffix_start: usize,
    pub message: String,
}

/// Tool for performing Fill-in-the-Middle edits via the DeepSeek FIM API.
pub struct FimEditTool {
    pub client: Option<DeepSeekClient>,
    pub model: String,
}

impl FimEditTool {
    #[must_use]
    pub fn new(client: Option<DeepSeekClient>, model: String) -> Self {
        Self { client, model }
    }
}

// === Errors ===

#[derive(Debug, Error)]
enum FimError {
    #[error("文件中未找到前缀锚点: '{0}'")]
    PrefixNotFound(String),
    #[error("在前缀锚点后未找到后缀锚点: '{0}'")]
    SuffixNotFound(String),
    #[error("前缀和后缀锚点重叠（后缀开始于 {0}，前缀结束于 {1}）")]
    AnchorsOverlap(usize, usize),
    #[error("FIM API 调用失败: {0}")]
    ApiFailed(String),
}

#[async_trait]
impl ToolSpec for FimEditTool {
    fn name(&self) -> &'static str {
        "fim_edit"
    }

    fn description(&self) -> &'static str {
        "使用填充中间（FIM）补全来编辑文件。提供文件路径、\
         prefix_anchor（出现在要替换部分之前的文本）和 \
         suffix_anchor（出现在要替换部分之后的文本）。该工具 \
         调用 DeepSeek 的 FIM 端点来生成替换内容。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要编辑的文件路径（相对于工作区）"
                },
                "prefix_anchor": {
                    "type": "string",
                    "description": "标记前缀结束的文本锚点。直到并包括此锚点的所有内容保持原样，位于生成内容之前。"
                },
                "suffix_anchor": {
                    "type": "string",
                    "description": "标记后缀开始的文本锚点。从此锚点往后的所有内容保持原样，位于生成内容之后。"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "最大生成的令牌数（默认：1024）"
                }
            },
            "required": ["path", "prefix_anchor", "suffix_anchor"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::ReadOnly,
            ToolCapability::WritesFiles,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Suggest
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path = required_str(&input, "path")?;
        let prefix_anchor = required_str(&input, "prefix_anchor")?;
        let suffix_anchor = required_str(&input, "suffix_anchor")?;
        let max_tokens = optional_u64(&input, "max_tokens", 1024);

        // 1. Read the file
        let resolved = context.resolve_path(path)?;
        let content = fs::read_to_string(&resolved).map_err(|e| {
            ToolError::execution_failed(format!("Failed to read {}: {}", resolved.display(), e))
        })?;

        // 2. Find prefix anchor
        let prefix_pos = content.find(prefix_anchor).ok_or_else(|| {
            ToolError::execution_failed(
                FimError::PrefixNotFound(prefix_anchor.to_string()).to_string(),
            )
        })?;
        let prefix_end = prefix_pos + prefix_anchor.len();

        // 3. Find suffix anchor (after prefix anchor)
        let suffix_pos = content[prefix_end..].find(suffix_anchor).ok_or_else(|| {
            ToolError::execution_failed(
                FimError::SuffixNotFound(suffix_anchor.to_string()).to_string(),
            )
        })?;
        let suffix_start = prefix_end + suffix_pos;

        // 4. Validate anchors don't overlap
        if suffix_start < prefix_end {
            return Err(ToolError::execution_failed(
                FimError::AnchorsOverlap(suffix_start, prefix_end).to_string(),
            ));
        }

        // 5. Extract prefix and suffix for the FIM API
        let fim_prompt = content[..prefix_end].to_string();
        let fim_suffix = content[suffix_start..].to_string();

        // 6. Call FIM API
        let generated_text = match self.client.as_ref() {
            Some(client) => client
                .fim_completion(&self.model, &fim_prompt, &fim_suffix, max_tokens as u32)
                .await
                .map_err(|e| {
                    ToolError::execution_failed(FimError::ApiFailed(e.to_string()).to_string())
                })?,
            None => {
                return Err(ToolError::execution_failed(
                    "FIM API 客户端不可用".to_string(),
                ));
            }
        };

        // 7. Build the new content and write it back
        let generated_len = generated_text.len();
        let new_content = format!("{}{}{}", fim_prompt, generated_text, fim_suffix);
        fs::write(&resolved, &new_content).map_err(|e| {
            ToolError::execution_failed(format!("Failed to write {}: {}", resolved.display(), e))
        })?;

        let result = FimEditResult {
            success: true,
            path: path.to_string(),
            generated_text,
            prefix_end,
            suffix_start,
            message: format!(
                "FIM 编辑已应用于 `{}`。在前缀锚点结束（字节 {}）和后缀锚点开始（字节 {}）之间生成了 {} 个字符。",
                path, generated_len, prefix_end, suffix_start,
            ),
        };

        ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}
