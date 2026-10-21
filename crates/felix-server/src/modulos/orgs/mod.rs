//! Organizações (ADR-007/008): criação, dados, exclusão lógica, membros e convites.

mod convites;
mod membros;

use crate::{
    app::AppState,
    db,
    http::{ClienteInfo, Corpo, ErroApi, OrgCtx, Papel, Sessao, criado, dados, sem_conteudo},
    modulos::{
        assinatura::{Assinatura, Plano, carregar_assinatura},
        auth::validar_nome,
    },
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

/// Organizações ativas (não excluídas) que um usuário pode ter criado.
pub const MAX_ORGS_CRIADAS: i64 = 10;

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route("/orgs", post(criar))
        .route("/orgs/{org}", get(obter).patch(atualizar).delete(excluir))
        .merge(membros::rotas())
        .merge(convites::rotas())
}

/// Trava a linha da organização (`FOR NO KEY UPDATE`) para serializar
/// verificações como "último dono" e limites de plano. Não bloqueia leituras
/// nem inserções com FK para a organização.
pub(crate) async fn travar_org(conn: &mut PgConnection, org_id: Uuid) -> Result<(), ErroApi> {
    sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM organizacoes WHERE id = $1 AND excluida_em IS NULL FOR NO KEY UPDATE",
    )
    .bind(org_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ErroApi::nao_encontrado_msg("Organização não encontrada."))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct NovaOrg {
    #[serde(default)]
    nome: String,
}

#[derive(Debug, Serialize)]
struct OrgCriada {
    id: Uuid,
    nome: String,
    papel: Papel,
    plano: Plano,
}

/// `POST /orgs`: organização nova no plano Gratuito (só o cadastro ganha teste).
/// Cada usuário cria no máximo [`MAX_ORGS_CRIADAS`] organizações ativas (409).
async fn criar(
    State(estado): State<AppState>,
    sessao: Sessao,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<NovaOrg>,
) -> Result<impl IntoResponse, ErroApi> {
    let nome = validar_nome(&corpo.nome, "o nome da organização")
        .map_err(|m| ErroApi::validacao_campo("nome", m))?;
    let mut tx = estado.pool.begin().await?;
    // Trava o usuário para serializar criações concorrentes.
    sqlx::query("SELECT 1 FROM usuarios WHERE id = $1 FOR NO KEY UPDATE")
        .bind(sessao.usuario_id)
        .execute(&mut *tx)
        .await?;
    let criadas: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organizacoes WHERE criado_por = $1 AND excluida_em IS NULL",
    )
    .bind(sessao.usuario_id)
    .fetch_one(&mut *tx)
    .await?;
    if criadas >= MAX_ORGS_CRIADAS {
        return Err(ErroApi::conflito(format!(
            "Você já criou {MAX_ORGS_CRIADAS} organizações ativas, o máximo permitido. \
             Exclua uma organização sem uso ou fale com o suporte."
        )));
    }
    let org_id: Uuid = sqlx::query_scalar(
        "INSERT INTO organizacoes (nome, criado_por) VALUES ($1, $2) RETURNING id",
    )
    .bind(&nome)
    .bind(sessao.usuario_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO membros (organizacao_id, usuario_id, papel) VALUES ($1, $2, 'dono')")
        .bind(org_id)
        .bind(sessao.usuario_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO assinaturas (organizacao_id, plano, status, provedor)
         VALUES ($1, 'gratuito', 'ativa', 'manual')",
    )
    .bind(org_id)
    .execute(&mut *tx)
    .await?;
    db::entrar_tenant(&mut tx, org_id).await?;
    db::auditar(
        &mut tx,
        org_id,
        Some(sessao.usuario_id),
        "criar",
        "organizacao",
        Some(&org_id.to_string()),
        json!({"nome": nome, "plano": "gratuito"}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(criado(OrgCriada {
        id: org_id,
        nome,
        papel: Papel::Dono,
        plano: Plano::Gratuito,
    }))
}

#[derive(Debug, Serialize)]
struct Organizacao {
    id: Uuid,
    nome: String,
    papel: Papel,
    assinatura: Assinatura,
}

async fn carregar(
    conn: &mut PgConnection,
    ctx: &OrgCtx,
    estado: &AppState,
) -> Result<Organizacao, ErroApi> {
    let nome: String = sqlx::query_scalar("SELECT nome FROM organizacoes WHERE id = $1")
        .bind(ctx.org_id)
        .fetch_one(&mut *conn)
        .await?;
    let assinatura =
        carregar_assinatura(conn, ctx.org_id, estado.agora(), estado.config.plano_manual).await?;
    Ok(Organizacao {
        id: ctx.org_id,
        nome,
        papel: ctx.papel,
        assinatura,
    })
}

async fn obter(
    State(estado): State<AppState>,
    ctx: OrgCtx,
) -> Result<Json<impl Serialize>, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let org = carregar(&mut tx, &ctx, &estado).await?;
    tx.commit().await?;
    Ok(dados(org))
}

async fn atualizar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<NovaOrg>,
) -> Result<Json<impl Serialize>, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let nome = validar_nome(&corpo.nome, "o nome da organização")
        .map_err(|m| ErroApi::validacao_campo("nome", m))?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let anterior: String = sqlx::query_scalar(
        "SELECT nome FROM organizacoes WHERE id = $1 AND excluida_em IS NULL FOR UPDATE",
    )
    .bind(ctx.org_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE organizacoes SET nome = $1 WHERE id = $2")
        .bind(&nome)
        .bind(ctx.org_id)
        .execute(&mut *tx)
        .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "atualizar",
        "organizacao",
        Some(&ctx.org_id.to_string()),
        json!({"nome": nome, "nome_anterior": anterior}),
        cliente.ip,
    )
    .await?;
    let org = carregar(&mut tx, &ctx, &estado).await?;
    tx.commit().await?;
    Ok(dados(org))
}

#[derive(Debug, Deserialize)]
struct Exclusao {
    #[serde(default)]
    confirmacao: String,
}

/// `DELETE /orgs/{org}`: exclusão lógica (`excluida_em`), só o dono, com o nome como confirmação.
async fn excluir(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<Exclusao>,
) -> Result<StatusCode, ErroApi> {
    ctx.exigir(Papel::Dono)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let nome: String = sqlx::query_scalar(
        "SELECT nome FROM organizacoes WHERE id = $1 AND excluida_em IS NULL FOR UPDATE",
    )
    .bind(ctx.org_id)
    .fetch_one(&mut *tx)
    .await?;
    if corpo.confirmacao.trim() != nome.trim() {
        return Err(ErroApi::validacao_campo(
            "confirmacao",
            "Digite exatamente o nome da organização para confirmar a exclusão.",
        ));
    }
    let agora = estado.agora();
    sqlx::query("UPDATE organizacoes SET excluida_em = $2 WHERE id = $1")
        .bind(ctx.org_id)
        .bind(agora)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE convites SET revogado_em = $2
         WHERE organizacao_id = $1 AND aceito_em IS NULL AND revogado_em IS NULL",
    )
    .bind(ctx.org_id)
    .bind(agora)
    .execute(&mut *tx)
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "excluir",
        "organizacao",
        Some(&ctx.org_id.to_string()),
        json!({"nome": nome}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(sem_conteudo())
}
