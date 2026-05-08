//! 轮次上下文与追踪。
//!
//! 一个"轮次"是一条用户消息及其产生的 AI 响应，
//! 包括期间发生的任何工具调用。
//!
//! ## 快照生命周期钩子
//!
//! [`pre_turn_snapshot`] 和 [`post_turn_snapshot`] 在轮次前后
//! 将工作区级快照存入侧 git 仓库（参见 `crate::snapshot`）。
//! 它们故意设计为非阻塞和非致命的：任何 IO 错误都会以 WARN 级别记录并吞掉，
//! 因此损坏的文件系统或缺失的 `git` 二进制文件永远不会使代理循环脱轨。
//! `/restore N` 和 `revert_turn` 工具都消费这些快照。

use crate::models::Usage;
use crate::snapshot::SnapshotRepo;
use std::path::Path;
use std::time::{Duration, Instant};

/// 单个轮次（用户消息 + AI 响应）的上下文。
#[derive(Debug)]
pub struct TurnContext {
    /// 轮次 ID
    pub id: String,

    /// 轮次开始时间
    #[allow(dead_code)]
    pub started_at: Instant,

    /// 轮次中的当前步骤（工具调用迭代）
    pub step: u32,

    /// 允许的最大步数
    pub max_steps: u32,

    /// 本轮次中进行的工具调用
    pub tool_calls: Vec<TurnToolCall>,

    /// 轮次是否已被取消
    #[allow(dead_code)]
    pub cancelled: bool,

    /// 本轮次的使用量
    pub usage: Usage,
}

/// 轮次内工具调用的记录。
#[derive(Debug, Clone)]
pub struct TurnToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration: Option<Duration>,
}

impl TurnContext {
    /// 创建新的轮次上下文
    pub fn new(max_steps: u32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: Instant::now(),
            step: 0,
            max_steps,
            tool_calls: Vec::new(),
            cancelled: false,
            usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
                ..Usage::default()
            },
        }
    }

    /// 递增步骤计数器
    pub fn next_step(&mut self) -> bool {
        self.step += 1;
        self.step <= self.max_steps
    }

    /// 检查轮次是否已达到最大步数
    pub fn at_max_steps(&self) -> bool {
        self.step >= self.max_steps
    }

    /// 记录一次工具调用
    pub fn record_tool_call(&mut self, call: TurnToolCall) {
        self.tool_calls.push(call);
    }

    /// 取消轮次
    #[allow(dead_code)]
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// 获取已用时间
    #[allow(dead_code)]
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// 从 API 响应中添加使用量
    pub fn add_usage(&mut self, usage: &Usage) {
        self.usage.input_tokens += usage.input_tokens;
        self.usage.output_tokens += usage.output_tokens;
        self.usage.prompt_cache_hit_tokens = add_optional_usage(
            self.usage.prompt_cache_hit_tokens,
            usage.prompt_cache_hit_tokens,
        );
        self.usage.prompt_cache_miss_tokens = add_optional_usage(
            self.usage.prompt_cache_miss_tokens,
            usage.prompt_cache_miss_tokens,
        );
        self.usage.reasoning_tokens =
            add_optional_usage(self.usage.reasoning_tokens, usage.reasoning_tokens);
    }
}

fn add_optional_usage(total: Option<u32>, delta: Option<u32>) -> Option<u32> {
    match (total, delta) {
        (Some(total), Some(delta)) => Some(total.saturating_add(delta)),
        (None, Some(delta)) => Some(delta),
        (Some(total), None) => Some(total),
        (None, None) => None,
    }
}

/// 获取 `pre-turn:<seq>` 工作区快照。
///
/// 成功时返回快照 SHA，任何错误返回 `None`。错误
/// 以 WARN 级别记录；轮次循环不能阻塞于此。
pub fn pre_turn_snapshot(workspace: &Path, turn_seq: u64) -> Option<String> {
    snapshot_with_label(workspace, &format!("pre-turn:{turn_seq}"))
}

/// 获取 `tool:<call_id>` 工作区快照，在执行文件修改工具调用
///（write_file、edit_file、apply_patch）之前获取。
///
/// 这支持精确撤销：`/undo` 可以恢复到最近的
/// `tool:<call_id>` 快照，以仅撤销最后一次文件写入。
///
/// 成功时返回快照 SHA，任何错误返回 `None`。错误
/// 以 WARN 级别记录，非致命。
pub fn pre_tool_snapshot(workspace: &Path, call_id: &str) -> Option<String> {
    snapshot_with_label(workspace, &format!("tool:{call_id}"))
}

/// 获取 `post-turn:<seq>` 工作区快照。故障模型与
/// [`pre_turn_snapshot`] 相同。
pub fn post_turn_snapshot(workspace: &Path, turn_seq: u64) -> Option<String> {
    snapshot_with_label(workspace, &format!("post-turn:{turn_seq}"))
}

fn snapshot_with_label(workspace: &Path, label: &str) -> Option<String> {
    match SnapshotRepo::open_or_init(workspace) {
        Ok(repo) => match repo.snapshot(label) {
            Ok(id) => Some(id.0),
            Err(e) => {
                tracing::warn!(target: "snapshot", "snapshot '{label}' failed: {e}");
                None
            }
        },
        Err(e) => {
            tracing::warn!(target: "snapshot", "snapshot repo init failed: {e}");
            None
        }
    }
}

impl TurnToolCall {
    /// 创建新的工具调用记录
    pub fn new(id: String, name: String, input: serde_json::Value) -> Self {
        Self {
            id,
            name,
            input,
            result: None,
            error: None,
            duration: None,
        }
    }

    /// 设置结果
    pub fn set_result(&mut self, result: String, duration: Duration) {
        self.result = Some(result);
        self.duration = Some(duration);
    }

    /// 设置错误
    pub fn set_error(&mut self, error: String, duration: Duration) {
        self.error = Some(error);
        self.duration = Some(duration);
    }
}
