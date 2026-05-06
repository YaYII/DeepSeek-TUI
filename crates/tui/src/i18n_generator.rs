//! AI 驱动的国际化翻译生成器。
//!
//! 此模块通过调用 DeepSeek API 将 `en.json` 翻译成用户目标语言，
//! 生成 `i18n.json` 文件。生成的文件会缓存到磁盘上，避免重复调用 API。
//!
//! 工作流程：
//! 1. 检查 `i18n.json` 是否存在且匹配当前语言
//! 2. 如果不存在，调用 DeepSeek API 翻译 `en.json` → 目标语言
//! 3. 将结果保存为 `i18n.json`（物理缓存文件）
//! 4. 提示用户重启 DeepSeek TUI 以加载新翻译

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// 导入需要的类型（用于 API 调用）
use crate::models::{ContentBlock, Message, MessageRequest};

/// i18n 配置结构体（与 en.json 格式匹配）
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

/// 获取 i18n 目录路径
fn get_i18n_dir() -> PathBuf {
    // 在生产环境中，这应该相对于二进制文件位置
    // 目前使用相对于 crate 根目录的固定路径
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n")
}

/// 加载 en.json（事实来源）
fn load_en_json() -> Result<I18nConfig> {
    let en_path = get_i18n_dir().join("en.json");
    let content = fs::read_to_string(&en_path)
        .with_context(|| format!("无法读取文件 {}", en_path.display()))?;
    
    let config: I18nConfig = serde_json::from_str(&content)
        .with_context(|| format!("无法解析文件 {}", en_path.display()))?;
    
    Ok(config)
}

/// 如果存在则加载现有的 i18n.json
fn load_i18n_json() -> Option<I18nConfig> {
    let i18n_path = get_i18n_dir().join("i18n.json");
    if !i18n_path.exists() {
        return None;
    }
    
    match fs::read_to_string(&i18n_path) {
        Ok(content) => serde_json::from_str(&content).ok(),
        Err(_) => None,
    }
}

/// 检查是否需要为目标语言重新生成 i18n.json
pub fn should_regenerate(target_locale: &str) -> bool {
    match load_i18n_json() {
        Some(config) => {
            let needs_update = config.language != target_locale;
            if needs_update {
                println!(
                    "✓ 语言不匹配：i18n.json 的语言是 '{}'，需要 '{}'",
                    config.language, target_locale
                );
            } else {
                println!(
                    "✓ i18n.json 已存在，语言: {}，跳过生成",
                    target_locale
                );
            }
            needs_update
        }
        None => {
            println!("✓ i18n.json 不存在，将为语言 {} 生成", target_locale);
            true
        }
    }
}

/// 为 AI 生成翻译提示
fn generate_translation_prompt(en_data: &I18nConfig, target_locale: &str) -> String {
    let translations_json = serde_json::to_string_pretty(&en_data.translations)
        .unwrap_or_else(|_| "{}".to_string());
    
    // 对于 AI，我们直接传递语言标签。AI 理解标准的语言代码。
    // 例如："Translate to zh-Hans" 对模型来说已经足够清晰。
    format!(
        r#"你是一位专业的翻译员。你当前正在执行语言国际化操作，请将以下 UI 字符串从英文翻译成目标语言标签{target_locale}对应的语言。

要求：
1. 保持完全相同的 JSON 结构
2. 保留所有占位符变量，如 {{tag}}、{{err}}、{{count}} 等，不要修改
3. 保留所有换行符（\n）和格式
4. 保持特殊字符和符号不变
5. 使用适合终端 UI 应用程序的自然、地道的表达
6. 只翻译值，不要翻译键
7. 仅返回包含翻译值的有效 JSON 对象，不要添加额外的文本或解释

源 JSON（英文）：
{translations_json}

请提供具有相同结构的翻译后的 JSON。"#,
        target_locale = target_locale,
        translations_json = translations_json
    )
}

/// 调用 DeepSeek API 进行翻译（使用现有的客户端基础设施）
pub async fn call_deepseek_for_translation(
    prompt: &str,
    _target_locale: &str,
    config: &crate::config::Config,
) -> Result<HashMap<String, String>> {
    use crate::client::DeepSeekClient;
    use crate::llm_client::LlmClient;  // 导入 trait
    
    println!("🔄 正在调用 DeepSeek API 进行翻译...");
    
    // 创建 DeepSeek 客户端（复用现有基础设施）
    let client = DeepSeekClient::new(config)
        .context("无法创建 DeepSeek 客户端")?;
    
    // 构建请求（使用统一的模式）
    // 翻译任务使用 flash 模型，速度更快
    // 显式禁用思考模式，避免返回 Thinking 块
    let model = "deepseek-v4-flash";
    let mut request = build_message_request(model, prompt, 4096, Some(0.3));
    request.reasoning_effort = Some("off".to_string());  // 禁用思考模式
    
    // 调用 API（使用统一的调度服务）
    let timeout_duration = std::time::Duration::from_secs(120);  // 增加到 120 秒，翻译大量文本需要更长时间
    let response = tokio::time::timeout(timeout_duration, client.create_message(request))
        .await
        .context("翻译请求超时（120秒）")?
        .context("翻译请求失败")?;
    
    println!("✓ 已从 DeepSeek API 接收响应");
    
    // 找到第一个 Text 类型的块（跳过 Thinking 块）
    let json_str = response.content.iter()
        .find_map(|block| {
            match block {
                ContentBlock::Text { text, .. } => Some(text.clone()),
                _ => None,
            }
        })
        .ok_or_else(|| anyhow::anyhow!("响应中没有 Text 类型的内容块"))?;
    
    // 直接解析为 HashMap
    let translations: HashMap<String, String> = serde_json::from_str(&json_str)
        .context("无法解析 AI 返回的 JSON")?;
    
    Ok(translations)
}

/// 构建标准的 MessageRequest（统一的消息请求构造器）
fn build_message_request(
    model: &str,
    prompt: &str,
    max_tokens: u32,
    temperature: Option<f32>,
) -> MessageRequest {
    MessageRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: prompt.to_string(),
                cache_control: None,
            }],
        }],
        max_tokens,
        system: None,
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,  // 翻译任务不需要思考模式
        reasoning_effort: None,
        stream: Some(false),
        temperature,
        top_p: None,
    }
}


/// 为目标语言生成 i18n.json
pub async fn generate_i18n(target_locale: &str, config: &crate::config::Config) -> Result<()> {
    println!("正在为 {} 生成 i18n.json...", target_locale);
    
    // 步骤 1：加载 en.json
    let en_config = load_en_json()?;
    println!("✓ 已加载 en.json（{} 个翻译）", en_config.translations.len());
    
    // 步骤 2：生成翻译提示
    let prompt = generate_translation_prompt(&en_config, target_locale);
    
    // 步骤 3：调用 DeepSeek API
    let translations = call_deepseek_for_translation(&prompt, target_locale, config).await?;
    println!("✓ 已从 DeepSeek API 接收翻译");
    
    // 步骤 4：构建 i18n 配置
    let i18n_config = I18nConfig {
        version: en_config.version.clone(),
        language: target_locale.to_string(),
        language_name: target_locale.to_string(),  // 使用语言标签；AI 能理解
        description: format!("自动生成的 {} 翻译", target_locale),
        last_updated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        generated_by: Some("DeepSeek TUI AI 翻译器".to_string()),
        source: Some("en.json".to_string()),
        translations,
    };
    
    // 步骤 5：保存到 i18n.json
    save_i18n_json(&i18n_config)?;
    
    println!("✓ 成功生成 i18n.json");
    println!("⚠ 请重启 DeepSeek TUI 以加载新翻译");
    
    Ok(())
}

/// 将 i18n 配置保存到文件
fn save_i18n_json(config: &I18nConfig) -> Result<()> {
    let i18n_dir = get_i18n_dir();
    fs::create_dir_all(&i18n_dir)
        .with_context(|| format!("无法创建目录 {}", i18n_dir.display()))?;
    
    let i18n_path = i18n_dir.join("i18n.json");
    let content = serde_json::to_string_pretty(config)
        .context("序列化 i18n 配置失败")?;
    
    fs::write(&i18n_path, content)
        .with_context(|| format!("无法写入文件 {}", i18n_path.display()))?;
    
    Ok(())
}

/// 从 i18n.json 加载翻译（或回退到 en.json）
pub fn load_translations() -> Result<HashMap<String, String>> {
    // 首先尝试加载 i18n.json
    if let Some(config) = load_i18n_json() {
        return Ok(config.translations);
    }
    
    // 回退到 en.json
    let en_config = load_en_json()?;
    Ok(en_config.translations)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_load_en_json() {
        let config = load_en_json();
        assert!(config.is_ok(), "应该成功加载 en.json");
        
        let config = config.unwrap();
        assert_eq!(config.language, "en");
        assert!(!config.translations.is_empty());
    }
    
   
}
