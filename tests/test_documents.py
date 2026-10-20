from types import SimpleNamespace
from unittest.mock import MagicMock, patch

import pymupdf
import pytest

from src.clients.gemini import GeminiClient
from src.models.documents import EmpenhoInput, NotaFiscalExtraida
from src.services.document import extrair_texto_pdf
from src.utils.config import Config
from src.utils.errors import AppError, ErrorCode


def test_pdf_text_extraction():
    with pymupdf.open() as document:
        page = document.new_page()
        page.insert_text((72, 72), "Empenho 2026NE000123")
        data = document.tobytes()
    assert "2026NE000123" in extrair_texto_pdf(data)


@pytest.mark.parametrize("data", [b"", b"not a PDF"])
def test_invalid_pdf(data):
    with pytest.raises(AppError) as error:
        extrair_texto_pdf(data)
    assert error.value.code == ErrorCode.DOCUMENT_READ_ERROR


def test_pdf_without_text_has_clear_error():
    with pymupdf.open() as document:
        document.new_page()
        data = document.tobytes()
    with pytest.raises(AppError, match="texto"):
        extrair_texto_pdf(data)


def test_config_environment_overrides_secrets(monkeypatch):
    monkeypatch.setenv("CHAVE_IA", "environment-test-value")
    cfg = Config.from_env({"CHAVE_IA": "secrets-test-value"})
    assert cfg.chave_ia == "environment-test-value"
    assert "environment-test-value" not in repr(cfg)


def test_gemini_missing_key_has_clear_error():
    with pytest.raises(AppError, match="CHAVE_IA"):
        GeminiClient(Config()).extrair("text", "nota_fiscal")


@pytest.mark.parametrize(
    "kind,payload,expected",
    [
        ("nota_fiscal", '{"numero_nf":"42","valor_nf":12.34}', NotaFiscalExtraida),
        (
            "empenho",
            '{"ug":"123456","gestao":"00001","empenho":"2026NE1","cnpj_fornecedor":"12345678000190","valor_total":100}',
            EmpenhoInput,
        ),
    ],
)
def test_gemini_validates_structured_output(kind, payload, expected):
    client = MagicMock()
    client.models.generate_content.return_value = SimpleNamespace(text=payload)
    with patch("src.clients.gemini.genai.Client") as factory:
        factory.return_value.__enter__.return_value = client
        result = GeminiClient(Config(chave_ia="test-value")).extrair("text", kind)
    assert isinstance(result, expected)
    assert (
        result.model_dump()["valor_nf" if kind == "nota_fiscal" else "valor_total"] > 0
    )


@pytest.mark.parametrize("text", [None, "not json", '{"numero_nf":"1","valor_nf":-1}'])
def test_gemini_rejects_invalid_output(text):
    client = MagicMock()
    client.models.generate_content.return_value = SimpleNamespace(text=text)
    with patch("src.clients.gemini.genai.Client") as factory:
        factory.return_value.__enter__.return_value = client
        with pytest.raises(AppError) as error:
            GeminiClient(Config(chave_ia="test-value")).extrair("text", "nota_fiscal")
    assert error.value.code == ErrorCode.AI_INVALID_RESPONSE


def test_gemini_does_not_leak_external_error():
    with patch(
        "src.clients.gemini.genai.Client", side_effect=RuntimeError("sensitive-value")
    ):
        with pytest.raises(AppError) as error:
            GeminiClient(Config(chave_ia="test-value")).extrair("text", "empenho")
    assert error.value.code == ErrorCode.AI_MODEL_ERROR
    assert "sensitive-value" not in str(error.value)
