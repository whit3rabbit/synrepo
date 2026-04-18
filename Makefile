.PHONY: build release test soak-test lint fmt fmt-check clean install check

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

soak-test:
	cargo test --test mutation_soak -- --ignored --test-threads=1

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

check: fmt-check lint test

clean:
	cargo clean

install:
	cargo install --path .
