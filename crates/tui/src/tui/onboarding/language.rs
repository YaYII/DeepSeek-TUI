//! Language picker for first-run onboarding (#566).
//!
//! Surfaces every locale the TUI ships translations for, plus an `auto`
//! option that defers to `LC_ALL` / `LANG`. Selection persists via
//! `Settings::save` immediately so the rest of onboarding (and every
//! subsequent session) reads the chosen tag.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::palette;
use crate::tui::app::App;

/// Locale options shown in the picker. Order matches the keyboard hotkeys.
/// Each entry is `(hotkey, settings_tag)`.
/// `settings_tag` is what `Settings::set("locale", …)` accepts.
///
/// Note: Display names are rendered dynamically based on the current locale.
pub const LANGUAGE_OPTIONS: &[(char, &str)] = &[
    ('1', "auto"),
    ('2', "en"),
    ('3', "ja"),
    ('4', "zh-Hans"),
    ('5', "zh-Hant"),
    ('6', "pt-BR"),
];

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let current_owned = app.current_locale_tag();
    let current = current_owned.as_str();

    // Detect system locale for display
    let detected = crate::localization::resolve_locale("auto");
    let detected_tag = detected.tag();

    let mut out: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            crate::localization::tr(app.ui_locale, crate::localization::MessageId::OnboardingLanguageTitle),
            Style::default()
                .fg(palette::DEEPSEEK_SKY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            crate::localization::tr(app.ui_locale, crate::localization::MessageId::OnboardingLanguageAutoDetected),
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("{} ", crate::localization::tr(app.ui_locale, crate::localization::MessageId::OnboardingLanguageDetected)),
                Style::default().fg(palette::TEXT_MUTED)
            ),
            Span::styled(
                detected_tag,
                Style::default()
                    .fg(palette::DEEPSEEK_BLUE)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            crate::localization::tr(app.ui_locale, crate::localization::MessageId::OnboardingLanguageChangeHint1),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(Span::styled(
            crate::localization::tr(app.ui_locale, crate::localization::MessageId::OnboardingLanguageChangeHint2),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(Span::styled(
            crate::localization::tr(app.ui_locale, crate::localization::MessageId::OnboardingLanguageChangeHint3),
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(""),
    ];

    out.push(Line::from(""));
    out.push(Line::from(vec![
        Span::styled(
            crate::localization::tr(app.ui_locale, crate::localization::MessageId::OnboardingLanguagePressKey),
            Style::default().fg(palette::TEXT_MUTED)
        ),
        Span::styled(
            "Enter",
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            crate::localization::tr(app.ui_locale, crate::localization::MessageId::OnboardingLanguageToContinue),
            Style::default().fg(palette::TEXT_MUTED)
        ),
    ]));

    out
}
