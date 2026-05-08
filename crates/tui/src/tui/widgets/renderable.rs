//! 可渲染 trait — 可渲染 UI 组件的通用接口。
use ratatui::{buffer::Buffer, layout::Rect};

pub trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn desired_height(&self, width: u16) -> u16;
    fn cursor_pos(&self, _area: Rect) -> Option<(u16, u16)> {
        None
    }
}
