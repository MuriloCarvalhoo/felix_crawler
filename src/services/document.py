"""PDF text extraction shared by commitment and invoice uploads."""

import pymupdf

from src.utils.errors import AppError, ErrorCode


def extrair_texto_pdf(data: bytes) -> str:
    try:
        with pymupdf.open(stream=data, filetype="pdf") as document:
            if document.needs_pass:
                raise ValueError("encrypted PDF")
            text = "\n".join(page.get_text() for page in document)
    except Exception:
        raise AppError(
            ErrorCode.DOCUMENT_READ_ERROR, "PDF inválido, protegido ou ilegível."
        ) from None
    if not text.strip():
        raise AppError(
            ErrorCode.DOCUMENT_READ_ERROR,
            "PDF sem texto extraível. Preencha os campos manualmente; OCR não está habilitado.",
        )
    return text
