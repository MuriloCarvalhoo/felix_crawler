//! Tipos de entrada e saída dos empenhos (03-api-v1.md, seção Empenhos).

use super::comum::anulavel;
use crate::{modulos::notas::tipos::Nota, pdf::ResultadoAnalise};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EmpenhoResumo {
    pub id: Uuid,
    pub empresa_id: Uuid,
    pub empresa_cnpj: String,
    pub ug: String,
    pub ug_nome: Option<String>,
    pub orgao: Option<String>,
    pub numero: String,
    pub modalidade: Option<String>,
    pub valor_total_centavos: i64,
    pub faturado_centavos: i64,
    pub recebido_centavos: i64,
    pub saldo_centavos: i64,
    pub status: String,
    pub origem: String,
    pub data_emissao: Option<NaiveDate>,
    pub criado_em: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Empenho {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub resumo: EmpenhoResumo,
    pub gestao: Option<String>,
    pub numero_edital_ata: Option<String>,
    pub regra_vencimento: String,
    pub prazo_dias: i32,
    pub observacao: Option<String>,
    pub anulado_em: Option<DateTime<Utc>>,
    pub motivo_anulacao: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notas: Option<Vec<Nota>>,
}

#[derive(Debug, Deserialize)]
pub struct NovoEmpenho {
    pub empresa_id: Uuid,
    pub ug: String,
    #[serde(default)]
    pub ug_nome: Option<String>,
    #[serde(default)]
    pub orgao: Option<String>,
    #[serde(default)]
    pub gestao: Option<String>,
    pub numero: String,
    #[serde(default)]
    pub modalidade: Option<String>,
    #[serde(default)]
    pub numero_edital_ata: Option<String>,
    pub valor_total_centavos: i64,
    #[serde(default)]
    pub data_emissao: Option<NaiveDate>,
    #[serde(default)]
    pub regra_vencimento: Option<String>,
    #[serde(default)]
    pub prazo_dias: Option<i64>,
    #[serde(default)]
    pub origem: Option<String>,
    #[serde(default)]
    pub observacao: Option<String>,
}

/// PATCH: todos opcionais; campos anuláveis aceitam `null` para limpar.
#[derive(Debug, Default, Deserialize)]
pub struct EdicaoEmpenho {
    #[serde(default)]
    pub ug: Option<String>,
    #[serde(default, deserialize_with = "anulavel")]
    pub ug_nome: Option<Option<String>>,
    #[serde(default, deserialize_with = "anulavel")]
    pub orgao: Option<Option<String>>,
    #[serde(default, deserialize_with = "anulavel")]
    pub gestao: Option<Option<String>>,
    #[serde(default)]
    pub numero: Option<String>,
    #[serde(default, deserialize_with = "anulavel")]
    pub modalidade: Option<Option<String>>,
    #[serde(default, deserialize_with = "anulavel")]
    pub numero_edital_ata: Option<Option<String>>,
    #[serde(default)]
    pub valor_total_centavos: Option<i64>,
    #[serde(default, deserialize_with = "anulavel")]
    pub data_emissao: Option<Option<NaiveDate>>,
    #[serde(default)]
    pub regra_vencimento: Option<String>,
    #[serde(default)]
    pub prazo_dias: Option<i64>,
    #[serde(default, deserialize_with = "anulavel")]
    pub observacao: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct Anulacao {
    #[serde(default)]
    pub motivo: String,
}

/// Query de `GET /orgs/{org}/empenhos`.
#[derive(Debug, Deserialize)]
pub struct FiltroEmpenhos {
    #[serde(default)]
    pub busca: Option<String>,
    #[serde(default)]
    pub empresa_id: Option<String>,
    #[serde(default)]
    pub ug: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub com_saldo: Option<String>,
    #[serde(flatten)]
    pub paginacao: crate::http::Paginacao,
}

/// Item da resposta de `/empenhos/analisar`.
#[derive(Debug, Serialize)]
pub struct ItemAnalise {
    #[serde(flatten)]
    pub resultado: ResultadoAnalise,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empresa_id_sugerida: Option<Uuid>,
    pub ja_cadastrado: bool,
}
