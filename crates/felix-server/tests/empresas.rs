//! Testes de empresas (CNPJs): CRUD, validação, papéis, limites e exclusão com empenhos.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

mod comum;

use axum::http::{Method, StatusCode};
use comum::TestApp;
use felix_server::http::Papel;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn crud_de_empresas() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let url = format!("/api/v1/orgs/{org}/empresas");

    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"razao_social": "  Alfa Ltda ", "cnpj": "11.222.333/0001-81"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert_eq!(j["dados"]["razao_social"], "Alfa Ltda");
    assert_eq!(j["dados"]["cnpj"], "11222333000181");
    assert!(j["dados"]["criado_em"].is_string());
    let id = j["dados"]["id"].as_str().unwrap().to_owned();

    // CNPJ alfanumérico (IN RFB 2.229/2024).
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"razao_social": "Beta SA", "cnpj": "12.abc.345/01de-35"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert_eq!(j["dados"]["cnpj"], "12ABC34501DE35");

    // Duplicado (sem máscara) → 409.
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"razao_social": "Outra", "cnpj": "11222333000181"})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{j}");

    // Validação.
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"razao_social": "", "cnpj": "11.222.333/0001-82"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(j["erro"]["campos"]["cnpj"].is_string());
    assert!(j["erro"]["campos"]["razao_social"].is_string());

    let (s, _, j) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
    assert_eq!(s, StatusCode::OK);
    let lista = j["dados"].as_array().unwrap();
    assert_eq!(lista.len(), 2);
    assert_eq!(lista[0]["razao_social"], "Alfa Ltda");

    let (s, _, j) = app
        .requisicao(
            Method::PATCH,
            &format!("{url}/{id}"),
            Some(&cookie),
            Some(json!({"razao_social": "Alfa Comércio Ltda"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["dados"]["razao_social"], "Alfa Comércio Ltda");
    assert_eq!(j["dados"]["cnpj"], "11222333000181");

    let (s, _, _) = app
        .requisicao(Method::DELETE, &format!("{url}/{id}"), Some(&cookie), None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = app
        .requisicao(Method::DELETE, &format!("{url}/{id}"), Some(&cookie), None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            &format!("{url}/{}", Uuid::now_v7()),
            Some(&cookie),
            Some(json!({"razao_social": "x"})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let mut tx = app.tenant(org).await;
    let acoes: Vec<String> =
        sqlx::query_scalar("SELECT acao FROM auditoria WHERE entidade = 'empresa' ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(acoes, vec!["criar", "criar", "atualizar", "excluir"]);
    drop(tx);
    app.encerrar().await;
}

#[tokio::test]
async fn papeis_em_empresas() {
    let app = TestApp::novo().await;
    let (_, org, _) = app.dono_com_org().await;
    let url = format!("/api/v1/orgs/{org}/empresas");
    for papel in [Papel::Leitor, Papel::Operador] {
        let (_, cookie) = app.membro(org, papel).await;
        let (s, _, _) = app
            .requisicao(
                Method::POST,
                &url,
                Some(&cookie),
                Some(json!({"razao_social": "X", "cnpj": "11222333000181"})),
            )
            .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{papel}");
        let (s, _, _) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
        assert_eq!(s, StatusCode::OK);
    }
    let (_, admin) = app.membro(org, Papel::Admin).await;
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&admin),
            Some(json!({"razao_social": "X", "cnpj": "11222333000181"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    app.encerrar().await;
}

#[tokio::test]
async fn excluir_empresa_com_empenhos_e_409() {
    let app = TestApp::novo().await;
    let (usuario, org, cookie) = app.dono_com_org().await;
    let url = format!("/api/v1/orgs/{org}/empresas");
    let (_, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"razao_social": "Alfa", "cnpj": "11222333000181"})),
        )
        .await;
    let id: Uuid = j["dados"]["id"].as_str().unwrap().parse().unwrap();
    let mut tx = app.tenant(org).await;
    sqlx::query(
        "INSERT INTO empenhos (organizacao_id, empresa_id, ug, numero, valor_total_centavos, criado_por)
         VALUES ($1, $2, '160203', '2026NE000001', 1000, $3)",
    )
    .bind(org)
    .bind(id)
    .bind(usuario)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (s, _, j) = app
        .requisicao(Method::DELETE, &format!("{url}/{id}"), Some(&cookie), None)
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(j["erro"]["mensagem"], "Empresa possui empenhos.");
    app.encerrar().await;
}

#[tokio::test]
async fn limite_de_cnpjs_no_plano_gratuito() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    sqlx::query("UPDATE assinaturas SET plano = 'gratuito', status = 'ativa', teste_ate = NULL")
        .execute(&app.pool)
        .await
        .unwrap();
    let url = format!("/api/v1/orgs/{org}/empresas");
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"razao_social": "Alfa", "cnpj": "11222333000181"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"razao_social": "Beta", "cnpj": "12.ABC.345/01DE-35"})),
        )
        .await;
    assert_eq!(s, StatusCode::PAYMENT_REQUIRED, "{j}");
    assert_eq!(j["erro"]["codigo"], "limite_plano");
    assert!(j["erro"]["mensagem"].as_str().unwrap().contains("Gratuito"));
    app.encerrar().await;
}
