//! Notificações da organização e geração manual de alertas.
//!
//! - `GET  /orgs/{org}/notificacoes?nao_lidas=true` [leitor] (`meta.nao_lidas`)
//! - `POST /orgs/{org}/notificacoes/{id}/lida` [leitor] → 204
//! - `POST /orgs/{org}/notificacoes/lidas` [leitor] → 204
//! - `POST /orgs/{org}/alertas/gerar` [admin] → `{criadas}`
//!
//! Notificações com `usuario_id` nulo valem para a organização inteira. Como
//! `lida_em` fica na própria notificação, marcar como lida vale para todos os
//! membros (simplificação aceita; notificações pessoais têm `usuario_id`).

pub mod alertas;

use crate::{
    app::AppState,
    db,
    http::{
        Caminho, ClienteInfo, Consulta, ErroApi, OrgCtx, Paginacao, Papel, dados, sem_conteudo,
    },
};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org}/notificacoes", get(listar))
        .route("/orgs/{org}/notificacoes/lidas", post(marcar_todas))
        .route("/orgs/{org}/notificacoes/{id}/lida", post(marcar_lida))
        .route("/orgs/{org}/alertas/gerar", post(gerar))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Notificacao {
    pub id: Uuid,
    pub tipo: String,
    pub titulo: String,
    pub corpo: String,
    pub link: Option<String>,
    pub lida_em: Option<DateTime<Utc>>,
    pub criado_em: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct Filtro {
    nao_lidas: Option<String>,
    #[serde(flatten)]
    paginacao: Paginacao,
}

/// Lê `true/false/1/0/sim/nao` (query string).
pub(crate) fn ler_bool(campo: &str, valor: Option<&str>) -> Result<bool, ErroApi> {
    match valor.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        None | Some("" | "false" | "0" | "nao" | "não") => Ok(false),
        Some("true" | "1" | "sim") => Ok(true),
        Some(_) => Err(ErroApi::validacao_campo(campo, "Use true ou false.")),
    }
}

async fn listar(
    ctx: OrgCtx,
    State(estado): State<AppState>,
    Consulta(filtro): Consulta<Filtro>,
) -> Result<impl IntoResponse, ErroApi> {
    let so_nao_lidas = ler_bool("nao_lidas", filtro.nao_lidas.as_deref())?;
    let p = filtro.paginacao;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let (total, nao_lidas): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE NOT $3 OR lida_em IS NULL),
                count(*) FILTER (WHERE lida_em IS NULL)
         FROM notificacoes
         WHERE organizacao_id = $1 AND (usuario_id IS NULL OR usuario_id = $2)",
    )
    .bind(ctx.org_id)
    .bind(ctx.usuario_id)
    .bind(so_nao_lidas)
    .fetch_one(&mut *tx)
    .await?;
    let itens: Vec<Notificacao> = sqlx::query_as(
        "SELECT id, tipo, titulo, corpo, link, lida_em, criado_em
         FROM notificacoes
         WHERE organizacao_id = $1 AND (usuario_id IS NULL OR usuario_id = $2)
           AND (NOT $3 OR lida_em IS NULL)
         ORDER BY criado_em DESC, id DESC
         LIMIT $4 OFFSET $5",
    )
    .bind(ctx.org_id)
    .bind(ctx.usuario_id)
    .bind(so_nao_lidas)
    .bind(p.limite())
    .bind(p.offset())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let axum::Json(lista) = p.lista(itens, total);
    Ok(axum::Json(lista.com_meta("nao_lidas", nao_lidas)))
}

async fn marcar_lida(
    ctx: OrgCtx,
    State(estado): State<AppState>,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
) -> Result<StatusCode, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let r = sqlx::query(
        "UPDATE notificacoes SET lida_em = COALESCE(lida_em, now())
         WHERE id = $1 AND organizacao_id = $2 AND (usuario_id IS NULL OR usuario_id = $3)",
    )
    .bind(id)
    .bind(ctx.org_id)
    .bind(ctx.usuario_id)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() == 0 {
        return Err(ErroApi::nao_encontrado_msg("Notificação não encontrada."));
    }
    tx.commit().await?;
    Ok(sem_conteudo())
}

async fn marcar_todas(ctx: OrgCtx, State(estado): State<AppState>) -> Result<StatusCode, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    sqlx::query(
        "UPDATE notificacoes SET lida_em = now()
         WHERE organizacao_id = $1 AND (usuario_id IS NULL OR usuario_id = $2) AND lida_em IS NULL",
    )
    .bind(ctx.org_id)
    .bind(ctx.usuario_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(sem_conteudo())
}

async fn gerar(
    ctx: OrgCtx,
    cliente: ClienteInfo,
    State(estado): State<AppState>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let resultado = alertas::gerar_para_org(&estado, ctx.org_id).await?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "gerar_alertas",
        "notificacao",
        None,
        json!({ "criadas": resultado.criadas, "emails": resultado.emails }),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(dados(json!({ "criadas": resultado.criadas })))
}
