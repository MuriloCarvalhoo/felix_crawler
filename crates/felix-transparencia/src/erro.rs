use thiserror::Error;

/// Erros da integração. As mensagens nunca incluem a chave da API.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ErroTransparencia {
    /// HTTP 401 ou 403: chave ausente, inválida, suspensa ou sem acesso à API restrita.
    #[error("chave da API do Portal da Transparência inválida ou sem permissão")]
    ChaveInvalida,
    /// HTTP 429: limite de requisições do Portal excedido.
    #[error("limite de requisições do Portal da Transparência excedido; tente mais tarde")]
    LimiteExcedido,
    /// HTTP 5xx, tempo esgotado ou falha de conexão.
    #[error("Portal da Transparência indisponível: {0}")]
    Indisponivel(String),
    /// Resposta fora do formato esperado (JSON inválido, status inesperado).
    #[error("resposta inesperada do Portal da Transparência: {0}")]
    Resposta(String),
    /// Configuração ou parâmetros inválidos.
    #[error("configuração inválida da integração com o Portal da Transparência: {0}")]
    Configuracao(String),
}
