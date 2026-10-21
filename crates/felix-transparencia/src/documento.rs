use chrono::NaiveDate;
use serde::Serialize;
use serde_json::Value;

use crate::conciliacao::numero_empenho;
use crate::erro::ErroTransparencia;
use crate::fase::Fase;
use crate::valores::{ler_data, ler_valor, texto};

/// Documento de despesa (empenho, liquidação ou pagamento) de
/// `documentos-por-favorecido` (`DespesasPorDocumentoDTO`).
///
/// Campos ausentes ou nulos viram texto vazio / `None`. O JSON original fica
/// em `bruto` para auditoria.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Documento {
    /// `documento`: UG (6) + gestão (5) + número (12), ex.: `160203000012026NE000290`.
    pub codigo: String,
    /// `documentoResumido`, ex.: `2026NE000290`, `2026NS000123`, `2026OB000456`.
    pub codigo_resumido: String,
    /// Fase do texto `fase` da resposta; se ausente ou desconhecida, a fase consultada.
    pub fase: Fase,
    pub data: Option<NaiveDate>,
    pub valor_centavos: Option<i64>,
    /// `codigoUg`
    pub ug_codigo: String,
    /// `ug`
    pub ug_nome: String,
    pub orgao: String,
    pub especie: String,
    pub observacao: String,
    /// `codigoFavorecido`
    pub favorecido_codigo: String,
    pub numero_processo: String,
    pub bruto: Value,
}

impl Documento {
    /// Lê um item da resposta da API. Falha apenas se o item não for um objeto JSON.
    pub fn de_json(item: Value, fase_consultada: Fase) -> Result<Self, ErroTransparencia> {
        let Value::Object(campos) = &item else {
            return Err(ErroTransparencia::Resposta(
                "item da lista de documentos não é um objeto JSON".to_owned(),
            ));
        };
        let campo = |nome: &str| texto(campos.get(nome));
        let fase = Fase::de_texto(&campo("fase")).unwrap_or(fase_consultada);
        Ok(Documento {
            codigo: campo("documento"),
            codigo_resumido: campo("documentoResumido"),
            fase,
            data: ler_data(&campo("data")),
            valor_centavos: ler_valor(campos.get("valor")),
            ug_codigo: campo("codigoUg"),
            ug_nome: campo("ug"),
            orgao: campo("orgao"),
            especie: campo("especie"),
            observacao: campo("observacao"),
            favorecido_codigo: campo("codigoFavorecido"),
            numero_processo: campo("numeroProcesso"),
            bruto: item,
        })
    }

    /// Número do empenho (`2026NE000290`) se este documento for um empenho.
    pub fn numero_empenho(&self) -> Option<String> {
        numero_empenho(&self.codigo_resumido).or_else(|| numero_empenho(&self.codigo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn le_documento_completo() {
        let item = json!({
            "data": "15/01/2026",
            "documento": "160203000012026NE000290",
            "documentoResumido": "2026NE000290",
            "fase": "Empenho",
            "especie": "Original",
            "codigoFavorecido": "19116488000145",
            "valor": "32.450,00",
            "codigoUg": "160203",
            "ug": "UG TESTE",
            "orgao": "Comando do Exército",
            "observacao": " obs ",
            "numeroProcesso": "64000.000123/2026-11"
        });
        let doc = Documento::de_json(item.clone(), Fase::Pagamento).unwrap();
        assert_eq!(doc.fase, Fase::Empenho);
        assert_eq!(doc.codigo_resumido, "2026NE000290");
        assert_eq!(doc.valor_centavos, Some(3_245_000));
        assert_eq!(doc.data, NaiveDate::from_ymd_opt(2026, 1, 15));
        assert_eq!(doc.observacao, "obs");
        assert_eq!(doc.numero_empenho().as_deref(), Some("2026NE000290"));
        assert_eq!(doc.bruto, item);
    }

    #[test]
    fn objeto_vazio_e_tolerado_e_nao_objeto_falha() {
        let doc = Documento::de_json(json!({}), Fase::Liquidacao).unwrap();
        assert_eq!(doc.fase, Fase::Liquidacao);
        assert_eq!(doc.codigo, "");
        assert_eq!(doc.valor_centavos, None);
        assert_eq!(doc.data, None);
        assert_eq!(doc.numero_empenho(), None);
        assert!(matches!(
            Documento::de_json(json!("x"), Fase::Empenho),
            Err(ErroTransparencia::Resposta(_))
        ));
    }

    #[test]
    fn serializa_fase_como_codigo() {
        let doc = Documento::de_json(json!({"fase": "Pagamento"}), Fase::Empenho).unwrap();
        let json = serde_json::to_value(&doc).unwrap();
        assert_eq!(json.get("fase"), Some(&json!(3)));
    }
}
