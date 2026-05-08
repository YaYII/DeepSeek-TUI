//! DeepSeek 模型基于文本的工具调用的旧版解析器。
//!
//! 结构化工具调用项是首选，因此引擎不再调用此解析器。
//! 保留用于参考/调试。
//!
//! 某些 DeepSeek 以各种格式将工具调用输出为文本：
//! ```text
//! [TOOL_CALL]
//! {tool => "tool_name", args => {...}}
//! [/TOOL_CALL]
//! ```
//!
//! 或 XML 风格格式：
//! ```text
//! <deepseek:tool_call>
//! <invoke name="tool_name">
//! <parameter name="arg">value</parameter>
//! </invoke>
//! </deepseek:tool_call>
//! ```
//!
//! 本模块将这些文本模式解析为结构化工具调用。

use regex::Regex;
use serde_json::{Value, json};
use std::sync::OnceLock;

/// 从文本内容解析出的工具调用。
#[derive(Debug, Clone)]
pub struct ParsedToolCall {
    /// 工具名称
    pub name: String,
    /// JSON 格式的工具参数
    pub args: Value,
    /// 工具调用的生成 ID
    pub id: String,
}

/// 解析文本中工具调用的结果。
#[derive(Debug)]
pub struct ParseResult {
    /// 移除了工具调用标记的文本（用于显示）
    pub clean_text: String,
    /// 在文本中找到的已解析工具调用
    pub tool_calls: Vec<ParsedToolCall>,
}

static TOOL_CALL_REGEX: OnceLock<Regex> = OnceLock::new();
static XML_TOOL_CALL_REGEX: OnceLock<Regex> = OnceLock::new();
static INVOKE_REGEX: OnceLock<Regex> = OnceLock::new();
static THINKING_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_tool_call_regex() -> &'static Regex {
    TOOL_CALL_REGEX.get_or_init(|| {
        // 匹配 [TOOL_CALL] ... [/TOOL_CALL] 块
        Regex::new(r"(?s)\[TOOL_CALL\]\s*(.*?)\s*\[/TOOL_CALL\]")
            .expect("TOOL_CALL regex pattern is valid")
    })
}

fn get_xml_tool_call_regex() -> &'static Regex {
    XML_TOOL_CALL_REGEX.get_or_init(|| {
        // 匹配 <deepseek:tool_call>...</deepseek:tool_call> 或类似的 XML 模式
        Regex::new(r"(?s)<(?:deepseek:)?tool_call[^>]*>\s*(.*?)\s*</(?:deepseek:)?tool_call>")
            .expect("XML tool_call regex pattern is valid")
    })
}

fn get_invoke_regex() -> &'static Regex {
    INVOKE_REGEX.get_or_init(|| {
        // 匹配 <invoke name="tool_name">...</invoke> 模式
        Regex::new(r#"(?s)<invoke\s+name\s*=\s*"([^"]+)"[^>]*>(.*?)</invoke>"#)
            .expect("invoke regex pattern is valid")
    })
}

fn get_thinking_regex() -> &'static Regex {
    THINKING_REGEX.get_or_init(|| {
        // 匹配思考块，包括部分闭合标签
        Regex::new(r"(?s)</?(?:think|thinking)[^>]*>").expect("thinking regex pattern is valid")
    })
}

/// 从文本内容解析工具调用。
/// 返回清理后的文本（移除了标记）和任何已解析的工具调用。
pub fn parse_tool_calls(text: &str) -> ParseResult {
    let mut tool_calls = Vec::new();
    let mut clean_text = text.to_string();
    let mut id_counter = 0;

    // 首先，移除思考标签
    let thinking_regex = get_thinking_regex();
    clean_text = thinking_regex.replace_all(&clean_text, "").to_string();

    // 解析 [TOOL_CALL] 格式
    let regex = get_tool_call_regex();
    for cap in regex.captures_iter(text) {
        let (Some(full_match), Some(inner)) = (cap.get(0), cap.get(1)) else {
            continue;
        };
        let full_match = full_match.as_str();
        let inner = inner.as_str().trim();

        if let Some(parsed) = parse_tool_call_inner(inner, &mut id_counter) {
            tool_calls.push(parsed);
        }

        clean_text = clean_text.replace(full_match, "");
    }

    // 解析 XML 风格 <deepseek:tool_call> 或 <tool_call> 格式
    let xml_regex = get_xml_tool_call_regex();
    for cap in xml_regex.captures_iter(text) {
        let (Some(full_match), Some(inner)) = (cap.get(0), cap.get(1)) else {
            continue;
        };
        let full_match = full_match.as_str();
        let inner = inner.as_str().trim();

        // 解析内部的 invoke 块
        if let Some(parsed) = parse_invoke_block(inner, &mut id_counter) {
            tool_calls.push(parsed);
        } else if let Some(parsed) = parse_tool_call_inner(inner, &mut id_counter) {
            tool_calls.push(parsed);
        }

        clean_text = clean_text.replace(full_match, "");
    }

    // 也解析可能未被包裹的独立 <invoke> 块
    let invoke_regex = get_invoke_regex();
    for cap in invoke_regex.captures_iter(&clean_text.clone()) {
        let (Some(full_match), Some(tool_name), Some(inner)) = (cap.get(0), cap.get(1), cap.get(2))
        else {
            continue;
        };
        let full_match = full_match.as_str();
        let tool_name = tool_name.as_str();
        let inner = inner.as_str();

        let args = parse_xml_parameters(inner);
        id_counter += 1;
        tool_calls.push(ParsedToolCall {
            name: tool_name.to_string(),
            args,
            id: format!("xml_tool_{id_counter}"),
        });

        clean_text = clean_text.replace(full_match, "");
    }

    // 清理多余的空白和空行
    clean_text = clean_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    ParseResult {
        clean_text,
        tool_calls,
    }
}

/// 将 `<invoke>` 块解析为工具调用。
fn parse_invoke_block(content: &str, id_counter: &mut u32) -> Option<ParsedToolCall> {
    let invoke_regex = get_invoke_regex();
    let cap = invoke_regex.captures(content)?;

    let tool_name = cap.get(1)?.as_str();
    let inner = cap.get(2)?.as_str();

    let args = parse_xml_parameters(inner);

    *id_counter += 1;
    Some(ParsedToolCall {
        name: tool_name.to_string(),
        args,
        id: format!("xml_tool_{id_counter}"),
    })
}

/// 解析 XML 风格参数，如 <parameter name="foo">value</parameter>
fn parse_xml_parameters(content: &str) -> Value {
    let param_regex = Regex::new(
        "<(?:parameter|param)\\s+name\\s*=\\s*\"([^\"]+)\"[^>]*>(.*?)</(?:parameter|param)>",
    )
    .ok();
    let simple_tag_regex =
        Regex::new("<([a-zA-Z_][a-zA-Z0-9_]*)>(.*?)</([a-zA-Z_][a-zA-Z0-9_]*)>").ok();

    let mut map = serde_json::Map::new();

    // Try parsing <parameter name="...">value</parameter>
    if let Some(regex) = param_regex {
        for cap in regex.captures_iter(content) {
            if let (Some(name), Some(value)) = (cap.get(1), cap.get(2)) {
                let name_str = name.as_str();
                let value_str = value.as_str().trim();

                // Try to parse as JSON, otherwise use as string
                let json_value = serde_json::from_str(value_str)
                    .unwrap_or_else(|_| Value::String(value_str.to_string()));
                map.insert(name_str.to_string(), json_value);
            }
        }
    }

    // Also try parsing <tagname>value</tagname> format
    if let Some(regex) = simple_tag_regex {
        for cap in regex.captures_iter(content) {
            if let (Some(name), Some(value), Some(close)) = (cap.get(1), cap.get(2), cap.get(3)) {
                if name.as_str() != close.as_str() {
                    continue;
                }
                let name_str = name.as_str();
                // Skip known wrapper tags
                if ["invoke", "tool_call", "parameter", "param"].contains(&name_str) {
                    continue;
                }
                let value_str = value.as_str().trim();
                if !map.contains_key(name_str) {
                    let json_value = serde_json::from_str(value_str)
                        .unwrap_or_else(|_| Value::String(value_str.to_string()));
                    map.insert(name_str.to_string(), json_value);
                }
            }
        }
    }

    Value::Object(map)
}

/// Parse the inner content of a `TOOL_CALL` block.
fn parse_tool_call_inner(inner: &str, id_counter: &mut u32) -> Option<ParsedToolCall> {
    // Try to parse as JSON first
    if let Ok(json) = serde_json::from_str::<Value>(inner) {
        return parse_from_json(&json, id_counter);
    }

    // Try the arrow syntax: {tool => "name", args => {...}}
    if let Some(parsed) = parse_arrow_syntax(inner, id_counter) {
        return Some(parsed);
    }

    // Try to extract tool name and args from any format
    parse_flexible_format(inner, id_counter)
}

/// Parse from JSON object.
fn parse_from_json(json: &Value, id_counter: &mut u32) -> Option<ParsedToolCall> {
    let obj = json.as_object()?;

    // Try different field names for the tool name
    let name = obj
        .get("tool")
        .or_else(|| obj.get("name"))
        .or_else(|| obj.get("function"))
        .and_then(|v| v.as_str())?
        .to_string();

    // Try different field names for the arguments
    let args = obj
        .get("args")
        .or_else(|| obj.get("arguments"))
        .or_else(|| obj.get("input"))
        .or_else(|| obj.get("parameters"))
        .cloned()
        .unwrap_or(json!({}));

    *id_counter += 1;
    Some(ParsedToolCall {
        name,
        args,
        id: format!("text_tool_{id_counter}"),
    })
}

/// Parse the arrow syntax: {tool => "name", args => {...}}
fn parse_arrow_syntax(inner: &str, id_counter: &mut u32) -> Option<ParsedToolCall> {
    // Extract tool name
    let tool_regex = Regex::new(r#"tool\s*=>\s*"([^"]+)""#).ok()?;
    let name = tool_regex.captures(inner)?.get(1)?.as_str().to_string();

    // Extract args - try to find the JSON object after "args =>"
    let args = if let Some(args_start) = inner.find("args =>") {
        let args_str = inner[args_start + 7..].trim();
        // Try to parse as JSON first
        if let Ok(args_json) = serde_json::from_str::<Value>(args_str) {
            args_json
        } else if let Some(brace_start) = args_str.find('{') {
            // Try to extract the content between braces
            let mut brace_count = 0;
            let mut end_idx = brace_start;
            for (i, c) in args_str[brace_start..].chars().enumerate() {
                match c {
                    '{' => brace_count += 1,
                    '}' => {
                        brace_count -= 1;
                        if brace_count == 0 {
                            end_idx = brace_start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let content = &args_str[brace_start + 1..end_idx - 1];

            // Try to parse as JSON
            if let Ok(json) = serde_json::from_str::<Value>(&format!("{{{content}}}")) {
                json
            } else {
                // Try CLI-style args: --arg_name "value" or --arg_name value
                parse_cli_style_args(content)
            }
        } else {
            json!({})
        }
    } else {
        json!({})
    };

    *id_counter += 1;
    Some(ParsedToolCall {
        name,
        args,
        id: format!("text_tool_{id_counter}"),
    })
}

/// Parse CLI-style arguments: --`arg_name` "value" or --`arg_name` value
fn parse_cli_style_args(content: &str) -> Value {
    let mut map = serde_json::Map::new();

    // Pattern: --arg_name "value" or --arg_name 'value' or --arg_name value
    let arg_regex =
        Regex::new(r#"--([a-zA-Z_][a-zA-Z0-9_]*)\s+(?:"([^"]*)"|'([^']*)'|(\S+))"#).ok();

    if let Some(regex) = arg_regex {
        for cap in regex.captures_iter(content) {
            if let Some(arg_name) = cap.get(1) {
                let arg_name = arg_name.as_str();
                // Get the value from whichever capture group matched
                let value = cap
                    .get(2)
                    .or_else(|| cap.get(3))
                    .or_else(|| cap.get(4))
                    .map_or("", |m| m.as_str());

                // Try to parse as JSON value, otherwise use as string
                let json_value = serde_json::from_str(value)
                    .unwrap_or_else(|_| Value::String(value.to_string()));
                map.insert(arg_name.to_string(), json_value);
            }
        }
    }

    // Also try simple key=value format
    let kv_regex =
        Regex::new(r#"([a-zA-Z_][a-zA-Z0-9_]*)\s*[:=]\s*(?:"([^"]*)"|'([^']*)'|(\S+))"#).ok();
    if let Some(regex) = kv_regex {
        for cap in regex.captures_iter(content) {
            if let Some(key) = cap.get(1) {
                let key = key.as_str();
                if !map.contains_key(key) {
                    let value = cap
                        .get(2)
                        .or_else(|| cap.get(3))
                        .or_else(|| cap.get(4))
                        .map_or("", |m| m.as_str());
                    let json_value = serde_json::from_str(value)
                        .unwrap_or_else(|_| Value::String(value.to_string()));
                    map.insert(key.to_string(), json_value);
                }
            }
        }
    }

    Value::Object(map)
}

/// Try to parse a flexible format.
fn parse_flexible_format(inner: &str, id_counter: &mut u32) -> Option<ParsedToolCall> {
    // Look for common patterns like:
    // tool: list_dir
    // name: "list_dir"
    // function: list_dir

    let patterns = [(
        r#"(?:tool|name|function)\s*[:=]\s*"?([a-zA-Z_][a-zA-Z0-9_]*)"?"#,
        1,
    )];

    for (pattern, group) in patterns {
        if let Ok(regex) = Regex::new(pattern)
            && let Some(cap) = regex.captures(inner)
            && let Some(name_match) = cap.get(group)
        {
            let name = name_match.as_str().to_string();

            // Try to extract args/input as JSON
            let args = extract_json_object(inner).unwrap_or(json!({}));

            *id_counter += 1;
            return Some(ParsedToolCall {
                name,
                args,
                id: format!("text_tool_{id_counter}"),
            });
        }
    }

    None
}

/// Extract the first JSON object from a string.
fn extract_json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let mut brace_count = 0;
    let mut end_idx = start;

    for (i, c) in text[start..].chars().enumerate() {
        match c {
            '{' => brace_count += 1,
            '}' => {
                brace_count -= 1;
                if brace_count == 0 {
                    end_idx = start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    let json_str = &text[start..end_idx];
    serde_json::from_str(json_str).ok()
}

/// Check if text contains tool call markers (either format).
pub fn has_tool_call_markers(text: &str) -> bool {
    text.contains("[TOOL_CALL]")
        || text.contains("<deepseek:tool_call")
        || text.contains("<tool_call")
        || text.contains("<invoke ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_arrow_syntax() {
        let text = r#"I'll list the directory.
[TOOL_CALL]
{tool => "list_dir", args => {}}
[/TOOL_CALL]"#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "list_dir");
        assert_eq!(result.clean_text, "I'll list the directory.");
    }

    #[test]
    fn test_parse_json_syntax() {
        let text = r#"Let me check.
[TOOL_CALL]
{"tool": "read_file", "args": {"path": "test.txt"}}
[/TOOL_CALL]"#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "read_file");
        assert_eq!(result.tool_calls[0].args["path"], "test.txt");
    }

    #[test]
    fn test_parse_multiple_tool_calls() {
        let text = r#"First I'll list, then read.
[TOOL_CALL]
{tool => "list_dir", args => {}}
[/TOOL_CALL]
[TOOL_CALL]
{tool => "read_file", args => {"path": "file.txt"}}
[/TOOL_CALL]"#;

        let result = parse_tool_calls(text);
        assert_eq!(result.tool_calls.len(), 2);
        assert_eq!(result.tool_calls[0].name, "list_dir");
        assert_eq!(result.tool_calls[1].name, "read_file");
    }

    #[test]
    fn test_no_tool_calls() {
        let text = "Just some regular text without any tool calls.";
        let result = parse_tool_calls(text);
        assert!(result.tool_calls.is_empty());
        assert_eq!(result.clean_text, text);
    }

    #[test]
    fn test_has_markers() {
        assert!(has_tool_call_markers("[TOOL_CALL]test[/TOOL_CALL]"));
        assert!(!has_tool_call_markers("no markers here"));
    }
}
