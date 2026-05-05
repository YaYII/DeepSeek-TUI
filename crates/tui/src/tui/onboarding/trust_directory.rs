//! Workspace trust prompt for onboarding.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::localization::{MessageId, tr};
use crate::palette;
use crate::tui::app::App;

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let locale = app.ui_locale;
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        tr(locale, MessageId::OnboardingTrustTitle),
        Style::default()
            .fg(palette::DEEPSEEK_SKY)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        tr(locale, MessageId::OnboardingTrustPrompt),
        Style::default().fg(palette::TEXT_PRIMARY),
    )));
    lines.push(Line::from(Span::styled(
        tr(locale, MessageId::OnboardingTrustWorkspaceLabel)
            .replace("{path}", &crate::utils::display_path(&app.workspace)),
        Style::default().fg(palette::TEXT_MUTED),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        tr(locale, MessageId::OnboardingTrustYExplain),
        Style::default().fg(palette::TEXT_MUTED),
    )));
    lines.push(Line::from(Span::styled(
        tr(locale, MessageId::OnboardingTrustNExplain),
        Style::default().fg(palette::TEXT_MUTED),
    )));
    if let Some(message) = app.status_message.as_deref() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(palette::STATUS_WARNING),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Press ", Style::default().fg(palette::TEXT_MUTED)),
        Span::styled(
            "Y",
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" to trust, ", Style::default().fg(palette::TEXT_MUTED)),
        Span::styled(
            "N",
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" to skip", Style::default().fg(palette::TEXT_MUTED)),
    ]));
    lines
}
