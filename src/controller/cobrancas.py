"""Orchestrate the company-scoped billing workflow."""

import re

from src.db.repository import CobrancasRepository
from src.models.documents import EmpenhoInput, Empresa, NotaFiscalInput
from src.utils.errors import AppError, ErrorCode


class CobrancasController:
    def __init__(self, repository: CobrancasRepository) -> None:
        self.repository = repository

    def salvar_empenho(self, empresa: Empresa, data: EmpenhoInput) -> int:
        if data.cnpj_fornecedor != re.sub(r"\D", "", empresa.cnpj):
            raise AppError(
                ErrorCode.ACCESS_DENIED, "CNPJ do empenho não pertence à empresa atual."
            )
        return self.repository.salvar_empenho(empresa.id, data)

    def salvar_nota(
        self, empresa: Empresa, empenho_id: int, data: NotaFiscalInput
    ) -> int:
        return self.repository.salvar_nota(empresa.id, empenho_id, data)
