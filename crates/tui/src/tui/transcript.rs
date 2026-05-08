//! 转录本 — 会话消息历史的可滚动视图。
//!
//! ## 每个单元格的修订缓存
//!
//! 朴素缓存在任何单元格发生变化时会使整个转录本失效。
//! 在流式传输期间，助手内容单元格在每个数据块上都会发生变化 —
//! 这将强制在每个块上重新包装每个单元格。Codex 通过跟踪每个单元格的
//! 修订计数器来避免这种情况；我们在此镜像该模式。
//!
//! 每个单元格索引都有一个配对的 `revision: u64`。缓存存储
//! `Vec<CachedCell>`，包含 `(cell_index, revision, lines, line_meta)`。
//! 在 `ensure` 上，遍历单元格；如果单元格当前的 `revision` 与缓存的
//! 匹配（且宽度/选项未更改），则重用渲染的行。
//! 否则仅重新渲染该单元格并重新组装。
//!
//! 宽度或渲染选项的更改仍会使整个缓存失效（正确的行为：
//! 换行布局取决于宽度以及哪些单元格整体可见）。

use std::sync::Arc;

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::tui::app::TranscriptSpacing;
use crate::tui::history::{HistoryCell, TranscriptRenderOptions};
use crate::tui::scrolling::TranscriptLineMeta;

/// 每个单元格的缓存渲染输出。当上游单元格的修订计数器未更改时，
/// 在 `ensure` 调用中重用。
///
/// 行存储在 `Arc` 后面，以便在缓存确保期间克隆 `CachedCell`
///（每帧触及每个单元格）是 O(1) 而非 O(rendered_line_count)。
/// 没有这个，在长转录本上滚动需要为每帧深克隆每个单元格的 `Vec<Line>`，
/// 这是 issue #78 的表面症状。扁平化步骤使用 `Arc::make_mut`
/// 为最终的 `lines` 组装生成拥有的 `Vec`，
/// 因此唯一的深克隆发生在扁平化输出上 — 每帧一次而不是每个单元格一次。
#[derive(Debug, Clone)]
struct CachedCell {
    /// 渲染此行/元数据时该单元格所处的修订版本。
    revision: u64,
    /// 此单元格的渲染行（不包含尾部单元格间分隔符），
    /// 通过 `Arc` 共享，因此缓存枚举为 O(N) 而非 O(N*lines)。
    lines: Arc<Vec<Line<'static>>>,
    /// 此单元格的渲染输出是否为空（例如 Thinking 隐藏时）。
    /// 缓存以便跳过空单元格而无需重新渲染。
    is_empty: bool,
    /// 此单元格是否为流式延续。决定分隔符规则。
    /// 缓存该值是因为 `is_stream_continuation` 方法本身成本低，
    /// 但通过缓存读取可以免于触碰单元格来决定分隔符。
    is_stream_continuation: bool,
    /// 此单元格是否为会话类型（User/Assistant/Thinking）。用于分隔符计算。
    is_conversational: bool,
    /// 此单元格是否为 System 或 Tool 单元格（影响分隔符规则）。
    is_system_or_tool: bool,
    /// 此单元格是否参与紧凑的工具卡片导轨组。
    is_tool_groupable: bool,
}

/// Cache of rendered transcript lines for the current viewport.
#[derive(Debug)]
pub struct TranscriptViewCache {
    width: u16,
    options: TranscriptRenderOptions,
    /// 每个单元格的渲染输出，按当前单元格位置索引。
    /// 长度始终等于上次 `ensure` 调用时看到的单元格数量。
    per_cell: Vec<CachedCell>,
    /// 从 `per_cell` 加上分隔符重新组装后的扁平行。
    lines: Vec<Line<'static>>,
    /// 与 `lines` 对齐的每行元数据。
    line_meta: Vec<TranscriptLineMeta>,
}

impl TranscriptViewCache {
    /// 创建一个空缓存。
    #[must_use]
    pub fn new() -> Self {
        Self {
            width: 0,
            options: TranscriptRenderOptions::default(),
            per_cell: Vec::new(),
            lines: Vec::new(),
            line_meta: Vec::new(),
        }
    }

    /// 确保缓存行与提供的单元格/宽度/每单元格修订版本匹配。
    ///
    /// 对于 `cell_revisions[i]` 与先前缓存的修订版本匹配的单元格，
    /// 重用其渲染行（当单元格形状 — 空/分隔符标志 — 也匹配时）。
    /// 宽度或选项的更改会使整个缓存失效。
    ///
    /// `cell_revisions.len()` 应与 `cells.len()` 相等。如果它们
    /// 不一致（正常使用中不应发生），缓存会将每个单元格视为脏的。
    ///
    /// 为测试和外部使用保留；实时渲染路径使用
    /// `ensure_split` 变体以避免每帧连接历史记录 + 活动单元格条目。
    #[allow(dead_code)]
    pub fn ensure(
        &mut self,
        cells: &[HistoryCell],
        cell_revisions: &[u64],
        width: u16,
        options: TranscriptRenderOptions,
    ) {
        self.ensure_split(&[cells], cell_revisions, width, options);
    }

    /// 确保缓存行与提供的单元格分片（逻辑上拼接）加上每单元格修订版本匹配。
    /// 避免了调用方在长转录本上每帧都要付出的
    /// `concat-into-Vec<HistoryCell>` 克隆开销。
    pub fn ensure_split(
        &mut self,
        cell_shards: &[&[HistoryCell]],
        cell_revisions: &[u64],
        width: u16,
        options: TranscriptRenderOptions,
    ) {
        let total_cells: usize = cell_shards.iter().map(|s| s.len()).sum();

        let layout_changed = self.width != width || self.options != options;
        if layout_changed {
            self.per_cell.clear();
        }
        self.width = width;
        self.options = options;

        // 跟踪是否有任何内容发生更改；如果所有单元格都在同一索引处被重用，
        // 我们可以跳过重新扁平化。
        let old_len = self.per_cell.len();
        let mut any_dirty = layout_changed || old_len != total_cells;
        let mut first_dirty: Option<usize> = if old_len != total_cells {
            Some(old_len.min(total_cells))
        } else {
            None
        };

        let mut new_per_cell: Vec<CachedCell> = Vec::with_capacity(total_cells);
        let revisions_match = cell_revisions.len() == total_cells;

        let mut idx: usize = 0;
        for shard in cell_shards {
            for cell in *shard {
                let current_rev = if revisions_match {
                    cell_revisions[idx]
                } else {
                    // 没有匹配的修订版本 — 强制在此周期重新渲染。
                    u64::MAX
                };

                // 如果修订版本匹配且在相同索引处，则重用缓存条目
                //（单元格在插入/删除时可能移动，因此我们仅在索引相同时
                // 才重用 — codex 对其活动单元格尾部也使用此更严格的不变条件）。
                if let Some(prev) = self.per_cell.get(idx)
                    && !layout_changed
                    && prev.revision == current_rev
                    && revisions_match
                {
                    new_per_cell.push(prev.clone());
                    idx += 1;
                    continue;
                }

                any_dirty = true;
                first_dirty = Some(first_dirty.map_or(idx, |current| current.min(idx)));
                let is_tool_groupable = matches!(cell, HistoryCell::Tool(_));
                let render_width = if is_tool_groupable {
                    width.saturating_sub(2).max(1)
                } else {
                    width
                };
                let rendered = cell.lines_with_options(render_width, options);
                let is_empty = rendered.is_empty();
                new_per_cell.push(CachedCell {
                    revision: current_rev,
                    lines: Arc::new(rendered),
                    is_empty,
                    is_stream_continuation: cell.is_stream_continuation(),
                    is_conversational: cell.is_conversational(),
                    is_system_or_tool: matches!(
                        cell,
                        HistoryCell::System { .. }
                            | HistoryCell::Error { .. }
                            | HistoryCell::Tool(_)
                            | HistoryCell::SubAgent(_)
                            | HistoryCell::ArchivedContext { .. }
                    ),
                    is_tool_groupable,
                });
                idx += 1;
            }
        }

        self.per_cell = new_per_cell;

        if !any_dirty {
            // 所有单元格都在相同索引处被重用：无需重新扁平化。
            //（宽度也未改变，因为那会触发 `layout_changed`。）
            return;
        }

        let rebuild_from = if layout_changed {
            0
        } else {
            first_dirty.unwrap_or(0).saturating_sub(1)
        };
        self.flatten_from(options.spacing, rebuild_from);
    }

    /// 从 `per_cell` 加上分隔符重新组装扁平的 `lines` / `line_meta`。
    fn flatten(&mut self, spacing: TranscriptSpacing) {
        self.lines.clear();
        self.line_meta.clear();
        self.append_flattened_cells(spacing, 0);
    }

    /// 仅重新组装从 `first_cell` 开始的后缀。
    ///
    /// 流式传输通常会修改活动尾部单元格。
    /// 从前一个单元格开始重建可保持分隔符正确性，
    /// 同时避免在每个令牌块上进行完整的 O(总转录行数) 扁平化。
    fn flatten_from(&mut self, spacing: TranscriptSpacing, first_cell: usize) {
        if first_cell == 0 || self.lines.is_empty() || self.line_meta.is_empty() {
            self.flatten(spacing);
            return;
        }

        let truncate_at = self
            .line_meta
            .iter()
            .position(|meta| match meta {
                TranscriptLineMeta::CellLine { cell_index, .. } => *cell_index >= first_cell,
                TranscriptLineMeta::Spacer => false,
            })
            .unwrap_or(self.lines.len());
        self.lines.truncate(truncate_at);
        self.line_meta.truncate(truncate_at);
        self.append_flattened_cells(spacing, first_cell);
    }

    fn append_flattened_cells(&mut self, spacing: TranscriptSpacing, start_cell: usize) {
        for (cell_index, cached) in self.per_cell.iter().enumerate().skip(start_cell) {
            if cached.is_empty {
                continue;
            }
            // Arc::make_mut 仅在写入时深克隆；由于我们从头重建了 `lines`，
            // 我们始终需要拥有数据。Deref 是零成本的，并给我们 &[Line]。
            let rendered_line_count = cached.lines.len();
            for (line_in_cell, line) in cached.lines.iter().enumerate() {
                self.lines.push(line_with_group_rail(
                    line,
                    tool_group_rail(
                        self.per_cell.as_slice(),
                        cell_index,
                        line_in_cell,
                        rendered_line_count,
                    ),
                    usize::from(self.width),
                ));
                self.line_meta.push(TranscriptLineMeta::CellLine {
                    cell_index,
                    line_in_cell,
                });
            }

            if let Some(next) = self.per_cell.get(cell_index + 1) {
                let spacer_rows = spacer_rows_between(cached, next, spacing);
                for _ in 0..spacer_rows {
                    self.lines.push(Line::from(""));
                    self.line_meta.push(TranscriptLineMeta::Spacer);
                }
            }
        }
    }

    /// 返回缓存行。
    #[must_use]
    pub fn lines(&self) -> &[Line<'static>] {
        &self.lines
    }

    /// 返回缓存的行元数据。
    #[must_use]
    pub fn line_meta(&self) -> &[TranscriptLineMeta] {
        &self.line_meta
    }

    /// 返回缓存行总数。
    #[must_use]
    pub fn total_lines(&self) -> usize {
        self.lines.len()
    }
}

fn spacer_rows_between(
    current: &CachedCell,
    next: &CachedCell,
    spacing: TranscriptSpacing,
) -> usize {
    if current.is_stream_continuation {
        return 0;
    }

    if current.is_tool_groupable && next.is_tool_groupable {
        return 0;
    }

    let conversational_gap = match spacing {
        TranscriptSpacing::Compact => 0,
        TranscriptSpacing::Comfortable => 1,
        TranscriptSpacing::Spacious => 2,
    };
    let secondary_gap = match spacing {
        TranscriptSpacing::Compact => 0,
        TranscriptSpacing::Comfortable | TranscriptSpacing::Spacious => 1,
    };

    if current.is_conversational && next.is_conversational {
        conversational_gap
    } else if current.is_system_or_tool || next.is_system_or_tool {
        secondary_gap
    } else {
        0
    }
}

fn tool_group_rail(
    cells: &[CachedCell],
    cell_index: usize,
    line_in_cell: usize,
    rendered_line_count: usize,
) -> Option<crate::tui::widgets::tool_card::CardRail> {
    let cached = cells.get(cell_index)?;
    if !cached.is_tool_groupable || rendered_line_count == 0 {
        return None;
    }

    let previous_is_tool = cell_index
        .checked_sub(1)
        .and_then(|idx| cells.get(idx))
        .is_some_and(|cell| cell.is_tool_groupable && !cell.is_empty);
    let next_is_tool = cells
        .get(cell_index + 1)
        .is_some_and(|cell| cell.is_tool_groupable && !cell.is_empty);
    let first_line_in_group = !previous_is_tool && line_in_cell == 0;
    let last_line_in_group = !next_is_tool && line_in_cell + 1 == rendered_line_count;

    let rail = match (first_line_in_group, last_line_in_group) {
        (true, true) if rendered_line_count == 1 => {
            crate::tui::widgets::tool_card::CardRail::Single
        }
        (true, _) => crate::tui::widgets::tool_card::CardRail::Top,
        (_, true) => crate::tui::widgets::tool_card::CardRail::Bottom,
        _ => crate::tui::widgets::tool_card::CardRail::Middle,
    };
    Some(rail)
}

fn line_with_group_rail(
    line: &Line<'static>,
    rail: Option<crate::tui::widgets::tool_card::CardRail>,
    max_width: usize,
) -> Line<'static> {
    let Some(rail) = rail else {
        return line.clone();
    };
    let glyph = crate::tui::widgets::tool_card::rail_glyph(rail);
    if glyph.is_empty() {
        let mut rendered = line.clone();
        rendered.spans = truncate_spans_to_width(rendered.spans, max_width);
        return rendered;
    }

    let mut rendered = line.clone();
    let mut spans = Vec::with_capacity(rendered.spans.len() + 1);
    spans.push(Span::styled(
        format!("{glyph} "),
        Style::default().fg(crate::palette::TEXT_DIM),
    ));
    spans.extend(rendered.spans);
    rendered.spans = truncate_spans_to_width(spans, max_width);
    rendered
}

fn truncate_spans_to_width(spans: Vec<Span<'static>>, max_width: usize) -> Vec<Span<'static>> {
    if max_width == 0 || spans.is_empty() {
        return Vec::new();
    }
    let current_width: usize = spans
        .iter()
        .map(|span| unicode_width::UnicodeWidthStr::width(span.content.as_ref()))
        .sum();
    if current_width <= max_width {
        return spans;
    }

    let ellipsis = if max_width > 3 { "..." } else { "" };
    let content_budget = max_width.saturating_sub(ellipsis.len());
    let mut used = 0usize;
    let mut truncated = Vec::with_capacity(spans.len() + usize::from(!ellipsis.is_empty()));
    let mut last_style = Style::default();

    'outer: for span in spans {
        last_style = span.style;
        let mut content = String::new();
        for ch in span.content.chars() {
            let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + width > content_budget {
                break 'outer;
            }
            content.push(ch);
            used += width;
        }
        if !content.is_empty() {
            truncated.push(Span::styled(content, span.style));
        }
    }

    if !ellipsis.is_empty() {
        truncated.push(Span::styled(ellipsis.to_string(), last_style));
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::history::{ExecCell, ExecSource, HistoryCell, ToolCell, ToolStatus};

    fn plain_lines(cache: &TranscriptViewCache) -> Vec<String> {
        cache
            .lines()
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    fn user_cell(content: &str) -> HistoryCell {
        HistoryCell::User {
            content: content.to_string(),
        }
    }

    fn assistant_cell(content: &str, streaming: bool) -> HistoryCell {
        HistoryCell::Assistant {
            content: content.to_string(),
            streaming,
        }
    }

    fn exec_tool_cell(command: &str) -> HistoryCell {
        HistoryCell::Tool(ToolCell::Exec(ExecCell {
            command: command.to_string(),
            status: ToolStatus::Running,
            output: None,
            started_at: None,
            duration_ms: None,
            source: ExecSource::Assistant,
            interaction: None,
        }))
    }

    #[test]
    fn cache_reuses_cells_when_revision_unchanged() {
        let cells = vec![
            user_cell("hello"),
            assistant_cell("world", false),
            user_cell("again"),
        ];
        let revisions = vec![1u64, 1, 1];

        let mut cache = TranscriptViewCache::new();
        cache.ensure(&cells, &revisions, 80, TranscriptRenderOptions::default());
        let first_lines: Vec<String> = cache
            .lines()
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        let first_total = cache.total_lines();
        assert!(first_total > 0, "期望非空渲染");

        // 捕获每个单元格的行快照以验证重用。
        let snapshot_per_cell: Vec<Vec<String>> = cache
            .per_cell
            .iter()
            .map(|c| {
                c.lines
                    .iter()
                    .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
                    .collect()
            })
            .collect();

        // 相同修订版本 => 一切重用，输出相同。
        cache.ensure(&cells, &revisions, 80, TranscriptRenderOptions::default());
        let second_lines: Vec<String> = cache
            .lines()
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(first_lines, second_lines);
        assert_eq!(cache.total_lines(), first_total);

        let snapshot_per_cell_2: Vec<Vec<String>> = cache
            .per_cell
            .iter()
            .map(|c| {
                c.lines
                    .iter()
                    .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
                    .collect()
            })
            .collect();
        assert_eq!(snapshot_per_cell, snapshot_per_cell_2);
    }

    #[test]
    fn bumping_one_cell_revision_only_rerenders_that_cell() {
        // 使用自定义 HistoryCell 包装器跟踪每个单元格的渲染次数
        // 需要 trait 更改；相反，我们通过检查 CachedCell 实例来检测重用。
        // 修订版本增加后，只有被修改的单元格的存储修订版本应与之前不同；
        // 其他单元格保持不变。

        let cells_v1 = vec![
            user_cell("hello"),
            assistant_cell("hi", true),
            user_cell("again"),
        ];
        let revs_v1 = vec![1u64, 1, 1];

        let mut cache = TranscriptViewCache::new();
        cache.ensure(&cells_v1, &revs_v1, 80, TranscriptRenderOptions::default());

        // 快照单元格 0 和 2 的缓存行（在差异中保持不变）。
        let cell0_lines_before = cache.per_cell[0]
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let cell2_lines_before = cache.per_cell[2]
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        // 修改单元格 1（助手流式增量）并仅增加其修订版本。
        let cells_v2 = vec![
            user_cell("hello"),
            assistant_cell("hi world", true),
            user_cell("again"),
        ];
        let revs_v2 = vec![1u64, 2, 1];

        cache.ensure(&cells_v2, &revs_v2, 80, TranscriptRenderOptions::default());

        // 单元格 0 和 2 字节相同（证明重用路径没有损坏数据）。
        let cell0_lines_after = cache.per_cell[0]
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let cell2_lines_after = cache.per_cell[2]
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert_eq!(cell0_lines_before, cell0_lines_after);
        assert_eq!(cell2_lines_before, cell2_lines_after);

        // 单元格 1 反映新内容。
        // 渲染器交错插入角色/空白跨度，因此拼接后的
        // 内容有内部填充（例如 "Assistant   hi   world"）。
        // 分别检查新令牌，而不是字面检查 "hi world" 子字符串。
        let cell1_after: String = cache.per_cell[1]
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            cell1_after.contains("hi") && cell1_after.contains("world"),
            "单元格 1 应使用新内容重新渲染；得到：{cell1_after}"
        );

        // 缓存中的修订版本反映了增量。
        assert_eq!(cache.per_cell[0].revision, 1);
        assert_eq!(cache.per_cell[1].revision, 2);
        assert_eq!(cache.per_cell[2].revision, 1);
    }

    #[test]
    fn tail_update_suffix_rebuild_matches_fresh_flatten() {
        let mut cells = vec![
            user_cell("first message"),
            assistant_cell("stable answer", false),
            user_cell("tail prompt"),
        ];
        let mut revisions = vec![1u64, 1, 1];
        let mut cache = TranscriptViewCache::new();
        cache.ensure(&cells, &revisions, 40, TranscriptRenderOptions::default());

        cells.push(assistant_cell("streaming tail", true));
        revisions.push(1);
        cache.ensure(&cells, &revisions, 40, TranscriptRenderOptions::default());

        if let HistoryCell::Assistant { content, .. } = cells.last_mut().unwrap() {
            content.push_str(" plus delta");
        }
        *revisions.last_mut().unwrap() += 1;
        cache.ensure(&cells, &revisions, 40, TranscriptRenderOptions::default());
        let incremental = plain_lines(&cache);

        let mut fresh = TranscriptViewCache::new();
        fresh.ensure(&cells, &revisions, 40, TranscriptRenderOptions::default());
        assert_eq!(incremental, plain_lines(&fresh));
    }

    #[test]
    fn width_change_rerenders_all_cells() {
        let cells = vec![
            user_cell("a fairly long message that may wrap at narrow widths"),
            assistant_cell("another long message body content", false),
        ];
        let revisions = vec![5u64, 7];

        let mut cache = TranscriptViewCache::new();
        cache.ensure(&cells, &revisions, 80, TranscriptRenderOptions::default());
        let wide_total = cache.total_lines();

        // 较窄的宽度应改变布局 — 所有内容重新渲染。
        cache.ensure(&cells, &revisions, 20, TranscriptRenderOptions::default());
        let narrow_total = cache.total_lines();

        assert_ne!(
            wide_total, narrow_total,
            "较窄宽度应产生不同数量的行"
        );

        // 恢复原始宽度会再次重新渲染。
        cache.ensure(&cells, &revisions, 80, TranscriptRenderOptions::default());
        assert_eq!(cache.total_lines(), wide_total);
    }

    #[test]
    fn streaming_assistant_only_rebuilds_one_cell_render_count() {
        // 验证行为 6：当一个 Assistant 单元格流式传输增量时，
        // 只有该单元格被重新渲染。我们使用连接到自定义 History 设置的
        // 计数包装器。由于 `lines_with_options` 在 `HistoryCell`（具体枚举）上，
        // 我们无法直接模拟它。相反，我们验证缓存的不变条件：
        // 具有未更改修订版本的单元格保留其先前的 CachedCell 条目（克隆相等），
        // 证明它们没有发生重新渲染。
        //
        // 我们通过将修订版本存储为单调递增的 u64 并验证
        // `per_cell.revision` 的 `Vec<u64>` 快照仅在递增的索引处不同来实现。

        let mut cells: Vec<HistoryCell> =
            (0..50).map(|i| user_cell(&format!("cell {i}"))).collect();
        cells.push(assistant_cell("streaming", true));
        let mut revisions: Vec<u64> = vec![1; 51];

        let mut cache = TranscriptViewCache::new();
        cache.ensure(&cells, &revisions, 80, TranscriptRenderOptions::default());

        // 快照单元格 0..50 渲染的总字节数（不变）。
        let stable_snapshot: Vec<String> = cache.per_cell[..50]
            .iter()
            .map(|c| {
                c.lines
                    .iter()
                    .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect();

        // 向助手单元格流式传输 10 个增量，仅增加其修订版本。
        for i in 0..10 {
            if let HistoryCell::Assistant { content, .. } = &mut cells[50] {
                content.push_str(&format!(" delta-{i}"));
            }
            revisions[50] += 1;
            cache.ensure(&cells, &revisions, 80, TranscriptRenderOptions::default());

            // 在每个增量之后，单元格 0..50 必须与初始渲染字节相同。
            // 如果我们重新渲染了它们，我们仍然会观察到相同的字节（确定性），
            // 但测试还会检查 CachedCell.revision 值保持在 1 —
            // 这意味着缓存从未替换它们，只是重用了它们。
            let stable_now: Vec<String> = cache.per_cell[..50]
                .iter()
                .map(|c| {
                    c.lines
                        .iter()
                        .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect();
            assert_eq!(
                stable_now, stable_snapshot,
                "稳定单元格在 delta {i} 处出现分歧"
            );

            for (idx, c) in cache.per_cell[..50].iter().enumerate() {
                assert_eq!(
                    c.revision, 1,
                    "单元格 {idx} 的修订版本在流式增量期间发生更改"
                );
            }
        }
    }

    #[test]
    fn missing_revisions_falls_back_to_full_render() {
        // 如果调用方传递长度错误的 `cell_revisions` 切片
        //（不应发生，但为防御性编程），缓存仍应
        // 产生正确的输出，而不是 panic 或跳过单元格。
        let cells = vec![user_cell("a"), assistant_cell("b", false)];
        let bogus_revisions = vec![1u64]; // wrong length

        let mut cache = TranscriptViewCache::new();
        cache.ensure(
            &cells,
            &bogus_revisions,
            80,
            TranscriptRenderOptions::default(),
        );

        // 两个单元格都被渲染了（无 panic，输出非空）。
        assert_eq!(cache.per_cell.len(), 2);
        assert!(!cache.lines().is_empty());
    }

    #[test]
    fn adjacent_tool_cells_render_as_one_railed_group() {
        let cells = vec![exec_tool_cell("cargo test"), exec_tool_cell("cargo clippy")];
        let revisions = vec![1u64, 1];
        let mut cache = TranscriptViewCache::new();

        cache.ensure(&cells, &revisions, 80, TranscriptRenderOptions::default());
        let lines = plain_lines(&cache);

        assert!(
            lines
                .first()
                .is_some_and(|line| line.starts_with("\u{256D} ")),
            "第一个工具行应打开共享导轨：{lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.starts_with("\u{2502} ")),
            "中间工具行应继续共享导轨：{lines:?}"
        );
        assert!(
            lines
                .last()
                .is_some_and(|line| line.starts_with("\u{2570} ")),
            "最后一个工具行应关闭共享导轨：{lines:?}"
        );
        assert!(
            !lines.iter().any(String::is_empty),
            "相邻的工具单元格不应被空白分隔行隔开：{lines:?}"
        );
    }

    #[test]
    fn tool_rails_preserve_rendered_width_budget() {
        let cells = vec![exec_tool_cell(
            "printf 'this is a command with enough text to wrap in narrow terminals'",
        )];
        let revisions = vec![1u64];
        let mut cache = TranscriptViewCache::new();

        cache.ensure(&cells, &revisions, 24, TranscriptRenderOptions::default());

        for line in plain_lines(&cache) {
            assert!(
                unicode_width::UnicodeWidthStr::width(line.as_str()) <= 24,
                "工具导轨行超出了窄宽度限制：{line:?}"
            );
        }
    }
}
