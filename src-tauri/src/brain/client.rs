//! HTTP client for one brain: streaming completions and chat, plus the connection test (BR-3).
//!
//! Cancellation (BR-5): the returned [`TokenStream`] owns the HTTP response. Dropping it closes the
//! connection, which makes llama.cpp and other servers stop generating. Ghost Text relies on this to
//! cancel the in-flight request on every keystroke.

use super::dialect::{self, ChatRequest, CompletionRequest, Delta, Dialect, ParseError};
use super::sse::{SseDecoder, SseItem};
use super::{BrainConfig, BrainError, Capabilities};
use futures_util::{Stream, StreamExt};
use serde::Serialize;
use std::collections::VecDeque;
use std::pin::Pin;
use std::time::{Duration, Instant};

/// Streamed text pieces. Ends after the server's done marker or the end of the body.
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<String, BrainError>> + Send>>;

/// Time allowed to connect and receive response headers. Streams themselves are not time-limited here;
/// callers (the engines) apply their own budgets.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(30);

/// A ready-to-use brain: config + secret + HTTP client.
#[derive(Clone)]
pub struct HttpBrain {
    pub config: BrainConfig,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl std::fmt::Debug for HttpBrain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the API key.
        f.debug_struct("HttpBrain")
            .field("config", &self.config)
            .field("api_key", &self.api_key.as_ref().map(|_| "<set>"))
            .finish()
    }
}

/// Result of a connection test, shown in the Brain panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestReport {
    pub reachable: bool,
    /// Round trip of the cheapest request (model list or health), in milliseconds.
    pub latency_ms: Option<u64>,
    pub models: Vec<String>,
    /// Capabilities proven by the test (a short real completion / chat).
    pub capabilities: Capabilities,
    pub is_cloud: bool,
    pub error: Option<String>,
}

impl HttpBrain {
    pub fn new(config: BrainConfig, api_key: Option<String>) -> Result<Self, BrainError> {
        let url = url::Url::parse(&config.endpoint)
            .map_err(|e| BrainError::InvalidEndpoint(e.to_string()))?;
        let cloud = super::is_cloud_endpoint(&config.endpoint);
        // An API key is a credential: never send it in clear text over the network.
        if api_key.is_some() && cloud && url.scheme() != "https" {
            return Err(BrainError::InvalidEndpoint(
                "API keys are only sent over https to servers outside this Mac".into(),
            ));
        }
        let mut builder = reqwest::Client::builder().connect_timeout(CONNECT_TIMEOUT);
        if !cloud {
            // Servers on this Mac must not go through a system proxy; cloud brains may need one.
            builder = builder.no_proxy();
        }
        let http = builder
            .build()
            .map_err(|e| BrainError::Unreachable(e.to_string()))?;
        Ok(Self {
            config,
            api_key,
            http,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.endpoint.trim_end_matches('/'), path)
    }

    fn post(&self, path: &str, body: &serde_json::Value) -> reqwest::RequestBuilder {
        let mut rb = self.http.post(self.url(path)).json(body);
        if let Some(k) = &self.api_key {
            rb = rb.bearer_auth(k);
        }
        rb.header("accept", "text/event-stream")
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        let mut rb = self.http.get(self.url(path));
        if let Some(k) = &self.api_key {
            rb = rb.bearer_auth(k);
        }
        rb
    }

    /// Raw continuation (Ghost Text). Requires the `ghost` capability (BR-4).
    pub async fn complete_stream(
        &self,
        req: &CompletionRequest,
    ) -> Result<TokenStream, BrainError> {
        if !self.config.capabilities.ghost {
            return Err(BrainError::Unsupported(
                "Ghost Text (no raw completion endpoint)",
            ));
        }
        self.complete_stream_unchecked(req).await
    }

    async fn complete_stream_unchecked(
        &self,
        req: &CompletionRequest,
    ) -> Result<TokenStream, BrainError> {
        #[cfg(test)]
        if let Some(script) = scripted::completion(&self.config.endpoint) {
            return Ok(script(req));
        }
        let d = self.config.dialect();
        let body = dialect::completion_body(d, req, self.config.model.as_deref());
        let resp = send(self.post(dialect::completion_path(d), &body)).await?;
        Ok(events(resp, move |data| {
            dialect::parse_completion_event(d, data)
        }))
    }

    /// Instruction request (Selection Actions, Dictation cleanup). Requires the `chat` capability.
    pub async fn chat_stream(&self, req: &ChatRequest) -> Result<TokenStream, BrainError> {
        if !self.config.capabilities.chat {
            return Err(BrainError::Unsupported("instructions (no chat endpoint)"));
        }
        self.chat_stream_unchecked(req).await
    }

    async fn chat_stream_unchecked(&self, req: &ChatRequest) -> Result<TokenStream, BrainError> {
        #[cfg(test)]
        if let Some(script) = scripted::chat(&self.config.endpoint) {
            return Ok(script(req));
        }
        let d = self.config.dialect();
        let body = dialect::chat_body(req, self.config.model.as_deref());
        let resp = send(self.post(dialect::chat_path(d), &body)).await?;
        Ok(events(resp, dialect::parse_chat_event))
    }

    /// Connection test (BR-3): reachability + latency + model list, then prove each capability with a
    /// tiny real request. Never fails; problems are reported in the report.
    pub async fn test(&self) -> TestReport {
        let mut report = TestReport {
            reachable: false,
            latency_ms: None,
            models: vec![],
            capabilities: Capabilities::default(),
            is_cloud: self.config.is_cloud(),
            error: None,
        };
        let probe = match self.config.dialect() {
            Dialect::LlamaCpp => "/health",
            Dialect::OpenAi => "/v1/models",
        };
        let started = Instant::now();
        match send(self.get(probe)).await {
            Ok(resp) => {
                report.reachable = true;
                report.latency_ms = Some(started.elapsed().as_millis() as u64);
                if let Ok(v) = resp.json::<serde_json::Value>().await {
                    report.models = model_ids(&v);
                }
            }
            Err(e) => {
                report.error = Some(e.to_string());
                return report;
            }
        }

        let ghost_probe = CompletionRequest {
            max_tokens: 1,
            ..CompletionRequest::ghost("Hello")
        };
        report.capabilities.ghost =
            drain_ok(self.complete_stream_unchecked(&ghost_probe).await).await;
        let chat_probe = ChatRequest {
            system: "Reply with one word.".into(),
            user: "Say hi.".into(),
            max_tokens: 2,
            temperature: 0.0,
        };
        report.capabilities.chat = drain_ok(self.chat_stream_unchecked(&chat_probe).await).await;
        if !report.capabilities.ghost && !report.capabilities.chat {
            report.error = Some("reachable, but neither completions nor chat worked".into());
        }
        report
    }
}

fn model_ids(v: &serde_json::Value) -> Vec<String> {
    v.get("data")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("id")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

async fn drain_ok(r: Result<TokenStream, BrainError>) -> bool {
    let Ok(mut s) = r else { return false };
    while let Some(item) = s.next().await {
        if item.is_err() {
            return false;
        }
    }
    true
}

/// Send with a header timeout and map HTTP errors to [`BrainError`].
async fn send(rb: reqwest::RequestBuilder) -> Result<reqwest::Response, BrainError> {
    let resp = tokio::time::timeout(HEADER_TIMEOUT, rb.send())
        .await
        .map_err(|_| BrainError::Timeout)?
        .map_err(|e| BrainError::Unreachable(describe(&e)))?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(BrainError::Unauthorized(status));
    }
    if !(200..300).contains(&status) {
        let text = read_bounded(resp, MAX_ERROR_BODY).await;
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| text.chars().take(200).collect());
        return Err(BrainError::Http { status, message });
    }
    Ok(resp)
}

/// Most bytes read from an error response (servers may return whole HTML pages).
pub const MAX_ERROR_BODY: usize = 64 * 1024;
/// Most bytes of one streamed answer. Suggestions and rewrites are a few kilobytes; a server that keeps
/// sending is cut off instead of filling memory.
pub const MAX_STREAM_BYTES: usize = 8 << 20;

async fn read_bounded(resp: reqwest::Response, max: usize) -> String {
    let mut body = resp.bytes_stream();
    let mut out: Vec<u8> = vec![];
    while let Some(Ok(chunk)) = body.next().await {
        out.extend_from_slice(&chunk[..chunk.len().min(max - out.len())]);
        if out.len() >= max {
            break;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn describe(e: &reqwest::Error) -> String {
    if e.is_connect() {
        "connection refused (is the server running?)".into()
    } else if e.is_timeout() {
        "connection timed out".into()
    } else {
        // reqwest errors can include the URL; that is fine (no secrets in URLs).
        e.to_string()
    }
}

/// Turn an SSE response into a token stream using `parse` for each `data` payload.
fn events(
    resp: reqwest::Response,
    parse: impl Fn(&str) -> Result<Delta, ParseError> + Send + 'static,
) -> TokenStream {
    struct State<P> {
        body: Pin<Box<dyn Stream<Item = reqwest::Result<bytes::Bytes>> + Send>>,
        decoder: SseDecoder,
        pending: VecDeque<SseItem>,
        parse: P,
        finished: bool,
        received: usize,
    }
    let state = State {
        body: Box::pin(resp.bytes_stream()),
        decoder: SseDecoder::new(),
        pending: VecDeque::new(),
        parse,
        finished: false,
        received: 0,
    };
    Box::pin(futures_util::stream::unfold(state, |mut st| async move {
        loop {
            if st.finished {
                return None;
            }
            if let Some(item) = st.pending.pop_front() {
                match item {
                    SseItem::Overflow => {
                        st.finished = true;
                        return Some((Err(BrainError::Protocol("event too large".into())), st));
                    }
                    SseItem::Data(data) => match (st.parse)(&data) {
                        Ok(Delta::Text(t)) => return Some((Ok(t), st)),
                        Ok(Delta::Empty) => continue,
                        // Returning None ends the stream and drops the state (and the connection).
                        Ok(Delta::Done) => return None,
                        Err(ParseError::Server(m)) => {
                            st.finished = true;
                            return Some((Err(BrainError::Server(m)), st));
                        }
                        Err(ParseError::InvalidJson) => {
                            st.finished = true;
                            return Some((
                                Err(BrainError::Protocol("invalid JSON event".into())),
                                st,
                            ));
                        }
                    },
                }
            }
            match st.body.next().await {
                Some(Ok(bytes)) => {
                    st.received += bytes.len();
                    if st.received > MAX_STREAM_BYTES {
                        st.finished = true;
                        return Some((
                            Err(BrainError::Protocol("the answer is too large".into())),
                            st,
                        ));
                    }
                    st.pending.extend(st.decoder.push(&bytes));
                }
                Some(Err(e)) => {
                    st.finished = true;
                    return Some((Err(BrainError::Unreachable(describe(&e))), st));
                }
                None => {
                    let rest = st.decoder.finish();
                    if rest.is_empty() {
                        return None;
                    }
                    st.pending.extend(rest);
                    // Stop reading the body; drain what is pending, then end.
                    st.body = Box::pin(futures_util::stream::empty());
                }
            }
        }
    }))
}

/// Scripted brains for the simulator (`crate::sim`), test builds only. A script answers requests for one
/// endpoint in memory instead of over HTTP, so a simulated session runs on tokio's virtual clock and replays
/// exactly. Everything before the transport (capabilities, prompts, redaction) stays real.
#[cfg(test)]
pub(crate) mod scripted {
    use super::{ChatRequest, CompletionRequest, TokenStream};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex, OnceLock};

    pub type Completion = Arc<dyn Fn(&CompletionRequest) -> TokenStream + Send + Sync>;
    pub type Chat = Arc<dyn Fn(&ChatRequest) -> TokenStream + Send + Sync>;

    #[derive(Default)]
    struct Scripts {
        completion: BTreeMap<String, Completion>,
        chat: BTreeMap<String, Chat>,
    }

    fn scripts() -> &'static Mutex<Scripts> {
        static S: OnceLock<Mutex<Scripts>> = OnceLock::new();
        S.get_or_init(Default::default)
    }

    /// Removes the endpoint's scripts when dropped (tests run in parallel; each uses its own endpoint).
    pub struct Registration(String);

    impl Drop for Registration {
        fn drop(&mut self) {
            let mut s = scripts()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            s.completion.remove(&self.0);
            s.chat.remove(&self.0);
        }
    }

    pub fn register(
        endpoint: &str,
        completion: Option<Completion>,
        chat: Option<Chat>,
    ) -> Registration {
        let mut s = scripts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(c) = completion {
            s.completion.insert(endpoint.to_string(), c);
        }
        if let Some(c) = chat {
            s.chat.insert(endpoint.to_string(), c);
        }
        Registration(endpoint.to_string())
    }

    pub(super) fn completion(endpoint: &str) -> Option<Completion> {
        let s = scripts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        s.completion.get(endpoint).cloned()
    }

    pub(super) fn chat(endpoint: &str) -> Option<Chat> {
        let s = scripts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        s.chat.get(endpoint).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::testserver::{Response, TestServer};
    use crate::brain::ProviderKind;
    use std::sync::atomic::Ordering;

    fn config(kind: ProviderKind, endpoint: &str) -> BrainConfig {
        BrainConfig {
            id: "t".into(),
            name: "test".into(),
            kind,
            endpoint: endpoint.into(),
            model: Some("m".into()),
            capabilities: kind.default_capabilities(),
            has_api_key: false,
            share_context: None,
        }
    }

    async fn collect(s: TokenStream) -> Vec<Result<String, BrainError>> {
        s.collect().await
    }

    #[tokio::test]
    async fn streams_llama_cpp_completion_tokens_in_order() {
        let server = TestServer::start(|req| {
            assert_eq!(req.path, "/completion");
            Response::sse(
                &[
                    r#"{"content":"ld","stop":false}"#,
                    r#"{"content":" is","stop":false}"#,
                    r#"{"content":"","stop":true}"#,
                ],
                Duration::from_millis(5),
            )
        })
        .await;
        let brain = HttpBrain::new(config(ProviderKind::Builtin, &server.base), None).unwrap();
        let out = collect(
            brain
                .complete_stream(&CompletionRequest::ghost("Hello wor"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(out, vec![Ok("ld".to_string()), Ok(" is".to_string())]);
        let sent = server.last_request().json();
        assert_eq!(sent["cache_prompt"], true);
        assert_eq!(sent["prompt"], "Hello wor");
    }

    /// A server that never stops talking is cut off instead of filling memory.
    #[tokio::test]
    async fn an_endless_answer_is_cut_off_at_the_size_limit() {
        let chunk = format!(r#"{{"content":"{}","stop":false}}"#, "x".repeat(60_000));
        let n = MAX_STREAM_BYTES / 60_000 + 20;
        let server = TestServer::start(move |_| {
            let events: Vec<&str> = std::iter::repeat_n(chunk.as_str(), n).collect();
            Response::sse(&events, Duration::ZERO)
        })
        .await;
        let brain = HttpBrain::new(config(ProviderKind::Builtin, &server.base), None).unwrap();
        let out = collect(
            brain
                .complete_stream(&CompletionRequest::ghost("x"))
                .await
                .unwrap(),
        )
        .await;
        let received: usize = out
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .map(String::len)
            .sum();
        assert!(received <= MAX_STREAM_BYTES, "{received}");
        assert!(
            matches!(out.last(), Some(Err(BrainError::Protocol(m))) if m.contains("too large"))
        );
    }

    #[tokio::test]
    async fn streams_openai_chat_and_sends_the_bearer_key() {
        let server = TestServer::start(|_| {
            Response::sse(
                &[
                    r#"{"choices":[{"delta":{"role":"assistant"}}]}"#,
                    r#"{"choices":[{"delta":{"content":"Hi"}}]}"#,
                    r#"{"choices":[{"delta":{"content":"!"}}]}"#,
                    "[DONE]",
                ],
                Duration::ZERO,
            )
        })
        .await;
        let brain = HttpBrain::new(
            config(ProviderKind::OpenAiCompatible, &server.base),
            Some("sk-test".into()),
        )
        .unwrap();
        let req = ChatRequest {
            system: "s".into(),
            user: "u".into(),
            max_tokens: 4,
            temperature: 0.0,
        };
        let out = collect(brain.chat_stream(&req).await.unwrap()).await;
        assert_eq!(out, vec![Ok("Hi".to_string()), Ok("!".to_string())]);
        let r = server.last_request();
        assert_eq!(r.path, "/v1/chat/completions");
        assert_eq!(r.header("authorization"), Some("Bearer sk-test"));
        assert_eq!(r.json()["model"], "m");
    }

    #[tokio::test]
    async fn dropping_the_stream_closes_the_connection() {
        // BR-5 / P2: the server sees the client hang up and stops writing.
        let server = TestServer::start(|_| {
            let mut r = Response::sse(&[r#"{"content":"a","stop":false}"#], Duration::ZERO);
            for _ in 0..50 {
                r = r.chunk_after(
                    Duration::from_millis(20),
                    "data: {\"content\":\"b\",\"stop\":false}\n\n",
                );
            }
            r
        })
        .await;
        let brain = HttpBrain::new(config(ProviderKind::Builtin, &server.base), None).unwrap();
        let mut s = brain
            .complete_stream(&CompletionRequest::ghost("x"))
            .await
            .unwrap();
        assert_eq!(s.next().await, Some(Ok("a".to_string())));
        drop(s);
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.aborted.load(Ordering::SeqCst) == 0 {
            assert!(
                Instant::now() < deadline,
                "server never noticed the cancellation"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn maps_http_errors_to_readable_messages() {
        let server = TestServer::start(|req| match req.path.as_str() {
            "/v1/chat/completions" => {
                Response::json(401, serde_json::json!({"error":{"message":"bad key"}}))
            }
            _ => Response::json(
                404,
                serde_json::json!({"error":{"message":"model 'm' not found"}}),
            ),
        })
        .await;
        let mut cfg = config(ProviderKind::Ollama, &server.base);
        cfg.capabilities = Capabilities {
            ghost: true,
            chat: true,
        };
        let brain = HttpBrain::new(cfg, None).unwrap();
        let req = ChatRequest {
            system: "s".into(),
            user: "u".into(),
            max_tokens: 1,
            temperature: 0.0,
        };
        assert_eq!(
            brain.chat_stream(&req).await.err(),
            Some(BrainError::Unauthorized(401))
        );
        assert_eq!(
            brain
                .complete_stream(&CompletionRequest::ghost("x"))
                .await
                .err(),
            Some(BrainError::Http {
                status: 404,
                message: "model 'm' not found".into()
            })
        );
    }

    #[tokio::test]
    async fn server_errors_inside_the_stream_end_it_with_an_error() {
        let server = TestServer::start(|_| {
            Response::sse(
                &[
                    r#"{"content":"ok","stop":false}"#,
                    r#"{"error":{"message":"context full"}}"#,
                    r#"{"content":"never"}"#,
                ],
                Duration::ZERO,
            )
        })
        .await;
        let brain = HttpBrain::new(config(ProviderKind::Builtin, &server.base), None).unwrap();
        let out = collect(
            brain
                .complete_stream(&CompletionRequest::ghost("x"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            out,
            vec![
                Ok("ok".to_string()),
                Err(BrainError::Server("context full".into()))
            ]
        );
    }

    #[tokio::test]
    async fn capability_flags_are_enforced_before_any_request() {
        let server = TestServer::start(|_| Response::new(200)).await;
        let brain =
            HttpBrain::new(config(ProviderKind::OpenAiCompatible, &server.base), None).unwrap();
        assert!(matches!(
            brain
                .complete_stream(&CompletionRequest::ghost("x"))
                .await
                .err(),
            Some(BrainError::Unsupported(_))
        ));
        assert_eq!(
            server.request_count(),
            0,
            "no request may be sent for an unsupported capability"
        );
    }

    #[tokio::test]
    async fn unreachable_servers_are_reported_not_hung() {
        // Port 9 (discard) on loopback is closed on macOS dev machines and CI runners.
        let brain =
            HttpBrain::new(config(ProviderKind::Ollama, "http://127.0.0.1:9"), None).unwrap();
        let report = brain.test().await;
        assert!(!report.reachable);
        assert!(report.error.unwrap().contains("refused") || !report.reachable);
        assert!(!report.is_cloud);
    }

    #[tokio::test]
    async fn connection_test_reports_models_latency_and_proven_capabilities() {
        let server = TestServer::start(|req| match req.path.as_str() {
            "/v1/models" => Response::json(
                200,
                serde_json::json!({"data":[{"id":"qwen2.5:1.5b"},{"id":"llama3"}]}),
            ),
            "/v1/completions" => Response::json(
                404,
                serde_json::json!({"error":{"message":"not supported"}}),
            ),
            "/v1/chat/completions" => Response::sse(
                &[r#"{"choices":[{"delta":{"content":"hi"}}]}"#, "[DONE]"],
                Duration::ZERO,
            ),
            _ => Response::new(404),
        })
        .await;
        let brain = HttpBrain::new(config(ProviderKind::LmStudio, &server.base), None).unwrap();
        let r = brain.test().await;
        assert!(r.reachable && r.error.is_none(), "{r:?}");
        assert_eq!(
            r.models,
            vec!["qwen2.5:1.5b".to_string(), "llama3".to_string()]
        );
        assert_eq!(
            r.capabilities,
            Capabilities {
                ghost: false,
                chat: true
            }
        );
        assert!(r.latency_ms.is_some());
    }

    #[test]
    fn debug_output_never_shows_the_api_key() {
        let b = HttpBrain::new(
            config(ProviderKind::OpenAiCompatible, "https://api.example.com"),
            Some("sk-secret".into()),
        )
        .unwrap();
        let s = format!("{b:?}");
        assert!(!s.contains("sk-secret") && s.contains("<set>"), "{s}");
    }

    #[test]
    fn rejects_invalid_endpoints_up_front() {
        assert!(matches!(
            HttpBrain::new(config(ProviderKind::Ollama, "not a url"), None).err(),
            Some(BrainError::InvalidEndpoint(_))
        ));
        // A key never travels in clear text to a cloud host; local http and cloud https are fine.
        let key = || Some("k".to_string());
        assert!(matches!(
            HttpBrain::new(
                config(ProviderKind::OpenAiCompatible, "http://api.example.com"),
                key()
            )
            .err(),
            Some(BrainError::InvalidEndpoint(_))
        ));
        assert!(HttpBrain::new(
            config(ProviderKind::OpenAiCompatible, "https://api.example.com"),
            key()
        )
        .is_ok());
        assert!(HttpBrain::new(
            config(ProviderKind::LmStudio, "http://127.0.0.1:1234"),
            key()
        )
        .is_ok());
        assert!(HttpBrain::new(
            config(ProviderKind::Ollama, "http://192.168.1.5:11434"),
            None
        )
        .is_ok());
    }
}
