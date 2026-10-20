.PHONY: build run test test-web lint coverage

build:
	cargo build --release --locked -j 2

# O binário não lê .env; o shell do make (sh) exporta as variáveis antes de rodar.
run:
	set -a; [ ! -f .env ] || . ./.env; set +a; cargo run --locked -p felix-server

test:
	cargo test --workspace --locked
	$(MAKE) test-web

test-web:
	node --test tests/frontend.test.cjs

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	node --check web/app.js

coverage:
	cargo llvm-cov --workspace --summary-only --fail-under-lines 80
