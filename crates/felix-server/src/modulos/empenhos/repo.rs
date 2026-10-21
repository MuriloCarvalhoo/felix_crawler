//! SQL dos empenhos (transação de tenant + filtro explícito por organização).

use super::tipos::{Empenho, EmpenhoResumo};
use crate::http::ErroApi;
use sqlx::PgConnection;
use uuid::Uuid;

/// Colunas de `EmpenhoResumo` com faturado/recebido/saldo agregados das notas.
const COLUNAS_RESUMO: &str = "e.id, e.empresa_id, emp.cnpj::text AS empresa_cnpj, e.ug::text AS ug,
       e.ug_nome, e.orgao, e.numero, e.modalidade, e.valor_total_centavos,
       COALESCE(a.faturado, 0)::bigint AS faturado_centavos,
       COALESCE(a.recebido, 0)::bigint AS recebido_centavos,
       (e.valor_total_centavos - COALESCE(a.faturado, 0))::bigint AS saldo_centavos,
       e.status, e.origem, e.data_emissao, e.criado_em";

const COLUNAS_DETALHE: &str = ", e.gestao, e.numero_edital_ata, e.regra_vencimento, e.prazo_dias,
       e.observacao, e.anulado_em, e.motivo_anulacao";

/// FROM com empresa e agregados das notas por empenho.
pub const FROM_EMPENHO: &str = "FROM empenhos e
  JOIN empresas emp ON emp.id = e.empresa_id AND emp.organizacao_id = e.organizacao_id
  LEFT JOIN LATERAL (
      SELECT SUM(n.valor_centavos) FILTER (WHERE n.status <> 'cancelada') AS faturado,
             SUM(n.valor_pago_centavos) FILTER (WHERE n.status = 'paga') AS recebido
        FROM notas n
       WHERE n.organizacao_id = e.organizacao_id AND n.empenho_id = e.id
  ) a ON true";

pub async fn buscar(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<Empenho, ErroApi> {
    let empenho: Option<Empenho> = sqlx::query_as(&format!(
        "SELECT {COLUNAS_RESUMO}{COLUNAS_DETALHE} {FROM_EMPENHO}
          WHERE e.organizacao_id = $1 AND e.id = $2"
    ))
    .bind(org)
    .bind(id)
    .fetch_optional(conn)
    .await?;
    empenho.ok_or_else(|| ErroApi::nao_encontrado_msg("Empenho não encontrado."))
}

/// Filtros já validados de `GET /empenhos`.
pub struct Filtro {
    pub busca: Option<String>,
    pub empresa_id: Option<Uuid>,
    pub ug: Option<String>,
    /// `None` = todos.
    pub status: Option<String>,
    pub com_saldo: Option<bool>,
    pub limite: i64,
    pub offset: i64,
}

const WHERE_LISTA: &str = "WHERE e.organizacao_id = $1
   AND ($2::text IS NULL OR e.numero ILIKE $2 OR e.ug ILIKE $2 OR e.ug_nome ILIKE $2
        OR e.orgao ILIKE $2 OR e.numero_edital_ata ILIKE $2)
   AND ($3::uuid IS NULL OR e.empresa_id = $3)
   AND ($4::text IS NULL OR e.ug = $4)
   AND ($5::text IS NULL OR e.status = $5)
   AND ($6::bool IS NULL OR ((e.valor_total_centavos - COALESCE(a.faturado, 0)) > 0) = $6)";

pub async fn listar(
    conn: &mut PgConnection,
    org: Uuid,
    f: &Filtro,
) -> Result<(Vec<EmpenhoResumo>, i64), ErroApi> {
    let itens: Vec<EmpenhoResumo> = sqlx::query_as(&format!(
        "SELECT {COLUNAS_RESUMO} {FROM_EMPENHO} {WHERE_LISTA}
          ORDER BY e.criado_em DESC, e.id DESC LIMIT $7 OFFSET $8"
    ))
    .bind(org)
    .bind(&f.busca)
    .bind(f.empresa_id)
    .bind(&f.ug)
    .bind(&f.status)
    .bind(f.com_saldo)
    .bind(f.limite)
    .bind(f.offset)
    .fetch_all(&mut *conn)
    .await?;
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) {FROM_EMPENHO} {WHERE_LISTA}"))
        .bind(org)
        .bind(&f.busca)
        .bind(f.empresa_id)
        .bind(&f.ug)
        .bind(&f.status)
        .bind(f.com_saldo)
        .fetch_one(conn)
        .await?;
    Ok((itens, total))
}

/// Empenho da organização com a mesma UG e número (exceto `ignorar`).
pub async fn existe_ug_numero(
    conn: &mut PgConnection,
    org: Uuid,
    ug: &str,
    numero: &str,
    ignorar: Option<Uuid>,
) -> Result<bool, ErroApi> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM empenhos
                         WHERE organizacao_id = $1 AND ug = $2 AND upper(numero) = upper($3)
                           AND ($4::uuid IS NULL OR id <> $4))",
    )
    .bind(org)
    .bind(ug)
    .bind(numero)
    .bind(ignorar)
    .fetch_one(conn)
    .await?)
}
