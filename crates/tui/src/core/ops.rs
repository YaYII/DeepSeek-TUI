//! UI 向核心引擎提交的操作。
//!
//! 这些操作通过通道从 TUI 流向引擎，
//! 使 UI 在引擎处理请求时保持响应。

use crate::compaction::CompactionConfig;
use crate::models::{Message, SystemPrompt};
use crate::tui::app::AppMode;
use crate::tui::approval::ApprovalMode;
use std::path::PathBuf;

/// 可提交给引擎的操作。
#[derive(Debug, Clone)]
pub enum Op {
    /// 向 AI 发送消息
    SendMessage {
        content: String,
        mode: AppMode,
        model: String,
        goal_objective: Option<String>,
        /// 推理力度层级：`"off" | "low" | "medium" | "high" | "max"`。
        /// `None` 让提供商应用其默认值。
        reasoning_effort: Option<String>,
        /// 当用户选择了自动思考时为 true，即使 UI 向模型 API 发送
        /// 具体的每轮值。
        reasoning_effort_auto: bool,
        /// 当用户选择了自动模型路由时为 true。
        auto_model: bool,
        allow_shell: bool,
        trust_mode: bool,
        auto_approve: bool,
        approval_mode: ApprovalMode,
    },

    /// 取消当前请求
    #[allow(dead_code)]
    CancelRequest,

    /// 批准需要权限的工具调用
    #[allow(dead_code)]
    ApproveToolCall { id: String },

    /// 拒绝需要权限的工具调用
    #[allow(dead_code)]
    DenyToolCall { id: String },

    /// 生成子代理
    #[allow(dead_code)]
    SpawnSubAgent { prompt: String },

    /// 列出当前子代理及其状态
    ListSubAgents,

    /// 更改操作模式
    #[allow(dead_code)]
    ChangeMode { mode: AppMode },

    /// 更新正在使用的模型
    #[allow(dead_code)]
    SetModel { model: String },

    /// 更新自动压缩设置
    SetCompaction { config: CompactionConfig },

    /// 同步引擎会话状态（用于恢复/加载）
    SyncSession {
        messages: Vec<Message>,
        system_prompt: Option<SystemPrompt>,
        model: String,
        workspace: PathBuf,
    },

    /// 立即运行上下文压缩。
    CompactContext,

    /// 按 Zhang et al. (arXiv:2512.24601) 算法 1 运行递归语言模型（RLM）轮次。
    /// 提示存储在 REPL 中作为 `context`；根 LLM 只看到元数据。
    Rlm {
        /// 用户的提示 — 存储在 REPL 中，而非 LLM 上下文中。
        content: String,
        /// 用于根 LLM 调用的模型。
        model: String,
        /// 用于子 LLM（llm_query）调用的模型。
        child_model: String,
        /// `sub_rlm()` 调用的递归预算。论文实验使用
        /// depth=1；默认值由 `/rlm` 命令设置。
        max_depth: u32,
    },

    /// 编辑最后一条用户消息：从会话中移除最后的用户+助手交换，
    /// 然后用新内容重新发送。
    #[allow(dead_code)]
    EditLastTurn { new_message: String },

    /// 关闭引擎
    Shutdown,
}
