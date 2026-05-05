//! Provider metadata and model alias data loaded from `assets/providers.json`.
//!
//! Edit `assets/providers.json` to customize provider display names,
//! add new providers, or update model deprecation notices without
//! recompiling Rust code.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

/// A single provider entry from providers.json.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderEntry {
    pub display_name: String,
    #[serde(default)]
    pub display_name_cn: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub default_flash_model: Option<String>,
    pub default_base_url: String,
    #[serde(default)]
    pub default_base_url_cn: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
}

/// A single model alias entry.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelAliasEntry {
    pub canonical: String,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub notice: String,
}

/// The top-level structure of providers.json.
#[derive(Debug, Clone, Deserialize)]
struct ProvidersDataFile {
    #[serde(default)]
    pub schema_version: u32,
    pub providers: HashMap<String, ProviderEntry>,
    #[serde(default)]
    pub model_aliases: HashMap<String, ModelAliasEntry>,
    #[serde(default)]
    pub common_models: Vec<String>,
}

/// Loaded providers data from `assets/providers.json`.
static PROVIDERS_DATA: LazyLock<ProvidersDataFile> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/providers.json"))
        .expect("Failed to parse embedded providers.json — check its JSON syntax")
});

/// Get the display name for a provider key (e.g. "deepseek" → "DeepSeek").
pub fn provider_display_name(provider_key: &str) -> Option<&'static str> {
    PROVIDERS_DATA
        .providers
        .get(provider_key)
        .map(|e| e.display_name.as_str())
}

/// Get the Chinese display name for a provider, falling back to the standard name.
pub fn provider_display_name_cn(provider_key: &str) -> Option<&'static str> {
    PROVIDERS_DATA.providers.get(provider_key).and_then(|e| {
        e.display_name_cn
            .as_deref()
            .or(Some(e.display_name.as_str()))
    })
}

/// Get the default model for a provider.
pub fn provider_default_model(provider_key: &str) -> Option<&'static str> {
    PROVIDERS_DATA
        .providers
        .get(provider_key)
        .map(|e| e.default_model.as_str())
}

/// Get the default flash model for a provider, falling back to default_model.
pub fn provider_default_flash_model(provider_key: &str) -> Option<&'static str> {
    PROVIDERS_DATA.providers.get(provider_key).and_then(|e| {
        e.default_flash_model
            .as_deref()
            .or(Some(e.default_model.as_str()))
    })
}

/// Get the default base URL for a provider.
pub fn provider_default_base_url(provider_key: &str) -> Option<&'static str> {
    PROVIDERS_DATA
        .providers
        .get(provider_key)
        .map(|e| e.default_base_url.as_str())
}

/// Get the China base URL for a provider, falling back to default_base_url.
pub fn provider_base_url_cn(provider_key: &str) -> Option<&'static str> {
    PROVIDERS_DATA.providers.get(provider_key).and_then(|e| {
        e.default_base_url_cn
            .as_deref()
            .or(Some(e.default_base_url.as_str()))
    })
}

/// Get the list of models for a provider.
pub fn provider_models(provider_key: &str) -> Option<&'static [String]> {
    PROVIDERS_DATA
        .providers
        .get(provider_key)
        .map(|e| e.models.as_slice())
}

/// Check if a model name is a known alias and return its canonical form.
pub fn resolve_model_alias(alias: &str) -> Option<&'static str> {
    let lower = alias.to_ascii_lowercase();
    PROVIDERS_DATA
        .model_aliases
        .get(&lower)
        .map(|e| e.canonical.as_str())
}

/// Check if a model alias is deprecated and return its deprecation info.
pub fn model_alias_deprecation(alias: &str) -> Option<(bool, &'static str)> {
    let lower = alias.to_ascii_lowercase();
    PROVIDERS_DATA.model_aliases.get(&lower).map(|e| {
        (
            e.deprecated,
            e.notice.as_str(),
        )
    })
}

/// Check if a model name is a known alias and return its deprecation metadata.
pub fn model_deprecation(alias: &str) -> Option<super::config::ModelDeprecation> {
    let lower = alias.to_ascii_lowercase();
    PROVIDERS_DATA.model_aliases.get(&lower).map(|e| {
        super::config::ModelDeprecation {
            alias: lower.clone(),
            replacement: e.canonical.clone(),
            notice: e.notice.clone(),
        }
    })
}

/// Get all model aliases that are deprecated.
pub fn deprecated_aliases() -> Vec<(String, String, String)> {
    PROVIDERS_DATA
        .model_aliases
        .iter()
        .filter(|(_, e)| e.deprecated)
        .map(|(alias, entry)| {
            (alias.clone(), entry.canonical.clone(), entry.notice.clone())
        })
        .collect()
}

/// Get the list of common DeepSeek models.
pub fn common_models() -> &'static [String] {
    PROVIDERS_DATA.common_models.as_slice()
}

/// Get all provider keys.
pub fn all_provider_keys() -> Vec<&'static str> {
    PROVIDERS_DATA.providers.keys().map(|s| s.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_data_loads_successfully() {
        assert!(!PROVIDERS_DATA.providers.is_empty(), "providers must be non-empty");
        assert!(PROVIDERS_DATA.providers.contains_key("deepseek"), "deepseek must be present");
    }

    #[test]
    fn deepseek_provider_has_correct_display_name() {
        assert_eq!(provider_display_name("deepseek"), Some("DeepSeek"));
    }

    #[test]
    fn nvidia_provider_has_correct_display_name() {
        assert_eq!(provider_display_name("nvidia_nim"), Some("NVIDIA NIM"));
    }

    #[test]
    fn deepseek_cn_name_resolves() {
        assert_eq!(provider_display_name_cn("deepseek"), Some("DeepSeek (中国)"));
    }

    #[test]
    fn nvidia_cn_name_falls_back_to_display_name() {
        assert_eq!(provider_display_name_cn("nvidia_nim"), Some("NVIDIA NIM"));
    }

    #[test]
    fn deepseek_default_model_resolves() {
        assert_eq!(provider_default_model("deepseek"), Some("deepseek-v4-pro"));
    }

    #[test]
    fn deepseek_default_base_url_resolves() {
        assert_eq!(
            provider_default_base_url("deepseek"),
            Some("https://api.deepseek.com")
        );
    }

    #[test]
    fn deepseek_cn_base_url_resolves() {
        assert_eq!(
            provider_base_url_cn("deepseek"),
            Some("https://api.deepseeki.com")
        );
    }

    #[test]
    fn nvidia_cn_base_url_falls_back() {
        assert_eq!(
            provider_base_url_cn("nvidia_nim"),
            Some("https://integrate.api.nvidia.com/v1")
        );
    }

    #[test]
    fn deepseek_chat_resolves_to_v4_flash() {
        assert_eq!(resolve_model_alias("deepseek-chat"), Some("deepseek-v4-flash"));
    }

    #[test]
    fn deepseek_chat_is_deprecated() {
        let (deprecated, notice) = model_alias_deprecation("deepseek-chat").unwrap();
        assert!(deprecated);
        assert!(notice.contains("Deprecated"));
    }

    #[test]
    fn v4pro_is_not_deprecated() {
        let (deprecated, _) = model_alias_deprecation("deepseek-v4pro").unwrap();
        assert!(!deprecated);
    }

    #[test]
    fn alias_resolution_is_case_insensitive() {
        assert_eq!(resolve_model_alias("DeepSeek-Chat"), Some("deepseek-v4-flash"));
        assert_eq!(resolve_model_alias("DEEPSEEK-R1"), Some("deepseek-v4-flash"));
    }

    #[test]
    fn common_models_are_defined() {
        let models = common_models();
        assert!(models.contains(&"deepseek-v4-pro".to_string()));
        assert!(models.contains(&"deepseek-v4-flash".to_string()));
    }

    #[test]
    fn unknown_provider_returns_none() {
        assert!(provider_display_name("nonexistent").is_none());
        assert!(provider_default_model("nonexistent").is_none());
    }

    #[test]
    fn all_providers_have_entries() {
        let keys = all_provider_keys();
        assert!(keys.contains(&"deepseek"));
        assert!(keys.contains(&"nvidia_nim"));
        assert!(keys.contains(&"openrouter"));
        assert!(keys.contains(&"novita"));
        assert!(keys.contains(&"fireworks"));
        assert!(keys.contains(&"sglang"));
    }
}
