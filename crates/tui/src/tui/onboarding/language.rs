//! 语言选择步骤。
//!
//! 展示 TUI 支持的所有语言环境，以及一个 `auto` 选项，
//! 该选项会遵循 `LC_ALL` / `LANG`。选择会立即通过
//! `Settings::save` 持久化，以便后续的入门引导步骤（以及每次
//! 后续会话）都会读取所选标签。

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::palette;
use crate::tui::app::App;

/// 语言选择器中展示的地区选项。顺序与键盘快捷键匹配
///（1-5）。每个条目为 `(hotkey, settings_tag, native_name, english_label)`。
/// `settings_tag` 是 `Settings::set("locale", …)` 接受的标签，
/// `localization::Locale` 会在下次读取时解析。
pub const LANGUAGE_OPTIONS: &[(char, &str, &str, &str)] = &[
    ('1', "auto", "自动检测", "(LC_ALL / LANG)"),
    ('2', "en", "English", ""),
    ('3', "ja", "日本語", "（日语）"),
    ('4', "zh-Hans", "简体中文", "（简体中文）"),
    ('5', "pt-BR", "Português (Brasil)", "（巴西葡萄牙语）"),
];

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let current_owned = app.current_locale_tag();
    let current = current_owned.as_str();

    let mut out: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "选择语言",
            Style::default()
                .fg(palette::DEEPSEEK_SKY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "选择界面语言。你随时可以通过 `/settings set locale <tag>` 更改。",
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
    ];

    for (hotkey, tag, native, english) in LANGUAGE_OPTIONS {
        let is_current = current == *tag;
        let bullet = if is_current { "●" } else { "○" };
        let bullet_color = if is_current {
            palette::DEEPSEEK_BLUE
        } else {
            palette::TEXT_MUTED
        };
        let mut spans: Vec<Span<'static>> = vec![
            Span::styled(format!("  {bullet}  "), Style::default().fg(bullet_color)),
            Span::styled(
                format!("[{hotkey}] "),
                Style::default()
                    .fg(palette::TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                native.to_string(),
                Style::default().fg(palette::TEXT_PRIMARY),
            ),
        ];
        if !english.is_empty() {
            spans.push(Span::styled(
                format!(" {english}"),
                Style::default().fg(palette::TEXT_MUTED),
            ));
        }
        out.push(Line::from(spans));
    }

    out.push(Line::from(""));
    out.push(Line::from(vec![
        Span::styled("按 ", Style::default().fg(palette::TEXT_MUTED)),
        Span::styled(
            "1-5",
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" 选择，或按 ", Style::default().fg(palette::TEXT_MUTED)),
        Span::styled(
            "Enter",
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " 保留当前设置",
            Style::default().fg(palette::TEXT_MUTED),
        ),
    ]));

    out
}
