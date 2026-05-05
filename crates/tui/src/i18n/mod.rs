//! AI-powered dynamic internationalization (i18n) system.
//!
//! This module implements a dual-file fallback mechanism:
//! - `i18n.json`: Target language translations (AI-generated)
//! - `en.json`: English original (fallback when keys are missing)
//!
//! # Architecture
//!
//! ```text
//! ~/.deepseek/i18n/
//! ├── i18n.json          # Current target language
//! ├── en.json            # English source (auto-generated)
//! └── cache/             # Translation cache
//!     └── <hash>.json
//! ```
//!
//! # Usage
//!
//! ```rust
//! use crate::i18n::I18nManager;
//!
//! let manager = I18nManager::new()?;
//! let text = manager.get("composer_placeholder");
//! // Returns i18n.json value, or falls back to en.json
//! ```

pub mod manager;
pub mod fallback;

pub use manager::I18nManager;
