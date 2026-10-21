//! Cobranças: cálculo de encargos, minuta de ofício, registro e texto guardado.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod comum;

use axum::http::{Method, StatusCode, header};
use chrono::NaiveDate;
use comum::{Resposta, TestApp};
use felix_server::http::Papel;
use serde_json::{Value, json};
use uuid::Uuid;

fn data(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

async fn enviar(
    app: &TestApp,
    metodo: Method,
    cookie: &str,
    caminho: &str,
    corpo: Option<Value>,
) -> Resposta {
    let mut req = app.req(metodo, caminho).cookie(cookie);
    if let Some(corpo) = corpo {
        req = req.json(corpo);
    }
    req.enviar().await
}

/// Org com empresa, empenho (corridos 30 dias) e NF de R$ 1.000,00 emitida em 01/09/2026
/// (vencimento 01/10/2026). Devolve (org, cookie do dono, id da nota).
async fn cenario(app: &TestApp, numero_nf: &str) -> (Uuid, String, String) {
    let (_, org, cookie) = app.dono_com_org().await;
    let mut tx = app.tenant(org).await;
    let empresa: Uuid = sqlx::query_scalar(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj)
         VALUES ($1, 'Fornecedora Exemplo Ltda', '19116488000145') RETURNING id",
    )
    .bind(org)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let r = enviar(
        app,
        Method::POST,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos"),
        Some(json!({"empresa_id": empresa, "ug": "160203", "ug_nome": "Base Administrativa do Exército",
                    "orgao": "Comando do Exército", "numero": "2026NE000290",
                    "modalidade": "Pregão", "numero_edital_ata": "90012/2025",
                    "valor_total_centavos": 3_245_000})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    let empenho = r.json["dados"]["id"].as_str().unwrap().to_owned();
    let r = enviar(
        app,
        Method::POST,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos/{empenho}/notas"),
        Some(json!({"numero": numero_nf, "tipo": "produto", "valor_centavos": 100_000, "data_emissao": "2026-09-01"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    (
        org,
        cookie,
        r.json["dados"]["id"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn calculo_e_minuta_de_oficio() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (org, cookie, nota) = cenario(&app, "NF 1/2").await;
    let base = format!("/api/v1/orgs/{org}/notas/{nota}");

    let r = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("{base}/calculo?multa_bps=200&juros_bps=100"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(
        r.json["dados"],
        json!({"vencimento": "2026-10-01", "dias_atraso": 19, "principal_centavos": 100_000,
               "multa_centavos": 2_000, "juros_centavos": 633, "total_centavos": 102_633})
    );
    let r = enviar(&app, Method::GET, &cookie, &format!("{base}/calculo"), None).await;
    assert_eq!(r.json["dados"]["total_centavos"], 100_000);
    let r = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("{base}/calculo?data_referencia=2026-10-01&multa_bps=200"),
        None,
    )
    .await;
    assert_eq!(r.json["dados"]["dias_atraso"], 0);
    assert_eq!(r.json["dados"]["multa_centavos"], 0);
    for query in [
        "multa_bps=10001",
        "juros_bps=-1",
        "data_referencia=2036-10-20",
        "data_referencia=x",
    ] {
        let r = enviar(
            &app,
            Method::GET,
            &cookie,
            &format!("{base}/calculo?{query}"),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
    }
    let r = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("{base}/calculo?data_referencia=2036-10-17"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);

    // Minuta (leitor pode baixar).
    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let r = enviar(
        &app,
        Method::GET,
        &leitor,
        &format!("{base}/oficio?multa_bps=200&juros_bps=100"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(
        r.headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    assert_eq!(
        r.headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"Oficio_NF_1_2.txt\""
    );
    let texto = r.texto();
    for trecho in [
        "MINUTA",
        "Fornecedora Exemplo Ltda",
        "19.116.488/0001-45",
        "Base Administrativa do Exército — UG 160203",
        "Comando do Exército",
        "Nota de Empenho nº 2026NE000290",
        "Pregão nº 90012/2025",
        "NF nº NF 1/2",
        "01/09/2026",
        "01/10/2026",
        "19 dia(s) de atraso",
        "R$ 1.000,00",
        "Multa (2,00%)",
        "R$ 20,00",
        "R$ 6,33",
        "R$ 1.026,33",
        "20 de outubro de 2026",
        "art. 141",
        "art. 92, V",
        "posição desta nota fiscal na ordem cronológica",
        "estágio atual da despesa",
        "data da liquidação",
        "previsão de pagamento",
        "não presume irregularidade",
    ] {
        assert!(texto.contains(trecho), "faltou {trecho:?} em:\n{texto}");
    }
    // A minuta não registra cobrança.
    let r = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("/api/v1/orgs/{org}/cobrancas"),
        None,
    )
    .await;
    assert_eq!(r.json["meta"]["total"], 0);

    // Outra organização → 404.
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    for sufixo in ["calculo", "oficio"] {
        let r = enviar(
            &app,
            Method::GET,
            &cookie_b,
            &format!("/api/v1/orgs/{org_b}/notas/{nota}/{sufixo}"),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{sufixo}");
    }
    app.encerrar().await;
}

#[tokio::test]
async fn registra_cobranca_e_baixa_texto_guardado() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (org, cookie, nota) = cenario(&app, "123").await;
    let caminho = format!("/api/v1/orgs/{org}/notas/{nota}/cobrancas");
    let corpo = json!({"multa_bps": 200, "juros_bps": 100, "canal": "sei", "protocolo": "00000.000123/2026-11",
                       "enviada_em": "2026-10-20", "observacao": "Enviado pelo SEI"});

    // Validações e papéis.
    let r = enviar(
        &app,
        Method::POST,
        &cookie,
        &caminho,
        Some(json!({"canal": "fax", "enviada_em": "2026-10-21", "multa_bps": 20000})),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    for campo in ["canal", "enviada_em", "multa_bps"] {
        assert!(r.json["erro"]["campos"].get(campo).is_some(), "{campo}");
    }
    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let r = enviar(&app, Method::POST, &leitor, &caminho, Some(corpo.clone())).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    let r = enviar(&app, Method::POST, &cookie, &caminho, Some(corpo.clone())).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    let c = &r.json["dados"];
    assert_eq!(c["canal"], "sei");
    assert_eq!(c["protocolo"], "00000.000123/2026-11");
    assert_eq!(c["nota_numero"], "123");
    assert_eq!(c["empenho_numero"], "2026NE000290");
    assert_eq!(c["ug"], "160203");
    assert_eq!(c["dias_atraso"], 19);
    assert_eq!(c["principal_centavos"], 100_000);
    assert_eq!(c["multa_centavos"], 2_000);
    assert_eq!(c["juros_centavos"], 633);
    assert_eq!(c["total_centavos"], 102_633);
    assert_eq!(c["multa_bps"], 200);
    assert_eq!(c["criado_por_nome"], "Dono");
    let texto = c["texto"].as_str().unwrap().to_owned();
    assert!(texto.contains("R$ 1.026,33"));
    let cobranca = c["id"].as_str().unwrap().to_owned();

    // Lista, detalhe da nota e texto guardado (independe de mudanças posteriores).
    let r = enviar(
        &app,
        Method::GET,
        &leitor,
        &format!("/api/v1/orgs/{org}/cobrancas?nota_id={nota}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["meta"]["total"], 1);
    assert_eq!(r.json["dados"][0]["id"], cobranca);
    assert!(r.json["dados"][0].get("texto").is_none());
    let r = enviar(
        &app,
        Method::GET,
        &leitor,
        &format!("/api/v1/orgs/{org}/cobrancas?nota_id={}", Uuid::now_v7()),
        None,
    )
    .await;
    assert_eq!(r.json["meta"]["total"], 0);
    let r = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("/api/v1/orgs/{org}/notas/{nota}"),
        None,
    )
    .await;
    assert_eq!(r.json["dados"]["cobrancas"][0]["id"], cobranca);
    app.fixar_hoje(data(2026, 11, 20));
    let r = enviar(
        &app,
        Method::GET,
        &leitor,
        &format!("/api/v1/orgs/{org}/cobrancas/{cobranca}/oficio"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        r.headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"Oficio_123.txt\""
    );
    assert_eq!(r.texto(), texto);

    // Nota paga não recebe cobrança.
    let r = enviar(
        &app,
        Method::POST,
        &cookie,
        &format!("/api/v1/orgs/{org}/notas/{nota}/eventos"),
        Some(json!({"tipo": "paga", "data": "2026-11-01"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let r = enviar(&app, Method::POST, &cookie, &caminho, Some(corpo.clone())).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    // Paga: cálculo padrão usa a data do pagamento (31 dias de atraso).
    let r = enviar(
        &app,
        Method::GET,
        &cookie,
        &format!("/api/v1/orgs/{org}/notas/{nota}/calculo"),
        None,
    )
    .await;
    assert_eq!(r.json["dados"]["dias_atraso"], 31);

    // Isolamento.
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let r = enviar(
        &app,
        Method::GET,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/cobrancas/{cobranca}/oficio"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = enviar(
        &app,
        Method::GET,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/cobrancas"),
        None,
    )
    .await;
    assert_eq!(r.json["meta"]["total"], 0);
    let r = enviar(
        &app,
        Method::POST,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/notas/{nota}/cobrancas"),
        Some(corpo),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    app.encerrar().await;
}
