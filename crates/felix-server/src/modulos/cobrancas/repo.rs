//! SQL das cobranças (sempre em transação de tenant).

use super::tipos::{Cobranca, CobrancaResumo, ContextoOficio};
use crate::http::ErroApi;
use sqlx::PgConnection;
use uuid::Uuid;

const SELECT_RESUMO: &str = "SELECT c.id, c.nota_id, n.numero AS nota_numero,
       e.numero AS empenho_numero, e.ug::text AS ug, c.canal, c.protocolo, c.enviada_em,
       c.total_centavos, c.dias_atraso, u.nome AS criado_por_nome, c.criado_em";

const FROM_COBRANCA: &str = "FROM cobrancas c
  JOIN notas n ON n.id = c.nota_id AND n.organizacao_id = c.organizacao_id
  JOIN empenhos e ON e.id = n.empenho_id AND e.organizacao_id = n.organizacao_id
  LEFT JOIN usuarios u ON u.id = c.criado_por";

/// Cobranças de uma nota (mais recentes primeiro).
pub async fn da_nota(
    conn: &mut PgConnection,
    org: Uuid,
    nota_id: Uuid,
) -> Result<Vec<CobrancaResumo>, ErroApi> {
    Ok(sqlx::query_as(&format!(
        "{SELECT_RESUMO} {FROM_COBRANCA}
          WHERE c.organizacao_id = $1 AND c.nota_id = $2
          ORDER BY c.criado_em DESC, c.id DESC"
    ))
    .bind(org)
    .bind(nota_id)
    .fetch_all(conn)
    .await?)
}

pub async fn listar(
    conn: &mut PgConnection,
    org: Uuid,
    nota_id: Option<Uuid>,
    limite: i64,
    offset: i64,
) -> Result<(Vec<CobrancaResumo>, i64), ErroApi> {
    let itens: Vec<CobrancaResumo> = sqlx::query_as(&format!(
        "{SELECT_RESUMO} {FROM_COBRANCA}
          WHERE c.organizacao_id = $1 AND ($2::uuid IS NULL OR c.nota_id = $2)
          ORDER BY c.criado_em DESC, c.id DESC LIMIT $3 OFFSET $4"
    ))
    .bind(org)
    .bind(nota_id)
    .bind(limite)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cobrancas
          WHERE organizacao_id = $1 AND ($2::uuid IS NULL OR nota_id = $2)",
    )
    .bind(org)
    .bind(nota_id)
    .fetch_one(conn)
    .await?;
    Ok((itens, total))
}

pub async fn buscar(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<Cobranca, ErroApi> {
    let cobranca: Option<Cobranca> = sqlx::query_as(&format!(
        "{SELECT_RESUMO}, c.multa_bps, c.juros_bps, c.data_referencia, c.principal_centavos,
                c.multa_centavos, c.juros_centavos, c.observacao, c.texto
         {FROM_COBRANCA}
          WHERE c.organizacao_id = $1 AND c.id = $2"
    ))
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?;
    cobranca.ok_or_else(|| ErroApi::nao_encontrado_msg("Cobrança não encontrada."))
}

/// Nota + empenho + empresa, ou 404.
pub async fn contexto(
    conn: &mut PgConnection,
    org: Uuid,
    nota_id: Uuid,
) -> Result<ContextoOficio, ErroApi> {
    let contexto: Option<ContextoOficio> = sqlx::query_as(
        "SELECT n.id AS nota_id, n.numero AS nota_numero, n.tipo AS nota_tipo,
                n.status AS nota_status, n.valor_centavos, n.data_emissao, n.data_recebimento,
                n.data_pagamento, n.vencimento, e.numero AS empenho_numero, e.ug::text AS ug,
                e.ug_nome, e.orgao, e.modalidade, e.numero_edital_ata, emp.razao_social,
                emp.cnpj::text AS cnpj
           FROM notas n
           JOIN empenhos e ON e.id = n.empenho_id AND e.organizacao_id = n.organizacao_id
           JOIN empresas emp ON emp.id = e.empresa_id AND emp.organizacao_id = e.organizacao_id
          WHERE n.organizacao_id = $1 AND n.id = $2",
    )
    .bind(org)
    .bind(nota_id)
    .fetch_optional(conn)
    .await?;
    contexto.ok_or_else(|| ErroApi::nao_encontrado_msg("Nota fiscal não encontrada."))
}
