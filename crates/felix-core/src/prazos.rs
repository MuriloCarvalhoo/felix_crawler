//! Regras de vencimento de empenhos (ADR-010).

use crate::feriados::CalendarioUtil;
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

/// Limite do art. 75, II, da Lei 14.133/2021 em 2026: R$ 65.492,11
/// (Decreto 12.807/2025). Até esse valor, os prazos da IN 77/2022 caem
/// pela metade.
pub const LIMITE_ART75_II_CENTAVOS: i64 = 6_549_211;

/// Dias úteis para liquidar e, depois, para pagar (IN SEGES/ME 77/2022, art. 7º).
const DIAS_UTEIS_IN77: u32 = 10;
const DIAS_UTEIS_IN77_REDUZIDO: u32 = 5;
const PRAZO_MAXIMO_DIAS: u32 = 3650;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegraVencimento {
    /// Emissão do empenho + N dias corridos (comportamento original).
    #[default]
    CorridosEmissao,
    /// Recebimento da NF + dias úteis de liquidação + dias úteis de pagamento.
    In77Uteis,
}

impl RegraVencimento {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CorridosEmissao => "corridos_emissao",
            Self::In77Uteis => "in77_uteis",
        }
    }
}

impl FromStr for RegraVencimento {
    type Err = ErroPrazo;

    fn from_str(texto: &str) -> Result<Self, Self::Err> {
        match texto {
            "corridos_emissao" => Ok(Self::CorridosEmissao),
            "in77_uteis" => Ok(Self::In77Uteis),
            _ => Err(ErroPrazo::RegraDesconhecida(texto.into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ErroPrazo {
    #[error("regra de vencimento desconhecida: {0}")]
    RegraDesconhecida(String),
    #[error("prazo de {0} dias fora dos limites (1 a 3650)")]
    PrazoForaDosLimites(u32),
    #[error("valor do empenho inválido")]
    ValorEmpenhoInvalido,
    #[error("vencimento fora do intervalo de datas suportado")]
    DataForaDoIntervalo,
}

/// O n-ésimo dia útil estritamente após `inicio`; `n = 0` devolve `inicio`.
pub fn somar_dias_uteis(inicio: NaiveDate, n: u32) -> Option<NaiveDate> {
    let mut calendario = CalendarioUtil::default();
    let mut data = inicio;
    let mut restantes = n;
    while restantes > 0 {
        data = data.succ_opt()?;
        if calendario.eh_dia_util(data) {
            restantes -= 1;
        }
    }
    Some(data)
}

/// Vencimento conforme a regra do empenho. Em `In77Uteis`, `prazo_dias` é
/// ignorado e a base é o recebimento da NF (ou a emissão, se ausente).
pub fn calcular_vencimento(
    regra: RegraVencimento,
    data_emissao: NaiveDate,
    data_recebimento: Option<NaiveDate>,
    prazo_dias: u32,
    valor_empenho_centavos: i64,
) -> Result<NaiveDate, ErroPrazo> {
    if valor_empenho_centavos < 0 {
        return Err(ErroPrazo::ValorEmpenhoInvalido);
    }
    match regra {
        RegraVencimento::CorridosEmissao => {
            if !(1..=PRAZO_MAXIMO_DIAS).contains(&prazo_dias) {
                return Err(ErroPrazo::PrazoForaDosLimites(prazo_dias));
            }
            data_emissao
                .checked_add_days(Days::new(u64::from(prazo_dias)))
                .ok_or(ErroPrazo::DataForaDoIntervalo)
        }
        RegraVencimento::In77Uteis => {
            let dias = if valor_empenho_centavos <= LIMITE_ART75_II_CENTAVOS {
                DIAS_UTEIS_IN77_REDUZIDO
            } else {
                DIAS_UTEIS_IN77
            };
            let base = data_recebimento.unwrap_or(data_emissao);
            let liquidacao = somar_dias_uteis(base, dias).ok_or(ErroPrazo::DataForaDoIntervalo)?;
            somar_dias_uteis(liquidacao, dias).ok_or(ErroPrazo::DataForaDoIntervalo)
        }
    }
}

/// Dias corridos após o vencimento; zero se ainda não venceu.
pub fn dias_atraso(vencimento: NaiveDate, hoje: NaiveDate) -> i64 {
    (hoje - vencimento).num_days().max(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn data(a: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, d).unwrap()
    }

    #[test]
    fn regra_converte_de_e_para_texto() {
        for regra in [RegraVencimento::CorridosEmissao, RegraVencimento::In77Uteis] {
            assert_eq!(regra.as_str().parse::<RegraVencimento>(), Ok(regra));
        }
        assert_eq!(
            RegraVencimento::CorridosEmissao.as_str(),
            "corridos_emissao"
        );
        assert_eq!(RegraVencimento::In77Uteis.as_str(), "in77_uteis");
        assert!("in77".parse::<RegraVencimento>().is_err());
        assert_eq!(RegraVencimento::default(), RegraVencimento::CorridosEmissao);
    }

    #[test]
    fn somar_zero_dias_devolve_o_inicio() {
        let sabado = data(2026, 10, 17);
        assert_eq!(somar_dias_uteis(sabado, 0), Some(sabado));
    }

    #[test]
    fn somar_atravessa_fim_de_semana() {
        // 16/10/2026 é sexta-feira.
        assert_eq!(
            somar_dias_uteis(data(2026, 10, 16), 1),
            Some(data(2026, 10, 19))
        );
        // A partir do sábado, o primeiro dia útil é segunda.
        assert_eq!(
            somar_dias_uteis(data(2026, 10, 17), 1),
            Some(data(2026, 10, 19))
        );
    }

    #[test]
    fn somar_atravessa_sexta_feira_santa() {
        // 01/04/2026 (quarta): 02/04 (1), 03/04 Sexta Santa, 06/04 (2), 07/04 (3).
        assert_eq!(
            somar_dias_uteis(data(2026, 4, 1), 3),
            Some(data(2026, 4, 7))
        );
    }

    #[test]
    fn somar_atravessa_tiradentes() {
        // 17/04/2026 (sexta): 20/04 (1), 21/04 Tiradentes, 22/04 (2).
        assert_eq!(
            somar_dias_uteis(data(2026, 4, 17), 2),
            Some(data(2026, 4, 22))
        );
    }

    #[test]
    fn somar_atravessa_virada_de_ano() {
        // 30/12/2026 (quarta): 31/12 (1), 01/01 feriado, 04/01/2027 (2).
        assert_eq!(
            somar_dias_uteis(data(2026, 12, 30), 2),
            Some(data(2027, 1, 4))
        );
    }

    #[test]
    fn somar_alem_da_data_maxima_falha() {
        assert_eq!(somar_dias_uteis(NaiveDate::MAX, 1), None);
    }

    #[test]
    fn corridos_emissao_mantem_comportamento_antigo() {
        let emissao = data(2026, 9, 1);
        let r = RegraVencimento::CorridosEmissao;
        assert_eq!(
            calcular_vencimento(r, emissao, Some(data(2026, 9, 20)), 30, 1),
            Ok(data(2026, 10, 1))
        );
        assert_eq!(
            calcular_vencimento(r, emissao, None, 3650, 1),
            Ok(data(2036, 8, 29))
        );
        assert_eq!(
            calcular_vencimento(r, emissao, None, 0, 1),
            Err(ErroPrazo::PrazoForaDosLimites(0))
        );
        assert_eq!(
            calcular_vencimento(r, emissao, None, 3651, 1),
            Err(ErroPrazo::PrazoForaDosLimites(3651))
        );
        assert_eq!(
            calcular_vencimento(r, NaiveDate::MAX, None, 1, 1),
            Err(ErroPrazo::DataForaDoIntervalo)
        );
    }

    #[test]
    fn in77_acima_do_limite_usa_dez_mais_dez_dias_uteis() {
        // Recebimento 02/03/2026 (segunda) + 20 dias úteis = 30/03/2026.
        let venc = calcular_vencimento(
            RegraVencimento::In77Uteis,
            data(2026, 1, 5),
            Some(data(2026, 3, 2)),
            0,
            LIMITE_ART75_II_CENTAVOS + 1,
        );
        assert_eq!(venc, Ok(data(2026, 3, 30)));
    }

    #[test]
    fn in77_ate_o_limite_usa_cinco_mais_cinco_dias_uteis() {
        // 02/03/2026 + 10 dias úteis = 16/03/2026.
        let venc = calcular_vencimento(
            RegraVencimento::In77Uteis,
            data(2026, 1, 5),
            Some(data(2026, 3, 2)),
            0,
            LIMITE_ART75_II_CENTAVOS,
        );
        assert_eq!(venc, Ok(data(2026, 3, 16)));
    }

    #[test]
    fn in77_sem_recebimento_conta_da_emissao_e_pula_carnaval() {
        // 13/02/2026 (sexta): 16 e 17/02 Carnaval; 10 dias úteis = 03/03/2026.
        let venc = calcular_vencimento(
            RegraVencimento::In77Uteis,
            data(2026, 2, 13),
            None,
            30,
            1_000,
        );
        assert_eq!(venc, Ok(data(2026, 3, 3)));
    }

    #[test]
    fn in77_ignora_prazo_em_dias_corridos() {
        let calc = |prazo| {
            calcular_vencimento(
                RegraVencimento::In77Uteis,
                data(2026, 3, 2),
                None,
                prazo,
                LIMITE_ART75_II_CENTAVOS + 1,
            )
        };
        assert_eq!(calc(0), calc(9999));
    }

    #[test]
    fn valor_de_empenho_negativo_e_rejeitado() {
        for regra in [RegraVencimento::CorridosEmissao, RegraVencimento::In77Uteis] {
            assert_eq!(
                calcular_vencimento(regra, data(2026, 3, 2), None, 30, -1),
                Err(ErroPrazo::ValorEmpenhoInvalido)
            );
        }
    }

    #[test]
    fn dias_atraso_nunca_e_negativo() {
        let venc = data(2026, 10, 1);
        assert_eq!(dias_atraso(venc, data(2026, 10, 16)), 15);
        assert_eq!(dias_atraso(venc, venc), 0);
        assert_eq!(dias_atraso(venc, data(2026, 9, 1)), 0);
    }
}
