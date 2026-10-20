//! Vencimento e juros pro-rata die; taxas dependem do contrato.

use crate::dinheiro::MAX_CENTAVOS;
use chrono::{Days, NaiveDate};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Calculo {
    pub vencimento: NaiveDate,
    pub dias_atraso: i64,
    pub principal_centavos: i64,
    pub multa_centavos: i64,
    pub juros_centavos: i64,
    pub total_centavos: i64,
}

pub fn calcular(
    emissao: NaiveDate,
    prazo: u32,
    principal: i64,
    hoje: NaiveDate,
    multa_bps: u32,
    juros_bps: u32,
) -> Result<Calculo, String> {
    if !(1..=3650).contains(&prazo)
        || !(1..=MAX_CENTAVOS).contains(&principal)
        || multa_bps > 100_000
        || juros_bps > 100_000
    {
        return Err("Prazo, principal ou taxas fora dos limites.".into());
    }
    let vencimento = emissao
        .checked_add_days(Days::new(u64::from(prazo)))
        .ok_or("Vencimento inválido.")?;
    let dias_atraso = (hoje - vencimento).num_days().max(0);
    let arredondar = |numerador: i128, denominador: i128| -> Result<i64, String> {
        i64::try_from((numerador + denominador / 2) / denominador)
            .map_err(|_| "Encargos excedem os limites.".into())
    };
    let multa_centavos = if dias_atraso > 0 {
        arredondar(i128::from(principal) * i128::from(multa_bps), 10_000)?
    } else {
        0
    };
    let juros_centavos = arredondar(
        i128::from(principal) * i128::from(juros_bps) * i128::from(dias_atraso),
        300_000,
    )?;
    let total_centavos = principal
        .checked_add(multa_centavos)
        .and_then(|v| v.checked_add(juros_centavos))
        .ok_or("Total excede os limites.")?;
    Ok(Calculo {
        vencimento,
        dias_atraso,
        principal_centavos: principal,
        multa_centavos,
        juros_centavos,
        total_centavos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vencimento_e_juros_em_centavos() {
        let emissao = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let hoje = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        let c = calcular(emissao, 30, 10_000, hoje, 200, 100).unwrap();
        assert_eq!(c.vencimento.to_string(), "2026-10-01");
        assert_eq!(
            (
                c.dias_atraso,
                c.multa_centavos,
                c.juros_centavos,
                c.total_centavos
            ),
            (15, 200, 50, 10_250)
        );
        let c = calcular(emissao, 30, 10_000, emissao, 200, 100).unwrap();
        assert_eq!((c.dias_atraso, c.total_centavos), (0, 10_000));
    }

    #[test]
    fn rejeita_valores_e_datas_fora_dos_limites() {
        let hoje = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert!(calcular(hoje, 0, 10_000, hoje, 0, 0).is_err());
        assert!(calcular(hoje, 30, -1, hoje, 0, 0).is_err());
    }
}
