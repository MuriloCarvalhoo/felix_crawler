//! Tipos de entrada e saída das notas fiscais.

use crate::modulos::empenhos::comum::anulavel;
use chrono::{DateTime, NaiveDate, Utc};
use felix_core::prazos;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const STATUS_NOTA: [&str; 4] = ["emitida", "liquidada", "paga", "cancelada"];
pub const TIPOS_NOTA: [&str; 2] = ["produto", "servico"];
pub const TAMANHO_NUMERO: usize = 40;
pub const TAMANHO_DOCUMENTO: usize = 100;
pub const TAMANHO_OBSERVACAO: usize = 1000;

/// `Nota` do contrato (03-api-v1.md).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Nota {
    pub id: Uuid,
    pub empenho_id: Uuid,
    pub empenho_numero: String,
    pub ug: String,
    pub ug_nome: Option<String>,
    pub empresa_cnpj: String,
    pub numero: String,
    pub tipo: String,
    pub valor_centavos: i64,
    pub data_emissao: NaiveDate,
    pub data_recebimento: Option<NaiveDate>,
    pub vencimento: NaiveDate,
    pub status: String,
    /// Calculado com `estado.hoje()` (ver [`Nota::com_atraso`]).
    #[sqlx(default)]
    pub dias_atraso: i64,
    pub valor_pago_centavos: i64,
    pub data_liquidacao: Option<NaiveDate>,
    pub data_pagamento: Option<NaiveDate>,
    pub documento_liquidacao: Option<String>,
    pub documento_pagamento: Option<String>,
    pub criado_em: DateTime<Utc>,
}

impl Nota {
    /// `true` enquanto emitida ou liquidada (conta para saldo aberto e atraso).
    pub fn aberta(&self) -> bool {
        matches!(self.status.as_str(), "emitida" | "liquidada")
    }

    /// Preenche `dias_atraso` = max(0, hoje − vencimento) se aberta; 0 caso contrário.
    pub fn com_atraso(self, hoje: NaiveDate) -> Self {
        let dias_atraso = if self.aberta() {
            prazos::dias_atraso(self.vencimento, hoje)
        } else {
            0
        };
        Self {
            dias_atraso,
            ..self
        }
    }
}

/// `Evento` do contrato.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Evento {
    pub id: Uuid,
    pub tipo: String,
    pub data: NaiveDate,
    pub valor_centavos: Option<i64>,
    pub documento: Option<String>,
    pub observacao: Option<String>,
    pub origem: String,
    pub criado_por_nome: Option<String>,
    pub criado_em: DateTime<Utc>,
}

/// `Nota` com eventos e cobranças (detalhe).
#[derive(Debug, Serialize)]
pub struct NotaDetalhe {
    #[serde(flatten)]
    pub nota: Nota,
    pub eventos: Vec<Evento>,
    pub cobrancas: Vec<crate::modulos::cobrancas::tipos::CobrancaResumo>,
}

#[derive(Debug, Deserialize)]
pub struct NovaNota {
    pub numero: String,
    pub tipo: String,
    pub valor_centavos: i64,
    pub data_emissao: NaiveDate,
    #[serde(default)]
    pub data_recebimento: Option<NaiveDate>,
}

#[derive(Debug, Default, Deserialize)]
pub struct EdicaoNota {
    #[serde(default)]
    pub numero: Option<String>,
    #[serde(default)]
    pub valor_centavos: Option<i64>,
    #[serde(default)]
    pub data_emissao: Option<NaiveDate>,
    #[serde(default, deserialize_with = "anulavel")]
    pub data_recebimento: Option<Option<NaiveDate>>,
}

#[derive(Debug, Deserialize)]
pub struct NovoEvento {
    pub tipo: String,
    pub data: NaiveDate,
    #[serde(default)]
    pub valor_centavos: Option<i64>,
    #[serde(default)]
    pub documento: Option<String>,
    #[serde(default)]
    pub observacao: Option<String>,
}

/// Query de `GET /orgs/{org}/notas`.
#[derive(Debug, Deserialize)]
pub struct FiltroNotas {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub empenho_id: Option<String>,
    #[serde(default)]
    pub situacao: Option<String>,
    #[serde(default)]
    pub busca: Option<String>,
    #[serde(default)]
    pub ordenar: Option<String>,
    #[serde(flatten)]
    pub paginacao: crate::http::Paginacao,
}
