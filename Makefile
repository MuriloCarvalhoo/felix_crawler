.PHONY: db-up db-down db-reset dev migrar test lint build build-frontend coverage

COMPOSE := docker compose -f deploy/compose.dev.yml
FELIX_TEST_DATABASE_URL ?= postgres://postgres:postgres@127.0.0.1:55433/postgres
HAS_FRONTEND := $(wildcard frontend/package.json)

# Postgres 18 de desenvolvimento/testes em 127.0.0.1:55433.
db-up:
	$(COMPOSE) up -d --wait

db-down:
	$(COMPOSE) down

# Apaga o volume (todos os dados locais).
db-reset:
	$(COMPOSE) down -v
	$(COMPOSE) up -d --wait

# O binário não lê .env; o shell do make exporta as variáveis antes de rodar.
# Sem FELIX_ENV o servidor assume prod (fail-closed): aqui o padrão é dev.
dev:
	set -a; [ ! -f .env ] || . ./.env; set +a; FELIX_ENV=$${FELIX_ENV:-dev} cargo run --locked -p felix-server

migrar:
	set -a; [ ! -f .env ] || . ./.env; set +a; FELIX_ENV=$${FELIX_ENV:-dev} cargo run --locked -p felix-server -- migrar

test:
	FELIX_TEST_DATABASE_URL=$(FELIX_TEST_DATABASE_URL) cargo test --workspace --locked
ifneq ($(HAS_FRONTEND),)
	npm --prefix frontend test
endif

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
ifneq ($(HAS_FRONTEND),)
	npm --prefix frontend run lint
endif

build-frontend:
ifneq ($(HAS_FRONTEND),)
	npm --prefix frontend ci
	npm --prefix frontend run build
endif

build: build-frontend
	cargo build --release --locked -p felix-server -j 2

coverage:
	FELIX_TEST_DATABASE_URL=$(FELIX_TEST_DATABASE_URL) cargo llvm-cov --workspace --summary-only --fail-under-lines 80
