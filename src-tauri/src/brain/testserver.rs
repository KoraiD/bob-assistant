//! Minimal HTTP/1.1 test server for the Brain layer tests (no network, no extra crates).
//!
//! A handler maps a parsed request to a response made of timed chunks. The server records whether the
//! client hung up before the response finished, which is how cancellation (BR-5) is verified.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct Request {
    #[allow(dead_code)] // kept for debugging failed tests
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// Body chunks, each written after its delay.
    pub chunks: Vec<(Duration, Vec<u8>)>,
}

impl Response {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: vec![],
            chunks: vec![],
        }
    }
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub fn body(mut self, b: impl Into<Vec<u8>>) -> Self {
        self.chunks.push((Duration::ZERO, b.into()));
        self
    }
    pub fn chunk_after(mut self, delay: Duration, b: impl Into<Vec<u8>>) -> Self {
        self.chunks.push((delay, b.into()));
        self
    }
    pub fn json(status: u16, v: serde_json::Value) -> Self {
        Self::new(status)
            .header("content-type", "application/json")
            .body(v.to_string())
    }
    /// An SSE response with one `data:` event per item.
    pub fn sse(events: &[&str], gap: Duration) -> Self {
        let mut r = Self::new(200).header("content-type", "text/event-stream");
        for e in events {
            r = r.chunk_after(gap, format!("data: {e}\n\n"));
        }
        r
    }
}

type Handler = Arc<dyn Fn(&Request) -> Response + Send + Sync>;

pub struct TestServer {
    pub base: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
    /// Responses whose client disconnected before all chunks were written.
    pub aborted: Arc<AtomicUsize>,
}

impl TestServer {
    pub async fn start(handler: impl Fn(&Request) -> Response + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handler: Handler = Arc::new(handler);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let aborted = Arc::new(AtomicUsize::new(0));
        let (reqs, ab) = (requests.clone(), aborted.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let (handler, reqs, ab) = (handler.clone(), reqs.clone(), ab.clone());
                tokio::spawn(async move {
                    let Some(req) = read_request(&mut sock).await else {
                        return;
                    };
                    reqs.lock().unwrap().push(req.clone());
                    let resp = handler(&req);
                    let mut head = format!("HTTP/1.1 {} X\r\nconnection: close\r\n", resp.status);
                    for (k, v) in &resp.headers {
                        head.push_str(&format!("{k}: {v}\r\n"));
                    }
                    head.push_str("\r\n");
                    if sock.write_all(head.as_bytes()).await.is_err() {
                        ab.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                    for (delay, bytes) in resp.chunks {
                        tokio::time::sleep(delay).await;
                        if sock.write_all(&bytes).await.is_err() || sock.flush().await.is_err() {
                            ab.fetch_add(1, Ordering::SeqCst);
                            return;
                        }
                    }
                    let _ = sock.shutdown().await;
                });
            }
        });
        Self {
            base,
            requests,
            aborted,
        }
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
    pub fn last_request(&self) -> Request {
        self.requests
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("no request")
    }
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<Request> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[header_end..].to_vec();
    while body.len() < len {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}
