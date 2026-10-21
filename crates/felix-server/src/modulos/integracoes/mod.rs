//! Integração com o Portal da Transparência (ADR-014).
//!
//! - `GET   /orgs/{org}/integracoes/transparencia` [leitor]
//! - `PATCH /orgs/{org}/integracoes/transparencia` [admin] `{ativa}`
//! - `POST  /orgs/{org}/integracoes/transparencia/sincronizar` [admin] `{ano?}` → 202 `{job_id}`
//!   (409 se já houver sincronização em andamento ou se a última manual foi há
//!   menos de 1 hora)
//! - `GET   /orgs/{org}/integracoes/transparencia/documentos` [leitor] `situacao?=novo, fase?, pagina`
//! - `POST  .../documentos/{id}/vincular` [operador] `{empenho_id}` | `{nota_id}`
//! - `POST  .../documentos/{id}/importar` [operador] → 201 `Empenho`
//! - `POST  .../documentos/{id}/ignorar` [operador]
//!
//! `ativa = true` faz a rodada diária (job `alertas_diarios`) enfileirar a
//! sincronização do ano corrente. `ultimo_status`: `ok`, `parcial` (parte dos
//! documentos ficou de fora ou falhou no meio; detalhe em `ultimo_erro`) ou `erro`.

pub mod documentos;
pub mod sincronizacao;

pub use sincronizacao::{agendar_sincronizacoes_ativas, sincronizar};

use crate::{
    app::AppState,
    db,
    http::{
        Caminho, ClienteInfo, Consulta, Corpo, ErroApi, OrgCtx, Paginacao, Papel, criado, dados,
    },
    jobs,
    modulos::assinatura::{limites_do_plano, plano_efetivo},
};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Datelike, TimeDelta, Utc};
use documentos::FiltroDocumentos;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sincronizacao::{MSG_NAO_CONFIGURADA, MSG_PLANO, TIPO_INTEGRACAO};
use sqlx::PgConnection;
use uuid::Uuid;

const BASE: &str = "/orgs/{org}/integracoes/transparencia";
/// Intervalo mínimo entre sincronizações manuais da mesma organização (a
/// automática diária não conta).
pub const INTERVALO_MANUAL_MINUTOS: i64 = 60;

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route(BASE, get(obter).patch(alterar))
        .route(&format!("{BASE}/sincronizar"), post(sincronizar_agora))
        .route(&format!("{BASE}/documentos"), get(listar_documentos))
        .route(
            &format!("{BASE}/documentos/{{id}}/vincular"),
            post(vincular),
        )
        .route(
            &format!("{BASE}/documentos/{{id}}/importar"),
            post(importar),
        )
        .route(&format!("{BASE}/documentos/{{id}}/ignorar"), post(ignorar))
}

/// Estado da integração para a tela de configurações.
#[derive(Debug, Serialize)]
pub struct EstadoIntegracao {
    pub configurada_no_servidor: bool,
    pub permitida_no_plano: bool,
    pub ativa: bool,
    pub ultima_sincronizacao_em: Option<DateTime<Utc>>,
    pub ultimo_status: Option<String>,
    pub ultimo_erro: Option<String>,
    /// Documentos com situação `novo`.
    pub pendentes: i64,
    /// Há job de sincronização pendente ou em execução.
    pub sincronizando: bool,
}

#[derive(Debug, Default, sqlx::FromRow)]
struct LinhaIntegracao {
    ativa: bool,
    ultima_sincronizacao_em: Option<DateTime<Utc>>,
    ultimo_status: Option<String>,
    ultimo_erro: Option<String>,
}

async fn carregar_estado(
    conn: &mut PgConnection,
    estado: &AppState,
    org: Uuid,
) -> Result<EstadoIntegracao, ErroApi> {
    let linha: Option<LinhaIntegracao> = sqlx::query_as(
        "SELECT ativa, ultima_sincronizacao_em, ultimo_status, ultimo_erro
             FROM integracoes WHERE organizacao_id = $1 AND tipo = $2",
    )
    .bind(org)
    .bind(TIPO_INTEGRACAO)
    .fetch_optional(&mut *conn)
    .await?;
    let linha = linha.unwrap_or_default();
    let pendentes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM documentos_transparencia WHERE organizacao_id = $1 AND situacao = 'novo'",
    )
    .bind(org)
    .fetch_one(&mut *conn)
    .await?;
    let sincronizando: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM jobs WHERE organizacao_id = $1 AND tipo = $2
                        AND status IN ('pendente', 'executando'))",
    )
    .bind(org)
    .bind(jobs::TIPO_TRANSPARENCIA)
    .fetch_one(&mut *conn)
    .await?;
    let plano = plano_efetivo(conn, org, estado.agora()).await?;
    Ok(EstadoIntegracao {
        configurada_no_servidor: estado.config.transparencia_key.is_some(),
        permitida_no_plano: limites_do_plano(plano).transparencia,
        ativa: linha.ativa,
        ultima_sincronizacao_em: linha.ultima_sincronizacao_em,
        ultimo_status: linha.ultimo_status,
        ultimo_erro: linha.ultimo_erro,
        pendentes,
        sincronizando,
    })
}

async fn obter(ctx: OrgCtx, State(estado): State<AppState>) -> Result<impl IntoResponse, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let resposta = carregar_estado(&mut tx, &estado, ctx.org_id).await?;
    tx.commit().await?;
    Ok(dados(resposta))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Alteracao {
    ativa: bool,
}

async fn alterar(
    ctx: OrgCtx,
    cliente: ClienteInfo,
    State(estado): State<AppState>,
    Corpo(alteracao): Corpo<Alteracao>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    if alteracao.ativa {
        let plano = plano_efetivo(&mut tx, ctx.org_id, estado.agora()).await?;
        if !limites_do_plano(plano).transparencia {
            return Err(ErroApi::limite_plano(MSG_PLANO));
        }
    }
    sqlx::query(
        "INSERT INTO integracoes (organizacao_id, tipo, ativa) VALUES ($1, $2, $3)
         ON CONFLICT (organizacao_id, tipo) DO UPDATE SET ativa = EXCLUDED.ativa",
    )
    .bind(ctx.org_id)
    .bind(TIPO_INTEGRACAO)
    .bind(alteracao.ativa)
    .execute(&mut *tx)
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "alterar",
        "integracao",
        Some(TIPO_INTEGRACAO),
        json!({ "ativa": alteracao.ativa }),
        cliente.ip,
    )
    .await?;
    let resposta = carregar_estado(&mut tx, &estado, ctx.org_id).await?;
    tx.commit().await?;
    Ok(dados(resposta))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PedidoSincronizacao {
    ano: Option<i32>,
}

async fn sincronizar_agora(
    ctx: OrgCtx,
    cliente: ClienteInfo,
    State(estado): State<AppState>,
    Corpo(pedido): Corpo<PedidoSincronizacao>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    if estado.config.transparencia_key.is_none() {
        return Err(ErroApi::indisponivel(MSG_NAO_CONFIGURADA));
    }
    let ano_atual = estado.hoje().year();
    let ano = pedido.ano.unwrap_or(ano_atual);
    if !(2014..=ano_atual).contains(&ano) {
        return Err(ErroApi::validacao_campo(
            "ano",
            format!("Informe um ano entre 2014 e {ano_atual}."),
        ));
    }
    let agora = estado.agora();
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let plano = plano_efetivo(&mut tx, ctx.org_id, agora).await?;
    if !limites_do_plano(plano).transparencia {
        return Err(ErroApi::limite_plano(MSG_PLANO));
    }
    // Serializa pedidos simultâneos da mesma organização.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 7262102))")
        .bind(ctx.org_id)
        .execute(&mut *tx)
        .await?;
    let em_andamento: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM jobs WHERE organizacao_id = $1 AND tipo = $2
                        AND status IN ('pendente', 'executando'))",
    )
    .bind(ctx.org_id)
    .bind(jobs::TIPO_TRANSPARENCIA)
    .fetch_one(&mut *tx)
    .await?;
    if em_andamento {
        return Err(ErroApi::conflito(
            "Já existe uma sincronização em andamento.",
        ));
    }
    let ultima_manual: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT ultima_sincronizacao_manual_em FROM integracoes
         WHERE organizacao_id = $1 AND tipo = $2",
    )
    .bind(ctx.org_id)
    .bind(TIPO_INTEGRACAO)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    let intervalo = TimeDelta::minutes(INTERVALO_MANUAL_MINUTOS);
    if let Some(ultima) = ultima_manual
        && agora < ultima + intervalo
    {
        let liberada = (ultima + intervalo - TimeDelta::hours(3)).format("%H:%M");
        return Err(ErroApi::conflito(format!(
            "A sincronização manual pode ser pedida uma vez por hora; tente novamente a partir das {liberada}."
        )));
    }
    // O job só existe se a transação confirmar (junto com a auditoria).
    let job_id = jobs::enfileirar_em(
        &mut tx,
        jobs::TIPO_TRANSPARENCIA,
        Some(ctx.org_id),
        json!({ "org_id": ctx.org_id, "ano": ano }),
        None,
    )
    .await?;
    sqlx::query(
        "INSERT INTO integracoes (organizacao_id, tipo, ultima_sincronizacao_manual_em)
         VALUES ($1, $2, $3)
         ON CONFLICT (organizacao_id, tipo) DO UPDATE
             SET ultima_sincronizacao_manual_em = EXCLUDED.ultima_sincronizacao_manual_em",
    )
    .bind(ctx.org_id)
    .bind(TIPO_INTEGRACAO)
    .bind(agora)
    .execute(&mut *tx)
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "sincronizar",
        "integracao",
        Some(TIPO_INTEGRACAO),
        json!({ "ano": ano, "job_id": job_id }),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::ACCEPTED, dados(json!({ "job_id": job_id }))))
}

#[derive(Debug, Deserialize)]
struct FiltroConsulta {
    situacao: Option<String>,
    fase: Option<String>,
    #[serde(flatten)]
    paginacao: Paginacao,
}

async fn listar_documentos(
    ctx: OrgCtx,
    State(estado): State<AppState>,
    Consulta(filtro): Consulta<FiltroConsulta>,
) -> Result<impl IntoResponse, ErroApi> {
    let situacao = match filtro.situacao.as_deref().map(str::trim) {
        None | Some("") => Some("novo"),
        Some("todos") => None,
        Some(s @ ("novo" | "vinculado" | "ignorado")) => Some(s),
        Some(_) => {
            return Err(ErroApi::validacao_campo(
                "situacao",
                "Use novo, vinculado, ignorado ou todos.",
            ));
        }
    };
    let fase = match filtro.fase.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(texto) => Some(
            texto
                .parse::<i16>()
                .ok()
                .filter(|f| (1..=3).contains(f))
                .ok_or_else(|| ErroApi::validacao_campo("fase", "Use 1, 2 ou 3."))?,
        ),
    };
    let p = filtro.paginacao;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let (itens, total) = documentos::listar(
        &mut tx,
        ctx.org_id,
        FiltroDocumentos {
            situacao,
            fase,
            limite: p.limite(),
            offset: p.offset(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(p.lista(itens, total))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Vinculo {
    empenho_id: Option<Uuid>,
    nota_id: Option<Uuid>,
}

async fn vincular(
    ctx: OrgCtx,
    cliente: ClienteInfo,
    State(estado): State<AppState>,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
    Corpo(vinculo): Corpo<Vinculo>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let doc = documentos::vincular(
        &mut tx,
        ctx.org_id,
        ctx.usuario_id,
        id,
        vinculo.empenho_id,
        vinculo.nota_id,
        estado.hoje(),
    )
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "vincular",
        "documento_transparencia",
        Some(&id.to_string()),
        json!({ "codigo": doc.codigo, "empenho_id": doc.empenho_id, "nota_id": doc.nota_id }),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(dados(doc))
}

async fn importar(
    ctx: OrgCtx,
    cliente: ClienteInfo,
    State(estado): State<AppState>,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let (doc, empenho) =
        documentos::importar(&mut tx, ctx.org_id, ctx.usuario_id, id, estado.agora()).await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "importar",
        "documento_transparencia",
        Some(&id.to_string()),
        json!({ "codigo": doc.codigo, "empenho_id": doc.empenho_id }),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(criado(empenho))
}

async fn ignorar(
    ctx: OrgCtx,
    cliente: ClienteInfo,
    State(estado): State<AppState>,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let doc = documentos::ignorar(&mut tx, ctx.org_id, id).await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "ignorar",
        "documento_transparencia",
        Some(&id.to_string()),
        json!({ "codigo": doc.codigo }),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(dados(doc))
}
