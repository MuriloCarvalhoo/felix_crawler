//! Extratores que convertem rejeições do axum para o envelope `ErroApi`.
//! Use `Corpo<T>` no lugar de `Json<T>`, `Consulta<T>` no lugar de `Query<T>`
//! e `Caminho<T>` no lugar de `Path<T>`.

use super::erro::ErroApi;
use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Query, Request},
    http::{StatusCode, request::Parts},
};
use serde::de::DeserializeOwned;

/// Corpo JSON (limite global de 64 KiB). Erros → 422 `validacao`
/// (413 se acima do limite, 415 sem `Content-Type: application/json`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Corpo<T>(pub T);

impl<S, T> FromRequest<S> for Corpo<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ErroApi;

    async fn from_request(req: Request, estado: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, estado).await {
            Ok(Json(valor)) => Ok(Self(valor)),
            Err(rejeicao) => Err(match rejeicao.status() {
                StatusCode::PAYLOAD_TOO_LARGE => ErroApi::novo(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "validacao",
                    "Corpo da requisição acima do limite.",
                ),
                StatusCode::UNSUPPORTED_MEDIA_TYPE => ErroApi::novo(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "validacao",
                    "Envie o corpo com Content-Type: application/json.",
                ),
                _ => ErroApi::validacao(format!("JSON inválido: {}", rejeicao.body_text())),
            }),
        }
    }
}

/// Query string. Erros → 422 `validacao`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Consulta<T>(pub T);

impl<S, T> FromRequestParts<S> for Consulta<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ErroApi;

    async fn from_request_parts(parts: &mut Parts, estado: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, estado)
            .await
            .map(|Query(valor)| Self(valor))
            .map_err(|r| ErroApi::validacao(format!("Parâmetros inválidos: {}", r.body_text())))
    }
}

/// Parâmetros de caminho. Valor inválido (ex.: UUID malformado) → 404 `nao_encontrado`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Caminho<T>(pub T);

impl<S, T> FromRequestParts<S> for Caminho<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ErroApi;

    async fn from_request_parts(parts: &mut Parts, estado: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, estado)
            .await
            .map(|Path(valor)| Self(valor))
            .map_err(|_| ErroApi::nao_encontrado())
    }
}
