//! Feriados nacionais e dias úteis.
//!
//! Carnaval (segunda e terça) e Corpus Christi são pontos facultativos
//! federais, mas são tratados como dias sem expediente: escolha conservadora,
//! que empurra o vencimento para mais tarde. Feriados estaduais e municipais
//! não entram no cálculo.

use chrono::{Datelike, Days, NaiveDate, Weekday};

/// Domingo de Páscoa pelo algoritmo de Meeus/Jones/Butcher (calendário
/// gregoriano). Devolve `None` fora de 1583..=9999.
pub fn pascoa(ano: i32) -> Option<NaiveDate> {
    if !(1583..=9999).contains(&ano) {
        return None;
    }
    let a = ano % 19;
    let b = ano / 100;
    let c = ano % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let mes = (h + l - 7 * m + 114) / 31;
    let dia = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(ano, u32::try_from(mes).ok()?, u32::try_from(dia).ok()?)
}

/// Feriados nacionais do ano, ordenados e sem repetição.
pub fn feriados_nacionais(ano: i32) -> Vec<NaiveDate> {
    let mut fixos = vec![
        (1, 1),
        (4, 21),
        (5, 1),
        (9, 7),
        (10, 12),
        (11, 2),
        (11, 15),
        (12, 25),
    ];
    // Dia Nacional de Zumbi e da Consciência Negra (Lei 14.759/2023).
    if ano >= 2024 {
        fixos.push((11, 20));
    }
    let mut lista: Vec<NaiveDate> = fixos
        .into_iter()
        .filter_map(|(mes, dia)| NaiveDate::from_ymd_opt(ano, mes, dia))
        .collect();
    if let Some(domingo) = pascoa(ano) {
        // Carnaval (segunda e terça), Sexta-feira Santa e Corpus Christi.
        let moveis = [
            domingo.checked_sub_days(Days::new(48)),
            domingo.checked_sub_days(Days::new(47)),
            domingo.checked_sub_days(Days::new(2)),
            domingo.checked_add_days(Days::new(60)),
        ];
        lista.extend(moveis.into_iter().flatten());
    }
    lista.sort_unstable();
    lista.dedup();
    lista
}

/// Segunda a sexta e fora dos feriados nacionais.
pub fn eh_dia_util(d: NaiveDate) -> bool {
    CalendarioUtil::default().eh_dia_util(d)
}

/// Guarda os feriados do último ano consultado para evitar recalcular a
/// lista a cada dia em laços.
#[derive(Debug, Default)]
pub(crate) struct CalendarioUtil {
    ano: Option<i32>,
    feriados: Vec<NaiveDate>,
}

impl CalendarioUtil {
    pub(crate) fn eh_dia_util(&mut self, d: NaiveDate) -> bool {
        if matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            return false;
        }
        let ano = d.year();
        if self.ano != Some(ano) {
            self.ano = Some(ano);
            self.feriados = feriados_nacionais(ano);
        }
        self.feriados.binary_search(&d).is_err()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn data(a: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, d).unwrap()
    }

    #[test]
    fn pascoa_de_anos_conhecidos() {
        for (ano, mes, dia) in [
            (2000, 4, 23),
            (2024, 3, 31),
            (2025, 4, 20),
            (2026, 4, 5),
            (2027, 3, 28),
            (2038, 4, 25),
        ] {
            assert_eq!(pascoa(ano), Some(data(ano, mes, dia)), "{ano}");
        }
    }

    #[test]
    fn pascoa_fora_do_calendario_gregoriano_suportado() {
        assert_eq!(pascoa(1582), None);
        assert_eq!(pascoa(10_000), None);
    }

    #[test]
    fn feriados_de_2026_incluem_fixos_e_moveis() {
        let lista = feriados_nacionais(2026);
        for (m, d) in [
            (1, 1),
            (2, 16),
            (2, 17),
            (4, 3),
            (4, 21),
            (5, 1),
            (6, 4),
            (9, 7),
            (10, 12),
            (11, 2),
            (11, 15),
            (11, 20),
            (12, 25),
        ] {
            assert!(lista.contains(&data(2026, m, d)), "{m}/{d}");
        }
        assert_eq!(lista.len(), 13);
        assert!(lista.is_sorted_by(|a, b| a < b));
    }

    #[test]
    fn consciencia_negra_so_a_partir_de_2024() {
        assert!(!feriados_nacionais(2023).contains(&data(2023, 11, 20)));
        assert!(feriados_nacionais(2024).contains(&data(2024, 11, 20)));
    }

    #[test]
    fn feriados_coincidentes_nao_se_repetem() {
        // Em 2000 a Sexta-feira Santa caiu em 21/04 (Tiradentes).
        let lista = feriados_nacionais(2000);
        assert_eq!(lista.iter().filter(|d| **d == data(2000, 4, 21)).count(), 1);
        assert!(lista.is_sorted_by(|a, b| a < b));
    }

    #[test]
    fn dia_util_exclui_fim_de_semana_e_feriado() {
        assert!(eh_dia_util(data(2026, 10, 20))); // terça
        assert!(!eh_dia_util(data(2026, 10, 17))); // sábado
        assert!(!eh_dia_util(data(2026, 10, 18))); // domingo
        assert!(!eh_dia_util(data(2026, 10, 12))); // segunda, feriado
        assert!(!eh_dia_util(data(2026, 2, 17))); // terça de Carnaval
    }
}
