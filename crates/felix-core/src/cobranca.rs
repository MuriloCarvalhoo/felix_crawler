//! Vencimento e juros pro-rata die; taxas dependem do contrato.

use crate::dinheiro::MAX_CENTAVOS;
use crate::prazos::{RegraVencimento, calcular_vencimento, dias_atraso};
use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Calculo {
    pub vencimento: NaiveDate,
    pub dias_atraso: i64,
    pub principal_centavos: i64,
    pub multa_centavos: i64,
    pub juros_centavos: i64,
    pub total_centavos: i64,
}

/// Taxa máxima de multa e de juros mensais, em pontos-base (10 000 = 100%).
/// É o mesmo limite validado pelo servidor nas cobranças.
pub const MAX_BPS: u32 = 10_000;

const FORA_DOS_LIMITES: &str = "Prazo, principal ou taxas fora dos limites.";

/// Vencimento por dias corridos da emissão + encargos. Mantido por
/// compatibilidade; delega a [`calcular_encargos`].
pub fn calcular(
    emissao: NaiveDate,
    prazo: u32,
    principal: i64,
    hoje: NaiveDate,
    multa_bps: u32,
    juros_bps: u32,
) -> Result<Calculo, String> {
    if !(1..=3650).contains(&prazo) {
        return Err(FORA_DOS_LIMITES.into());
    }
    validar_encargos(principal, multa_bps, juros_bps)?;
    let vencimento = calcular_vencimento(
        RegraVencimento::CorridosEmissao,
        emissao,
        None,
        prazo,
        principal,
    )
    .map_err(|_| "Vencimento inválido.")?;
    calcular_encargos(vencimento, principal, hoje, multa_bps, juros_bps)
}

/// Multa (só com atraso) e juros mensais pro-rata die (taxa / 30 por dia),
/// arredondados em centavos, a partir de um vencimento já calculado.
pub fn calcular_encargos(
    vencimento: NaiveDate,
    principal: i64,
    hoje: NaiveDate,
    multa_bps: u32,
    juros_bps: u32,
) -> Result<Calculo, String> {
    validar_encargos(principal, multa_bps, juros_bps)?;
    let dias_atraso = dias_atraso(vencimento, hoje);
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

fn validar_encargos(principal: i64, multa_bps: u32, juros_bps: u32) -> Result<(), String> {
    if !(1..=MAX_CENTAVOS).contains(&principal) || multa_bps > MAX_BPS || juros_bps > MAX_BPS {
        return Err(FORA_DOS_LIMITES.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calcular_encargos_igual_a_calcular() {
        let emissao = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let vencimento = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        for hoje in [
            emissao,
            vencimento,
            NaiveDate::from_ymd_opt(2026, 10, 2).unwrap(),
            NaiveDate::from_ymd_opt(2027, 3, 15).unwrap(),
        ] {
            for (principal, multa, juros) in
                [(10_000, 200, 100), (1, 0, 0), (987_654_321, 1_000, 333)]
            {
                let antigo = calcular(emissao, 30, principal, hoje, multa, juros).unwrap();
                let novo = calcular_encargos(vencimento, principal, hoje, multa, juros).unwrap();
                assert_eq!(antigo, novo);
            }
        }
    }

    #[test]
    fn calcular_encargos_valida_limites() {
        let dia = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert!(calcular_encargos(dia, 0, dia, 0, 0).is_err());
        assert!(calcular_encargos(dia, MAX_CENTAVOS + 1, dia, 0, 0).is_err());
        assert!(calcular_encargos(dia, 1, dia, MAX_BPS + 1, 0).is_err());
        assert!(calcular_encargos(dia, 1, dia, 0, MAX_BPS + 1).is_err());
        assert!(calcular_encargos(dia, MAX_CENTAVOS, dia, MAX_BPS, MAX_BPS).is_ok());
        assert_eq!(MAX_BPS, 10_000, "100% — mesmo limite do servidor");
    }

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
