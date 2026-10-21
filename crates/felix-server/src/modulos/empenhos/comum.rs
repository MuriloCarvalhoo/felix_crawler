//! Utilitários compartilhados pelos módulos de empenhos, notas, cobranças,
//! painel e exportação (validação de texto, datas, CNPJ, LIKE e downloads).

use crate::http::ErroApi;
use axum::{
    body::Body,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;

/// Campo anulável em PATCH: ausente → `None`; `null` → `Some(None)`; valor → `Some(Some(v))`.
pub(crate) fn anulavel<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Acumulador de erros por campo (422 com `campos`).
#[derive(Default)]
pub(crate) struct Erros(BTreeMap<String, String>);

impl Erros {
    pub(crate) fn add(&mut self, campo: &str, mensagem: impl Into<String>) {
        self.0
            .entry(campo.to_owned())
            .or_insert_with(|| mensagem.into());
    }

    /// Mantém só os erros dos `campos` indicados (PATCH valida o que foi enviado).
    pub(crate) fn reter(&mut self, campos: &[&str]) {
        self.0.retain(|campo, _| campos.contains(&campo.as_str()));
    }

    pub(crate) fn resultado(self) -> Result<(), ErroApi> {
        match ErroApi::validacao_campos(self.0) {
            Some(erro) => Err(erro),
            None => Ok(()),
        }
    }
}

/// Texto obrigatório: aparado, sem caracteres de controle, 1..=max caracteres.
pub(crate) fn texto_obrigatorio(erros: &mut Erros, campo: &str, valor: &str, max: usize) -> String {
    let limpo = limpar(valor);
    if limpo.is_empty() {
        erros.add(campo, "Campo obrigatório.");
    } else if limpo.chars().count() > max {
        erros.add(campo, format!("Use no máximo {max} caracteres."));
    }
    limpo
}

/// Texto opcional: vazio vira `None`; acima de `max` caracteres → erro.
pub(crate) fn texto_opcional(
    erros: &mut Erros,
    campo: &str,
    valor: Option<&str>,
    max: usize,
) -> Option<String> {
    let limpo = limpar(valor?);
    if limpo.is_empty() {
        return None;
    }
    if limpo.chars().count() > max {
        erros.add(campo, format!("Use no máximo {max} caracteres."));
    }
    Some(limpo)
}

/// Texto opcional para dados externos (Portal, legado): limpo e cortado em
/// `max` caracteres em vez de recusado. Vazio vira `None`.
pub(crate) fn texto_cortado(valor: Option<&str>, max: usize) -> Option<String> {
    let limpo = limpar(valor?);
    if limpo.is_empty() {
        return None;
    }
    Some(limpo.chars().take(max).collect())
}

fn limpar(valor: &str) -> String {
    valor
        .trim()
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

/// Escapa `%`, `_` e `\` e envolve em `%...%` para `ILIKE`.
pub(crate) fn padrao_busca(busca: Option<&str>) -> Option<String> {
    let busca = busca?.trim();
    if busca.is_empty() {
        return None;
    }
    let mut padrao = String::with_capacity(busca.len() + 2);
    padrao.push('%');
    for c in busca.chars().take(100) {
        if matches!(c, '%' | '_' | '\\') {
            padrao.push('\\');
        }
        padrao.push(c);
    }
    padrao.push('%');
    Some(padrao)
}

/// `true` se o erro do banco é violação de unicidade.
pub(crate) fn violacao_unica(erro: &sqlx::Error) -> bool {
    matches!(erro, sqlx::Error::Database(db) if db.is_unique_violation())
}

/// `dd/mm/aaaa`.
pub(crate) fn data_br(data: NaiveDate) -> String {
    data.format("%d/%m/%Y").to_string()
}

const MESES: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

/// "20 de outubro de 2026".
pub(crate) fn data_extenso(data: NaiveDate) -> String {
    let mes = usize::try_from(data.month0())
        .ok()
        .and_then(|i| MESES.get(i))
        .copied()
        .unwrap_or("");
    format!("{} de {mes} de {}", data.day(), data.year())
}

/// `19.116.488/0001-45` (devolve o texto original se não tiver 14 dígitos).
pub(crate) fn formatar_cnpj(cnpj: &str) -> String {
    let d: Vec<char> = cnpj.chars().filter(char::is_ascii_digit).collect();
    if d.len() != 14 {
        return cnpj.to_owned();
    }
    let parte = |a: usize, b: usize| d.get(a..b).map(|s| s.iter().collect::<String>());
    match (
        parte(0, 2),
        parte(2, 5),
        parte(5, 8),
        parte(8, 12),
        parte(12, 14),
    ) {
        (Some(a), Some(b), Some(c), Some(e), Some(f)) => format!("{a}.{b}.{c}/{e}-{f}"),
        _ => cnpj.to_owned(),
    }
}

/// Nome de arquivo seguro: só ASCII alfanumérico, `-` e `_` (demais viram `_`), até 60.
pub(crate) fn nome_arquivo_seguro(texto: &str) -> String {
    let nome: String = texto
        .chars()
        .take(60)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if nome.is_empty() {
        "sem_numero".into()
    } else {
        nome
    }
}

/// Resposta de download (`Content-Disposition: attachment`).
pub(crate) fn anexo(
    conteudo: Vec<u8>,
    content_type: &'static str,
    nome_arquivo: &str,
) -> Result<Response, ErroApi> {
    let disposicao = HeaderValue::from_str(&format!("attachment; filename=\"{nome_arquivo}\""))
        .map_err(|e| ErroApi::interno_log("cabeçalho de download", e))?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (header::CONTENT_DISPOSITION, disposicao),
        ],
        Body::from(conteudo),
    )
        .into_response())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod testes {
    use super::*;

    #[test]
    fn busca_escapa_curingas() {
        assert_eq!(padrao_busca(Some(" 50%_x ")).unwrap(), "%50\\%\\_x%");
        assert!(padrao_busca(Some("  ")).is_none());
        assert!(padrao_busca(None).is_none());
    }

    #[test]
    fn datas_e_cnpj() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        assert_eq!(data_br(d), "05/03/2026");
        assert_eq!(data_extenso(d), "5 de março de 2026");
        assert_eq!(formatar_cnpj("19116488000145"), "19.116.488/0001-45");
        assert_eq!(formatar_cnpj("123"), "123");
    }

    #[test]
    fn nome_arquivo() {
        assert_eq!(nome_arquivo_seguro("12/3 \"x\";"), "12_3__x__");
        assert_eq!(nome_arquivo_seguro(""), "sem_numero");
    }

    #[test]
    fn textos() {
        let mut e = Erros::default();
        assert_eq!(texto_obrigatorio(&mut e, "a", "  x\u{7}y ", 10), "x y");
        assert!(texto_opcional(&mut e, "b", Some("  "), 10).is_none());
        assert!(e.resultado().is_ok());
        let mut e = Erros::default();
        texto_obrigatorio(&mut e, "a", "", 10);
        texto_opcional(&mut e, "b", Some("abcdef"), 3);
        let erro = e.resultado().unwrap_err();
        assert_eq!(erro.campos().len(), 2);
    }
}
