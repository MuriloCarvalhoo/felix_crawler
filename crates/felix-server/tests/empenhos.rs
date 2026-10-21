//! Empenhos: análise de PDFs reais, CRUD, filtros, anulação, limites e isolamento.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod comum;

use axum::http::{Method, StatusCode};
use chrono::NaiveDate;
use comum::TestApp;
use felix_server::http::Papel;
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

const CNPJ: &str = "19116488000145";

fn data(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

async fn criar_empresa(app: &TestApp, org: Uuid, cnpj: &str) -> Uuid {
    let mut tx = app.tenant(org).await;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(org)
    .bind("Fornecedora Teste Ltda")
    .bind(cnpj)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn post(app: &TestApp, cookie: &str, caminho: &str, corpo: Value) -> (StatusCode, Value) {
    let r = app
        .req(Method::POST, caminho)
        .cookie(cookie)
        .json(corpo)
        .enviar()
        .await;
    (r.status, r.json)
}

async fn get(app: &TestApp, cookie: &str, caminho: &str) -> (StatusCode, Value) {
    let r = app.req(Method::GET, caminho).cookie(cookie).enviar().await;
    (r.status, r.json)
}

async fn patch(app: &TestApp, cookie: &str, caminho: &str, corpo: Value) -> (StatusCode, Value) {
    let r = app
        .req(Method::PATCH, caminho)
        .cookie(cookie)
        .json(corpo)
        .enviar()
        .await;
    (r.status, r.json)
}

async fn criar_empenho(
    app: &TestApp,
    org: Uuid,
    cookie: &str,
    empresa: Uuid,
    numero: &str,
    valor: i64,
    extra: Value,
) -> Value {
    let mut corpo = json!({
        "empresa_id": empresa, "ug": "160203", "numero": numero,
        "valor_total_centavos": valor, "ug_nome": "Base Administrativa",
        "orgao": "Comando do Exército", "modalidade": "Pregão", "numero_edital_ata": "90012/2025"
    });
    for (k, v) in extra.as_object().unwrap() {
        corpo[k] = v.clone();
    }
    let (status, json) = post(app, cookie, &format!("/api/v1/orgs/{org}/empenhos"), corpo).await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    json["dados"].clone()
}

async fn criar_nota(
    app: &TestApp,
    org: Uuid,
    cookie: &str,
    empenho: &str,
    numero: &str,
    valor: i64,
    emissao: &str,
) -> (StatusCode, Value) {
    post(
        app,
        cookie,
        &format!("/api/v1/orgs/{org}/empenhos/{empenho}/notas"),
        json!({"numero": numero, "tipo": "produto", "valor_centavos": valor, "data_emissao": emissao}),
    )
    .await
}

fn pdf_real(numero: &str) -> (String, Vec<u8>) {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/empenho");
    let caminho = std::fs::read_dir(&raiz)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_string_lossy().contains(numero))
        .unwrap();
    let nome = caminho.file_name().unwrap().to_string_lossy().into_owned();
    (nome, std::fs::read(caminho).unwrap())
}

#[tokio::test]
async fn analisa_pdfs_reais_com_sugestao_de_empresa() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let empresa = criar_empresa(&app, org, CNPJ).await;
    criar_empenho(
        &app,
        org,
        &cookie,
        empresa,
        "2026NE000290",
        3_245_000,
        json!({}),
    )
    .await;
    let (n1, b1) = pdf_real("2026NE000290");
    let (n2, b2) = pdf_real("2026NE000291");
    let caminho = format!("/api/v1/orgs/{org}/empenhos/analisar");
    let (status, _, json) = app
        .multipart(&caminho, Some(&cookie), &[(&n1, &b1), (&n2, &b2)])
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let itens = json["dados"].as_array().unwrap();
    assert_eq!(itens.len(), 2);
    for item in itens {
        assert_eq!(item["analise"]["dados"]["cnpj_fornecedor"], CNPJ);
        assert_eq!(item["empresa_id_sugerida"], json!(empresa));
    }
    assert_eq!(itens[0]["analise"]["dados"]["empenho"], "2026NE000290");
    assert_eq!(itens[0]["ja_cadastrado"], true);
    assert_eq!(itens[1]["ja_cadastrado"], false);

    // Outra organização sem a empresa: sem sugestão; leitor não analisa.
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let (status, _, json) = app
        .multipart(
            &format!("/api/v1/orgs/{org_b}/empenhos/analisar"),
            Some(&cookie_b),
            &[(&n1, &b1)],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["dados"][0].get("empresa_id_sugerida").is_none());
    assert_eq!(json["dados"][0]["ja_cadastrado"], false);
    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let (status, _, _) = app.multipart(&caminho, Some(&leitor), &[(&n1, &b1)]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    app.encerrar().await;
}

#[tokio::test]
async fn cria_valida_e_filtra_empenhos() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (_, org, cookie) = app.dono_com_org().await;
    let empresa = criar_empresa(&app, org, CNPJ).await;
    let empresa2 = criar_empresa(&app, org, "11222333000181").await;
    let base = format!("/api/v1/orgs/{org}/empenhos");

    // Validações.
    let (status, json) = post(
        &app,
        &cookie,
        &base,
        json!({"empresa_id": empresa, "ug": "123", "numero": " ", "valor_total_centavos": 0,
               "gestao": "12", "prazo_dias": 0, "regra_vencimento": "x", "origem": "legado",
               "data_emissao": "2030-01-01"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    for campo in [
        "ug",
        "numero",
        "valor_total_centavos",
        "gestao",
        "prazo_dias",
        "regra_vencimento",
        "origem",
        "data_emissao",
    ] {
        assert!(
            json["erro"]["campos"].get(campo).is_some(),
            "{campo}: {json}"
        );
    }
    let (_, org_b, _) = app.dono_com_org().await;
    let empresa_b = criar_empresa(&app, org_b, CNPJ).await;
    let (status, json) = post(
        &app,
        &cookie,
        &base,
        json!({"empresa_id": empresa_b, "ug": "160203", "numero": "X", "valor_total_centavos": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(json["erro"]["campos"].get("empresa_id").is_some());

    // Criação e duplicado (número normalizado em maiúsculas).
    let e1 = criar_empenho(
        &app,
        org,
        &cookie,
        empresa,
        "2026ne000290",
        3_245_000,
        json!({"gestao": "00001", "data_emissao": "2026-06-15", "origem": "pdf"}),
    )
    .await;
    assert_eq!(e1["numero"], "2026NE000290");
    assert_eq!(e1["empresa_cnpj"], CNPJ);
    assert_eq!(e1["saldo_centavos"], 3_245_000);
    assert_eq!(e1["regra_vencimento"], "corridos_emissao");
    assert_eq!(e1["prazo_dias"], 30);
    assert_eq!(e1["origem"], "pdf");
    let (status, json) = post(
        &app,
        &cookie,
        &base,
        json!({"empresa_id": empresa, "ug": "160203", "numero": "2026NE000290", "valor_total_centavos": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    let e2 = criar_empenho(
        &app,
        org,
        &cookie,
        empresa2,
        "2026NE000291",
        100_000,
        json!({"ug": "160100", "ug_nome": "Hospital Militar", "numero_edital_ata": "777/2026"}),
    )
    .await;
    let e3 = criar_empenho(
        &app,
        org,
        &cookie,
        empresa,
        "2026NE000292",
        50_000,
        json!({}),
    )
    .await;
    // Fatura todo o e3 para testar com_saldo.
    let (status, _) = criar_nota(
        &app,
        org,
        &cookie,
        e3["id"].as_str().unwrap(),
        "1",
        50_000,
        "2026-10-01",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let ids = |json: &Value| -> Vec<String> {
        json["dados"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["numero"].as_str().unwrap().to_owned())
            .collect()
    };
    let (status, json) = get(&app, &cookie, &base).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["meta"]["total"], 3);
    assert_eq!(ids(&json), ["2026NE000292", "2026NE000291", "2026NE000290"]);
    let e3_lista = &json["dados"][0];
    assert_eq!(e3_lista["faturado_centavos"], 50_000);
    assert_eq!(e3_lista["saldo_centavos"], 0);

    for (query, esperado) in [
        ("busca=ne000291", vec!["2026NE000291"]),
        ("busca=hospital", vec!["2026NE000291"]),
        ("busca=777", vec!["2026NE000291"]),
        (
            "busca=ex%C3%A9rcito",
            vec!["2026NE000292", "2026NE000291", "2026NE000290"],
        ),
        ("ug=160100", vec!["2026NE000291"]),
        (&format!("empresa_id={empresa2}"), vec!["2026NE000291"]),
        ("com_saldo=true", vec!["2026NE000291", "2026NE000290"]),
        ("com_saldo=false", vec!["2026NE000292"]),
        ("status=anulado", vec![]),
        ("status=todos&por_pagina=1&pagina=2", vec!["2026NE000291"]),
    ] {
        let (status, json) = get(&app, &cookie, &format!("{base}?{query}")).await;
        assert_eq!(status, StatusCode::OK, "{query}: {json}");
        assert_eq!(ids(&json), esperado, "{query}");
    }
    let (_, json) = get(&app, &cookie, &format!("{base}?por_pagina=1&pagina=2")).await;
    assert_eq!(
        json["meta"],
        json!({"total": 3, "pagina": 2, "por_pagina": 1})
    );
    for query in ["status=x", "ug=12", "com_saldo=talvez", "empresa_id=abc"] {
        let (status, _) = get(&app, &cookie, &format!("{base}?{query}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
    }

    // Detalhe com notas.
    let (status, json) = get(
        &app,
        &cookie,
        &format!("{base}/{}", e3["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dados"]["notas"].as_array().unwrap().len(), 1);
    assert_eq!(json["dados"]["notas"][0]["vencimento"], "2026-10-31");

    // PATCH: campos, valor abaixo do faturado e duplicado.
    let caminho_e3 = format!("{base}/{}", e3["id"].as_str().unwrap());
    let (status, json) = patch(
        &app,
        &cookie,
        &caminho_e3,
        json!({"ug_nome": "Nova UG", "observacao": "obs", "orgao": null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["ug_nome"], "Nova UG");
    assert_eq!(json["dados"]["observacao"], "obs");
    assert!(json["dados"]["orgao"].is_null());
    assert_eq!(json["dados"]["modalidade"], "Pregão");
    let (status, _) = patch(
        &app,
        &cookie,
        &caminho_e3,
        json!({"valor_total_centavos": 49_999}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = patch(
        &app,
        &cookie,
        &caminho_e3,
        json!({"numero": "2026NE000290"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Mudança de regra/prazo recalcula vencimento das notas abertas.
    let (status, json) = patch(&app, &cookie, &caminho_e3, json!({"prazo_dias": 10})).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (_, json) = get(&app, &cookie, &caminho_e3).await;
    assert_eq!(json["dados"]["notas"][0]["vencimento"], "2026-10-11");
    let (status, _) = patch(
        &app,
        &cookie,
        &caminho_e3,
        json!({"regra_vencimento": "in77_uteis"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, json) = get(&app, &cookie, &caminho_e3).await;
    // 01/10/2026 (quinta) + 5 + 5 dias úteis (12/10 feriado) = 16/10/2026.
    assert_eq!(json["dados"]["notas"][0]["vencimento"], "2026-10-16");

    // Papéis e auditoria.
    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let (status, _) = get(&app, &leitor, &base).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post(
        &app,
        &leitor,
        &base,
        json!({"empresa_id": empresa, "ug": "160203", "numero": "Z", "valor_total_centavos": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = patch(&app, &leitor, &caminho_e3, json!({"ug_nome": "x"})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let mut tx = app.tenant(org).await;
    let acoes: Vec<String> =
        sqlx::query_scalar("SELECT acao FROM auditoria WHERE entidade = 'empenho' ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(acoes.iter().filter(|a| *a == "empenho.criar").count(), 3);
    assert!(acoes.iter().any(|a| a == "empenho.editar"));
    drop(tx);
    let _ = e2;
    app.encerrar().await;
}

#[tokio::test]
async fn anulacao_exige_motivo_e_notas_resolvidas() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 10, 20));
    let (_, org, cookie) = app.dono_com_org().await;
    let empresa = criar_empresa(&app, org, CNPJ).await;
    let e = criar_empenho(
        &app,
        org,
        &cookie,
        empresa,
        "2026NE000001",
        100_000,
        json!({}),
    )
    .await;
    let id = e["id"].as_str().unwrap();
    let caminho = format!("/api/v1/orgs/{org}/empenhos/{id}/anular");
    let (status, _) = post(&app, &cookie, &caminho, json!({"motivo": "  "})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (_, nota) = criar_nota(&app, org, &cookie, id, "10", 10_000, "2026-10-01").await;
    let nota_id = nota["dados"]["id"].as_str().unwrap().to_owned();
    let (status, json) = post(&app, &cookie, &caminho, json!({"motivo": "Erro"})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        json["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("Cancele as notas abertas")
    );

    // Nota paga impede anulação.
    let eventos = format!("/api/v1/orgs/{org}/notas/{nota_id}/eventos");
    let (status, _) = post(
        &app,
        &cookie,
        &eventos,
        json!({"tipo": "paga", "data": "2026-10-10"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, json) = post(&app, &cookie, &caminho, json!({"motivo": "Erro"})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(json["erro"]["mensagem"].as_str().unwrap().contains("paga"));

    // Reabre (dono) e cancela: agora anula.
    let (status, _) = post(
        &app,
        &cookie,
        &eventos,
        json!({"tipo": "reaberta", "data": "2026-10-11"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post(
        &app,
        &cookie,
        &eventos,
        json!({"tipo": "cancelada", "data": "2026-10-11"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, json) = post(
        &app,
        &cookie,
        &caminho,
        json!({"motivo": "Empenho emitido em duplicidade"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["status"], "anulado");
    assert_eq!(
        json["dados"]["motivo_anulacao"],
        "Empenho emitido em duplicidade"
    );
    assert!(json["dados"]["anulado_em"].is_string());
    let (status, _) = post(&app, &cookie, &caminho, json!({"motivo": "de novo"})).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Anulado: some da lista padrão, não aceita notas, edição nem reabertura de nota.
    let (_, json) = get(&app, &cookie, &format!("/api/v1/orgs/{org}/empenhos")).await;
    assert_eq!(json["meta"]["total"], 0);
    let (_, json) = get(
        &app,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos?status=anulado"),
    )
    .await;
    assert_eq!(json["meta"]["total"], 1);
    let (status, _) = criar_nota(&app, org, &cookie, id, "11", 1_000, "2026-10-01").await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = patch(
        &app,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos/{id}"),
        json!({"ug_nome": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = post(
        &app,
        &cookie,
        &eventos,
        json!({"tipo": "reaberta", "data": "2026-10-12"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    app.encerrar().await;
}

#[tokio::test]
async fn limite_do_plano_e_isolamento() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let empresa = criar_empresa(&app, org, CNPJ).await;
    let e = criar_empenho(
        &app,
        org,
        &cookie,
        empresa,
        "2026NE000001",
        100_000,
        json!({}),
    )
    .await;
    let id = e["id"].as_str().unwrap();

    // Org B não enxerga o empenho da org A (nem pelo próprio caminho).
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    for caminho in [
        format!("/api/v1/orgs/{org_b}/empenhos/{id}"),
        format!("/api/v1/orgs/{org}/empenhos/{id}"),
        format!("/api/v1/orgs/{org}/empenhos"),
    ] {
        let (status, _) = get(&app, &cookie_b, &caminho).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{caminho}");
    }
    let (status, _) = patch(
        &app,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/empenhos/{id}"),
        json!({"ug_nome": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(
        &app,
        &cookie_b,
        &format!("/api/v1/orgs/{org_b}/empenhos/{id}/anular"),
        json!({"motivo": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, json) = get(&app, &cookie_b, &format!("/api/v1/orgs/{org_b}/empenhos")).await;
    assert_eq!(json["meta"]["total"], 0);

    // Plano gratuito: 30 empenhos ativos → 402.
    sqlx::query(
        "UPDATE assinaturas SET plano = 'gratuito', status = 'ativa' WHERE organizacao_id = $1",
    )
    .bind(org)
    .execute(&app.pool)
    .await
    .unwrap();
    let mut tx = app.tenant(org).await;
    sqlx::query(
        "INSERT INTO empenhos (organizacao_id, empresa_id, ug, numero, valor_total_centavos)
         SELECT $1, $2, '160203', 'LOTE' || g, 100 FROM generate_series(1, 29) g",
    )
    .bind(org)
    .bind(empresa)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (status, json) = post(
        &app,
        &cookie,
        &format!("/api/v1/orgs/{org}/empenhos"),
        json!({"empresa_id": empresa, "ug": "160203", "numero": "EXTRA", "valor_total_centavos": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED, "{json}");
    assert_eq!(json["erro"]["codigo"], "limite_plano");
    app.encerrar().await;
}

#[tokio::test]
async fn patch_valida_so_os_campos_enviados() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(2026, 3, 10));
    let (_, org, cookie) = app.dono_com_org().await;
    let empresa = criar_empresa(&app, org, CNPJ).await;
    // Empenho legado/Portal com dados fora do padrão da tela.
    let mut tx = app.tenant(org).await;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO empenhos (organizacao_id, empresa_id, ug, numero, valor_total_centavos,
                               gestao, modalidade, data_emissao, origem)
         VALUES ($1, $2, '160203', '2026ne000290 legado', 1000, '123', repeat('m', 150),
                 '1999-12-31', 'legado') RETURNING id",
    )
    .bind(org)
    .bind(empresa)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let url = format!("/api/v1/orgs/{org}/empenhos/{id}");

    let (status, json) = patch(&app, &cookie, &url, json!({"observacao": "Conferido"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["observacao"], "Conferido");
    // Os campos não enviados ficam como estavam.
    assert_eq!(json["dados"]["gestao"], "123");
    assert_eq!(json["dados"]["numero"], "2026ne000290 legado");
    assert_eq!(json["dados"]["data_emissao"], "1999-12-31");

    // O campo enviado continua validado.
    let (status, json) = patch(&app, &cookie, &url, json!({"gestao": "12"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    assert!(json["erro"]["campos"]["gestao"].is_string(), "{json}");
    assert!(json["erro"]["campos"].get("modalidade").is_none(), "{json}");
    let (status, json) = patch(&app, &cookie, &url, json!({"gestao": "00001"})).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["gestao"], "00001");
    app.encerrar().await;
}

/// Upload multipart que envia o começo do arquivo e para (cliente lento/travado).
async fn upload_travado(app: &TestApp) -> (StatusCode, Value, std::time::Duration) {
    use axum::body::{Body, Bytes};
    use futures_util::StreamExt;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let inicio = "--fronteira\r\nContent-Disposition: form-data; name=\"arquivos\"; \
                  filename=\"a.pdf\"\r\nContent-Type: application/pdf\r\n\r\n%PDF-1.4 parcial";
    let fluxo = futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from(inicio))])
        .chain(futures_util::stream::pending());
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/v1/teste/upload")
        .header(axum::http::header::HOST, comum::HOST_TESTE)
        .header("x-felix", "1")
        .header(
            axum::http::header::CONTENT_TYPE,
            "multipart/form-data; boundary=fronteira",
        )
        .body(Body::from_stream(fluxo))
        .unwrap();
    let comeco = std::time::Instant::now();
    let resposta = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        app.router.clone().oneshot(request),
    )
    .await
    .unwrap_or_else(|_| panic!("o servidor deve encerrar o upload parado"))
    .unwrap();
    let status = resposta.status();
    let corpo = resposta.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&corpo).unwrap(),
        comeco.elapsed(),
    )
}

#[tokio::test]
async fn upload_parado_expira_por_inatividade() {
    use axum::{
        Json, Router,
        extract::{Multipart, State},
        routing::post,
    };
    use felix_server::{app::AppState, http::ErroApi, pdf};
    async fn receber(
        State(estado): State<AppState>,
        multipart: Multipart,
    ) -> Result<Json<usize>, ErroApi> {
        let prazos = pdf::PrazosUpload {
            inatividade: std::time::Duration::from_millis(300),
            total: std::time::Duration::from_secs(5),
        };
        let r = pdf::receber_e_analisar_com_prazos(&estado, multipart, prazos).await?;
        Ok(Json(r.len()))
    }
    let app = TestApp::com_rotas(Router::new().route("/teste/upload", post(receber))).await;
    let (status, json, tempo) = upload_travado(&app).await;
    assert_eq!(status, StatusCode::REQUEST_TIMEOUT, "{json}");
    assert!(
        json["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("parado"),
        "{json}"
    );
    assert!(tempo < std::time::Duration::from_secs(3), "{tempo:?}");
    // A vaga de admissão foi liberada.
    assert_eq!(
        app.estado.admissao.available_permits(),
        felix_server::app::ANALISES_ADMITIDAS
    );
    app.encerrar().await;
}

#[test]
fn prazos_padrao_do_upload() {
    use felix_server::pdf::PRAZOS_UPLOAD;
    assert_eq!(
        PRAZOS_UPLOAD.inatividade,
        std::time::Duration::from_secs(15)
    );
    assert_eq!(PRAZOS_UPLOAD.total, std::time::Duration::from_secs(30));
}
