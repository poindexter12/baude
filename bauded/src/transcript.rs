//! Transcript JSONL → chat messages: the read-side contract documented in
//! docs/remote-daemon-plan.md. Typed user prompts, assistant text, and
//! compact tool-call summaries; thinking blocks, tool results, sidechain
//! (subagent) traffic, and injected context are filtered out.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Text,
    ToolUse,
}

#[derive(Serialize, Debug, Clone)]
pub struct ChatMessage {
    /// Record uuid; an assistant record emitting several blocks suffixes the
    /// extras with `#1`, `#2`, … so every message id stays unique.
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub role: Role,
    pub kind: Kind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Parse one transcript line into zero or more chat messages.
pub fn parse_line(line: &str) -> Vec<ChatMessage> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    if v["isSidechain"].as_bool() == Some(true) || v["isMeta"].as_bool() == Some(true) {
        return vec![];
    }
    let Some(uuid) = v["uuid"].as_str().map(str::to_string) else {
        return vec![];
    };
    let parent_uuid = v["parentUuid"].as_str().map(str::to_string);
    let timestamp = v["timestamp"].as_str().map(str::to_string);

    match v["type"].as_str() {
        Some("user") => {
            // Typed prompts have string content; tool results arrive as a
            // list of tool_result blocks and aren't chat.
            let Some(text) = v["message"]["content"].as_str() else {
                return vec![];
            };
            vec![ChatMessage {
                uuid,
                parent_uuid,
                timestamp,
                role: Role::User,
                kind: Kind::Text,
                text: text.to_string(),
                model: None,
            }]
        }
        Some("assistant") => {
            let model = v["message"]["model"].as_str().map(str::to_string);
            let Some(blocks) = v["message"]["content"].as_array() else {
                return vec![];
            };
            let mut out = Vec::new();
            for block in blocks {
                let (kind, text) = match block["type"].as_str() {
                    Some("text") => {
                        let t = block["text"].as_str().unwrap_or("");
                        if t.trim().is_empty() {
                            continue;
                        }
                        (Kind::Text, t.to_string())
                    }
                    Some("tool_use") => (Kind::ToolUse, tool_summary(block)),
                    // thinking, etc.
                    _ => continue,
                };
                let n = out.len();
                out.push(ChatMessage {
                    uuid: if n == 0 {
                        uuid.clone()
                    } else {
                        format!("{uuid}#{n}")
                    },
                    parent_uuid: parent_uuid.clone(),
                    timestamp: timestamp.clone(),
                    role: Role::Assistant,
                    kind,
                    text,
                    model: model.clone(),
                });
            }
            out
        }
        _ => vec![],
    }
}

/// `ToolName(most informative arg)`, single line, truncated.
fn tool_summary(block: &Value) -> String {
    const ARG_KEYS: [&str; 9] = [
        "file_path",
        "path",
        "command",
        "pattern",
        "query",
        "prompt",
        "url",
        "skill",
        "description",
    ];
    const MAX_ARG: usize = 120;
    let name = block["name"].as_str().unwrap_or("tool");
    let input = &block["input"];
    let arg = ARG_KEYS.iter().find_map(|k| input[k].as_str()).or_else(|| {
        input
            .as_object()
            .and_then(|o| o.values().find_map(|v| v.as_str()))
    });
    match arg {
        Some(a) => {
            let a = a.replace('\n', " ");
            let a: String = if a.chars().count() > MAX_ARG {
                a.chars().take(MAX_ARG - 1).collect::<String>() + "…"
            } else {
                a
            };
            format!("{name}({a})")
        }
        None => format!("{name}()"),
    }
}

/// Parse a whole transcript file.
pub fn parse_file(path: &Path) -> Vec<ChatMessage> {
    fs::read_to_string(path)
        .map(|s| s.lines().flat_map(parse_line).collect())
        .unwrap_or_default()
}

/// Drop everything up to and including the message with uuid `after`.
/// An unknown cursor returns the full list — clients dedupe by uuid.
pub fn after(mut messages: Vec<ChatMessage>, after: &str) -> Vec<ChatMessage> {
    match messages.iter().position(|m| m.uuid == after) {
        Some(i) => messages.split_off(i + 1),
        None => messages,
    }
}

/// Incremental transcript tail for SSE: tracks a byte offset, consumes only
/// complete lines (a partial trailing line is re-read next poll).
#[derive(Default)]
pub struct Tail {
    offset: u64,
}

impl Tail {
    /// Start at the current end of `path` — history is served by the
    /// non-streaming endpoint; the stream only carries what happens next.
    pub fn end_of(path: &Path) -> Tail {
        Tail {
            offset: fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        }
    }

    pub fn read_new(&mut self, path: &Path) -> Vec<ChatMessage> {
        let Ok(mut f) = fs::File::open(path) else {
            return vec![];
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // Truncated/replaced file — start over.
            self.offset = 0;
        }
        if len == self.offset || f.seek(SeekFrom::Start(self.offset)).is_err() {
            return vec![];
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            return vec![];
        }
        let consumed = match buf.rfind('\n') {
            Some(i) => i + 1,
            None => return vec![],
        };
        let messages = buf[..consumed].lines().flat_map(parse_line).collect();
        self.offset += consumed as u64;
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn typed_user_prompt() {
        let line = r#"{"type":"user","uuid":"u1","parentUuid":null,"timestamp":"2026-06-11T10:00:00Z","message":{"role":"user","content":"fix the bug"},"promptSource":"typed"}"#;
        let msgs = parse_line(line);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].text, "fix the bug");
        assert_eq!(msgs[0].uuid, "u1");
    }

    #[test]
    fn meta_and_sidechain_skipped() {
        let meta =
            r#"{"type":"user","uuid":"u2","isMeta":true,"message":{"content":"<injected>"}}"#;
        let side = r#"{"type":"assistant","uuid":"a9","isSidechain":true,"message":{"content":[{"type":"text","text":"subagent"}]}}"#;
        assert!(parse_line(meta).is_empty());
        assert!(parse_line(side).is_empty());
    }

    #[test]
    fn tool_results_skipped() {
        let line = r#"{"type":"user","uuid":"u3","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#;
        assert!(parse_line(line).is_empty());
    }

    #[test]
    fn assistant_text_and_tool_use() {
        let line = r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","message":{"model":"claude-fable-5","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"Looking at it."},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/tmp/x.rs"}}],"usage":{}}}"#;
        let msgs = parse_line(line);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].uuid, "a1");
        assert_eq!(msgs[0].kind, Kind::Text);
        assert_eq!(msgs[0].model.as_deref(), Some("claude-fable-5"));
        assert_eq!(msgs[1].uuid, "a1#1");
        assert_eq!(msgs[1].kind, Kind::ToolUse);
        assert_eq!(msgs[1].text, "Read(/tmp/x.rs)");
    }

    #[test]
    fn garbage_and_other_types() {
        assert!(parse_line("not json").is_empty());
        assert!(parse_line(r#"{"type":"file-history-snapshot","uuid":"x"}"#).is_empty());
    }

    #[test]
    fn after_cursor() {
        let lines = [
            r#"{"type":"user","uuid":"u1","message":{"content":"one"}}"#,
            r#"{"type":"user","uuid":"u2","message":{"content":"two"}}"#,
        ];
        let msgs: Vec<_> = lines.iter().flat_map(|l| parse_line(l)).collect();
        let rest = after(msgs.clone(), "u1");
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].uuid, "u2");
        // unknown cursor → everything
        assert_eq!(after(msgs, "nope").len(), 2);
    }

    #[test]
    fn tail_consumes_only_complete_lines() {
        let dir = std::env::temp_dir().join(format!("bauded-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"{{"type":"user","uuid":"u1","message":{{"content":"one"}}}}"#
        )
        .unwrap();
        write!(f, r#"{{"type":"user","uuid":"u2","#).unwrap(); // partial
        f.flush().unwrap();

        let mut tail = Tail::default();
        let first = tail.read_new(&path);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].uuid, "u1");

        write!(f, r#""message":{{"content":"two"}}}}"#).unwrap();
        writeln!(f).unwrap();
        f.flush().unwrap();
        let second = tail.read_new(&path);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].uuid, "u2");

        std::fs::remove_dir_all(&dir).ok();
    }
}
