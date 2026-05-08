//! 工具卡片 — 渲染工具调用状态和输出。
//!
//! 工具卡片是当代理运行 `read_file`、`exec_shell`、`apply_patch` 等时
//! 出现的方框。视觉词汇有意保持简洁：一个动词字形标识家族，一个左侧轨道
//! 将卡片锚定到时间线上，旋转动画频率（720 毫秒/步）复用了现有的工具状态动画。
//!
//! 此模块拥有：
//!
//! - [`ToolFamily`] — 七个规范家族加上一个 `Generic` 回退，
//!   用于尚未分配家族的任何内容。
//! - [`tool_family_for_title`] — 将旧的 `render_tool_header` 标题字符串
//!   （`"Shell"`、`"Patch"`、`"Workspace"` 等）映射到家族。允许
//!   现有调用点直接使用家族字形，无需重新架构每个单元。
//! - [`family_glyph`] / [`family_label`] — 每个家族的动词字形 + 标签。
//!   字形是单个字素；标签是简短动词。
//! - [`CardRail`] / [`rail_glyph`] — 锚定到左边距的 `╭ │ ╰` 轨道，
//!   便于视觉上对多行卡片进行分组。
//!
//! 实际的行组合仍在 `history.rs` 内部完成；此模块是词汇表，而非布局引擎。
//! 保持小巧意味着未来的视觉刷新只需修改此处的常量。

/// 工具家族 — 代理正在执行的动词。用于为卡片标题选择字形和标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFamily {
    /// 读取、列出、探索。`▷ read`。
    Read,
    /// 编辑、补丁、写入。`◆ patch`。
    Patch,
    /// Shell、子进程。`▶ run`。
    Run,
    /// Grep、模糊文件搜索、网络搜索。`⌕ find`。
    Find,
    /// 单个子代理分发。`◐ delegate`。
    Delegate,
    /// 多代理扇出分发（rlm）。`⋮⋮ fanout`。
    Fanout,
    /// 递归语言模型工作。`⋮⋮ rlm`。
    Rlm,
    /// 推理/思维链。`… think`。推理有自己的渲染路径
    ///（`history.rs` 中的 `render_thinking`）；此处声明该家族
    /// 是为了完整性，以便未来任何使用它的代码都有匹配的字形和标签词汇。
    #[allow(dead_code)]
    Think,
    /// 尚未拥有家族字形的任何内容 — 回退到中性圆点，以便卡片仍能干净渲染。
    Generic,
}

/// 将旧的工具标题字符串（传递给 `render_tool_header` 的值）映射到家族。
/// 无法识别的任何内容都会回退到 [`ToolFamily::Generic`]，
/// 以便卡片仍能渲染——它们只是失去了动词字形处理，直到在此处添加家族为止。
#[must_use]
pub fn tool_family_for_title(title: &str) -> ToolFamily {
    match title {
        "Shell" => ToolFamily::Run,
        "Patch" | "Diff" => ToolFamily::Patch,
        "Workspace" | "Image" => ToolFamily::Read,
        "Search" => ToolFamily::Find,
        "Plan" | "Review" => ToolFamily::Generic,
        _ => ToolFamily::Generic,
    }
}

/// 将任意工具名称（如暴露给模型的 — 例如 `read_file`、`apply_patch`、
/// `agent_spawn`）映射到家族。由 `GenericToolCell` 使用，
/// 因为每个通用单元都共享标题 `"Tool"`，所以 `tool_family_for_title`
/// 快捷方式不够用。
#[must_use]
pub fn tool_family_for_name(name: &str) -> ToolFamily {
    match name {
        "read_file" | "list_dir" | "view_image" => ToolFamily::Read,
        "edit_file" | "apply_patch" | "write_file" => ToolFamily::Patch,
        "exec_shell" | "exec_shell_wait" | "exec_shell_interact" => ToolFamily::Run,
        "grep_files" | "file_search" | "web_search" | "fetch_url" => ToolFamily::Find,
        "agent_spawn" => ToolFamily::Delegate,
        "rlm" => ToolFamily::Rlm,
        _ => ToolFamily::Generic,
    }
}

/// 从公开工具名称和已清理的参数摘要构建紧凑的语义摘要。
#[must_use]
pub fn tool_header_summary_for_name(name: &str, input_summary: Option<&str>) -> Option<String> {
    let summary = input_summary?.trim();
    if summary.is_empty() {
        return None;
    }

    let preferred_keys = match tool_family_for_name(name) {
        ToolFamily::Read | ToolFamily::Patch => ["path", "file", "target", "content"].as_slice(),
        ToolFamily::Run => ["command", "cmd", "script"].as_slice(),
        ToolFamily::Find => ["query", "pattern", "path", "scope"].as_slice(),
        ToolFamily::Delegate | ToolFamily::Fanout | ToolFamily::Rlm => {
            ["prompt", "task", "model"].as_slice()
        }
        ToolFamily::Think | ToolFamily::Generic => {
            ["query", "path", "command", "prompt"].as_slice()
        }
    };

    for key in preferred_keys {
        if let Some(value) = summary_value(summary, key) {
            return Some(value);
        }
    }

    Some(summary.to_string())
}

fn summary_value(summary: &str, key: &str) -> Option<String> {
    for part in summary.split(", ") {
        let Some((part_key, value)) = part.split_once(':') else {
            continue;
        };
        if part_key.trim() == key {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// 家族的字形。单个字素，以便 `render_tool_header` 中的标题布局计算保持简单（一个单元宽）。
#[must_use]
pub fn family_glyph(family: ToolFamily) -> &'static str {
    match family {
        ToolFamily::Read => "\u{25B7}",           // ▷
        ToolFamily::Patch => "\u{25C6}",          // ◆
        ToolFamily::Run => "\u{25B6}",            // ▶
        ToolFamily::Find => "\u{2315}",           // ⌕
        ToolFamily::Delegate => "\u{25D0}",       // ◐
        ToolFamily::Fanout => "\u{22EE}\u{22EE}", // ⋮⋮ (two cells)
        ToolFamily::Rlm => "\u{22EE}\u{22EE}",    // ⋮⋮ (two cells)
        ToolFamily::Think => "\u{2026}",          // …
        ToolFamily::Generic => "\u{2022}",        // •
    }
}

/// 家族的简短动词标签 — 出现在卡片标题中字形旁边。
/// 故意使用小写；动词字形 + 标签是新的卡片标题词汇。
#[must_use]
pub fn family_label(family: ToolFamily) -> &'static str {
    match family {
        ToolFamily::Read => "read",
        ToolFamily::Patch => "patch",
        ToolFamily::Run => "run",
        ToolFamily::Find => "find",
        ToolFamily::Delegate => "delegate",
        ToolFamily::Fanout => "fanout",
        ToolFamily::Rlm => "rlm",
        ToolFamily::Think => "think",
        ToolFamily::Generic => "tool",
    }
}

/// 多行卡片中的行位置 — 驱动左侧轨道字形，使方框从上到下读作连续组。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // wired by future card-refactor follow-ups
pub enum CardRail {
    /// 卡片的第一行 — 标题。`╭`。
    Top,
    /// 任何中间行 — 正文内容。`│`。
    Middle,
    /// 卡片的最后一行。`╰`。
    Bottom,
    /// 单行卡片 — 完全没有轨道。
    Single,
}

/// 将 [`CardRail`] 位置映射到其轨道字形。以 `&str` 返回，
/// 因为调用方将其粘贴到 span 中。
#[must_use]
#[allow(dead_code)] // wired by future card-refactor follow-ups
pub fn rail_glyph(rail: CardRail) -> &'static str {
    match rail {
        CardRail::Top => "\u{256D}",    // ╭
        CardRail::Middle => "\u{2502}", // │
        CardRail::Bottom => "\u{2570}", // ╰
        CardRail::Single => "",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CardRail, ToolFamily, family_glyph, family_label, rail_glyph, tool_family_for_name,
        tool_family_for_title, tool_header_summary_for_name,
    };

    #[test]
    fn legacy_titles_route_to_expected_families() {
        assert_eq!(tool_family_for_title("Shell"), ToolFamily::Run);
        assert_eq!(tool_family_for_title("Patch"), ToolFamily::Patch);
        assert_eq!(tool_family_for_title("Workspace"), ToolFamily::Read);
        assert_eq!(tool_family_for_title("Search"), ToolFamily::Find);
        assert_eq!(tool_family_for_title("Diff"), ToolFamily::Patch);
        assert_eq!(tool_family_for_title("Plan"), ToolFamily::Generic);
        assert_eq!(tool_family_for_title("unknown title"), ToolFamily::Generic);
    }

    #[test]
    fn tool_names_route_to_families_by_verb() {
        assert_eq!(tool_family_for_name("read_file"), ToolFamily::Read);
        assert_eq!(tool_family_for_name("apply_patch"), ToolFamily::Patch);
        assert_eq!(tool_family_for_name("exec_shell"), ToolFamily::Run);
        assert_eq!(tool_family_for_name("grep_files"), ToolFamily::Find);
        assert_eq!(tool_family_for_name("agent_spawn"), ToolFamily::Delegate);
        assert_eq!(tool_family_for_name("rlm"), ToolFamily::Rlm);
        assert_eq!(
            tool_family_for_name("totally_new_tool"),
            ToolFamily::Generic
        );
    }

    #[test]
    fn tool_header_summary_prefers_family_specific_arguments() {
        assert_eq!(
            tool_header_summary_for_name("read_file", Some("path: src/main.rs, limit: 20"))
                .as_deref(),
            Some("src/main.rs")
        );
        assert_eq!(
            tool_header_summary_for_name("exec_shell", Some("command: cargo test, cwd: /repo"))
                .as_deref(),
            Some("cargo test")
        );
        assert_eq!(
            tool_header_summary_for_name("grep_files", Some("pattern: TODO, path: crates"))
                .as_deref(),
            Some("TODO")
        );
        assert_eq!(
            tool_header_summary_for_name("unknown", Some("alpha: beta")).as_deref(),
            Some("alpha: beta")
        );
    }

    #[test]
    fn each_family_has_a_glyph_and_label() {
        // Smoke test — surface accidental empties from a future refactor.
        for family in [
            ToolFamily::Read,
            ToolFamily::Patch,
            ToolFamily::Run,
            ToolFamily::Find,
            ToolFamily::Delegate,
            ToolFamily::Fanout,
            ToolFamily::Rlm,
            ToolFamily::Think,
            ToolFamily::Generic,
        ] {
            assert!(
                !family_glyph(family).is_empty(),
                "family {family:?} has empty glyph",
            );
            assert!(
                !family_label(family).is_empty(),
                "family {family:?} has empty label",
            );
        }
    }

    #[test]
    fn card_rail_glyphs_form_a_box() {
        assert_eq!(rail_glyph(CardRail::Top), "\u{256D}");
        assert_eq!(rail_glyph(CardRail::Middle), "\u{2502}");
        assert_eq!(rail_glyph(CardRail::Bottom), "\u{2570}");
        assert!(rail_glyph(CardRail::Single).is_empty());
    }
}
