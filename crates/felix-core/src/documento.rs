//! Parser orientado às seções do relatório SIAFI.

use crate::dinheiro::ler_reais;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DadosEmpenho {
    pub ug: Option<String>,
    pub gestao: Option<String>,
    pub empenho: Option<String>,
    pub cnpj_fornecedor: Option<String>,
    pub cnpj_emitente: Option<String>,
    pub fornecedor: Option<String>,
    pub valor_total_centavos: Option<i64>,
    pub data_emissao: Option<String>,
    pub modalidade: Option<String>,
    pub origem: String,
    pub numero_edital_ata: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Analise {
    pub perfil: String,
    pub dados: DadosEmpenho,
    pub avisos: Vec<String>,
}

pub struct Parser {
    ug: Regex,
    cnpj: Regex,
    numero: Regex,
    emissao: Regex,
    gestao: Regex,
    modalidade: Regex,
    edital: Regex,
    complementar: Regex,
    adesao: Regex,
}

fn secao<'a>(texto: &'a str, inicio: &str, fim: &str) -> Option<&'a str> {
    let trecho = texto.split_once(inicio)?.1;
    Some(trecho.split_once(fim).map_or(trecho, |(antes, _)| antes))
}

pub fn normalizar_cnpj(texto: &str) -> Option<String> {
    let mut resultado = String::with_capacity(14);
    for c in texto.chars() {
        if c.is_ascii_digit() {
            resultado.push(c);
        } else if !matches!(c, '.' | '/' | '-' | ' ') {
            return None;
        }
    }
    (resultado.len() == 14).then_some(resultado)
}

impl Parser {
    pub fn novo() -> Result<Self, regex::Error> {
        Ok(Self {
            ug: Regex::new(r"(?m)^\s*(\d{6})\s+\S")?,
            cnpj: Regex::new(r"(?m)^\s*(\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2})\s+([^\n]*)")?,
            numero: Regex::new(r"(?m)^\s*(\d{4})\s+NE\s+(\d{1,6})\s*$")?,
            emissao: Regex::new(r"(?m)^\s*(\d{2}/\d{2}/\d{4})\s+[^\n]*?\s+([\d.]+,\d{2})\s*$")?,
            gestao: Regex::new(r"(?im)^\s*Gest[ãa]o\s*:?\s*(\d{5})\s*$")?,
            modalidade: Regex::new(
                r"(?im)^\s*\d{1,4}\s+(PREG[ÃA]O|DISPENSA[^\n]*|INEXIGIBILIDADE[^\n]*|CONCORR[ÊE]NCIA[^\n]*)",
            )?,
            edital: Regex::new(
                r"(?i)(?:PREG[ÃA]O(?:\s+SRP)?|ATA)\s*(?:N[º°O.]*)?\s*(\d{1,6})\s*/\s*(\d{4})",
            )?,
            complementar: Regex::new(r"(?m)^\s*\d{8}(\d{5})(\d{4})\s*-\s*UASG")?,
            adesao: Regex::new(r"(?i)(?:ADES[ÃA]O\s+[ÀA]\s+ATA|CARONA|N[ÃA]O\s+PARTICIPANTE)")?,
        })
    }

    pub fn analisar(&self, texto: &str) -> Analise {
        let primeira = texto.split('\u{c}').next().unwrap_or(texto);
        let mut dados = DadosEmpenho {
            origem: "nao_identificada".into(),
            ..Default::default()
        };
        let conhecido = primeira.contains("Nota de Empenho") && primeira.contains("UG Emitente");
        if let Some(emitente) = secao(primeira, "UG Emitente", "Ano") {
            dados.ug = self
                .ug
                .captures(emitente)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_owned());
            dados.cnpj_emitente = self
                .cnpj
                .captures(emitente)
                .and_then(|c| c.get(1))
                .and_then(|m| normalizar_cnpj(m.as_str()));
        }
        if let Some(c) =
            secao(primeira, "Favorecido", "Amparo Legal").and_then(|s| self.cnpj.captures(s))
        {
            dados.cnpj_fornecedor = c.get(1).and_then(|m| normalizar_cnpj(m.as_str()));
            dados.fornecedor = c.get(2).map(|m| m.as_str().trim().to_owned());
        }
        if let Some(c) = self.numero.captures(primeira)
            && let (Some(ano), Some(numero)) = (c.get(1), c.get(2))
        {
            dados.empenho = Some(format!("{}NE{:0>6}", ano.as_str(), numero.as_str()));
        }
        if let Some(c) =
            secao(primeira, "Data de Emissão", "Favorecido").and_then(|s| self.emissao.captures(s))
        {
            dados.data_emissao = c
                .get(1)
                .and_then(|m| chrono::NaiveDate::parse_from_str(m.as_str(), "%d/%m/%Y").ok())
                .map(|d| d.to_string());
            dados.valor_total_centavos = c
                .get(2)
                .and_then(|m| ler_reais(m.as_str()).ok())
                .filter(|v| *v > 0);
        }
        dados.gestao = self
            .gestao
            .captures(primeira)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_owned());
        let amparo = secao(primeira, "Amparo Legal", "Descrição").unwrap_or("");
        if let Some(m) = self.modalidade.captures(amparo).and_then(|c| c.get(1)) {
            let valor = m.as_str().to_uppercase();
            dados.modalidade = Some(if valor.starts_with("PREG") {
                "Pregão".into()
            } else {
                m.as_str().trim().to_owned()
            });
            dados.origem = if valor.starts_with("DISPENSA") || valor.starts_with("INEXIGIBILIDADE")
            {
                "contratacao_direta"
            } else {
                "licitacao"
            }
            .into();
        }
        let descricao = secao(primeira, "Descrição", "Local da Entrega").unwrap_or("");
        if self.adesao.is_match(descricao) {
            dados.origem = "adesao_ata".into();
        }
        if let Some(c) = self.edital.captures(descricao)
            && let (Some(numero), Some(ano)) = (c.get(1), c.get(2))
        {
            dados.numero_edital_ata = Some(format!("{}/{}", numero.as_str(), ano.as_str()));
        }
        if dados.numero_edital_ata.is_none()
            && let Some(c) = secao(primeira, "Informação Complementar", "Sistema de Origem")
                .and_then(|s| self.complementar.captures(s))
            && let (Some(numero), Some(ano)) = (c.get(1), c.get(2))
        {
            dados.numero_edital_ata = Some(format!("{}/{}", numero.as_str(), ano.as_str()));
        }
        let mut avisos = Vec::with_capacity(8);
        if !conhecido {
            avisos.push("Layout não reconhecido como SIAFI; confira todos os campos.".into());
        }
        for (nome, ausente) in [
            ("UG", dados.ug.is_none()),
            ("Gestão", dados.gestao.is_none()),
            ("Empenho", dados.empenho.is_none()),
            ("CNPJ do favorecido", dados.cnpj_fornecedor.is_none()),
            ("Valor total", dados.valor_total_centavos.is_none()),
            ("Edital/ata", dados.numero_edital_ata.is_none()),
        ] {
            if ausente {
                avisos.push(format!(
                    "{nome} não informado ou não identificado; confira manualmente."
                ));
            }
        }
        Analise {
            perfil: if conhecido { "siafi" } else { "desconhecido" }.into(),
            dados,
            avisos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nao_inventa_dados_em_documento_desconhecido() {
        let a = Parser::novo()
            .unwrap()
            .analisar("PDF sem campos de empenho");
        assert!(a.dados.ug.is_none());
        assert!(a.dados.valor_total_centavos.is_none());
        assert!(!a.avisos.is_empty());
    }

    #[test]
    fn srp_nao_e_carona_sem_evidencia() {
        let p = Parser::novo().unwrap();
        let a = p.analisar("Nota de Empenho\nAmparo Legal\n229 PREGAO\nDescrição\nPREGAO SRP Nº 90012/2025\nLocal da Entrega");
        assert_eq!(a.dados.origem, "licitacao");
        assert_eq!(a.dados.numero_edital_ata.as_deref(), Some("90012/2025"));
        let a = p.analisar("Nota de Empenho\nAmparo Legal\n229 PREGAO\nDescrição\nADESÃO À ATA Nº 12/2026\nLocal da Entrega");
        assert_eq!(a.dados.origem, "adesao_ata");
    }
}
