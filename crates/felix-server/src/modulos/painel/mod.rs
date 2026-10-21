//! Painel de contas a receber (03-api-v1.md, seção Painel). Dono: agente G.
//!
//! "Aberta" = nota emitida ou liquidada. Datas relativas usam `estado.hoje()` (BRT).

use crate::{
    app::AppState,
    db,
    http::{ErroApi, OrgCtx, dados},
    modulos::notas::{repo::SELECT_NOTA, tipos::Nota},
};
use axum::{Router, extract::State, response::IntoResponse, routing::get};
use chrono::{Datelike, Months, NaiveDate};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

pub fn rotas() -> Router<AppState> {
    Router::new().route("/orgs/{org}/painel", get(painel))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Totais {
    a_receber_centavos: i64,
    vencido_centavos: i64,
    vence_30d_centavos: i64,
    recebido_mes_centavos: i64,
    notas_abertas: i64,
    notas_vencidas: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct TotaisEmpenhos {
    empenhos_ativos: i64,
    saldo_empenhos_centavos: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Faixa {
    faixa: String,
    centavos: i64,
    quantidade: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct PorUg {
    ug: String,
    ug_nome: Option<String>,
    aberto_centavos: i64,
    vencido_centavos: i64,
    /// Média de dias de atraso das notas vencidas da UG (0 se nenhuma).
    atraso_medio_dias: i64,
}

#[derive(Debug, Serialize)]
struct Painel {
    #[serde(flatten)]
    totais: Totais,
    #[serde(flatten)]
    empenhos: TotaisEmpenhos,
    aging: Vec<Faixa>,
    por_ug: Vec<PorUg>,
    proximos_vencimentos: Vec<Nota>,
    mais_atrasadas: Vec<Nota>,
}

const FAIXAS: [&str; 5] = ["a_vencer", "1_30", "31_60", "61_90", "90_mais"];
const LIMITE_LISTAS: i64 = 10;

async fn painel(State(estado): State<AppState>, ctx: OrgCtx) -> Result<impl IntoResponse, ErroApi> {
    let hoje = estado.hoje();
    let inicio_mes = hoje.with_day(1).unwrap_or(hoje);
    let fim_mes = inicio_mes
        .checked_add_months(Months::new(1))
        .unwrap_or(NaiveDate::MAX);
    let org = ctx.org_id;
    let mut tx = db::tenant(&estado.pool, org).await?;
    let totais = totais(&mut tx, org, hoje, inicio_mes, fim_mes).await?;
    let empenhos: TotaisEmpenhos = sqlx::query_as(
        "SELECT COUNT(*) AS empenhos_ativos,
                COALESCE(SUM(e.valor_total_centavos - COALESCE(f.faturado, 0)), 0)::bigint
                    AS saldo_empenhos_centavos
           FROM empenhos e
           LEFT JOIN (SELECT empenho_id, SUM(valor_centavos) AS faturado
                        FROM notas WHERE organizacao_id = $1 AND status <> 'cancelada'
                       GROUP BY empenho_id) f ON f.empenho_id = e.id
          WHERE e.organizacao_id = $1 AND e.status = 'ativo'",
    )
    .bind(org)
    .fetch_one(&mut *tx)
    .await?;
    let aging = aging(&mut tx, org, hoje).await?;
    let por_ug: Vec<PorUg> = sqlx::query_as(
        "SELECT e.ug::text AS ug, MAX(e.ug_nome) AS ug_nome,
                SUM(n.valor_centavos)::bigint AS aberto_centavos,
                COALESCE(SUM(n.valor_centavos) FILTER (WHERE n.vencimento < $2), 0)::bigint
                    AS vencido_centavos,
                COALESCE(ROUND(AVG($2 - n.vencimento) FILTER (WHERE n.vencimento < $2)), 0)::bigint
                    AS atraso_medio_dias
           FROM notas n
           JOIN empenhos e ON e.id = n.empenho_id AND e.organizacao_id = n.organizacao_id
          WHERE n.organizacao_id = $1 AND n.status IN ('emitida', 'liquidada')
          GROUP BY e.ug
          ORDER BY aberto_centavos DESC, e.ug
          LIMIT $3",
    )
    .bind(org)
    .bind(hoje)
    .bind(LIMITE_LISTAS)
    .fetch_all(&mut *tx)
    .await?;
    let proximos_vencimentos = notas_abertas(&mut tx, org, hoje, false).await?;
    let mais_atrasadas = notas_abertas(&mut tx, org, hoje, true).await?;
    tx.commit().await?;
    Ok(dados(Painel {
        totais,
        empenhos,
        aging,
        por_ug,
        proximos_vencimentos,
        mais_atrasadas,
    }))
}

async fn totais(
    conn: &mut PgConnection,
    org: Uuid,
    hoje: NaiveDate,
    inicio_mes: NaiveDate,
    fim_mes: NaiveDate,
) -> Result<Totais, ErroApi> {
    Ok(sqlx::query_as(
        "WITH n AS (
             SELECT valor_centavos, valor_pago_centavos, vencimento, data_pagamento, status,
                    status IN ('emitida', 'liquidada') AS aberta
               FROM notas WHERE organizacao_id = $1)
         SELECT COALESCE(SUM(valor_centavos) FILTER (WHERE aberta), 0)::bigint
                    AS a_receber_centavos,
                COALESCE(SUM(valor_centavos) FILTER (WHERE aberta AND vencimento < $2), 0)::bigint
                    AS vencido_centavos,
                COALESCE(SUM(valor_centavos) FILTER (
                    WHERE aberta AND vencimento BETWEEN $2 AND $2 + 30), 0)::bigint
                    AS vence_30d_centavos,
                COALESCE(SUM(valor_pago_centavos) FILTER (
                    WHERE status = 'paga' AND data_pagamento >= $3 AND data_pagamento < $4), 0)::bigint
                    AS recebido_mes_centavos,
                COUNT(*) FILTER (WHERE aberta) AS notas_abertas,
                COUNT(*) FILTER (WHERE aberta AND vencimento < $2) AS notas_vencidas
           FROM n",
    )
    .bind(org)
    .bind(hoje)
    .bind(inicio_mes)
    .bind(fim_mes)
    .fetch_one(conn)
    .await?)
}

async fn aging(conn: &mut PgConnection, org: Uuid, hoje: NaiveDate) -> Result<Vec<Faixa>, ErroApi> {
    let linhas: Vec<Faixa> = sqlx::query_as(
        "SELECT CASE
                  WHEN $2 - vencimento <= 0 THEN 'a_vencer'
                  WHEN $2 - vencimento <= 30 THEN '1_30'
                  WHEN $2 - vencimento <= 60 THEN '31_60'
                  WHEN $2 - vencimento <= 90 THEN '61_90'
                  ELSE '90_mais'
                END AS faixa,
                SUM(valor_centavos)::bigint AS centavos,
                COUNT(*) AS quantidade
           FROM notas
          WHERE organizacao_id = $1 AND status IN ('emitida', 'liquidada')
          GROUP BY 1",
    )
    .bind(org)
    .bind(hoje)
    .fetch_all(conn)
    .await?;
    Ok(FAIXAS
        .iter()
        .map(|faixa| {
            linhas.iter().find(|l| l.faixa == *faixa).map_or_else(
                || Faixa {
                    faixa: (*faixa).to_owned(),
                    centavos: 0,
                    quantidade: 0,
                },
                |l| Faixa {
                    faixa: l.faixa.clone(),
                    centavos: l.centavos,
                    quantidade: l.quantidade,
                },
            )
        })
        .collect())
}

/// Até 10 notas abertas: a vencer (vencimento mais próximo) ou vencidas (mais antigas).
async fn notas_abertas(
    conn: &mut PgConnection,
    org: Uuid,
    hoje: NaiveDate,
    vencidas: bool,
) -> Result<Vec<Nota>, ErroApi> {
    let condicao = if vencidas {
        "n.vencimento < $2"
    } else {
        "n.vencimento >= $2"
    };
    let notas: Vec<Nota> = sqlx::query_as(&format!(
        "{SELECT_NOTA}
          WHERE n.organizacao_id = $1 AND n.status IN ('emitida', 'liquidada') AND {condicao}
          ORDER BY n.vencimento ASC, n.valor_centavos DESC, n.id
          LIMIT $3"
    ))
    .bind(org)
    .bind(hoje)
    .bind(LIMITE_LISTAS)
    .fetch_all(conn)
    .await?;
    Ok(notas.into_iter().map(|n| n.com_atraso(hoje)).collect())
}
