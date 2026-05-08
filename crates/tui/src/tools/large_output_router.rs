//! 大输出路由 — 将大工具输出路由到分页器。
//!
//! 任何超过配置阈值的估计 token 数的工具结果在此处被拦截，
//! 在到达父上下文之前进行处理。一个轻量级的 V4-Flash 合成子代理
//! 压缩原始输出；只有合成结果返回给父上下文。原始内容存储在 workshop
//! 变量 `last_tool_result` 中，以便父代理在稍后需要完整文本时可以
//! 调用 `promote_to_context`。
//!
//! 每个工具的阈值可以覆盖全局默认值。单个工具调用可以传递 `raw=true`
//! 以完全绕过路由。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::tools::spec::ToolResult;

// ── Constants ──────────────────────────────────────────────────────────────────

/// 默认 token 阈值，超过此值的工具结果将通过 workshop 路由。与 issue 规范的 4096 tokens 一致。
pub const DEFAULT_LARGE_OUTPUT_THRESHOLD_TOKENS: usize = 4_096;

/// 用于启发式估算的近似每 token 字符数。
/// 我们有意选择一个保守的值（3 字符/token），以便宁可路由也不将原始数据转储到父上下文中。
const CHARS_PER_TOKEN_ESTIMATE: usize = 3;

/// workshop 变量名称，用于存储原始工具输出。
pub const WORKSHOP_LAST_TOOL_RESULT_VAR: &str = "last_tool_result";

// ── Configuration ─────────────────────────────────────────────────────────────

/// `config.toml` 中的 `[workshop]` 配置节。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WorkshopConfig {
    /// Token 阈值，超过此值的工具结果将通过 workshop 合成子代理路由。
    /// 默认值：[`DEFAULT_LARGE_OUTPUT_THRESHOLD_TOKENS`]。
    #[serde(default)]
    pub large_output_threshold_tokens: Option<usize>,

    /// 每个工具的阈值覆盖（工具名称 → token 限制）。出现在此处的工具
    /// 使用此限制而不是 `large_output_threshold_tokens`。
    #[serde(default)]
    pub per_tool_thresholds: Option<HashMap<String, usize>>,
}

impl WorkshopConfig {
    /// 解析给定工具名称的有效阈值。
    #[must_use]
    pub fn threshold_for(&self, tool_name: &str) -> usize {
        if let Some(per_tool) = self.per_tool_thresholds.as_ref()
            && let Some(&limit) = per_tool.get(tool_name)
        {
            return limit;
        }
        self.large_output_threshold_tokens
            .unwrap_or(DEFAULT_LARGE_OUTPUT_THRESHOLD_TOKENS)
    }
}

// ── Token estimation ──────────────────────────────────────────────────────────

/// 使用字符计数启发式估算 `text` 中的 token 数量。
///
/// 这避免了对真实分词器的依赖；估算故意保守（少算 token），
/// 以便我们积极路由，而不是让 5K token 的数据块漏过。
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    // Round up: partial last token still costs a token.
    chars.div_ceil(CHARS_PER_TOKEN_ESTIMATE)
}

// ── Router ────────────────────────────────────────────────────────────────────

/// 由 [`LargeOutputRouter::route`] 返回的决策。
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// 输出足够小；原样传递。
    PassThrough,
    /// 输出超过阈值，已被（或应该被）合成。
    Synthesise {
        /// 原始输出的估计 token 数量。
        estimated_tokens: usize,
        /// 被超过的阈值。
        threshold: usize,
    },
}

/// 拦截工具结果并将大结果路由到 workshop。
///
/// 此类型特意设为 `Clone` 和 `Default`，以便可以低成本地嵌入
/// [`ToolContext`](crate::tools::spec::ToolContext) 中，无需 `Arc` 包装。
#[derive(Debug, Clone, Default)]
pub struct LargeOutputRouter {
    config: WorkshopConfig,
}

impl LargeOutputRouter {
    /// 从解析后的 workshop 配置构造路由器。
    #[must_use]
    pub fn new(config: WorkshopConfig) -> Self {
        Self { config }
    }

    /// 判断 `tool_name` 的 `result` 是否应该被合成。
    ///
    /// 当工具调用包含 `raw = true` 时传递 `raw_bypass = true`。
    #[must_use]
    pub fn route(&self, tool_name: &str, result: &ToolResult, raw_bypass: bool) -> RouteDecision {
        if raw_bypass || !result.success {
            return RouteDecision::PassThrough;
        }
        let threshold = self.config.threshold_for(tool_name);
        let estimated_tokens = estimate_tokens(&result.content);
        if estimated_tokens > threshold {
            RouteDecision::Synthesise {
                estimated_tokens,
                threshold,
            }
        } else {
            RouteDecision::PassThrough
        }
    }

    /// 构建发送给 V4-Flash workshop 子代理的合成提示。
    ///
    /// 提示有意保持简洁——Flash 是一个快速模型，我们只需要忠实的摘要，而非深入推理。
    ///
    /// 这是后续 LLM 实时合成调用的构建块（一旦异步 Flash 客户端可以从注册表层安全调用）。
    /// 该方法公开，以便此 crate 外部的调用者可以对提示结构进行单元测试。
    #[must_use]
    #[allow(dead_code)] // used by future Flash synthesis call; keep for API stability
    pub fn synthesis_prompt(tool_name: &str, raw_output: &str, estimated_tokens: usize) -> String {
        format!(
            "You are a synthesis assistant. The tool `{tool_name}` produced {estimated_tokens} tokens \
             of output that is too large to include directly in the parent context.\n\n\
             Summarise the output below into a concise, faithful synthesis of ≤ 800 words. \
             Preserve key facts, numbers, file paths, error messages, and any actionable \
             information. Do NOT add commentary or interpretation beyond what is in the source.\n\n\
             <raw_tool_output>\n{raw_output}\n</raw_tool_output>"
        )
    }

    /// 用 workshop 来源标头和关于存储的原始输出的提示来包装合成结果。
    #[must_use]
    pub fn wrap_synthesis(
        tool_name: &str,
        synthesis: &str,
        estimated_tokens: usize,
        threshold: usize,
    ) -> String {
        format!(
            "[workshop-synthesis: tool={tool_name}, raw_tokens≈{estimated_tokens}, \
             threshold={threshold}, raw_stored_in={WORKSHOP_LAST_TOOL_RESULT_VAR}]\n\n{synthesis}"
        )
    }
}

// ── Workshop variable store ───────────────────────────────────────────────────

/// workshop 变量的进程内存储，跨会话内的工具调用持久化。
/// 目前暴露的唯一变量是 `last_tool_result`，它保存最近的原始大工具输出，
/// 供 `promote_to_context` 使用。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkshopVariables {
    /// 最近通过 workshop 路由的大工具输出的原始内容。
    /// 未发生路由时为空字符串。
    #[serde(default)]
    pub last_tool_result: String,

    /// 产生 `last_tool_result` 的工具名称。
    #[serde(default)]
    pub last_tool_name: String,
}

impl WorkshopVariables {
    /// 存储来自大工具路由事件的原始输出。
    pub fn store_raw(&mut self, tool_name: &str, raw: &str) {
        self.last_tool_result = raw.to_string();
        self.last_tool_name = tool_name.to_string();
    }

    /// 检索并清除存储的原始输出（消费语义，防止变量被意外提升两次）。
    ///
    /// 由 `promote_to_context` 工具调用（尚未在此 PR 中连接）。
    #[must_use]
    #[allow(dead_code)] // consumed by promote_to_context tool in follow-up
    pub fn take_raw(&mut self) -> Option<(String, String)> {
        if self.last_tool_result.is_empty() {
            return None;
        }
        let content = std::mem::take(&mut self.last_tool_result);
        let name = std::mem::take(&mut self.last_tool_name);
        Some((name, content))
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(content: &str) -> ToolResult {
        ToolResult::success(content.to_string())
    }

    #[test]
    fn pass_through_below_threshold() {
        let router = LargeOutputRouter::default();
        let small = "x".repeat(100);
        let result = make_result(&small);
        assert_eq!(
            router.route("read_file", &result, false),
            RouteDecision::PassThrough
        );
    }

    #[test]
    fn synthesise_above_threshold() {
        let router = LargeOutputRouter::default();
        // DEFAULT threshold = 4096 tokens; 3 chars/token → 4096*3 = 12288 chars
        let big = "a".repeat(13_000);
        let result = make_result(&big);
        assert!(matches!(
            router.route("read_file", &result, false),
            RouteDecision::Synthesise { .. }
        ));
    }

    #[test]
    fn raw_bypass_skips_routing() {
        let router = LargeOutputRouter::default();
        let big = "a".repeat(13_000);
        let result = make_result(&big);
        // raw=true → always pass through regardless of size
        assert_eq!(
            router.route("exec_shell", &result, true),
            RouteDecision::PassThrough
        );
    }

    #[test]
    fn error_results_always_pass_through() {
        let router = LargeOutputRouter::default();
        let big = "error: ".repeat(2_000);
        let result = ToolResult::error(big);
        assert_eq!(
            router.route("exec_shell", &result, false),
            RouteDecision::PassThrough
        );
    }

    #[test]
    fn per_tool_threshold_override() {
        let mut per_tool = HashMap::new();
        per_tool.insert("grep_files".to_string(), 100); // very low
        let config = WorkshopConfig {
            large_output_threshold_tokens: Some(4096),
            per_tool_thresholds: Some(per_tool),
        };
        let router = LargeOutputRouter::new(config);
        // 100 tokens * 3 = 300 chars → trigger with 400 chars
        let medium = "b".repeat(400);
        let result = make_result(&medium);
        assert!(matches!(
            router.route("grep_files", &result, false),
            RouteDecision::Synthesise { .. }
        ));
        // Other tools still use the global threshold
        assert_eq!(
            router.route("read_file", &result, false),
            RouteDecision::PassThrough
        );
    }

    #[test]
    fn estimate_tokens_conservative() {
        // 9 chars → ceil(9/3) = 3 tokens
        assert_eq!(estimate_tokens("123456789"), 3);
        // 10 chars → ceil(10/3) = 4 tokens
        assert_eq!(estimate_tokens("1234567890"), 4);
        // Empty string
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn workshop_variables_store_and_take() {
        let mut vars = WorkshopVariables::default();
        assert!(vars.take_raw().is_none());

        vars.store_raw("read_file", "raw content here");
        let taken = vars.take_raw().expect("should have content");
        assert_eq!(taken.0, "read_file");
        assert_eq!(taken.1, "raw content here");

        // Second take is empty — consume semantics
        assert!(vars.take_raw().is_none());
    }

    #[test]
    fn wrap_synthesis_includes_provenance_header() {
        let wrapped = LargeOutputRouter::wrap_synthesis("web_search", "key facts here", 5000, 4096);
        assert!(wrapped.contains("workshop-synthesis"));
        assert!(wrapped.contains("web_search"));
        assert!(wrapped.contains("5000"));
        assert!(wrapped.contains("key facts here"));
    }
}
