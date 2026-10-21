//! Erro da API com o envelope da ADR-009:
//! `{"erro": {"codigo": "...", "mensagem": "...", "campos": {...}?}}`.

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroApi {
    status: StatusCode,
    codigo: &'static str,
    mensagem: String,
    campos: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct Envelope<'a> {
    erro: Corpo<'a>,
}

#[derive(Serialize)]
struct Corpo<'a> {
    codigo: &'a str,
    mensagem: &'a str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    campos: &'a BTreeMap<String, String>,
}

impl ErroApi {
    /// Erro com código livre (ex.: `credenciais_invalidas`). Prefira os construtores.
    pub fn novo(status: StatusCode, codigo: &'static str, mensagem: impl Into<String>) -> Self {
        Self {
            status,
            codigo,
            mensagem: mensagem.into(),
            campos: BTreeMap::new(),
        }
    }

    /// 401 `nao_autenticado`.
    pub fn nao_autenticado() -> Self {
        Self::novo(
            StatusCode::UNAUTHORIZED,
            "nao_autenticado",
            "Sessão ausente ou expirada. Entre novamente.",
        )
    }

    /// 401 `credenciais_invalidas` (login), mensagem genérica.
    pub fn credenciais_invalidas() -> Self {
        Self::novo(
            StatusCode::UNAUTHORIZED,
            "credenciais_invalidas",
            "E-mail ou senha incorretos.",
        )
    }

    /// 403 `proibido`.
    pub fn proibido(mensagem: impl Into<String>) -> Self {
        Self::novo(StatusCode::FORBIDDEN, "proibido", mensagem)
    }

    /// 403 `proibido` com a mensagem padrão de papel insuficiente.
    pub fn papel_insuficiente() -> Self {
        Self::proibido("Seu papel nesta organização não permite esta ação.")
    }

    /// 404 `nao_encontrado` com mensagem padrão.
    pub fn nao_encontrado() -> Self {
        Self::nao_encontrado_msg("Registro não encontrado.")
    }

    pub fn nao_encontrado_msg(mensagem: impl Into<String>) -> Self {
        Self::novo(StatusCode::NOT_FOUND, "nao_encontrado", mensagem)
    }

    /// 422 `validacao` sem campos.
    pub fn validacao(mensagem: impl Into<String>) -> Self {
        Self::novo(StatusCode::UNPROCESSABLE_ENTITY, "validacao", mensagem)
    }

    /// 422 `validacao` com um campo: `ErroApi::validacao_campo("cnpj", "CNPJ inválido")`.
    pub fn validacao_campo(campo: impl Into<String>, mensagem: impl Into<String>) -> Self {
        let mensagem = mensagem.into();
        Self::validacao(mensagem.clone()).com_campo(campo, mensagem)
    }

    /// 422 `validacao` a partir de vários campos; `None` se o mapa estiver vazio.
    pub fn validacao_campos(campos: BTreeMap<String, String>) -> Option<Self> {
        if campos.is_empty() {
            return None;
        }
        let mut erro = Self::validacao("Revise os campos destacados.");
        erro.campos = campos;
        Some(erro)
    }

    /// Acrescenta (ou substitui) a mensagem de um campo.
    pub fn com_campo(mut self, campo: impl Into<String>, mensagem: impl Into<String>) -> Self {
        self.campos.insert(campo.into(), mensagem.into());
        self
    }

    /// 409 `conflito`.
    pub fn conflito(mensagem: impl Into<String>) -> Self {
        Self::novo(StatusCode::CONFLICT, "conflito", mensagem)
    }

    /// 402 `limite_plano`.
    pub fn limite_plano(mensagem: impl Into<String>) -> Self {
        Self::novo(StatusCode::PAYMENT_REQUIRED, "limite_plano", mensagem)
    }

    /// 429 `ocupado` com `Retry-After: 2`.
    pub fn ocupado() -> Self {
        Self::novo(
            StatusCode::TOO_MANY_REQUESTS,
            "ocupado",
            "Servidor ocupado. Tente novamente em 2 segundos.",
        )
    }

    /// 429 `ocupado` com mensagem própria (ex.: limite de tentativas de login).
    pub fn ocupado_msg(mensagem: impl Into<String>) -> Self {
        Self::novo(StatusCode::TOO_MANY_REQUESTS, "ocupado", mensagem)
    }

    /// 503 `indisponivel`.
    pub fn indisponivel(mensagem: impl Into<String>) -> Self {
        Self::novo(StatusCode::SERVICE_UNAVAILABLE, "indisponivel", mensagem)
    }

    /// 500 `interno` sem detalhes para o cliente.
    pub fn interno() -> Self {
        Self::novo(
            StatusCode::INTERNAL_SERVER_ERROR,
            "interno",
            "Erro interno. Tente novamente; se persistir, contate o suporte.",
        )
    }

    /// 500 `interno`, registrando o contexto e o erro no log (com o `request_id` do span).
    pub fn interno_log(contexto: &str, erro: impl fmt::Display) -> Self {
        tracing::error!(contexto, erro = %erro, "Erro interno");
        Self::interno()
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }
    pub fn codigo(&self) -> &'static str {
        self.codigo
    }
    pub fn mensagem(&self) -> &str {
        &self.mensagem
    }
    pub fn campos(&self) -> &BTreeMap<String, String> {
        &self.campos
    }
}

impl fmt::Display for ErroApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): {}", self.codigo, self.status, self.mensagem)
    }
}

impl std::error::Error for ErroApi {}

impl IntoResponse for ErroApi {
    fn into_response(self) -> Response {
        let corpo = Envelope {
            erro: Corpo {
                codigo: self.codigo,
                mensagem: &self.mensagem,
                campos: &self.campos,
            },
        };
        let mut resposta = (self.status, Json(corpo)).into_response();
        if self.status == StatusCode::TOO_MANY_REQUESTS {
            resposta
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
        }
        resposta
    }
}

impl From<sqlx::Error> for ErroApi {
    fn from(erro: sqlx::Error) -> Self {
        use sqlx::error::ErrorKind;
        match &erro {
            sqlx::Error::RowNotFound => Self::nao_encontrado(),
            sqlx::Error::PoolTimedOut => {
                tracing::warn!("Pool do banco esgotado");
                Self::indisponivel("Banco de dados ocupado. Tente novamente em instantes.")
            }
            sqlx::Error::Database(db) => match db.kind() {
                ErrorKind::UniqueViolation => Self::conflito("Registro já existe."),
                ErrorKind::ForeignKeyViolation => {
                    Self::conflito("Registro relacionado inexistente ou ainda em uso.")
                }
                ErrorKind::CheckViolation => Self::validacao("Dados fora das regras permitidas."),
                _ => Self::interno_log("banco", &erro),
            },
            _ => Self::interno_log("banco", &erro),
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod testes {
    use super::*;
    use http_body_util::BodyExt;

    async fn corpo(erro: ErroApi) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        let resposta = erro.into_response();
        let status = resposta.status();
        let headers = resposta.headers().clone();
        let bytes = resposta.into_body().collect().await.unwrap().to_bytes();
        (status, headers, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn envelope_com_e_sem_campos() {
        let (status, _, json) = corpo(ErroApi::validacao_campo("cnpj", "CNPJ inválido")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["erro"]["codigo"], "validacao");
        assert_eq!(json["erro"]["campos"]["cnpj"], "CNPJ inválido");
        let (status, _, json) = corpo(ErroApi::nao_encontrado()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(json["erro"].get("campos").is_none());
    }

    #[tokio::test]
    async fn codigos_e_status() {
        let casos = [
            (ErroApi::nao_autenticado(), 401, "nao_autenticado"),
            (ErroApi::papel_insuficiente(), 403, "proibido"),
            (ErroApi::nao_encontrado(), 404, "nao_encontrado"),
            (ErroApi::validacao("x"), 422, "validacao"),
            (ErroApi::conflito("x"), 409, "conflito"),
            (ErroApi::limite_plano("x"), 402, "limite_plano"),
            (ErroApi::ocupado(), 429, "ocupado"),
            (ErroApi::indisponivel("x"), 503, "indisponivel"),
            (ErroApi::interno(), 500, "interno"),
        ];
        for (erro, status, codigo) in casos {
            assert_eq!(erro.status().as_u16(), status);
            assert_eq!(erro.codigo(), codigo);
        }
        let (_, headers, _) = corpo(ErroApi::ocupado()).await;
        assert_eq!(headers[header::RETRY_AFTER], "2");
    }

    #[test]
    fn campos_vazios_nao_geram_erro() {
        assert!(ErroApi::validacao_campos(BTreeMap::new()).is_none());
        let mut campos = BTreeMap::new();
        campos.insert("nome".to_owned(), "Obrigatório".to_owned());
        let erro = ErroApi::validacao_campos(campos).unwrap();
        assert_eq!(erro.campos()["nome"], "Obrigatório");
    }

    #[test]
    fn linha_ausente_vira_404() {
        assert_eq!(
            ErroApi::from(sqlx::Error::RowNotFound).codigo(),
            "nao_encontrado"
        );
    }
}
