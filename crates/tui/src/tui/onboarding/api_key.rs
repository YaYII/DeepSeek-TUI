//! API 密钥输入屏幕（入门引导）。

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::palette;
use crate::tui::app::App;

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "连接你的 DeepSeek API 密钥",
            Style::default()
                .fg(palette::DEEPSEEK_SKY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "步骤 1：打开 https://platform.deepseek.com/api_keys 并创建一个密钥。",
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            "步骤 2：将密钥粘贴到下方并按 Enter。",
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "保存到 ~/.deepseek/config.toml，可在任意目录下使用。",
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(Span::styled(
            "请完整粘贴所颁发的密钥（不含空格或换行）。",
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
    ];

    let masked = mask_key(&app.api_key_input);
    let display = if masked.is_empty() {
        "（在此处粘贴密钥）"
    } else {
        masked.as_str()
    };
    lines.push(Line::from(vec![
        Span::styled("Key: ", Style::default().fg(palette::TEXT_MUTED)),
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
        "按 Enter 保存，按 Esc 返回。",
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
