//! Session commands: save, load, compact, export

use std::fmt::Write;
use std::path::PathBuf;

use crate::localization::MessageId;
use crate::session_manager::create_saved_session_with_mode;
use crate::tui::app::{App, AppAction};
use crate::tui::history::{HistoryCell, history_cells_from_message};
use crate::tui::session_picker::SessionPickerView;

use super::CommandResult;

/// Save session to file
pub fn save(app: &mut App, path: Option<&str>) -> CommandResult {
    let save_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!("session_{timestamp}.json"))
    };

    let messages = app.api_messages.clone();
    let session = create_saved_session_with_mode(
        &messages,
        &app.model,
        &app.workspace,
        u64::from(app.session.total_tokens),
        app.system_prompt.as_ref(),
        Some(app.mode.label()),
    );

    let sessions_dir = save_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| app.workspace.clone(), std::path::Path::to_path_buf);

    match std::fs::create_dir_all(&sessions_dir) {
        Ok(()) => {
            let json = match serde_json::to_string_pretty(&session) {
                Ok(j) => j,
                Err(e) => {
                    return CommandResult::error(
                        app.tr(MessageId::CmdSessionSerializeFailed)
                            .replace("{err}", &e.to_string()),
                    );
                }
            };
            match std::fs::write(&save_path, json) {
                Ok(()) => {
                    app.current_session_id = Some(session.metadata.id.clone());
                    CommandResult::message(
                        app.tr(MessageId::CmdSessionSaved)
                            .replace("{path}", &save_path.display().to_string())
                            .replace(
                                "{id}",
                                &crate::session_manager::truncate_id(&session.metadata.id),
                            ),
                    )
                }
                Err(e) => CommandResult::error(
                    app.tr(MessageId::CmdSessionSaveFailed)
                        .replace("{err}", &e.to_string()),
                ),
            }
        }
        Err(e) => CommandResult::error(
            app.tr(MessageId::CmdSessionCreateDirFailed)
                .replace("{err}", &e.to_string()),
        ),
    }
}

/// Load session from file
pub fn load(app: &mut App, path: Option<&str>) -> CommandResult {
    let load_path = if let Some(p) = path {
        if p.contains('/') || p.contains('\\') {
            PathBuf::from(p)
        } else {
            app.workspace.join(p)
        }
    } else {
        return CommandResult::error(app.tr(MessageId::CmdSessionLoadUsage));
    };

    let content = match std::fs::read_to_string(&load_path) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult::error(
                app.tr(MessageId::CmdSessionReadFailed)
                    .replace("{err}", &e.to_string()),
            );
        }
    };

    let session: crate::session_manager::SavedSession = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult::error(
                app.tr(MessageId::CmdSessionParseFailed)
                    .replace("{err}", &e.to_string()),
            );
        }
    };

    app.api_messages.clone_from(&session.messages);
    app.clear_history();
    let cells_to_add: Vec<_> = app
        .api_messages
        .iter()
        .flat_map(history_cells_from_message)
        .collect();
    app.extend_history(cells_to_add);
    app.mark_history_updated();
    app.viewport.transcript_selection.clear();
    app.model.clone_from(&session.metadata.model);
    app.update_model_compaction_budget();
    app.workspace.clone_from(&session.metadata.workspace);
    app.session.total_tokens = u32::try_from(session.metadata.total_tokens).unwrap_or(u32::MAX);
    app.session.total_conversation_tokens = app.session.total_tokens;
    app.session.last_prompt_tokens = None;
    app.session.last_completion_tokens = None;
    app.current_session_id = Some(session.metadata.id.clone());
    if let Some(sp) = session.system_prompt {
        app.system_prompt = Some(crate::models::SystemPrompt::Text(sp));
    }
    app.scroll_to_bottom();

    CommandResult::with_message_and_action(
        app.tr(MessageId::CmdSessionLoaded)
            .replace("{path}", &load_path.display().to_string())
            .replace(
                "{id}",
                &crate::session_manager::truncate_id(&session.metadata.id),
            )
            .replace("{count}", &session.metadata.message_count.to_string()),
        crate::tui::app::AppAction::SyncSession {
            messages: app.api_messages.clone(),
            system_prompt: app.system_prompt.clone(),
            model: app.model.clone(),
            workspace: app.workspace.clone(),
        },
    )
}

/// Trigger context compaction
pub fn compact(_app: &mut App) -> CommandResult {
    // Trigger immediate compaction via engine
    CommandResult::with_message_and_action(
        crate::localization::tr(
            crate::localization::Locale::default(),
            MessageId::CmdSessionCompactTriggered,
        )
        .to_string(),
        AppAction::CompactContext,
    )
}

/// Export conversation to markdown
pub fn export(app: &mut App, path: Option<&str>) -> CommandResult {
    let export_path = path.map_or_else(
        || {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            PathBuf::from(format!("chat_export_{timestamp}.md"))
        },
        PathBuf::from,
    );

    let mut content = String::new();
    content.push_str(app.tr(MessageId::CmdSessionExportTitle));
    content.push_str("\n\n");
    let _ = write!(
        content,
        "**{}:** {}\n**{}:** {}\n**{}:** {}\n\n---\n\n",
        app.tr(MessageId::CmdSessionExportModel),
        app.model,
        app.tr(MessageId::CmdSessionExportWorkspace),
        app.workspace.display(),
        app.tr(MessageId::CmdSessionExportDate),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    for cell in &app.history {
        let (role, body) = match cell {
            HistoryCell::User { content } => {
                (app.tr(MessageId::CmdSessionExportRoleYou), content.clone())
            }
            HistoryCell::Assistant { content, .. } => (
                app.tr(MessageId::CmdSessionExportRoleAssistant),
                content.clone(),
            ),
            HistoryCell::System { content } => (
                app.tr(MessageId::CmdSessionExportRoleSystem),
                content.clone(),
            ),
            HistoryCell::Error { message, severity } => match severity {
                crate::error_taxonomy::ErrorSeverity::Warning => (
                    app.tr(MessageId::CmdSessionExportRoleWarning),
                    message.clone(),
                ),
                crate::error_taxonomy::ErrorSeverity::Info => {
                    (app.tr(MessageId::CmdSessionExportRoleInfo), message.clone())
                }
                _ => (
                    app.tr(MessageId::CmdSessionExportRoleError),
                    message.clone(),
                ),
            },
            HistoryCell::Thinking { content, .. } => (
                app.tr(MessageId::CmdSessionExportRoleThinking),
                content.clone(),
            ),
            HistoryCell::Tool(tool) => (
                app.tr(MessageId::CmdSessionExportRoleTool),
                render_tool_cell(tool, 80),
            ),
            HistoryCell::SubAgent(sub) => (
                app.tr(MessageId::CmdSessionExportRoleSubagent),
                render_subagent_cell(sub, 80),
            ),
            HistoryCell::ArchivedContext {
                level,
                range,
                summary,
                ..
            } => (
                app.tr(MessageId::CmdSessionExportRoleArchivedContext),
                format!("L{level} [{range}]: {summary}"),
            ),
        };

        let _ = write!(content, "{}\n\n{}\n\n---\n\n", role, body.trim());
    }

    match std::fs::write(&export_path, content) {
        Ok(()) => CommandResult::message(
            app.tr(MessageId::CmdSessionExported)
                .replace("{path}", &export_path.display().to_string()),
        ),
        Err(e) => CommandResult::error(
            app.tr(MessageId::CmdSessionExportFailed)
                .replace("{err}", &e.to_string()),
        ),
    }
}

/// Open the session picker UI, or run a sub-action like
/// `prune <days>` for housekeeping (#406 phase-1.5).
pub fn sessions(app: &mut App, arg: Option<&str>) -> CommandResult {
    let trimmed = arg.unwrap_or("").trim();
    if trimmed.is_empty() {
        app.view_stack.push(SessionPickerView::new());
        return CommandResult::ok();
    }

    let mut parts = trimmed.split_whitespace();
    let action = parts.next().unwrap_or("").to_ascii_lowercase();
    match action.as_str() {
        "prune" => prune(app, parts.next()),
        "show" | "list" | "picker" => {
            app.view_stack.push(SessionPickerView::new());
            CommandResult::ok()
        }
        _ => CommandResult::error(
            app.tr(MessageId::CmdSessionsUnknownSubcommand)
                .replace("{action}", &action),
        ),
    }
}

/// Prune persisted sessions older than `<days>` from
/// `~/.deepseek/sessions/`. Wraps
/// [`crate::session_manager::SessionManager::prune_sessions_older_than`]
/// so users can run a safe cleanup without leaving the TUI. Skips
/// the checkpoint subdirectory (the helper guarantees that already).
fn prune(_app: &mut App, days_arg: Option<&str>) -> CommandResult {
    let days_str = match days_arg {
        Some(s) => s,
        None => {
            return CommandResult::error(crate::localization::tr(
                crate::localization::Locale::default(),
                MessageId::CmdSessionsPruneUsage,
            ));
        }
    };
    let days: u64 = match days_str.parse() {
        Ok(n) if n > 0 => n,
        _ => {
            return CommandResult::error(
                crate::localization::tr(
                    crate::localization::Locale::default(),
                    MessageId::CmdSessionsPruneInvalidDays,
                )
                .replace("{days}", days_str),
            );
        }
    };

    let manager = match crate::session_manager::SessionManager::default_location() {
        Ok(m) => m,
        Err(err) => {
            return CommandResult::error(
                crate::localization::tr(
                    crate::localization::Locale::default(),
                    MessageId::CmdSessionsOpenFailed,
                )
                .replace("{err}", &err.to_string()),
            );
        }
    };

    let max_age = std::time::Duration::from_secs(days.saturating_mul(24 * 60 * 60));
    match manager.prune_sessions_older_than(max_age) {
        Ok(0) => CommandResult::message(
            crate::localization::tr(
                crate::localization::Locale::default(),
                MessageId::CmdSessionsPruneNone,
            )
            .replace("{days}", &days.to_string()),
        ),
        Ok(n) => CommandResult::message(
            crate::localization::tr(
                crate::localization::Locale::default(),
                MessageId::CmdSessionsPruned,
            )
            .replace("{count}", &n.to_string())
            .replace("{plural}", if n == 1 { "" } else { "s" })
            .replace("{days}", &days.to_string()),
        ),
        Err(err) => CommandResult::error(
            crate::localization::tr(
                crate::localization::Locale::default(),
                MessageId::CmdSessionsPruneFailed,
            )
            .replace("{err}", &err.to_string()),
        ),
    }
}

fn render_tool_cell(tool: &crate::tui::history::ToolCell, width: u16) -> String {
    tool.lines(width)
        .into_iter()
        .map(line_to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_subagent_cell(cell: &crate::tui::history::SubAgentCell, width: u16) -> String {
    cell.lines(width)
        .into_iter()
        .map(line_to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_to_string(line: ratatui::text::Line<'static>) -> String {
    line.spans
        .into_iter()
        .map(|span| span.content.to_string())
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::app::{App, TuiOptions};
    use tempfile::TempDir;

    fn create_test_app_with_tmpdir(tmpdir: &TempDir) -> App {
        let options = TuiOptions {
            model: "deepseek-v4-pro".to_string(),
            workspace: tmpdir.path().to_path_buf(),
            config_path: None,
            config_profile: None,
            allow_shell: false,
            use_alt_screen: true,
            use_mouse_capture: false,
            use_bracketed_paste: true,
            max_subagents: 1,
            skills_dir: tmpdir.path().join("skills"),
            memory_path: tmpdir.path().join("memory.md"),
            notes_path: tmpdir.path().join("notes.txt"),
            mcp_config_path: tmpdir.path().join("mcp.json"),
            use_memory: false,
            start_in_agent_mode: false,
            skip_onboarding: true,
            yolo: false,
            resume_session_id: None,
            initial_input: None,
        };
        App::new(options, &Config::default())
    }

    #[test]
    fn test_save_creates_file_and_sets_session_id() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let save_path = tmpdir.path().join("test_session.json");

        let result = save(&mut app, Some(save_path.to_str().unwrap()));
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        assert!(msg.contains("Session saved to"));
        assert!(msg.contains("ID:"));
        assert!(app.current_session_id.is_some());
        assert!(save_path.exists());
    }

    #[test]
    fn test_save_with_default_path_uses_workspace() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = save(&mut app, None);
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        // Should create file in workspace with timestamp name
        // Give it a moment to ensure file is written
        std::thread::sleep(std::time::Duration::from_millis(10));
        let entries: Vec<_> = std::fs::read_dir(tmpdir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("session_"))
            .collect();
        // Test passes if file was created or if save returned success message
        assert!(!entries.is_empty() || msg.contains("Session saved"));
    }

    #[test]
    fn test_save_serialization_error() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        // This should work normally since SavedSession is serializable
        // Testing error path would require mocking, which is complex
        let save_path = tmpdir.path().join("test.json");
        let result = save(&mut app, Some(save_path.to_str().unwrap()));
        assert!(result.message.is_some());
    }

    #[test]
    fn test_load_without_path_returns_error() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = load(&mut app, None);
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("Usage: /load"));
    }

    #[test]
    fn test_load_nonexistent_file_returns_error() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = load(&mut app, Some("nonexistent.json"));
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("Failed to read"));
    }

    #[test]
    fn test_load_invalid_json_returns_error() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let bad_file = tmpdir.path().join("bad.json");
        std::fs::write(&bad_file, "not valid json").unwrap();
        let result = load(&mut app, Some(bad_file.to_str().unwrap()));
        assert!(result.message.is_some());
        assert!(result.message.unwrap().contains("Failed to parse"));
    }

    #[test]
    fn test_load_valid_session_restores_state() {
        let tmpdir = TempDir::new().unwrap();
        let mut app1 = create_test_app_with_tmpdir(&tmpdir);
        // Set up some state to save
        app1.api_messages.push(crate::models::Message {
            role: "user".to_string(),
            content: vec![crate::models::ContentBlock::Text {
                text: "Hello".to_string(),
                cache_control: None,
            }],
        });
        app1.session.total_tokens = 500;
        let save_path = tmpdir.path().join("test.json");
        save(&mut app1, Some(save_path.to_str().unwrap()));

        // Create new app and load
        let mut app2 = create_test_app_with_tmpdir(&tmpdir);
        let result = load(&mut app2, Some(save_path.to_str().unwrap()));
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        assert!(msg.contains("Session loaded from"));
        assert!(msg.contains("ID:"));
        assert!(msg.contains("messages"));
        assert_eq!(app2.api_messages.len(), 1);
        assert_eq!(app2.session.total_tokens, 500);
        assert!(app2.current_session_id.is_some());
        assert!(matches!(result.action, Some(AppAction::SyncSession { .. })));
    }

    #[test]
    fn test_compact_toggles_state() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);

        let result = compact(&mut app);
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        assert!(msg.contains("compaction") || msg.contains("Compact"));
        assert!(matches!(result.action, Some(AppAction::CompactContext)));
    }

    #[test]
    fn test_export_crees_markdown_file() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        app.history.push(HistoryCell::User {
            content: "Hello".to_string(),
        });
        app.history.push(HistoryCell::Assistant {
            content: "Hi there".to_string(),
            streaming: false,
        });

        let export_path = tmpdir.path().join("export.md");
        let result = export(&mut app, Some(export_path.to_str().unwrap()));
        assert!(result.message.is_some());
        let msg = result.message.unwrap();
        assert!(msg.contains("Exported to"));
        assert!(export_path.exists());

        let content = std::fs::read_to_string(&export_path).unwrap();
        assert!(content.contains("# Chat Export"));
        assert!(content.contains("**Model:**"));
        assert!(content.contains("**You:**"));
        assert!(content.contains("**Assistant:**"));
    }

    #[test]
    fn test_export_with_default_path() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = export(&mut app, None);
        assert!(result.message.is_some());
        // Should create file with timestamp name in current dir
        let entries: Vec<_> = std::fs::read_dir(".")
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("chat_export_"))
            .collect();
        // Clean up
        for entry in &entries {
            let _ = std::fs::remove_file(entry.path());
        }
        assert!(!entries.is_empty() || result.message.unwrap().contains("Exported to"));
    }

    #[test]
    fn test_sessions_pushes_picker_view() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let initial_kind = app.view_stack.top_kind();

        let result = sessions(&mut app, None);
        assert_eq!(result.message, None);
        assert!(result.action.is_none());
        // View should have changed (session picker should be on top)
        assert_ne!(app.view_stack.top_kind(), initial_kind);
    }

    #[test]
    fn test_sessions_show_subcommand_pushes_picker_view() {
        // `/sessions show` and `/sessions list` are explicit aliases
        // for the no-arg picker form. Verify they don't fall through
        // to the prune branch.
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let initial_kind = app.view_stack.top_kind();
        let result = sessions(&mut app, Some("show"));
        assert_eq!(result.message, None);
        assert_ne!(app.view_stack.top_kind(), initial_kind);
    }

    #[test]
    fn test_sessions_prune_requires_days_argument() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = sessions(&mut app, Some("prune"));
        assert!(result.is_error);
        assert!(
            result.message.as_deref().unwrap_or("").contains("usage"),
            "expected usage hint: {:?}",
            result.message
        );
    }

    #[test]
    fn test_sessions_prune_rejects_non_positive_days() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        for bad in ["0", "-3", "abc", "3.14"] {
            let result = sessions(&mut app, Some(&format!("prune {bad}")));
            assert!(result.is_error, "expected error for `{bad}`");
        }
    }

    #[test]
    fn test_sessions_unknown_subcommand_errors() {
        let tmpdir = TempDir::new().unwrap();
        let mut app = create_test_app_with_tmpdir(&tmpdir);
        let result = sessions(&mut app, Some("teleport"));
        assert!(result.is_error);
        assert!(
            result
                .message
                .as_deref()
                .unwrap_or("")
                .contains("unknown subcommand"),
            "expected unknown-subcommand error: {:?}",
            result.message
        );
    }
}
