//! 侧边栏 — 会话信息、文件树和工具状态的侧面板。

use std::fmt::Write;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Widget,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

use crate::deepseek_theme::Theme;
use crate::palette;
use crate::tools::plan::StepStatus;
use crate::tools::subagent::SubAgentStatus;
use crate::tools::todo::TodoStatus;

use super::app::{App, SidebarFocus};
use super::history::{HistoryCell, ToolCell, ToolStatus};
use super::subagent_routing::active_fanout_counts;
use super::ui::truncate_line_to_width;

pub fn render_sidebar(f: &mut Frame, area: Rect, app: &App) {
    if area.width < 24 || area.height < 8 {
        // 在区域上绘制样式化块，以便来自前一（更宽）帧的过时单元格
        // 不会作为渗透伪影持续存在 (#400)。
        Block::default()
            .style(Style::default().bg(app.ui_theme.surface_bg))
            .render(area, f.buffer_mut());
        return;
    }

    match app.sidebar_focus {
        SidebarFocus::Auto => render_sidebar_auto(f, area, app),
        SidebarFocus::Plan => render_sidebar_plan(f, area, app),
        SidebarFocus::Todos => render_sidebar_todos(f, area, app),
        SidebarFocus::Tasks => render_sidebar_tasks(f, area, app),
        SidebarFocus::Agents => render_sidebar_subagents(f, area, app),
        SidebarFocus::Context => render_context_panel(f, area, app),
    }
}

/// 构建自动模式面板堆栈。空面板折叠为零高度，
/// 因此非空面板获得整个侧边栏空间。没有这个，Plan 会被裁剪，
/// 因为 Todos/Tasks/Agents 即使无内容也会各保留 25% 的高度。
/// Plan 始终渲染（它拥有会话范围的空状态提示）。
fn render_sidebar_auto(f: &mut Frame, area: Rect, app: &App) {
    #[derive(Clone, Copy)]
    enum Panel {
        Plan,
        Todos,
        Tasks,
        Agents,
        Context,
    }

    let todos_empty = app
        .todos
        .try_lock()
        .map(|todos| todos.snapshot().items.is_empty())
        .unwrap_or(false); // assume non-empty when locked so we don't hide updating data
    let tasks_empty = app.runtime_turn_id.is_none() && app.task_panel.is_empty();
    let agents_empty = app.subagent_cache.is_empty()
        && app.agent_progress.is_empty()
        && active_fanout_counts(app).is_none()
        && !foreground_rlm_running(app);

    let mut visible: Vec<Panel> = Vec::with_capacity(5);
    visible.push(Panel::Plan);
    if !todos_empty {
        visible.push(Panel::Todos);
    }
    if !tasks_empty {
        visible.push(Panel::Tasks);
    }
    if !agents_empty {
        visible.push(Panel::Agents);
    }
    if app.context_panel {
        visible.push(Panel::Context);
    }

    let constraints: Vec<Constraint> = match visible.len() {
        1 => vec![Constraint::Min(0)],
        2 => vec![Constraint::Percentage(50), Constraint::Min(0)],
        3 => vec![
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Min(0),
        ],
        4 => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Min(6),
        ],
        _ => vec![
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Min(6),
        ],
    };

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (panel, rect) in visible.iter().zip(sections.iter()) {
        match panel {
            Panel::Plan => render_sidebar_plan(f, *rect, app),
            Panel::Todos => render_sidebar_todos(f, *rect, app),
            Panel::Tasks => render_sidebar_tasks(f, *rect, app),
            Panel::Agents => render_sidebar_subagents(f, *rect, app),
            Panel::Context => render_context_panel(f, *rect, app),
        }
    }
}

/// 计划部分是 `update_plan` 工具输出的**唯一真实来源** (#408)。
/// 它与待办部分截然不同：待办是用户或模型正在跟踪的检查清单工作项；
/// 计划步骤是模型通过 `update_plan` 记录的更高级策略。
/// 该面板还承载两个不属于其他部分的会话范围指示器 —
/// 目标（`/goal`）和周期计数器（#124）— 因为它们共享
/// "代理正在尝试做什么，宏观图景"这一主题。
///
/// 当面板完全为空（无目标、无周期、无计划）时，
/// 它会渲染为一个安静的部分，底部有一条暗淡的提示，
/// 而不是过去显示的直白的"无活动计划"占位符。
/// 那会让用户怀疑面板是否坏了；
/// 而这条提示会告诉他们该面板的作用以及如何填充它。
fn render_sidebar_plan(f: &mut Frame, area: Rect, app: &App) {
    if area.height < 3 {
        return;
    }

    let theme = Theme::for_palette_mode(app.ui_theme.mode);
    let content_width = area.width.saturating_sub(4) as usize;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(usize::from(area.height).max(4));

    // === 目标模式 (#397) — 金色轮廓，与待办项匹配 ===
    if let Some(ref objective) = app.goal.goal_objective {
        lines.push(Line::from(Span::styled(
            format!(
                "◆ {}",
                truncate_line_to_width(objective, content_width.max(1))
            ),
            Style::default()
                .fg(palette::STATUS_WARNING)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )));
        if let Some(budget) = app.goal.goal_token_budget {
            let used = app.session.total_conversation_tokens;
            let pct = if budget > 0 {
                ((used as f64 / budget as f64) * 100.0).min(100.0)
            } else {
                0.0
            };
            let bar_width = content_width.min(20);
            let filled = ((pct / 100.0) * bar_width as f64) as usize;
            let bar = format!(
                "[{}{}] {:.0}%",
                "█".repeat(filled),
                "░".repeat(bar_width.saturating_sub(filled)),
                pct
            );
            lines.push(Line::from(Span::styled(
                format!("  tokens: {used}/{budget} {}", bar),
                Style::default().fg(palette::TEXT_MUTED),
            )));
        }
        // 金色分隔线
        lines.push(Line::from(Span::styled(
            "─".repeat(content_width.min(24)),
            Style::default().fg(palette::STATUS_WARNING),
        )));
    }

    // 周期指示器（issue #124）。仅在边界触发后显示 —
    // cycle_count == 0 的新用户不需要这行装饰。
    if app.cycle_count > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "cycles: {} (active: {})",
                app.cycle_count,
                app.cycle_count.saturating_add(1)
            ),
            Style::default().fg(theme.plan_summary_color),
        )));
    }

    match app.plan_state.try_lock() {
        Ok(plan) => {
            if plan.is_empty() {
                // 过去，每当渲染时如果没有计划步骤，这里就会显示直白的
                // "无活动计划"占位符，即使
                // 用户设置了目标或已经历周期 — 这让面板
                // 看起来像是坏了。#408 之后我们改为发出一个安静的提示，
                // 解释该面板的作用，但仅当
                // *所有*面板信号都为空时，这样不会
                // 挤占已经有目标/周期
                // 指示器的面板。
                let nothing_above = app.goal.goal_objective.is_none() && app.cycle_count == 0;
                if nothing_above {
                    lines.push(Line::from(Span::styled(
                        plan_panel_empty_hint(content_width.max(1)),
                        Style::default().fg(palette::TEXT_MUTED).italic(),
                    )));
                }
            } else {
                let (pending, in_progress, completed) = plan.counts();
                let total = pending + in_progress + completed;
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}%", plan.progress_percent()),
                        Style::default().fg(theme.plan_progress_color).bold(),
                    ),
                    Span::styled(
                        format!(" complete ({completed}/{total})"),
                        Style::default().fg(theme.plan_summary_color),
                    ),
                ]));

                if let Some(explanation) = plan.explanation() {
                    lines.push(Line::from(Span::styled(
                        truncate_line_to_width(explanation, content_width.max(1)),
                        Style::default().fg(theme.plan_explanation_color),
                    )));
                }

                let usable_rows = area.height.saturating_sub(3) as usize;
                let max_steps = usable_rows.saturating_sub(lines.len());
                for step in plan.steps().iter().take(max_steps) {
                    let (prefix, color) = match &step.status {
                        StepStatus::Pending => ("[ ]", theme.plan_pending_color),
                        StepStatus::InProgress => ("[~]", theme.plan_in_progress_color),
                        StepStatus::Completed => ("[x]", theme.plan_completed_color),
                    };
                    let mut text = format!("{prefix} {}", step.text);
                    let elapsed = step.elapsed_str();
                    if !elapsed.is_empty() {
                        let _ = write!(text, " ({elapsed})");
                    }
                    lines.push(Line::from(Span::styled(
                        truncate_line_to_width(&text, content_width.max(1)),
                        Style::default().fg(color),
                    )));
                }

                let remaining = plan.steps().len().saturating_sub(max_steps);
                if remaining > 0 {
                    lines.push(Line::from(Span::styled(
                        format!("还有 {remaining} 步"),
                        Style::default().fg(theme.plan_summary_color),
                    )));
                }
            }
        }
        Err(_) => {
            lines.push(Line::from(Span::styled(
                "计划状态更新中...",
                Style::default().fg(theme.plan_summary_color),
            )));
        }
    }

    render_sidebar_section(f, area, "计划", lines, app);
}

/// One-line hint shown when the Plan section has nothing to display
/// (no goal, no cycle, no steps). Ellipsizes for narrow widths so
/// even a 24-column sidebar doesn't wrap mid-word. Visible across
/// modes — the panel's role doesn't change between Plan / Agent /
/// YOLO; only its content does.
#[must_use]
fn plan_panel_empty_hint(content_width: usize) -> String {
    let full = "跟踪 update_plan / /goal / 周期";
    truncate_line_to_width(full, content_width)
}

fn render_sidebar_todos(f: &mut Frame, area: Rect, app: &App) {
    if area.height < 3 {
        return;
    }

    let content_width = area.width.saturating_sub(4) as usize;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(usize::from(area.height).max(4));

    match app.todos.try_lock() {
        Ok(todos) => {
            let snapshot = todos.snapshot();
            if snapshot.items.is_empty() {
                lines.push(Line::from(Span::styled(
                    "无待办",
                    Style::default().fg(palette::TEXT_MUTED),
                )));
            } else {
                let total = snapshot.items.len();
                let completed = snapshot
                    .items
                    .iter()
                    .filter(|item| item.status == TodoStatus::Completed)
                    .count();
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}%", snapshot.completion_pct),
                        Style::default().fg(palette::STATUS_SUCCESS).bold(),
                    ),
                    Span::styled(
                        format!(" complete ({completed}/{total})"),
                        Style::default().fg(palette::TEXT_MUTED),
                    ),
                ]));

                let usable_rows = area.height.saturating_sub(3) as usize;
                let max_items = usable_rows.saturating_sub(lines.len());
                for item in snapshot.items.iter().take(max_items) {
                    let (prefix, color) = match item.status {
                        TodoStatus::Pending => ("[ ]", palette::TEXT_MUTED),
                        TodoStatus::InProgress => ("[~]", palette::STATUS_WARNING),
                        TodoStatus::Completed => ("[x]", palette::STATUS_SUCCESS),
                    };
                    let text = format!("{prefix} #{} {}", item.id, item.content);
                    lines.push(Line::from(Span::styled(
                        truncate_line_to_width(&text, content_width.max(1)),
                        Style::default().fg(color),
                    )));
                }

                let remaining = snapshot.items.len().saturating_sub(max_items);
                if remaining > 0 {
                    lines.push(Line::from(Span::styled(
                        format!("还有 {remaining} 个待办"),
                        Style::default().fg(palette::TEXT_MUTED),
                    )));
                }
            }
        }
        Err(_) => {
            lines.push(Line::from(Span::styled(
                "待办列表更新中...",
                Style::default().fg(palette::TEXT_MUTED),
            )));
        }
    }

    render_sidebar_section(f, area, "待办", lines, app);
}

fn render_sidebar_tasks(f: &mut Frame, area: Rect, app: &App) {
    if area.height < 3 {
        return;
    }

    let content_width = area.width.saturating_sub(4) as usize;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(usize::from(area.height).max(4));

    if let Some(turn_id) = app.runtime_turn_id.as_ref() {
        let status = app
            .runtime_turn_status
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        lines.push(Line::from(Span::styled(
            truncate_line_to_width(
                &format!("turn {} ({status})", truncate_line_to_width(turn_id, 12)),
                content_width.max(1),
            ),
            Style::default().fg(palette::DEEPSEEK_SKY),
        )));
    }

    if app.task_panel.is_empty() {
        lines.push(Line::from(Span::styled(
            "无活动任务",
            Style::default().fg(palette::TEXT_MUTED),
        )));
    } else {
        let running = app
            .task_panel
            .iter()
            .filter(|task| task.status == "running")
            .count();
        lines.push(Line::from(vec![
            Span::styled(
                if running == app.task_panel.len() {
                    format!("{running} running")
                } else {
                    format!("{} active", app.task_panel.len())
                },
                Style::default().fg(palette::DEEPSEEK_SKY).bold(),
            ),
            Span::styled(
                if running == app.task_panel.len() {
                    String::new()
                } else {
                    format!(" ({running} running)")
                },
                Style::default().fg(palette::TEXT_MUTED),
            ),
        ]));

        let usable_rows = area.height.saturating_sub(3) as usize;
        let max_items = usable_rows.saturating_sub(lines.len());
        for task in app.task_panel.iter().take(max_items) {
            let color = match task.status.as_str() {
                "queued" => palette::TEXT_MUTED,
                "running" => palette::STATUS_WARNING,
                "completed" => palette::STATUS_SUCCESS,
                "failed" => palette::STATUS_ERROR,
                "canceled" => palette::TEXT_DIM,
                _ => palette::TEXT_MUTED,
            };
            let duration = task
                .duration_ms
                .map(|ms| format!("{:.1}s", ms as f64 / 1000.0))
                .unwrap_or_else(|| "-".to_string());
            let label = format!(
                "{} {} {}",
                truncate_line_to_width(&task.id, 10),
                task.status,
                duration
            );
            lines.push(Line::from(Span::styled(
                truncate_line_to_width(&label, content_width.max(1)),
                Style::default().fg(color),
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "  {}",
                    truncate_line_to_width(
                        &task.prompt_summary,
                        content_width.saturating_sub(2).max(1)
                    )
                ),
                Style::default().fg(palette::TEXT_DIM),
            )));
        }
    }

    render_sidebar_section(f, area, "任务", lines, app);
}

fn render_sidebar_subagents(f: &mut Frame, area: Rect, app: &App) {
    if area.height < 3 {
        return;
    }

    let content_width = area.width.saturating_sub(4) as usize;

    // 降级为导航器（issue #128）：位于转录本中的 DelegateCard /
    // FanoutCard 现在承载实时操作树和点阵网格。侧边栏
    // 仅显示计数 + 角色组合，以便用户一目了然地扫描并行工作，
    // 并滚动到匹配的转录卡片以了解详情。
    let cached_ids: std::collections::HashSet<&str> = app
        .subagent_cache
        .iter()
        .map(|agent| agent.agent_id.as_str())
        .collect();
    let progress_only_count = app
        .agent_progress
        .keys()
        .filter(|id| !cached_ids.contains(id.as_str()))
        .count();
    let cached_running = app
        .subagent_cache
        .iter()
        .filter(|agent| matches!(agent.status, SubAgentStatus::Running))
        .count();
    let role_counts: std::collections::BTreeMap<String, usize> =
        app.subagent_cache
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut acc, agent| {
                *acc.entry(agent.agent_type.as_str().to_string())
                    .or_insert(0) += 1;
                acc
            });
    let (fanout_running, fanout_total) = active_fanout_counts(app)
        .map(|(running, total)| (running, Some(total)))
        .unwrap_or((0, None));
    let foreground_rlm_running = foreground_rlm_running(app);

    let summary = SidebarSubagentSummary {
        cached_total: app.subagent_cache.len(),
        cached_running,
        progress_only_count,
        fanout_total,
        fanout_running,
        foreground_rlm_running,
        role_counts,
    };
    let lines = subagent_navigator_lines(&summary, content_width);

    render_sidebar_section(f, area, "代理", lines, app);
}

/// 子代理侧边栏所需数据的最小投影。从
/// `render_sidebar_subagents` 中提取出来，以便无需完整的
/// `App` 即可对渲染进行快照测试。
#[derive(Debug, Clone, Default)]
pub struct SidebarSubagentSummary {
    pub cached_total: usize,
    pub cached_running: usize,
    pub progress_only_count: usize,
    pub fanout_total: Option<usize>,
    pub fanout_running: usize,
    pub foreground_rlm_running: bool,
    pub role_counts: std::collections::BTreeMap<String, usize>,
}

fn foreground_rlm_running(app: &App) -> bool {
    app.active_cell.as_ref().is_some_and(|active| {
        active.entries().iter().any(|entry| {
            matches!(
                entry,
                HistoryCell::Tool(ToolCell::Generic(generic))
                    if generic.name == "rlm" && generic.status == ToolStatus::Running
            )
        })
    })
}

/// 从摘要投影构建降级导航器行。公开
/// 供此模块中的快照测试使用。
pub fn subagent_navigator_lines(
    summary: &SidebarSubagentSummary,
    content_width: usize,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(4);

    let fanout_total = summary.fanout_total.unwrap_or(0);
    if summary.cached_total == 0
        && summary.progress_only_count == 0
        && fanout_total == 0
        && !summary.foreground_rlm_running
    {
        lines.push(Line::from(Span::styled(
            "无代理",
            Style::default().fg(palette::TEXT_MUTED),
        )));
        return lines;
    }

    let (live_running, total) = if let Some(total) = summary.fanout_total {
        (summary.fanout_running, total)
    } else {
        (
            summary.cached_running + summary.progress_only_count,
            summary.cached_total + summary.progress_only_count,
        )
    };
    let done = total.saturating_sub(live_running);
    let header = if live_running > 0 {
        vec![
            Span::styled(
                format!("{live_running} running"),
                Style::default().fg(palette::DEEPSEEK_SKY).bold(),
            ),
            Span::styled(
                format!(" / {total}"),
                Style::default().fg(palette::TEXT_MUTED),
            ),
        ]
    } else {
        vec![Span::styled(
            format!("{done} done"),
            Style::default().fg(palette::STATUS_SUCCESS),
        )]
    };
    lines.push(Line::from(header));

    if !summary.role_counts.is_empty() {
        let mix: Vec<String> = summary
            .role_counts
            .iter()
            .map(|(role, count)| format!("{count} {role}"))
            .collect();
        let role_line = mix.join(" \u{00B7} ");
        lines.push(Line::from(Span::styled(
            truncate_line_to_width(&role_line, content_width.max(1)),
            Style::default().fg(palette::TEXT_DIM),
        )));
    }

    if summary.foreground_rlm_running {
        lines.push(Line::from(vec![
            Span::styled("RLM", Style::default().fg(palette::DEEPSEEK_SKY).bold()),
            Span::styled(
                " 前台工作活动中",
                Style::default().fg(palette::TEXT_DIM),
            ),
        ]));
    }

    lines.push(Line::from(Span::styled(
        "（参见转录卡片了解详情）",
        Style::default().fg(palette::TEXT_MUTED).italic(),
    )));

    lines
}

/// 会话上下文面板 (#504) — 整合的会话状态概览。
///
/// 一目了然：工作集、令牌用量 / 上下文百分比、运行中
/// 成本、MCP 服务器数量、LSP 切换状态、周期计数和内存
/// 文件大小 + 修改时间。每个部分都是紧凑的单行，
/// 使面板读起来像仪表板而不是滚动列表。
fn render_context_panel(f: &mut Frame, area: Rect, app: &App) {
    if area.height < 3 {
        return;
    }

    let content_width = area.width.saturating_sub(4) as usize;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(usize::from(area.height).max(4));

    // ── 工作集 ──────────────────────────────────────────────
    let ws_name = app
        .workspace
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("(根目录)")
        .to_string();
    lines.push(Line::from(vec![
        Span::styled(
            truncate_line_to_width(&ws_name, content_width.max(1)),
            Style::default().fg(palette::DEEPSEEK_SKY).bold(),
        ),
        Span::styled(
            format!("  {}", app.workspace_context.as_deref().unwrap_or("")),
            Style::default().fg(palette::TEXT_DIM),
        ),
    ]));

    // ── 令牌用量 ──────────────────────────────────────────────
    let total_tokens = app.session.total_conversation_tokens;
    let window = crate::models::context_window_for_model(&app.model).unwrap_or(1_048_576);
    let pct = if window > 0 {
        ((total_tokens as f64 / window as f64) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let bar_width = content_width.min(20);
    let filled = ((pct / 100.0) * bar_width as f64) as usize;
    let bar = format!(
        "[{}{}] {:.0}%",
        "█".repeat(filled),
        "░".repeat(bar_width.saturating_sub(filled)),
        pct
    );
    lines.push(Line::from(Span::styled(
        format!(
            "context: {}/{} tokens  {}",
            total_tokens,
            window,
            truncate_line_to_width(&bar, content_width.saturating_sub(32).max(8))
        ),
        Style::default().fg(palette::TEXT_MUTED),
    )));

    // ── 会话成本 ─────────────────────────────────────────────
    let total_cost = app.displayed_session_cost_for_currency(app.cost_currency);
    let session_cost = app.session_cost_for_currency(app.cost_currency);
    let agent_cost = app.subagent_cost_for_currency(app.cost_currency);
    lines.push(Line::from(Span::styled(
        format!(
            "cost: {} (session {} + agents {})",
            app.format_cost_amount(total_cost),
            app.format_cost_amount(session_cost),
            app.format_cost_amount(agent_cost)
        ),
        Style::default().fg(palette::TEXT_MUTED),
    )));

    // ── MCP 服务器 ──────────────────────────────────────────────
    if app.mcp_configured_count > 0 {
        let restart_hint = if app.mcp_restart_required {
            " (需要重启)"
        } else {
            ""
        };
        lines.push(Line::from(Span::styled(
            format!(
                "mcp: {} server(s){}",
                app.mcp_configured_count, restart_hint
            ),
            Style::default().fg(palette::TEXT_MUTED),
        )));
    }

    // ── LSP ──────────────────────────────────────────────────────
    let lsp_label = if app.lsp_enabled { "开" } else { "关" };
    lines.push(Line::from(Span::styled(
        format!("lsp: {}", lsp_label),
        Style::default().fg(palette::TEXT_MUTED),
    )));

    // ── 周期 ───────────────────────────────────────────────────
    if app.cycle_count > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "cycles: {} crossed, {} briefing(s)",
                app.cycle_count,
                app.cycle_briefings.len()
            ),
            Style::default().fg(palette::TEXT_MUTED),
        )));
    }

    // ── 记忆 ───────────────────────────────────────────────────
    if app.use_memory {
        let size_hint = std::fs::metadata(&app.memory_path)
            .map(|m| m.len())
            .map(|bytes| {
                if bytes >= 1024 * 1024 {
                    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
                } else if bytes >= 1024 {
                    format!("{:.1} KB", bytes as f64 / 1024.0)
                } else {
                    format!("{} B", bytes)
                }
            })
            .unwrap_or_else(|_| "—".to_string());
        lines.push(Line::from(Span::styled(
            format!("memory: {} ({})", app.memory_path.display(), size_hint),
            Style::default().fg(palette::TEXT_MUTED),
        )));
    }

    render_sidebar_section(f, area, "会话", lines, app);
}

fn render_sidebar_section(
    f: &mut Frame,
    area: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    app: &App,
) {
    if area.width < 4 || area.height < 3 {
        // 在退出前清除过期单元格 (#400)。
        Block::default()
            .style(Style::default().bg(app.ui_theme.surface_bg))
            .render(area, f.buffer_mut());
        return;
    }

    let theme = Theme::for_palette_mode(app.ui_theme.mode);
    // 截断面板标题，使其始终适合部分宽度，
    // 即使在调整大小后也是如此。标题占用最多 4 个字符的边框装饰
    //（每侧两个空格 + 一个空格），因此启用边框时
    // 最大标题长度为 area.width.saturating_sub(4)。
    let max_title_width = area.width.saturating_sub(4).max(1) as usize;
    let display_title = truncate_line_to_width(title, max_title_width);

    // 将行限制在可见部分区域内，使得 Paragraph 换行
    // 溢出无法在 Block 边界外写入单元格 (#400)。边框 + 内边距占用 2 行；
    // 其余空间用于内容。
    let visible_content_rows = area
        .height
        .saturating_sub(2) // top + bottom border
        .saturating_sub(theme.section_padding.top + theme.section_padding.bottom)
        as usize;
    let lines: Vec<Line<'static>> =
        if lines.len() > visible_content_rows && visible_content_rows > 0 {
            lines.into_iter().take(visible_content_rows).collect()
        } else {
            lines
        };

    let section = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .title(Line::from(vec![Span::styled(
                format!(" {display_title} "),
                Style::default().fg(theme.section_title_color).bold(),
            )]))
            .borders(theme.section_borders)
            .border_type(theme.section_border_type)
            .border_style(Style::default().fg(theme.section_border_color))
            .style(Style::default().bg(theme.section_bg))
            .padding(theme.section_padding),
    );

    f.render_widget(section, area);
}

#[cfg(test)]
mod tests {
    use super::{SidebarSubagentSummary, plan_panel_empty_hint, subagent_navigator_lines};
    use ratatui::text::Line;

    fn lines_to_text(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    // ---- #408 计划面板空状态提示 ----

    #[test]
    fn plan_panel_empty_hint_mentions_panels_role() {
        // 该提示替换了旧的"无活动计划"占位符；它应解释面板跟踪的内容，
        // 以便用户判断面板是坏了还是本轮未被使用。
        let hint = plan_panel_empty_hint(80);
        assert!(
            hint.contains("update_plan"),
            "提示应提及工具名称：{hint:?}"
        );
        assert!(
            hint.contains("/goal") || hint.contains("goal"),
            "提示应提及 /goal：{hint:?}"
        );
    }

    #[test]
    fn plan_panel_empty_hint_truncates_to_narrow_widths() {
        // 宽度 16 强制使用省略号；提示仍应能容纳。
        let hint = plan_panel_empty_hint(16);
        assert!(
            hint.chars().count() <= 16,
            "提示宽度 {} > 16：{hint:?}",
            hint.chars().count()
        );
    }

    #[test]
    fn plan_panel_empty_hint_does_not_say_no_active_plan() {
        // 回归防护：占位符过去显示"无活动计划"，
        // 这让面板看起来像是坏了。提示不应重新引入该措辞。
        let hint = plan_panel_empty_hint(80);
        assert!(
            !hint.to_ascii_lowercase().contains("no active plan"),
            "提示回归到旧占位符：{hint:?}"
        );
    }

    #[test]
    fn navigator_empty_state_says_no_agents() {
        let summary = SidebarSubagentSummary::default();
        let lines = subagent_navigator_lines(&summary, 32);
        let text = lines_to_text(&lines);
        assert_eq!(text, vec!["无代理".to_string()]);
    }

    #[test]
    fn navigator_running_state_renders_count_role_and_navigator_hint() {
        // 两个 general 代理（一个运行中，一个已完成）+ 一个 explore（运行中）。
        let mut role_counts = std::collections::BTreeMap::new();
        role_counts.insert("general".to_string(), 2);
        role_counts.insert("explore".to_string(), 1);
        let summary = SidebarSubagentSummary {
            cached_total: 3,
            cached_running: 2,
            progress_only_count: 0,
            fanout_total: None,
            fanout_running: 0,
            foreground_rlm_running: false,
            role_counts,
        };
        let text = lines_to_text(&subagent_navigator_lines(&summary, 64));
        assert!(text[0].contains("2 running"), "header 内容：{:?}", text[0]);
        assert!(text[0].contains("/ 3"), "header 中的总数：{:?}", text[0]);
        assert!(
            text[1].contains("1 explore") && text[1].contains("2 general"),
            "角色组合行：{:?}",
            text[1]
        );
        assert!(
            text.iter().any(|l| l.contains("transcript card")),
            "导航提示必须引用转录本：{text:?}",
        );
    }

    #[test]
    fn navigator_uses_fanout_total_when_fanout_has_seeded_slots() {
        let summary = SidebarSubagentSummary {
            cached_total: 1,
            cached_running: 1,
            progress_only_count: 0,
            fanout_total: Some(6),
            fanout_running: 1,
            foreground_rlm_running: false,
            role_counts: std::collections::BTreeMap::new(),
        };

        let text = lines_to_text(&subagent_navigator_lines(&summary, 64));

        assert!(text[0].contains("1 running"), "header: {:?}", text[0]);
        assert!(text[0].contains("/ 6"), "fanout 总数：{:?}", text[0]);
    }

    #[test]
    fn navigator_settled_state_says_done() {
        let mut role_counts = std::collections::BTreeMap::new();
        role_counts.insert("general".to_string(), 1);
        let summary = SidebarSubagentSummary {
            cached_total: 1,
            cached_running: 0,
            progress_only_count: 0,
            fanout_total: None,
            fanout_running: 0,
            foreground_rlm_running: false,
            role_counts,
        };
        let text = lines_to_text(&subagent_navigator_lines(&summary, 32));
        assert!(text[0].contains("1 done"), "已完成 header：{:?}", text[0]);
    }

    #[test]
    fn navigator_truncates_long_role_mix_to_content_width() {
        // Build a wide role mix; assert it doesn't blow past content_width.
        let mut role_counts = std::collections::BTreeMap::new();
        for role in ["general", "explore", "plan", "review", "custom", "extra"] {
            role_counts.insert(role.to_string(), 1);
        }
        let summary = SidebarSubagentSummary {
            cached_total: 6,
            cached_running: 6,
            progress_only_count: 0,
            fanout_total: None,
            fanout_running: 0,
            foreground_rlm_running: false,
            role_counts,
        };
        let lines = subagent_navigator_lines(&summary, 16);
        let role_line: &str = lines[1]
            .spans
            .first()
            .map(|s| s.content.as_ref())
            .unwrap_or("");
        assert!(
            role_line.chars().count() <= 16,
            "角色行 {role_line:?} 超过了内容宽度"
        );
    }

    #[test]
    fn navigator_shows_foreground_rlm_work_when_no_subagents_exist() {
        let summary = SidebarSubagentSummary {
            foreground_rlm_running: true,
            ..SidebarSubagentSummary::default()
        };
        let text = lines_to_text(&subagent_navigator_lines(&summary, 64));

        assert!(!text[0].contains("无代理"), "header 内容：{:?}", text);
        assert!(
            text.iter()
                .any(|line| line.contains("RLM 前台工作活动中")),
            "RLM 工作必须在代理面板中可见：{text:?}"
        );
    }
}
