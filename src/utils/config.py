"""Runtime configuration; no secrets are loaded during module import."""

import os
from collections.abc import Mapping
from dataclasses import dataclass, field
from pathlib import Path


@dataclass(frozen=True)
class Config:
    database_path: Path = Path("saas_cobrancas.db")
    chave_ia: str = field(default="", repr=False)
    chave_cgu: str = field(default="", repr=False)
    gemini_model: str = "gemini-flash-latest"
    log_level: str = "INFO"

    @classmethod
    def from_env(cls, secrets: Mapping[str, str] | None = None) -> "Config":
        values = secrets or {}

        def get(key: str, default: str = "") -> str:
            return os.environ.get(key, values.get(key, default))

        return cls(
            database_path=Path(get("DATABASE_PATH", "saas_cobrancas.db")),
            chave_ia=get("CHAVE_IA"),
            chave_cgu=get("CHAVE_CGU"),
            gemini_model=get("GEMINI_MODEL", "gemini-flash-latest"),
            log_level=get("LOG_LEVEL", "INFO"),
        )
