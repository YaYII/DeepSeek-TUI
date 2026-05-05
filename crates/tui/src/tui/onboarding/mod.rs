//! Onboarding flow rendering and helpers.

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
use crate::localization::{MessageId, tr};
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

    let locale = app.ui_locale;
    let lines = match app.onboarding {
        OnboardingState::Welcome => welcome::lines(locale),
        OnboardingState::Language => language::lines(app),
        OnboardingState::ApiKey => api_key::lines(app),
        OnboardingState::TrustDirectory => trust_directory::lines(app),
        OnboardingState::Tips => tips_lines(locale),
        OnboardingState::Translating => translating_lines(app),
        OnboardingState::None => Vec::new(),
    };

    if !lines.is_empty() {
        let (step, total) = onboarding_step(app);
        let panel = Block::default()
            .title(Line::from(Span::styled(
                tr(locale, MessageId::OnboardingPanelTitle),
                Style::default()
                    .fg(palette::DEEPSEEK_BLUE)
                    .add_modifier(Modifier::BOLD),
            )))
            .title_bottom(Line::from(Span::styled(
                tr(locale, MessageId::OnboardingStepIndicator)
                    .replace("{step}", &step.to_string())
                    .replace("{total}", &total.to_string()),
                Style::default()
                    .fg(palette::TEXT_MUTED)
                    .add_modifier(Modifier::BOLD),
            )))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::BORDER_COLOR))
            .style(Style::default().bg(palette::DEEPSEEK_SLATE))
            .padding(Padding::new(2, 2, 1, 1));
        let inner = panel.inner(content_area);
        f.render_widget(panel, content_area);
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }
}

fn onboarding_step(app: &App) -> (usize, usize) {
    let needs_trust = !app.trust_mode && needs_trust(&app.workspace);
    // Flow: Welcome → ApiKey(optional) → Language → Trust(optional) → Tips
    let mut total = 3; // Welcome + Language + Tips
    if app.onboarding_needs_api_key {
        total += 1;
    }
    if needs_trust {
        total += 1;
    }

    let step = match app.onboarding {
        OnboardingState::Welcome => 1,
        OnboardingState::ApiKey => 2,
        OnboardingState::Language | OnboardingState::Translating => {
            if app.onboarding_needs_api_key { 3 } else { 2 }
        }
        OnboardingState::TrustDirectory => {
            if app.onboarding_needs_api_key { 4 } else { 3 }
        }
        OnboardingState::Tips => total,
        OnboardingState::None => total,
    };

    (step, total)
}

pub fn tips_lines(locale: crate::localization::Locale) -> Vec<ratatui::text::Line<'static>> {
    vec![
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingTipsTitle),
            Style::default()
                .fg(palette::DEEPSEEK_SKY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingTipsTip1),
            Style::default(),
        )),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingTipsTip2),
            Style::default(),
        )),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingTipsTip3),
            Style::default(),
        )),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingTipsTip4),
            Style::default(),
        )),
        Line::from(vec![
            Span::styled("Press ", Style::default().fg(palette::TEXT_MUTED)),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(palette::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to open the workspace",
                Style::default().fg(palette::TEXT_MUTED),
            ),
        ]),
    ]
}

/// Rendered while the AI translator is translating UI strings via the
/// DeepSeek API. Shows an animated progress indicator with elapsed time.
///
/// Note: strings here are hardcoded English because this screen renders
/// *during* translation — the target language text doesn't exist yet.
pub fn translating_lines(app: &App) -> Vec<ratatui::text::Line<'static>> {
    let elapsed = app
        .translation_started_at
        .map(|start| start.elapsed().as_secs())
        .unwrap_or(0);

    let dots_count = (elapsed % 4) + 1;
    let dots = ".".repeat(dots_count as usize);

    let title = format!("Translating{}", dots);

    vec![
        Line::from(Span::styled(
            title,
            Style::default()
                .fg(palette::DEEPSEEK_SKY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "The UI is being translated to your selected language via DeepSeek API.",
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            format!("Elapsed: {}s (usually takes 10-30 seconds)", elapsed),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Please wait...",
            Style::default().fg(palette::TEXT_MUTED),
        )),
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
    let markers = [
        workspace.join(".deepseek").join("trusted"),
        workspace.join(".deepseek").join("trust.json"),
    ];
    !markers.iter().any(|path| path.exists())
}

pub fn mark_trusted(workspace: &Path) -> std::io::Result<PathBuf> {
    let dir = workspace.join(".deepseek");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("trusted");
    std::fs::write(&path, "")?;
    Ok(path)
}
