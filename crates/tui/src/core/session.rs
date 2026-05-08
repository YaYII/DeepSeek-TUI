//! 核心引擎的会话状态管理。
//!
//! 追踪对话历史、令牌使用情况和会话元数据。

use crate::cycle_manager::CycleBriefing;
use crate::models::{Message, SystemPrompt, Usage};
use crate::project_context::{ProjectContext, load_project_context_with_parents};
use crate::tui::approval::ApprovalMode;
use crate::working_set::WorkingSet;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// 引擎的会话状态。
#[derive(Debug, Clone)]
pub struct Session {
    /// 正在使用的模型
    pub model: String,

    /// DeepSeek 思考模式的推理力度层级：
    /// `"off" | "low" | "medium" | "high" | "max"`。`None` 让提供商
    /// 应用其自身的默认值。
    pub reasoning_effort: Option<String>,
    /// 用户是否选择了自动推理力度。
    pub reasoning_effort_auto: bool,

    /// 用户是否选择了自动模型路由。
    pub auto_model: bool,

    /// 工作区目录
    pub workspace: PathBuf,

    /// 系统提示（可选）
    pub system_prompt: Option<SystemPrompt>,
    /// 上次组装的稳定系统提示的哈希值。用于在未更改时
    /// 避免替换 `system_prompt`。
    pub last_system_prompt_hash: Option<u64>,
    /// 由上下文压缩生成的持久摘要块。
    pub compaction_summary_prompt: Option<SystemPrompt>,

    /// 对话历史（API 格式）
    pub messages: Vec<Message>,

    /// 本会话中使用的总令牌数
    pub total_usage: SessionUsage,

    /// 是否允许执行 shell
    pub allow_shell: bool,

    /// 是否信任工作区外的路径
    pub trust_mode: bool,

    /// 当前会话是否应自动批准工具安全检查。
    pub auto_approve: bool,

    /// 用于引导系统提示的实时 UI 审批策略。
    pub approval_mode: ApprovalMode,

    /// 笔记文件路径
    pub notes_path: PathBuf,

    /// MCP 配置路径
    pub mcp_config_path: PathBuf,

    /// 会话 ID（用于追踪）
    pub id: String,

    /// 从 AGENTS.md 等加载的项目上下文。
    pub project_context: Option<ProjectContext>,

    /// 用于上下文管理的仓库感知工作集。
    pub working_set: WorkingSet,

    /// 本会话中跨越的循环边界数（issue #124）。
    /// 活跃循环索引为 `cycle_count + 1`（循环对用户从 1 开始计数）。
    pub cycle_count: u32,

    /// *当前* 循环的 UTC 开始时间。在引擎重置对话缓冲区时更新。
    /// 由归档头部和 `/cycles` 命令的显示使用。
    pub current_cycle_started: DateTime<Utc>,

    /// 在过去的循环边界产生的简报，按时间顺序排列。
    /// 有界增长：每个循环一个条目，简报上限约 3,000 令牌。
    pub cycle_briefings: Vec<CycleBriefing>,
}

/// 会话的累计使用统计。
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_field_names)]
pub struct SessionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[allow(dead_code)]
    pub cache_creation_input_tokens: u64,
    #[allow(dead_code)]
    pub cache_read_input_tokens: u64,
}

impl SessionUsage {
    /// 从一轮中添加使用量
    pub fn add(&mut self, usage: &Usage) {
        self.input_tokens += u64::from(usage.input_tokens);
        self.output_tokens += u64::from(usage.output_tokens);
        if let Some(tokens) = usage.prompt_cache_miss_tokens {
            self.cache_creation_input_tokens += u64::from(tokens);
        }
        if let Some(tokens) = usage.prompt_cache_hit_tokens {
            self.cache_read_input_tokens += u64::from(tokens);
        }
    }
}

impl Session {
    /// 创建新会话
    pub fn new(
        model: String,
        workspace: PathBuf,
        allow_shell: bool,
        trust_mode: bool,
        notes_path: PathBuf,
        mcp_config_path: PathBuf,
    ) -> Self {
        // 从 AGENTS.md、CLAUDE.md 等加载项目上下文。
        let project_context = load_project_context_with_parents(&workspace);
        let has_context = project_context.has_instructions();

        Self {
            model,
            reasoning_effort: None,
            reasoning_effort_auto: false,
            auto_model: false,
            workspace,
            system_prompt: None,
            compaction_summary_prompt: None,
            messages: Vec::new(),
            total_usage: SessionUsage::default(),
            allow_shell,
            trust_mode,
            auto_approve: false,
            approval_mode: ApprovalMode::Suggest,
            notes_path,
            mcp_config_path,
            id: uuid::Uuid::new_v4().to_string(),
            project_context: if has_context {
                Some(project_context)
            } else {
                None
            },
            last_system_prompt_hash: None,
            working_set: WorkingSet::default(),
            cycle_count: 0,
            current_cycle_started: Utc::now(),
            cycle_briefings: Vec::new(),
        }
    }

    /// 向对话中添加一条消息
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// 从当前消息重建工作集（尽力而为）。
    pub fn rebuild_working_set(&mut self) {
        self.working_set
            .rebuild_from_messages(&self.messages, &self.workspace);
    }
}
