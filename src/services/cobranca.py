"""Pure due-date, monetary calculation and notification functions."""

from dataclasses import dataclass
from datetime import date, timedelta
from decimal import ROUND_HALF_UP, Decimal

from src.models.documents import Cobranca, Empresa

CENT = Decimal("0.01")


@dataclass(frozen=True)
class CalculoCobranca:
    vencimento: date
    dias_atraso: int
    principal: Decimal
    multa: Decimal
    juros: Decimal
    total: Decimal


def calcular_cobranca(
    emissao: date,
    prazo_dias: int,
    principal: Decimal,
    hoje: date,
    multa_pct: Decimal = Decimal("0"),
    juros_pct: Decimal = Decimal("0"),
) -> CalculoCobranca:
    if prazo_dias < 1 or any(
        not v.is_finite() or v < 0 for v in (principal, multa_pct, juros_pct)
    ):
        raise ValueError("Prazo, principal e taxas devem ser válidos e não negativos.")
    vencimento = emissao + timedelta(days=prazo_dias)
    atraso = max(0, (hoje - vencimento).days)
    multa = (principal * multa_pct / 100 if atraso else Decimal("0")).quantize(
        CENT, ROUND_HALF_UP
    )
    juros = (principal * juros_pct / 100 / 30 * atraso).quantize(CENT, ROUND_HALF_UP)
    return CalculoCobranca(
        vencimento, atraso, principal, multa, juros, principal + multa + juros
    )


def reais(value: Decimal) -> str:
    return f"{value:,.2f}".translate(str.maketrans({",": ".", ".": ","}))


def gerar_oficio(
    empresa: Empresa,
    nota: Cobranca,
    calculo: CalculoCobranca,
    multa_pct: Decimal,
    juros_pct: Decimal,
) -> str:
    return f"""MINUTA — OFÍCIO DE COBRANÇA Nº ____/{calculo.vencimento.year}

À Unidade Gestora: {nota.ug}
Assunto: Solicitação de pagamento — Lei nº 14.133/2021
Referência: Empenho nº {nota.empenho} | {nota.modalidade} ({nota.numero_edital_ata}) | NF nº {nota.numero_nf}

Prezado(a) Ordenador(a) de Despesas,

Solicitamos a verificação e regularização do pagamento da {nota.tipo_documento} nº {nota.numero_nf},
emitida em {nota.data_emissao_nf:%d/%m/%Y}, no valor de R$ {reais(calculo.principal)}.
O vencimento calculado é {calculo.vencimento:%d/%m/%Y}, com {calculo.dias_atraso} dia(s) de atraso.

O art. 92, V, da Lei nº 14.133/2021 trata das condições de pagamento e dos critérios de
atualização monetária. Solicitamos informações sobre a observância da ordem cronológica
de pagamentos prevista no art. 141, quando aplicável.
Os encargos abaixo dependem da previsão contratual e da conferência do termo inicial:

- Principal: R$ {reais(calculo.principal)}
- Multa informada ({multa_pct}%): R$ {reais(calculo.multa)}
- Juros informados ({juros_pct}% a.m., pro-rata die / 30): R$ {reais(calculo.juros)}
- Total calculado: R$ {reais(calculo.total)}

Solicitamos esclarecimentos e providências para a regularização.

Atenciosamente,
{empresa.razao_social} (CNPJ: {empresa.cnpj})
"""
