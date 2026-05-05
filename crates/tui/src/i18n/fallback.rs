//! Fallback logic for i18n — integrated with the central `tr()` in localization.rs.
//!
//! The main `tr()` function now handles the full fallback chain internally:
//! active locale JSON → embedded en.json → key name.
//! This module provides optional helpers for consumers that need explicit control.

use crate::i18n::I18nManager;
use crate::localization::{Locale, MessageId, tr};

/// Get a translation with explicit I18nManager fallback.
///
/// # Fallback Chain
/// 1. AI-translated i18n.json (target language) via I18nManager
/// 2. Standard `tr()` fallback (embedded en.json → key name)
pub fn tr_with_manager(manager: Option<&I18nManager>, locale: Locale, id: MessageId) -> String {
    let key = id.to_key();

    // Try AI i18n first via I18nManager
    if let Some(mgr) = manager {
        if let Some(text) = mgr.get(key) {
            return text;
        }
    }

    // Fall back to the standard tr() chain
    tr(locale, id).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_to_key_conversion() {
        assert_eq!(
            MessageId::ComposerPlaceholder.to_key(),
            "composer_placeholder"
        );
        assert_eq!(
            MessageId::HelpTitle.to_key(),
            "help_title"
        );
    }
}
