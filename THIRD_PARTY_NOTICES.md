# Third-party notices

BOB's own code is MIT licensed (see [LICENSE](LICENSE)). It uses the following third-party components. None of the
binaries or model files below are stored in this repository; scripts download pinned, SHA-256-verified copies.

## Runtime components downloaded by scripts or by the app
| Component | Version | License | How it is used |
|---|---|---|---|
| [llama.cpp](https://github.com/ggml-org/llama.cpp) `llama-server` and ggml libraries | build b12633 (prebuilt macOS arm64) | MIT | local model server, run as a separate process (`make sidecars`) |
| [Qwen2.5-1.5B-Instruct GGUF](https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF) | Q4_K_M | Apache-2.0 | default local model, downloaded by the user from the Brains tab |
| [Qwen3-4B-Instruct-2507 GGUF](https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF) (base model by Qwen) | Q4_K_M | Apache-2.0 | optional local model |
| [Qwen3-1.7B-Base GGUF](https://huggingface.co/mradermacher/Qwen3-1.7B-Base-GGUF) (base model by Qwen) | Q4_K_M | Apache-2.0 | optional, experimental Ghost Text model |
| [whisper.cpp](https://github.com/ggml-org/whisper.cpp) `whisper.xcframework` (with ggml) | build b5907 (official prebuilt) | MIT | on-device speech recognition linked into the Swift helper (`make whisper`), embedded in BOB.app |
| [Whisper large-v3 turbo, ggml Q5_0](https://huggingface.co/ggerganov/whisper.cpp) (model by OpenAI) | revision 5359861 | MIT | default dictation model, downloaded by the user |
| [Whisper base, ggml](https://huggingface.co/ggerganov/whisper.cpp) (model by OpenAI) | revision 5359861 | MIT | light dictation model, downloaded by the user |

## Libraries
Rust crates (see `src-tauri/Cargo.lock`) and npm packages (see `package-lock.json`) are used under their own permissive
licenses (MIT, Apache-2.0, BSD, ISC and similar). Notable: Tauri (MIT/Apache-2.0), tokio (MIT), reqwest (MIT/Apache-2.0),
rustls (Apache-2.0/ISC/MIT), keyring (MIT/Apache-2.0), objc2 (MIT), React (MIT), react-markdown, remark-gfm and
remark-emoji (MIT), Tailwind CSS (MIT), Vite (MIT), Vitest (MIT). Apart from whisper.cpp (above), the Swift helper
uses only Apple system frameworks.

## Research references (not code)
Design decisions cite public documentation and source, for example the llama.cpp server README and the Chromium
accessibility source (`browser_accessibility_cocoa.mm`); see `docs/ARCHITECTURE.md` and `docs/COMPATIBILITY.md`.
No code was copied from them.
