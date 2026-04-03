.PHONY: install build test test-all lint fmt check clean

install:
	cargo install --path .

build:
	cargo build --release

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
