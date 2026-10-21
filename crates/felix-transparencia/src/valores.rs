//! Leitura tolerante de textos, datas e valores monetários vindos da API.
//!
//! Valores em texto são convertidos para centavos sem ponto flutuante.

use chrono::NaiveDate;
use serde_json::Value;

/// Texto aparado; ausente/nulo vira vazio; números e booleanos viram texto.
pub(crate) fn texto(valor: Option<&Value>) -> String {
    match valor {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// Aceita `dd/mm/aaaa` e `aaaa-mm-dd` (também `aaaa-mm-ddTHH:MM:SS…`).
pub(crate) fn ler_data(texto: &str) -> Option<NaiveDate> {
    let texto = texto.trim();
    if let Ok(data) = NaiveDate::parse_from_str(texto, "%d/%m/%Y") {
        return Some(data);
    }
    let prefixo = texto.get(..10)?;
    let resto = texto.get(10..)?;
    if !(resto.is_empty() || resto.starts_with('T') || resto.starts_with(' ')) {
        return None;
    }
    NaiveDate::parse_from_str(prefixo, "%Y-%m-%d").ok()
}

/// Valor monetário em centavos a partir de texto ou número JSON.
///
/// - Texto: ver [`centavos_de_texto`].
/// - Número inteiro: reais inteiros (`8514` → 851.400 centavos).
/// - Número fracionário: usa a representação decimal mais curta do `f64`
///   (a mesma que o `serde_json` imprime, ex.: `0.29`) e a converte como texto
///   com ponto decimal, arredondando a terceira casa (metade para longe do zero).
///   Assim `0.29` vira 29 centavos, e não 28 como em `(0.29 * 100.0) as i64`.
///   Representações em notação científica (valores enormes ou ínfimos) viram `None`.
pub(crate) fn ler_valor(valor: Option<&Value>) -> Option<i64> {
    match valor? {
        Value::String(s) => centavos_de_texto(s),
        Value::Number(n) => {
            if let Some(inteiro) = n.as_i64() {
                return inteiro.checked_mul(100);
            }
            let decimal = n.to_string();
            if decimal.contains(['e', 'E']) {
                return None;
            }
            let (negativo, decimal) = sinal(&decimal);
            let (inteiro, fracao) = decimal.split_once('.').unwrap_or((decimal, ""));
            montar(negativo, inteiro, fracao)
        }
        _ => None,
    }
}

/// Converte "1.234,56", "R$ 1.234,56", "1234,56", "1234.56", "1234" e "-10,5".
///
/// Regras: com vírgula, ela é a separadora decimal e pontos são milhar. Sem
/// vírgula, vários pontos são milhar; um único ponto seguido de exatamente três
/// dígitos (com até três dígitos antes) é milhar (`1.234` = mil duzentos e trinta
/// e quatro reais); qualquer outro ponto único é decimal. Mais de duas casas
/// decimais são arredondadas pela terceira (metade para longe do zero).
pub(crate) fn centavos_de_texto(texto: &str) -> Option<i64> {
    let texto = texto.trim();
    let texto = texto.strip_prefix("R$").unwrap_or(texto).trim();
    let (negativo, texto) = sinal(texto);
    if texto.contains(',') {
        let (inteiro, fracao) = texto.split_once(',')?;
        if fracao.contains(',') {
            return None;
        }
        let inteiro = sem_milhar(inteiro)?;
        return montar(negativo, &inteiro, fracao);
    }
    match texto.matches('.').count() {
        0 => montar(negativo, texto, ""),
        1 => {
            let (antes, depois) = texto.split_once('.')?;
            if depois.len() == 3 && (1..=3).contains(&antes.len()) {
                montar(negativo, &format!("{antes}{depois}"), "")
            } else {
                montar(negativo, antes, depois)
            }
        }
        _ => montar(negativo, &sem_milhar(texto)?, ""),
    }
}

fn sinal(texto: &str) -> (bool, &str) {
    match texto.strip_prefix('-') {
        Some(resto) => (true, resto.trim_start()),
        None => (false, texto),
    }
}

/// Remove separadores de milhar validando os grupos ("1.234.567" → "1234567").
fn sem_milhar(inteiro: &str) -> Option<String> {
    if !inteiro.contains('.') {
        return Some(inteiro.to_owned());
    }
    let mut grupos = inteiro.split('.');
    let primeiro = grupos.next()?;
    if primeiro.is_empty() || primeiro.len() > 3 {
        return None;
    }
    let mut resultado = primeiro.to_owned();
    for grupo in grupos {
        if grupo.len() != 3 {
            return None;
        }
        resultado.push_str(grupo);
    }
    Some(resultado)
}

fn so_digitos(texto: &str) -> bool {
    texto.bytes().all(|b| b.is_ascii_digit())
}

fn montar(negativo: bool, inteiro: &str, fracao: &str) -> Option<i64> {
    if (inteiro.is_empty() && fracao.is_empty()) || !so_digitos(inteiro) || !so_digitos(fracao) {
        return None;
    }
    let unidades = if inteiro.is_empty() {
        0
    } else {
        inteiro.parse::<i64>().ok()?
    };
    let mut digitos = fracao.bytes().map(|b| i64::from(b - b'0'));
    let dezenas = digitos.next().unwrap_or(0);
    let unidades_centavo = digitos.next().unwrap_or(0);
    let arredonda = i64::from(digitos.next().unwrap_or(0) >= 5);
    let total = unidades
        .checked_mul(100)?
        .checked_add(dezenas * 10 + unidades_centavo + arredonda)?;
    Some(if negativo { -total } else { total })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn textos_monetarios() {
        for (entrada, esperado) in [
            ("32.450,00", Some(3_245_000)),
            ("1.234,56", Some(123_456)),
            ("R$ 1.234,56", Some(123_456)),
            ("1234,56", Some(123_456)),
            ("1234.56", Some(123_456)),
            ("1234.5", Some(123_450)),
            ("1234", Some(123_400)),
            ("1.234", Some(123_400)),
            ("1.234.567,89", Some(123_456_789)),
            ("1.234.567", Some(123_456_700)),
            ("0,01", Some(1)),
            ("0,005", Some(1)),
            ("0,004", Some(0)),
            ("-10,5", Some(-1_050)),
            ("12.34.56", None),
            ("1,2,3", None),
            ("", None),
            ("abc", None),
            ("1.23,00", None),
            ("99999999999999999999,00", None),
        ] {
            assert_eq!(centavos_de_texto(entrada), esperado, "entrada {entrada:?}");
        }
    }

    #[test]
    fn numeros_json() {
        assert_eq!(ler_valor(Some(&json!(8514))), Some(851_400));
        assert_eq!(ler_valor(Some(&json!(0.29))), Some(29));
        assert_eq!(ler_valor(Some(&json!(1.005))), Some(101));
        assert_eq!(ler_valor(Some(&json!(1.234))), Some(123));
        assert_eq!(ler_valor(Some(&json!(-2.5))), Some(-250));
        assert_eq!(ler_valor(Some(&json!(1e300))), None);
        assert_eq!(ler_valor(Some(&json!(null))), None);
        assert_eq!(ler_valor(None), None);
        assert_eq!(ler_valor(Some(&json!("32.450,00"))), Some(3_245_000));
    }

    #[test]
    fn datas() {
        let esperado = NaiveDate::from_ymd_opt(2026, 1, 15);
        assert_eq!(ler_data("15/01/2026"), esperado);
        assert_eq!(ler_data("2026-01-15"), esperado);
        assert_eq!(ler_data("2026-01-15T00:00:00"), esperado);
        assert_eq!(ler_data(""), None);
        assert_eq!(ler_data("31/02/2026"), None);
        assert_eq!(ler_data("2026-01-15xyz"), None);
        assert_eq!(ler_data("Ação"), None);
    }

    #[test]
    fn textos_tolerantes() {
        assert_eq!(texto(Some(&json!("  abc "))), "abc");
        assert_eq!(texto(Some(&json!(160203))), "160203");
        assert_eq!(texto(Some(&json!(null))), "");
        assert_eq!(texto(None), "");
    }
}
