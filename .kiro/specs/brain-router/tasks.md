# Brain router: tasks

Plan task 3.

- [x] 1. `Brain` trait, capabilities, error types, `SseDecoder` (_BR-1, BR-11_)
  - [x] 1.1 Unit tests + property test P1
- [x] 2. `OpenAiCompatBrain` for Ollama/LM Studio/cloud with mock-server tests (_BR-2, BR-4, BR-5_)
  - [x] 2.1 Cancellation test P2
- [x] 3. Keychain-backed secrets (_BR-6_)
- [x] 4. Backoff helper with property test P4 (_BR-10_)
- [x] 5. Sidecar manager: spawn, health, warm-up, restart, idle stop (_BR-8_)
- [x] 6. Model manager: catalog, resumable download, sha256 verify, property test P3 (_BR-9_)
- [x] 7. Connection test + cloud labeling (_BR-3, BR-7_)
- [x] 8. Brain settings panel (React): list, add/edit, test, latency badge, capability chips
- [x] 9. Docs: ARCHITECTURE, PRIVACY updated; journal entry

## Implementation notes
- The trait became a concrete `HttpBrain` with a `Dialect` enum: every provider BOB supports speaks HTTP, and the only
  difference is the request/response shape (llama.cpp `/completion` vs OpenAI `/v1/*`). Same contract (streaming,
  cancellation by drop, capability checks), less indirection.
- Anthropic (optional in BR-2) is not implemented; any OpenAI-compatible gateway works today.
