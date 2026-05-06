//! Runtime i18n file discovery and transparent localization task prompts.
//!
//! The application never calls a hidden translation API from here. `/localize`
//! sends the generated instructions through the normal TUI conversation so the
//! user can inspect, edit, and approve the files the assistant writes.

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

/// Build the visible TUI task sent by `/localize <locale>`.
pub fn generate_tui_localization_prompt(target_locale: &str) -> Result<String> {
    let en_path = bundled_i18n_dir().join("en.json");
    let source = fs::read_to_string(&en_path)
        .with_context(|| format!("Unable to read {}", en_path.display()))?;
    let output_dir = user_i18n_dir();
    let output_path = output_dir.join("i18n.json");
    let prompt_files = prompt_localization_targets(&output_dir);
    let prompt_file_list = prompt_files
        .iter()
        .map(|target| {
            format!(
                "- `{}` -> `{}`",
                target.source.display(),
                target.output.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        r#"请为 DeepSeek TUI 生成本地化文件。

目标语言标签：{target_locale}
英文 UI 源文件：{en_path}
输出目录：{output_dir}
UI 输出文件：{output_path}

所有输出文件都放在 `{output_dir}`。运行时优先读取这里的本地文件；文件缺失时回退到软件内置英文内容。

还要本地化系统 prompt 覆盖文件。命名规则是 `原文件名.i18n.原后缀`，例如 `base.md` 输出为 `base.i18n.md`。

系统 prompt 源文件与输出文件：
{prompt_file_list}

请执行这些步骤：
1. 创建输出目录 `{output_dir}`。
2. 读取下面的英文 JSON，保持顶层结构兼容 `version`、`language`、`language_name`、`description`、`last_updated`、`generated_by`、`source`、`translations`。
3. 只翻译 `translations` 里的值，不要翻译键。
4. 保留所有占位符变量，例如 `{{tag}}`、`{{err}}`、`{{count}}`、`{{cost}}`、`{{path}}`。
5. 保留换行语义、命令名、配置键、模型名、文件路径、URL、快捷键和终端符号。
6. 将 UI 翻译写入 `{output_path}`，其中 `language` 必须是 `{target_locale}`，`source` 使用 `en.json`，`generated_by` 使用 `DeepSeek TUI visible localization workflow`。
7. 对上面列出的每个系统 prompt 源文件，读取源文件，翻译人类可见说明文本，保留 Markdown 结构、XML/HTML 标签、工具名、命令名、配置键、代码块、占位符和 sentinel 字符串。把结果写入对应的 `*.i18n.*` 输出文件。
8. 写入后用 JSON 解析器校验 `{output_path}` 是合法 JSON，并检查翻译键集合与英文源一致。发现问题请直接修正文件。
9. 完成后告诉我需要重启 TUI 才会加载新的本地化文件。

英文 UI 源 JSON：
```json
{source}
```
"#,
        target_locale = target_locale,
        en_path = en_path.display(),
        output_dir = output_dir.display(),
        output_path = output_path.display(),
        prompt_file_list = prompt_file_list,
        source = source
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_en_json_from_bundled_source() {
        let config = load_en_json().expect("load en.json");
        assert_eq!(config.language, "en");
        assert!(!config.translations.is_empty());
    }

    #[test]
    fn localize_prompt_mentions_user_i18n_outputs() {
        let prompt = generate_tui_localization_prompt("zh-Hans").expect("prompt");
        assert!(prompt.contains("zh-Hans"));
        assert!(prompt.contains("i18n.json"));
        for layer in crate::prompts::LOCALIZABLE_PROMPT_LAYERS {
            let output = crate::prompts::i18n_prompt_file_name(layer.relative_path);
            assert!(prompt.contains(&output), "missing {output}");
        }
    }
}
