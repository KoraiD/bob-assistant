# BOB developer tasks. `make help` lists them. (No `just` dependency on purpose.)
.DEFAULT_GOAL := help
CARGO_MANIFEST := src-tauri/Cargo.toml
NATIVE_DIR := native/bob-native

.PHONY: help setup dev build test test-rust test-swift test-ui lint fmt check-kiro check clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  \033[35m%-12s\033[0m %s\n", $$1, $$2}'

setup: ## Install JS dependencies
	npm install

dev: ## Run the app in development mode (Tauri + Vite)
	npm run tauri dev

build: ## Build the release app bundle
	npm run tauri build

test: test-rust test-swift test-ui ## Run every test suite

test-rust: ## Rust unit + property tests
	cargo test --manifest-path $(CARGO_MANIFEST)

test-swift: ## Swift package tests
	cd $(NATIVE_DIR) && swift test

test-ui: ## Frontend tests
	npm test

lint: ## Clippy (deny warnings) + TypeScript typecheck
	cargo clippy --manifest-path $(CARGO_MANIFEST) --all-targets -- -D warnings
	npm run typecheck

fmt: ## Format Rust code
	cargo fmt --manifest-path $(CARGO_MANIFEST)

check-kiro: ## Verify Kiroween rules: .kiro committed content present and never ignored
	@bash scripts/check-kiro.sh

check: check-kiro lint test ## Everything CI runs

clean: ## Remove build artifacts
	rm -rf dist src-tauri/target $(NATIVE_DIR)/.build
