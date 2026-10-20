.PHONY: install lock run test lint format

install:
	uv sync --frozen

lock:
	uv lock

run:
	uv run streamlit run app.py --server.address 127.0.0.1

test:
	uv run pytest

lint:
	uv run ruff check .
	uv run ruff format --check .

format:
	uv run ruff check --fix .
	uv run ruff format .
