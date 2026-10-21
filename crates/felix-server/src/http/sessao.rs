//! Sessões opacas em cookie (ADR-006): o banco guarda só o SHA-256 do token.
//! Expiração deslizante de 7 dias (renovada no máximo 1x por hora) e absoluta de 30 dias.

use super::erro::ErroApi;
use crate::{
    app::AppState,
    config::Config,
    seguranca::{gerar_token, hash_token, token_bem_formado},
};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, HeaderValue, header, request::Parts},
};
use sqlx::{PgConnection, PgExecutor, PgPool};
use std::net::{IpAddr, SocketAddr};
use uuid::Uuid;

pub const COOKIE_SESSAO: &str = "felix_sessao";
/// Validade do cookie no navegador = expiração absoluta (30 dias).
pub const COOKIE_MAX_AGE: i64 = 30 * 24 * 60 * 60;
pub const USER_AGENT_MAX: usize = 512;

/// Sessão autenticada. Rejeição: 401 `nao_autenticado`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sessao {
    pub sessao_id: Uuid,
    pub usuario_id: Uuid,
}

/// Sessão se houver cookie válido; `None` caso contrário (erro só se o banco falhar).
#[derive(Debug, Clone, Copy)]
pub struct SessaoOpcional(pub Option<Sessao>);

/// Sessão recém-criada.
#[derive(Debug, Clone)]
pub struct NovaSessao {
    pub sessao_id: Uuid,
    /// Token em claro (vai só no cookie).
    pub token: String,
    /// Valor pronto para `Set-Cookie`.
    pub cookie: HeaderValue,
}

/// Cria a sessão e devolve o cabeçalho `Set-Cookie`. Aceita pool, conexão ou
/// transação (`&mut *tx`) para participar da mesma transação do cadastro.
pub async fn criar_sessao<'e, E: PgExecutor<'e>>(
    executor: E,
    cookie_seguro: bool,
    usuario_id: Uuid,
    ip: Option<IpAddr>,
    user_agent: Option<&str>,
) -> Result<NovaSessao, ErroApi> {
    let token = gerar_token();
    let user_agent: Option<String> = user_agent.map(|ua| ua.chars().take(USER_AGENT_MAX).collect());
    let sessao_id: Uuid = sqlx::query_scalar(
        "INSERT INTO sessoes (usuario_id, token_hash, expira_em, ip, user_agent)
         VALUES ($1, $2, now() + interval '7 days', $3::text::inet, $4)
         RETURNING id",
    )
    .bind(usuario_id)
    .bind(hash_token(&token))
    .bind(ip.map(|ip| ip.to_string()))
    .bind(user_agent)
    .fetch_one(executor)
    .await?;
    let cookie = cookie_sessao(&token, cookie_seguro)?;
    Ok(NovaSessao {
        sessao_id,
        token,
        cookie,
    })
}

/// `Set-Cookie` da sessão.
pub fn cookie_sessao(token: &str, seguro: bool) -> Result<HeaderValue, ErroApi> {
    let seguro = if seguro { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE_SESSAO}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={COOKIE_MAX_AGE}{seguro}"
    ))
    .map_err(|e| ErroApi::interno_log("cookie_sessao", e))
}

/// `Set-Cookie` que apaga o cookie da sessão (logout).
pub fn cookie_remocao(seguro: bool) -> HeaderValue {
    if seguro {
        HeaderValue::from_static("felix_sessao=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0; Secure")
    } else {
        HeaderValue::from_static("felix_sessao=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
    }
}

/// Revoga uma sessão (logout).
pub async fn revogar_sessao(pool: &PgPool, sessao_id: Uuid) -> Result<(), ErroApi> {
    sqlx::query("UPDATE sessoes SET revogada_em = now() WHERE id = $1 AND revogada_em IS NULL")
        .bind(sessao_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Revoga as sessões do usuário, exceto `manter` (troca/redefinição de senha).
pub async fn revogar_sessoes_do_usuario(
    conn: &mut PgConnection,
    usuario_id: Uuid,
    manter: Option<Uuid>,
) -> Result<u64, ErroApi> {
    let resultado = sqlx::query(
        "UPDATE sessoes SET revogada_em = now()
         WHERE usuario_id = $1 AND revogada_em IS NULL AND ($2::uuid IS NULL OR id <> $2)",
    )
    .bind(usuario_id)
    .bind(manter)
    .execute(conn)
    .await?;
    Ok(resultado.rows_affected())
}

/// Token do cookie `felix_sessao`, se houver.
pub fn token_do_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|par| par.trim().split_once('='))
        .find(|(nome, _)| *nome == COOKIE_SESSAO)
        .map(|(_, valor)| valor.trim())
}

/// IP e user-agent do cliente. O IP vem da conexão; `X-Real-IP` (ou o último
/// item de `X-Forwarded-For`) só é aceito quando o par da conexão está em
/// `FELIX_TRUSTED_PROXIES` (padrão: loopback, ex.: nginx local).
#[derive(Debug, Clone, Default)]
pub struct ClienteInfo {
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

impl FromRequestParts<AppState> for ClienteInfo {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        estado: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(cliente_info(parts, &estado.config))
    }
}

/// IP informado pelo proxy: `X-Real-IP` ou, na falta dele, o último item de
/// `X-Forwarded-For` (o que o próprio proxy acrescentou).
fn ip_do_proxy(headers: &HeaderMap) -> Option<IpAddr> {
    let real = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<IpAddr>().ok());
    real.or_else(|| {
        headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .next_back()
            .and_then(|v| v.trim().parse::<IpAddr>().ok())
    })
}

pub fn cliente_info(parts: &Parts, config: &Config) -> ClienteInfo {
    let par = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_canonical());
    let ip = match par {
        Some(p) if config.proxy_confiavel(p) => ip_do_proxy(&parts.headers)
            .map(|ip| ip.to_canonical())
            .or(Some(p)),
        outro => outro,
    };
    let user_agent = parts
        .headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(USER_AGENT_MAX).collect());
    ClienteInfo { ip, user_agent }
}

async fn resolver(parts: &mut Parts, estado: &AppState) -> Result<Option<Sessao>, ErroApi> {
    if let Some(sessao) = parts.extensions.get::<Sessao>() {
        return Ok(Some(*sessao));
    }
    let Some(token) = token_do_cookie(&parts.headers) else {
        return Ok(None);
    };
    if !token_bem_formado(token) {
        return Ok(None);
    }
    let linha: Option<(Uuid, Uuid, bool)> = sqlx::query_as(
        "SELECT id, usuario_id, ultimo_uso_em < now() - interval '1 hour'
         FROM sessoes
         WHERE token_hash = $1 AND revogada_em IS NULL AND expira_em > now()
           AND criado_em > now() - interval '30 days'",
    )
    .bind(hash_token(token))
    .fetch_optional(&estado.pool)
    .await?;
    let Some((sessao_id, usuario_id, renovar)) = linha else {
        return Ok(None);
    };
    if renovar {
        sqlx::query(
            "UPDATE sessoes SET ultimo_uso_em = now(),
                expira_em = least(now() + interval '7 days', criado_em + interval '30 days')
             WHERE id = $1",
        )
        .bind(sessao_id)
        .execute(&estado.pool)
        .await?;
    }
    let sessao = Sessao {
        sessao_id,
        usuario_id,
    };
    parts.extensions.insert(sessao);
    Ok(Some(sessao))
}

impl FromRequestParts<AppState> for Sessao {
    type Rejection = ErroApi;

    async fn from_request_parts(
        parts: &mut Parts,
        estado: &AppState,
    ) -> Result<Self, Self::Rejection> {
        resolver(parts, estado)
            .await?
            .ok_or_else(ErroApi::nao_autenticado)
    }
}

impl FromRequestParts<AppState> for SessaoOpcional {
    type Rejection = ErroApi;

    async fn from_request_parts(
        parts: &mut Parts,
        estado: &AppState,
    ) -> Result<Self, Self::Rejection> {
        resolver(parts, estado).await.map(SessaoOpcional)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_cookie_entre_outros() {
        let mut h = HeaderMap::new();
        h.append(header::COOKIE, HeaderValue::from_static("a=1; b=2"));
        h.append(
            header::COOKIE,
            HeaderValue::from_static("x=y; felix_sessao=abc ; z=w"),
        );
        assert_eq!(token_do_cookie(&h), Some("abc"));
        assert_eq!(token_do_cookie(&HeaderMap::new()), None);
    }

    fn partes(par: &str, cabecalhos: &[(&str, &str)]) -> Parts {
        let mut builder = axum::http::Request::builder().uri("/");
        for (nome, valor) in cabecalhos {
            builder = builder.header(*nome, *valor);
        }
        let (mut parts, ()) = builder.body(()).unwrap().into_parts();
        parts
            .extensions
            .insert(ConnectInfo(SocketAddr::new(par.parse().unwrap(), 40_000)));
        parts
    }

    #[test]
    fn ip_do_cliente_so_confia_em_proxies_configurados() {
        let mut config = Config::para_testes("postgres://x", "/x".into());
        let ip = |par: &str, cab: &[(&str, &str)], config: &Config| {
            cliente_info(&partes(par, cab), config)
                .ip
                .map(|i| i.to_string())
        };
        // Par não confiável: cabeçalhos ignorados.
        let forjado = [("x-real-ip", "1.1.1.1"), ("x-forwarded-for", "2.2.2.2")];
        assert_eq!(
            ip("203.0.113.5", &forjado, &config).as_deref(),
            Some("203.0.113.5")
        );
        // Loopback (padrão): X-Real-IP primeiro, depois o último hop do X-Forwarded-For.
        assert_eq!(
            ip("127.0.0.1", &forjado, &config).as_deref(),
            Some("1.1.1.1")
        );
        let xff = [("x-forwarded-for", "9.9.9.9, 3.3.3.3")];
        assert_eq!(ip("127.0.0.1", &xff, &config).as_deref(), Some("3.3.3.3"));
        assert_eq!(
            ip("::1", &[("x-real-ip", "lixo")], &config).as_deref(),
            Some("::1")
        );
        // Proxy em outra rede, configurado explicitamente.
        config.proxies_confiaveis = vec!["10.0.0.0/8".parse().unwrap()];
        assert_eq!(
            ip("10.0.0.2", &forjado, &config).as_deref(),
            Some("1.1.1.1")
        );
        assert_eq!(
            ip("127.0.0.1", &forjado, &config).as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(
            ip(
                "::ffff:10.0.0.2",
                &[("x-real-ip", "::ffff:4.4.4.4")],
                &config
            )
            .as_deref(),
            Some("4.4.4.4")
        );
    }

    #[test]
    fn atributos_do_cookie() {
        let c = cookie_sessao("tok", true).unwrap();
        let c = c.to_str().unwrap();
        for attr in [
            "felix_sessao=tok",
            "HttpOnly",
            "SameSite=Lax",
            "Path=/",
            "Secure",
        ] {
            assert!(c.contains(attr), "{c}");
        }
        assert!(
            !cookie_sessao("tok", false)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("Secure")
        );
        assert!(
            cookie_remocao(false)
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
    }
}
