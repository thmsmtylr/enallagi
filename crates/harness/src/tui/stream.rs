//! stream: turns one `stage.output` chunk (raw process stdout, possibly
//! several stream-json lines batched together since `agent::spawn` streams
//! at no more than 1/s) into the lines the output pane shows.
//!
//! `claude`, `gemini`, `qwen`, `amp` and `cursor` all emit one JSON object
//! per line on stdout. A line that parses and carries a recognised shape is
//! rendered as `tool  <name>  <input>` or `text  <first line>`; anything
//! else -- unparseable JSON, a shape we don't know, plain text -- is shown
//! raw, verbatim.

use serde_json::Value;

/// Splits one chunk into its lines and renders each independently.
pub fn parse_chunk(chunk: &str) -> Vec<String> {
    chunk.lines().map(render_line).collect()
}

/// True if any line of the chunk is a `type: assistant` (or `message`)
/// stream-json event -- used to count agent turns for the output pane's
/// title.
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

fn render_line(line: &str) -> String {
    let Some(v) = parse_object(line) else {
        return line.to_string();
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
        "tool_use" | "tool_result" | "text" => render_item(&v),
        _ => None,
    };
    match rendered {
        Some(s) if !s.is_empty() => s,
        _ => line.to_string(),
    }
}

/// Renders one content item -- either an entry of `message.content[]`, or a
/// top-level gemini/qwen-shaped object with the same `type`/fields.
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
        _ => None,
    }
}

fn first_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
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
