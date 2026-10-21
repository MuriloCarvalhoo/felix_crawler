//! SQL das notas fiscais. Toda função recebe a conexão de uma transação de
//! tenant (`db::tenant`) e filtra também por `organizacao_id`.

use super::tipos::{Evento, Nota};
use crate::http::ErroApi;
use chrono::NaiveDate;
use felix_core::prazos::{self, RegraVencimento};
use sqlx::PgConnection;
use uuid::Uuid;

/// SELECT base de `Nota` (aliases: `n` nota, `e` empenho, `emp` empresa).
pub const SELECT_NOTA: &str =
    "SELECT n.id, n.empenho_id, e.numero AS empenho_numero, e.ug::text AS ug,
       e.ug_nome, emp.cnpj::text AS empresa_cnpj, n.numero, n.tipo, n.valor_centavos,
       n.data_emissao, n.data_recebimento, n.vencimento, n.status, n.valor_pago_centavos,
       n.data_liquidacao, n.data_pagamento, n.documento_liquidacao, n.documento_pagamento,
       n.criado_em
  FROM notas n
  JOIN empenhos e ON e.id = n.empenho_id AND e.organizacao_id = n.organizacao_id
  JOIN empresas emp ON emp.id = e.empresa_id AND emp.organizacao_id = e.organizacao_id";

/// Dados do empenho necessários para gravar notas, com a linha travada.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmpenhoTravado {
    pub id: Uuid,
    pub status: String,
    pub valor_total_centavos: i64,
    pub regra_vencimento: String,
    pub prazo_dias: i32,
}

impl EmpenhoTravado {
    /// Vencimento de uma nota deste empenho pela regra configurada.
    pub fn vencimento(
        &self,
        data_emissao: NaiveDate,
        data_recebimento: Option<NaiveDate>,
    ) -> Result<NaiveDate, ErroApi> {
        calcular_vencimento(
            &self.regra_vencimento,
            self.prazo_dias,
            self.valor_total_centavos,
            data_emissao,
            data_recebimento,
        )
    }
}

/// Vencimento pela regra (ADR-010); erros de regra/prazo viram 422.
pub fn calcular_vencimento(
    regra: &str,
    prazo_dias: i32,
    valor_empenho_centavos: i64,
    data_emissao: NaiveDate,
    data_recebimento: Option<NaiveDate>,
) -> Result<NaiveDate, ErroApi> {
    let regra: RegraVencimento = regra
        .parse()
        .map_err(|e: prazos::ErroPrazo| ErroApi::validacao(e.to_string()))?;
    let prazo =
        u32::try_from(prazo_dias).map_err(|_| ErroApi::validacao("Prazo do empenho inválido."))?;
    prazos::calcular_vencimento(
        regra,
        data_emissao,
        data_recebimento,
        prazo,
        valor_empenho_centavos,
    )
    .map_err(|e| ErroApi::validacao(format!("Não foi possível calcular o vencimento: {e}.")))
}

/// `SELECT ... FOR UPDATE` no empenho (serializa gravações que afetam o saldo).
pub async fn travar_empenho(
    conn: &mut PgConnection,
    org: Uuid,
    empenho_id: Uuid,
) -> Result<Option<EmpenhoTravado>, ErroApi> {
    Ok(sqlx::query_as(
        "SELECT id, status, valor_total_centavos, regra_vencimento, prazo_dias
           FROM empenhos WHERE organizacao_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(org)
    .bind(empenho_id)
    .fetch_optional(conn)
    .await?)
}

/// Soma das notas não canceladas do empenho, opcionalmente sem uma nota.
pub async fn faturado(
    conn: &mut PgConnection,
    org: Uuid,
    empenho_id: Uuid,
    excluir_nota: Option<Uuid>,
) -> Result<i64, ErroApi> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(SUM(valor_centavos), 0)::bigint FROM notas
          WHERE organizacao_id = $1 AND empenho_id = $2 AND status <> 'cancelada'
            AND ($3::uuid IS NULL OR id <> $3)",
    )
    .bind(org)
    .bind(empenho_id)
    .bind(excluir_nota)
    .fetch_one(conn)
    .await?)
}

/// Nota completa ou 404.
pub async fn buscar(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    hoje: NaiveDate,
) -> Result<Nota, ErroApi> {
    let nota: Option<Nota> = sqlx::query_as(&format!(
        "{SELECT_NOTA} WHERE n.organizacao_id = $1 AND n.id = $2"
    ))
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?;
    nota.map(|n| n.com_atraso(hoje))
        .ok_or_else(|| ErroApi::nao_encontrado_msg("Nota fiscal não encontrada."))
}

/// Notas de um empenho, ordenadas por emissão.
pub async fn do_empenho(
    conn: &mut PgConnection,
    org: Uuid,
    empenho_id: Uuid,
    hoje: NaiveDate,
) -> Result<Vec<Nota>, ErroApi> {
    let notas: Vec<Nota> = sqlx::query_as(&format!(
        "{SELECT_NOTA} WHERE n.organizacao_id = $1 AND n.empenho_id = $2
          ORDER BY n.data_emissao, n.criado_em"
    ))
    .bind(org)
    .bind(empenho_id)
    .fetch_all(conn)
    .await?;
    Ok(notas.into_iter().map(|n| n.com_atraso(hoje)).collect())
}

/// Filtros já validados de `GET /notas`.
pub struct Filtro {
    pub status: Option<Vec<String>>,
    pub empenho_id: Option<Uuid>,
    pub situacao: Option<String>,
    pub busca: Option<String>,
    pub ordenar_criado: bool,
    pub limite: i64,
    pub offset: i64,
}

const WHERE_LISTA: &str = "WHERE n.organizacao_id = $1
   AND ($2::text[] IS NULL OR n.status = ANY($2))
   AND ($3::uuid IS NULL OR n.empenho_id = $3)
   AND ($4::text IS NULL
        OR ($4 = 'vencida' AND n.status IN ('emitida','liquidada') AND n.vencimento < $5)
        OR ($4 = 'a_vencer' AND n.status IN ('emitida','liquidada') AND n.vencimento >= $5)
        OR ($4 = 'vence_7d' AND n.status IN ('emitida','liquidada')
            AND n.vencimento BETWEEN $5 AND $5 + 7))
   AND ($6::text IS NULL OR n.numero ILIKE $6 OR e.numero ILIKE $6 OR e.ug ILIKE $6
        OR e.ug_nome ILIKE $6 OR e.orgao ILIKE $6)";

pub async fn listar(
    conn: &mut PgConnection,
    org: Uuid,
    filtro: &Filtro,
    hoje: NaiveDate,
) -> Result<(Vec<Nota>, i64), ErroApi> {
    let ordem = if filtro.ordenar_criado {
        "n.criado_em DESC, n.id DESC"
    } else {
        "n.vencimento ASC, n.criado_em ASC, n.id ASC"
    };
    let notas: Vec<Nota> = sqlx::query_as(&format!(
        "{SELECT_NOTA} {WHERE_LISTA} ORDER BY {ordem} LIMIT $7 OFFSET $8"
    ))
    .bind(org)
    .bind(&filtro.status)
    .bind(filtro.empenho_id)
    .bind(&filtro.situacao)
    .bind(hoje)
    .bind(&filtro.busca)
    .bind(filtro.limite)
    .bind(filtro.offset)
    .fetch_all(&mut *conn)
    .await?;
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM notas n
           JOIN empenhos e ON e.id = n.empenho_id AND e.organizacao_id = n.organizacao_id
         {WHERE_LISTA}"
    ))
    .bind(org)
    .bind(&filtro.status)
    .bind(filtro.empenho_id)
    .bind(&filtro.situacao)
    .bind(hoje)
    .bind(&filtro.busca)
    .fetch_one(conn)
    .await?;
    Ok((
        notas.into_iter().map(|n| n.com_atraso(hoje)).collect(),
        total,
    ))
}

pub async fn eventos(
    conn: &mut PgConnection,
    org: Uuid,
    nota_id: Uuid,
) -> Result<Vec<Evento>, ErroApi> {
    Ok(sqlx::query_as(
        "SELECT ev.id, ev.tipo, ev.data, ev.valor_centavos, ev.documento, ev.observacao,
                ev.origem, u.nome AS criado_por_nome, ev.criado_em
           FROM eventos_nota ev
           LEFT JOIN usuarios u ON u.id = ev.criado_por
          WHERE ev.organizacao_id = $1 AND ev.nota_id = $2
          ORDER BY ev.criado_em, ev.id",
    )
    .bind(org)
    .bind(nota_id)
    .fetch_all(conn)
    .await?)
}
