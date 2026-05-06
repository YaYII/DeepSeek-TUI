//! Workspace trust prompt for onboarding.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::localization;
use crate::palette;
use crate::tui::app::App;

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let locale = app.ui_locale;
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        localization::tr(locale, localization::MessageId::OnboardingTrustTitle),
        Style::default()
            .fg(palette::DEEPSEEK_SKY)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        localization::tr(locale, localization::MessageId::OnboardingTrustQuestion),
        Style::default().fg(palette::TEXT_PRIMARY),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "{} {}",
            localization::tr(locale, localization::MessageId::OnboardingTrustWorkspace),
            crate::utils::display_path(&app.workspace)
        ),
        Style::default().fg(palette::TEXT_MUTED),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        localization::tr(locale, localization::MessageId::OnboardingTrustYesExplanation),
        Style::default().fg(palette::TEXT_MUTED),
    )));
    lines.push(Line::from(Span::styled(
        localization::tr(locale, localization::MessageId::OnboardingTrustNoExplanation),
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
        Span::styled(localization::tr(locale, localization::MessageId::OnboardingTrustPressY), Style::default().fg(palette::TEXT_MUTED)),
        Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingTrustYKey),
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(localization::tr(locale, localization::MessageId::OnboardingTrustToTrust), Style::default().fg(palette::TEXT_MUTED)),
        Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingTrustNKey),
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(localization::tr(locale, localization::MessageId::OnboardingTrustToSkip), Style::default().fg(palette::TEXT_MUTED))
    ]));
    lines
}
