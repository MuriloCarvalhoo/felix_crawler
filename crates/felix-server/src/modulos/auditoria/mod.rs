//! `GET /orgs/{org}/auditoria` (ADR-018): trilha de mutações, só admin/dono.

use crate::{
    app::AppState,
    db,
    http::{Consulta, ErroApi, OrgCtx, Paginacao, Papel},
};
use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn rotas() -> Router<AppState> {
    Router::new().route("/orgs/{org}/auditoria", get(listar))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EntradaAuditoria {
    pub id: i64,
    pub usuario_id: Option<Uuid>,
    pub usuario_nome: Option<String>,
    pub acao: String,
    pub entidade: String,
    pub entidade_id: Option<String>,
    pub dados: serde_json::Value,
    pub criado_em: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct Filtro {
    entidade: Option<String>,
    #[serde(flatten)]
    paginacao: Paginacao,
}

async fn listar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Consulta(filtro): Consulta<Filtro>,
) -> Result<Json<impl Serialize>, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let entidade = filtro
        .entidade
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty());
    let p = filtro.paginacao;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM auditoria
         WHERE organizacao_id = $1 AND ($2::text IS NULL OR entidade = $2)",
    )
    .bind(ctx.org_id)
    .bind(entidade)
    .fetch_one(&mut *tx)
    .await?;
    let itens: Vec<EntradaAuditoria> = sqlx::query_as(
        "SELECT a.id, a.usuario_id, u.nome AS usuario_nome, a.acao, a.entidade, a.entidade_id,
                a.dados, a.criado_em
         FROM auditoria a LEFT JOIN usuarios u ON u.id = a.usuario_id
         WHERE a.organizacao_id = $1 AND ($2::text IS NULL OR a.entidade = $2)
         ORDER BY a.criado_em DESC, a.id DESC
         LIMIT $3 OFFSET $4",
    )
    .bind(ctx.org_id)
    .bind(entidade)
    .bind(p.limite())
    .bind(p.offset())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(p.lista(itens, total))
}
