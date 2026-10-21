//! Membros: listar, trocar papel e remover (inclusive sair da organização).
//!
//! Regras (ADR-008): admin altera papéis até admin; só dono promove a dono ou
//! altera/remove um dono; a organização nunca fica sem dono; qualquer membro
//! pode sair, exceto o último dono. Quem é removido ou rebaixado para abaixo
//! de admin tem os convites pendentes que criou revogados.

use super::travar_org;
use crate::{
    app::AppState,
    db,
    http::{Caminho, ClienteInfo, Corpo, ErroApi, OrgCtx, Papel, dados, sem_conteudo},
};
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

const MSG_ULTIMO_DONO: &str =
    "A organização precisa de pelo menos um dono. Promova outra pessoa a dono antes.";

pub(super) fn rotas() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org}/membros", get(listar))
        .route(
            "/orgs/{org}/membros/{usuario_id}",
            axum::routing::patch(alterar_papel).delete(remover),
        )
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Membro {
    pub usuario_id: Uuid,
    pub nome: String,
    pub email: String,
    pub papel: Papel,
    pub desde: DateTime<Utc>,
}

const SELECT_MEMBRO: &str =
    "SELECT m.usuario_id, u.nome, u.email::text AS email, m.papel, m.criado_em AS desde
     FROM membros m JOIN usuarios u ON u.id = m.usuario_id
     WHERE m.organizacao_id = $1";

async fn listar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
) -> Result<Json<impl Serialize>, ErroApi> {
    let mut membros: Vec<Membro> = sqlx::query_as(SELECT_MEMBRO)
        .bind(ctx.org_id)
        .fetch_all(&estado.pool)
        .await?;
    membros.sort_by(|a, b| {
        b.papel
            .cmp(&a.papel)
            .then_with(|| a.nome.to_lowercase().cmp(&b.nome.to_lowercase()))
    });
    Ok(dados(membros))
}

async fn buscar(
    conn: &mut PgConnection,
    org_id: Uuid,
    usuario_id: Uuid,
) -> Result<Membro, ErroApi> {
    sqlx::query_as(&format!("{SELECT_MEMBRO} AND m.usuario_id = $2"))
        .bind(org_id)
        .bind(usuario_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ErroApi::nao_encontrado_msg("Membro não encontrado."))
}

async fn contar_donos(conn: &mut PgConnection, org_id: Uuid) -> Result<i64, ErroApi> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM membros WHERE organizacao_id = $1 AND papel = 'dono'",
    )
    .bind(org_id)
    .fetch_one(&mut *conn)
    .await?)
}

/// Revoga os convites pendentes criados por `usuario_id` na organização.
async fn revogar_convites_de(
    conn: &mut PgConnection,
    org_id: Uuid,
    usuario_id: Uuid,
    agora: DateTime<Utc>,
) -> Result<u64, ErroApi> {
    let resultado = sqlx::query(
        "UPDATE convites SET revogado_em = $3
         WHERE organizacao_id = $1 AND convidado_por = $2
           AND aceito_em IS NULL AND revogado_em IS NULL",
    )
    .bind(org_id)
    .bind(usuario_id)
    .bind(agora)
    .execute(&mut *conn)
    .await?;
    Ok(resultado.rows_affected())
}

#[derive(Debug, Deserialize)]
struct NovoPapel {
    papel: Papel,
}

async fn alterar_papel(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, usuario_id)): Caminho<(Uuid, Uuid)>,
    Corpo(corpo): Corpo<NovoPapel>,
) -> Result<Json<impl Serialize>, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    travar_org(&mut tx, ctx.org_id).await?;
    let alvo = buscar(&mut tx, ctx.org_id, usuario_id).await?;
    if (alvo.papel == Papel::Dono || corpo.papel == Papel::Dono) && ctx.papel != Papel::Dono {
        return Err(ErroApi::proibido(
            "Somente um dono pode promover a dono ou alterar o papel de um dono.",
        ));
    }
    if alvo.papel == corpo.papel {
        tx.commit().await?;
        return Ok(dados(alvo));
    }
    if alvo.papel == Papel::Dono && contar_donos(&mut tx, ctx.org_id).await? <= 1 {
        return Err(ErroApi::conflito(MSG_ULTIMO_DONO));
    }
    sqlx::query("UPDATE membros SET papel = $3 WHERE organizacao_id = $1 AND usuario_id = $2")
        .bind(ctx.org_id)
        .bind(usuario_id)
        .bind(corpo.papel)
        .execute(&mut *tx)
        .await?;
    let convites_revogados = if corpo.papel < Papel::Admin {
        revogar_convites_de(&mut tx, ctx.org_id, usuario_id, estado.agora()).await?
    } else {
        0
    };
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "alterar_papel",
        "membro",
        Some(&usuario_id.to_string()),
        json!({
            "papel": corpo.papel,
            "papel_anterior": alvo.papel,
            "email": alvo.email,
            "convites_revogados": convites_revogados,
        }),
        cliente.ip,
    )
    .await?;
    let membro = buscar(&mut tx, ctx.org_id, usuario_id).await?;
    tx.commit().await?;
    Ok(dados(membro))
}

async fn remover(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, usuario_id)): Caminho<(Uuid, Uuid)>,
) -> Result<StatusCode, ErroApi> {
    let saindo = usuario_id == ctx.usuario_id;
    if !saindo {
        ctx.exigir(Papel::Admin)?;
    }
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    travar_org(&mut tx, ctx.org_id).await?;
    let alvo = buscar(&mut tx, ctx.org_id, usuario_id).await?;
    if alvo.papel == Papel::Dono {
        if !saindo && ctx.papel != Papel::Dono {
            return Err(ErroApi::proibido(
                "Somente um dono pode remover outro dono.",
            ));
        }
        if contar_donos(&mut tx, ctx.org_id).await? <= 1 {
            return Err(ErroApi::conflito(MSG_ULTIMO_DONO));
        }
    }
    sqlx::query("DELETE FROM membros WHERE organizacao_id = $1 AND usuario_id = $2")
        .bind(ctx.org_id)
        .bind(usuario_id)
        .execute(&mut *tx)
        .await?;
    let convites_revogados =
        revogar_convites_de(&mut tx, ctx.org_id, usuario_id, estado.agora()).await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        if saindo { "sair" } else { "remover" },
        "membro",
        Some(&usuario_id.to_string()),
        json!({"papel": alvo.papel, "email": alvo.email, "convites_revogados": convites_revogados}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(sem_conteudo())
}
