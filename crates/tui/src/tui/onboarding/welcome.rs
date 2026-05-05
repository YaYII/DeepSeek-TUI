//! Welcome screen content for onboarding.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::localization::{Locale, MessageId, tr};
use crate::palette;

pub fn lines(locale: Locale) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingWelcomeTitle),
            Style::default()
                .fg(palette::DEEPSEEK_BLUE)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("Version {}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingWelcomeDesc),
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingWelcomeStep1),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingWelcomeStep2),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingWelcomePromptEnter),
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            tr(locale, MessageId::OnboardingWelcomePromptExit),
            Style::default().fg(palette::TEXT_MUTED),
        )),
    ]
}
