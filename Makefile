# BOB developer tasks. `make help` lists them. (No `just` dependency on purpose.)
.DEFAULT_GOAL := help
CARGO_MANIFEST := src-tauri/Cargo.toml
NATIVE_DIR := native/bob-native

.PHONY: audit help setup dev build whisper native app release sidecars probe test test-rust test-swift test-ui screenshot bench style-bench e2e test-bench lint fmt check-kiro check clean

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
release: ## Build the signed release DMG in release/ (needs BOB_SIGN_IDENTITY; notarizes with BOB_NOTARY_PROFILE)
	bash scripts/build-release.sh

sidecars: ## Fetch the pinned, SHA-256-verified llama-server build into src-tauri/binaries/llama
	bash scripts/fetch-sidecars.sh

probe: native ## Compatibility spike: observe the focused field for 20s (see docs/COMPATIBILITY.md)
	$(NATIVE_DIR)/.build/debug/bob-native --probe 20

test: native test-rust test-swift test-ui ## Run every test suite

test-rust: ## Rust unit + property tests
	cargo test --manifest-path $(CARGO_MANIFEST)

test-swift: ## Swift package tests (helper and Test Bench logic)
	cd $(NATIVE_DIR) && swift test
	cd native/test-bench && swift test

test-ui: ## Frontend tests
	npm test

bench: ## Ghost Text benchmark with the real model (about 3 min per model/setting; MODELS="id ..." to override)
	cargo run --release --features bench --manifest-path $(CARGO_MANIFEST) --bin ghost-bench -- $(foreach m,$(or $(MODELS),qwen2.5-1.5b-instruct-q4km),--model $(m))

style-bench: ## What the language and style settings change on the real model (add -- --mid for gap filling)
	cargo run --release --features bench --manifest-path $(CARGO_MANIFEST) --bin style-bench

e2e: ## End-to-end check of the running BOB in TextEdit with real key events (takes over the keyboard for a minute)
	swiftc -O scripts/e2e.swift -o /tmp/bob-e2e && /tmp/bob-e2e

test-bench: ## Test Bench: a test BOB (separate profile) suggests while a typist types into the bench's own editor (quit your BOB first; ARGS="--brain builtin --minutes 10")
	bash scripts/test-bench.sh $(ARGS)

screenshot: ## Capture the UI with headless Chrome into bench-results/local/ (visual review)
	bash scripts/screenshot.sh

lint: ## Clippy (deny warnings) + TypeScript typecheck
	cargo clippy --manifest-path $(CARGO_MANIFEST) --all-targets --all-features -- -D warnings
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
