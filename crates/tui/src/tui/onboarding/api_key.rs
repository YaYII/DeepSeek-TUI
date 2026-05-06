//! API key entry screen for onboarding.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::localization;
use crate::palette;
use crate::tui::app::App;

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let locale = app.ui_locale;
    let mut lines = vec![
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingApiKeyTitle),
            Style::default()
                .fg(palette::DEEPSEEK_SKY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingApiKeyStep1),
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingApiKeyStep2),
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(""),
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingApiKeySavedTo),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(Span::styled(
            localization::tr(locale, localization::MessageId::OnboardingApiKeyPasteExact),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
    ];

    let masked = mask_key(&app.api_key_input);
    let display = if masked.is_empty() {
        localization::tr(locale, localization::MessageId::OnboardingApiKeyPlaceholder)
    } else {
        masked.as_str()
    };
    lines.push(Line::from(vec![
        Span::styled(localization::tr(locale, localization::MessageId::OnboardingApiKeyLabel), Style::default().fg(palette::TEXT_MUTED)),
        Span::styled(
            display.to_string(),
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    if let Some(message) = app.status_message.as_deref() {
        lines.push(Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(palette::STATUS_WARNING),
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        localization::tr(locale, localization::MessageId::OnboardingApiKeyInstructions),
        Style::default().fg(palette::TEXT_MUTED),
    )));

    lines
}

fn mask_key(input: &str) -> String {
    let trimmed = input.trim();
    let len = trimmed.chars().count();
    if len == 0 {
        return String::new();
    }
    if len <= 4 {
        return "*".repeat(len);
    }
    let visible: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{}{}", "*".repeat(len - 4), visible)
}
