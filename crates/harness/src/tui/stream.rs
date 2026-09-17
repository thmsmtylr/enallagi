//! Turns one `stage.output` chunk into the lines the output pane shows: a recognised JSON shape renders as `tool`/`text`, anything else shows raw.

use serde_json::Value;

pub fn parse_chunk(chunk: &str) -> Vec<String> {
    chunk.lines().filter_map(render_line).collect()
}

pub fn is_assistant_chunk(chunk: &str) -> bool {
    chunk.lines().any(|line| {
        parse_object(line)
            .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string))
            .is_some_and(|ty| ty == "assistant" || ty == "message")
    })
}

fn parse_object(line: &str) -> Option<Value> {
    serde_json::from_str::<Value>(line).ok()
}

// A recognised message with nothing to show renders as nothing, never as raw JSON.
fn render_line(line: &str) -> Option<String> {
    let Some(v) = parse_object(line) else {
        return Some(line.to_string());
    };
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("");
    let rendered = match ty {
        "assistant" | "message" => {
            v.pointer("/message/content")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(render_item)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
        }
        "tool_use" | "tool_result" | "text" | "thinking" => render_item(&v).or(Some(String::new())),
        _ => None,
    };
    match rendered {
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
        None => Some(line.to_string()),
    }
}

// either an entry of message.content[], or a top-level gemini/qwen-shaped object with the same fields
fn render_item(item: &Value) -> Option<String> {
    match item.get("type").and_then(Value::as_str)? {
        "tool_use" => {
            let name = item.get("name").and_then(Value::as_str).unwrap_or("?");
            let input = item.get("input").map(Value::to_string).unwrap_or_default();
            Some(format!("tool  {name}  {}", first_chars(&input, 60)))
        }
        "tool_result" => {
            let content = item
                .get("content")
                .map(Value::to_string)
                .unwrap_or_default();
            Some(format!("tool  result  {}", first_chars(&content, 60)))
        }
        "text" => {
            let text = item.get("text").and_then(Value::as_str).unwrap_or("");
            Some(format!("text  {}", text.lines().next().unwrap_or("")))
        }
        "thinking" => {
            let text = item.get("thinking").and_then(Value::as_str).unwrap_or("");
            let first = text.lines().find(|l| !l.trim().is_empty())?;
            Some(format!("think {}", first_chars(first, 60)))
        }
        _ => None,
    }
}

fn first_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_thinking_only_message_renders_nothing() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"","signature":"abc"}]}}"#;
        assert_eq!(parse_chunk(line), Vec::<String>::new());
    }

    #[test]
    fn thinking_with_text_renders_both() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"weighing the two\nmore"},{"type":"text","text":"done"}]}}"#;
        assert_eq!(
            parse_chunk(line),
            vec!["think weighing the two\ntext  done".to_string()]
        );
    }
    use super::*;

    #[test]
    fn assistant_tool_use_renders_tool_line() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"path":"src/schema.ts"}}]}}"#;
        let out = parse_chunk(line);
        assert_eq!(out.len(), 1);
        assert!(out[0].starts_with("tool  Read  "));
        assert!(out[0].contains("schema.ts"));
    }

    #[test]
    fn assistant_text_renders_first_line_only() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"line one\nline two"}]}}"#;
        let out = parse_chunk(line);
        assert_eq!(out[0], "text  line one");
    }

    #[test]
    fn gemini_shaped_tool_use_renders_the_same_way() {
        let line = r#"{"type":"tool_use","name":"Bash","input":{"command":"bun run check"}}"#;
        let out = parse_chunk(line);
        assert!(out[0].starts_with("tool  Bash  "));
    }

    #[test]
    fn unparseable_and_unknown_shapes_are_raw() {
        assert_eq!(parse_chunk("not json")[0], "not json");
        assert_eq!(parse_chunk(r#"{"type":"other"}"#)[0], r#"{"type":"other"}"#);
    }

    #[test]
    fn multi_line_chunk_renders_one_line_per_input_line() {
        let chunk = "not json\n{\"type\":\"other\"}";
        assert_eq!(parse_chunk(chunk).len(), 2);
    }

    #[test]
    fn assistant_chunk_is_detected_for_turn_counting() {
        assert!(is_assistant_chunk(
            r#"{"type":"assistant","message":{"content":[]}}"#
        ));
        assert!(!is_assistant_chunk("not json"));
    }
}
