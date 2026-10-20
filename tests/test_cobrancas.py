from concurrent.futures import ThreadPoolExecutor
from datetime import date
from decimal import Decimal

import pytest
from pydantic import ValidationError as ModelValidationError

from src.controller.cobrancas import CobrancasController
from src.db.repository import CobrancasRepository
from src.models.documents import EmpenhoInput, NotaFiscalInput
from src.services.cobranca import calcular_cobranca, gerar_oficio
from src.utils.errors import AppError


@pytest.fixture
def repository(tmp_path):
    repo = CobrancasRepository(tmp_path / "test.db")
    repo.init_db()
    return repo


@pytest.fixture
def empresa(repository):
    return repository.obter_empresa_demo()


def empenho(**changes):
    return EmpenhoInput.model_validate(
        {
            "ug": "123456",
            "gestao": "00001",
            "empenho": "2026NE000123",
            "cnpj_fornecedor": "12.345.678/0001-90",
            "valor_total": "100.00",
            "modalidade": "Dispensa/Inexigibilidade",
            "numero_edital_ata": "1/2026",
            **changes,
        }
    )


def nota(valor="60.00", numero="1"):
    return NotaFiscalInput(
        numero_nf=numero,
        valor_nf=valor,
        tipo_documento="Nota Fiscal (Produtos)",
        data_emissao_nf=date(2026, 9, 1),
        prazo_dias_pagamento=30,
    )


def test_multiple_invoices_and_balance(repository, empresa):
    controller = CobrancasController(repository)
    emp_id = controller.salvar_empenho(empresa, empenho())
    controller.salvar_nota(empresa, emp_id, nota())
    controller.salvar_nota(empresa, emp_id, nota("40.00", "2"))
    with pytest.raises(AppError, match="saldo"):
        controller.salvar_nota(empresa, emp_id, nota("0.01", "3"))
    assert len(repository.listar_cobrancas(empresa.id)) == 2


def test_concurrent_invoices_cannot_exceed_balance(repository, empresa):
    controller = CobrancasController(repository)
    emp_id = controller.salvar_empenho(empresa, empenho())

    def save(number):
        try:
            controller.salvar_nota(empresa, emp_id, nota(numero=str(number)))
            return True
        except AppError:
            return False

    with ThreadPoolExecutor(max_workers=2) as pool:
        assert sum(pool.map(save, [1, 2])) == 1
    assert len(repository.listar_cobrancas(empresa.id)) == 1


def test_company_cannot_link_invoice_to_other_tenant(repository, empresa):
    controller = CobrancasController(repository)
    emp_id = controller.salvar_empenho(empresa, empenho())
    with pytest.raises(AppError, match="empresa"):
        repository.salvar_nota(empresa.id + 1, emp_id, nota())
    assert repository.listar_empenhos(empresa.id + 1) == []
    assert repository.listar_cobrancas(empresa.id + 1) == []
    assert repository.listar_cobrancas(empresa.id) == []


@pytest.mark.parametrize("cnpj", ["", "123456780001901", "99888777000166"])
def test_invalid_or_different_cnpj_is_rejected(repository, empresa, cnpj):
    with pytest.raises((AppError, ModelValidationError)):
        CobrancasController(repository).salvar_empenho(
            empresa, empenho(cnpj_fornecedor=cnpj)
        )
    assert repository.listar_empenhos(empresa.id) == []


@pytest.mark.parametrize("value", ["0", "-1", "NaN", "Infinity", "1.001"])
def test_invalid_money_is_rejected(value):
    with pytest.raises(ModelValidationError):
        nota(value)


def test_due_date_and_daily_interest():
    result = calcular_cobranca(
        date(2026, 9, 1),
        30,
        Decimal("100"),
        date(2026, 10, 16),
        Decimal("2"),
        Decimal("1"),
    )
    assert result.vencimento == date(2026, 10, 1)
    assert result.dias_atraso == 15
    assert result.multa == Decimal("2.00")
    assert result.juros == Decimal("0.50")
    assert result.total == Decimal("102.50")


def test_no_penalties_before_or_on_due_date():
    result = calcular_cobranca(
        date(2026, 9, 1),
        30,
        Decimal("100"),
        date(2026, 10, 1),
        Decimal("2"),
        Decimal("1"),
    )
    assert result.dias_atraso == 0
    assert result.total == Decimal("100.00")


def test_letter_uses_company_invoice_and_calculation(repository, empresa):
    controller = CobrancasController(repository)
    emp_id = controller.salvar_empenho(empresa, empenho())
    controller.salvar_nota(empresa, emp_id, nota())
    row = repository.listar_cobrancas(empresa.id)[0]
    result = calcular_cobranca(
        row.data_emissao_nf,
        row.prazo_dias_pagamento,
        row.valor_nf,
        date(2026, 10, 16),
        Decimal("2"),
        Decimal("1"),
    )
    text = gerar_oficio(empresa, row, result, Decimal("2"), Decimal("1"))
    assert empresa.razao_social in text
    assert "2026NE000123" in text
    assert "61,50" in text


def test_existing_schema_is_reused(repository, empresa):
    controller = CobrancasController(repository)
    controller.salvar_empenho(empresa, empenho())
    repository.init_db()
    assert repository.obter_empresa_demo() == empresa
    assert len(repository.listar_empenhos(empresa.id)) == 1


def test_legacy_commitments_remain_readable(repository, empresa):
    # The original UI accepted short UG/Gestão values and a missing supplier CNPJ.
    with repository.connection() as conn:
        conn.execute(
            """INSERT INTO empenhos (empresa_id, ug, gestao, empenho, cnpj_fornecedor,
               modalidade, numero_edital_ata, valor_total) VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
            (empresa.id, "123", "1", "NE antigo", "", "", "", 100.001),
        )
    records = repository.listar_empenhos(empresa.id)
    assert records[0].empenho == "NE antigo"
    assert records[0].cnpj_fornecedor == ""
    assert records[0].valor_total == Decimal("100.001")
