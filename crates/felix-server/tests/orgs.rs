//! Testes de organizações: dados, exclusão, membros/papéis, convites e isolamento.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

mod comum;

use axum::http::{Method, StatusCode, header};
use comum::{SENHA_PADRAO, TestApp};
use felix_server::{config::Ambiente, http::Papel};
use serde_json::{Value, json};
use uuid::Uuid;

async fn emails_para(app: &TestApp, para: &str) -> Vec<Value> {
    let (s, _, j) = app
        .requisicao(Method::GET, "/api/v1/dev/emails", None, None)
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    j["dados"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["para"] == para)
        .cloned()
        .collect()
}

fn token_convite(email: &Value) -> String {
    let corpo = email["corpo"].as_str().unwrap();
    let marcador = "http://127.0.0.1:8090/convite/";
    let inicio = corpo.find(marcador).expect("link do convite") + marcador.len();
    corpo[inicio..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

async fn convidar(app: &TestApp, org: Uuid, cookie: &str, email: &str, papel: &str) -> String {
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/orgs/{org}/convites"),
            Some(cookie),
            Some(json!({"email": email, "papel": papel})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert_eq!(j["dados"]["email"], email.to_lowercase());
    let emails = emails_para(app, &email.to_lowercase()).await;
    token_convite(&emails[0])
}

async fn me(app: &TestApp, cookie: &str) -> Value {
    let (s, _, j) = app
        .requisicao(Method::GET, "/api/v1/auth/me", Some(cookie), None)
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    j["dados"].clone()
}

#[tokio::test]
async fn obter_atualizar_e_excluir_org() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let url = format!("/api/v1/orgs/{org}");
    let (s, _, j) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["dados"]["nome"], "Organização de teste");
    assert_eq!(j["dados"]["papel"], "dono");
    assert_eq!(j["dados"]["assinatura"]["plano"], "profissional");
    assert_eq!(j["dados"]["assinatura"]["limites"]["cnpjs"], 5);
    assert_eq!(j["dados"]["assinatura"]["uso"]["usuarios"], 1);
    assert_eq!(j["dados"]["assinatura"]["troca_paga_disponivel"], true);

    let (s, _, j) = app
        .requisicao(
            Method::PATCH,
            &url,
            Some(&cookie),
            Some(json!({"nome": "Nova Razão"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["dados"]["nome"], "Nova Razão");

    // Operador não renomeia; admin não exclui.
    let (_, op) = app.membro(org, Papel::Operador).await;
    let (s, _, _) = app
        .requisicao(Method::PATCH, &url, Some(&op), Some(json!({"nome": "X"})))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (_, admin) = app.membro(org, Papel::Admin).await;
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &url,
            Some(&admin),
            Some(json!({"confirmacao": "Nova Razão"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Confirmação errada.
    let (s, _, j) = app
        .requisicao(
            Method::DELETE,
            &url,
            Some(&cookie),
            Some(json!({"confirmacao": "Outra"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(j["erro"]["campos"]["confirmacao"].is_string());

    let (s, _, j) = app
        .requisicao(
            Method::DELETE,
            &url,
            Some(&cookie),
            Some(json!({"confirmacao": "Nova Razão"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{j}");
    assert_eq!(me(&app, &cookie).await["organizacoes"], json!([]));
    let (s, _, _) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Auditoria da exclusão gravada.
    let mut tx = app.tenant(org).await;
    let acoes: Vec<(String, String)> =
        sqlx::query_as("SELECT acao, entidade FROM auditoria ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert!(
        acoes.contains(&("atualizar".into(), "organizacao".into())),
        "{acoes:?}"
    );
    assert!(
        acoes.contains(&("excluir".into(), "organizacao".into())),
        "{acoes:?}"
    );
    drop(tx);
    app.encerrar().await;
}

#[tokio::test]
async fn papeis_de_membros() {
    let app = TestApp::novo().await;
    let (dono, org, cookie_dono) = app.dono_com_org().await;
    let (admin, cookie_admin) = app.membro(org, Papel::Admin).await;
    let (operador, cookie_op) = app.membro(org, Papel::Operador).await;
    let (leitor, cookie_leitor) = app.membro(org, Papel::Leitor).await;
    let membros = format!("/api/v1/orgs/{org}/membros");

    let (s, _, j) = app
        .requisicao(Method::GET, &membros, Some(&cookie_leitor), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let lista = j["dados"].as_array().unwrap();
    assert_eq!(lista.len(), 4);
    assert_eq!(lista[0]["papel"], "dono");
    assert!(lista[0]["email"].is_string() && lista[0]["desde"].is_string());

    // Operador não convida nem altera papéis.
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/orgs/{org}/convites"),
            Some(&cookie_op),
            Some(json!({"email": "x@exemplo.com", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            &format!("{membros}/{leitor}"),
            Some(&cookie_op),
            Some(json!({"papel": "operador"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Admin promove leitor a admin, mas não a dono, nem mexe no dono.
    let (s, _, j) = app
        .requisicao(
            Method::PATCH,
            &format!("{membros}/{leitor}"),
            Some(&cookie_admin),
            Some(json!({"papel": "admin"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["dados"]["papel"], "admin");
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            &format!("{membros}/{operador}"),
            Some(&cookie_admin),
            Some(json!({"papel": "dono"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            &format!("{membros}/{dono}"),
            Some(&cookie_admin),
            Some(json!({"papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("{membros}/{dono}"),
            Some(&cookie_admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Último dono não se rebaixa nem sai.
    let (s, _, j) = app
        .requisicao(
            Method::PATCH,
            &format!("{membros}/{dono}"),
            Some(&cookie_dono),
            Some(json!({"papel": "admin"})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{j}");
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("{membros}/{dono}"),
            Some(&cookie_dono),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Dono promove admin a dono; aí o primeiro pode sair.
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            &format!("{membros}/{admin}"),
            Some(&cookie_dono),
            Some(json!({"papel": "dono"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("{membros}/{dono}"),
            Some(&cookie_dono),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(me(&app, &cookie_dono).await["organizacoes"], json!([]));

    // Leitor (agora admin) remove operador; operador perde acesso.
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("{membros}/{operador}"),
            Some(&cookie_leitor),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/orgs/{org}"),
            Some(&cookie_op),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Membro inexistente → 404.
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            &format!("{membros}/{}", Uuid::now_v7()),
            Some(&cookie_admin),
            Some(json!({"papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let mut tx = app.tenant(org).await;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM auditoria WHERE entidade = 'membro' AND acao IN ('alterar_papel','sair','remover')",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(n, 4);
    drop(tx);
    app.encerrar().await;
}

#[tokio::test]
async fn membro_comum_pode_sair() {
    let app = TestApp::novo().await;
    let (_, org, _) = app.dono_com_org().await;
    let (leitor, cookie) = app.membro(org, Papel::Leitor).await;
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("/api/v1/orgs/{org}/membros/{leitor}"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    app.encerrar().await;
}

#[tokio::test]
async fn convite_para_usuario_novo() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let token = convidar(&app, org, &cookie, "Novo@Exemplo.com", "operador").await;

    let (s, _, j) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/orgs/{org}/convites"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(j["dados"].as_array().unwrap().len(), 1);
    assert!(j["dados"][0].get("token").is_none());

    // Duplicado.
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/orgs/{org}/convites"),
            Some(&cookie),
            Some(json!({"email": "novo@exemplo.com", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (s, _, j) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/convites/{token}"),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(
        j["dados"],
        json!({"organizacao_nome": "Organização de teste", "email": "novo@exemplo.com", "papel": "operador", "usuario_existe": false})
    );

    // Sem nome/senha → 422.
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/convites/{token}/aceitar"),
            None,
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(j["erro"]["campos"]["senha"].is_string() && j["erro"]["campos"]["nome"].is_string());

    let r = app
        .req(Method::POST, &format!("/api/v1/convites/{token}/aceitar"))
        .json(json!({"nome": "Novo Membro", "senha": "senha nova bem forte"}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["dados"]["usuario"]["email"], "novo@exemplo.com");
    assert_eq!(r.json["dados"]["organizacoes"][0]["papel"], "operador");
    assert_eq!(r.json["dados"]["organizacao_id"], org.to_string());
    let set = r.headers[header::SET_COOKIE].to_str().unwrap();
    let cookie_novo = set.split(';').next().unwrap().to_owned();
    assert_eq!(
        me(&app, &cookie_novo).await["organizacoes"][0]["id"],
        org.to_string()
    );

    // Token já usado.
    let (s, _, _) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/convites/{token}"),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/convites/{token}/aceitar"),
            None,
            Some(json!({"nome": "X", "senha": "senha nova bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Já membro → 409 ao convidar de novo.
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/orgs/{org}/convites"),
            Some(&cookie),
            Some(json!({"email": "novo@exemplo.com", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let mut tx = app.tenant(org).await;
    let acoes: Vec<String> =
        sqlx::query_scalar("SELECT acao FROM auditoria WHERE entidade = 'convite' ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(acoes, vec!["criar", "aceitar"]);
    let dados: Vec<Value> = sqlx::query_scalar("SELECT dados FROM auditoria")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    for d in dados {
        let texto = d.to_string();
        assert!(
            !texto.contains(&token) && !texto.contains("senha"),
            "{texto}"
        );
    }
    drop(tx);
    app.encerrar().await;
}

#[tokio::test]
async fn convite_para_usuario_existente() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let existente = app
        .criar_usuario("existe@exemplo.com", "Existe", SENHA_PADRAO)
        .await;
    let outro = app
        .criar_usuario("outro@exemplo.com", "Outro", SENHA_PADRAO)
        .await;
    let token = convidar(&app, org, &cookie, "existe@exemplo.com", "leitor").await;

    let (_, _, j) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/convites/{token}"),
            None,
            None,
        )
        .await;
    assert_eq!(j["dados"]["usuario_existe"], true);

    let aceitar = format!("/api/v1/convites/{token}/aceitar");
    // Sem sessão → 401 com mensagem clara.
    let (s, _, j) = app.requisicao(Method::POST, &aceitar, None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "{j}");
    assert!(
        j["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("existe@exemplo.com")
    );
    // Sessão de outro usuário → 403.
    let cookie_outro = app.cookie_de(outro).await;
    let (s, _, _) = app
        .requisicao(Method::POST, &aceitar, Some(&cookie_outro), None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Sessão correta, sem corpo.
    let cookie_existente = app.cookie_de(existente).await;
    let r = app
        .req(Method::POST, &aceitar)
        .cookie(&cookie_existente)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert!(r.headers.get(header::SET_COOKIE).is_none());
    assert_eq!(r.json["dados"]["organizacoes"][0]["papel"], "leitor");
    assert_eq!(r.json["dados"]["organizacao_id"], org.to_string());
    app.encerrar().await;
}

#[tokio::test]
async fn convites_invalidos_expirados_e_revogados() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let (s, _, _) = app
        .requisicao(Method::GET, "/api/v1/convites/token-invalido", None, None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/convites/{}", "a".repeat(43)),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Admin não convida dono; papel inválido → 422; e-mail inválido → 422.
    let (_, cookie_admin) = app.membro(org, Papel::Admin).await;
    let url = format!("/api/v1/orgs/{org}/convites");
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie_admin),
            Some(json!({"email": "d@exemplo.com", "papel": "dono"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie_admin),
            Some(json!({"email": "d@exemplo.com", "papel": "rei"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie_admin),
            Some(json!({"email": "nada", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // Revogado.
    let token = convidar(&app, org, &cookie, "rev@exemplo.com", "leitor").await;
    let (_, _, j) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
    let id = j["dados"][0]["id"].as_str().unwrap().to_owned();
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("{url}/{id}"),
            Some(&cookie_admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/convites/{token}"),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("{url}/{id}"),
            Some(&cookie_admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Expirado (7 dias).
    let token = convidar(&app, org, &cookie, "exp@exemplo.com", "leitor").await;
    app.relogio
        .definir(app.estado.agora() + chrono::TimeDelta::days(8));
    let (s, _, _) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/convites/{token}"),
            None,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/convites/{token}/aceitar"),
            None,
            Some(json!({"nome": "X", "senha": "senha nova bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, _, j) = app.requisicao(Method::GET, &url, Some(&cookie), None).await;
    assert_eq!(j["dados"], json!([]));
    app.encerrar().await;
}

#[tokio::test]
async fn isolamento_entre_organizacoes() {
    let app = TestApp::novo().await;
    let (_, org_a, cookie_a) = app.dono_com_org().await;
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/orgs/{org_b}/empresas"),
            Some(&cookie_b),
            Some(json!({"razao_social": "B Ltda", "cnpj": "11.222.333/0001-81"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    let empresa_b = j["dados"]["id"].as_str().unwrap().to_owned();
    let (_, _, j) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/orgs/{org_b}/membros"),
            Some(&cookie_b),
            None,
        )
        .await;
    let dono_b = j["dados"][0]["usuario_id"].as_str().unwrap().to_owned();

    for (metodo, caminho, corpo) in [
        (Method::GET, format!("/api/v1/orgs/{org_b}"), None),
        (
            Method::PATCH,
            format!("/api/v1/orgs/{org_b}"),
            Some(json!({"nome": "x"})),
        ),
        (Method::GET, format!("/api/v1/orgs/{org_b}/membros"), None),
        (
            Method::DELETE,
            format!("/api/v1/orgs/{org_b}/membros/{dono_b}"),
            None,
        ),
        (Method::GET, format!("/api/v1/orgs/{org_b}/convites"), None),
        (Method::GET, format!("/api/v1/orgs/{org_b}/empresas"), None),
        (
            Method::PATCH,
            format!("/api/v1/orgs/{org_b}/empresas/{empresa_b}"),
            Some(json!({"razao_social": "x"})),
        ),
        (
            Method::DELETE,
            format!("/api/v1/orgs/{org_b}/empresas/{empresa_b}"),
            None,
        ),
        (
            Method::GET,
            format!("/api/v1/orgs/{org_b}/assinatura"),
            None,
        ),
        (Method::GET, format!("/api/v1/orgs/{org_b}/auditoria"), None),
    ] {
        let (s, _, j) = app
            .requisicao(metodo.clone(), &caminho, Some(&cookie_a), corpo)
            .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{metodo} {caminho}: {j}");
    }
    // Empresa de B pelo caminho de A → 404 também.
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("/api/v1/orgs/{org_a}/empresas/{empresa_b}"),
            Some(&cookie_a),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    app.encerrar().await;
}

#[tokio::test]
async fn dev_emails_some_em_producao() {
    let app = TestApp::novo().await;
    let (s, _, _) = app
        .requisicao(Method::GET, "/api/v1/dev/emails", None, None)
        .await;
    assert_eq!(s, StatusCode::OK);

    let mut config = (*app.estado.config).clone();
    config.ambiente = Ambiente::Prod;
    let estado = felix_server::app::AppState::novo(app.pool.clone(), config).unwrap();
    let router = felix_server::app::montar(estado, felix_server::modulos::rotas());
    let req = axum::http::Request::builder()
        .uri("/api/v1/dev/emails")
        .header(header::HOST, comum::HOST_TESTE)
        .body(axum::body::Body::empty())
        .unwrap();
    let resposta = tower::ServiceExt::oneshot(router, req).await.unwrap();
    assert_eq!(resposta.status(), StatusCode::NOT_FOUND);
    app.encerrar().await;
}

// ----------------------------- F1: cotas e convites -----------------------------

#[tokio::test]
async fn cota_diaria_de_convites_por_organizacao() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/orgs/{org}/assinatura"),
            Some(&cookie),
            Some(json!({"plano": "empresarial"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let url = format!("/api/v1/orgs/{org}/convites");
    for i in 0..20 {
        let (s, _, j) = app
            .requisicao(
                Method::POST,
                &url,
                Some(&cookie),
                Some(json!({"email": format!("p{i}@exemplo.com"), "papel": "leitor"})),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{i}: {j}");
    }
    let (s, h, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"email": "p20@exemplo.com", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "{j}");
    assert_eq!(j["erro"]["codigo"], "ocupado");
    assert!(
        j["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("20 convites"),
        "{j}"
    );
    assert!(h.contains_key(header::RETRY_AFTER));
    // Depois de 24 h a cota renova.
    app.relogio
        .definir(app.estado.agora() + chrono::TimeDelta::hours(25));
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &url,
            Some(&cookie),
            Some(json!({"email": "p20@exemplo.com", "papel": "leitor"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    app.encerrar().await;
}

#[tokio::test]
async fn limite_de_organizacoes_criadas_por_usuario() {
    let app = TestApp::novo().await;
    let (_, _, cookie) = app.dono_com_org().await;
    let mut ultima = None;
    for i in 0..10 {
        let (s, _, j) = app
            .requisicao(
                Method::POST,
                "/api/v1/orgs",
                Some(&cookie),
                Some(json!({"nome": format!("Org {i}")})),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{i}: {j}");
        ultima = Some(j["dados"]["id"].as_str().unwrap().to_owned());
    }
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/orgs",
            Some(&cookie),
            Some(json!({"nome": "Org 11"})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{j}");
    assert!(
        j["erro"]["mensagem"].as_str().unwrap().contains("10"),
        "{j}"
    );
    // Excluir uma libera a vaga.
    let ultima = ultima.unwrap();
    let (s, _, j) = app
        .requisicao(
            Method::DELETE,
            &format!("/api/v1/orgs/{ultima}"),
            Some(&cookie),
            Some(json!({"confirmacao": "Org 9"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{j}");
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/orgs",
            Some(&cookie),
            Some(json!({"nome": "Org 11"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    app.encerrar().await;
}

async fn convite_publico(app: &TestApp, token: &str) -> StatusCode {
    app.requisicao(
        Method::GET,
        &format!("/api/v1/convites/{token}"),
        None,
        None,
    )
    .await
    .0
}

#[tokio::test]
async fn convites_de_quem_perdeu_o_papel_de_admin() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;

    // Rebaixado para operador: convites pendentes dele são revogados.
    let (admin, cookie_admin) = app.membro(org, Papel::Admin).await;
    let token = convidar(&app, org, &cookie_admin, "a1@exemplo.com", "leitor").await;
    assert_eq!(convite_publico(&app, &token).await, StatusCode::OK);
    let (s, _, j) = app
        .requisicao(
            Method::PATCH,
            &format!("/api/v1/orgs/{org}/membros/{admin}"),
            Some(&cookie),
            Some(json!({"papel": "operador"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(convite_publico(&app, &token).await, StatusCode::NOT_FOUND);
    let (_, _, j) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/orgs/{org}/convites"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(j["dados"], json!([]), "{j}");

    // Removido: idem.
    let (admin2, cookie_admin2) = app.membro(org, Papel::Admin).await;
    let token2 = convidar(&app, org, &cookie_admin2, "a2@exemplo.com", "leitor").await;
    let (s, _, _) = app
        .requisicao(
            Method::DELETE,
            &format!("/api/v1/orgs/{org}/membros/{admin2}"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(convite_publico(&app, &token2).await, StatusCode::NOT_FOUND);

    // Mesmo sem a revogação (papel alterado por fora), o aceite exige que quem
    // convidou ainda seja admin ou dono.
    let (admin3, cookie_admin3) = app.membro(org, Papel::Admin).await;
    let token3 = convidar(&app, org, &cookie_admin3, "a3@exemplo.com", "leitor").await;
    app.adicionar_membro(org, admin3, Papel::Leitor).await;
    assert_eq!(convite_publico(&app, &token3).await, StatusCode::NOT_FOUND);
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/convites/{token3}/aceitar"),
            None,
            Some(json!({"nome": "Novo", "senha": SENHA_PADRAO})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{j}");
    assert!(
        j["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .starts_with("Convite inválido"),
        "{j}"
    );
    // Promovido de volta, o convite volta a valer.
    app.adicionar_membro(org, admin3, Papel::Admin).await;
    assert_eq!(convite_publico(&app, &token3).await, StatusCode::OK);
    app.encerrar().await;
}

#[tokio::test]
async fn aceite_de_convite_limitado_por_ip() {
    let app = TestApp::novo().await;
    let origem: std::net::IpAddr = "10.20.30.40".parse().unwrap();
    let falso = "x".repeat(43);
    for _ in 0..20 {
        let r = app
            .req(Method::POST, &format!("/api/v1/convites/{falso}/aceitar"))
            .ip(origem)
            .enviar()
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
    }
    let r = app
        .req(Method::POST, &format!("/api/v1/convites/{falso}/aceitar"))
        .ip(origem)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    app.encerrar().await;
}
