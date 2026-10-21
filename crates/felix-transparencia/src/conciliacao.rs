//! Funções puras de conciliação. Elas apenas sugerem vínculos; a confirmação é humana.

use crate::documento::Documento;

/// Extrai o número do empenho no formato `AAAANE000000` de `2026NE000290`,
/// `2026NE290`, `2026 NE 290` ou do código completo `160203000012026NE000290`.
/// O número é normalizado para 6 dígitos com zeros à esquerda.
pub fn numero_empenho(codigo_resumido_ou_codigo: &str) -> Option<String> {
    let compacto: Vec<u8> = codigo_resumido_ou_codigo
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .map(|b| b.to_ascii_uppercase())
        .collect();
    let mut inicio = 0;
    while let Some(relativo) = compacto
        .get(inicio..)?
        .windows(2)
        .position(|par| par == b"NE")
    {
        let pos = inicio + relativo;
        if let Some(numero) = empenho_em(&compacto, pos) {
            return Some(numero);
        }
        inicio = pos + 1;
    }
    None
}

/// Tenta ler `AAAA` + `NE` + 1..=6 dígitos com o `NE` na posição `pos`.
fn empenho_em(compacto: &[u8], pos: usize) -> Option<String> {
    let ano = compacto.get(pos.checked_sub(4)?..pos)?;
    if !ano.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let numero: Vec<u8> = compacto
        .get(pos + 2..)?
        .iter()
        .copied()
        .take_while(u8::is_ascii_digit)
        .collect();
    if !(1..=6).contains(&numero.len()) {
        return None;
    }
    let ano = std::str::from_utf8(ano).ok()?;
    let numero = std::str::from_utf8(&numero).ok()?;
    Some(format!("{ano}NE{numero:0>6}"))
}

/// Empenho já cadastrado, para conciliação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmpenhoRef<T> {
    pub id: T,
    pub ug: String,
    pub numero: String,
}

/// Nota fiscal cadastrada, para conciliação de liquidações e pagamentos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotaRef<T> {
    pub id: T,
    pub empenho_numero: String,
    pub ug: String,
    pub valor_centavos: i64,
    pub status_aberta: bool,
}

/// Empenho com a mesma UG e o mesmo número normalizado. Se nenhum ou mais de
/// um corresponder (ambíguo), retorna `None`.
pub fn sugerir_empenho<'a, T>(
    ug: &str,
    numero_empenho_documento: &str,
    empenhos: &'a [EmpenhoRef<T>],
) -> Option<&'a EmpenhoRef<T>> {
    let alvo = numero_empenho(numero_empenho_documento)?;
    let ug = ug.trim();
    if ug.is_empty() {
        return None;
    }
    unico(empenhos.iter().filter(|empenho| {
        empenho.ug.trim() == ug && numero_empenho(&empenho.numero).as_deref() == Some(&alvo)
    }))
}

/// Nota aberta do mesmo empenho e da mesma UG do documento (liquidação/pagamento).
///
/// 1. Se exatamente uma candidata tem valor igual ao do documento, ela é sugerida;
///    se mais de uma tem o mesmo valor, retorna `None` (decisão humana).
/// 2. Sem valor igual (ex.: pagamento líquido de retenções) ou sem valor no
///    documento, sugere a candidata somente se ela for a única nota aberta do empenho.
pub fn sugerir_nota<'a, T>(
    doc: &Documento,
    numero_empenho_documento: &str,
    notas: &'a [NotaRef<T>],
) -> Option<&'a NotaRef<T>> {
    let alvo = numero_empenho(numero_empenho_documento)?;
    let ug = doc.ug_codigo.trim();
    if ug.is_empty() {
        return None;
    }
    let candidatas: Vec<&NotaRef<T>> = notas
        .iter()
        .filter(|nota| {
            nota.status_aberta
                && nota.ug.trim() == ug
                && numero_empenho(&nota.empenho_numero).as_deref() == Some(&alvo)
        })
        .collect();
    if let Some(valor) = doc.valor_centavos {
        let mut iguais = candidatas
            .iter()
            .copied()
            .filter(|nota| nota.valor_centavos == valor);
        if let Some(primeira) = iguais.next() {
            return if iguais.next().is_some() {
                None
            } else {
                Some(primeira)
            };
        }
    }
    unico(candidatas.into_iter())
}

fn unico<I: Iterator>(mut itens: I) -> Option<I::Item> {
    let primeiro = itens.next()?;
    if itens.next().is_some() {
        None
    } else {
        Some(primeiro)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fase::Fase;
    use serde_json::json;

    #[test]
    fn extrai_numero_do_empenho() {
        for (entrada, esperado) in [
            ("2026NE000290", Some("2026NE000290")),
            ("160203000012026NE000290", Some("2026NE000290")),
            ("2026NE290", Some("2026NE000290")),
            ("2026 NE 290", Some("2026NE000290")),
            ("2026ne000290", Some("2026NE000290")),
            ("Empenho 2026NE000534 ref", Some("2026NE000534")),
            ("NENE2026NE1", Some("2026NE000001")),
            ("2026NS000123", None),
            ("2026OB000456", None),
            ("2026NE0002901", None),
            ("26NE000290", None),
            ("2026NE", None),
            ("", None),
        ] {
            assert_eq!(numero_empenho(entrada).as_deref(), esperado, "{entrada:?}");
        }
    }

    fn empenho(id: u32, ug: &str, numero: &str) -> EmpenhoRef<u32> {
        EmpenhoRef {
            id,
            ug: ug.to_owned(),
            numero: numero.to_owned(),
        }
    }

    #[test]
    fn sugere_empenho_por_ug_e_numero() {
        let empenhos = [
            empenho(1, "160203", "2026NE000291"),
            empenho(2, "160203", "2026NE290"),
            empenho(3, "999999", "2026NE000290"),
        ];
        let sugerido = sugerir_empenho("160203", "160203000012026NE000290", &empenhos);
        assert_eq!(sugerido.map(|e| e.id), Some(2));
        assert!(sugerir_empenho("160204", "2026NE000290", &empenhos).is_none());
        assert!(sugerir_empenho("", "2026NE000290", &empenhos).is_none());
        assert!(sugerir_empenho("160203", "2026NS000123", &empenhos).is_none());
        let duplicados = [
            empenho(1, "160203", "2026NE000290"),
            empenho(2, "160203", "2026NE000290"),
        ];
        assert!(sugerir_empenho("160203", "2026NE000290", &duplicados).is_none());
    }

    fn nota(id: u32, empenho: &str, valor: i64, aberta: bool) -> NotaRef<u32> {
        NotaRef {
            id,
            empenho_numero: empenho.to_owned(),
            ug: "160203".to_owned(),
            valor_centavos: valor,
            status_aberta: aberta,
        }
    }

    fn liquidacao(valor: Option<&str>) -> Documento {
        let mut item = json!({"documentoResumido": "2026NS000123", "codigoUg": "160203"});
        if let Some(valor) = valor {
            item.as_object_mut()
                .unwrap()
                .insert("valor".to_owned(), json!(valor));
        }
        Documento::de_json(item, Fase::Liquidacao).unwrap()
    }

    #[test]
    fn sugere_nota_com_valor_igual() {
        let notas = [
            nota(1, "2026NE000290", 1_000_000, true),
            nota(2, "2026NE000290", 3_245_000, true),
            nota(3, "2026NE000291", 3_245_000, true),
            nota(4, "2026NE000290", 3_245_000, false),
        ];
        let doc = liquidacao(Some("32.450,00"));
        assert_eq!(
            sugerir_nota(&doc, "2026NE000290", &notas).map(|n| n.id),
            Some(2)
        );
    }

    #[test]
    fn valor_igual_ambiguo_nao_sugere() {
        let notas = [
            nota(1, "2026NE000290", 3_245_000, true),
            nota(2, "2026NE000290", 3_245_000, true),
        ];
        assert!(sugerir_nota(&liquidacao(Some("32.450,00")), "2026NE000290", &notas).is_none());
    }

    #[test]
    fn sem_valor_igual_sugere_apenas_nota_unica() {
        let unica = [nota(1, "2026NE000290", 3_245_000, true)];
        let pagamento_liquido = liquidacao(Some("29.383,48"));
        assert_eq!(
            sugerir_nota(&pagamento_liquido, "2026NE000290", &unica).map(|n| n.id),
            Some(1)
        );
        assert_eq!(
            sugerir_nota(&liquidacao(None), "2026NE000290", &unica).map(|n| n.id),
            Some(1)
        );
        let duas = [
            nota(1, "2026NE000290", 1_000_000, true),
            nota(2, "2026NE000290", 2_000_000, true),
        ];
        assert!(sugerir_nota(&pagamento_liquido, "2026NE000290", &duas).is_none());
    }

    #[test]
    fn nota_de_outra_ug_ou_fechada_nao_e_sugerida() {
        let mut outra_ug = nota(1, "2026NE000290", 3_245_000, true);
        outra_ug.ug = "160204".to_owned();
        let notas = [outra_ug, nota(2, "2026NE000290", 3_245_000, false)];
        assert!(sugerir_nota(&liquidacao(Some("32.450,00")), "2026NE000290", &notas).is_none());
        assert!(sugerir_nota(&liquidacao(Some("32.450,00")), "inválido", &notas).is_none());
    }
}
