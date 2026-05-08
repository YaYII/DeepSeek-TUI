//! 欢迎/启动步骤。

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::palette;

pub fn lines() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "DeepSeek TUI",
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
            "一个面向长时间模型会话的沉浸式终端工作区。",
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            "你需要添加 API 密钥，确认是否信任此目录，然后进入聊天界面。",
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(Span::styled(
            "主输入框支持多行，你可以编写完整的提示词，而不用把所有内容挤在一行。",
            Style::default().fg(palette::TEXT_MUTED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "按 Enter 继续。",
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            "Ctrl+C 随时退出。",
            Style::default().fg(palette::TEXT_MUTED),
        )),
    ]
}
