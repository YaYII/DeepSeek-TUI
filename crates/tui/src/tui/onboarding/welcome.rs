//! Welcome screen content for onboarding.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::localization;
use crate::palette;
use crate::tui::app::App;

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let locale = app.ui_locale;
    vec![
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingWelcomeTitle),
            Style::default()
                .fg(palette::DEEPSEEK_BLUE)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "{} {}",
                localization::tr(locale, localization::MessageId::OnboardingWelcomeVersion),
                env!("CARGO_PKG_VERSION")
            ),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            localization::tr(
                locale,
                localization::MessageId::OnboardingWelcomeDescription,
            ),
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingWelcomeSteps),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingWelcomeComposer),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingWelcomePressEnter),
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingWelcomeCtrlC),
            Style::default().fg(palette::TEXT_MUTED),
        )),
    ]
}
