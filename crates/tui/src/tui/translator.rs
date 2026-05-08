/// 语言检测和翻译工具。
///
/// 在展示层对模型输出的英文内容进行中文翻译。
/// 仅翻译自然语言散文，保留代码、路径、标识符等不变。

use crate::client::DeepSeekClient;
use crate::config::Config;
use crate::llm_client::LlmClient;
use anyhow::Result;
use crate::models::{ContentBlock, Message, MessageRequest};

/// 检测文本是否以英文为主。
/// 只检测真正需要翻译的文本，中英文混合且中文为主时不触发翻译。
pub fn is_mostly_english(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }

    let mut english_chars = 0usize;
    let mut chinese_chars = 0usize;

    for ch in text.chars() {
        if ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch.is_ascii_punctuation() || ch.is_ascii_whitespace() {
            english_chars += 1;
        } else if ('\u{4e00}'..='\u{9fff}').contains(&ch)
            || ('\u{3400}'..='\u{4dbf}').contains(&ch)
        {
            chinese_chars += 1;
        }
    }

    let total = english_chars + chinese_chars;
    if total == 0 {
        return false;
    }

    // 英文占比超过 60% 且中文字符少于 40%
    let english_ratio = english_chars as f64 / total as f64;
    english_ratio > 0.6
}

/// 翻译器，将英文文本翻译为简体中文。
pub struct Translator {
    client: DeepSeekClient,
}

impl Translator {
    /// 从配置创建翻译器。
    /// 返回 `None` 表示翻译器不可用（如 API 密钥未配置）。
    pub fn from_config(config: &Config) -> Option<Self> {
        match DeepSeekClient::new(config) {
            Ok(client) => Some(Self { client }),
            Err(err) => {
                tracing::warn!(?err, "翻译器初始化失败");
                None
            }
        }
    }

    /// 将文本翻译为简体中文。
    ///
    /// 使用 `deepseek-v4-flash` 模型，关闭思考模式，
    /// 以最快速度完成翻译。
    pub async fn translate(&self, text: &str) -> Result<String> {
        let system_msg = "You are a translator. Translate the following text to Simplified Chinese. \
                         Keep code, file paths, identifiers, tool names, URLs and log lines unchanged. \
                         Only translate natural language prose. Return ONLY the translation, no explanations or notes.";

        let request = MessageRequest {
            model: "deepseek-v4-flash".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: vec![ContentBlock::Text {
                        text: system_msg.to_string(),
                        cache_control: None,
                    }],
                },
                Message {
                    role: "user".to_string(),
                    content: vec![ContentBlock::Text {
                        text: text.to_string(),
                        cache_control: None,
                    }],
                },
            ],
            max_tokens: 4096,
            system: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: Some("off".to_string()),
            stream: None,
            metadata: None,
            thinking: None,
            temperature: None,
            top_p: None,
        };

        let response = self.client.create_message(request).await?;

        let translated = response
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if translated.is_empty() {
            // 翻译失败时返回原文
            Ok(text.to_string())
        } else {
            Ok(translated)
        }
    }
}
