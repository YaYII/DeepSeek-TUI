//! 文件树面板 — Ctrl+Shift+E 切换左侧工作区文件导航器。
//!
//! 显示带有可展开目录的工作区目录树。Up/Down 导航，
//! Enter 展开/折叠目录或为文件插入 `@path`，
//! Esc 关闭面板。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

use crate::deepseek_theme::Theme;
use crate::palette;
use crate::tui::ui::truncate_line_to_width;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// 文件树中的单个条目。
#[derive(Debug, Clone)]
pub struct FileTreeEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

/// 文件树面板的可变状态。
#[derive(Debug, Clone)]
pub struct FileTreeState {
    /// 可见条目的扁平列表（考虑展开/折叠状态）。
    pub entries: Vec<FileTreeEntry>,
    /// 光标在 `entries` 中的索引。
    pub cursor: usize,
    /// 在 `entries` 中的滚动偏移量。
    pub scroll_offset: usize,
    /// 已展开目录路径的集合（规范化）。
    pub expanded_dirs: HashSet<PathBuf>,
    /// 工作区根目录。
    pub workspace: PathBuf,
    /// 树是否仍在构建中（异步初始遍历进行中）。
    pub is_loading: bool,
    /// 用于异步树构建结果的共享单元 (#399 S3)。
    loading_cell: Option<Arc<Mutex<Option<Vec<FileTreeEntry>>>>>,
}

impl FileTreeState {
    /// 通过遍历 `workspace` 构建新的树状态。
    /// 在后台线程上启动初始遍历 (#399 S3)。
    pub fn new(workspace: &Path) -> Self {
        let expanded_dirs = HashSet::new();
        let loading_cell = Arc::new(Mutex::new(None));
        let cell = loading_cell.clone();
        let ws = workspace.to_path_buf();
        crate::utils::spawn_blocking_supervised("file-tree-build", move || {
            let entries = build_file_tree_inner(&ws, &HashSet::new(), None);
            if let Ok(mut guard) = cell.lock() {
                *guard = Some(entries);
            }
        });
        Self {
            entries: Vec::new(),
            cursor: 0,
            scroll_offset: 0,
            expanded_dirs,
            workspace: workspace.to_path_buf(),
            is_loading: true,
            loading_cell: Some(loading_cell),
        }
    }

    /// 轮询异步构建结果。从渲染循环中调用。
    pub fn poll_loading(&mut self) {
        if !self.is_loading {
            return;
        }
        // Take the Arc out temporarily to avoid a double-borrow of self.
        let cell = match self.loading_cell.take() {
            Some(c) => c,
            None => return,
        };
        let mut done = false;
        if let Ok(mut guard) = cell.lock()
            && let Some(entries) = guard.take()
        {
            self.entries = entries;
            self.is_loading = false;
            self.clamp_cursor();
            done = true;
        }
        if !done {
            // Put the cell back so we can poll again next frame.
            self.loading_cell = Some(cell);
        }
    }

    /// 从当前的 `expanded_dirs` 集合重建扁平条目列表。
    /// 加载进行中时，重建会延迟。
    pub fn rebuild(&mut self) {
        if self.is_loading {
            // 延迟重建直到异步加载完成
            return;
        }
        self.entries = build_file_tree_inner(&self.workspace, &self.expanded_dirs, None);
        self.clamp_cursor();
    }

    /// 将光标上移一行。
    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
        self.clamp_scroll();
    }

    /// 将光标下移一行。
    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
        self.clamp_scroll();
    }

    /// 激活光标下的条目。
    ///
    /// 当条目为应被提及的文件时返回 `Some(path)`（`@path` 插入到编辑器中）。
    /// 切换目录展开/折叠后返回 `None`。
    pub fn activate(&mut self) -> Option<PathBuf> {
        let entry = self.entries.get(self.cursor)?;
        if entry.is_dir {
            let norm = normalize_path(&entry.path);
            if self.expanded_dirs.contains(&norm) {
                self.expanded_dirs.remove(&norm);
            } else {
                self.expanded_dirs.insert(norm);
            }
            self.rebuild();
            None
        } else {
            // 返回相对于工作区的路径。
            entry.path.strip_prefix(&self.workspace).ok().map(|rel| {
                let mut p = PathBuf::new();
                for comp in rel.components() {
                    p.push(comp);
                }
                p
            })
        }
    }

    /// 确保光标在边界内。
    fn clamp_cursor(&mut self) {
        if !self.entries.is_empty() && self.cursor >= self.entries.len() {
            self.cursor = self.entries.len().saturating_sub(1);
        }
    }

    /// 确保滚动偏移使光标保持可见。
    fn clamp_scroll(&mut self) {
        let visible_height = 20usize; // will be overridden per render
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        }
        if self.scroll_offset + visible_height <= self.cursor {
            self.scroll_offset = self.cursor.saturating_add(1).saturating_sub(visible_height);
        }
    }

    /// Adjust scroll for a given visible height.
    #[allow(dead_code)]
    pub fn adjust_scroll(&mut self, visible: usize) {
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        }
        if visible > 0 && self.cursor >= self.scroll_offset + visible {
            self.scroll_offset = self.cursor.saturating_add(1).saturating_sub(visible);
        }
    }
}

// ---------------------------------------------------------------------------
// Tree building
// ---------------------------------------------------------------------------

/// Build the flat visible-entry list.
///
/// Walks the workspace directory recursively. Directories in `expanded_dirs`
/// have their children included; collapsed directories show only the directory
/// entry itself. Entries are sorted: directories first, then files, each group
/// alphabetically.
fn build_file_tree_inner(
    workspace: &Path,
    expanded_dirs: &HashSet<PathBuf>,
    single_root: Option<&Path>,
) -> Vec<FileTreeEntry> {
    let mut entries: Vec<FileTreeEntry> = Vec::new();

    // Determine which root to scan.
    let scan_root = single_root.unwrap_or(workspace);

    // Collect children of `scan_root`.
    let mut children: Vec<(String, PathBuf, bool)> = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(scan_root) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            // Skip well-known ignored directories.
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && matches!(name, ".git" | "node_modules" | "target" | ".DS_Store")
            {
                continue;
            }
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            let is_dir = ft.is_dir();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_string())
                .unwrap_or_default();
            children.push((name, path, is_dir));
        }
    }

    // Sort: dirs first, then files, alphabetical within each group.
    children.sort_by(
        |(a_name, _, a_dir), (b_name, _, b_dir)| match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a_name.to_lowercase().cmp(&b_name.to_lowercase()),
        },
    );

    // Compute depth for the current level.
    let depth = if single_root.is_some() {
        let rel = scan_root.strip_prefix(workspace).unwrap_or(scan_root);
        rel.components().count()
    } else {
        0
    };

    for (name, path, is_dir) in &children {
        let norm = normalize_path(path);
        let is_expanded = *is_dir && expanded_dirs.contains(&norm);

        entries.push(FileTreeEntry {
            name: name.clone(),
            path: path.clone(),
            is_dir: *is_dir,
            depth,
            expanded: is_expanded,
        });

        // If it's an expanded directory, recurse.
        if is_expanded {
            let sub = build_file_tree_inner(workspace, expanded_dirs, Some(path));
            entries.extend(sub);
        }
    }

    entries
}

/// Normalise a path for use as a HashSet key.
fn normalize_path(path: &Path) -> PathBuf {
    let components: Vec<_> = path.components().collect();
    // Try to strip workspace prefix.
    PathBuf::from_iter(components.iter().map(|c| c.as_os_str()))
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

const FILE_TREE_MIN_WIDTH: u16 = 20;

/// Render the file tree inside `area`.
/// Polls async loading state before rendering (#399 S3).
pub fn render_file_tree(
    f: &mut Frame,
    area: Rect,
    state: &mut FileTreeState,
    mode: palette::PaletteMode,
) {
    state.poll_loading();
    if area.width < FILE_TREE_MIN_WIDTH || area.height < 3 {
        return;
    }

    let content_width = area.width.saturating_sub(4) as usize;
    let visible_rows = area.height.saturating_sub(3) as usize;

    let scroll = state.scroll_offset;
    let max_visible = visible_rows.max(1);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(max_visible + 1);

    if state.is_loading {
        lines.push(Line::from(Span::styled(
            "  Building file tree...",
            Style::default().fg(palette::TEXT_MUTED),
        )));
    } else if state.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (empty)",
            Style::default().fg(palette::TEXT_MUTED),
        )));
    } else {
        let render_end = (scroll + max_visible).min(state.entries.len());
        for idx in scroll..render_end {
            let entry = &state.entries[idx];
            let is_selected = idx == state.cursor;

            // Build the line prefix: indent + expand/collapse marker + icon.
            let indent = "  ".repeat(entry.depth);
            let expand_marker = if entry.is_dir {
                if entry.expanded {
                    "\u{25BC} "
                } else {
                    "\u{25B6} "
                } // ▼ / ▶
            } else {
                "  "
            };
            let icon = if entry.is_dir {
                "\u{1F4C1} "
            } else {
                "\u{1F4C4} "
            }; // 📁 / 📄

            // Build the display text.
            let raw = format!("{indent}{expand_marker}{icon}{}", entry.name);
            let display = truncate_line_to_width(&raw, content_width.max(1));

            let style = if is_selected {
                Style::default()
                    .fg(palette::SELECTION_TEXT)
                    .bg(palette::SELECTION_BG)
            } else {
                Style::default().fg(palette::TEXT_PRIMARY)
            };

            lines.push(Line::from(Span::styled(display, style)));
        }
    }

    // Use the same theme as the sidebar for consistent styling.
    let theme = Theme::for_palette_mode(mode);
    let section = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .title(Line::from(Span::styled(
                " Files ",
                Style::default().fg(theme.section_title_color).bold(),
            )))
            .borders(theme.section_borders)
            .border_type(theme.section_border_type)
            .border_style(Style::default().fg(theme.section_border_color))
            .style(Style::default().bg(theme.section_bg))
            .padding(theme.section_padding),
    );

    f.render_widget(section, area);
}
