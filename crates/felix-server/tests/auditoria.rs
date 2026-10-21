//! Testes da trilha de auditoria: papéis, paginação, filtro e nome do usuário.
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

#[tokio::test]
async fn auditoria_registra_e_lista_mutacoes() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let base = format!("/api/v1/orgs/{org}");
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            &base,
            Some(&cookie),
            Some(json!({"nome": "Org Renomeada"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    for (razao, cnpj) in [("Alfa", "11222333000181"), ("Beta", "12ABC34501DE35")] {
        let (s, _, j) = app
            .requisicao(
                Method::POST,
                &format!("{base}/empresas"),
                Some(&cookie),
                Some(json!({"razao_social": razao, "cnpj": cnpj})),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{j}");
    }
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &format!("{base}/convites"),
            Some(&cookie),
            Some(json!({"email": "c@exemplo.com", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);

    let url = format!("{base}/auditoria");
    let (s, _, j) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["meta"]["total"], 4);
    let primeiro = &j["dados"][0];
    assert_eq!(primeiro["entidade"], "convite");
    assert_eq!(primeiro["acao"], "criar");
    assert_eq!(primeiro["usuario_nome"], "Dono");
    assert!(primeiro["id"].is_number() && primeiro["criado_em"].is_string());
    assert_eq!(j["dados"][3]["entidade"], "organizacao");

    let (_, _, j) = app
        .requisicao(
            Method::GET,
            &format!("{url}?entidade=empresa&por_pagina=1&pagina=2"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(j["meta"]["total"], 2);
    assert_eq!(j["meta"]["pagina"], 2);
    assert_eq!(j["dados"].as_array().unwrap().len(), 1);
    assert_eq!(j["dados"][0]["dados"]["razao_social"], "Alfa");

    // Operador não vê; admin vê.
    let (_, op) = app.membro(org, Papel::Operador).await;
    let (s, _, _) = app.requisicao(Method::GET, &url, Some(&op), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (_, admin) = app.membro(org, Papel::Admin).await;
    let (s, _, _) = app.requisicao(Method::GET, &url, Some(&admin), None).await;
    assert_eq!(s, StatusCode::OK);
    app.encerrar().await;
}

#[tokio::test]
async fn auditoria_e_imutavel_para_o_papel_da_aplicacao() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            &format!("/api/v1/orgs/{org}"),
            Some(&cookie),
            Some(json!({"nome": "Outro nome"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    for sql in [
        "UPDATE auditoria SET acao = 'adulterada'",
        "DELETE FROM auditoria",
        "UPDATE eventos_autenticacao SET tipo = 'logout'",
    ] {
        let mut tx = app.tenant(org).await;
        let erro = sqlx::query(sql).execute(&mut *tx).await.unwrap_err();
        let codigo = erro
            .as_database_error()
            .and_then(|d| d.code())
            .map(|c| c.into_owned());
        assert_eq!(codigo.as_deref(), Some("42501"), "{sql}: {erro}");
    }
    let mut tx = app.tenant(org).await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM auditoria WHERE acao = 'atualizar'")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 1);
    drop(tx);
    app.encerrar().await;
}
