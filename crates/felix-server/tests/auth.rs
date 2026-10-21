//! Testes de autenticação: cadastro, login, logout, Me, troca e redefinição de senha,
//! limitador de tentativas e caixa de saída de e-mail.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

mod comum;

use axum::http::{Method, StatusCode, header};
use comum::{SENHA_PADRAO, TestApp};
use serde_json::{Value, json};
use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

const SENHA: &str = "cavalo correto bateria";

fn cookie_da_resposta(headers: &axum::http::HeaderMap) -> String {
    let valor = headers
        .get(header::SET_COOKIE)
        .expect("Set-Cookie ausente")
        .to_str()
        .unwrap();
    valor.split(';').next().unwrap().trim().to_owned()
}

async fn cadastrar(app: &TestApp, email: &str) -> (Value, String) {
    let r = app
        .req(Method::POST, "/api/v1/auth/cadastro")
        .json(json!({
            "nome": "Maria Silva",
            "email": email,
            "senha": SENHA,
            "organizacao_nome": "Comercial Silva",
        }))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    let cookie = cookie_da_resposta(&r.headers);
    (r.json["dados"].clone(), cookie)
}

async fn login(app: &TestApp, email: &str, senha: &str) -> comum::Resposta {
    app.req(Method::POST, "/api/v1/auth/login")
        .json(json!({"email": email, "senha": senha}))
        .enviar()
        .await
}

async fn me(app: &TestApp, cookie: &str) -> (StatusCode, Value) {
    let (s, _, j) = app
        .requisicao(Method::GET, "/api/v1/auth/me", Some(cookie), None)
        .await;
    (s, j)
}

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

/// "Esqueci a senha" roda em segundo plano: espera até haver `quantidade`
/// e-mails para `para` e devolve a lista (mais recente primeiro).
async fn esperar_emails(app: &TestApp, para: &str, quantidade: usize) -> Vec<Value> {
    for _ in 0..200 {
        let emails = emails_para(app, para).await;
        if emails.len() >= quantidade {
            return emails;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("Esperava {quantidade} e-mail(s) para {para}");
}

async fn tipos_de_evento(app: &TestApp) -> Vec<String> {
    sqlx::query_scalar("SELECT tipo FROM eventos_autenticacao ORDER BY criado_em, id")
        .fetch_all(&app.pool)
        .await
        .unwrap()
}

fn token_do_link(corpo: &str, marcador: &str) -> String {
    let inicio = corpo.find(marcador).unwrap() + marcador.len();
    corpo[inicio..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

#[tokio::test]
async fn fluxo_cadastro_me_logout_login() {
    let app = TestApp::novo().await;
    let (me_cadastro, cookie) = cadastrar(&app, "  Maria@Exemplo.COM ").await;
    assert_eq!(me_cadastro["usuario"]["email"], "maria@exemplo.com");
    assert_eq!(me_cadastro["usuario"]["nome"], "Maria Silva");
    let orgs = me_cadastro["organizacoes"].as_array().unwrap();
    assert_eq!(orgs.len(), 1);
    assert_eq!(orgs[0]["nome"], "Comercial Silva");
    assert_eq!(orgs[0]["papel"], "dono");
    assert_eq!(orgs[0]["plano"], "profissional");

    // Assinatura em teste por 14 dias.
    let org = orgs[0]["id"].as_str().unwrap().to_owned();
    let (s, _, j) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/orgs/{org}/assinatura"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["dados"]["status"], "teste");
    assert!(j["dados"]["teste_ate"].is_string());

    let (s, j) = me(&app, &cookie).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(j["dados"], me_cadastro);

    // Logout apaga cookie e revoga a sessão.
    let r = app
        .req(Method::POST, "/api/v1/auth/logout")
        .cookie(&cookie)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let set = r.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(set.contains("Max-Age=0"), "{set}");
    let (s, j) = me(&app, &cookie).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "{j}");

    // Login com e-mail em caixa diferente.
    let r = login(&app, "MARIA@exemplo.com", SENHA).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["dados"]["usuario"]["email"], "maria@exemplo.com");
    let cookie2 = cookie_da_resposta(&r.headers);
    let (s, _) = me(&app, &cookie2).await;
    assert_eq!(s, StatusCode::OK);

    // Auditoria do cadastro.
    let mut tx = app.tenant(org.parse().unwrap()).await;
    let acoes: Vec<String> = sqlx::query_scalar("SELECT acao FROM auditoria")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert!(acoes.contains(&"cadastro".to_owned()), "{acoes:?}");
    drop(tx);
    app.encerrar().await;
}

#[tokio::test]
async fn cadastro_validacoes_e_email_duplicado() {
    let app = TestApp::novo().await;
    let r = app
        .req(Method::POST, "/api/v1/auth/cadastro")
        .json(json!({"nome": "Ana", "email": "ana@exemplo.com", "senha": "curta", "organizacao_nome": "Org"}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.json["erro"]["codigo"], "validacao");
    assert!(r.json["erro"]["campos"]["senha"].is_string(), "{}", r.json);

    let r = app
        .req(Method::POST, "/api/v1/auth/cadastro")
        .json(json!({"nome": " ", "email": "sem-arroba", "senha": SENHA, "organizacao_nome": ""}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    for campo in ["nome", "email", "organizacao_nome"] {
        assert!(
            r.json["erro"]["campos"][campo].is_string(),
            "{campo}: {}",
            r.json
        );
    }

    cadastrar(&app, "ana@exemplo.com").await;
    let r = app
        .req(Method::POST, "/api/v1/auth/cadastro")
        .json(json!({"nome": "Outra", "email": "ANA@exemplo.com", "senha": SENHA, "organizacao_nome": "X"}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.json);
    assert!(r.headers.get(header::SET_COOKIE).is_none());
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM organizacoes")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "cadastro duplicado não pode deixar organização órfã");
    app.encerrar().await;
}

#[tokio::test]
async fn login_invalido_e_generico() {
    let app = TestApp::novo().await;
    app.criar_usuario("joao@exemplo.com", "João", SENHA_PADRAO)
        .await;
    let errada = login(&app, "joao@exemplo.com", "senha errada qualquer").await;
    let inexistente = login(&app, "ninguem@exemplo.com", "senha errada qualquer").await;
    for r in [&errada, &inexistente] {
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.json["erro"]["codigo"], "credenciais_invalidas");
        assert!(r.headers.get(header::SET_COOKIE).is_none());
    }
    assert_eq!(errada.json, inexistente.json);
    let r = login(&app, "joao@exemplo.com", SENHA_PADRAO).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["dados"]["organizacoes"], json!([]));
    app.encerrar().await;
}

async fn login_de(app: &TestApp, ip: IpAddr, email: &str, senha: &str) -> comum::Resposta {
    app.req(Method::POST, "/api/v1/auth/login")
        .ip(ip)
        .json(json!({"email": email, "senha": senha}))
        .enviar()
        .await
}

fn ip(a: u8, b: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(10, 0, a, b))
}

#[tokio::test]
async fn limitador_de_login_por_email_conta_so_falhas() {
    let app = TestApp::novo().await;
    app.criar_usuario("alvo@exemplo.com", "Alvo", SENHA_PADRAO)
        .await;
    // Sucesso zera o contador do e-mail: 4 falhas + sucesso + 4 falhas não bloqueiam.
    for rodada in 0..2_u8 {
        for i in 0..4_u8 {
            let r = login_de(&app, ip(rodada, i), "alvo@exemplo.com", "errada errada").await;
            assert_eq!(r.status, StatusCode::UNAUTHORIZED, "rodada {rodada}, {i}");
        }
        let r = login_de(&app, ip(9, rodada), "alvo@exemplo.com", SENHA_PADRAO).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    }
    // 5 falhas seguidas (IPs diferentes) → bloqueio curto do e-mail, mesmo com a senha certa.
    for i in 0..5_u8 {
        let r = login_de(&app, ip(2, i), "ALVO@exemplo.com ", "errada errada").await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "tentativa {i}");
    }
    let r = login_de(&app, ip(3, 1), "alvo@exemplo.com", SENHA_PADRAO).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.json);
    assert_eq!(r.json["erro"]["codigo"], "ocupado");
    assert_eq!(
        r.json["erro"]["mensagem"],
        "Muitas tentativas. Aguarde alguns minutos."
    );
    // E-mail acima de 254 caracteres → 422 antes de tocar o limitador.
    let longo = format!("{}@exemplo.com", "a".repeat(250));
    let r = login_de(&app, ip(3, 2), &longo, "x").await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.json);
    assert!(r.json["erro"]["campos"]["email"].is_string());
    app.encerrar().await;
}

#[tokio::test]
async fn limitador_de_login_por_ip() {
    let app = TestApp::novo().await;
    app.criar_usuario("vitima@exemplo.com", "Vítima", SENHA_PADRAO)
        .await;
    let atacante = ip(9, 9);
    for i in 0..30 {
        let r = login_de(
            &app,
            atacante,
            &format!("x{i}@exemplo.com"),
            "errada errada",
        )
        .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "tentativa {i}");
    }
    // IP estourado: 429 e o e-mail da vítima não acumula falhas.
    for _ in 0..10 {
        let r = login_de(&app, atacante, "vitima@exemplo.com", "errada errada").await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    }
    let r = login_de(&app, ip(1, 1), "vitima@exemplo.com", SENHA_PADRAO).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    app.encerrar().await;
}

#[tokio::test]
async fn limitador_de_esqueci_cadastro_e_redefinir() {
    let app = TestApp::novo().await;
    // "Esqueci a senha": 10 por IP.
    let origem = ip(8, 8);
    for _ in 0..10 {
        let r = app
            .req(Method::POST, "/api/v1/auth/senha/esqueci")
            .ip(origem)
            .json(json!({"email": "alguem@exemplo.com"}))
            .enviar()
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
    }
    let r = app
        .req(Method::POST, "/api/v1/auth/senha/esqueci")
        .ip(origem)
        .json(json!({"email": "outro@exemplo.com"}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);

    // Cadastro: 5 por hora por IP.
    let origem = ip(7, 7);
    for i in 0..5 {
        let r = app
            .req(Method::POST, "/api/v1/auth/cadastro")
            .ip(origem)
            .json(json!({"nome": "N", "email": format!("c{i}@exemplo.com"), "senha": SENHA, "organizacao_nome": "O"}))
            .enviar()
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    }
    let r = app
        .req(Method::POST, "/api/v1/auth/cadastro")
        .ip(origem)
        .json(json!({"nome": "N", "email": "c9@exemplo.com", "senha": SENHA, "organizacao_nome": "O"}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.json);
    // Outro IP continua podendo.
    let r = app
        .req(Method::POST, "/api/v1/auth/cadastro")
        .ip(ip(7, 8))
        .json(json!({"nome": "N", "email": "c9@exemplo.com", "senha": SENHA, "organizacao_nome": "O"}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);

    // Redefinição por token: 20 por IP.
    let origem = ip(6, 6);
    for _ in 0..20 {
        let r = app
            .req(Method::POST, "/api/v1/auth/senha/redefinir")
            .ip(origem)
            .json(json!({"token": "x".repeat(43), "senha": "nova senha bem forte"}))
            .enviar()
            .await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let r = app
        .req(Method::POST, "/api/v1/auth/senha/redefinir")
        .ip(origem)
        .json(json!({"token": "x".repeat(43), "senha": "nova senha bem forte"}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    app.encerrar().await;
}

#[tokio::test]
async fn limitador_de_troca_de_senha_por_usuario() {
    let app = TestApp::novo().await;
    let (_, cookie) = cadastrar(&app, "troca@exemplo.com").await;
    for i in 0..5 {
        let (s, _, j) = app
            .requisicao(
                Method::POST,
                "/api/v1/auth/me/senha",
                Some(&cookie),
                Some(json!({"senha_atual": "errada errada", "senha_nova": "nova senha bem forte"})),
            )
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "tentativa {i}: {j}");
    }
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/me/senha",
            Some(&cookie),
            Some(json!({"senha_atual": SENHA, "senha_nova": "nova senha bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "{j}");
    app.encerrar().await;
}

#[tokio::test]
async fn eventos_de_autenticacao_sem_segredos() {
    let app = TestApp::novo().await;
    let (_, cookie) = cadastrar(&app, "eva@exemplo.com").await;
    let r = login_de(&app, ip(4, 4), "eva@exemplo.com", "errada errada").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = login_de(&app, ip(4, 4), "ninguem@exemplo.com", "errada errada").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = app
        .req(Method::POST, "/api/v1/auth/login")
        .ip(ip(4, 5))
        .cabecalho("user-agent", "navegador-teste")
        .json(json!({"email": "eva@exemplo.com", "senha": SENHA}))
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/me/senha",
            Some(&cookie),
            Some(json!({"senha_atual": SENHA, "senha_nova": "nova senha bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let r = app
        .req(Method::POST, "/api/v1/auth/logout")
        .cookie(&cookie)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(
        tipos_de_evento(&app).await,
        [
            "login_falha",
            "login_falha",
            "login_sucesso",
            "senha_trocada",
            "logout"
        ]
    );
    type Linha = (
        Option<uuid::Uuid>,
        Option<Vec<u8>>,
        Option<String>,
        Option<String>,
    );
    let linhas: Vec<Linha> = sqlx::query_as(
        "SELECT usuario_id, email_hash, host(ip), user_agent FROM eventos_autenticacao
         WHERE tipo IN ('login_falha', 'login_sucesso') ORDER BY criado_em, id",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert!(
        linhas[0].0.is_some(),
        "falha com usuário existente guarda o id"
    );
    assert!(linhas[1].0.is_none(), "e-mail inexistente não tem usuário");
    assert_eq!(linhas[0].1.as_ref().map(Vec::len), Some(32));
    assert_eq!(linhas[0].1, linhas[2].1, "mesmo e-mail, mesmo hash");
    assert_eq!(linhas[0].2.as_deref(), Some("10.0.4.4"));
    assert_eq!(linhas[2].3.as_deref(), Some("navegador-teste"));
    let texto: String =
        sqlx::query_scalar("SELECT string_agg(e::text, ' ') FROM eventos_autenticacao e")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(
        !texto.contains("eva@exemplo.com") && !texto.contains("errada"),
        "{texto}"
    );
    app.encerrar().await;
}

#[tokio::test]
async fn dev_emails_exige_host_loopback() {
    let app = TestApp::novo().await;
    let r = app
        .req(Method::GET, "/api/v1/dev/emails")
        .cabecalho("host", "felix.exemplo.com.br")
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.json);
    let r = app
        .req(Method::GET, "/api/v1/dev/emails")
        .cabecalho("host", "localhost:5179")
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    app.encerrar().await;
}

#[tokio::test]
async fn esqueci_e_redefinir_senha() {
    let app = TestApp::novo().await;
    let (_, cookie) = cadastrar(&app, "rosa@exemplo.com").await;
    let r2 = login(&app, "rosa@exemplo.com", SENHA).await;
    let cookie2 = cookie_da_resposta(&r2.headers);

    // E-mail inexistente: 204 e nada na caixa de saída.
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/senha/esqueci",
            None,
            Some(json!({"email": "fantasma@exemplo.com"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // O trabalho roda em segundo plano: dá tempo para um e-mail indevido aparecer.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM emails_saida")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);

    let (s, _, _) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/senha/esqueci",
            None,
            Some(json!({"email": "ROSA@exemplo.com"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let emails = esperar_emails(&app, "rosa@exemplo.com", 1).await;
    let corpo = emails[0]["corpo"].as_str().unwrap();
    assert!(
        corpo.contains("http://127.0.0.1:8090/redefinir-senha?token="),
        "{corpo}"
    );
    let token = token_do_link(corpo, "redefinir-senha?token=");
    assert_eq!(token.len(), 43);

    // Token inválido e senha fraca.
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/senha/redefinir",
            None,
            Some(json!({"token": "x".repeat(43), "senha": "nova senha bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/senha/redefinir",
            None,
            Some(json!({"token": token, "senha": "123"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(j["erro"]["campos"]["senha"].is_string());

    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/senha/redefinir",
            None,
            Some(json!({"token": token, "senha": "nova senha bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{j}");
    assert!(
        tipos_de_evento(&app)
            .await
            .contains(&"senha_redefinida".to_owned())
    );
    // Todas as sessões revogadas.
    assert_eq!(me(&app, &cookie).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&app, &cookie2).await.0, StatusCode::UNAUTHORIZED);
    // Token não pode ser reutilizado.
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/senha/redefinir",
            None,
            Some(json!({"token": token, "senha": "outra senha bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        login(&app, "rosa@exemplo.com", SENHA).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "rosa@exemplo.com", "nova senha bem forte")
            .await
            .status,
        StatusCode::OK
    );

    // Token expirado (1 h).
    app.requisicao(
        Method::POST,
        "/api/v1/auth/senha/esqueci",
        None,
        Some(json!({"email": "rosa@exemplo.com"})),
    )
    .await;
    let emails = esperar_emails(&app, "rosa@exemplo.com", 2).await;
    let corpo = emails[0]["corpo"].as_str().unwrap().to_owned();
    let token2 = token_do_link(&corpo, "redefinir-senha?token=");
    assert_ne!(token2, token);
    app.relogio
        .definir(app.estado.agora() + chrono::TimeDelta::minutes(61));
    let (s, _, _) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/senha/redefinir",
            None,
            Some(json!({"token": token2, "senha": "mais uma senha forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    app.encerrar().await;
}

#[tokio::test]
async fn troca_de_senha_revoga_outras_sessoes() {
    let app = TestApp::novo().await;
    let (_, cookie) = cadastrar(&app, "lia@exemplo.com").await;
    let outra = cookie_da_resposta(&login(&app, "lia@exemplo.com", SENHA).await.headers);

    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/me/senha",
            Some(&cookie),
            Some(json!({"senha_atual": "errada errada", "senha_nova": "nova senha bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");
    assert!(j["erro"]["campos"]["senha_atual"].is_string());
    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/me/senha",
            Some(&cookie),
            Some(json!({"senha_atual": SENHA, "senha_nova": "1234567890"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(j["erro"]["campos"]["senha_nova"].is_string());

    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/auth/me/senha",
            Some(&cookie),
            Some(json!({"senha_atual": SENHA, "senha_nova": "nova senha bem forte"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{j}");
    assert_eq!(me(&app, &cookie).await.0, StatusCode::OK);
    assert_eq!(me(&app, &outra).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        login(&app, "lia@exemplo.com", "nova senha bem forte")
            .await
            .status,
        StatusCode::OK
    );
    app.encerrar().await;
}

#[tokio::test]
async fn patch_me_e_segunda_org() {
    let app = TestApp::novo().await;
    let (_, cookie) = cadastrar(&app, "bia@exemplo.com").await;
    let (s, _, j) = app
        .requisicao(
            Method::PATCH,
            "/api/v1/auth/me",
            Some(&cookie),
            Some(json!({"nome": "  Beatriz  "})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["dados"]["usuario"]["nome"], "Beatriz");
    let (s, _, _) = app
        .requisicao(
            Method::PATCH,
            "/api/v1/auth/me",
            Some(&cookie),
            Some(json!({"nome": ""})),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    let (s, _, j) = app
        .requisicao(
            Method::POST,
            "/api/v1/orgs",
            Some(&cookie),
            Some(json!({"nome": "Segunda Org"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert_eq!(j["dados"]["papel"], "dono");
    assert_eq!(j["dados"]["plano"], "gratuito");
    let (_, j) = me(&app, &cookie).await;
    let nomes: Vec<&str> = j["dados"]["organizacoes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["nome"].as_str().unwrap())
        .collect();
    assert_eq!(nomes, vec!["Comercial Silva", "Segunda Org"]);
    app.encerrar().await;
}

#[tokio::test]
async fn me_sem_sessao_e_plano_efetivo_apos_teste() {
    let app = TestApp::novo().await;
    let (s, _, j) = app
        .requisicao(Method::GET, "/api/v1/auth/me", None, None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(j["erro"]["codigo"], "nao_autenticado");
    let (_, cookie) = cadastrar(&app, "teo@exemplo.com").await;
    app.relogio
        .definir(app.estado.agora() + chrono::TimeDelta::days(15));
    let (_, j) = me(&app, &cookie).await;
    assert_eq!(j["dados"]["organizacoes"][0]["plano"], "gratuito");
    app.encerrar().await;
}
