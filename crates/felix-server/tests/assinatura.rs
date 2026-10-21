//! Testes de assinatura: GET/POST do plano, limites de usuários e funções públicas.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

mod comum;

use axum::http::{Method, StatusCode};
use chrono::TimeDelta;
use comum::TestApp;
use felix_server::{
    email,
    http::Papel,
    modulos::assinatura::{Plano, Recurso, plano_efetivo, verificar_limite},
};
use serde_json::json;

#[tokio::test]
async fn obter_e_trocar_plano() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let url = format!("/api/v1/orgs/{org}/assinatura");
    let (s, _, j) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
    assert_eq!(s, StatusCode::OK, "{j}");
    let a = &j["dados"];
    assert_eq!(a["plano"], "profissional");
    assert_eq!(a["status"], "teste");
    assert_eq!(
        a["limites"],
        json!({"cnpjs": 5, "empenhos_ativos": null, "usuarios": 10, "transparencia": true, "alertas_email": true})
    );
    assert_eq!(
        a["uso"],
        json!({"cnpjs": 0, "empenhos_ativos": 0, "usuarios": 1})
    );
    assert_eq!(a["troca_paga_disponivel"], true);

    // Admin não troca plano; plano inválido → 422.
    let (_, admin) = app.membro(org, Papel::Admin).await;
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&admin),
            Some(json!({"plano": "empresarial"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"plano": "ouro"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"plano": "empresarial"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["dados"]["plano"], "empresarial");
    assert_eq!(j["dados"]["status"], "ativa");
    assert_eq!(j["dados"]["teste_ate"], json!(null));
    assert_eq!(j["dados"]["limites"]["cnpjs"], json!(null));
    assert_eq!(j["dados"]["uso"]["usuarios"], 2);

    // Leitor vê a assinatura.
    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let (s, _, j) = app.requisicao(Method::GET, &url, Some(&leitor), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(j["dados"]["plano"], "empresarial");

    let mut tx = app.tenant(org).await;
    let dados: serde_json::Value = sqlx::query_scalar(
        "SELECT dados FROM auditoria WHERE entidade = 'assinatura' AND acao = 'trocar_plano'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(dados["plano"], "empresarial");
    assert_eq!(dados["plano_anterior"], "profissional");
    drop(tx);
    app.encerrar().await;
}

#[tokio::test]
async fn teste_expirado_vira_gratuito() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let mut conn = app.pool.acquire().await.unwrap();
    let agora = app.estado.agora();
    assert_eq!(
        plano_efetivo(&mut conn, org, agora).await.unwrap(),
        Plano::Profissional
    );
    let depois = agora + TimeDelta::days(15);
    assert_eq!(
        plano_efetivo(&mut conn, org, depois).await.unwrap(),
        Plano::Gratuito
    );
    drop(conn);
    app.relogio.definir(depois);
    let (_, _, j) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/orgs/{org}/assinatura"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(j["dados"]["plano"], "gratuito");
    assert_eq!(j["dados"]["limites"]["usuarios"], 2);
    app.encerrar().await;
}

#[tokio::test]
async fn limite_de_usuarios_conta_convites_pendentes() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let url = format!("/api/v1/orgs/{org}/assinatura");
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"plano": "gratuito"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let convites = format!("/api/v1/orgs/{org}/convites");
    // Gratuito: 2 usuários. Dono + 1 convite pendente = 2.
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &convites,
            Some(&cookie),
            Some(json!({"email": "segundo@exemplo.com", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &convites,
            Some(&cookie),
            Some(json!({"email": "terceiro@exemplo.com", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::PAYMENT_REQUIRED, "{j}");
    assert_eq!(j["erro"]["codigo"], "limite_plano");

    // Funções públicas usadas por outros módulos.
    let mut tx = app.tenant(org).await;
    let erro = verificar_limite(&mut tx, org, Recurso::Usuario, app.estado.agora())
        .await
        .unwrap_err();
    assert_eq!(erro.codigo(), "limite_plano");
    assert!(
        verificar_limite(&mut tx, org, Recurso::Cnpj, app.estado.agora())
            .await
            .is_ok()
    );
    assert!(
        verificar_limite(&mut tx, org, Recurso::EmpenhoAtivo, app.estado.agora())
            .await
            .is_ok()
    );
    let id = email::enfileirar(&mut tx, "alguem@exemplo.com", "Assunto\r\nX", "Corpo")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (status, assunto): (String, String) =
        sqlx::query_as("SELECT status, assunto FROM emails_saida WHERE id = $1")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(status, "registrado");
    assert_eq!(assunto, "AssuntoX");
    app.encerrar().await;
}

/// Router com `FELIX_PLANO_MANUAL=false` (padrão de produção) sobre o mesmo banco.
fn router_sem_plano_manual(app: &TestApp) -> axum::Router {
    let mut config = (*app.estado.config).clone();
    config.plano_manual = false;
    let estado = felix_server::app::AppState::novo(app.pool.clone(), config)
        .unwrap()
        .com_relogio(std::sync::Arc::new(app.relogio.clone()));
    felix_server::app::montar(estado, felix_server::modulos::rotas())
}

async fn trocar_plano(
    router: &axum::Router,
    org: uuid::Uuid,
    cookie: &str,
    plano: &str,
) -> (StatusCode, serde_json::Value) {
    use http_body_util::BodyExt;
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/orgs/{org}/assinatura"))
        .header(axum::http::header::HOST, comum::HOST_TESTE)
        .header(axum::http::header::COOKIE, cookie)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .header("x-felix", "1")
        .body(axum::body::Body::from(
            json!({ "plano": plano }).to_string(),
        ))
        .unwrap();
    let resposta = tower::ServiceExt::oneshot(router.clone(), req)
        .await
        .unwrap();
    let status = resposta.status();
    let corpo = resposta.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&corpo).unwrap())
}

#[tokio::test]
async fn sem_provedor_so_permite_ir_para_o_gratuito() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let router = router_sem_plano_manual(&app);
    for plano in ["profissional", "empresarial"] {
        let (s, j) = trocar_plano(&router, org, &cookie, plano).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{plano}: {j}");
        assert_eq!(
            j["erro"]["mensagem"],
            "Mudança de plano pago depende do provedor de pagamento (em configuração)."
        );
    }
    let (s, j) = trocar_plano(&router, org, &cookie, "gratuito").await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["dados"]["plano"], "gratuito");
    assert_eq!(j["dados"]["troca_paga_disponivel"], false);
    assert_eq!(j["dados"]["status"], "ativa");
    app.encerrar().await;
}

#[tokio::test]
async fn downgrade_exige_uso_dentro_dos_novos_limites() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    for (razao, cnpj) in [("Alfa", "11222333000181"), ("Beta", "12ABC34501DE35")] {
        let (s, _, j) = app
            .requisicao(
                Method::POST,
                &format!("/api/v1/orgs/{org}/empresas"),
                Some(&cookie),
                Some(json!({"razao_social": razao, "cnpj": cnpj})),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{j}");
    }
    app.membro(org, Papel::Leitor).await;
    app.membro(org, Papel::Leitor).await;
    let url = format!("/api/v1/orgs/{org}/assinatura");
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"plano": "gratuito"})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{j}");
    assert_eq!(j["erro"]["codigo"], "conflito");
    let mensagem = j["erro"]["mensagem"].as_str().unwrap();
    assert!(mensagem.contains("Gratuito"), "{mensagem}");
    assert_eq!(j["erro"]["campos"]["cnpjs"], "2 CNPJs (limite 1)");
    assert!(j["erro"]["campos"]["usuarios"].is_string(), "{j}");
    assert!(j["erro"]["campos"].get("empenhos_ativos").is_none());
    // Nada mudou.
    let (_, _, j) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
    assert_eq!(j["dados"]["plano"], "profissional");
    // Mesmo sem provedor, o downgrade continua validando o uso.
    let router = router_sem_plano_manual(&app);
    let (s, _) = trocar_plano(&router, org, &cookie, "gratuito").await;
    assert_eq!(s, StatusCode::CONFLICT);
    app.encerrar().await;
}

#[tokio::test]
async fn emails_enviados_sao_redigidos_e_antigos_purgados() {
    let app = TestApp::novo().await;
    let mut conn = app.pool.acquire().await.unwrap();
    let enviado = email::enfileirar(&mut conn, "a@exemplo.com", "Convite", "token secreto")
        .await
        .unwrap();
    let pendente = email::enfileirar(&mut conn, "b@exemplo.com", "Convite", "token secreto")
        .await
        .unwrap();
    sqlx::query("UPDATE emails_saida SET status = 'enviado', enviado_em = now() WHERE id = $1")
        .bind(enviado)
        .execute(&mut *conn)
        .await
        .unwrap();
    assert!(email::redigir_apos_envio(&mut conn, enviado).await.unwrap());
    assert!(
        !email::redigir_apos_envio(&mut conn, pendente)
            .await
            .unwrap()
    );
    let corpos: Vec<String> = sqlx::query_scalar("SELECT corpo FROM emails_saida ORDER BY para")
        .fetch_all(&mut *conn)
        .await
        .unwrap();
    assert_eq!(corpos, [email::CORPO_REDIGIDO, "token secreto"]);

    sqlx::query("UPDATE emails_saida SET criado_em = now() - interval '31 days' WHERE id = $1")
        .bind(pendente)
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);
    assert_eq!(email::purgar_antigos(&app.pool, 30).await.unwrap(), 1);
    let restantes: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM emails_saida")
        .fetch_all(&app.pool)
        .await
        .unwrap();
    assert_eq!(restantes, [enviado]);
    app.encerrar().await;
}
