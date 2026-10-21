//! Rotas de desenvolvimento (ADR-012). `GET /dev/emails` lista a caixa de
//! saída para testes de convite e redefinição. Responde 404 em produção e,
//! mesmo em dev, quando o `Host` não é loopback (servidor exposto por engano).

use crate::{
    app::AppState,
    config::Ambiente,
    http::{ErroApi, dados},
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, header},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::net::IpAddr;
use uuid::Uuid;

/// Quantidade de e-mails listados (mais recentes primeiro).
pub const LIMITE_EMAILS: i64 = 50;

pub fn rotas() -> Router<AppState> {
    Router::new().route("/dev/emails", get(emails))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EmailDev {
    pub id: Uuid,
    pub para: String,
    pub assunto: String,
    pub corpo: String,
    pub status: String,
    pub criado_em: DateTime<Utc>,
}

/// `true` se o cabeçalho `Host` aponta para loopback (`localhost`, `127.x`, `[::1]`).
/// Com mais de um `Host`, todos precisam ser loopback.
pub fn host_loopback(headers: &HeaderMap) -> bool {
    let mut hosts = headers.get_all(header::HOST).iter().peekable();
    hosts.peek().is_some() && hosts.all(|v| v.to_str().is_ok_and(|host| nome_loopback(host.trim())))
}

fn nome_loopback(host: &str) -> bool {
    let nome = if let Some(resto) = host.strip_prefix('[') {
        resto.split_once(']').map_or("", |(ip, _)| ip)
    } else {
        host.rsplit_once(':').map_or(host, |(nome, porta)| {
            if porta.bytes().all(|b| b.is_ascii_digit()) {
                nome
            } else {
                host
            }
        })
    };
    nome.eq_ignore_ascii_case("localhost")
        || nome
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.to_canonical().is_loopback())
}

async fn emails(
    State(estado): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<impl Serialize>, ErroApi> {
    if estado.config.ambiente != Ambiente::Dev || !host_loopback(&headers) {
        return Err(ErroApi::nao_encontrado_msg("Rota da API não encontrada."));
    }
    let emails: Vec<EmailDev> = sqlx::query_as(
        "SELECT id, para::text AS para, assunto, corpo, status, criado_em FROM emails_saida
         ORDER BY criado_em DESC, id DESC LIMIT $1",
    )
    .bind(LIMITE_EMAILS)
    .fetch_all(&estado.pool)
    .await?;
    Ok(dados(emails))
}

#[cfg(test)]
mod testes {
    use super::*;
    use axum::http::HeaderValue;

    fn com_host(host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        h
    }

    #[test]
    fn hosts_loopback() {
        for host in [
            "127.0.0.1:8090",
            "localhost:5179",
            "LOCALHOST",
            "[::1]:8090",
            "127.9.9.9",
        ] {
            assert!(host_loopback(&com_host(host)), "{host}");
        }
        for host in [
            "felix.exemplo.com.br",
            "10.0.0.1:8090",
            "localhost.evil.com",
            "[::2]:80",
            "::1",
        ] {
            assert!(!host_loopback(&com_host(host)), "{host}");
        }
        assert!(!host_loopback(&HeaderMap::new()));
        let mut dois = com_host("127.0.0.1:8090");
        dois.append(
            header::HOST,
            HeaderValue::from_static("felix.exemplo.com.br"),
        );
        assert!(!host_loopback(&dois));
    }
}
