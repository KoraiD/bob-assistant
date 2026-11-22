# Brain router: design

## Types (Rust)
```rust
trait Brain: Send + Sync {
    fn id(&self) -> &BrainId;
    fn capabilities(&self) -> Capabilities;           // ghost_capable, chat, is_cloud
    async fn complete_stream(&self, req: CompletionRequest, cancel: CancellationToken)
        -> Result<TokenStream, BrainError>;           // raw continuation
    async fn chat_stream(&self, req: ChatRequest, cancel: CancellationToken)
        -> Result<TokenStream, BrainError>;
    async fn test(&self) -> TestReport;               // reachable, model, latency
}
```
Providers: `LlamaServerBrain` (managed sidecar), `OpenAiCompatBrain` (Ollama, LM Studio, OpenAI-like cloud),
optional `AnthropicBrain`. `BrainRouter` maps roles (`ghost`, `selection`, `dictation_cleanup`) to brains.

## Sidecar lifecycle
`SidecarManager`: choose free port, generate API key, spawn with `--host 127.0.0.1 --api-key ... --no-webui`
(flags validated against the pinned llama.cpp build in Task 3), poll `/health` (503 while loading),
warm up (`n_predict: 0`), restart on crash, idle-stop for the instruct model.

## Model manager
Catalog (id, url, sha256, size, role). Resumable download via HTTP Range, verify sha256, atomic rename.

## Stream parsing
`SseDecoder` consumes arbitrary byte chunks and yields complete events. Property: output is independent of chunk boundaries.

## Correctness properties
- **P1 (BR-11):** for any valid SSE byte stream and any split into chunks, decoded events are identical.
- **P2 (BR-5):** dropping the stream cancels the underlying request (verified with a mock server that observes disconnect).
- **P3 (BR-9):** a file whose sha256 differs is never moved into the models directory.
- **P4 (BR-10):** backoff delays are monotonic non-decreasing and never exceed the cap.

## Security
Loopback bind, per-launch random key, Keychain for user keys, redaction before cloud calls (see `privacy-security.md`).
