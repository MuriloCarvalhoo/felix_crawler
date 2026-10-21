//! Tipos das cobranças (03-api-v1.md, seção Cobranças).

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CANAIS: [&str; 5] = ["email", "protocolo", "sei", "correios", "outro"];
/// Teto das taxas (100% em pontos-base), igual ao CHECK da tabela `cobrancas`.
pub use felix_core::cobranca::MAX_BPS;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CobrancaResumo {
    pub id: Uuid,
    pub nota_id: Uuid,
    pub nota_numero: String,
    pub empenho_numero: String,
    pub ug: String,
    pub canal: String,
    pub protocolo: Option<String>,
    pub enviada_em: NaiveDate,
    pub total_centavos: i64,
    pub dias_atraso: i32,
    pub criado_por_nome: Option<String>,
    pub criado_em: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Cobranca {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub resumo: CobrancaResumo,
    pub multa_bps: i32,
    pub juros_bps: i32,
    pub data_referencia: NaiveDate,
    pub principal_centavos: i64,
    pub multa_centavos: i64,
    pub juros_centavos: i64,
    pub observacao: Option<String>,
    pub texto: String,
}

/// Query de `/calculo` e `/oficio`.
#[derive(Debug, Default, Deserialize)]
pub struct ParametrosCalculo {
    #[serde(default)]
    pub multa_bps: u32,
    #[serde(default)]
    pub juros_bps: u32,
    #[serde(default)]
    pub data_referencia: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct NovaCobranca {
    #[serde(default)]
    pub multa_bps: u32,
    #[serde(default)]
    pub juros_bps: u32,
    pub canal: String,
    #[serde(default)]
    pub protocolo: Option<String>,
    pub enviada_em: NaiveDate,
    #[serde(default)]
    pub observacao: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FiltroCobrancas {
    #[serde(default)]
    pub nota_id: Option<String>,
    #[serde(flatten)]
    pub paginacao: crate::http::Paginacao,
}

/// Dados da nota, do empenho e da empresa usados no cálculo e no ofício.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContextoOficio {
    pub nota_id: Uuid,
    pub nota_numero: String,
    pub nota_tipo: String,
    pub nota_status: String,
    pub valor_centavos: i64,
    pub data_emissao: NaiveDate,
    pub data_recebimento: Option<NaiveDate>,
    pub data_pagamento: Option<NaiveDate>,
    pub vencimento: NaiveDate,
    pub empenho_numero: String,
    pub ug: String,
    pub ug_nome: Option<String>,
    pub orgao: Option<String>,
    pub modalidade: Option<String>,
    pub numero_edital_ata: Option<String>,
    pub razao_social: String,
    pub cnpj: String,
}
