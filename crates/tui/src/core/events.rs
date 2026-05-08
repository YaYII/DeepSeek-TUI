//! 核心引擎向 UI 发出的事件。
//!
//! 这些事件通过通道从引擎流向 TUI，
//! 实现非阻塞的实时更新。

use std::{path::PathBuf, sync::Arc};

use serde_json::Value;

use crate::core::coherence::CoherenceState;
use crate::error_taxonomy::ErrorEnvelope;
use crate::models::{Message, SystemPrompt, Usage};
use crate::tools::spec::{ToolError, ToolResult};
use crate::tools::subagent::SubAgentResult;
use crate::tools::user_input::UserInputRequest;

/// 轮次的最终状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcomeStatus {
    Completed,
    Interrupted,
    Failed,
}

/// 引擎发出以更新 UI 的事件。
#[derive(Debug, Clone)]
pub enum Event {
    // === Streaming 事件 ===
    /// 新的消息块已开始
    MessageStarted {
        #[allow(dead_code)]
        index: usize,
    },

    /// 增量文本内容增量
    MessageDelta {
        #[allow(dead_code)]
        index: usize,
        content: String,
    },

    /// 消息块已完成
    MessageComplete {
        #[allow(dead_code)]
        index: usize,
    },

    /// 思考块已开始
    ThinkingStarted {
        #[allow(dead_code)]
        index: usize,
    },

    /// 增量思考内容增量
    ThinkingDelta {
        #[allow(dead_code)]
        index: usize,
        content: String,
    },

    /// 思考块已完成
    ThinkingComplete {
        #[allow(dead_code)]
        index: usize,
    },

    // === 工具事件 ===
    /// 工具调用已发起
    ToolCallStarted {
        id: String,
        name: String,
        input: Value,
    },

    /// 工具执行进度（用于长期运行的工具）
    #[allow(dead_code)]
    ToolCallProgress { id: String, output: String },

    /// 工具调用已完成
    ToolCallComplete {
        id: String,
        name: String,
        result: Result<ToolResult, ToolError>,
    },

    // === 轮次生命周期 ===
    /// 新轮次已开始（用户发送了消息）
    TurnStarted { turn_id: String },

    /// 轮次已完成（没有更多工具调用）
    TurnComplete {
        usage: Usage,
        status: TurnOutcomeStatus,
        error: Option<String>,
    },

    /// 上下文压缩已开始。
    CompactionStarted {
        id: String,
        auto: bool,
        message: String,
    },

    /// 上下文压缩已完成。
    CompactionCompleted {
        id: String,
        auto: bool,
        message: String,
        /// 压缩前的消息数量。
        #[allow(dead_code)]
        messages_before: Option<usize>,
        /// 压缩后的消息数量。
        #[allow(dead_code)]
        messages_after: Option<usize>,
    },

    /// 上下文压缩失败。
    CompactionFailed {
        id: String,
        auto: bool,
        message: String,
    },

    /// 检查点重启循环边界推进（issue #124）。上一个
    /// 周期已经归档到磁盘；引擎已将其内存中的
    /// 消息缓冲区交换为周期 `to` 的种子消息。
    /// 携带完整的简报记录，以便 UI 可以填充
    /// `app.cycle_briefings` 用于 `/cycle <n>`。
    CycleAdvanced {
        from: u32,
        to: u32,
        briefing: crate::cycle_manager::CycleBriefing,
    },

    /// 容量决策遥测。
    #[allow(dead_code)]
    CapacityDecision {
        session_id: String,
        turn_id: String,
        h_hat: f64,
        c_hat: f64,
        slack: f64,
        min_slack: f64,
        violation_ratio: f64,
        p_fail: f64,
        risk_band: String,
        action: String,
        cooldown_blocked: bool,
        reason: String,
    },

    /// 容量干预遥测。
    #[allow(dead_code)]
    CapacityIntervention {
        session_id: String,
        turn_id: String,
        action: String,
        before_prompt_tokens: usize,
        after_prompt_tokens: usize,
        compaction_size_reduction: usize,
        replay_outcome: Option<String>,
        replan_performed: bool,
    },

    /// 容量内存持久化失败遥测。
    #[allow(dead_code)]
    CapacityMemoryPersistFailed {
        session_id: String,
        turn_id: String,
        action: String,
        error: String,
    },

    /// 自然语言会话一致性状态。
    CoherenceState {
        state: CoherenceState,
        label: String,
        description: String,
        reason: String,
    },

    // === 子代理事件 ===
    /// 子代理已生成
    AgentSpawned { id: String, prompt: String },

    /// 子代理进度更新
    AgentProgress { id: String, status: String },

    /// 子代理已完成
    AgentComplete { id: String, result: String },

    /// 子代理列表
    AgentList { agents: Vec<SubAgentResult> },

    /// 结构化子代理邮箱信封（issue #128）。携带单调递增的
    /// seq 和类型化的 `MailboxMessage`，以便 UI 可以将每个
    /// 信封路由到正确的对话内卡片。
    SubAgentMailbox {
        seq: u64,
        message: crate::tools::subagent::MailboxMessage,
    },

    // === 系统事件 ===
    /// 发生了一个错误
    Error {
        envelope: ErrorEnvelope,
        #[allow(dead_code)]
        recoverable: bool,
    },

    /// 用于 UI 显示的状态消息
    Status { message: String },

    /// 暂停终端输入事件（用于交互式子进程）。
    PauseEvents {
        /// 可选的单次通知，在 UI 实际将终端释放给子进程后触发。
        ack: Option<Arc<tokio::sync::Notify>>,
    },

    /// 子进程完成后恢复终端输入事件
    ResumeEvents,

    /// 请求用户批准工具调用
    ApprovalRequired {
        id: String,
        tool_name: String,
        description: String,
        /// 每次调用审批缓存的指纹键（§5.A）。
        approval_key: String,
    },

    /// 请求工具调用的用户输入
    UserInputRequired {
        id: String,
        request: UserInputRequest,
    },

    /// 来自引擎会话的权威 API 对话状态。
    ///
    /// UI 接收细粒度的显示事件，但这些并不总是 API 对话
    /// 的无损表示。DeepSeek 可以直接发出推理内容后跟工具调用，
    /// 而没有可见的助手文本块，并且该助手消息仍然需要持久化
    /// 以便后续 `reasoning_content` 重放。
    SessionUpdated {
        messages: Vec<Message>,
        system_prompt: Option<SystemPrompt>,
        model: String,
        workspace: PathBuf,
    },

    /// 沙箱拒绝后请求用户决策
    #[allow(dead_code)]
    ElevationRequired {
        tool_id: String,
        tool_name: String,
        command: Option<String>,
        denial_reason: String,
        blocked_network: bool,
        blocked_write: bool,
    },
}

impl Event {
    /// 从分类信封创建错误事件。信封自身的
    /// `recoverable` 标志控制 UI 是否切换到离线模式。
    pub fn error(envelope: ErrorEnvelope) -> Self {
        let recoverable = envelope.recoverable;
        Event::Error {
            envelope,
            recoverable,
        }
    }

    /// 创建新的状态事件
    pub fn status(message: impl Into<String>) -> Self {
        Event::Status {
            message: message.into(),
        }
    }
}
