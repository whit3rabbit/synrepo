.PHONY: build build-semantic release test ci-test soak-test lint ci-lint fmt fmt-check clean install check ci-check

build:
	cargo build

build-semantic:
	cargo build --features semantic-triage

release:
	cargo build --release

test:
	cargo test

ci-test:
	cargo test --workspace -- --test-threads=1

soak-test:
	cargo test --test mutation_soak -- --ignored --test-threads=1

lint:
	cargo clippy -- -D warnings

ci-lint:
	cargo clippy --workspace --bins --lib -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

check: fmt-check lint test

ci-check: fmt-check ci-lint ci-test

clean:
	cargo clean

install:
	cargo install --path .
