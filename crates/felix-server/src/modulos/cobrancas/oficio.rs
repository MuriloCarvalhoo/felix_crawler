//! Minuta de ofício de cobrança administrativa (Lei 14.133/2021, arts. 92, V,
//! e 141). Pede informações sobre a ordem cronológica e o estágio da despesa,
//! sem presumir irregularidade apenas pelo atraso.

use super::tipos::ContextoOficio;
use crate::modulos::empenhos::comum::{data_br, data_extenso, formatar_cnpj};
use chrono::{Datelike, NaiveDate};
use felix_core::{cobranca::Calculo, dinheiro::reais};
use std::fmt::Write;

/// Percentual a partir de pontos-base: 200 → "2,00".
fn percentual(bps: u32) -> String {
    reais(i64::from(bps))
}

fn linha_valor(rotulo: &str, centavos: i64) -> String {
    let valor = format!("R$ {}", reais(centavos));
    let pontos = 58_usize.saturating_sub(rotulo.chars().count() + valor.chars().count());
    format!("   {rotulo} {} {valor}\n", ".".repeat(pontos.max(3)))
}

/// Gera o texto da minuta. `data` é a data do ofício e da atualização dos valores.
pub fn gerar(
    c: &ContextoOficio,
    calculo: &Calculo,
    multa_bps: u32,
    juros_bps: u32,
    data: NaiveDate,
) -> String {
    let documento = if c.nota_tipo == "servico" {
        "nota fiscal de serviço"
    } else {
        "nota fiscal"
    };
    let unidade = c.ug_nome.as_deref().unwrap_or("Unidade Gestora");
    let cnpj = formatar_cnpj(&c.cnpj);
    let mut referencia = format!("Nota de Empenho nº {}", c.empenho_numero);
    match (&c.modalidade, &c.numero_edital_ata) {
        (Some(m), Some(e)) => {
            let _ = write!(referencia, " | {m} nº {e}");
        }
        (Some(m), None) => {
            let _ = write!(referencia, " | {m}");
        }
        (None, Some(e)) => {
            let _ = write!(referencia, " | Edital/Ata nº {e}");
        }
        (None, None) => {}
    }
    let _ = write!(referencia, " | NF nº {}", c.nota_numero);

    let mut t = String::with_capacity(4096);
    t.push_str("MINUTA — revise os dados e as condições do contrato antes de enviar.\n\n");
    let _ = writeln!(t, "OFÍCIO Nº ____/{}\n", data.year());
    let _ = writeln!(t, "__________, {}.\n", data_extenso(data));
    t.push_str("À\n");
    let _ = writeln!(t, "{unidade} — UG {}", c.ug);
    if let Some(orgao) = &c.orgao {
        let _ = writeln!(t, "{orgao}");
    }
    t.push_str("A/C: Ordenador(a) de Despesas / Setor de Execução Financeira\n\n");
    let _ = writeln!(
        t,
        "Assunto: Solicitação de informações sobre o pagamento da {documento} nº {}",
        c.nota_numero
    );
    let _ = writeln!(t, "Referência: {referencia}\n");
    t.push_str("Prezado(a) Senhor(a),\n\n");
    let _ = writeln!(
        t,
        "{}, inscrita no CNPJ sob o nº {cnpj}, fornecedora vinculada à Nota de Empenho \
         acima referida, vem, respeitosamente, expor e solicitar o que segue.\n",
        c.razao_social
    );
    let recebimento = c
        .data_recebimento
        .map(|r| format!(", recebida pela Administração em {}", data_br(r)))
        .unwrap_or_default();
    let _ = writeln!(
        t,
        "1. Em {}, foi emitida a {documento} nº {}, no valor de R$ {}{recebimento}, \
         referente ao objeto da Nota de Empenho nº {}.\n",
        data_br(c.data_emissao),
        c.nota_numero,
        reais(c.valor_centavos),
        c.empenho_numero
    );
    if let (true, Some(pagamento)) = (c.nota_status == "paga", c.data_pagamento) {
        let _ = writeln!(
            t,
            "2. Pelas condições de pagamento aplicáveis, o vencimento foi apurado em {}. \
             O pagamento foi identificado em {}, {} dia(s) após o vencimento.\n",
            data_br(calculo.vencimento),
            data_br(pagamento),
            felix_core::prazos::dias_atraso(calculo.vencimento, pagamento)
        );
    } else if calculo.dias_atraso > 0 {
        let _ = writeln!(
            t,
            "2. Pelas condições de pagamento aplicáveis, o vencimento foi apurado em {}. \
             Até {}, não identificamos o pagamento, o que corresponde a {} dia(s) de atraso.\n",
            data_br(calculo.vencimento),
            data_br(data),
            calculo.dias_atraso
        );
    } else {
        let _ = writeln!(
            t,
            "2. Pelas condições de pagamento aplicáveis, o vencimento foi apurado em {}.\n",
            data_br(calculo.vencimento)
        );
    }
    t.push_str(
        "3. Considerando a ordem cronológica de pagamentos prevista no art. 141 da \
         Lei nº 14.133/2021, solicitamos a gentileza de nos informar:\n\
         \u{20}  a) a posição desta nota fiscal na ordem cronológica de pagamentos da unidade;\n\
         \u{20}  b) o estágio atual da despesa (liquidação ou pagamento);\n\
         \u{20}  c) a data da liquidação, caso já realizada;\n\
         \u{20}  d) a previsão de pagamento.\n\n",
    );
    t.push_str(
        "4. O art. 92, V, da Lei nº 14.133/2021 determina que o contrato estabeleça os \
         critérios de atualização monetária entre a data do adimplemento das obrigações e \
         a do efetivo pagamento. Apresentamos o demonstrativo abaixo, sujeito à conferência \
         das condições contratuais e do termo inicial:\n\n",
    );
    t.push_str(&linha_valor("Principal", calculo.principal_centavos));
    t.push_str(&linha_valor(
        &format!("Multa ({}%)", percentual(multa_bps)),
        calculo.multa_centavos,
    ));
    t.push_str(&linha_valor(
        &format!("Juros ({}% a.m., pro rata die)", percentual(juros_bps)),
        calculo.juros_centavos,
    ));
    t.push_str(&linha_valor(
        &format!("Total atualizado até {}", data_br(data)),
        calculo.total_centavos,
    ));
    t.push('\n');
    t.push_str(
        "5. Esta solicitação tem caráter informativo e não presume irregularidade por parte \
         da Administração; busca apenas acompanhar o processo e viabilizar a regularização \
         do pagamento.\n\n",
    );
    t.push_str(
        "Colocamo-nos à disposição para esclarecimentos e para a apresentação de documentos \
         complementares.\n\nAtenciosamente,\n\n\n______________________________\n",
    );
    let _ = writeln!(t, "{}", c.razao_social);
    let _ = writeln!(t, "CNPJ {cnpj}");
    t
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod testes {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn texto_tem_referencias_e_valores() {
        let d = |a, m, dia| NaiveDate::from_ymd_opt(a, m, dia).unwrap();
        let c = ContextoOficio {
            nota_id: Uuid::nil(),
            nota_numero: "123".into(),
            nota_tipo: "produto".into(),
            nota_status: "emitida".into(),
            valor_centavos: 100_000,
            data_emissao: d(2026, 9, 1),
            data_recebimento: None,
            data_pagamento: None,
            vencimento: d(2026, 10, 1),
            empenho_numero: "2026NE000290".into(),
            ug: "160203".into(),
            ug_nome: Some("Base Administrativa".into()),
            orgao: Some("Comando do Exército".into()),
            modalidade: Some("Pregão".into()),
            numero_edital_ata: Some("90012/2025".into()),
            razao_social: "Empresa X Ltda".into(),
            cnpj: "19116488000145".into(),
        };
        let calc = felix_core::cobranca::calcular_encargos(
            c.vencimento,
            100_000,
            d(2026, 10, 20),
            200,
            100,
        )
        .unwrap();
        let t = gerar(&c, &calc, 200, 100, d(2026, 10, 20));
        for trecho in [
            "MINUTA",
            "20 de outubro de 2026",
            "19.116.488/0001-45",
            "Empresa X Ltda",
            "UG 160203",
            "Pregão nº 90012/2025",
            "art. 141",
            "art. 92, V",
            "ordem cronológica",
            "estágio atual da despesa",
            "data da liquidação",
            "previsão de pagamento",
            "19 dia(s) de atraso",
            "R$ 1.000,00",
            "R$ 20,00",
            "R$ 6,33",
            "R$ 1.026,33",
            "não presume irregularidade",
        ] {
            assert!(t.contains(trecho), "faltou {trecho}:\n{t}");
        }
    }
}
