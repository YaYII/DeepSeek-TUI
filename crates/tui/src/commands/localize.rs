//! `/localize` slash command.

use crate::commands::CommandResult;
use crate::tui::app::{App, AppAction};

pub fn localize(_app: &mut App, arg: Option<&str>) -> CommandResult {
    let Some(target_locale) = arg.map(str::trim).filter(|value| !value.is_empty()) else {
        return CommandResult::error("Usage: /localize <locale>, for example /localize zh-Hans");
    };

    match crate::i18n_files::generate_tui_localization_prompt(target_locale) {
        Ok(prompt) => CommandResult::action(AppAction::SendMessage(prompt)),
        Err(err) => CommandResult::error(format!("Failed to prepare localization task: {err}")),
    }
}
