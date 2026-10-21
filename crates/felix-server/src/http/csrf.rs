//! Proteção CSRF das mutações em `/api` (ADR-006): cabeçalho `X-Felix: 1`
//! obrigatório e, quando houver `Origin`, ela deve ser a do próprio host ou
//! `FELIX_PUBLIC_URL`.

use super::erro::ErroApi;
use crate::{app::AppState, config::Config};
use axum::{
    extract::{Request, State},
    http::{Method, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub const CABECALHO: &str = "x-felix";

/// Origens do Vite em desenvolvimento (ADR-016), aceitas só com `FELIX_ENV=dev`.
const ORIGENS_DEV: &[&str] = &["http://127.0.0.1:5179", "http://localhost:5179"];

pub async fn verificar(State(estado): State<AppState>, request: Request, next: Next) -> Response {
    if let Err(erro) = conferir(&estado.config, &request) {
        return erro.into_response();
    }
    next.run(request).await
}

fn conferir(config: &Config, request: &Request) -> Result<(), ErroApi> {
    if matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        return Ok(());
    }
    let headers = request.headers();
    if headers.get(CABECALHO).and_then(|v| v.to_str().ok()) != Some("1") {
        return Err(ErroApi::proibido(
            "Requisição recusada: cabeçalho X-Felix ausente.",
        ));
    }
    let Some(origem) = headers.get(header::ORIGIN) else {
        return Ok(());
    };
    let origem = origem.to_str().unwrap_or_default();
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .or_else(|| request.uri().authority().map(|a| a.as_str()))
        .unwrap_or_default();
    let mesma_origem = !host.is_empty()
        && (origem == format!("http://{host}") || origem == format!("https://{host}"));
    let publica = origem == config.public_url;
    let vite = !config.producao() && ORIGENS_DEV.contains(&origem);
    if mesma_origem || publica || vite {
        Ok(())
    } else {
        Err(ErroApi::proibido(
            "Requisição recusada: origem não autorizada.",
        ))
    }
}
