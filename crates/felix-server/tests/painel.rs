//! Painel: totais, aging, por UG e listas com números conferidos à mão.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod comum;

use axum::http::{Method, StatusCode};
use chrono::NaiveDate;
use comum::TestApp;
use felix_server::http::Papel;
use serde_json::{Value, json};
use uuid::Uuid;

async fn enviar(
    app: &TestApp,
    metodo: Method,
    cookie: &str,
    caminho: &str,
    corpo: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = app.req(metodo, caminho).cookie(cookie);
    if let Some(corpo) = corpo {
        req = req.json(corpo);
    }
    let r = req.enviar().await;
    (r.status, r.json)
}

#[tokio::test]
async fn painel_com_numeros_conferidos() {
    let app = TestApp::novo().await;
    app.fixar_hoje(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
    let (_, org, cookie) = app.dono_com_org().await;

    // Painel vazio: faixas presentes com zero.
    let (status, json) = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("/api/v1/orgs/{org}/painel"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["a_receber_centavos"], 0);
    assert_eq!(json["dados"]["aging"].as_array().unwrap().len(), 5);

    let mut tx = app.tenant(org).await;
    let empresa: Uuid = sqlx::query_scalar(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj) VALUES ($1, 'F', '19116488000145') RETURNING id",
    )
    .bind(org)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut empenhos = Vec::new();
    for (ug, nome, numero, valor) in [
        ("160203", "Base Adm", "NE-A", 1_000_000),
        ("160100", "Hospital", "NE-B", 500_000),
        ("160300", "Anulada", "NE-C", 10_000),
    ] {
        let (status, json) = enviar(
            &app,
            Method::POST,
            &cookie,
            &format!("/api/v1/orgs/{org}/empenhos"),
            Some(json!({"empresa_id": empresa, "ug": ug, "ug_nome": nome, "numero": numero, "valor_total_centavos": valor})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
        empenhos.push(json["dados"]["id"].as_str().unwrap().to_owned());
    }
    let (status, _) = enviar(
        &app,
        Method::POST,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos/{}/anular", empenhos[2]),
        Some(json!({"motivo": "teste"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // (empenho, NF, valor, emissão, evento) — vencimento = emissão + 30 dias.
    let notas = [
        (0, "n1", 100_000, "2026-09-01", None), // venc 01/10: 19 dias (1_30)
        (
            0,
            "n2",
            200_000,
            "2026-07-01",
            Some(("liquidada", "2026-08-01")),
        ), // venc 31/07: 81 dias (61_90)
        (1, "n3", 50_000, "2026-10-10", None),  // venc 09/11: a vencer em 30 dias
        (1, "n4", 30_000, "2026-10-15", None),  // venc 14/11: a vencer em 30 dias
        (0, "n5", 40_000, "2026-09-05", Some(("paga", "2026-10-05"))), // recebido no mês
        (0, "n6", 10_000, "2026-08-01", Some(("paga", "2026-09-15"))), // recebido no mês anterior
        (
            1,
            "n7",
            50_000,
            "2026-10-01",
            Some(("cancelada", "2026-10-02")),
        ),
        (0, "n8", 70_000, "2026-05-01", None), // venc 31/05: 142 dias (90_mais)
    ];
    let mut ids = std::collections::HashMap::new();
    for (empenho, numero, valor, emissao, evento) in notas {
        let (status, json) = enviar(
            &app,
            Method::POST,
            &cookie,
            &format!("/api/v1/orgs/{org}/empenhos/{}/notas", empenhos[empenho]),
            Some(json!({"numero": numero, "tipo": "produto", "valor_centavos": valor, "data_emissao": emissao})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
        let id = json["dados"]["id"].as_str().unwrap().to_owned();
        if let Some((tipo, data)) = evento {
            let (status, json) = enviar(
                &app,
                Method::POST,
                &cookie,
                &format!("/api/v1/orgs/{org}/notas/{id}/eventos"),
                Some(json!({"tipo": tipo, "data": data})),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{json}");
        }
        ids.insert(numero, id);
    }

    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let (status, json) = enviar(
        &app,
        Method::GET,
        &leitor,
        &format!("/api/v1/orgs/{org}/painel"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let p = &json["dados"];
    assert_eq!(p["a_receber_centavos"], 450_000);
    assert_eq!(p["vencido_centavos"], 370_000);
    assert_eq!(p["vence_30d_centavos"], 80_000);
    assert_eq!(p["recebido_mes_centavos"], 40_000);
    assert_eq!(p["notas_abertas"], 5);
    assert_eq!(p["notas_vencidas"], 3);
    assert_eq!(p["empenhos_ativos"], 2);
    // A: 1.000.000 − (n1+n2+n5+n6+n8 = 420.000); B: 500.000 − (n3+n4 = 80.000).
    assert_eq!(p["saldo_empenhos_centavos"], 1_000_000);
    assert_eq!(
        p["aging"],
        json!([
            {"faixa": "a_vencer", "centavos": 80_000, "quantidade": 2},
            {"faixa": "1_30", "centavos": 100_000, "quantidade": 1},
            {"faixa": "31_60", "centavos": 0, "quantidade": 0},
            {"faixa": "61_90", "centavos": 200_000, "quantidade": 1},
            {"faixa": "90_mais", "centavos": 70_000, "quantidade": 1}
        ])
    );
    assert_eq!(
        p["por_ug"],
        json!([
            {"ug": "160203", "ug_nome": "Base Adm", "aberto_centavos": 370_000,
             "vencido_centavos": 370_000, "atraso_medio_dias": 81},
            {"ug": "160100", "ug_nome": "Hospital", "aberto_centavos": 80_000,
             "vencido_centavos": 0, "atraso_medio_dias": 0}
        ])
    );
    let numeros = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|n| n["numero"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(numeros(&p["proximos_vencimentos"]), ["n3", "n4"]);
    assert_eq!(numeros(&p["mais_atrasadas"]), ["n8", "n2", "n1"]);
    assert_eq!(p["mais_atrasadas"][0]["dias_atraso"], 142);
    assert_eq!(p["proximos_vencimentos"][0]["id"], json!(ids["n3"]));

    // Outra organização vê painel zerado; não membro → 404.
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let (_, json) = enviar(
        &app,
        Method::GET,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/painel"),
        None,
    )
    .await;
    assert_eq!(json["dados"]["a_receber_centavos"], 0);
    assert_eq!(json["dados"]["empenhos_ativos"], 0);
    let (status, _) = enviar(
        &app,
        Method::GET,
        &cookie_b,
        &format!("/api/v1/orgs/{org}/painel"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.encerrar().await;
}
