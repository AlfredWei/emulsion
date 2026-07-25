.PHONY: help install dev test build check spike spike-revert clean

APP_DIR := app
TAURI_CONF := $(APP_DIR)/src-tauri/tauri.conf.json

help:
	@echo "Emulsion — available targets:"
	@echo "  make install       Install frontend dependencies (npm install)"
	@echo "  make dev           Start the app (Tauri dev window + Vite dev server)"
	@echo "  make test          Run the Rust core test suite (cargo test --lib)"
	@echo "  make build         Production build (installer/bundle for this OS)"
	@echo "  make check         Svelte/TS type-check the frontend"
	@echo "  make spike         Start the app pointed at the M0 WebGPU spike page"
	@echo "  make spike-revert  Undo spike's tauri.conf.json edit if left dirty"
	@echo "  make clean         Remove build artifacts (node_modules, target/, build/)"

install:
	cd $(APP_DIR) && npm install

dev:
	cd $(APP_DIR) && npm run tauri dev

test:
	cd $(APP_DIR)/src-tauri && cargo test --lib

build:
	cd $(APP_DIR) && npm run tauri build

check:
	cd $(APP_DIR) && npm run check

# Temporarily points the main window at /m0-spike (see docs/adr/ADR-0004)
# to re-run the in-webview WebGPU validation, then restores tauri.conf.json
# on exit whether the app quit cleanly or was interrupted (Ctrl-C).
spike:
	@cp $(TAURI_CONF) $(TAURI_CONF).bak
	@sed -i '' 's#"height": 800#"height": 800,\n        "url": "/m0-spike"#' $(TAURI_CONF)
	@trap 'mv $(TAURI_CONF).bak $(TAURI_CONF)' EXIT INT TERM; \
		cd $(APP_DIR) && npm run tauri dev

spike-revert:
	@if [ -f $(TAURI_CONF).bak ]; then mv $(TAURI_CONF).bak $(TAURI_CONF); echo "reverted $(TAURI_CONF)"; \
	else echo "nothing to revert"; fi

clean:
	rm -rf $(APP_DIR)/node_modules $(APP_DIR)/build $(APP_DIR)/.svelte-kit $(APP_DIR)/src-tauri/target
