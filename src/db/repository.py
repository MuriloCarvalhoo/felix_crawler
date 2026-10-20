"""SQLite repository compatible with the original database schema."""

import sqlite3
from contextlib import contextmanager
from datetime import date
from decimal import Decimal
from pathlib import Path

from src.models.documents import (
    Cobranca,
    Empenho,
    EmpenhoInput,
    Empresa,
    NotaFiscalInput,
)
from src.utils.errors import AppError, ErrorCode


class CobrancasRepository:
    def __init__(self, database_path: Path) -> None:
        self.database_path = database_path

    @contextmanager
    def connection(self):
        connection = sqlite3.connect(self.database_path, timeout=10)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys = ON")
        try:
            with connection:
                yield connection
        finally:
            connection.close()

    def init_db(self) -> None:
        with self.connection() as conn:
            conn.executescript("""
                CREATE TABLE IF NOT EXISTS empresas (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, razao_social TEXT, cnpj TEXT UNIQUE
                );
                CREATE TABLE IF NOT EXISTS empenhos (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, empresa_id INTEGER, ug TEXT,
                    gestao TEXT, empenho TEXT, cnpj_fornecedor TEXT, modalidade TEXT,
                    numero_edital_ata TEXT, valor_total REAL, data_cadastro TEXT,
                    FOREIGN KEY(empresa_id) REFERENCES empresas(id)
                );
                CREATE TABLE IF NOT EXISTS notas_fiscais (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, empenho_id INTEGER,
                    tipo_documento TEXT, numero_nf TEXT, valor_nf REAL, status TEXT,
                    data_emissao_nf TEXT, prazo_dias_pagamento INTEGER, data_emissao_ob TEXT,
                    FOREIGN KEY(empenho_id) REFERENCES empenhos(id)
                );
            """)

    def obter_empresa_demo(self) -> Empresa:
        """Local demonstration only; this is not authentication."""
        with self.connection() as conn:
            conn.execute("BEGIN IMMEDIATE")
            row = conn.execute("SELECT * FROM empresas ORDER BY id LIMIT 1").fetchone()
            if row is None:
                conn.execute(
                    "INSERT INTO empresas (razao_social, cnpj) VALUES (?, ?)",
                    ("Minha Empresa Licitações LTDA", "12.345.678/0001-90"),
                )
                row = conn.execute(
                    "SELECT * FROM empresas ORDER BY id LIMIT 1"
                ).fetchone()
            return Empresa.model_validate(dict(row))

    def salvar_empenho(self, empresa_id: int, data: EmpenhoInput) -> int:
        with self.connection() as conn:
            cursor = conn.execute(
                """INSERT INTO empenhos (empresa_id, ug, gestao, empenho, cnpj_fornecedor,
                   modalidade, numero_edital_ata, valor_total, data_cadastro)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    empresa_id,
                    data.ug,
                    data.gestao,
                    data.empenho,
                    data.cnpj_fornecedor,
                    data.modalidade,
                    data.numero_edital_ata,
                    str(data.valor_total),
                    date.today().isoformat(),
                ),
            )
            return cursor.lastrowid

    def listar_empenhos(self, empresa_id: int) -> list[Empenho]:
        with self.connection() as conn:
            rows = conn.execute(
                "SELECT * FROM empenhos WHERE empresa_id = ? ORDER BY id", (empresa_id,)
            ).fetchall()
        return [Empenho.model_validate(dict(row)) for row in rows]

    def salvar_nota(
        self, empresa_id: int, empenho_id: int, data: NotaFiscalInput
    ) -> int:
        with self.connection() as conn:
            # Serialize balance checks and writes, including across Streamlit sessions.
            conn.execute("BEGIN IMMEDIATE")
            row = conn.execute(
                "SELECT valor_total FROM empenhos WHERE id = ? AND empresa_id = ?",
                (empenho_id, empresa_id),
            ).fetchone()
            if row is None:
                raise AppError(
                    ErrorCode.ACCESS_DENIED, "Empenho não pertence à empresa atual."
                )
            values = conn.execute(
                "SELECT valor_nf FROM notas_fiscais WHERE empenho_id = ?", (empenho_id,)
            ).fetchall()
            total = sum((Decimal(str(item[0])) for item in values), Decimal("0"))
            if total + data.valor_nf > Decimal(str(row[0])):
                raise AppError(
                    ErrorCode.BALANCE_EXCEEDED,
                    "Valor da nota excede o saldo do empenho.",
                )
            cursor = conn.execute(
                """INSERT INTO notas_fiscais (empenho_id, tipo_documento, numero_nf, valor_nf,
                   status, data_emissao_nf, prazo_dias_pagamento)
                   VALUES (?, ?, ?, ?, 'PENDENTE', ?, ?)""",
                (
                    empenho_id,
                    data.tipo_documento,
                    data.numero_nf,
                    str(data.valor_nf),
                    data.data_emissao_nf.isoformat(),
                    data.prazo_dias_pagamento,
                ),
            )
            return cursor.lastrowid

    def listar_cobrancas(self, empresa_id: int) -> list[Cobranca]:
        with self.connection() as conn:
            rows = conn.execute(
                """SELECT e.ug, e.empenho, e.modalidade, e.numero_edital_ata,
                          nf.id AS nf_id, nf.tipo_documento, nf.numero_nf, nf.valor_nf,
                          nf.data_emissao_nf, nf.prazo_dias_pagamento
                   FROM empenhos e JOIN notas_fiscais nf ON nf.empenho_id = e.id
                   WHERE e.empresa_id = ? AND nf.status = 'PENDENTE' ORDER BY nf.id""",
                (empresa_id,),
            ).fetchall()
        return [Cobranca.model_validate(dict(row)) for row in rows]
