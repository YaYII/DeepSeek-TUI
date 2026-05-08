//! `/localize` slash command.

use crate::commands::CommandResult;
use crate::tui::app::App;

fn target_locale_for_localize(arg: Option<&str>) -> String {
    arg.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            crate::localization::resolve_locale("auto")
                .tag()
                .to_string()
        })
}

pub fn localize(app: &mut App, arg: Option<&str>) -> CommandResult {
    let target_locale = target_locale_for_localize(arg);
    let output_dir = crate::i18n_files::user_i18n_dir();

    match crate::i18n_files::scaffold_tui_localization_files(&output_dir, &target_locale) {
        Ok(report) => {
            // Switch to YOLO mode for auto-approval of file operations
            app.mode = crate::tui::app::AppMode::Yolo;
            
            // Generate translation request using API directly
            let prompt = generate_api_translation_request(&report, &target_locale);
            
            // Send the message directly with YOLO mode
            CommandResult::action(crate::tui::app::AppAction::SendMessage(prompt))
        }
        Err(err) => CommandResult::error(format!("Failed to scaffold localization files: {err}")),
    }
}

fn generate_api_translation_request(report: &crate::i18n_files::LocalizationScaffoldReport, target_locale: &str) -> String {
    // 使用相对于当前工作目录的路径
    let file_list = report.created_files.iter()
        .map(|p| {
            // 尝试获取相对于当前工作目录的路径
            if let Ok(cwd) = std::env::current_dir() {
                if let Ok(relative) = p.strip_prefix(&cwd) {
                    format!("- {}", relative.display())
                } else {
                    // 如果不在当前目录下，尝试查找项目根目录
                    // 向上搜索 .deepseek 目录
                    let mut search_dir = cwd.as_path();
                    let mut found_relative = None;
                    
                    loop {
                        if let Ok(rel) = p.strip_prefix(search_dir) {
                            found_relative = Some(rel.to_path_buf());
                            break;
                        }
                        
                        match search_dir.parent() {
                            Some(parent) => search_dir = parent,
                            None => break,
                        }
                    }
                    
                    if let Some(rel) = found_relative {
                        format!("- {}", rel.display())
                    } else {
                        // 最后回退到绝对路径
                        format!("- {}", p.display())
                    }
                }
            } else {
                // 如果无法获取当前目录，使用绝对路径
                format!("- {}", p.display())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    
    format!(
        "Please translate all i18n files to {} for DeepSeek TUI.\n\nFiles to translate:\n{}\n\nInstructions:\n1. Read each file using read_file tool with the path shown above\n2. Translate ALL content to {} (Simplified Chinese)\n3. For i18n.json: Translate all values in the \"translations\" object, keep keys unchanged\n4. For *.i18n.* files: Translate the entire content\n5. Maintain exact file structure and formatting\n6. Do NOT translate code blocks, variable names, or technical terms\n7. Keep placeholder syntax like {{key}} unchanged\n8. Write translated content back using write_file tool\n\nUse deepseek-v4-flash model for faster response.",
        target_locale,
        file_list,
        target_locale
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_locale_uses_explicit_argument_when_present() {
        assert_eq!(target_locale_for_localize(Some("zh-Hans")), "zh-Hans");
    }

    #[test]
    fn target_locale_defaults_to_system_locale_when_argument_missing() {
        let locale = target_locale_for_localize(None);
        assert!(!locale.is_empty());
    }
}
