.PHONY: help install dev test test-frontend test-e2e build check clean

APP_DIR := app

help:
	@echo "Emulsion — available targets:"
	@echo "  make install       Install frontend dependencies (npm install)"
	@echo "  make dev           Start the app (Tauri dev window + Vite dev server)"
	@echo "  make test          Run the Rust core test suite (cargo test --lib)"
	@echo "  make test-frontend Run the frontend unit test suite (vitest)"
	@echo "  make test-e2e      Run the WebdriverIO e2e suite against a real debug build"
	@echo "  make build         Production build (installer/bundle for this OS)"
	@echo "  make check         Svelte/TS type-check the frontend"
	@echo "  make clean         Remove build artifacts (node_modules, target/, build/)"

install:
	cd $(APP_DIR) && npm install

dev:
	cd $(APP_DIR) && npm run tauri dev

test:
	cd $(APP_DIR)/src-tauri && cargo test --lib

test-frontend:
	cd $(APP_DIR) && npm test

test-e2e:
	cd $(APP_DIR) && npm run test:e2e

build:
	cd $(APP_DIR) && npm run tauri build

check:
	cd $(APP_DIR) && npm run check

clean:
	rm -rf $(APP_DIR)/node_modules $(APP_DIR)/build $(APP_DIR)/.svelte-kit $(APP_DIR)/src-tauri/target
