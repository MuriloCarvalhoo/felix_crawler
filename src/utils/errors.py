"""Application errors shared by the controller, storage and clients."""

from enum import StrEnum


class ErrorCode(StrEnum):
    INVALID_INPUT = "INVALID_INPUT"
    ACCESS_DENIED = "ACCESS_DENIED"
    BALANCE_EXCEEDED = "BALANCE_EXCEEDED"
    DOCUMENT_READ_ERROR = "DOCUMENT_READ_ERROR"
    AI_MODEL_ERROR = "AI_MODEL_ERROR"
    AI_INVALID_RESPONSE = "AI_INVALID_RESPONSE"


class AppError(Exception):
    def __init__(self, code: ErrorCode, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
