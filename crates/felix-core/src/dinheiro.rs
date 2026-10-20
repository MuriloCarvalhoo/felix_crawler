//! Conversões monetárias sem ponto flutuante.

use thiserror::Error;

pub const MAX_CENTAVOS: i64 = 1_000_000_000_000;

#[derive(Debug, Error)]
#[error("valor monetário inválido: use reais com duas casas decimais")]
pub struct DinheiroInvalido;

pub fn ler_reais(valor: &str) -> Result<i64, DinheiroInvalido> {
    let valor = valor
        .trim()
        .strip_prefix("R$")
        .unwrap_or(valor.trim())
        .trim();
    let (inteiro, fracao) = valor.split_once(',').ok_or(DinheiroInvalido)?;
    if fracao.len() != 2 || !fracao.bytes().all(|c| c.is_ascii_digit()) {
        return Err(DinheiroInvalido);
    }
    let mut grupos = inteiro.split('.');
    let primeiro = grupos.next().ok_or(DinheiroInvalido)?;
    if primeiro.is_empty()
        || !primeiro.bytes().all(|c| c.is_ascii_digit())
        || (inteiro.contains('.') && primeiro.len() > 3)
        || grupos.any(|g| g.len() != 3 || !g.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err(DinheiroInvalido);
    }
    let unidades = inteiro
        .replace('.', "")
        .parse::<i64>()
        .map_err(|_| DinheiroInvalido)?;
    let centavos = fracao.parse::<i64>().map_err(|_| DinheiroInvalido)?;
    let valor = unidades
        .checked_mul(100)
        .and_then(|v| v.checked_add(centavos))
        .ok_or(DinheiroInvalido)?;
    if valor > MAX_CENTAVOS {
        return Err(DinheiroInvalido);
    }
    Ok(valor)
}

pub fn reais(centavos: i64) -> String {
    let valor = centavos.unsigned_abs();
    let inteiro = (valor / 100).to_string();
    let mut result = String::with_capacity(inteiro.len() + inteiro.len() / 3 + 4);
    if centavos < 0 {
        result.push('-');
    }
    for (i, c) in inteiro.chars().enumerate() {
        if i > 0 && (inteiro.len() - i).is_multiple_of(3) {
            result.push('.');
        }
        result.push(c);
    }
    result.push_str(&format!(",{:02}", valor % 100));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valores_brasileiros_sao_exatos() {
        for (texto, esperado) in [
            ("32.450,00", 3_245_000),
            ("36.657,30", 3_665_730),
            ("30.056,91", 3_005_691),
            ("0,01", 1),
        ] {
            assert_eq!(ler_reais(texto).unwrap(), esperado);
        }
        assert_eq!(reais(3_245_000), "32.450,00");
    }

    #[test]
    fn valores_ambiguos_ou_invalidos_falham() {
        for texto in [
            "",
            "NaN",
            "-1,00",
            "1.23,45",
            "1,001",
            "1,2",
            "1000000000000000000,00",
        ] {
            assert!(ler_reais(texto).is_err(), "{texto}");
        }
    }
}
