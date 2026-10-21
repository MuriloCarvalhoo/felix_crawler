//! Exportação CSV (planilhas em português): `;`, UTF-8 com BOM, valores `1234,56`,
//! datas `dd/mm/aaaa`; campos de texto que começam com `= + - @` (ou tab/CR)
//! recebem `'` para neutralizar fórmulas. Dono: agente G.
//!
//! Até [`MAX_LINHAS`] linhas por arquivo; acima disso → 422 sugerindo o filtro
//! opcional `?ano=AAAA` (ano da data de emissão). As linhas são lidas do banco
//! em fluxo e escritas direto no CSV (sem cópia intermediária).

use crate::{
    app::AppState,
    db,
    http::{Consulta, ErroApi, OrgCtx},
    modulos::{
        empenhos::{
            comum::{anexo, data_br},
            repo::FROM_EMPENHO,
        },
        notas::{repo::SELECT_NOTA, tipos::Nota},
    },
};
use axum::{Router, extract::State, response::Response, routing::get};
use chrono::{DateTime, NaiveDate, Utc};
use futures_util::TryStreamExt;
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org}/exportar/empenhos.csv", get(empenhos_csv))
        .route("/orgs/{org}/exportar/notas.csv", get(notas_csv))
}

const BOM: &[u8] = b"\xEF\xBB\xBF";
/// Linhas por exportação.
pub const MAX_LINHAS: i64 = 50_000;

/// Query opcional das exportações.
#[derive(Debug, Default, Deserialize)]
pub struct FiltroExportacao {
    #[serde(default)]
    ano: Option<String>,
}

impl FiltroExportacao {
    fn ano(&self) -> Result<Option<i32>, ErroApi> {
        match self.ano.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(texto) => texto
                .parse::<i32>()
                .ok()
                .filter(|a| (2000..=2100).contains(a))
                .map(Some)
                .ok_or_else(|| ErroApi::validacao_campo("ano", "Informe o ano com 4 dígitos.")),
        }
    }
}

/// 422 se a exportação passar de [`MAX_LINHAS`].
async fn conferir_limite(
    conn: &mut PgConnection,
    sql_contagem: &str,
    org: Uuid,
    ano: Option<i32>,
) -> Result<(), ErroApi> {
    let total: i64 = sqlx::query_scalar(sql_contagem)
        .bind(org)
        .bind(ano)
        .fetch_one(conn)
        .await?;
    if total > MAX_LINHAS {
        return Err(ErroApi::validacao(format!(
            "A exportação teria {total} linhas; o limite é {MAX_LINHAS}. \
             Filtre por ano de emissão (ex.: ?ano=2026) e exporte em partes."
        )));
    }
    Ok(())
}

/// Escritor CSV (`;`, BOM) sobre um buffer em memória.
fn novo_escritor(cabecalho: &[&str]) -> Result<csv::Writer<Vec<u8>>, ErroApi> {
    let mut escritor = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_writer(BOM.to_vec());
    escritor
        .write_record(cabecalho)
        .map_err(|e| ErroApi::interno_log("csv", e))?;
    Ok(escritor)
}

fn concluir(escritor: csv::Writer<Vec<u8>>) -> Result<Vec<u8>, ErroApi> {
    escritor
        .into_inner()
        .map_err(|e| ErroApi::interno_log("csv", e.error().to_string()))
}
const CONTENT_TYPE_CSV: &str = "text/csv; charset=utf-8";

/// `1234,56` (sem separador de milhar).
pub(crate) fn valor_csv(centavos: i64) -> String {
    let sinal = if centavos < 0 { "-" } else { "" };
    let abs = centavos.unsigned_abs();
    format!("{sinal}{},{:02}", abs / 100, abs % 100)
}

/// Neutraliza injeção de fórmula em planilhas.
pub(crate) fn texto_csv(texto: &str) -> String {
    match texto.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{texto}"),
        _ => texto.to_owned(),
    }
}

fn opt_texto(texto: Option<&str>) -> String {
    texto.map(texto_csv).unwrap_or_default()
}

fn opt_data(data: Option<NaiveDate>) -> String {
    data.map(data_br).unwrap_or_default()
}

fn rotulo_status(status: &str) -> &str {
    match status {
        "ativo" => "Ativo",
        "anulado" => "Anulado",
        "emitida" => "Emitida",
        "liquidada" => "Liquidada",
        "paga" => "Paga",
        "cancelada" => "Cancelada",
        outro => outro,
    }
}

#[cfg(test)]
fn gerar_csv(cabecalho: &[&str], linhas: Vec<Vec<String>>) -> Result<Vec<u8>, ErroApi> {
    let mut escritor = novo_escritor(cabecalho)?;
    for linha in linhas {
        escritor
            .write_record(&linha)
            .map_err(|e| ErroApi::interno_log("csv", e))?;
    }
    concluir(escritor)
}

#[derive(sqlx::FromRow)]
struct LinhaEmpenho {
    razao_social: String,
    empresa_cnpj: String,
    ug: String,
    ug_nome: Option<String>,
    orgao: Option<String>,
    gestao: Option<String>,
    numero: String,
    modalidade: Option<String>,
    numero_edital_ata: Option<String>,
    data_emissao: Option<NaiveDate>,
    valor_total_centavos: i64,
    faturado_centavos: i64,
    recebido_centavos: i64,
    saldo_centavos: i64,
    status: String,
    origem: String,
    regra_vencimento: String,
    prazo_dias: i32,
    observacao: Option<String>,
    criado_em: DateTime<Utc>,
}

async fn empenhos_csv(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Consulta(filtro): Consulta<FiltroExportacao>,
) -> Result<Response, ErroApi> {
    let ano = filtro.ano()?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    conferir_limite(
        &mut tx,
        "SELECT count(*) FROM empenhos e WHERE e.organizacao_id = $1
           AND ($2::int IS NULL OR extract(year FROM e.data_emissao) = $2)",
        ctx.org_id,
        ano,
    )
    .await?;
    let sql = format!(
        "SELECT emp.razao_social, emp.cnpj::text AS empresa_cnpj, e.ug::text AS ug, e.ug_nome,
                e.orgao, e.gestao, e.numero, e.modalidade, e.numero_edital_ata, e.data_emissao,
                e.valor_total_centavos,
                COALESCE(a.faturado, 0)::bigint AS faturado_centavos,
                COALESCE(a.recebido, 0)::bigint AS recebido_centavos,
                (e.valor_total_centavos - COALESCE(a.faturado, 0))::bigint AS saldo_centavos,
                e.status, e.origem, e.regra_vencimento, e.prazo_dias, e.observacao, e.criado_em
         {FROM_EMPENHO}
          WHERE e.organizacao_id = $1
            AND ($2::int IS NULL OR extract(year FROM e.data_emissao) = $2)
          ORDER BY e.ug, e.numero"
    );
    let cabecalho = [
        "Empresa",
        "CNPJ",
        "UG",
        "Nome da UG",
        "Órgão",
        "Gestão",
        "Empenho",
        "Modalidade",
        "Edital/Ata",
        "Emissão",
        "Valor total (R$)",
        "Faturado (R$)",
        "Recebido (R$)",
        "Saldo (R$)",
        "Situação",
        "Origem",
        "Regra de vencimento",
        "Prazo (dias)",
        "Observação",
        "Cadastrado em",
    ];
    let mut escritor = novo_escritor(&cabecalho)?;
    let mut linhas = sqlx::query_as::<_, LinhaEmpenho>(&sql)
        .bind(ctx.org_id)
        .bind(ano)
        .fetch(&mut *tx);
    while let Some(l) = linhas.try_next().await? {
        escritor
            .write_record([
                texto_csv(&l.razao_social),
                l.empresa_cnpj,
                l.ug,
                opt_texto(l.ug_nome.as_deref()),
                opt_texto(l.orgao.as_deref()),
                opt_texto(l.gestao.as_deref()),
                texto_csv(&l.numero),
                opt_texto(l.modalidade.as_deref()),
                opt_texto(l.numero_edital_ata.as_deref()),
                opt_data(l.data_emissao),
                valor_csv(l.valor_total_centavos),
                valor_csv(l.faturado_centavos),
                valor_csv(l.recebido_centavos),
                valor_csv(l.saldo_centavos),
                rotulo_status(&l.status).to_owned(),
                l.origem,
                if l.regra_vencimento == "in77_uteis" {
                    "IN 77/2022 (dias úteis)".to_owned()
                } else {
                    "Dias corridos da emissão".to_owned()
                },
                l.prazo_dias.to_string(),
                opt_texto(l.observacao.as_deref()),
                data_br(crate::app::data_brt(l.criado_em)),
            ])
            .map_err(|e| ErroApi::interno_log("csv", e))?;
    }
    drop(linhas);
    tx.commit().await?;
    anexo(
        concluir(escritor)?,
        CONTENT_TYPE_CSV,
        &nome_arquivo("empenhos", &estado),
    )
}

fn nome_arquivo(base: &str, estado: &AppState) -> String {
    format!("{base}_{}.csv", estado.hoje().format("%Y-%m-%d"))
}

async fn notas_csv(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Consulta(filtro): Consulta<FiltroExportacao>,
) -> Result<Response, ErroApi> {
    let ano = filtro.ano()?;
    let hoje = estado.hoje();
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    conferir_limite(
        &mut tx,
        "SELECT count(*) FROM notas n WHERE n.organizacao_id = $1
           AND ($2::int IS NULL OR extract(year FROM n.data_emissao) = $2)",
        ctx.org_id,
        ano,
    )
    .await?;
    let sql = format!(
        "{SELECT_NOTA} WHERE n.organizacao_id = $1
           AND ($2::int IS NULL OR extract(year FROM n.data_emissao) = $2)
         ORDER BY n.vencimento, n.criado_em"
    );
    let cabecalho = [
        "Empenho",
        "UG",
        "Nome da UG",
        "CNPJ",
        "Nota fiscal",
        "Tipo",
        "Valor (R$)",
        "Emissão",
        "Recebimento",
        "Vencimento",
        "Situação",
        "Dias de atraso",
        "Valor pago (R$)",
        "Liquidação",
        "Pagamento",
        "Documento de liquidação",
        "Documento de pagamento",
    ];
    let mut escritor = novo_escritor(&cabecalho)?;
    let mut notas = sqlx::query_as::<_, Nota>(&sql)
        .bind(ctx.org_id)
        .bind(ano)
        .fetch(&mut *tx);
    while let Some(n) = notas.try_next().await? {
        let n = n.com_atraso(hoje);
        escritor
            .write_record([
                texto_csv(&n.empenho_numero),
                n.ug,
                opt_texto(n.ug_nome.as_deref()),
                n.empresa_cnpj,
                texto_csv(&n.numero),
                if n.tipo == "servico" {
                    "Serviço".to_owned()
                } else {
                    "Produto".to_owned()
                },
                valor_csv(n.valor_centavos),
                data_br(n.data_emissao),
                opt_data(n.data_recebimento),
                data_br(n.vencimento),
                rotulo_status(&n.status).to_owned(),
                n.dias_atraso.to_string(),
                valor_csv(n.valor_pago_centavos),
                opt_data(n.data_liquidacao),
                opt_data(n.data_pagamento),
                opt_texto(n.documento_liquidacao.as_deref()),
                opt_texto(n.documento_pagamento.as_deref()),
            ])
            .map_err(|e| ErroApi::interno_log("csv", e))?;
    }
    drop(notas);
    tx.commit().await?;
    anexo(
        concluir(escritor)?,
        CONTENT_TYPE_CSV,
        &nome_arquivo("notas", &estado),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod testes {
    use super::*;

    #[test]
    fn valores_e_formulas() {
        assert_eq!(valor_csv(123_456), "1234,56");
        assert_eq!(valor_csv(5), "0,05");
        assert_eq!(valor_csv(-5), "-0,05");
        for perigoso in ["=1+1", "+1", "-1", "@SUM(A1)", "\tx"] {
            assert!(texto_csv(perigoso).starts_with('\''));
        }
        assert_eq!(texto_csv("NF 1"), "NF 1");
    }

    #[test]
    fn csv_com_bom_e_ponto_e_virgula() {
        let csv = gerar_csv(&["A", "B"], vec![vec!["x;y".into(), "1,00".into()]]).unwrap();
        assert!(csv.starts_with(BOM));
        let texto = String::from_utf8(csv.get(3..).unwrap().to_vec()).unwrap();
        assert_eq!(texto, "A;B\n\"x;y\";1,00\n");
    }
}
