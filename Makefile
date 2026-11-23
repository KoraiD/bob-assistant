# BOB developer tasks. `make help` lists them. (No `just` dependency on purpose.)
.DEFAULT_GOAL := help
CARGO_MANIFEST := src-tauri/Cargo.toml
NATIVE_DIR := native/bob-native

.PHONY: audit help setup dev build whisper native app sidecars probe chip-demo sign-dev test test-rust test-swift test-ui screenshot bench lint fmt check-kiro check clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  \033[35m%-12s\033[0m %s\n", $$1, $$2}'

setup: ## Install JS dependencies
	npm install

dev: ## Run the app in development mode (Tauri + Vite)
	npm run tauri dev

build: ## Build the release app bundle
	npm run tauri build

whisper: ## Fetch the pinned, SHA-256-verified whisper.cpp xcframework for the helper
	bash scripts/fetch-whisper.sh

native: whisper ## Build the Swift helper (debug)
	cd $(NATIVE_DIR) && swift build

app: native ## Build a signed BOB.app with the helper inside (set BOB_SIGN_IDENTITY to keep permission grants)
	bash scripts/build-app.sh

sidecars: ## Fetch the pinned, SHA-256-verified llama-server build into src-tauri/binaries/llama
	bash scripts/fetch-sidecars.sh

probe: native ## Compatibility spike: observe the focused field for 20s (see docs/COMPATIBILITY.md)
	$(NATIVE_DIR)/.build/debug/bob-native --probe 20

chip-demo: native ## Show a ghost chip at the caret in any app (needs Accessibility)
	$(NATIVE_DIR)/.build/debug/bob-native --chip-demo

sign-dev: native ## Sign the dev helper with $$BOB_SIGN_IDENTITY so macOS keeps its permission grants
	bash scripts/sign-dev.sh

test: native test-rust test-swift test-ui ## Run every test suite

test-rust: ## Rust unit + property tests
	cargo test --manifest-path $(CARGO_MANIFEST)

test-swift: ## Swift package tests
	cd $(NATIVE_DIR) && swift test

test-ui: ## Frontend tests
	npm test

bench: ## Ghost Text benchmark with the real model (about 3 min per model/setting; MODELS="id ..." to override)
	cargo run --release --manifest-path $(CARGO_MANIFEST) --bin ghost-bench -- $(foreach m,$(or $(MODELS),qwen2.5-1.5b-instruct-q4km),--model $(m))

screenshot: ## Capture the UI with headless Chrome into bench-results/local/ (visual review)
	bash scripts/screenshot.sh

lint: ## Clippy (deny warnings) + TypeScript typecheck
	cargo clippy --manifest-path $(CARGO_MANIFEST) --all-targets -- -D warnings
	npm run typecheck

audit: ## Security advisories: Rust crates (cargo-audit) and npm packages
	cargo audit --file src-tauri/Cargo.lock
	npm audit

fmt: ## Format Rust code
	cargo fmt --manifest-path $(CARGO_MANIFEST)

check-kiro: ## Verify Kiroween rules: .kiro committed content present and never ignored
	@bash scripts/check-kiro.sh

check: check-kiro lint test ## Everything CI runs

clean: ## Remove build artifacts
	rm -rf dist src-tauri/target $(NATIVE_DIR)/.build
