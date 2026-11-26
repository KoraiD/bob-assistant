//! BOB for AI agents (proposal 17): a Model Context Protocol server.
//!
//! Two halves, so an agent never gets more than BOB's own UI allows:
//! - `bob-mcp` (src/bin/bob-mcp.rs) is the stdio MCP server an agent launches (Kiro, Claude Desktop, any MCP
//!   client). It speaks JSON-RPC 2.0 and forwards tool calls to the running app.
//! - The app listens on a control socket next to the helper socket (0600 in a 0700 folder, same-user peers
//!   only) **only while Settings > AI agents is on** (off by default). Every call is audited, rate limited
//!   and runs through the same brains, redaction and cloud rules as the hotkeys.
//!
//! Wire format on the control socket: one JSON object per line. Request `{"id":1,"tool":"bob_rewrite",
//! "args":{...}}`, response `{"id":1,"ok":true,"result":...}` or `{"id":1,"ok":false,"error":"..."}`.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// MCP revision this server implements (clients may ask for older ones; the tool surface is the same).
pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const CONTROL_SOCKET: &str = "control.sock";
/// Most calls per connection per minute.
pub const RATE_PER_MINUTE: usize = 60;
pub const MAX_REQUEST_BYTES: usize = 256 * 1024;

/// Next to the helper socket in the profile's runtime folder (`paths`, so the `bob-mcp` bridge follows
/// `BOB_HOME` too); `BOB_CONTROL_SOCKET` overrides it (tests).
pub fn control_socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("BOB_CONTROL_SOCKET") {
        return PathBuf::from(p);
    }
    crate::paths::runtime_dir().join(CONTROL_SOCKET)
}

pub const INSTRUCTIONS: &str = "BOB is a privacy-first writing assistant running on this Mac. Use bob_rewrite \
to fix, rephrase, shorten, translate or otherwise rewrite text with the user's chosen brain and writing style, \
and bob_complete to get BOB's next-words suggestion for a text. bob_status shows which brains and features are \
on. Text you send follows the user's rules: it stays on this Mac unless the user picked a cloud brain, and \
credential-like text is never sent to a cloud brain.";

/// The tools, as MCP `tools/list` returns them.
pub fn tools() -> Value {
    let text =
        json!({"type": "string", "description": "The text to work on (at most 8,000 characters)."});
    json!([
        {
            "name": "bob_status",
            "title": "BOB status",
            "description": "Which features are on, whether BOB is paused, and the brains (local or cloud) with their roles.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "bob_list_actions",
            "title": "List rewrite actions",
            "description": "The Selection Actions BOB offers (built-in and the user's own), usable as `action` in bob_rewrite.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "bob_rewrite",
            "title": "Rewrite text",
            "description": "Rewrite text with BOB's Selection Actions brain and the user's writing style. Give `action` (a label from bob_list_actions, for example \"Fix grammar\") or a free `instruction`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": text,
                    "action": {"type": "string", "description": "Label of a Selection Action, for example \"Fix grammar\" or \"Make shorter\"."},
                    "instruction": {"type": "string", "description": "A free instruction, used when no action is given, for example \"make it sound friendlier\"."}
                },
                "required": ["text"]
            },
            "annotations": {"readOnlyHint": true, "openWorldHint": true}
        },
        {
            "name": "bob_complete",
            "title": "Suggest next words",
            "description": "BOB's Ghost Text suggestion for what comes after `text` (a few words, or nothing when BOB is unsure).",
            "inputSchema": {"type": "object", "properties": {"text": text}, "required": ["text"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": true}
        },
        {
            "name": "bob_pause",
            "title": "Pause BOB",
            "description": "Pause every BOB feature for an hour, until tomorrow morning, or until resumed (for example while recording the screen).",
            "inputSchema": {
                "type": "object",
                "properties": {"for": {"type": "string", "enum": ["hour", "tomorrow", "indefinite"]}},
                "required": ["for"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true}
        },
        {
            "name": "bob_resume",
            "title": "Resume BOB",
            "description": "End a pause.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true}
        },
        {
            "name": "bob_set_feature",
            "title": "Turn a feature on or off",
            "description": "Turn Ghost Text or Voice Dictation on or off (the same as the switches in BOB).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "feature": {"type": "string", "enum": ["ghost_text", "voice_dictation"]},
                    "enabled": {"type": "boolean"}
                },
                "required": ["feature", "enabled"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true}
        },
        {
            "name": "bob_usage",
            "title": "Usage statistics",
            "description": "How many words BOB typed for the user today, this week and overall (counts only).",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        }
    ])
}

/// Sliding one-minute window.
pub struct RateLimit {
    calls: std::collections::VecDeque<Instant>,
    per_minute: usize,
}

impl RateLimit {
    pub fn new(per_minute: usize) -> Self {
        Self {
            calls: Default::default(),
            per_minute,
        }
    }
    pub fn allow(&mut self, now: Instant) -> bool {
        while self
            .calls
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
        {
            self.calls.pop_front();
        }
        if self.calls.len() >= self.per_minute {
            return false;
        }
        self.calls.push_back(now);
        true
    }
}

// ---------------------------------------------------------------------------------------------------------
// stdio MCP server (bob-mcp)

/// Answer one JSON-RPC message. `call` forwards a tool call to the app. Returns the response to write, or
/// `None` for notifications.
pub async fn handle_rpc<F, Fut>(msg: &Value, call: F) -> Option<Value>
where
    F: FnOnce(String, Value) -> Fut,
    Fut: std::future::Future<Output = Result<Value, String>>,
{
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let reply = |result: Value| Some(json!({"jsonrpc": "2.0", "id": id.clone(), "result": result}));
    let error = |code: i64, message: &str| {
        Some(
            json!({"jsonrpc": "2.0", "id": id.clone(), "error": {"code": code, "message": message}}),
        )
    };
    id.as_ref()?; // notifications (initialized, cancelled) need no answer
    match method {
        "initialize" => {
            let asked = msg
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION);
            let version = if ["2024-11-05", "2025-03-26", "2025-06-18"].contains(&asked) {
                asked
            } else {
                PROTOCOL_VERSION
            };
            reply(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "bob", "title": "BOB (Bring Your Own Brain)", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS
            }))
        }
        "ping" => reply(json!({})),
        "tools/list" => reply(json!({"tools": tools()})),
        "tools/call" => {
            let Some(name) = msg.pointer("/params/name").and_then(Value::as_str) else {
                return error(-32602, "missing tool name");
            };
            if !tools()
                .as_array()
                .is_some_and(|t| t.iter().any(|x| x["name"] == name))
            {
                return error(-32602, &format!("unknown tool {name}"));
            }
            let args = msg
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            // Tool failures are results with isError, so the agent can read and react to them.
            let (text, is_error) = match call(name.to_string(), args).await {
                Ok(v) => (
                    match v {
                        Value::String(s) => s,
                        other => serde_json::to_string_pretty(&other).unwrap_or_default(),
                    },
                    false,
                ),
                Err(e) => (e, true),
            };
            reply(json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
        }
        _ => error(-32601, &format!("method not found: {method}")),
    }
}

/// Forward one tool call to the running app over the control socket.
pub async fn call_app(tool: String, args: Value) -> Result<Value, String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let path = control_socket_path();
    let stream = tokio::net::UnixStream::connect(&path).await.map_err(|_| {
        "BOB is not reachable. Open BOB and turn on Settings > AI agents (MCP).".to_string()
    })?;
    let (r, mut w) = stream.into_split();
    let mut line = serde_json::to_string(&json!({"id": 1, "tool": tool, "args": args}))
        .map_err(|e| e.to_string())?;
    line.push('\n');
    w.write_all(line.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(r);
    let mut answer = String::new();
    tokio::time::timeout(Duration::from_secs(120), reader.read_line(&mut answer))
        .await
        .map_err(|_| "BOB did not answer in time.".to_string())?
        .map_err(|e| e.to_string())?;
    let v: Value =
        serde_json::from_str(&answer).map_err(|_| "BOB sent an invalid answer.".to_string())?;
    if v["ok"] == true {
        Ok(v["result"].clone())
    } else {
        Err(v["error"]
            .as_str()
            .unwrap_or("BOB could not do that.")
            .to_string())
    }
}

/// Run the stdio server until stdin closes (bob-mcp's main).
pub async fn serve_stdio() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(msg) if msg.is_array() => Some(json!({"jsonrpc": "2.0", "id": null,
                "error": {"code": -32600, "message": "batches are not supported"}})),
            Ok(msg) => handle_rpc(&msg, call_app).await,
            Err(_) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}),
            ),
        };
        if let Some(r) = response {
            let mut s = r.to_string();
            s.push('\n');
            if out.write_all(s.as_bytes()).await.is_err() || out.flush().await.is_err() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// The app side: control socket

/// What the control server calls for each request (the app wires it to its engines).
pub type Dispatch = std::sync::Arc<
    dyn Fn(
            String,
            Value,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>>
        + Send
        + Sync,
>;

/// Listen on the control socket until the returned task is aborted. Replaces a stale socket file.
pub fn start_control(dispatch: Dispatch) -> std::io::Result<tauri::async_runtime::JoinHandle<()>> {
    use std::os::unix::fs::PermissionsExt;
    let path = control_socket_path();
    if let Some(dir) = path.parent() {
        crate::fsutil::private_dir(dir)?;
    }
    let _ = std::fs::remove_file(&path);
    let std_listener = std::os::unix::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    std_listener.set_nonblocking(true)?;
    let handle = tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::UnixListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => return log::warn!("mcp: cannot listen: {e}"),
        };
        log::info!("mcp: listening for AI agents");
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let me = unsafe_geteuid();
            if stream.peer_cred().map(|c| c.uid()).ok() != Some(me) {
                crate::audit::record("security.peer_rejected", json!({"socket": "control"}));
                continue;
            }
            tokio::spawn(serve_control(stream, dispatch.clone()));
        }
    });
    Ok(handle)
}

fn unsafe_geteuid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    // SAFETY: no arguments, cannot fail.
    unsafe { geteuid() }
}

async fn serve_control(stream: tokio::net::UnixStream, dispatch: Dispatch) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    let (r, mut w) = stream.into_split();
    let mut reader = BufReader::new(r);
    let mut limit = RateLimit::new(RATE_PER_MINUTE);
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        match (&mut reader)
            .take(MAX_REQUEST_BYTES as u64 + 1)
            .read_until(b'\n', &mut buf)
            .await
        {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&buf).into_owned();
        let too_large = buf.len() > MAX_REQUEST_BYTES;
        let answer = if too_large {
            json!({"id": null, "ok": false, "error": "request too large"})
        } else {
            match serde_json::from_str::<Value>(&line) {
                Err(_) => json!({"id": null, "ok": false, "error": "invalid request"}),
                Ok(req) => {
                    let id = req["id"].clone();
                    let tool = req["tool"].as_str().unwrap_or("").to_string();
                    let args = req.get("args").cloned().unwrap_or_else(|| json!({}));
                    if !limit.allow(Instant::now()) {
                        json!({"id": id, "ok": false, "error": "Too many requests; try again in a minute."})
                    } else {
                        let chars = args["text"].as_str().map_or(0, |t| t.chars().count());
                        let started = Instant::now();
                        let result = dispatch(tool.clone(), args).await;
                        crate::audit::record(
                            "mcp.call",
                            json!({"tool": tool, "chars": chars, "ok": result.is_ok(),
                                   "ms": started.elapsed().as_millis() as u64}),
                        );
                        match result {
                            Ok(v) => json!({"id": id, "ok": true, "result": v}),
                            Err(e) => json!({"id": id, "ok": false, "error": e}),
                        }
                    }
                }
            }
        };
        let mut s = answer.to_string();
        s.push('\n');
        if w.write_all(s.as_bytes()).await.is_err() || too_large {
            break; // an oversized line cannot be resynchronized: close
        }
    }
}

/// Stop listening and remove the socket file.
pub fn stop_control(handle: Option<tauri::async_runtime::JoinHandle<()>>) {
    if let Some(h) = handle {
        h.abort();
        log::info!("mcp: stopped");
    }
    let _ = std::fs::remove_file(control_socket_path());
}

// ---------------------------------------------------------------------------------------------------------
// The app's tool implementations

/// Tool implementations backed by the app's engines.
pub fn app_dispatch(app: tauri::AppHandle) -> Dispatch {
    std::sync::Arc::new(move |tool: String, args: Value| {
        let app = app.clone();
        Box::pin(async move { run_tool(&app, &tool, &args).await })
    })
}

fn text_arg(args: &Value) -> Result<String, String> {
    args.get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "`text` is required.".to_string())
}

async fn run_tool(app: &tauri::AppHandle, tool: &str, args: &Value) -> Result<Value, String> {
    use crate::dictation::engine::DictationHandle;
    use crate::ghost::engine::GhostHandle;
    use crate::selection::engine::SelectionHandle;
    use tauri::Manager;
    let brains = app.state::<crate::commands::Brains>().inner().clone();
    let context = app.state::<crate::commands::Context>().inner().clone();
    let paused = app
        .try_state::<std::sync::Arc<crate::pause::Pause>>()
        .is_some_and(|p| p.is_paused());
    let menu = || {
        let s = app.state::<SelectionHandle>().settings();
        crate::selection::actions::menu(&s, &context.settings().language)
    };
    match tool {
        "bob_status" => {
            let snap = brains.snapshot().await;
            let ghost = app.state::<GhostHandle>().settings().enabled;
            let dictation = app.state::<DictationHandle>().settings().enabled;
            let brains: Vec<Value> = snap
                .brains
                .iter()
                .map(|b| {
                    json!({"id": b.config.id, "name": b.config.name,
                           "where": if b.config.id == crate::brain::registry::BUILTIN_ID { "this Mac" }
                                    else if b.is_cloud { "cloud" } else { "local" }})
                })
                .collect();
            Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "paused": paused,
                "features": {"ghost_text": ghost && !paused, "selection_actions": !paused, "voice_dictation": dictation && !paused},
                "roles": snap.roles,
                "brains": brains,
            }))
        }
        "bob_list_actions" => Ok(json!(menu()
            .into_iter()
            .map(|a| a.label)
            .collect::<Vec<_>>())),
        "bob_rewrite" => {
            if paused {
                return Err("BOB is paused. Resume it first (bob_resume).".into());
            }
            let text = text_arg(args)?;
            let action = match (args["action"].as_str(), args["instruction"].as_str()) {
                (Some(label), _) => menu()
                    .into_iter()
                    .find(|a| a.label.eq_ignore_ascii_case(label.trim()))
                    .ok_or_else(|| format!("Unknown action {label:?}; see bob_list_actions."))?,
                (None, Some(i)) if !i.trim().is_empty() => {
                    crate::selection::engine::voice_action(i)
                }
                _ => return Err("Give an `action` or an `instruction`.".into()),
            };
            crate::selection::engine::rewrite(&brains, &context, &action, &text)
                .await
                .map(Value::String)
        }
        "bob_complete" => {
            if paused {
                return Err("BOB is paused. Resume it first (bob_resume).".into());
            }
            let text = text_arg(args)?;
            if text.chars().count() > crate::selection::engine::MAX_CHARS {
                return Err("The text is too long.".into());
            }
            let ghost = app.state::<GhostHandle>();
            let memory = if context.settings().ghost.memory {
                ghost
                    .memory()
                    .most_used(crate::context::GHOST_MEMORY_PHRASES)
            } else {
                vec![]
            };
            let s = crate::ghost::engine::suggest_once(&brains, &context, &memory, &text).await?;
            Ok(json!({ "suggestion": s }))
        }
        "bob_pause" => {
            let how = match args["for"].as_str() {
                Some("hour") => crate::pause::PauseFor::Hour,
                Some("tomorrow") => crate::pause::PauseFor::UntilTomorrow,
                Some("indefinite") => crate::pause::PauseFor::Indefinitely,
                _ => return Err("`for` must be hour, tomorrow or indefinite.".into()),
            };
            Ok(json!(crate::shell::set_pause(app, Some(how))))
        }
        "bob_resume" => Ok(json!(crate::shell::set_pause(app, None))),
        "bob_set_feature" => {
            let on = args["enabled"]
                .as_bool()
                .ok_or("`enabled` must be true or false.")?;
            match args["feature"].as_str() {
                Some("ghost_text") => app.state::<GhostHandle>().set_enabled(on),
                Some("voice_dictation") => {
                    let d = app.state::<DictationHandle>();
                    let mut s = d.settings();
                    s.enabled = on;
                    d.set_settings(s).map_err(|e| e.to_string())?;
                    crate::commands::push_dictation(app);
                }
                _ => return Err("`feature` must be ghost_text or voice_dictation.".into()),
            }
            crate::audit::record(
                "settings.feature",
                json!({"feature": args["feature"], "enabled": on, "by": "mcp"}),
            );
            crate::shell::changed(app);
            Ok(json!({"feature": args["feature"], "enabled": on}))
        }
        "bob_usage" => {
            let v = crate::usage::view();
            Ok(json!({
                "today_words": v.today.words_for_you(),
                "week_words": v.week.words_for_you(),
                "all_time_words": v.all_time.words_for_you(),
                "minutes_saved_week": (v.minutes_saved_week * 10.0).round() / 10.0,
                "since": v.since,
            }))
        }
        other => Err(format!("unknown tool {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn rpc(msg: Value) -> Option<Value> {
        handle_rpc(&msg, |tool, args| async move {
            match tool.as_str() {
                "bob_status" => Ok(json!({"paused": false})),
                _ => Err(format!("{tool} failed for {}", args["text"])),
            }
        })
        .await
    }

    #[tokio::test]
    async fn speaks_mcp_initialize_list_and_call() {
        let init = rpc(json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"kiro","version":"1"}}}))
        .await
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "bob");
        assert!(init["result"]["capabilities"]["tools"].is_object());
        assert!(
            rpc(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
                .await
                .is_none()
        );
        let list = rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .await
            .unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"bob_rewrite") && names.contains(&"bob_complete"));
        for t in list["result"]["tools"].as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object", "{t}");
        }
        let ok = rpc(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"bob_status","arguments":{}}}))
            .await
            .unwrap();
        assert_eq!(ok["result"]["isError"], false);
        assert!(ok["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("paused"));
        let failed = rpc(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"bob_rewrite","arguments":{"text":"x"}}}))
            .await
            .unwrap();
        assert_eq!(failed["result"]["isError"], true);
        let unknown =
            rpc(json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"rm_rf"}}))
                .await
                .unwrap();
        assert_eq!(unknown["error"]["code"], -32602);
        let missing = rpc(json!({"jsonrpc":"2.0","id":6,"method":"resources/list"}))
            .await
            .unwrap();
        assert_eq!(missing["error"]["code"], -32601);
    }

    /// bob-mcp's forwarding and the app's control socket, end to end over a real socket.
    #[tokio::test(flavor = "multi_thread")]
    async fn tool_calls_reach_the_app_over_the_control_socket() {
        let dir = std::env::temp_dir().join(format!("bob-mcp-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BOB_CONTROL_SOCKET", dir.join("control.sock"));
        let dispatch: Dispatch = std::sync::Arc::new(|tool, args| {
            Box::pin(async move {
                match tool.as_str() {
                    "bob_complete" => Ok(
                        json!({"suggestion": format!("{} soon", args["text"].as_str().unwrap_or(""))}),
                    ),
                    _ => Err("nope".to_string()),
                }
            })
        });
        let h = start_control(dispatch).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(control_socket_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let ok = call_app("bob_complete".into(), json!({"text": "see you"}))
            .await
            .unwrap();
        assert_eq!(ok["suggestion"], "see you soon");
        assert_eq!(
            call_app("bob_status".into(), json!({})).await.unwrap_err(),
            "nope"
        );
        let calls = crate::audit::recent(Some("mcp.call"), 10);
        assert!(calls
            .iter()
            .any(|e| e.detail["tool"] == "bob_complete" && e.detail["chars"] == 7));
        stop_control(Some(h));
        assert!(!control_socket_path().exists());
        let err = call_app("bob_status".into(), json!({})).await.unwrap_err();
        assert!(err.contains("Settings > AI agents"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Random lines on the control socket: valid and invalid requests, invalid UTF-8, deep nesting, unknown
    /// tools. Every line gets exactly one JSON answer, valid requests past the rate limit are refused, an
    /// oversized line is answered and closes the connection, and the server task ends without a panic.
    #[tokio::test]
    async fn the_control_server_survives_random_input() {
        use rand::{Rng, SeedableRng};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let dispatch: Dispatch = std::sync::Arc::new(|tool, _| {
            Box::pin(async move {
                if tool == "bob_status" {
                    Ok(json!({"paused": false}))
                } else {
                    Err("unknown tool".to_string())
                }
            })
        });
        for seed in 0..20u64 {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let (server_end, client_end) = tokio::net::UnixStream::pair().unwrap();
            let server = tokio::spawn(serve_control(server_end, dispatch.clone()));
            let (r, mut w) = client_end.into_split();
            let mut answers = BufReader::new(r).lines();
            let mut valid = 0usize;
            for i in 0..90 {
                let line: Vec<u8> = match rng.random_range(0..6) {
                    0 => b"{not json".to_vec(),
                    1 => (0..rng.random_range(1..200))
                        .map(|_| rng.random_range(0..=255u8))
                        .filter(|b| *b != b'\n')
                        .collect(),
                    2 => format!("{}{}", "[".repeat(5000), "]".repeat(5000)).into_bytes(),
                    3 => json!({"id": i, "tool": "rm_rf", "args": {"text": "x"}})
                        .to_string()
                        .into_bytes(),
                    _ => json!({"id": i, "tool": "bob_status", "args": {}})
                        .to_string()
                        .into_bytes(),
                };
                let is_valid = serde_json::from_slice::<Value>(&line).is_ok_and(|v| v.is_object());
                w.write_all(&line).await.unwrap();
                w.write_all(b"\n").await.unwrap();
                let answer = tokio::time::timeout(Duration::from_secs(5), answers.next_line())
                    .await
                    .expect("an answer for every line")
                    .unwrap()
                    .expect("the connection stays open");
                let v: Value = serde_json::from_str(&answer).expect("answers are JSON");
                assert!(v["ok"].is_boolean(), "seed {seed}: {answer}");
                if is_valid {
                    valid += 1;
                    let limited = v["error"]
                        .as_str()
                        .is_some_and(|e| e.starts_with("Too many"));
                    assert_eq!(
                        limited,
                        valid > RATE_PER_MINUTE,
                        "seed {seed}, request {valid}: {answer}"
                    );
                }
            }
            let huge = vec![b'a'; MAX_REQUEST_BYTES + 10];
            w.write_all(&huge).await.unwrap();
            w.write_all(b"\n").await.unwrap();
            let answer = answers.next_line().await.unwrap().unwrap();
            assert!(answer.contains("request too large"), "{answer}");
            assert!(
                answers.next_line().await.unwrap().is_none(),
                "closed after an oversized line"
            );
            server.await.expect("the server task never panics");
        }
    }

    proptest::proptest! {
        /// Any JSON-RPC message gets a well-formed answer (or none, for a notification), never a panic.
        #[test]
        fn any_rpc_message_gets_a_well_formed_answer(
            id in proptest::option::of(0i64..1000),
            method in proptest::sample::select(vec!["initialize", "tools/list", "tools/call", "ping",
                "notifications/initialized", "resources/list", "", "tools/call/x"]),
            tool in proptest::sample::select(vec!["bob_status", "bob_rewrite", "rm_rf", ""]),
            params_kind in 0u8..4,
        ) {
            let params = match params_kind {
                0 => json!({"name": tool, "arguments": {"text": "hello"}}),
                1 => json!({"name": tool}),
                2 => json!([1, 2, 3]),
                _ => Value::Null,
            };
            let mut msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
            if let Some(id) = id {
                msg["id"] = json!(id);
            }
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
            let answer = rt.block_on(handle_rpc(&msg, |_, _| async { Ok(json!({"ok": true})) }));
            if let Some(a) = answer {
                proptest::prop_assert_eq!(&a["jsonrpc"], "2.0");
                proptest::prop_assert!(a.get("result").is_some() != a.get("error").is_some(), "{}", a);
            }
        }
    }

    #[test]
    fn the_rate_limit_slides() {
        let mut r = RateLimit::new(2);
        let t = Instant::now();
        assert!(r.allow(t) && r.allow(t));
        assert!(!r.allow(t + Duration::from_secs(30)));
        assert!(r.allow(t + Duration::from_secs(61)));
    }
}
