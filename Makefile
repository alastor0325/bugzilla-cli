.PHONY: install build test test-all lint fmt check clean

install:
	cargo build
	mkdir -p ~/.local/bin
	ln -sf $(shell pwd)/target/debug/bugzilla-cli ~/.local/bin/bugzilla-cli
	@echo "Symlinked to ~/.local/bin/bugzilla-cli — run 'cargo build' to update"

install-release:
	cargo install --path .

build:
	cargo build

test:
	cargo test --lib

test-all:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

check: lint test

clean:
	cargo clean
