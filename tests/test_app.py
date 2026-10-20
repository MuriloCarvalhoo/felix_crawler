from datetime import date
from pathlib import Path

from streamlit.testing.v1 import AppTest

from src.controller.cobrancas import CobrancasController
from src.db.repository import CobrancasRepository
from src.models.documents import EmpenhoInput, NotaFiscalInput

APP_PATH = Path(__file__).resolve().parents[1] / "app.py"


def input_by_label(elements, label):
    return next(item for item in elements if item.label == label)


def click(app, label):
    input_by_label(app.button, label).click().run()
    assert not app.exception


def test_manual_registration_and_balance_error(tmp_path, monkeypatch):
    path = tmp_path / "ui.db"
    monkeypatch.setenv("DATABASE_PATH", str(path))
    monkeypatch.setenv("CHAVE_IA", "")
    app = AppTest.from_file(APP_PATH, default_timeout=15).run()
    assert not app.exception
    assert any("demonstração" in item.value.lower() for item in app.warning)
    for label, value in {
        "UG": "123456",
        "Gestão": "00001",
        "Nº Empenho": "2026NE000123",
        "CNPJ Fornecedor no Documento": "12.345.678/0001-90",
    }.items():
        input_by_label(app.text_input, label).set_value(value)
    input_by_label(app.number_input, "Valor Total R$").set_value(100.0)
    click(app, "Salvar Empenho")
    input_by_label(app.text_input, "Número da Nota").set_value("42")
    input_by_label(app.number_input, "Valor R$").set_value(60.0)
    input_by_label(app.date_input, "Data de Emissão da Nota").set_value(
        date(2020, 1, 1)
    )
    click(app, "Salvar e Iniciar Cronômetro")
    assert app.text_area
    assert "NF nº 42" in app.text_area[0].value
    input_by_label(app.text_input, "Número da Nota").set_value("43")
    input_by_label(app.number_input, "Valor R$").set_value(50.0)
    click(app, "Salvar e Iniciar Cronômetro")
    assert any("saldo" in error.value for error in app.error)
    repository = CobrancasRepository(path)
    assert len(repository.listar_cobrancas(repository.obter_empresa_demo().id)) == 1


def test_current_invoice_and_invalid_form(tmp_path, monkeypatch):
    monkeypatch.setenv("DATABASE_PATH", str(tmp_path / "ui.db"))
    monkeypatch.setenv("CHAVE_IA", "")
    repository = CobrancasRepository(tmp_path / "ui.db")
    repository.init_db()
    empresa = repository.obter_empresa_demo()
    controller = CobrancasController(repository)
    emp_id = controller.salvar_empenho(
        empresa,
        EmpenhoInput(
            ug="123456",
            gestao="00001",
            empenho="NE1",
            cnpj_fornecedor=empresa.cnpj,
            valor_total="100",
        ),
    )
    controller.salvar_nota(
        empresa,
        emp_id,
        NotaFiscalInput(
            numero_nf="1",
            valor_nf="10",
            tipo_documento="NFS-e (Serviços)",
            data_emissao_nf=date.today(),
            prazo_dias_pagamento=30,
        ),
    )
    app = AppTest.from_file(APP_PATH, default_timeout=15).run()
    assert not app.exception
    assert any("dentro do prazo" in item.value for item in app.success)
    click(app, "Salvar Empenho")
    assert app.error
