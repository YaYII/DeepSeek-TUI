//! 参数修复 — 修复格式错误的 JSON 工具参数。
//!
//! DeepSeek 流式传输 `tool_calls.function.arguments` 作为增量。
//! 两种失败情况很常见：(a) SSE 块边界在 JSON 字符串内切割，重新组装后
//! 留下尾部逗号或未闭合的括号；(b) 一些本地后端在 JSON 字符串值中
//! 发出字面控制字符。
//!
//! 修复阶梯在回退到空对象之前运行五个阶段：
//!
//!  1. 严格解析——如果能解析则完成。
//!  2. 去除字符串值中的字面控制字符。
//!  3. 去除 `}` 或 `]` 前的尾部逗号。
//!  4. 平衡括号/方括号（附加闭合符号）。
//!  5. 如果增量为负，去除多余的闭合符号。
//!  6. 回退：空对象 `{}`。

use serde_json::{Map, Value};

/// 我们尝试修复的最大原始参数长度（1 MiB）。
const MAX_ARG_LEN: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ArgRepairError {
    #[error("argument exceeded {0} chars; refusing to repair")]
    TooLarge(usize),
}

/// 将原始 JSON 参数字符串修复为有效的 `serde_json::Value`。
///
/// 运行确定性阶梯；成功时返回解析后的值。
/// 最终回退是空对象 `{}`，以便调度始终进行。
pub fn repair(raw: &str) -> Result<Value, ArgRepairError> {
    if raw.len() > MAX_ARG_LEN {
        return Err(ArgRepairError::TooLarge(raw.len()));
    }
    // Stage 1: strict parse
    if let Ok(v) = serde_json::from_str(raw) {
        return Ok(v);
    }
    // Stage 2: strip control chars inside strings
    let mut s = strip_control_chars_in_strings(raw);
    if let Ok(v) = serde_json::from_str(&s) {
        return Ok(v);
    }
    // Stage 3: strip trailing commas
    s = strip_trailing_commas(&s);
    if let Ok(v) = serde_json::from_str(&s) {
        return Ok(v);
    }
    // Stage 4: balance braces
    s = balance_braces(&s, 50);
    if let Ok(v) = serde_json::from_str(&s) {
        return Ok(v);
    }
    // Stage 5: strip excess closers
    s = strip_excess_closers(&s);
    if let Ok(v) = serde_json::from_str(&s) {
        return Ok(v);
    }
    // Fallback: empty object
    Ok(Value::Object(Map::new()))
}

/// 去除出现在 JSON 字符串值内部的 ASCII 控制字符（0x00–0x1F 除 \t、\n、\r 外）。
/// 我们逐字符遍历，跟踪是否在字符串内部（在未转义的双引号之间）。
fn strip_control_chars_in_strings(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_string = false;
    let mut escape = false;
    for ch in s.chars() {
        if escape {
            out.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            out.push(ch);
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            continue;
        }
        if in_string && (ch as u32) < 0x20 && ch != '\t' && ch != '\n' && ch != '\r' {
            // Drop control characters inside strings
            continue;
        }
        out.push(ch);
    }
    out
}

/// 去除 `}` 或 `]` 前的尾部逗号。
fn strip_trailing_commas(s: &str) -> String {
    // Repeatedly replace ",}" and ",]" until stable (handles nested cases).
    let mut out = s.to_string();
    loop {
        let prev = out.clone();
        out = out.replace(",}", "}").replace(",]", "]");
        // Handle trailing comma at end of string
        out = out.trim_end_matches(',').to_string();
        if out == prev {
            break;
        }
    }
    out
}

/// 平衡大括号和方括号：统计 `{`/`}` 和 `[`/`]` 数量，如果增量为正（开比闭多）
/// 则附加闭合符号。限制迭代次数，以防灾难性损坏的输入永远循环。
fn balance_braces(s: &str, max_iter: usize) -> String {
    let mut out = s.to_string();
    for _ in 0..max_iter {
        let brace_delta: i32 = out
            .chars()
            .map(|ch| match ch {
                '{' => 1,
                '}' => -1,
                _ => 0,
            })
            .sum();
        let bracket_delta: i32 = out
            .chars()
            .map(|ch| match ch {
                '[' => 1,
                ']' => -1,
                _ => 0,
            })
            .sum();
        if brace_delta <= 0 && bracket_delta <= 0 {
            break;
        }
        // Append needed closers in reverse order (brackets before braces
        // for correct nesting when both are unbalanced).
        for _ in 0..bracket_delta.max(0) {
            out.push(']');
        }
        for _ in 0..brace_delta.max(0) {
            out.push('}');
        }
    }
    out
}

/// 当增量为负时（闭比开多），去除多余的闭合符号。
fn strip_excess_closers(s: &str) -> String {
    let mut brace_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '}' => {
                if brace_depth > 0 {
                    brace_depth -= 1;
                    out.push(ch);
                }
                // else drop excess closer
            }
            ']' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                    out.push(ch);
                }
            }
            '{' => {
                brace_depth += 1;
                out.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strict_parse_passes_through() {
        let v = repair(r#"{"path": "hello.txt"}"#).unwrap();
        assert_eq!(v, json!({"path": "hello.txt"}));
    }

    #[test]
    fn repairs_trailing_comma() {
        let v = repair(r#"{"path": "hello.txt",}"#).unwrap();
        assert_eq!(v, json!({"path": "hello.txt"}));
    }

    #[test]
    fn repairs_trailing_comma_in_array() {
        let v = repair(r#"["a", "b",]"#).unwrap();
        assert_eq!(v, json!(["a", "b"]));
    }

    #[test]
    fn repairs_missing_close_brace() {
        let v = repair(r#"{"path": "hello.txt""#).unwrap();
        assert_eq!(v, json!({"path": "hello.txt"}));
    }

    #[test]
    fn repairs_missing_close_bracket() {
        let v = repair(r#"["a", "b""#).unwrap();
        assert_eq!(v, json!(["a", "b"]));
    }

    #[test]
    fn strips_embedded_control_chars() {
        // Raw \x0B (vertical tab) inside a string value
        let raw = "{\"key\": \"val\x0Bue\"}";
        let v = repair(raw).unwrap();
        assert_eq!(v, json!({"key": "value"}));
    }

    #[test]
    fn handles_empty_string() {
        let v = repair("").unwrap();
        assert_eq!(v, json!({}));
    }

    #[test]
    fn handles_gibberish() {
        let v = repair("not json at all").unwrap();
        assert_eq!(v, json!({}));
    }

    #[test]
    fn balances_nested_braces() {
        let v = repair(r#"{"outer": {"inner": "val""#).unwrap();
        assert_eq!(v, json!({"outer": {"inner": "val"}}));
    }

    #[test]
    fn strips_excess_closers() {
        let v = repair(r#"{"key": "val"}}"#).unwrap();
        assert_eq!(v, json!({"key": "val"}));
    }

    #[test]
    fn handles_double_encoded_json() {
        // This is a valid JSON string containing a JSON object literal.
        // repair parses it as a string; the engine's existing fallback
        // (parse_tool_input) will unwrap the string and re-parse.
        let v = repair(r#""{\"path\": \"hello.txt\"}""#).unwrap();
        assert_eq!(v, Value::String(r#"{"path": "hello.txt"}"#.to_string()));
    }

    #[test]
    fn oversize_input_rejected() {
        let big = "x".repeat(MAX_ARG_LEN + 1);
        assert!(repair(&big).is_err());
    }

    #[test]
    fn repairs_brace_balance_with_trailing_comma() {
        let v = repair(r#"{"a": 1,"#).unwrap();
        assert_eq!(v, json!({"a": 1}));
    }
}
