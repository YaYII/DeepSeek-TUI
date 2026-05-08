//! 代理卡片 — 渲染子代理状态和输出。
//!
//! 两张卡片消费 #130 邮箱流并在聊天转录本中实时渲染：
//!
//! - [`DelegateCard`] — 单个 `agent_spawn` 调用。最近 3 个操作
//!   的实时树，以及包含状态/字形/角色的标题。
//! - [`FanoutCard`] — `rlm` 扇出（或任何未来的多子分发）。
//!   工作槽的点阵（`●` 已填充，`○` 待处理）以及聚合计数行。
//!
//! 两张卡片都是由 [`apply_to_delegate`] / [`apply_to_fanout`] 更新的状态机。
//! 侧边栏（见 `tui/sidebar.rs`）将详细信息委托给转录本中处于活动状态的卡片，
//! 因此这些是主要的状态呈现面。

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::palette;
use crate::tools::subagent::MailboxMessage;
use crate::tui::widgets::tool_card::{ToolFamily, family_glyph, family_label};

/// `DelegateCard` 上保留的最大最近操作数。旧条目从头部丢弃；
/// 省略号行表示截断。
pub const DELEGATE_MAX_ACTIONS: usize = 3;

/// 委托/扇出代理的生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentLifecycle {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AgentLifecycle {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Pending => palette::TEXT_MUTED,
            Self::Running => palette::STATUS_WARNING,
            Self::Completed => palette::STATUS_SUCCESS,
            Self::Failed => palette::STATUS_ERROR,
            Self::Cancelled => palette::TEXT_MUTED,
        }
    }
}

/// 单个委托的 `agent_spawn` 调用的卡片。
///
/// 存储最近 [`DELEGATE_MAX_ACTIONS`] 个操作行；旧条目被截断，
/// 在可见尾部上方渲染一个省略号行。
#[derive(Debug, Clone)]
pub struct DelegateCard {
    pub agent_id: String,
    pub agent_type: String,
    pub status: AgentLifecycle,
    pub summary: Option<String>,
    actions: Vec<String>,
    truncated: bool,
}

impl DelegateCard {
    #[must_use]
    pub fn new(agent_id: impl Into<String>, agent_type: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            agent_type: agent_type.into(),
            status: AgentLifecycle::Pending,
            summary: None,
            actions: Vec::new(),
            truncated: false,
        }
    }

    pub fn push_action(&mut self, action: impl Into<String>) {
        self.actions.push(action.into());
        if self.actions.len() > DELEGATE_MAX_ACTIONS {
            // 每次溢出丢弃一个头部条目，使稳定状态恰好为
            // DELEGATE_MAX_ACTIONS 行；省略号行表示其余部分。
            self.actions.remove(0);
            self.truncated = true;
        }
    }

    #[must_use]
    pub fn render_lines(&self, _width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(self.actions.len() + 3);
        lines.push(card_header(
            ToolFamily::Delegate,
            self.status,
            &self.agent_type,
            &self.agent_id,
        ));
        if self.truncated {
            lines.push(Line::from(Span::styled(
                "  \u{2026}".to_string(), // …
                Style::default().fg(palette::TEXT_MUTED),
            )));
        }
        for action in &self.actions {
            lines.push(Line::from(vec![
                Span::styled("  \u{2502} ", Style::default().fg(palette::TEXT_DIM)),
                Span::styled(
                    truncate_action(action, 200),
                    Style::default().fg(palette::TEXT_TOOL_OUTPUT),
                ),
            ]));
        }
        if self.status.is_terminal()
            && let Some(summary) = self.summary.as_ref()
        {
            lines.push(Line::from(vec![
                Span::styled("  \u{2570} ", Style::default().fg(palette::TEXT_DIM)),
                Span::styled(
                    truncate_action(summary, 200),
                    Style::default().fg(self.status.color()),
                ),
            ]));
        }
        lines
    }

    /// 持有的操作数 — 为测试暴露；上限为 `DELEGATE_MAX_ACTIONS`。
    #[must_use]
    #[cfg(test)]
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    /// 头部是否被截断（旧操作已丢弃）。
    #[must_use]
    #[cfg(test)]
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

/// 扇出组中的工作槽。
#[derive(Debug, Clone)]
pub struct WorkerSlot {
    /// 稳定的逻辑工作键。即使在具体的子代理 ID 存在后仍与工作槽绑定。
    pub worker_id: String,
    /// 生成后的具体代理 ID；占位符使用工作 ID。
    pub agent_id: String,
    pub status: AgentLifecycle,
}

impl WorkerSlot {
    #[must_use]
    pub fn new(worker_id: impl Into<String>, status: AgentLifecycle) -> Self {
        let worker_id = worker_id.into();
        Self {
            agent_id: worker_id.clone(),
            worker_id,
            status,
        }
    }
}

/// `rlm`（或任何多子分发）扇出的卡片：点阵 + 聚合计数。
///
/// 槽在 `ChildSpawned` 信封到达时添加（或当工作计数预先已知时由引擎预先分配）；
/// 每个槽在观察到其 `Completed` / `Failed` / `Cancelled` 信封时独立转换。
#[derive(Debug, Clone)]
pub struct FanoutCard {
    pub kind: String,
    pub workers: Vec<WorkerSlot>,
}

impl FanoutCard {
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            workers: Vec::new(),
        }
    }

    /// 当扇出大小预先已知时预填充工作槽。
    #[allow(dead_code)]
    pub fn with_workers<I, S>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for id in ids {
            self.workers
                .push(WorkerSlot::new(id.into(), AgentLifecycle::Pending));
        }
        self
    }

    /// 按 ID 更新或插入工作槽。
    pub fn upsert_worker(&mut self, agent_id: &str, status: AgentLifecycle) {
        if let Some(slot) = self
            .workers
            .iter_mut()
            .find(|s| s.agent_id == agent_id || s.worker_id == agent_id)
        {
            slot.agent_id = agent_id.to_string();
            slot.status = status;
        } else {
            self.workers.push(WorkerSlot::new(agent_id, status));
        }
    }

    /// 将真实的代理 ID 附加到第一个待处理的占位槽。扇出卡片
    /// 在子代理存在之前从任务 ID 播种；当子代理启动时，
    /// 这保持点数稳定，而不是为同一个工作单元附加第二个圆圈。
    pub fn claim_pending_worker(&mut self, agent_id: &str, status: AgentLifecycle) {
        if let Some(slot) = self.workers.iter_mut().find(|s| s.agent_id == agent_id) {
            slot.status = status;
            return;
        }
        if let Some(slot) = self
            .workers
            .iter_mut()
            .find(|s| matches!(s.status, AgentLifecycle::Pending))
        {
            slot.agent_id = agent_id.to_string();
            slot.status = status;
            return;
        }
        self.upsert_worker(agent_id, status);
    }

    fn counts(&self) -> (usize, usize, usize, usize) {
        let mut done = 0usize;
        let mut running = 0usize;
        let mut failed = 0usize;
        let mut pending = 0usize;
        for slot in &self.workers {
            match slot.status {
                AgentLifecycle::Completed => done += 1,
                AgentLifecycle::Running => running += 1,
                AgentLifecycle::Failed | AgentLifecycle::Cancelled => failed += 1,
                AgentLifecycle::Pending => pending += 1,
            }
        }
        (done, running, failed, pending)
    }

    #[must_use]
    pub fn dot_grid(&self) -> String {
        let mut s = String::with_capacity(self.workers.len());
        for slot in &self.workers {
            let glyph = match slot.status {
                AgentLifecycle::Completed => '\u{25CF}', // ●
                AgentLifecycle::Running => '\u{25D0}',   // ◐
                AgentLifecycle::Failed => '\u{00D7}',    // ×
                AgentLifecycle::Cancelled => '\u{2298}', // ⊘
                AgentLifecycle::Pending => '\u{25CB}',   // ○
            };
            s.push(glyph);
        }
        s
    }

    #[must_use]
    pub fn render_lines(&self, _width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(3);
        let header_status = self.aggregate_status();
        let title = format!("{} ({} workers)", self.kind, self.workers.len());
        let family = if self.kind == "rlm" {
            ToolFamily::Rlm
        } else {
            ToolFamily::Fanout
        };
        lines.push(card_header(family, header_status, &self.kind, &title));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                self.dot_grid(),
                Style::default()
                    .fg(palette::DEEPSEEK_SKY)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        let (done, running, failed, pending) = self.counts();
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!(
                    "{done} done \u{00B7} {running} running \u{00B7} {failed} failed \u{00B7} {pending} pending"
                ),
                Style::default().fg(palette::TEXT_MUTED),
            ),
        ]));
        lines
    }

    fn aggregate_status(&self) -> AgentLifecycle {
        let (done, running, failed, pending) = self.counts();
        if running > 0 || pending > 0 {
            AgentLifecycle::Running
        } else if failed > 0 && done == 0 {
            AgentLifecycle::Failed
        } else if done > 0 {
            AgentLifecycle::Completed
        } else {
            AgentLifecycle::Pending
        }
    }

    /// 工作计数（通过邮箱播种或观察到的槽）。
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }
}

fn card_header(
    family: ToolFamily,
    status: AgentLifecycle,
    role: &str,
    detail: &str,
) -> Line<'static> {
    let glyph = family_glyph(family);
    let verb = family_label(family);
    let header_color = status.color();
    Line::from(vec![
        Span::styled(
            format!("{glyph} "),
            Style::default()
                .fg(header_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            verb.to_string(),
            Style::default()
                .fg(header_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(role.to_string(), Style::default().fg(palette::TEXT_PRIMARY)),
        Span::raw(" "),
        Span::styled(
            format!("[{}]", status.label()),
            Style::default().fg(header_color),
        ),
        Span::raw(" "),
        Span::styled(detail.to_string(), Style::default().fg(palette::TEXT_MUTED)),
    ])
}

fn truncate_action(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        let mut out: String = trimmed.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

/// 将邮箱信封应用于 `DelegateCard`。如果状态更改则返回 `true`（UI 可能需要重绘）；
/// 如果信封是针对不同的 `agent_id`，则返回 `false`。
pub fn apply_to_delegate(card: &mut DelegateCard, msg: &MailboxMessage) -> bool {
    if msg.agent_id() != card.agent_id {
        return false;
    }
    match msg {
        MailboxMessage::Started { .. } => {
            card.status = AgentLifecycle::Running;
        }
        MailboxMessage::Progress { status, .. } => {
            card.status = AgentLifecycle::Running;
            if !is_low_signal_progress(status) {
                card.push_action(status);
            }
        }
        MailboxMessage::ToolCallStarted { tool_name, .. } => {
            card.push_action(format!("{tool_name} running"));
        }
        MailboxMessage::ToolCallCompleted { tool_name, ok, .. } => {
            card.push_action(format!("{tool_name} {}", if *ok { "ok" } else { "failed" }));
        }
        MailboxMessage::Completed { summary, .. } => {
            card.status = AgentLifecycle::Completed;
            card.summary = Some(summary.clone());
        }
        MailboxMessage::Failed { error, .. } => {
            card.status = AgentLifecycle::Failed;
            card.summary = Some(error.clone());
        }
        MailboxMessage::Cancelled { .. } => {
            card.status = AgentLifecycle::Cancelled;
        }
        MailboxMessage::ChildSpawned { .. } => {
            // 委托卡片代表单个代理；子生成属于兄弟扇出卡片，而非此卡片。
            return false;
        }
        MailboxMessage::TokenUsage { .. } => {
            // 成本累积在调用此 apply 函数之前发生在 handle_subagent_mailbox (ui.rs) 中；
            // TokenUsage 在实践中永远不会到达此分支。
            return false;
        }
    }
    true
}

fn is_low_signal_progress(status: &str) -> bool {
    let status = status.trim().to_ascii_lowercase();
    status.contains("requesting model response")
        || status.starts_with("started (")
        || (status.starts_with("step ") && status.contains(": complete"))
}

/// 将邮箱信封应用于 `FanoutCard`。根据信封所涉及的子代理更新每个工作槽的状态。
/// 发生更改时返回 `true`。
pub fn apply_to_fanout(card: &mut FanoutCard, msg: &MailboxMessage) -> bool {
    let id = msg.agent_id();
    match msg {
        MailboxMessage::Started { .. } => {
            card.claim_pending_worker(id, AgentLifecycle::Running);
            true
        }
        MailboxMessage::Progress { .. } | MailboxMessage::ToolCallStarted { .. } => {
            card.claim_pending_worker(id, AgentLifecycle::Running);
            true
        }
        MailboxMessage::ToolCallCompleted { .. } => true,
        MailboxMessage::Completed { .. } => {
            card.upsert_worker(id, AgentLifecycle::Completed);
            true
        }
        MailboxMessage::Failed { .. } => {
            card.upsert_worker(id, AgentLifecycle::Failed);
            true
        }
        MailboxMessage::Cancelled { .. } => {
            card.upsert_worker(id, AgentLifecycle::Cancelled);
            true
        }
        MailboxMessage::ChildSpawned { child_id, .. } => {
            card.upsert_worker(child_id, AgentLifecycle::Pending);
            true
        }
        MailboxMessage::TokenUsage { .. } => {
            // 成本累积在调用此 apply 函数之前发生在 handle_subagent_mailbox (ui.rs) 中；
            // TokenUsage 在实践中永远不会到达此分支。
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_strings(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn delegate_card_truncates_to_last_three_actions_with_ellipsis() {
        let mut card = DelegateCard::new("agent_001", "general");
        card.push_action("read README.md");
        card.push_action("grep TODO");
        card.push_action("edit src/lib.rs");
        // 未达到限制 — 尚未截断。
        assert!(!card.truncated());
        assert_eq!(card.action_count(), DELEGATE_MAX_ACTIONS);

        card.push_action("write tests");
        card.push_action("run cargo test");
        assert!(card.truncated(), "溢出时截断标志翻转");
        assert_eq!(
            card.action_count(),
            DELEGATE_MAX_ACTIONS,
            "稳定的稳态大小"
        );

        let rendered = render_to_strings(&card.render_lines(80));
        assert!(
            rendered.iter().any(|line| line.contains('\u{2026}')),
            "省略号指示符必须渲染：{rendered:?}"
        );
        // 最旧的两个操作 ("read README.md", "grep TODO") 已被丢弃。
        assert!(
            !rendered.iter().any(|line| line.contains("read README.md")),
            "最旧操作已被驱逐：{rendered:?}"
        );
        assert!(
            rendered.iter().any(|line| line.contains("run cargo test")),
            "最新操作已保留：{rendered:?}"
        );
        assert!(
            rendered.iter().any(|line| line.contains("write tests")),
            "次新操作已保留：{rendered:?}"
        );
        assert!(
            rendered.iter().any(|line| line.contains("edit src/lib.rs")),
            "第三新操作已保留：{rendered:?}"
        );
    }

    #[test]
    fn delegate_card_terminal_status_renders_summary_row() {
        let mut card = DelegateCard::new("agent_002", "explore");
        card.push_action("listing files");
        let msg = MailboxMessage::Completed {
            agent_id: "agent_002".into(),
            summary: "scanned 42 files, no TODOs found".into(),
        };
        assert!(apply_to_delegate(&mut card, &msg));
        assert_eq!(card.status, AgentLifecycle::Completed);
        let rendered = render_to_strings(&card.render_lines(80));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("scanned 42 files")),
            "总结行在终端状态上渲染：{rendered:?}"
        );
    }

    #[test]
    fn delegate_card_ignores_low_signal_scheduler_progress() {
        let mut card = DelegateCard::new("agent_003", "general");
        let msg = MailboxMessage::progress("agent_003", "step 1/100: requesting model response");

        assert!(apply_to_delegate(&mut card, &msg));
        assert_eq!(card.status, AgentLifecycle::Running);
        assert_eq!(
            card.action_count(),
            0,
            "调度器进度不应成为过时的转录本行"
        );

        let rendered = render_to_strings(&card.render_lines(80)).join("\n");
        assert!(!rendered.contains("step 1/100"), "{rendered}");
        assert!(
            !rendered.contains("requesting model response"),
            "{rendered}"
        );
    }

    #[test]
    fn delegate_tool_rows_omit_internal_step_numbers() {
        let mut card = DelegateCard::new("agent_004", "general");

        assert!(apply_to_delegate(
            &mut card,
            &MailboxMessage::ToolCallStarted {
                agent_id: "agent_004".into(),
                tool_name: "read_file".into(),
                step: 7,
            }
        ));
        assert!(apply_to_delegate(
            &mut card,
            &MailboxMessage::ToolCallCompleted {
                agent_id: "agent_004".into(),
                tool_name: "read_file".into(),
                step: 7,
                ok: true,
            }
        ));

        let rendered = render_to_strings(&card.render_lines(80)).join("\n");
        assert!(rendered.contains("read_file"), "{rendered}");
        assert!(
            !rendered.contains("[7]"),
            "内部循环步骤号在实时卡片中没有用：{rendered}"
        );
    }

    #[test]
    fn delegate_card_ignores_envelopes_for_other_agents() {
        let mut card = DelegateCard::new("agent_a", "general");
        let other = MailboxMessage::progress("agent_b", "noise");
        assert!(!apply_to_delegate(&mut card, &other));
        assert_eq!(card.action_count(), 0);
    }

    #[test]
    fn fanout_card_dot_grid_renders_stateful_worker_slots() {
        let mut card = FanoutCard::new("fanout")
            .with_workers(["w_1", "w_2", "w_3", "w_4", "w_5", "w_6", "w_7"]);
        card.upsert_worker("w_1", AgentLifecycle::Completed);
        card.upsert_worker("w_2", AgentLifecycle::Completed);
        card.upsert_worker("w_3", AgentLifecycle::Running);
        card.upsert_worker("w_4", AgentLifecycle::Failed);
        // 5/6/7 保持 Pending。

        // 已完成填充；运行中和失败是不同的；待处理保持开放。
        assert_eq!(
            card.dot_grid(),
            "\u{25CF}\u{25CF}\u{25D0}\u{00D7}\u{25CB}\u{25CB}\u{25CB}"
        );
    }

    #[test]
    fn fanout_card_aggregate_counts_match_dot_grid() {
        let mut card = FanoutCard::new("rlm").with_workers(["w_1", "w_2", "w_3", "w_4"]);
        card.upsert_worker("w_1", AgentLifecycle::Completed);
        card.upsert_worker("w_2", AgentLifecycle::Completed);
        card.upsert_worker("w_3", AgentLifecycle::Completed);
        card.upsert_worker("w_4", AgentLifecycle::Failed);
        let rendered = render_to_strings(&card.render_lines(80));
        // 统计行也携带 "running"；标题可能通过生命周期状态徽章单独提到 "done"。
        let stats = rendered
            .iter()
            .find(|line| line.contains("running") && line.contains("pending"))
            .expect("counts line present");
        assert!(stats.contains("3 done"), "已完成计数：{stats}");
        assert!(
            stats.contains("1 failed"),
            "失败/取消合并到同一个桶中：{stats}"
        );
        assert!(stats.contains("0 running"), "无运行中：{stats}");
        assert!(stats.contains("0 pending"), "无待处理：{stats}");
    }

    #[test]
    fn fanout_apply_inserts_unknown_worker_via_child_spawned() {
        let mut card = FanoutCard::new("fanout");
        let msg = MailboxMessage::ChildSpawned {
            parent_id: "root".into(),
            child_id: "agent_late".into(),
        };
        assert!(apply_to_fanout(&mut card, &msg));
        assert_eq!(card.worker_count(), 1);
        assert_eq!(card.workers[0].agent_id, "agent_late");
        assert_eq!(card.workers[0].status, AgentLifecycle::Pending);
    }

    #[test]
    fn fanout_started_claims_seeded_pending_slot_without_growing_grid() {
        let mut card = FanoutCard::new("fanout").with_workers(["task:a", "task:b"]);
        let started =
            MailboxMessage::started("agent_live", crate::tools::subagent::SubAgentType::General);

        assert!(apply_to_fanout(&mut card, &started));

        assert_eq!(card.worker_count(), 2);
        assert_eq!(card.workers[0].agent_id, "agent_live");
        assert_eq!(card.workers[0].status, AgentLifecycle::Running);
        assert_eq!(card.workers[1].agent_id, "task:b");
        assert_eq!(card.workers[1].status, AgentLifecycle::Pending);
    }

    #[test]
    fn fanout_apply_transitions_worker_through_lifecycle() {
        let mut card = FanoutCard::new("fanout").with_workers(["w_1"]);
        let started = MailboxMessage::started("w_1", crate::tools::subagent::SubAgentType::General);
        apply_to_fanout(&mut card, &started);
        assert_eq!(card.workers[0].status, AgentLifecycle::Running);

        let done = MailboxMessage::Completed {
            agent_id: "w_1".into(),
            summary: "ok".into(),
        };
        apply_to_fanout(&mut card, &done);
        assert_eq!(card.workers[0].status, AgentLifecycle::Completed);
    }

    #[test]
    fn fanout_dot_grid_arithmetic_for_various_n() {
        // 抽查几种扇出大小及其状态组合；这是问题验收中提到的算术快照。
        let cases: &[(usize, usize, &str)] = &[
            (1, 0, "\u{25CB}"),
            (1, 1, "\u{25CF}"),
            (3, 2, "\u{25CF}\u{25CF}\u{25CB}"),
            (
                7,
                3,
                "\u{25CF}\u{25CF}\u{25CF}\u{25CB}\u{25CB}\u{25CB}\u{25CB}",
            ),
        ];
        for (total, done, expected) in cases {
            let ids: Vec<String> = (0..*total).map(|i| format!("w_{i}")).collect();
            let mut card = FanoutCard::new("fanout").with_workers(ids.iter().cloned());
            for id in ids.iter().take(*done) {
                card.upsert_worker(id, AgentLifecycle::Completed);
            }
            assert_eq!(
                card.dot_grid(),
                *expected,
                "扇出点阵在 total={total} done={done} 时",
            );
        }
    }
}
