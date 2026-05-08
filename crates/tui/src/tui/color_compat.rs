//! 颜色兼容性辅助函数 — 将 ratatui 颜色转换为 ANSI 和终端颜色。
//!
//! Ratatui 的 crossterm 后端为每个 `Color::Rgb` 单元发出真彩色 SGR。
//! 这对于真彩色终端是正确的，但 macOS Terminal.app 通常只声明
//! `xterm-256color`；在那里发送 `38;2` / `48;2` 可能会呈现为杂散的
//! 绿色/青色背景。此后端在将每个单元交给 crossterm 之前，
//! 会将其适配到检测到的颜色深度。

use std::io::{self, Write};

use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};

use crate::palette::{self, ColorDepth, PaletteMode};

#[derive(Debug)]
pub(crate) struct ColorCompatBackend<W: Write> {
    inner: CrosstermBackend<W>,
    depth: ColorDepth,
    palette_mode: PaletteMode,
    /// 在调整大小事件期间，终端模拟器可能会在短时间内报告过时的尺寸
    ///（在 macOS Terminal.app 和 Windows ConHost 上观察到）。
    /// 强制使用预期大小可防止 ratatui 内部的 `autoresize` 在 `draw()`
    /// 内部将视口缩小回过时的尺寸。
    forced_size: Option<Size>,
}

impl<W: Write> ColorCompatBackend<W> {
    pub(crate) fn new(writer: W, depth: ColorDepth, palette_mode: PaletteMode) -> Self {
        Self {
            inner: CrosstermBackend::new(writer),
            depth,
            palette_mode,
            forced_size: None,
        }
    }

    /// 强制使用指定的终端尺寸，绕过 ratatui 的内部 autoresize。
    pub(crate) fn force_size(&mut self, size: Size) {
        self.forced_size = Some(size);
    }

    /// 清除强制尺寸，恢复为终端报告的实际尺寸。
    pub(crate) fn clear_forced_size(&mut self) {
        self.forced_size = None;
    }

    /// 在运行时更新调色板模式（深色/浅色）。
    pub(crate) fn set_palette_mode(&mut self, palette_mode: PaletteMode) {
        self.palette_mode = palette_mode;
    }
}

impl<W: Write> Write for ColorCompatBackend<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut self.inner)
    }
}

impl<W: Write> Backend for ColorCompatBackend<W> {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let adapted = content
            .map(|(x, y, cell)| {
                let mut cell = cell.clone();
                adapt_cell_colors(&mut cell, self.depth, self.palette_mode);
                (x, y, cell)
            })
            .collect::<Vec<_>>();
        self.inner
            .draw(adapted.iter().map(|(x, y, cell)| (*x, *y, cell)))
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        match self.forced_size {
            Some(size) => Ok(size),
            None => self.inner.size(),
        }
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

fn adapt_cell_colors(cell: &mut Cell, depth: ColorDepth, palette_mode: PaletteMode) {
    let original_bg = cell.bg;
    cell.fg = palette::adapt_fg_for_palette_mode(cell.fg, original_bg, palette_mode);
    cell.bg = palette::adapt_bg_for_palette_mode(cell.bg, palette_mode);
    cell.fg = palette::adapt_color(cell.fg, depth);
    cell.bg = palette::adapt_bg(cell.bg, depth);
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, io::Write, rc::Rc};

    use ratatui::backend::Backend;
    use ratatui::{buffer::Cell, style::Color};

    use super::*;

    #[derive(Clone, Default)]
    struct SharedWriter(Rc<RefCell<Vec<u8>>>);

    impl Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn adapts_rgb_cells_to_indexed_on_ansi256() {
        let mut cell = Cell::default();
        let mut cell = Cell::default();
        cell.set_fg(Color::Rgb(53, 120, 229));
        cell.set_bg(Color::Rgb(11, 21, 38));

        adapt_cell_colors(&mut cell, ColorDepth::Ansi256, PaletteMode::Dark);

        assert!(matches!(cell.fg, Color::Indexed(_)));
        assert!(matches!(cell.bg, Color::Indexed(_)));
    }

    #[test]
    fn leaves_truecolor_cells_unchanged() {
        let mut cell = Cell::default();
        cell.set_fg(Color::Rgb(53, 120, 229));
        cell.set_bg(Color::Rgb(11, 21, 38));

        adapt_cell_colors(&mut cell, ColorDepth::TrueColor, PaletteMode::Dark);

        assert_eq!(cell.fg, Color::Rgb(53, 120, 229));
        assert_eq!(cell.bg, Color::Rgb(11, 21, 38));
    }

    #[test]
    fn ansi256_backend_output_does_not_emit_truecolor_sgr() {
        let writer = SharedWriter::default();
        let capture = writer.0.clone();
        let mut backend = ColorCompatBackend::new(writer, ColorDepth::Ansi256, PaletteMode::Dark);
        let mut cell = Cell::default();
        cell.set_symbol("x")
            .set_fg(Color::Rgb(53, 120, 229))
            .set_bg(Color::Rgb(11, 21, 38));

        backend.draw(std::iter::once((0, 0, &cell))).unwrap();

        let output = String::from_utf8_lossy(&capture.borrow()).to_string();
        assert!(!output.contains("38;2;"), "{output:?}");
        assert!(!output.contains("48;2;"), "{output:?}");
    }

    #[test]
    fn light_palette_maps_dark_cells_before_depth_adaptation() {
        let mut cell = Cell::default();
        cell.set_fg(Color::White);
        cell.set_bg(Color::Rgb(11, 21, 38));

        adapt_cell_colors(&mut cell, ColorDepth::TrueColor, PaletteMode::Light);

        assert_eq!(cell.fg, palette::LIGHT_TEXT_BODY);
        assert_eq!(cell.bg, palette::LIGHT_SURFACE);
    }

    #[test]
    fn backend_palette_mode_can_follow_runtime_theme_changes() {
        let writer = SharedWriter::default();
        let mut backend = ColorCompatBackend::new(writer, ColorDepth::TrueColor, PaletteMode::Dark);

        assert_eq!(backend.palette_mode, PaletteMode::Dark);
        backend.set_palette_mode(PaletteMode::Light);
        assert_eq!(backend.palette_mode, PaletteMode::Light);
    }
}
