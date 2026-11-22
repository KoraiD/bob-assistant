//! Request bodies and streamed-response parsing for the two HTTP dialects BOB speaks.
//!
//! * `LlamaCpp`: llama.cpp's native `/completion` endpoint. Used for the built-in sidecar because it
//!   exposes `cache_prompt`, `id_slot` and `t_max_predict_ms`, the levers behind the Ghost Text budget
//!   (see the llama.cpp server README, recorded in docs/PERFORMANCE.md).
//! * `OpenAi`: `/v1/completions` and `/v1/chat/completions`, spoken by Ollama, LM Studio, llama.cpp and
//!   cloud providers.
//!
//! Everything here is pure: building JSON and turning one SSE `data` payload into a [`Delta`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    LlamaCpp,
    OpenAi,
}

/// A raw continuation request (Ghost Text).
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionRequest {
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_k: u32,
    pub stop: Vec<String>,
    /// Hard cap on generation time (llama.cpp `t_max_predict_ms`). Ignored by the OpenAI dialect.
    pub max_predict_ms: Option<u32>,
    /// Pin a llama.cpp slot so its prompt cache is reused between keystrokes.
    pub slot: Option<i32>,
}

impl CompletionRequest {
    /// Defaults tuned for short inline continuations (targets in .kiro/steering/perf-budgets.md).
    pub fn ghost(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            max_tokens: 12,
            temperature: 0.2,
            top_k: 20,
            stop: vec!["\n".into()],
            max_predict_ms: Some(400),
            slot: Some(0),
        }
    }
}

/// An instruction request (Selection Actions, Dictation cleanup).
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub system: String,
    pub user: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// One parsed streaming event.
#[derive(Debug, Clone, PartialEq)]
pub enum Delta {
    /// New text to append.
    Text(String),
    /// The server signalled the end of the stream.
    Done,
    /// A well-formed event that carries no text (role headers, usage, timings).
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("server sent invalid JSON in a stream event")]
    InvalidJson,
    #[error("server reported an error: {0}")]
    Server(String),
}

/// Path of the raw-completion endpoint, relative to the base URL.
pub fn completion_path(d: Dialect) -> &'static str {
    match d {
        Dialect::LlamaCpp => "/completion",
        Dialect::OpenAi => "/v1/completions",
    }
}

pub fn chat_path(_d: Dialect) -> &'static str {
    "/v1/chat/completions"
}

pub fn completion_body(d: Dialect, req: &CompletionRequest, model: Option<&str>) -> Value {
    match d {
        Dialect::LlamaCpp => {
            let mut v = json!({
                "prompt": req.prompt,
                "n_predict": req.max_tokens,
                "temperature": req.temperature,
                "top_k": req.top_k,
                "stop": req.stop,
                "stream": true,
                "cache_prompt": true,
            });
            if let Some(ms) = req.max_predict_ms {
                v["t_max_predict_ms"] = json!(ms);
            }
            if let Some(slot) = req.slot {
                v["id_slot"] = json!(slot);
            }
            v
        }
        Dialect::OpenAi => {
            let mut v = json!({
                "prompt": req.prompt,
                "max_tokens": req.max_tokens,
                "temperature": req.temperature,
                "stop": req.stop,
                "stream": true,
            });
            if let Some(m) = model {
                v["model"] = json!(m);
            }
            v
        }
    }
}

pub fn chat_body(req: &ChatRequest, model: Option<&str>) -> Value {
    let mut v = json!({
        "messages": [
            {"role": "system", "content": req.system},
            {"role": "user", "content": req.user},
        ],
        "max_tokens": req.max_tokens,
        "temperature": req.temperature,
        "stream": true,
    });
    if let Some(m) = model {
        v["model"] = json!(m);
    }
    v
}

fn server_error(v: &Value) -> Option<String> {
    let e = v.get("error")?;
    Some(
        e.get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| e.to_string()),
    )
}

/// Parse one SSE `data` payload from a completion stream.
pub fn parse_completion_event(d: Dialect, data: &str) -> Result<Delta, ParseError> {
    if data.trim() == "[DONE]" {
        return Ok(Delta::Done);
    }
    let v: Value = serde_json::from_str(data).map_err(|_| ParseError::InvalidJson)?;
    if let Some(msg) = server_error(&v) {
        return Err(ParseError::Server(msg));
    }
    match d {
        Dialect::LlamaCpp => {
            let text = v.get("content").and_then(Value::as_str).unwrap_or("");
            if v.get("stop").and_then(Value::as_bool) == Some(true) && text.is_empty() {
                return Ok(Delta::Done);
            }
            Ok(if text.is_empty() {
                Delta::Empty
            } else {
                Delta::Text(text.to_string())
            })
        }
        Dialect::OpenAi => {
            let text = v
                .pointer("/choices/0/text")
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(if text.is_empty() {
                Delta::Empty
            } else {
                Delta::Text(text.to_string())
            })
        }
    }
}

/// Parse one SSE `data` payload from a chat stream.
pub fn parse_chat_event(data: &str) -> Result<Delta, ParseError> {
    if data.trim() == "[DONE]" {
        return Ok(Delta::Done);
    }
    let v: Value = serde_json::from_str(data).map_err(|_| ParseError::InvalidJson)?;
    if let Some(msg) = server_error(&v) {
        return Err(ParseError::Server(msg));
    }
    let text = v
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .unwrap_or("");
    Ok(if text.is_empty() {
        Delta::Empty
    } else {
        Delta::Text(text.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_body_carries_the_latency_levers() {
        let b = completion_body(
            Dialect::LlamaCpp,
            &CompletionRequest::ghost("Hello wor"),
            None,
        );
        assert_eq!(b["prompt"], "Hello wor");
        assert_eq!(b["cache_prompt"], true);
        assert_eq!(b["stream"], true);
        assert_eq!(b["id_slot"], 0);
        assert_eq!(b["t_max_predict_ms"], 400);
        assert_eq!(b["n_predict"], 12);
        assert_eq!(b["stop"], json!(["\n"]));
        assert!(b.get("model").is_none());
    }

    #[test]
    fn openai_body_uses_standard_fields_only() {
        let b = completion_body(
            Dialect::OpenAi,
            &CompletionRequest::ghost("x"),
            Some("qwen"),
        );
        assert_eq!(b["model"], "qwen");
        assert_eq!(b["max_tokens"], 12);
        for llama_only in [
            "cache_prompt",
            "id_slot",
            "t_max_predict_ms",
            "n_predict",
            "top_k",
        ] {
            assert!(
                b.get(llama_only).is_none(),
                "{llama_only} must not leak into OpenAI requests"
            );
        }
    }

    #[test]
    fn chat_body_has_system_then_user() {
        let b = chat_body(
            &ChatRequest {
                system: "S".into(),
                user: "U".into(),
                max_tokens: 5,
                temperature: 0.3,
            },
            None,
        );
        assert_eq!(b["messages"][0], json!({"role":"system","content":"S"}));
        assert_eq!(b["messages"][1], json!({"role":"user","content":"U"}));
        assert!(b.get("model").is_none());
    }

    #[test]
    fn parses_llama_stream_events() {
        let p = |s| parse_completion_event(Dialect::LlamaCpp, s);
        assert_eq!(
            p(r#"{"content":"ld","stop":false}"#),
            Ok(Delta::Text("ld".into()))
        );
        assert_eq!(
            p(r#"{"content":"","stop":true,"timings":{}}"#),
            Ok(Delta::Done)
        );
        assert_eq!(p(r#"{"content":"","stop":false}"#), Ok(Delta::Empty));
    }

    #[test]
    fn parses_openai_stream_events() {
        assert_eq!(
            parse_completion_event(
                Dialect::OpenAi,
                r#"{"choices":[{"text":" there","index":0}]}"#
            ),
            Ok(Delta::Text(" there".into()))
        );
        assert_eq!(
            parse_completion_event(Dialect::OpenAi, "[DONE]"),
            Ok(Delta::Done)
        );
        assert_eq!(
            parse_chat_event(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#),
            Ok(Delta::Empty)
        );
        assert_eq!(
            parse_chat_event(r#"{"choices":[{"delta":{"content":"Hi"}}]}"#),
            Ok(Delta::Text("Hi".into()))
        );
    }

    #[test]
    fn surfaces_server_errors_and_invalid_json() {
        assert_eq!(
            parse_chat_event(r#"{"error":{"message":"model not found"}}"#),
            Err(ParseError::Server("model not found".into()))
        );
        assert_eq!(parse_chat_event("{nope"), Err(ParseError::InvalidJson));
    }
}
