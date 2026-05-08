//! 首次运行引导向导。

pub mod api_key;
pub mod language;
pub mod trust_directory;
pub mod welcome;

use std::path::{Path, PathBuf};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};

use crate::palette;
use crate::tui::app::{App, OnboardingState};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().style(Style::default().bg(palette::DEEPSEEK_INK));
    f.render_widget(block, area);

    let content_width = 76.min(area.width.saturating_sub(4));
    let content_height = 20.min(area.height.saturating_sub(4));
    let content_area = Rect {
        x: (area.width - content_width) / 2,
        y: (area.height - content_height) / 2,
        width: content_width,
        height: content_height,
    };

    let lines = match app.onboarding {
        OnboardingState::Welcome => welcome::lines(),
        OnboardingState::Language => language::lines(app),
        OnboardingState::ApiKey => api_key::lines(app),
        OnboardingState::TrustDirectory => trust_directory::lines(app),
        OnboardingState::Tips => tips_lines(),
        OnboardingState::None => Vec::new(),
    };

    if !lines.is_empty() {
        let mut panel = Block::default()
            .title(Line::from(Span::styled(
                " DeepSeek TUI ",
                Style::default()
                    .fg(palette::DEEPSEEK_BLUE)
                    .add_modifier(Modifier::BOLD),
            )))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::BORDER_COLOR))
            .style(Style::default().bg(palette::DEEPSEEK_SLATE))
            .padding(Padding::new(2, 2, 1, 1));
        if !app.onboarding_workspace_trust_gate {
            let (step, total) = onboarding_step(app);
            panel = panel.title_bottom(Line::from(Span::styled(
                format!(" Step {step}/{total} "),
                Style::default()
                    .fg(palette::TEXT_MUTED)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        let inner = panel.inner(content_area);
        f.render_widget(panel, content_area);
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }
}

fn onboarding_step(app: &App) -> (usize, usize) {
    let needs_trust = !app.trust_mode && needs_trust(&app.workspace);
    // 欢迎 + 语言 + 提示始终显示。
    let mut total = 3;
    if app.onboarding_needs_api_key {
        total += 1;
    }
    if needs_trust {
        total += 1;
    }

    let step = match app.onboarding {
        OnboardingState::Welcome => 1,
        OnboardingState::Language => 2,
        OnboardingState::ApiKey => 3,
        OnboardingState::TrustDirectory => {
            // 欢迎（1）+ 语言（2）+ 可选的 API 密钥
            if app.onboarding_needs_api_key { 4 } else { 3 }
        }
        OnboardingState::Tips => total,
        OnboardingState::None => total,
    };

    (step, total)
}

pub fn tips_lines() -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};

    vec![
        Line::from(Span::styled(
            "快速上手",
            Style::default()
                .fg(palette::DEEPSEEK_SKY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::raw(
            "用自然语言描述任务。需要命令时使用 /help 或 Ctrl+K。",
        )),
        Line::from(Span::raw(
            "底部输入框支持多行：按 Enter 发送，按 Alt+Enter 或 Ctrl+J 换行。",
        )),
        Line::from(Span::raw(
            "仅在任务变化时切换模式：Plan 用于先审查再执行，Agent 用于自动执行，YOLO 用于自动审批。",
        )),
        Line::from(Span::raw(
            "Ctrl+R 恢复先前的会话，Esc 退出当前草稿或覆盖层。",
        )),
        Line::from(vec![
            Span::styled("按 ", Style::default().fg(palette::TEXT_MUTED)),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(palette::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " 打开工作区",
                Style::default().fg(palette::TEXT_MUTED),
            ),
        ]),
    ]
}

pub fn default_marker_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".deepseek").join(".onboarded"))
}

pub fn is_onboarded() -> bool {
    default_marker_path().is_some_and(|path| path.exists())
}

pub fn mark_onboarded() -> std::io::Result<PathBuf> {
    let path = default_marker_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found")
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, "")?;
    Ok(path)
}

pub fn needs_trust(workspace: &Path) -> bool {
    if crate::config::is_workspace_trusted(workspace) {
        return false;
    }

    let markers = [
        workspace.join(".deepseek").join("trusted"),
        workspace.join(".deepseek").join("trust.json"),
    ];
    !markers.iter().any(|path| path.exists())
}

pub fn mark_trusted(workspace: &Path) -> anyhow::Result<PathBuf> {
    crate::config::save_workspace_trust(workspace)
}
