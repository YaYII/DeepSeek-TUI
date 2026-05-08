//! Runtime i18n file discovery and local localization file scaffolding.
//!
//! The application never calls a hidden translation API from here. `/localize`
//! writes readable target files to the local i18n directory so they can be
//! translated later by DeepSeek or any other tool.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// i18n configuration structure matching `en.json` / `i18n.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I18nConfig {
    pub version: String,
    pub language: String,
    pub language_name: String,
    pub description: String,
    pub last_updated: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub translations: HashMap<String, String>,
}

pub(crate) fn user_i18n_dir() -> PathBuf {
    if let Ok(path) = std::env::var("DEEPSEEK_I18N_DIR") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return crate::config::expand_path(trimmed);
        }
    }

    crate::config::default_i18n_dir()
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n"))
}

fn bundled_i18n_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n")
}

fn load_en_json() -> Result<I18nConfig> {
    let en_path = bundled_i18n_dir().join("en.json");
    let content = fs::read_to_string(&en_path)
        .with_context(|| format!("Unable to read {}", en_path.display()))?;

    serde_json::from_str(&content).with_context(|| format!("Unable to parse {}", en_path.display()))
}

fn load_user_i18n_json() -> Option<I18nConfig> {
    let i18n_path = user_i18n_dir().join("i18n.json");
    if !i18n_path.exists() {
        return None;
    }

    match fs::read_to_string(&i18n_path) {
        Ok(content) => serde_json::from_str(&content).ok(),
        Err(_) => None,
    }
}

pub fn load_translations() -> Result<HashMap<String, String>> {
    if let Some(config) = load_user_i18n_json() {
        return Ok(config.translations);
    }

    Ok(load_en_json()?.translations)
}

/// Result of scaffolding localizable files into the target i18n directory.
#[derive(Debug, Clone, Default)]
pub struct LocalizationScaffoldReport {
    #[allow(dead_code)]
    pub output_dir: PathBuf,
    pub created_files: Vec<PathBuf>,
}

impl LocalizationScaffoldReport {
    fn record_created(&mut self, path: PathBuf) {
        self.created_files.push(path);
    }
}

pub(crate) fn scaffold_tui_localization_files(
    output_dir: &Path,
    _target_locale: &str,
) -> Result<LocalizationScaffoldReport> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("Unable to create {}", output_dir.display()))?;

    let mut report = LocalizationScaffoldReport {
        output_dir: output_dir.to_path_buf(),
        ..LocalizationScaffoldReport::default()
    };

    // Copy en.json as i18n.json (user will translate it)
    let en_source_path = bundled_i18n_dir().join("en.json");
    let i18n_output_path = output_dir.join("i18n.json");
    let en_content = fs::read_to_string(&en_source_path)
        .with_context(|| format!("Unable to read {}", en_source_path.display()))?;
    ensure_text_scaffold(&i18n_output_path, &en_content)?;
    report.record_created(i18n_output_path);

    for target in prompt_localization_targets(output_dir) {
        let source = fs::read_to_string(&target.source)
            .with_context(|| format!("Unable to read {}", target.source.display()))?;
        ensure_text_scaffold(&target.output, &source)?;
        report.record_created(target.output);
    }

    Ok(report)
}

#[derive(Debug)]
struct PromptLocalizationTarget {
    source: PathBuf,
    output: PathBuf,
}

fn prompt_localization_targets(output_dir: &Path) -> Vec<PromptLocalizationTarget> {
    crate::prompts::LOCALIZABLE_PROMPT_LAYERS
        .iter()
        .map(|layer| {
            let source = crate::prompts::bundled_prompt_path(layer.relative_path);
            let output =
                output_dir.join(crate::prompts::i18n_prompt_file_name(layer.relative_path));
            PromptLocalizationTarget { source, output }
        })
        .collect()
}

fn ensure_text_scaffold(path: &Path, contents: &str) -> Result<bool> {
    // Always overwrite to ensure latest source content
    crate::utils::write_atomic(path, contents.as_bytes())
        .with_context(|| format!("Unable to write {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_en_json_from_bundled_source() {
        let config = load_en_json().expect("load en.json");
        assert_eq!(config.language, "en");
        assert!(!config.translations.is_empty());
    }

    #[test]
    fn scaffold_localization_files_creates_readable_targets() {
        let tmp = tempdir().expect("tempdir");
        let report = scaffold_tui_localization_files(tmp.path(), "zh-Hans").expect("scaffold");
        
        // Check i18n.json is created (copied from en.json)
        assert!(
            report
                .created_files
                .iter()
                .any(|path| path.ends_with("i18n.json"))
        );
        assert!(tmp.path().join("i18n.json").exists());
        
        // Verify it's a copy of en.json
        let i18n_config: I18nConfig = serde_json::from_str(
            &fs::read_to_string(tmp.path().join("i18n.json")).expect("read i18n json"),
        )
        .expect("parse i18n json");
        assert_eq!(i18n_config.language, "en"); // Should still be "en" since it's a direct copy
        assert!(!i18n_config.translations.is_empty());

        for layer in crate::prompts::LOCALIZABLE_PROMPT_LAYERS {
            let output = tmp
                .path()
                .join(crate::prompts::i18n_prompt_file_name(layer.relative_path));
            assert!(output.exists(), "missing {}", output.display());
            let raw = fs::read_to_string(&output).expect("localized prompt");
            assert!(!raw.trim().is_empty(), "empty {}", output.display());
        }
    }

    #[test]
    fn scaffold_localization_files_overwrites_existing_targets() {
        let tmp = tempdir().expect("tempdir");
        let output_dir = tmp.path();
        fs::create_dir_all(output_dir).expect("mkdir");
        let i18n_path = output_dir.join("i18n.json");
        let seed = I18nConfig {
            version: "1.0".to_string(),
            language: "zh-Hans".to_string(),
            language_name: "简体中文".to_string(),
            description: "seed".to_string(),
            last_updated: "2026-05-06".to_string(),
            generated_by: None,
            source: None,
            translations: HashMap::from([(String::from("key"), String::from("value"))]),
        };
        let seed_json = serde_json::to_string(&seed).expect("serialize seed");
        fs::write(&i18n_path, &seed_json).expect("seed i18n");
        let prompt_path = output_dir.join("base.i18n.md");
        fs::write(&prompt_path, "existing translation").expect("seed prompt");

        let report = scaffold_tui_localization_files(output_dir, "zh-Hans").expect("scaffold");

        // Should overwrite existing files
        assert!(report.created_files.contains(&i18n_path));
        assert!(report.created_files.contains(&prompt_path));

        // Content should be updated (direct copy from en.json)
        let new_i18n_content = fs::read_to_string(&i18n_path).expect("read i18n");
        let new_config: I18nConfig = serde_json::from_str(&new_i18n_content).expect("parse");
        assert_eq!(new_config.language, "en"); // Direct copy, so still "en"
        assert_ne!(new_config.description, "seed"); // Should match en.json description

        let new_prompt_content = fs::read_to_string(&prompt_path).expect("read prompt");
        assert_ne!(new_prompt_content, "existing translation"); // Should be updated
    }
}
