"""Validated boundaries for extracted data and user input."""

import re
from datetime import date
from decimal import Decimal
from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator

Money = Annotated[Decimal, Field(gt=0, decimal_places=2, allow_inf_nan=False)]
Text = Annotated[str, Field(min_length=1)]


class DocumentModel(BaseModel):
    model_config = ConfigDict(str_strip_whitespace=True)


class Empresa(DocumentModel):
    id: int
    razao_social: str
    cnpj: str


class EmpenhoInput(DocumentModel):
    ug: Annotated[str, Field(pattern=r"^\d{6}$")]
    gestao: Annotated[str, Field(pattern=r"^\d{5}$")]
    empenho: Text
    cnpj_fornecedor: str
    modalidade: str = ""
    numero_edital_ata: str = ""
    valor_total: Money

    @field_validator("cnpj_fornecedor")
    @classmethod
    def normalize_cnpj(cls, value: str) -> str:
        value = re.sub(r"[.\-/\s]", "", value)
        if not re.fullmatch(r"\d{14}", value, flags=re.ASCII):
            raise ValueError("CNPJ deve conter 14 dígitos.")
        return value


class Empenho(DocumentModel):
    """Stored rows may predate the validation rules used for new submissions."""

    id: int
    empresa_id: int
    ug: str
    gestao: str
    empenho: str
    cnpj_fornecedor: str
    modalidade: str
    numero_edital_ata: str
    valor_total: Annotated[Decimal, Field(gt=0, allow_inf_nan=False)]


class NotaFiscalExtraida(DocumentModel):
    numero_nf: Text
    valor_nf: Money


class NotaFiscalInput(NotaFiscalExtraida):
    tipo_documento: Literal["Nota Fiscal (Produtos)", "NFS-e (Serviços)"]
    data_emissao_nf: date
    prazo_dias_pagamento: Annotated[int, Field(ge=1, le=3650)]


class Cobranca(DocumentModel):
    """Read historical invoices without applying new-input precision limits."""

    nf_id: int
    numero_nf: str
    valor_nf: Annotated[Decimal, Field(gt=0, allow_inf_nan=False)]
    tipo_documento: str
    data_emissao_nf: date
    prazo_dias_pagamento: Annotated[int, Field(ge=1)]
    ug: str
    empenho: str
    modalidade: str
    numero_edital_ata: str
