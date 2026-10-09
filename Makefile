# Rust builds run at the lowest CPU/IO priority on 2 cores so they never starve
# the trading bot on this machine. See docs/hosting.md.
CARGO := nice -n 19 ionice -c3 cargo
JOBS  := -j 2

.PHONY: help dev-server dev-web web build release check test fmt install clean

help:
	@echo "make dev-server  Run the Rust API on :8080 (debug)"
	@echo "make dev-web     Run Vite with hot reload on :5173 (proxies /api to :8080)"
	@echo "make web         Install deps and build the frontend into web/dist"
	@echo "make build       Debug build of the server"
	@echo "make release     Frontend + optimized server binary -> target/release/kanban"
	@echo "make check       rustfmt, clippy, svelte-check"
	@echo "make test        Rust API tests + frontend unit tests"
	@echo "make install     Install binary + systemd unit (uses sudo)"

dev-server:
	$(CARGO) run $(JOBS)

dev-web:
	npm --prefix web run dev

web/node_modules: web/package-lock.json
	npm --prefix web ci --no-fund --no-audit
	@touch $@

web: web/node_modules
	npm --prefix web run build

build:
	$(CARGO) build $(JOBS)

release: web
	$(CARGO) build --release $(JOBS)
	@ls -lh target/release/kanban

check: web/node_modules
	cargo fmt --all -- --check
	$(CARGO) clippy $(JOBS) --all-targets -- -D warnings
	npm --prefix web run check

test: web/node_modules
	$(CARGO) test $(JOBS)
	npm --prefix web test

fmt:
	cargo fmt --all

install: release
	sudo deploy/install.sh target/release/kanban

clean:
	cargo clean
	rm -rf web/dist
