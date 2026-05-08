//! 并行工具执行 — 并行运行多个工具。
//!
//! 注意：这个元工具有意不再向代理注册（请参见 `ToolRegistryBuilder::with_parallel_tool`）。
//! DeepSeek-V4 支持单次助手轮次中原生的并行 `tool_calls`，并且
//! 公开 OpenAI 内部名称 `multi_tool_use.parallel` 会导致
//! 模型幻觉生成 ChatGPT 风格的 XML 包装器。该结构体保留
//! 下来，以便引擎兼容性分发器和历史会话仍能正常解析它。

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};
use async_trait::async_trait;
use serde_json::{Value, json};

#[allow(dead_code)]
pub struct MultiToolUseParallelTool;

#[async_trait]
impl ToolSpec for MultiToolUseParallelTool {
    fn name(&self) -> &'static str {
        "multi_tool_use.parallel"
    }

    fn description(&self) -> &'static str {
        "并行执行多个工具调用并返回其结果。"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tool_uses": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "recipient_name": { "type": "string" },
                            "parameters": { "type": "object" }
                        },
                        "required": ["recipient_name", "parameters"]
                    }
                }
            },
            "required": ["tool_uses"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        _input: Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        Err(ToolError::execution_failed(
            "multi_tool_use.parallel 必须由引擎处理",
        ))
    }
}
