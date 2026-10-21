//! Notas fiscais: saldo, concorrência, máquina de estados, vencimento, atraso,
//! filtros e isolamento.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod comum;

use axum::http::{Method, StatusCode};
use chrono::NaiveDate;
use comum::TestApp;
use felix_server::http::Papel;
use serde_json::{Value, json};
use uuid::Uuid;

const CNPJ: &str = "19116488000145";
/// R$ 65.492,11 (art. 75, II, da Lei 14.133/2021 em 2026).
const LIMITE: i64 = 6_549_211;

fn data(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

async fn criar_empresa(app: &TestApp, org: Uuid) -> Uuid {
    let mut tx = app.tenant(org).await;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj) VALUES ($1, 'Fornecedora Ltda', $2) RETURNING id",
    )
    .bind(org)
    .bind(CNPJ)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

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

async fn criar_empenho(
    app: &TestApp,
    org: Uuid,
    cookie: &str,
    numero: &str,
    valor: i64,
    extra: Value,
) -> String {
    let empresa: Uuid = {
        let mut tx = app.tenant(org).await;
        let existente: Option<Uuid> = sqlx::query_scalar("SELECT id FROM empresas LIMIT 1")
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
        drop(tx);
        match existente {
            Some(id) => id,
            None => criar_empresa(app, org).await,
        }
    };
    let mut corpo = json!({"empresa_id": empresa, "ug": "160203", "numero": numero,
                           "valor_total_centavos": valor, "ug_nome": "Base Adm"});
    for (k, v) in extra.as_object().unwrap() {
        corpo[k] = v.clone();
    }
    let (status, json) = enviar(
        app,
        Method::POST,
        cookie,
        &format!("/api/v1/orgs/{org}/empenhos"),
        Some(corpo),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    json["dados"]["id"].as_str().unwrap().to_owned()
}

async fn criar_nota(
    app: &TestApp,
    org: Uuid,
    cookie: &str,
    empenho: &str,
    corpo: Value,
) -> (StatusCode, Value) {
    enviar(
        app,
        Method::POST,
        cookie,
        &format!("/api/v1/orgs/{org}/empenhos/{empenho}/notas"),
        Some(corpo),
    )
    .await
}

fn nf(numero: &str, valor: i64, emissao: &str) -> Value {
    json!({"numero": numero, "tipo": "produto", "valor_centavos": valor, "data_emissao": emissao})
}

async fn evento(
    app: &TestApp,
    org: Uuid,
    cookie: &str,
    nota: &str,
    corpo: Value,
) -> (StatusCode, Value) {
    enviar(
        app,
        Method::POST,
        cookie,
        &format!("/api/v1/orgs/{org}/notas/{nota}/eventos"),
        Some(corpo),
    )
    .await
}

#[tokio::test]
async fn cria_notas_respeitando_saldo_e_duplicidade() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (_, org, cookie) = app.dono_com_org().await;
    let empenho = criar_empenho(&app, org, &cookie, "2026NE000001", 100_000, json!({})).await;

    let (status, json) = criar_nota(
        &app,
        org,
        &cookie,
        &empenho,
        nf("100", 60_000, "2026-09-01"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    let n = &json["dados"];
    assert_eq!(n["vencimento"], "2026-10-01");
    assert_eq!(n["status"], "emitida");
    assert_eq!(n["empenho_numero"], "2026NE000001");
    assert_eq!(n["ug"], "160203");
    assert_eq!(n["ug_nome"], "Base Adm");
    assert_eq!(n["empresa_cnpj"], CNPJ);
    assert_eq!(n["dias_atraso"], 19);
    assert_eq!(n["valor_pago_centavos"], 0);

    let (status, json) = criar_nota(
        &app,
        org,
        &cookie,
        &empenho,
        nf("101", 40_001, "2026-09-01"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        json["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("400,00"),
        "{json}"
    );
    let (status, _) =
        criar_nota(&app, org, &cookie, &empenho, nf("100", 1_000, "2026-09-01")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = criar_nota(
        &app,
        org,
        &cookie,
        &empenho,
        json!({"numero": "100", "tipo": "servico", "valor_centavos": 40_000, "data_emissao": "2026-09-02"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Validações.
    let (status, json) = criar_nota(
        &app,
        org,
        &cookie,
        &empenho,
        json!({"numero": "", "tipo": "x", "valor_centavos": 0, "data_emissao": "2026-12-01"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    for campo in ["numero", "tipo", "valor_centavos", "data_emissao"] {
        assert!(json["erro"]["campos"].get(campo).is_some(), "{campo}");
    }
    let (status, json) = criar_nota(
        &app,
        org,
        &cookie,
        &empenho,
        json!({"numero": "9", "tipo": "produto", "valor_centavos": 1, "data_emissao": "2026-09-10",
               "data_recebimento": "2026-09-09"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(json["erro"]["campos"].get("data_recebimento").is_some());

    // Empenho reflete faturado e saldo.
    let (_, json) = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos/{empenho}"),
        None,
    )
    .await;
    assert_eq!(json["dados"]["faturado_centavos"], 100_000);
    assert_eq!(json["dados"]["saldo_centavos"], 0);
    assert_eq!(json["dados"]["notas"].as_array().unwrap().len(), 2);

    // Leitor não cria; empenho de outra org → 404.
    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let (status, _) = criar_nota(&app, org, &leitor, &empenho, nf("200", 1, "2026-09-01")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let (status, _) =
        criar_nota(&app, org_b, &cookie_b, &empenho, nf("200", 1, "2026-09-01")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = criar_nota(
        &app,
        org,
        &cookie,
        &Uuid::now_v7().to_string(),
        nf("200", 1, "2026-09-01"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.encerrar().await;
}

#[tokio::test]
async fn notas_simultaneas_nunca_deixam_saldo_negativo() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (_, org, cookie) = app.dono_com_org().await;
    let empenho = criar_empenho(&app, org, &cookie, "2026NE000002", 100_000, json!({})).await;
    let caminho = format!("/api/v1/orgs/{org}/empenhos/{empenho}/notas");
    let f = |i: u32| {
        app.req(Method::POST, &caminho)
            .cookie(&cookie)
            .json(nf(&format!("C{i}"), 15_000, "2026-10-01"))
            .enviar()
    };
    let r = tokio::join!(f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7), f(8), f(9));
    let status = [
        r.0.status, r.1.status, r.2.status, r.3.status, r.4.status, r.5.status, r.6.status,
        r.7.status, r.8.status, r.9.status,
    ];
    let criadas = status.iter().filter(|s| **s == StatusCode::CREATED).count();
    let recusadas = status
        .iter()
        .filter(|s| **s == StatusCode::CONFLICT)
        .count();
    assert_eq!((criadas, recusadas), (6, 4), "{status:?}");
    let (_, json) = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos/{empenho}"),
        None,
    )
    .await;
    assert_eq!(json["dados"]["faturado_centavos"], 90_000);
    assert_eq!(json["dados"]["saldo_centavos"], 10_000);
    app.encerrar().await;
}

#[tokio::test]
async fn maquina_de_estados_da_nota() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (_, org, cookie) = app.dono_com_org().await;
    let (_, operador) = app.membro(org, Papel::Operador).await;
    let empenho = criar_empenho(&app, org, &cookie, "2026NE000003", 100_000, json!({})).await;
    let (_, json) = criar_nota(&app, org, &cookie, &empenho, nf("1", 50_000, "2026-09-01")).await;
    let nota = json["dados"]["id"].as_str().unwrap().to_owned();

    // Datas inválidas e tipo inválido.
    let (status, _) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "liquidada", "data": "2026-08-31"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "liquidada", "data": "2026-10-21"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "estornada", "data": "2026-10-01"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "reaberta", "data": "2026-10-01"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // emitida → liquidada → paga (valor padrão).
    let (status, json) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "liquidada", "data": "2026-10-05", "documento": "2026NS000123"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["status"], "liquidada");
    assert_eq!(json["dados"]["data_liquidacao"], "2026-10-05");
    assert_eq!(json["dados"]["documento_liquidacao"], "2026NS000123");
    let (status, json) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "liquidada", "data": "2026-10-06"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        json["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("Transição inválida")
    );
    let (status, _) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "paga", "data": "2026-10-04"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "pagamento antes da liquidação"
    );
    let (status, json) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "paga", "data": "2026-10-10", "documento": "2026OB000777"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["status"], "paga");
    assert_eq!(json["dados"]["valor_pago_centavos"], 50_000);
    assert_eq!(json["dados"]["data_pagamento"], "2026-10-10");
    assert_eq!(json["dados"]["dias_atraso"], 0);
    let (status, json) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "cancelada", "data": "2026-10-11"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(json["erro"]["mensagem"].as_str().unwrap().contains("paga"));
    let (status, _) = enviar(
        &app,
        Method::PATCH,
        &operador,
        &format!("/api/v1/orgs/{org}/notas/{nota}"),
        Some(json!({"numero": "2"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "só emitida edita");

    // Reabrir: operador não; dono sim; limpa liquidação e pagamento.
    let (status, _) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "reaberta", "data": "2026-10-12"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, json) = evento(
        &app,
        org,
        &cookie,
        &nota,
        json!({"tipo": "reaberta", "data": "2026-10-12", "observacao": "OB estornada"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let n = &json["dados"];
    assert_eq!(n["status"], "emitida");
    assert_eq!(n["valor_pago_centavos"], 0);
    assert!(n["data_liquidacao"].is_null() && n["data_pagamento"].is_null());
    assert!(n["documento_liquidacao"].is_null() && n["documento_pagamento"].is_null());
    assert_eq!(n["dias_atraso"], 19);
    let (status, _) = evento(
        &app,
        org,
        &cookie,
        &nota,
        json!({"tipo": "reaberta", "data": "2026-10-12"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // emitida → paga direto (liquidação implícita) com valor menor.
    let (status, _) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "paga", "data": "2026-10-13", "valor_centavos": 50_001}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, json) = evento(
        &app,
        org,
        &operador,
        &nota,
        json!({"tipo": "paga", "data": "2026-10-13", "valor_centavos": 49_000}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dados"]["data_liquidacao"], "2026-10-13");
    assert_eq!(json["dados"]["valor_pago_centavos"], 49_000);

    // Detalhe: eventos com autor.
    let (status, json) = enviar(
        &app,
        Method::GET,
        &operador,
        &format!("/api/v1/orgs/{org}/notas/{nota}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let eventos: Vec<&str> = json["dados"]["eventos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["tipo"].as_str().unwrap())
        .collect();
    assert_eq!(eventos, ["liquidada", "paga", "reaberta", "paga"]);
    assert_eq!(json["dados"]["eventos"][0]["criado_por_nome"], "Membro");
    assert_eq!(json["dados"]["eventos"][0]["documento"], "2026NS000123");
    assert_eq!(json["dados"]["eventos"][1]["valor_centavos"], 50_000);
    assert_eq!(json["dados"]["eventos"][2]["observacao"], "OB estornada");
    assert_eq!(json["dados"]["cobrancas"], json!([]));

    // Cancelada libera saldo; reabrir revalida saldo.
    let (_, json) = criar_nota(&app, org, &cookie, &empenho, nf("2", 50_000, "2026-09-02")).await;
    let nota2 = json["dados"]["id"].as_str().unwrap().to_owned();
    let (status, json) = evento(
        &app,
        org,
        &operador,
        &nota2,
        json!({"tipo": "cancelada", "data": "2026-10-14"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dados"]["status"], "cancelada");
    let (status, _) = evento(
        &app,
        org,
        &operador,
        &nota2,
        json!({"tipo": "liquidada", "data": "2026-10-14"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = criar_nota(&app, org, &cookie, &empenho, nf("3", 50_000, "2026-09-03")).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, json) = evento(
        &app,
        org,
        &cookie,
        &nota2,
        json!({"tipo": "reaberta", "data": "2026-10-15"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert!(json["erro"]["mensagem"].as_str().unwrap().contains("saldo"));
    app.encerrar().await;
}

#[tokio::test]
async fn edicao_de_nota_emitida_recalcula_e_revalida() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (_, org, cookie) = app.dono_com_org().await;
    let empenho = criar_empenho(
        &app,
        org,
        &cookie,
        "2026NE000004",
        100_000,
        json!({"prazo_dias": 10}),
    )
    .await;
    let (_, json) = criar_nota(&app, org, &cookie, &empenho, nf("1", 60_000, "2026-09-01")).await;
    let nota = json["dados"]["id"].as_str().unwrap().to_owned();
    criar_nota(&app, org, &cookie, &empenho, nf("2", 30_000, "2026-09-01")).await;
    let caminho = format!("/api/v1/orgs/{org}/notas/{nota}");
    let (status, json) = enviar(
        &app,
        Method::PATCH,
        &cookie,
        &caminho,
        Some(json!({"valor_centavos": 70_000, "data_emissao": "2026-10-01"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["vencimento"], "2026-10-11");
    assert_eq!(json["dados"]["valor_centavos"], 70_000);
    let (status, _) = enviar(
        &app,
        Method::PATCH,
        &cookie,
        &caminho,
        Some(json!({"valor_centavos": 70_001})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = enviar(
        &app,
        Method::PATCH,
        &cookie,
        &caminho,
        Some(json!({"numero": "2"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "duplicada");
    let (status, _) = enviar(
        &app,
        Method::PATCH,
        &cookie,
        &caminho,
        Some(json!({"data_recebimento": "2026-09-30"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, json) = enviar(
        &app,
        Method::PATCH,
        &cookie,
        &caminho,
        Some(json!({"data_recebimento": "2026-10-02", "numero": "1A"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dados"]["data_recebimento"], "2026-10-02");
    assert_eq!(json["dados"]["numero"], "1A");
    let (status, json) = enviar(
        &app,
        Method::PATCH,
        &cookie,
        &caminho,
        Some(json!({"data_recebimento": null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["dados"]["data_recebimento"].is_null());
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let (status, _) = enviar(
        &app,
        Method::PATCH,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/notas/{nota}"),
        Some(json!({"numero": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = enviar(
        &app,
        Method::GET,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/notas/{nota}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.encerrar().await;
}

#[tokio::test]
async fn vencimento_pelas_duas_regras() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (_, org, cookie) = app.dono_com_org().await;
    let acima = criar_empenho(
        &app,
        org,
        &cookie,
        "IN77A",
        LIMITE + 1,
        json!({"regra_vencimento": "in77_uteis"}),
    )
    .await;
    let ate = criar_empenho(
        &app,
        org,
        &cookie,
        "IN77B",
        LIMITE,
        json!({"regra_vencimento": "in77_uteis"}),
    )
    .await;
    let corridos = criar_empenho(
        &app,
        org,
        &cookie,
        "CORR",
        LIMITE,
        json!({"prazo_dias": 45}),
    )
    .await;
    let corpo = json!({"numero": "1", "tipo": "servico", "valor_centavos": 1_000,
                       "data_emissao": "2026-02-27", "data_recebimento": "2026-03-02"});
    let (_, json) = criar_nota(&app, org, &cookie, &acima, corpo.clone()).await;
    // 02/03/2026 + 10 + 10 dias úteis = 30/03/2026.
    assert_eq!(json["dados"]["vencimento"], "2026-03-30");
    let (_, json) = criar_nota(&app, org, &cookie, &ate, corpo.clone()).await;
    // 02/03/2026 + 5 + 5 dias úteis = 16/03/2026.
    assert_eq!(json["dados"]["vencimento"], "2026-03-16");
    let (_, json) = criar_nota(&app, org, &cookie, &corridos, corpo).await;
    // 27/02/2026 + 45 dias corridos.
    assert_eq!(json["dados"]["vencimento"], "2026-04-13");
    let nota_ate = {
        let (_, json) = enviar(
            &app,
            Method::GET,
            &cookie,
            &format!("/api/v1/orgs/{org}/notas?empenho_id={ate}"),
            None,
        )
        .await;
        json["dados"][0]["id"].as_str().unwrap().to_owned()
    };
    // Aumentar o valor acima do limite dobra os prazos (recalcula notas abertas).
    let (status, _) = enviar(
        &app,
        Method::PATCH,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos/{ate}"),
        Some(json!({"valor_total_centavos": LIMITE + 1})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, json) = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("/api/v1/orgs/{org}/notas/{nota_ate}"),
        None,
    )
    .await;
    assert_eq!(json["dados"]["vencimento"], "2026-03-30");
    app.encerrar().await;
}

#[tokio::test]
async fn atraso_e_filtros_da_lista() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (_, org, cookie) = app.dono_com_org().await;
    let empenho = criar_empenho(&app, org, &cookie, "2026NE000005", 1_000_000, json!({})).await;
    let outro = criar_empenho(
        &app,
        org,
        &cookie,
        "2026NE000006",
        1_000_000,
        json!({"ug": "160100", "ug_nome": "Hospital"}),
    )
    .await;
    // vencimentos: 01/10 (vencida, 19 dias), 25/10 (vence em 5 dias), 19/11 (a vencer), 05/10 paga.
    let mut ids = Vec::new();
    for (empenho, numero, emissao) in [
        (&empenho, "A1", "2026-09-01"),
        (&empenho, "A2", "2026-09-25"),
        (&outro, "B1", "2026-10-20"),
        (&outro, "B2", "2026-09-05"),
    ] {
        let (status, json) =
            criar_nota(&app, org, &cookie, empenho, nf(numero, 1_000, emissao)).await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
        ids.push(json["dados"]["id"].as_str().unwrap().to_owned());
    }
    evento(
        &app,
        org,
        &cookie,
        &ids[3],
        json!({"tipo": "paga", "data": "2026-10-06"}),
    )
    .await;
    let base = format!("/api/v1/orgs/{org}/notas");
    let numeros = |json: &Value| -> Vec<String> {
        json["dados"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["numero"].as_str().unwrap().to_owned())
            .collect()
    };
    let (status, json) = enviar(&app, Method::GET, &cookie, &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(numeros(&json), ["A1", "B2", "A2", "B1"]);
    assert_eq!(json["meta"]["total"], 4);
    let atraso: Vec<i64> = json["dados"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["dias_atraso"].as_i64().unwrap())
        .collect();
    assert_eq!(atraso, [19, 0, 0, 0]);
    for (query, esperado) in [
        ("situacao=vencida", vec!["A1"]),
        ("situacao=a_vencer", vec!["A2", "B1"]),
        ("situacao=vence_7d", vec!["A2"]),
        ("status=paga", vec!["B2"]),
        ("status=emitida,paga", vec!["A1", "B2", "A2", "B1"]),
        (&format!("empenho_id={outro}"), vec!["B2", "B1"]),
        ("busca=hospital", vec!["B2", "B1"]),
        ("busca=a2", vec!["A2"]),
        ("ordenar=criado_em", vec!["B2", "B1", "A2", "A1"]),
        ("por_pagina=2&pagina=2", vec!["A2", "B1"]),
    ] {
        let (status, json) =
            enviar(&app, Method::GET, &cookie, &format!("{base}?{query}"), None).await;
        assert_eq!(status, StatusCode::OK, "{query}: {json}");
        assert_eq!(numeros(&json), esperado, "{query}");
    }
    for query in [
        "status=aberta",
        "situacao=x",
        "ordenar=valor",
        "empenho_id=1",
    ] {
        let (status, _) =
            enviar(&app, Method::GET, &cookie, &format!("{base}?{query}"), None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
    }
    // Avança o relógio: atraso acompanha `hoje`.
    app.fixar_hoje(data(2026, 10, 30));
    let (_, json) = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("{base}/{}", ids[0]),
        None,
    )
    .await;
    assert_eq!(json["dados"]["dias_atraso"], 29);
    let (_, json) = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("{base}?situacao=vencida"),
        None,
    )
    .await;
    assert_eq!(numeros(&json), ["A1", "A2"]);
    // Leitor lê; org B não vê nada.
    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let (status, _) = enviar(&app, Method::GET, &leitor, &base, None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let (_, json) = enviar(
        &app,
        Method::GET,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/notas"),
        None,
    )
    .await;
    assert_eq!(json["meta"]["total"], 0);
    let (status, _) = evento(
        &app,
        org_b,
        &cookie_b,
        &ids[0],
        json!({"tipo": "cancelada", "data": "2026-10-20"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.encerrar().await;
}
