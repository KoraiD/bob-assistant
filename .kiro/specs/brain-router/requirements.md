# Brain router: requirements

"Bring Your Own Brain": one interface over local and cloud models. EARS IDs `BR-n`.

## User stories
- As a user, I want to pick any model (built-in, Ollama, LM Studio, cloud) and see whether it works.
- As a privacy-conscious user, I want local models by default and clear labels when text leaves my Mac.

## Requirements
- **BR-1** THE System SHALL expose a `Brain` abstraction with capability flags (`ghost_capable`, `chat`) and a streaming completion API.
- **BR-2** THE System SHALL support providers: built-in sidecar (`llama-server`), Ollama, LM Studio and any OpenAI-compatible endpoint. Anthropic is optional.
- **BR-3** WHEN a brain is added or edited, THE System SHALL offer a connection test reporting reachability, detected model, capability flags and round-trip latency.
- **BR-4** THE System SHALL use a raw completions endpoint for Ghost Text and SHALL mark chat-only brains as not Ghost-capable.
- **BR-5** WHEN a request is cancelled, THE System SHALL close the HTTP connection so the server stops generating.
- **BR-6** THE System SHALL store API keys only in the macOS Keychain.
- **BR-7** WHEN a brain sends data off-device (non-loopback host), THE System SHALL label it "cloud" in the UI.
- **BR-8** THE System SHALL start the built-in `llama-server` bound to 127.0.0.1 with a random per-launch API key and no web UI, health-check it, restart it on crash and stop it on idle (instruct model) or exit.
- **BR-9** THE System SHALL download models to `~/Library/Application Support/BOB/models` with progress, resumable downloads and sha256 verification, and SHALL refuse to load a model that fails verification.
- **BR-10** IF a brain is unreachable, THEN THE System SHALL report a clear error and SHALL back off exponentially (cap 5 s) instead of retrying in a tight loop.
- **BR-11** THE System SHALL parse streamed responses (SSE / JSON lines) correctly for any chunking of the byte stream.

## Out of scope (v1)
Function/tool calling, vision, embeddings, multi-user auth.
