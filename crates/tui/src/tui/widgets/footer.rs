//! 页脚栏组件，显示模式、状态、模型和辅助标签。
//!
//! `FooterWidget` 是 [`FooterProps`] 结构体的纯渲染：所有内容
//!（标签、颜色、span 簇）在每次重绘时在更高层计算一次，
//! 然后 `FooterWidget::new(props).render(area, buf)` 绘制结果。
//! 该小部件不拥有任何 `App` 知识；这镜像了 `HeaderWidget`
//!（以及 Codex 的 `bottom_pane::footer::Footer`）使用的布局。

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::localization::{Locale, MessageId, tr};
use crate::palette;
use crate::tui::app::{App, AppMode};

use super::Renderable;

/// 页脚渲染所需的预计算数据。
///
/// 所有字段都是拥有的 `String` / `Vec<Span<'static>>` 值，
/// 因此 props 可以在每次重绘时构建一次，然后交给无借用的小部件。
#[derive(Debug, Clone)]
pub struct FooterProps {
    /// 模式芯片后显示的当前模型标识符。
    pub model: String,
    /// `"agent"` / `"yolo"` / `"plan"` — 规范设置标签。
    pub mode_label: &'static str,
    /// 模式芯片使用的颜色。
    pub mode_color: Color,
    /// 芯片之间小分隔符的颜色。
    pub text_dim_color: Color,
    /// 模型标签的颜色。
    pub text_hint_color: Color,
    /// 稳定辅助芯片（如成本）的颜色。
    pub text_muted_color: Color,
    /// 整个页脚/状态栏行的背景色。
    pub footer_bg: Color,
    /// 状态标签，如 `"ready"`、`"thinking ⌫"`、`"working"`。
    /// 当标签等于 `"ready"` 时，页脚完全隐藏状态段。
    pub state_label: String,
    /// 状态标签的颜色。
    pub state_color: Color,
    /// 一致性芯片跨度（无活动干预时为空）。
    pub coherence: Vec<Span<'static>>,
    /// 子代理计数芯片跨度（零个在运行时为空）。
    pub agents: Vec<Span<'static>>,
    /// 推理回放芯片跨度（为零/不适用时为空）。
    pub reasoning_replay: Vec<Span<'static>>,
    /// 缓存命中率芯片跨度（未报告使用情况时为空）。
    pub cache: Vec<Span<'static>>,
    /// MCP 服务器健康芯片跨度（未配置 MCP 服务器时为空）。
    /// 惰性填充 — 请参见 [`footer_mcp_chip`]。(#502)
    pub mcp: Vec<Span<'static>>,
    /// 累积模型工作芯片跨度（"worked 3h 12m"）。对已完成轮次
    /// 的经过时间求和（来自 `App::cumulative_turn_duration`），
    /// **不是**自启动以来的挂钟时间 — 空闲的 TUI 不应声称它一直在"工作"。
    /// 在累积轮次时间超过 60 秒之前为空。由 [`footer_worked_chip`] 填充。(#448)
    pub worked: Vec<Span<'static>>,
    /// 全局重试状态表面的快照 (#499)。在 props 构建时采样一次，
    /// 在活动时作为前景横幅渲染在页脚左侧。在此处捕获
    ///（而不是在渲染时从 `retry_status` 读取），以便测试可以固定
    /// 确定性状态，而无需与并行运行器竞争。
    pub retry: crate::retry_status::RetryState,
    /// 会话成本芯片跨度（低于显示阈值时为空）。
    /// 在左侧簇中渲染（在模型名称之后）— 成本是稳定信息，
    /// 而非瞬态信号，因此它与模式和模型一起存在。
    pub cost: Vec<Span<'static>>,
    /// 可选的 toast，当存在时替换左侧状态行。
    pub toast: Option<FooterToast>,
    /// 当为 `Some(frame_idx)` 时，左侧状态行和右侧芯片之间的间隙
    /// 填充了基于 `frame_idx` 的动画水柱条（给定帧确定）。
    /// `None` 保持间隙为纯空白，即空闲/就绪状态。
    pub working_strip_frame: Option<u64>,
}

const WAVE_GLYPHS: [char; 8] = [
    '\u{2581}', // ▁
    '\u{2582}', // ▂
    '\u{2583}', // ▃
    '\u{2584}', // ▄
    '\u{2585}', // ▅
    '\u{2586}', // ▆
    '\u{2587}', // ▇
    '\u{2588}', // █
];

/// 页脚实时工作波动画的一帧。`col` 是条内的单元格索引，
/// `width` 是条的总宽度，`frame` 是原始毫秒计数器。
/// 返回该帧中该单元格应出现的字形。
///
/// 视觉效果：由单单元格块高度字形组成的全宽相移波。
/// 早期的波峰对动画仅在四舍五入的波峰位置跨越终端单元格边界时更改；
/// 在 80 毫秒的重绘节奏下，它显示为可见的跳动。
/// 采样几个移动的正弦分量使每次重绘都有新外观，
/// 同时保持数学确定性以便测试。
#[must_use]
pub fn footer_working_strip_glyph_at(col: usize, width: usize, frame: u64) -> char {
    if width == 0 {
        return ' ';
    }

    let t = frame as f64 / 1000.0;
    let x = col as f64;

    let primary = (x * 0.52 - t * 8.0).sin();
    let swell = (x * 0.18 + t * 3.1).sin() * 0.35;
    let shimmer = (x * 1.35 - t * 11.0).sin() * 0.12;
    let value = ((primary + swell + shimmer) / 1.47).clamp(-1.0, 1.0);
    let normalized = (value + 1.0) * 0.5;
    let idx = (normalized * (WAVE_GLYPHS.len() - 1) as f64).round() as usize;
    WAVE_GLYPHS[idx.min(WAVE_GLYPHS.len() - 1)]
}

/// 构建每帧实时工作波动画字符串，宽度为 `width` 个字符。
/// 当宽度为 0 时返回空字符串。结果与请求的视觉宽度相同
///（所选块高度字形每列一个字符），并且可以安全地放入
/// 页脚左右段之间的 `Span` 中。
#[must_use]
pub fn footer_working_strip_string(width: usize, frame: u64) -> String {
    let mut out = String::with_capacity(width * 4);
    for col in 0..width {
        out.push(footer_working_strip_glyph_at(col, width, frame));
    }
    out
}

/// 将本地化的 "working" 标签脉冲式地通过 0-3 个尾部 ASCII 点，
/// 基于 `frame` 按键。周期为 4 帧（匹配四个状态），
/// 因此相邻的滴答声明显不同。点保持 ASCII 不管语言环境如何，
/// 以便动画在不同脚本中读取相同。返回 `String`，
/// 以便调用方可以将其放入 `Span::styled` 中，无需生命周期技巧。
#[must_use]
pub fn footer_working_label(frame: u64, locale: Locale) -> String {
    let dots = (frame % 4) as usize;
    let base = tr(locale, MessageId::FooterWorking);
    let mut out = String::with_capacity(base.len() + dots);
    out.push_str(base);
    for _ in 0..dots {
        out.push('.');
    }
    out
}

/// 当有子代理在运行时构建 "N agents" 芯片跨度列表。
/// N == 0 时为空列表，完全隐藏芯片。N == 1 时使用单数形式，
/// 自然阅读；其他情况使用复数。复数模板存在于语言环境注册表中，
/// 以便 CJK 语言环境可以在没有英语复数 `s` 的情况下渲染计数。
#[must_use]
pub fn footer_agents_chip(running: usize, locale: Locale) -> Vec<Span<'static>> {
    if running == 0 {
        return Vec::new();
    }
    let text = if running == 1 {
        tr(locale, MessageId::FooterAgentSingular).to_string()
    } else {
        tr(locale, MessageId::FooterAgentsPlural).replace("{count}", &running.to_string())
    };
    vec![Span::styled(
        text,
        Style::default().fg(palette::DEEPSEEK_SKY),
    )]
}

/// 为页脚右侧簇构建累积经过时间芯片 ("worked 3h 12m") (#448)。
/// 在会话的前一分钟内隐藏，以便新启动不会渲染一个立即开始
/// 计时的嘈杂 `worked 5s` 指示器。超过阈值后，
/// 重用 [`crate::tui::notifications::humanize_duration`] 实现
/// 一致的 w/d/h/m 格式。
#[must_use]
pub fn footer_worked_chip(elapsed: std::time::Duration) -> Vec<Span<'static>> {
    if elapsed < std::time::Duration::from_secs(60) {
        return Vec::new();
    }
    let label = format!(
        "worked {}",
        crate::tui::notifications::humanize_duration(elapsed)
    );
    vec![Span::styled(
        label,
        Style::default().fg(palette::TEXT_MUTED),
    )]
}

/// 从用户存储的快照构建 "MCP M/N" 健康芯片 (#502)。
/// `connected` 是当前可达的服务器数；`configured` 是用户 MCP 配置中声明的数量。
/// 当 `configured` 为零时，芯片完全隐藏。
///
/// 按健康状况对计数进行颜色编码：
/// - 全部可达 → 成功
/// - 部分可达 → 警告
/// - 无可达但至少配置了一个 → 错误
/// - 已配置但尚无实时快照 → 静音（仅计数）
#[must_use]
pub fn footer_mcp_chip(connected: Option<usize>, configured: usize) -> Vec<Span<'static>> {
    if configured == 0 {
        return Vec::new();
    }
    let (label, color) = match connected {
        None => (format!("MCP {configured}"), palette::TEXT_MUTED),
        Some(c) if c == configured => (format!("MCP {c}/{configured}"), palette::STATUS_SUCCESS),
        Some(0) => (format!("MCP 0/{configured}"), palette::STATUS_ERROR),
        Some(c) => (format!("MCP {c}/{configured}"), palette::STATUS_WARNING),
    };
    vec![Span::styled(label, Style::default().fg(color))]
}

/// 短时间路由到页脚左段的状态 toast。
#[derive(Debug, Clone)]
pub struct FooterToast {
    pub text: String,
    pub color: Color,
}

impl FooterProps {
    /// 从通用应用状态构建页脚 props。`tui/ui.rs` 中的辅助函数
    ///（例如 `footer_state_label`、`footer_coherence_spans`）提供
    /// 预样式化的 span 和标签 — 此构造函数只是捆绑它们。
    ///
    /// 参数展开是有意的：每个输入一对一映射到调用方从 `App` 解析的
    /// 一段预计算页脚内容。强制将这些放入构建器会模糊调用点，
    /// 而不会使数据流更清晰。
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn from_app(
        app: &App,
        toast: Option<FooterToast>,
        state_label: &'static str,
        state_color: Color,
        coherence: Vec<Span<'static>>,
        agents: Vec<Span<'static>>,
        reasoning_replay: Vec<Span<'static>>,
        cache: Vec<Span<'static>>,
        cost: Vec<Span<'static>>,
    ) -> Self {
        let (mode_label, mode_color) = mode_style(app);
        // MCP 芯片 (#502) — 被动，派生自用户现有的快照。
        // `connected` 为 `None`，直到用户运行 `/mcp`，
        // 这与问题规范目前接受的触发器相同。
        let mcp_configured = app.mcp_configured_count;
        let mcp_connected = app
            .mcp_snapshot
            .as_ref()
            .map(|s| s.servers.iter().filter(|server| server.connected).count());
        let mcp = footer_mcp_chip(mcp_connected, mcp_configured);
        // #448: 累积工作时间芯片。对实际轮次持续时间求和
        //（在 `TurnComplete` 上设置），而不是挂钟运行时间 —
        // 已经打开并空闲了 4 分钟的 TUI 不应声称 "worked 4m"。
        // 芯片保持为空，直到足够的轮次累积到超过 `footer_worked_chip` 中的 60 秒阈值。
        let worked = footer_worked_chip(app.cumulative_turn_duration);
        Self {
            model: app.model_display_label(),
            mode_label,
            mode_color,
            text_dim_color: app.ui_theme.text_dim,
            text_hint_color: app.ui_theme.text_hint,
            text_muted_color: app.ui_theme.text_muted,
            footer_bg: app.ui_theme.footer_bg,
            state_label: state_label.to_string(),
            state_color,
            coherence,
            agents,
            reasoning_replay,
            cache,
            mcp,
            worked,
            cost,
            toast,
            working_strip_frame: None,
            retry: crate::retry_status::snapshot(),
        }
    }
}

fn mode_style(app: &App) -> (&'static str, Color) {
    let label = match app.mode {
        AppMode::Agent => "agent",
        AppMode::Yolo => "yolo",
        AppMode::Plan => "plan",
    };
    let color = match app.mode {
        AppMode::Agent => app.ui_theme.mode_agent,
        AppMode::Yolo => app.ui_theme.mode_yolo,
        AppMode::Plan => app.ui_theme.mode_plan,
    };
    (label, color)
}

/// 纯渲染页脚。每帧构建一次，然后 `render(area, buf)`。
pub struct FooterWidget {
    props: FooterProps,
}

impl FooterWidget {
    #[must_use]
    pub fn new(props: FooterProps) -> Self {
        Self { props }
    }

    fn auxiliary_spans(&self, max_width: usize) -> Vec<Span<'static>> {
        // `cost` 现在在左侧簇中渲染 — 将其排除在右侧芯片展示之外。
        // 一致性/代理/回放/缓存是瞬态信号；它们属于右侧，
        // 在那里出现和消失，不会干扰稳定的模式·模型·成本行。
        let parts: Vec<&Vec<Span<'static>>> = [
            &self.props.coherence,
            &self.props.agents,
            &self.props.reasoning_replay,
            &self.props.cache,
            &self.props.mcp,
            // `worked` 是优先级最低的芯片 — 在窄宽度下最先丢弃
            //（下面的优先级循环从尾部移除）。`cost` 是稳定信息，
            // 留在左侧簇中，眼睛无需扫描即可找到它。
            &self.props.worked,
        ]
        .into_iter()
        .filter(|spans| !spans.is_empty())
        .collect();

        // Try to fit as many parts as possible, dropping from the end.
        for end in (0..=parts.len()).rev() {
            let mut combined: Vec<Span<'static>> = Vec::new();
            for (i, part) in parts[..end].iter().enumerate() {
                if i > 0 {
                    combined.push(Span::raw("  "));
                }
                combined.extend(part.iter().cloned());
            }
            if span_width(&combined) <= max_width {
                return combined;
            }
        }
        Vec::new()
    }

    fn toast_spans(toast: &FooterToast, max_width: usize) -> Vec<Span<'static>> {
        let truncated = truncate_to_width(&toast.text, max_width.max(1));
        vec![Span::styled(truncated, Style::default().fg(toast.color))]
    }

    /// 使用优先级排序构建左侧状态行。
    ///
    /// 优先级顺序（最高到最低 — 最后丢弃）：
    /// 1. 模式标签（在任何宽度下始终可见；仅作为最后手段截断）
    /// 2. 模型名称（始终可见；当状态和成本消失后在词中截断）
    /// 3. 成本芯片 — 在状态之后第二个丢弃（稳定信息仍希望可见）
    /// 4. 状态标签（例如 "working"、"draft"）— 空间紧张时最先丢弃
    ///
    /// 在每个宽度 ≥ 40 列时，行永远不会在提示中间换行：
    /// 小部件选择 (`mode · model · cost · status`、`mode · model · cost`、
    /// `mode · model`、`mode`) 之一，并在 `max_width` 内渲染该单行。
    /// 成本位于模型和状态之间，以便眼睛无需扫描过波动动画就能找到
    /// "这次运行要花我多少钱"。
    fn status_line_spans(&self, max_width: usize) -> Vec<Span<'static>> {
        if max_width == 0 {
            return Vec::new();
        }

        let mode_label = self.props.mode_label;
        let sep = " \u{00B7} ";
        let model = self.props.model.as_str();
        let show_status = self.props.state_label != "ready";
        let status_label = self.props.state_label.as_str();
        let cost_text = spans_text(&self.props.cost);
        let show_cost = !cost_text.is_empty();

        let mode_w = mode_label.width();
        let sep_w = sep.width();
        let model_w = UnicodeWidthStr::width(model);
        let status_w = status_label.width();
        let cost_w = cost_text.width();

        // Tier 1: mode · model · cost · status — everything fits.
        let full_w = mode_w
            + sep_w
            + model_w
            + if show_cost { sep_w + cost_w } else { 0 }
            + if show_status { sep_w + status_w } else { 0 };
        if (show_cost || show_status) && full_w <= max_width {
            return self.build_status_line_spans(
                mode_label,
                model.to_string(),
                show_cost.then(|| cost_text.clone()),
                show_status.then_some(status_label),
            );
        }

        // Tier 2: mode · model · cost — drop status first.
        if show_cost {
            let with_cost_w = mode_w + sep_w + model_w + sep_w + cost_w;
            if with_cost_w <= max_width {
                return self.build_status_line_spans(
                    mode_label,
                    model.to_string(),
                    Some(cost_text.clone()),
                    None,
                );
            }
        }

        // Tier 3: mode · model — drop cost too.
        let mode_model_w = mode_w + sep_w + model_w;
        if mode_model_w <= max_width {
            return self.build_status_line_spans(mode_label, model.to_string(), None, None);
        }

        // Tier 4: mode · <truncated model> — keep both labels visible by
        // ellipsizing the model name. Only do this when there is enough room
        // for at least the ellipsis ("..."). Below that we drop to mode-only.
        let prefix_w = mode_w + sep_w;
        if prefix_w < max_width {
            let model_budget = max_width - prefix_w;
            if model_budget >= 4 {
                let truncated = truncate_to_width(model, model_budget);
                if !truncated.is_empty() {
                    return self.build_status_line_spans(mode_label, truncated, None, None);
                }
            }
        }

        // Tier 5: mode-only. If even the mode label cannot fit, truncate it
        // so the footer never wraps to a second row.
        if mode_w <= max_width {
            return vec![Span::styled(
                mode_label.to_string(),
                Style::default().fg(self.props.mode_color),
            )];
        }
        vec![Span::styled(
            truncate_to_width(mode_label, max_width),
            Style::default().fg(self.props.mode_color),
        )]
    }

    fn build_status_line_spans(
        &self,
        mode_label: &'static str,
        model_label: String,
        cost: Option<String>,
        status: Option<&str>,
    ) -> Vec<Span<'static>> {
        let sep = " \u{00B7} ";
        let mut spans: Vec<Span<'static>> = Vec::new();
        // Skip the mode chip when the user has toggled it off via
        // `/statusline`. The widget no longer assumes mode is always
        // present so an opt-out user doesn't see a stray separator.
        if !mode_label.is_empty() {
            spans.push(Span::styled(
                mode_label.to_string(),
                Style::default().fg(self.props.mode_color),
            ));
        }
        // Same treatment for the model label — gating both keeps the bar
        // visually tidy when only auxiliary chips remain.
        if !model_label.is_empty() {
            if !spans.is_empty() {
                spans.push(Span::styled(
                    sep.to_string(),
                    Style::default().fg(self.props.text_dim_color),
                ));
            }
            spans.push(Span::styled(
                model_label,
                Style::default().fg(self.props.text_hint_color),
            ));
        }
        if let Some(cost_text) = cost {
            if !spans.is_empty() {
                spans.push(Span::styled(
                    sep.to_string(),
                    Style::default().fg(self.props.text_dim_color),
                ));
            }
            spans.push(Span::styled(
                cost_text,
                Style::default().fg(self.props.text_muted_color),
            ));
        }
        if let Some(status_label) = status {
            if !spans.is_empty() {
                spans.push(Span::styled(
                    sep.to_string(),
                    Style::default().fg(self.props.text_dim_color),
                ));
            }
            spans.push(Span::styled(
                status_label.to_string(),
                Style::default().fg(self.props.state_color),
            ));
        }
        spans
    }
}

fn spans_text(spans: &[Span<'_>]) -> String {
    spans.iter().map(|s| s.content.as_ref()).collect::<String>()
}

/// 当 props 捕获的快照报告活动重试或最终失败时，
/// 渲染重试横幅 (#499)。空闲时返回 `None`，
/// 以便调用方回退到常规状态行/toast。
fn retry_banner_spans(max_width: usize, props: &FooterProps) -> Option<Vec<Span<'static>>> {
    let (label, color) = match &props.retry {
        crate::retry_status::RetryState::Active(banner) => {
            let secs = props.retry.seconds_remaining().unwrap_or(0);
            // 四舍五入到 1 秒 — 我们无论如何每帧重绘，
            // 所以倒计时在视觉上滴答作响，无需我们安排任何额外内容。
            (
                format!("⟳ retry {} in {secs}s — {}", banner.attempt, banner.reason),
                crate::palette::STATUS_WARNING,
            )
        }
        crate::retry_status::RetryState::Failed { reason, .. } => {
            (format!("× failed: {reason}"), crate::palette::STATUS_ERROR)
        }
        crate::retry_status::RetryState::Idle => return None,
    };
    let truncated = truncate_to_width(&label, max_width);
    Some(vec![Span::styled(truncated, Style::default().fg(color))])
}

impl Renderable for FooterWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let available_width = area.width as usize;
        if available_width == 0 {
            return;
        }

        let right_spans = self.auxiliary_spans(available_width);
        let right_width = span_width(&right_spans);
        let min_gap = if right_width > 0 { 2 } else { 0 };
        let max_left_width = available_width
            .saturating_sub(right_width)
            .saturating_sub(min_gap)
            .max(1);

        let left_spans = if let Some(banner) = retry_banner_spans(max_left_width, &self.props) {
            // 重试横幅优先于 toast 和常规状态行，
            // 以便用户清晰看到它 (#499)。
            // 横幅在成功或下一个 `TurnStarted` 时自动清除（引擎发出清除）。
            banner
        } else if let Some(toast) = self.props.toast.as_ref() {
            Self::toast_spans(toast, max_left_width)
        } else {
            self.status_line_spans(max_left_width)
        };

        let left_width = span_width(&left_spans);
        let spacer_width = available_width.saturating_sub(left_width + right_width);

        // 当轮次正在进行时，用细小的动画水柱条填充间隙；
        // 否则间隙保持为纯空白。
        let spacer_span = match self.props.working_strip_frame {
            Some(frame) if spacer_width > 0 => Span::styled(
                footer_working_strip_string(spacer_width, frame),
                Style::default().fg(palette::DEEPSEEK_SKY),
            ),
            _ => Span::raw(" ".repeat(spacer_width)),
        };

        let mut all_spans = left_spans;
        all_spans.push(spacer_span);
        all_spans.extend(right_spans);

        let paragraph =
            Paragraph::new(Line::from(all_spans)).style(Style::default().bg(self.props.footer_bg));
        paragraph.render(area, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        1
    }
}

fn span_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.width()).sum()
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width <= 3 {
        return text.chars().take(max_width).collect();
    }

    let mut out = String::new();
    let mut width = 0usize;
    let limit = max_width.saturating_sub(3);
    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{FooterProps, FooterWidget, Renderable};
    use crate::config::Config;
    use crate::localization::Locale;
    use crate::palette;
    use crate::tui::app::{App, AppMode, TuiOptions};
    use ratatui::{
        style::{Color, Style},
        text::Span,
    };
    use std::path::PathBuf;

    fn make_app() -> App {
        let options = TuiOptions {
            model: "deepseek-v4-flash".to_string(),
            workspace: PathBuf::from("."),
            config_path: None,
            config_profile: None,
            allow_shell: false,
            use_alt_screen: true,
            use_mouse_capture: false,
            use_bracketed_paste: true,
            max_subagents: 1,
            skills_dir: PathBuf::from("."),
            memory_path: PathBuf::from("memory.md"),
            notes_path: PathBuf::from("notes.txt"),
            mcp_config_path: PathBuf::from("mcp.json"),
            use_memory: false,
            start_in_agent_mode: true,
            skip_onboarding: true,
            yolo: false,
            resume_session_id: None,
            initial_input: None,
        };
        let mut app = App::new(options, &Config::default());
        // App::new may pick up `default_model` from a local user Settings
        // file, which overrides the option above. Pin the model explicitly
        // so these tests are independent of any host-side configuration.
        app.model = "deepseek-v4-flash".to_string();
        app
    }

    fn idle_props_for(app: &App) -> FooterProps {
        let mut props = FooterProps::from_app(
            app,
            None,
            "ready",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );
        // `from_app` reads the process-wide retry-status surface; pin
        // `Idle` so footer tests don't pick up state set by retry-banner
        // tests running in parallel.
        props.retry = crate::retry_status::RetryState::Idle;
        props
    }

    #[test]
    fn from_app_idle_state_carries_ready_label_and_no_chips() {
        let app = make_app();
        let props = idle_props_for(&app);

        assert_eq!(props.state_label, "ready");
        assert_eq!(props.state_color, palette::TEXT_MUTED);
        assert_eq!(props.mode_label, "agent");
        assert_eq!(props.mode_color, palette::MODE_AGENT);
        assert_eq!(props.text_dim_color, palette::TEXT_DIM);
        assert_eq!(props.text_hint_color, palette::TEXT_HINT);
        assert_eq!(props.text_muted_color, palette::TEXT_MUTED);
        assert_eq!(props.model, "deepseek-v4-flash");
        assert!(props.coherence.is_empty());
        assert!(props.agents.is_empty());
        assert!(props.cache.is_empty());
        assert!(props.cost.is_empty());
        assert!(props.reasoning_replay.is_empty());
        // #448: fresh apps don't get a `worked` chip until completed
        // turns have added up to >= 60s of model work. A freshly-built
        // App has cumulative_turn_duration == 0 so the chip is empty.
        assert!(props.worked.is_empty());
        assert!(props.toast.is_none());
    }

    #[test]
    fn worked_chip_tracks_completed_turn_time_not_session_uptime() {
        // Regression test for the v0.8.8 takedown: the chip used to
        // read `App::session_started_at.elapsed()`, so a TUI that had
        // been open and idle for several minutes claimed "worked 3m"
        // even though no turn had ever fired. The chip now sources
        // from `App::cumulative_turn_duration`, which is only ever
        // incremented on `TurnComplete`. Pin both directions:
        //
        //   1. cumulative == 0 (no turn finished yet)  → empty
        //   2. cumulative crosses 60s (real work)      → label shows
        //   3. wall-clock since launch is irrelevant   → not consulted
        let mut app = make_app();
        // The whole point: cumulative_turn_duration starts at zero,
        // so however long the TUI has been open the chip stays empty
        // until a turn actually completes and adds time.
        let props = idle_props_for(&app);
        assert!(
            props.worked.is_empty(),
            "idle app with zero cumulative turn time must not show worked chip"
        );

        // A real turn finishes for 90s of model work — chip lights up.
        // (`humanize_duration` keeps both units when both are non-zero,
        // so 90s renders as `1m 30s`, not `1m`.)
        app.cumulative_turn_duration = std::time::Duration::from_secs(90);
        let props = idle_props_for(&app);
        let text: String = props
            .worked
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        assert_eq!(text, "worked 1m 30s");
    }

    #[test]
    fn footer_worked_chip_hidden_below_one_minute() {
        use std::time::Duration;
        for secs in [0, 1, 30, 59] {
            let chip = super::footer_worked_chip(Duration::from_secs(secs));
            assert!(
                chip.is_empty(),
                "worked chip must be hidden at {secs}s; got {chip:?}"
            );
        }
    }

    #[test]
    fn footer_worked_chip_shows_humanized_label_above_threshold() {
        use std::time::Duration;
        // 1 minute on the dot — boundary, must render.
        let chip = super::footer_worked_chip(Duration::from_secs(60));
        let text: String = chip.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "worked 1m");

        // 3h 12m — the issue's golden example.
        let chip = super::footer_worked_chip(Duration::from_secs(11_550));
        let text: String = chip.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "worked 3h 12m");

        // Multi-day session — exercises the d/h band.
        let chip = super::footer_worked_chip(Duration::from_secs(2 * 86_400 + 5 * 3600));
        let text: String = chip.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "worked 2d 5h");
    }

    #[test]
    fn from_app_loading_state_uses_thinking_label_and_warning_color() {
        let app = make_app();
        let props = FooterProps::from_app(
            &app,
            None,
            "thinking \u{238B}",
            palette::STATUS_WARNING,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );

        assert!(props.state_label.starts_with("thinking"));
        assert_eq!(props.state_color, palette::STATUS_WARNING);
    }

    #[test]
    fn from_app_statusline_colors_come_from_ui_theme() {
        let mut app = make_app();
        app.ui_theme.mode_agent = Color::Rgb(1, 2, 3);
        app.ui_theme.text_dim = Color::Rgb(4, 5, 6);
        app.ui_theme.text_hint = Color::Rgb(7, 8, 9);
        app.ui_theme.text_muted = Color::Rgb(10, 11, 12);
        app.ui_theme.footer_bg = Color::Rgb(13, 14, 15);

        let props = idle_props_for(&app);

        assert_eq!(props.mode_color, Color::Rgb(1, 2, 3));
        assert_eq!(props.text_dim_color, Color::Rgb(4, 5, 6));
        assert_eq!(props.text_hint_color, Color::Rgb(7, 8, 9));
        assert_eq!(props.text_muted_color, Color::Rgb(10, 11, 12));
        assert_eq!(props.footer_bg, Color::Rgb(13, 14, 15));
    }

    #[test]
    fn render_applies_footer_background_to_full_row() {
        let mut app = make_app();
        app.ui_theme.footer_bg = Color::Rgb(13, 14, 15);
        let props = idle_props_for(&app);
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 60, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);

        widget.render(area, &mut buf);

        for x in 0..area.width {
            assert_eq!(buf[(x, 0)].bg, Color::Rgb(13, 14, 15));
        }
    }

    // ---- agents chip wording ----
    #[test]
    fn footer_agents_chip_is_empty_when_no_agents_running() {
        let chip = super::footer_agents_chip(0, Locale::En);
        assert!(chip.is_empty(), "0 agents in flight → no chip");
    }

    #[test]
    fn footer_agents_chip_uses_singular_for_one() {
        let chip = super::footer_agents_chip(1, Locale::En);
        assert_eq!(chip.len(), 1);
        assert_eq!(chip[0].content.as_ref(), "1 agent");
    }

    #[test]
    fn footer_agents_chip_uses_plural_for_many() {
        let chip = super::footer_agents_chip(3, Locale::En);
        assert_eq!(chip.len(), 1);
        assert_eq!(chip[0].content.as_ref(), "3 agents");
    }

    #[test]
    fn footer_agents_chip_renders_into_widget() {
        let app = make_app();
        let agents = super::footer_agents_chip(2, Locale::En);
        let props = FooterProps::from_app(
            &app,
            None,
            "ready",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            agents,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 60, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            rendered.contains("2 agents"),
            "expected agents chip in render: {rendered:?}",
        );
    }

    #[test]
    fn from_app_mode_color_matches_mode_for_each_variant() {
        let mut app = make_app();
        let cases = [
            (AppMode::Agent, "agent", palette::MODE_AGENT),
            (AppMode::Yolo, "yolo", palette::MODE_YOLO),
            (AppMode::Plan, "plan", palette::MODE_PLAN),
        ];
        for (mode, expected_label, expected_color) in cases {
            app.mode = mode;
            let props = idle_props_for(&app);
            assert_eq!(
                props.mode_label, expected_label,
                "label mismatch for {mode:?}",
            );
            assert_eq!(
                props.mode_color, expected_color,
                "color mismatch for {mode:?}",
            );
        }
    }

    #[test]
    fn footer_mcp_chip_hidden_when_no_servers() {
        assert!(super::footer_mcp_chip(None, 0).is_empty());
        assert!(super::footer_mcp_chip(Some(0), 0).is_empty());
    }

    #[test]
    fn footer_mcp_chip_shows_count_only_until_snapshot_arrives() {
        let spans = super::footer_mcp_chip(None, 3);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "MCP 3");
    }

    #[test]
    fn footer_mcp_chip_uses_success_color_when_all_connected() {
        let spans = super::footer_mcp_chip(Some(3), 3);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "MCP 3/3");
        assert_eq!(spans[0].style.fg, Some(palette::STATUS_SUCCESS));
    }

    #[test]
    fn footer_mcp_chip_uses_warning_color_when_partial() {
        let spans = super::footer_mcp_chip(Some(2), 3);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "MCP 2/3");
        assert_eq!(spans[0].style.fg, Some(palette::STATUS_WARNING));
    }

    #[test]
    fn footer_mcp_chip_uses_error_color_when_zero_connected() {
        let spans = super::footer_mcp_chip(Some(0), 3);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "MCP 0/3");
        assert_eq!(spans[0].style.fg, Some(palette::STATUS_ERROR));
    }

    #[test]
    fn render_shows_retry_banner_when_active() {
        // Since `FooterProps::retry` is now a captured snapshot rather
        // than a global read at render time, we can pin the state on
        // the props directly without touching the global surface.
        let app = make_app();
        let mut props = idle_props_for(&app);
        props.retry = crate::retry_status::RetryState::Active(crate::retry_status::RetryBanner {
            attempt: 2,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(7),
            reason: "rate limited".to_string(),
        });
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 80, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            rendered.contains("retry 2"),
            "expected retry banner in render: {rendered:?}",
        );
        assert!(
            rendered.contains("rate limited"),
            "expected reason in render: {rendered:?}",
        );
    }

    #[test]
    fn render_shows_failure_row_when_failed() {
        let app = make_app();
        let mut props = idle_props_for(&app);
        props.retry = crate::retry_status::RetryState::Failed {
            reason: "upstream 500".to_string(),
            since: std::time::Instant::now(),
        };
        let widget = FooterWidget::new(props);
        let area = ratatui::layout::Rect::new(0, 0, 80, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);
        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(
            rendered.contains("failed"),
            "expected failure row: {rendered:?}",
        );
        assert!(
            rendered.contains("upstream 500"),
            "expected reason: {rendered:?}",
        );
    }

    #[test]
    fn render_emits_mode_and_model_when_idle() {
        let app = make_app();
        let props = idle_props_for(&app);
        let widget = FooterWidget::new(props);

        let area = ratatui::layout::Rect::new(0, 0, 60, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);

        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(rendered.contains("agent"));
        assert!(rendered.contains("deepseek-v4-flash"));
        assert!(!rendered.contains("ready"));
    }

    #[test]
    fn working_strip_string_width_matches_request() {
        // The strip must produce exactly `width` characters per frame —
        // otherwise the spacer math in `FooterWidget::render` would
        // mis-align the right-hand chips. Each wave glyph is one cell wide.
        for width in [0usize, 1, 8, 60, 200] {
            let s = super::footer_working_strip_string(width, 7);
            assert_eq!(s.chars().count(), width, "width {width} mismatch");
        }
    }

    #[test]
    fn working_strip_glyph_is_deterministic_per_frame() {
        // Same (col, width, frame) -> same glyph. Frames are raw
        // milliseconds so the strip can move at repaint cadence.
        let a = super::footer_working_strip_string(40, 150);
        let b = super::footer_working_strip_string(40, 150);
        assert_eq!(a, b, "deterministic given the same frame");
        let c = super::footer_working_strip_string(40, 230);
        assert_ne!(a, c, "advancing one repaint window must change the strip",);
    }

    #[test]
    fn working_strip_renders_glyphs_only_when_frame_is_some() {
        // Idle: spacer is plain whitespace. Active: spacer contains the
        // wave animation glyphs and visibly differs from the idle render.
        let app = make_app();
        let mut props = idle_props_for(&app);

        let area = ratatui::layout::Rect::new(0, 0, 80, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        FooterWidget::new(props.clone()).render(area, &mut buf);
        let idle: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();

        props.working_strip_frame = Some(600);
        let mut buf2 = ratatui::buffer::Buffer::empty(area);
        FooterWidget::new(props).render(area, &mut buf2);
        let active: String = (0..area.width).map(|x| buf2[(x, 0)].symbol()).collect();

        assert_ne!(
            idle, active,
            "active footer must visibly differ from idle one"
        );
        assert!(
            active
                .chars()
                .any(|glyph| super::WAVE_GLYPHS.contains(&glyph)),
            "active strip must contain at least one animation glyph: {active:?}",
        );
    }

    #[test]
    fn working_strip_changes_at_repaint_cadence() {
        let width = 60;
        let f0 = super::footer_working_strip_string(width, 0);
        let f80 = super::footer_working_strip_string(width, 80);
        let changed = f0
            .chars()
            .zip(f80.chars())
            .filter(|(before, after)| before != after)
            .count();
        assert!(
            changed > width / 4,
            "expected the wave to drift across one 80ms repaint; changed {changed}/{width}"
        );
    }

    #[test]
    fn working_strip_renders_multiple_wave_heights() {
        let s = super::footer_working_strip_string(60, 0);
        let mut distinct = Vec::new();
        for glyph in s.chars() {
            if super::WAVE_GLYPHS.contains(&glyph) && !distinct.contains(&glyph) {
                distinct.push(glyph);
            }
        }
        assert!(
            distinct.len() >= 5,
            "expected several wave heights, saw {distinct:?}",
        );
    }

    #[test]
    fn working_label_pulses_dots_through_full_cycle() {
        // The label sequence `working` → `working.` → `working..` →
        // `working...` then wraps back. Each frame is a discrete tick;
        // the cycle is exactly 4 frames so adjacent ticks visibly differ.
        assert_eq!(super::footer_working_label(0, Locale::En), "working");
        assert_eq!(super::footer_working_label(1, Locale::En), "working.");
        assert_eq!(super::footer_working_label(2, Locale::En), "working..");
        assert_eq!(super::footer_working_label(3, Locale::En), "working...");
        assert_eq!(
            super::footer_working_label(4, Locale::En),
            "working",
            "wraps back at frame 4",
        );
        assert_eq!(super::footer_working_label(7, Locale::En), "working...");
    }

    /// Render the footer at `width` and return the visible single-line text.
    fn render_at_width(props: FooterProps, width: u16) -> String {
        let area = ratatui::layout::Rect::new(0, 0, width, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        FooterWidget::new(props).render(area, &mut buf);
        (0..area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn props_with_status(state: &str) -> FooterProps {
        let app = make_app();
        FooterProps::from_app(
            &app,
            None,
            // Production state labels are `&'static str`; for tests we leak a
            // copy to match that lifetime.
            Box::leak(state.to_string().into_boxed_str()),
            palette::DEEPSEEK_SKY,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        )
    }

    /// Issue #88 — at the widest tier the footer shows mode · model · status
    /// without any truncation.
    #[test]
    fn footer_priority_drop_full_at_120_cols() {
        let props = props_with_status("working");
        let line = render_at_width(props, 120);
        assert!(line.contains("agent"), "mode visible: {line:?}");
        assert!(
            line.contains("deepseek-v4-flash"),
            "model visible: {line:?}"
        );
        assert!(line.contains("working"), "status visible: {line:?}");
        assert!(!line.contains("..."), "no truncation expected: {line:?}");
    }

    #[test]
    fn footer_priority_drop_full_at_100_cols() {
        let props = props_with_status("working");
        let line = render_at_width(props, 100);
        assert!(line.contains("agent"));
        assert!(line.contains("deepseek-v4-flash"));
        assert!(line.contains("working"));
    }

    /// At 80 cols the short status label "working" still fits alongside mode +
    /// model. The line never wraps mid-hint.
    #[test]
    fn footer_priority_drop_full_at_80_cols() {
        let props = props_with_status("working");
        let line = render_at_width(props, 80);
        assert!(line.contains("agent"));
        assert!(line.contains("deepseek-v4-flash"));
        assert!(!line.contains("..."), "no mid-word truncation: {line:?}");
        assert!(line.len() <= 80, "fits in 80 cols: {line:?}");
    }

    /// Status drops before the model is truncated. With a longer status label
    /// at 40 cols the status segment is dropped to keep mode + model intact.
    #[test]
    fn footer_priority_drop_status_first_at_40_cols() {
        let props = props_with_status("refreshing context");
        // "agent · deepseek-v4-flash · refreshing context" = 46 cols. At 40
        // the status label drops, keeping mode + model verbatim.
        let line = render_at_width(props, 40);
        assert!(line.contains("agent"), "mode kept: {line:?}");
        assert!(
            line.contains("deepseek-v4-flash"),
            "model kept verbatim: {line:?}"
        );
        assert!(
            !line.contains("refreshing"),
            "status dropped before model truncated: {line:?}",
        );
        assert!(line.len() <= 40, "fits in 40 cols: {line:?}");
    }

    /// At 60 cols mode + model + a long status all just fit (49 cols), so the
    /// whole line is preserved.
    #[test]
    fn footer_priority_drop_full_at_60_cols() {
        let props = props_with_status("working");
        let line = render_at_width(props, 60);
        assert!(line.contains("agent"));
        assert!(line.contains("deepseek-v4-flash"));
        assert!(line.contains("working"));
    }

    /// Below 30 cols the model truncates with an ellipsis only after the
    /// status label has already been dropped. Mode label always survives.
    #[test]
    fn footer_priority_drop_truncates_model_only_when_status_already_gone() {
        let props = props_with_status("working");
        let line = render_at_width(props, 20);
        assert!(line.starts_with("agent"), "mode stays at front: {line:?}");
        assert!(
            line.contains("..."),
            "model truncated as last resort: {line:?}"
        );
        assert!(!line.contains("working"), "status dropped: {line:?}");
    }

    fn props_with_status_and_cost(state: &str, cost: &str) -> FooterProps {
        let app = make_app();
        FooterProps::from_app(
            &app,
            None,
            Box::leak(state.to_string().into_boxed_str()),
            palette::DEEPSEEK_SKY,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            vec![Span::styled(cost.to_string(), Style::default())],
        )
    }

    /// v0.6.6 redesign — cost lives on the LEFT, between model and status.
    /// At wide widths the line reads `mode · model · cost · status`.
    #[test]
    fn footer_cost_renders_in_left_cluster_at_wide_widths() {
        let props = props_with_status_and_cost("working", "$0.42");
        let line = render_at_width(props, 120);
        let mode_pos = line.find("agent").expect("mode visible");
        let model_pos = line.find("deepseek-v4-flash").expect("model visible");
        let cost_pos = line.find("$0.42").expect("cost visible on left");
        let status_pos = line.find("working").expect("status visible");
        assert!(mode_pos < model_pos);
        assert!(model_pos < cost_pos, "cost must follow model: {line:?}");
        assert!(cost_pos < status_pos, "cost must precede status: {line:?}");
    }

    /// Cost is preserved when status drops — cost is steady info, status is
    /// a transient signal.
    #[test]
    fn footer_cost_outranks_status_when_space_tight() {
        // "agent · deepseek-v4-flash · $0.42 · refreshing context" = 53 cols.
        // At 47 the status drops but the cost survives (47 ≥ 36 mode+model+cost).
        let props = props_with_status_and_cost("refreshing context", "$0.42");
        let line = render_at_width(props, 47);
        assert!(line.contains("agent"));
        assert!(line.contains("deepseek-v4-flash"));
        assert!(
            line.contains("$0.42"),
            "cost survives status drop: {line:?}"
        );
        assert!(!line.contains("refreshing"), "status dropped: {line:?}");
    }

    #[test]
    fn render_swaps_toast_for_status_line() {
        let app = make_app();
        let toast = super::FooterToast {
            text: "session saved".to_string(),
            color: Color::Green,
        };
        let props = FooterProps::from_app(
            &app,
            Some(toast),
            "ready",
            palette::TEXT_MUTED,
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
            Vec::<Span<'static>>::new(),
        );
        let widget = FooterWidget::new(props);

        let area = ratatui::layout::Rect::new(0, 0, 60, 1);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        widget.render(area, &mut buf);

        let rendered: String = (0..area.width).map(|x| buf[(x, 0)].symbol()).collect();
        assert!(rendered.contains("session saved"));
        assert!(!rendered.contains("agent"));
        assert!(!rendered.contains("deepseek-v4-flash"));
    }
}
