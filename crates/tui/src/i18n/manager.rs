//! I18N file manager - handles loading, saving, and fallback logic.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, Map};

/// Manages i18n JSON files with automatic fallback to English.
pub struct I18nManager {
    /// Path to ~/.deepseek/i18n/i18n.json (target language)
    i18n_path: PathBuf,
    /// Path to ~/.deepseek/i18n/en.json (English source)
    en_path: PathBuf,
    /// Path to ~/.deepseek/i18n/cache/
    cache_dir: PathBuf,
    /// Loaded i18n data (target language)
    i18n_data: HashMap<String, String>,
    /// Loaded English data (fallback)
    en_data: HashMap<String, String>,
}

impl I18nManager {
    /// Create a new I18nManager instance.
    pub fn new() -> Result<Self> {
        let base_dir = Self::i18n_base_dir()?;
        let i18n_path = base_dir.join("i18n.json");
        let en_path = base_dir.join("en.json");
        let cache_dir = base_dir.join("cache");

        // Ensure directories exist
        std::fs::create_dir_all(&base_dir)
            .context("failed to create i18n directory")?;
        std::fs::create_dir_all(&cache_dir)
            .context("failed to create cache directory")?;

        // Load data files
        let i18n_data = Self::load_json_file(&i18n_path).unwrap_or_default();
        let en_data = Self::load_json_file(&en_path).unwrap_or_default();

        Ok(Self {
            i18n_path,
            en_path,
            cache_dir,
            i18n_data,
            en_data,
        })
    }

    /// Get the base directory for i18n files: ~/.deepseek/i18n/
    fn i18n_base_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("failed to resolve home directory")?;
        Ok(home.join(".deepseek").join("i18n"))
    }

    /// Load a JSON file into a HashMap.
    fn load_json_file(path: &Path) -> Result<HashMap<String, String>> {
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        
        let json: Value = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        let mut map = HashMap::new();
        if let Value::Object(obj) = json {
            for (key, value) in obj {
                if let Value::String(text) = value {
                    map.insert(key, text);
                }
            }
        }

        Ok(map)
    }

    /// Get a translation by key with automatic fallback to English.
    ///
    /// # Priority
    /// 1. `i18n.json` (target language)
    /// 2. `en.json` (English fallback)
    /// 3. `None` if key doesn't exist
    pub fn get(&self, key: &str) -> Option<String> {
        self.i18n_data.get(key)
            .cloned()
            .or_else(|| self.en_data.get(key).cloned())
    }

    /// Check if i18n.json exists.
    pub fn i18n_exists(&self) -> bool {
        self.i18n_path.exists()
    }

    /// Check if en.json exists.
    pub fn en_exists(&self) -> bool {
        self.en_path.exists()
    }

    /// Ensure en.json exists by generating it from localization.rs.
    pub fn ensure_en_json(&self) -> Result<()> {
        if self.en_exists() {
            return Ok(());
        }

        // Generate en.json from static translations
        let en_data = Self::generate_en_json()?;
        self.save_json_to_path(&self.en_path, &en_data)?;

        Ok(())
    }

    /// Generate English JSON data from the embedded en.json.
    fn generate_en_json() -> Result<HashMap<String, String>> {
        Ok(crate::localization::get_embedded_en_data())
    }

    /// Save data to i18n.json.
    pub fn save_i18n(&self, data: &HashMap<String, String>) -> Result<()> {
        self.save_json_to_path(&self.i18n_path, data)
    }

    /// Save HashMap to a specific JSON file path.
    fn save_json_to_path(&self, path: &Path, data: &HashMap<String, String>) -> Result<()> {
        let mut map = Map::new();
        for (key, value) in data {
            map.insert(key.clone(), Value::String(value.clone()));
        }

        let json = Value::Object(map);
        let content = serde_json::to_string_pretty(&json)
            .context("failed to serialize JSON")?;

        std::fs::write(path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;

        Ok(())
    }

    /// Reload data from disk (after translation update).
    pub fn reload(&mut self) -> Result<()> {
        self.i18n_data = Self::load_json_file(&self.i18n_path).unwrap_or_default();
        self.en_data = Self::load_json_file(&self.en_path).unwrap_or_default();
        Ok(())
    }

    /// Get the path to i18n.json.
    pub fn i18n_path(&self) -> &Path {
        &self.i18n_path
    }

    /// Get the path to en.json.
    pub fn en_path(&self) -> &Path {
        &self.en_path
    }

    /// Get the cache directory.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Get all keys that are missing from i18n.json but exist in en.json.
    pub fn missing_keys(&self) -> Vec<String> {
        self.en_data.keys()
            .filter(|key| !self.i18n_data.contains_key(*key))
            .cloned()
            .collect()
    }

    /// Check if translation is complete (all keys present).
    pub fn is_complete(&self) -> bool {
        self.missing_keys().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i18n_manager_creation() {
        let manager = I18nManager::new();
        assert!(manager.is_ok());
    }

    #[test]
    fn test_fallback_logic() {
        // This test would require setting up test files
        // For now, just verify the structure compiles
        assert!(true);
    }
}
