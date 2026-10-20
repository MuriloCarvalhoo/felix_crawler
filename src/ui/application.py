"""Streamlit presentation; business rules live outside this module."""

import hashlib
import logging
import sqlite3
from datetime import date
from decimal import Decimal

import streamlit as st
from pydantic import ValidationError
from streamlit.errors import StreamlitSecretNotFoundError

from src.clients.gemini import GeminiClient
from src.controller.cobrancas import CobrancasController
from src.db.repository import CobrancasRepository
from src.models.documents import EmpenhoInput, Empresa, NotaFiscalInput
from src.services.cobranca import calcular_cobranca, gerar_oficio, reais
from src.services.document import extrair_texto_pdf
from src.utils.config import Config
from src.utils.errors import AppError

logger = logging.getLogger(__name__)


def _config() -> Config:
    try:
        secrets = {
            key: st.secrets[key]
            for key in (
                "CHAVE_IA",
                "CHAVE_CGU",
                "DATABASE_PATH",
                "GEMINI_MODEL",
                "LOG_LEVEL",
            )
            if key in st.secrets
        }
    except StreamlitSecretNotFoundError:
        secrets = {}
    return Config.from_env(secrets)


def _extract(config: Config, tipo: str) -> dict:
    labels = {
        "empenho": "Upload do Empenho em PDF",
        "nota_fiscal": "Upload da Nota Fiscal em PDF",
    }
    uploaded = st.file_uploader(labels[tipo], type=["pdf"], key=f"pdf_{tipo}")
    if uploaded is None:
        return {}
    if not config.chave_ia:
        st.info("IA não configurada. Preencha os campos manualmente.")
        return {}
    data = uploaded.getvalue()
    fingerprint = hashlib.sha256(data).hexdigest()
    # Session-local: documents and extraction results are never cached across users.
    key = f"extraction_{tipo}"
    cached = st.session_state.get(key)
    if cached and cached[0] == (fingerprint, config.gemini_model):
        return cached[1]
    try:
        with st.spinner("Extraindo dados do documento..."):
            result = (
                GeminiClient(config).extrair(extrair_texto_pdf(data), tipo).model_dump()
            )
    except AppError as error:
        st.warning(error.message)
        return {}
    st.session_state[key] = ((fingerprint, config.gemini_model), result)
    st.success("Dados extraídos. Confira os campos antes de salvar.")
    return result


def _save_error(error: Exception) -> None:
    if isinstance(error, AppError):
        st.error(error.message)
    elif isinstance(error, ValidationError):
        fields = ", ".join(".".join(map(str, item["loc"])) for item in error.errors())
        st.error(f"Confira os campos obrigatórios e seus formatos: {fields}.")
    else:
        logger.error("Database operation failed (%s)", type(error).__name__)
        st.error("Não foi possível salvar os dados no banco. Tente novamente.")


def _render_empenho(
    config: Config, controller: CobrancasController, empresa: Empresa
) -> None:
    st.header("📄 1. Cadastrar Empenho (Análise IA)")
    data = _extract(config, "empenho")
    with st.form("form_emp"):
        ug = st.text_input("UG", value=data.get("ug", ""))
        gestao = st.text_input("Gestão", value=data.get("gestao", ""))
        numero = st.text_input("Nº Empenho", value=data.get("empenho", ""))
        cnpj = st.text_input(
            "CNPJ Fornecedor no Documento", value=data.get("cnpj_fornecedor", "")
        )
        modalidade = st.text_input(
            "Modalidade / Origem", value=data.get("modalidade", "")
        )
        edital = st.text_input(
            "Nº Edital / Ata", value=data.get("numero_edital_ata", "")
        )
        valor = st.number_input(
            "Valor Total R$",
            min_value=0.0,
            value=float(data.get("valor_total", 0)),
            step=0.01,
            format="%.2f",
        )
        submitted = st.form_submit_button("Salvar Empenho")
    if submitted:
        try:
            controller.salvar_empenho(
                empresa,
                EmpenhoInput(
                    ug=ug,
                    gestao=gestao,
                    empenho=numero,
                    cnpj_fornecedor=cnpj,
                    modalidade=modalidade,
                    numero_edital_ata=edital,
                    valor_total=Decimal(str(valor)),
                ),
            )
        except (AppError, ValidationError, sqlite3.Error) as error:
            _save_error(error)
        else:
            st.session_state["notice"] = "Empenho gravado com sucesso!"
            st.rerun()


def _render_nota(
    config: Config, controller: CobrancasController, empresa: Empresa
) -> None:
    st.header("🧾 2. Vincular Nota Fiscal (NF / NFS-e)")
    empenhos = controller.repository.listar_empenhos(empresa.id)
    if not empenhos:
        st.info("Nenhum empenho cadastrado para esta empresa ainda.")
        return
    options = {item.id: item for item in empenhos}
    selected = st.selectbox(
        "Selecione o Empenho",
        list(options),
        format_func=lambda key: (
            f"{options[key].empenho} ({options[key].modalidade}) - Total: R$ {reais(options[key].valor_total)} [#{key}]"
        ),
    )
    data = _extract(config, "nota_fiscal")
    with st.form("form_nf"):
        first, second = st.columns(2)
        with first:
            tipo = st.selectbox(
                "Tipo de Documento", ["Nota Fiscal (Produtos)", "NFS-e (Serviços)"]
            )
            numero = st.text_input("Número da Nota", value=data.get("numero_nf", ""))
            valor = st.number_input(
                "Valor R$",
                min_value=0.0,
                value=float(data.get("valor_nf", 0)),
                step=0.01,
                format="%.2f",
            )
        with second:
            emissao = st.date_input("Data de Emissão da Nota", value=date.today())
            prazo = st.number_input(
                "Prazo Limite do Edital (Dias)", min_value=1, max_value=3650, value=30
            )
        submitted = st.form_submit_button("Salvar e Iniciar Cronômetro")
    if submitted:
        try:
            controller.salvar_nota(
                empresa,
                selected,
                NotaFiscalInput(
                    numero_nf=numero,
                    valor_nf=Decimal(str(valor)),
                    tipo_documento=tipo,
                    data_emissao_nf=emissao,
                    prazo_dias_pagamento=prazo,
                ),
            )
        except (AppError, ValidationError, sqlite3.Error) as error:
            _save_error(error)
        else:
            st.session_state["notice"] = "Nota Fiscal vinculada ao contrato!"
            st.rerun()


def _render_cobrancas(controller: CobrancasController, empresa: Empresa) -> None:
    st.header("⚖️ Central de Vencimentos e Notificação Extrajudicial")
    notas = controller.repository.listar_cobrancas(empresa.id)
    if not notas:
        st.info("Nenhuma cobrança pendente registrada no momento.")
        return
    atrasadas = [
        (
            nota,
            calcular_cobranca(
                nota.data_emissao_nf,
                nota.prazo_dias_pagamento,
                nota.valor_nf,
                date.today(),
            ),
        )
        for nota in notas
    ]
    atrasadas = [(nota, calc) for nota, calc in atrasadas if calc.dias_atraso > 0]
    if not atrasadas:
        st.success(
            "Todas as notas fiscais cadastradas estão dentro do prazo do edital."
        )
        return
    st.warning(f"Existem {len(atrasadas)} nota(s) fiscal(is) com pagamento em atraso.")
    st.dataframe(
        [
            {
                "Empenho": nota.empenho,
                "UG": nota.ug,
                "Modalidade": nota.modalidade,
                "NF": nota.numero_nf,
                "Valor R$": reais(nota.valor_nf),
                "Emissão": f"{nota.data_emissao_nf:%d/%m/%Y}",
                "Vencimento": f"{calc.vencimento:%d/%m/%Y}",
                "Dias Atraso": calc.dias_atraso,
            }
            for nota, calc in atrasadas
        ],
        hide_index=True,
        width="stretch",
    )
    st.subheader("📝 Gerar Minuta de Ofício de Cobrança")
    selected = st.selectbox(
        "Selecione a Nota em Atraso:",
        range(len(atrasadas)),
        format_func=lambda index: (
            f"NF {atrasadas[index][0].numero_nf} - UG {atrasadas[index][0].ug} ({atrasadas[index][1].dias_atraso} dias de atraso)"
        ),
    )
    left, right = st.columns(2)
    with left:
        multa = st.number_input("Multa de Mora (%)", min_value=0.0, value=0.0)
    with right:
        juros = st.number_input("Juros de Mora Mensal (%)", min_value=0.0, value=0.0)
    st.caption(
        "Informe as taxas previstas no contrato. O prazo é contado da emissão da NF; confira o termo inicial contratual."
    )
    nota, _ = atrasadas[selected]
    calc = calcular_cobranca(
        nota.data_emissao_nf,
        nota.prazo_dias_pagamento,
        nota.valor_nf,
        date.today(),
        Decimal(str(multa)),
        Decimal(str(juros)),
    )
    oficio = gerar_oficio(empresa, nota, calc, Decimal(str(multa)), Decimal(str(juros)))
    edited = st.text_area(
        "Minuta do Ofício para Conferência:", value=oficio, height=350
    )
    st.download_button(
        "💾 Baixar Ofício (.txt)",
        data=edited,
        file_name=f"Oficio_Cobranca_{nota.nf_id}.txt",
        mime="text/plain",
    )


def main() -> None:
    st.set_page_config(
        page_title="SaaS Gestão de Cobranças Públicas", page_icon="🏛️", layout="wide"
    )
    config = _config()
    logging.basicConfig(level=config.log_level)
    repository = CobrancasRepository(config.database_path)
    try:
        repository.init_db()
        empresa = repository.obter_empresa_demo()
        controller = CobrancasController(repository)
        st.title("🏛️ Plataforma SaaS de Gestão e Cobrança de Empenhos")
        st.warning("Modo demonstração: empresa simulada, sem autenticação. Uso local.")
        st.caption(f"🏢 Empresa: {empresa.razao_social} | CNPJ: {empresa.cnpj}")
        if notice := st.session_state.pop("notice", None):
            st.success(notice)
        left, right = st.columns([1, 2])
        with left:
            _render_empenho(config, controller, empresa)
        with right:
            _render_nota(config, controller, empresa)
        st.divider()
        _render_cobrancas(controller, empresa)
    except (sqlite3.Error, ValidationError) as error:
        logger.error("Could not load application data (%s)", type(error).__name__)
        st.error(
            "Não foi possível carregar o banco. Confira DATABASE_PATH e a integridade dos registros."
        )
