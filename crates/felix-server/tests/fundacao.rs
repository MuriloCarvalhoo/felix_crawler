//! Testes da fundação: health/ready, RLS, CSRF, sessão, OrgCtx, envelope, SPA e upload.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod comum;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart, State},
    http::{Method, StatusCode, header},
    routing::{get, post},
};
use comum::{Opcoes, TestApp};
use felix_server::{
    app::AppState,
    db,
    http::{ClienteInfo, Corpo, ErroApi, OrgCtx, Papel, Sessao, dados, sem_conteudo},
    pdf,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

fn rotas_teste() -> Router<AppState> {
    Router::new()
        .route(
            "/teste/sessao",
            get(|s: Sessao| async move { dados(json!({ "usuario_id": s.usuario_id })) }),
        )
        .route(
            "/teste/eco",
            post(|Corpo(v): Corpo<Value>| async move { dados(v) }),
        )
        .route(
            "/orgs/{org}/teste",
            get(|ctx: OrgCtx| async move { dados(json!({ "papel": ctx.papel, "org": ctx.org_id })) }),
        )
        .route(
            "/orgs/{org}/teste/admin",
            post(|ctx: OrgCtx| async move {
                ctx.exigir(Papel::Admin)?;
                Ok::<_, ErroApi>(sem_conteudo())
            }),
        )
        .route(
            "/teste/validacao",
            get(|| async { ErroApi::validacao_campo("cnpj", "CNPJ inválido") }),
        )
        .route("/teste/ocupado", get(|| async { ErroApi::ocupado() }))
        .route(
            "/teste/cliente",
            get(|c: ClienteInfo| async move {
                dados(json!({ "ip": c.ip.map(|ip| ip.to_string()) }))
            }),
        )
        .route(
            "/teste/duplicado",
            post(|State(e): State<AppState>| async move {
                for _ in 0..2 {
                    sqlx::query(
                        "INSERT INTO usuarios (email, nome, senha_hash) VALUES ('dup@x.com', 'D', 'x')",
                    )
                    .execute(&e.pool)
                    .await?;
                }
                Ok::<_, ErroApi>(sem_conteudo())
            }),
        )
        .route(
            "/teste/hoje",
            get(|State(e): State<AppState>| async move { dados(e.hoje()) }),
        )
        .route(
            "/teste/analisar",
            post(|State(e): State<AppState>, m: Multipart| async move {
                pdf::receber_e_analisar(&e, m).await.map(dados)
            })
            .layer(DefaultBodyLimit::max(pdf::LIMITE_CORPO_ANALISE)),
        )
}

async fn app_teste() -> TestApp {
    TestApp::com_rotas(rotas_teste()).await
}

// ----------------------------- health / ready -----------------------------

#[tokio::test]
async fn health_ready_e_cabecalhos() {
    let app = TestApp::novo().await;
    let (status, headers, json) = app.requisicao(Method::GET, "/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json, json!({"status": "ok"}));
    assert!(
        headers[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(headers.contains_key("x-request-id"));
    let (status, headers, json) = app.requisicao(Method::GET, "/ready", None, None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    app.encerrar().await;
}

#[tokio::test]
async fn ready_503_quando_banco_cai() {
    let app = TestApp::novo().await;
    app.pool.close().await;
    let (status, _, json) = app.requisicao(Method::GET, "/ready", None, None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["erro"]["codigo"], "indisponivel");
    app.encerrar().await;
}

#[tokio::test]
async fn migracoes_criam_todo_o_esquema() {
    let app = TestApp::novo().await;
    let tabelas: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name <> '_sqlx_migrations' ORDER BY 1",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();
    let esperadas = [
        "alertas_email_enviados",
        "assinaturas",
        "auditoria",
        "cobrancas",
        "convites",
        "documentos_transparencia",
        "emails_saida",
        "empenhos",
        "empresas",
        "eventos_autenticacao",
        "eventos_nota",
        "integracoes",
        "jobs",
        "membros",
        "notas",
        "notificacoes",
        "organizacoes",
        "sessoes",
        "tokens_usuario",
        "usuarios",
    ];
    assert_eq!(tabelas, esperadas);
    let com_rls: Vec<String> = sqlx::query_scalar(
        "SELECT relname::text FROM pg_class
         WHERE relnamespace = 'public'::regnamespace AND relkind = 'r'
           AND relrowsecurity AND relforcerowsecurity ORDER BY 1",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        com_rls,
        [
            "alertas_email_enviados",
            "auditoria",
            "cobrancas",
            "documentos_transparencia",
            "empenhos",
            "empresas",
            "eventos_nota",
            "integracoes",
            "notas",
            "notificacoes"
        ]
    );
    let id: Uuid = sqlx::query_scalar("SELECT uuidv7()")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(id.get_version_num(), 7);
    app.encerrar().await;
}

// ----------------------------- RLS -----------------------------

async fn org_com_empresa(app: &TestApp, cnpj: &str) -> Uuid {
    let (_, org, _) = app.dono_com_org().await;
    let mut tx = app.tenant(org).await;
    sqlx::query(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj) VALUES ($1, 'Empresa', $2)",
    )
    .bind(org)
    .bind(cnpj)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    org
}

#[tokio::test]
async fn rls_sem_org_id_nao_retorna_linhas() {
    let app = TestApp::novo().await;
    org_com_empresa(&app, "11111111000111").await;
    // Papel felix_app sem app.org_id.
    let mut tx = app.pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE felix_app")
        .execute(&mut *tx)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM empresas")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 0);
    tx.rollback().await.unwrap();
    // Dono das tabelas fora de transação de tenant: FORCE RLS também filtra.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM empresas")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    app.encerrar().await;
}

#[tokio::test]
async fn rls_org_a_nao_le_nem_escreve_em_b() {
    let app = TestApp::novo().await;
    let a = org_com_empresa(&app, "11111111000111").await;
    let b = org_com_empresa(&app, "22222222000122").await;

    let mut tx = app.tenant(a).await;
    let cnpjs: Vec<String> = sqlx::query_scalar("SELECT cnpj::text FROM empresas")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(cnpjs, ["11111111000111"]);
    let alteradas = sqlx::query("UPDATE empresas SET razao_social = 'X' WHERE organizacao_id = $1")
        .bind(b)
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(alteradas, 0);
    let apagadas = sqlx::query("DELETE FROM empresas WHERE organizacao_id = $1")
        .bind(b)
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(apagadas, 0);
    tx.commit().await.unwrap();

    // WITH CHECK impede inserir em B estando no contexto de A.
    let mut tx = app.tenant(a).await;
    let erro = sqlx::query(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj) VALUES ($1, 'Intrusa', '33333333000133')",
    )
    .bind(b)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(
        erro.as_database_error().and_then(|d| d.code()).as_deref(),
        Some("42501"),
        "{erro}"
    );
    drop(tx);

    // Também não move uma linha própria para B.
    let mut tx = app.tenant(a).await;
    let erro = sqlx::query("UPDATE empresas SET organizacao_id = $1")
        .bind(b)
        .execute(&mut *tx)
        .await;
    assert!(erro.is_err());
    drop(tx);

    let mut tx = app.tenant(b).await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM empresas")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 1);
    app.encerrar().await;
}

#[tokio::test]
async fn auditoria_fica_no_tenant() {
    let app = TestApp::novo().await;
    let (usuario, a, _) = app.dono_com_org().await;
    let (_, b, _) = app.dono_com_org().await;
    let mut tx = app.tenant(a).await;
    db::auditar(
        &mut tx,
        a,
        Some(usuario),
        "criar",
        "empresa",
        Some("x"),
        json!({"cnpj": "11111111000111"}),
        Some("10.0.0.1".parse().unwrap()),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut tx = app.tenant(a).await;
    let (acao, ip): (String, Option<String>) =
        sqlx::query_as("SELECT acao, host(ip) FROM auditoria")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!((acao.as_str(), ip.as_deref()), ("criar", Some("10.0.0.1")));
    drop(tx);
    let mut tx = app.tenant(b).await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM auditoria")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 0);
    app.encerrar().await;
}

// ----------------------------- CSRF -----------------------------

#[tokio::test]
async fn csrf_exige_x_felix_e_origem_valida() {
    let app = app_teste().await;
    let r = app
        .req(Method::POST, "/api/v1/teste/eco")
        .json(json!({"a": 1}))
        .sem_x_felix()
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.json["erro"]["codigo"], "proibido");

    let r = app
        .req(Method::POST, "/api/v1/teste/eco")
        .json(json!({"a": 1}))
        .cabecalho("origin", "https://malicioso.example")
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    for origem in ["http://127.0.0.1:8090", "http://127.0.0.1:5179"] {
        let r = app
            .req(Method::POST, "/api/v1/teste/eco")
            .json(json!({"a": 1}))
            .cabecalho("origin", origem)
            .enviar()
            .await;
        assert_eq!(r.status, StatusCode::OK, "{origem}: {}", r.json);
        assert_eq!(r.json, json!({"dados": {"a": 1}}));
    }

    // GET não exige X-Felix.
    let r = app
        .req(Method::GET, "/api/v1/teste/validacao")
        .sem_x_felix()
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    app.encerrar().await;
}

// ----------------------------- sessão -----------------------------

#[tokio::test]
async fn sessao_valida_expirada_revogada_e_absoluta() {
    let app = app_teste().await;
    let (status, _, json) = app
        .requisicao(Method::GET, "/api/v1/teste/sessao", None, None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["erro"]["codigo"], "nao_autenticado");

    let (status, _, _) = app
        .requisicao(
            Method::GET,
            "/api/v1/teste/sessao",
            Some("felix_sessao=lixo"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let usuario = app
        .criar_usuario("ana@teste.felix", "Ana", comum::SENHA_PADRAO)
        .await;
    let cookie = app.cookie_de(usuario).await;
    let (status, _, json) = app
        .requisicao(
            Method::GET,
            "/api/v1/teste/sessao",
            Some(&format!("outro=1; {cookie}")),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dados"]["usuario_id"], usuario.to_string());

    // Expirada.
    sqlx::query("UPDATE sessoes SET expira_em = now() - interval '1 second'")
        .execute(&app.pool)
        .await
        .unwrap();
    let (status, _, _) = app
        .requisicao(Method::GET, "/api/v1/teste/sessao", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Revogada.
    let cookie = app.cookie_de(usuario).await;
    sqlx::query("UPDATE sessoes SET revogada_em = now()")
        .execute(&app.pool)
        .await
        .unwrap();
    let (status, _, _) = app
        .requisicao(Method::GET, "/api/v1/teste/sessao", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Absoluta de 30 dias, mesmo com expira_em no futuro.
    let cookie = app.cookie_de(usuario).await;
    sqlx::query(
        "UPDATE sessoes SET criado_em = now() - interval '31 days', expira_em = now() + interval '1 day'
         WHERE revogada_em IS NULL",
    )
    .execute(&app.pool)
    .await
    .unwrap();
    let (status, _, _) = app
        .requisicao(Method::GET, "/api/v1/teste/sessao", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.encerrar().await;
}

#[tokio::test]
async fn sessao_desliza_no_maximo_uma_vez_por_hora() {
    let app = app_teste().await;
    let usuario = app
        .criar_usuario("bia@teste.felix", "Bia", comum::SENHA_PADRAO)
        .await;
    let cookie = app.cookie_de(usuario).await;
    sqlx::query(
        "UPDATE sessoes SET ultimo_uso_em = now() - interval '2 hours', expira_em = now() + interval '1 hour'",
    )
    .execute(&app.pool)
    .await
    .unwrap();
    let (status, _, _) = app
        .requisicao(Method::GET, "/api/v1/teste/sessao", Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let renovada: bool = sqlx::query_scalar(
        "SELECT expira_em > now() + interval '6 days 23 hours' AND ultimo_uso_em > now() - interval '1 minute' FROM sessoes",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(renovada);

    // Uso recente: não renova.
    sqlx::query("UPDATE sessoes SET ultimo_uso_em = now() - interval '10 minutes', expira_em = now() + interval '1 hour'")
        .execute(&app.pool)
        .await
        .unwrap();
    app.requisicao(Method::GET, "/api/v1/teste/sessao", Some(&cookie), None)
        .await;
    let intacta: bool =
        sqlx::query_scalar("SELECT expira_em < now() + interval '2 hours' FROM sessoes")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(intacta);
    app.encerrar().await;
}

// ----------------------------- OrgCtx -----------------------------

#[tokio::test]
async fn org_ctx_membro_papel_e_isolamento() {
    let app = app_teste().await;
    let (_, org, cookie_dono) = app.dono_com_org().await;
    let (_, outra_org, _) = app.dono_com_org().await;
    let caminho = format!("/api/v1/orgs/{org}/teste");

    let (status, _, json) = app
        .requisicao(Method::GET, &caminho, Some(&cookie_dono), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["dados"]["papel"], "dono");

    // Sem sessão → 401.
    let (status, _, _) = app.requisicao(Method::GET, &caminho, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Não membro → 404.
    let (status, _, json) = app
        .requisicao(
            Method::GET,
            &format!("/api/v1/orgs/{outra_org}/teste"),
            Some(&cookie_dono),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["erro"]["codigo"], "nao_encontrado");

    // UUID inválido → 404.
    let (status, _, _) = app
        .requisicao(
            Method::GET,
            "/api/v1/orgs/nao-e-uuid/teste",
            Some(&cookie_dono),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Papel insuficiente → 403; suficiente → 204.
    let (_, cookie_leitor) = app.membro(org, Papel::Leitor).await;
    let (status, _, json) = app
        .requisicao(Method::GET, &caminho, Some(&cookie_leitor), None)
        .await;
    assert_eq!(
        (status, json["dados"]["papel"].clone()),
        (StatusCode::OK, json!("leitor"))
    );
    let admin = format!("/api/v1/orgs/{org}/teste/admin");
    let (status, _, json) = app
        .requisicao(Method::POST, &admin, Some(&cookie_leitor), None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["erro"]["codigo"], "proibido");
    let (_, cookie_admin) = app.membro(org, Papel::Admin).await;
    let (status, _, _) = app
        .requisicao(Method::POST, &admin, Some(&cookie_admin), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Organização excluída → 404.
    sqlx::query("UPDATE organizacoes SET excluida_em = now() WHERE id = $1")
        .bind(org)
        .execute(&app.pool)
        .await
        .unwrap();
    let (status, _, _) = app
        .requisicao(Method::GET, &caminho, Some(&cookie_dono), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.encerrar().await;
}

// ----------------------------- envelope -----------------------------

#[tokio::test]
async fn envelope_de_erros() {
    let app = app_teste().await;
    let (status, headers, json) = app
        .requisicao(Method::GET, "/api/v1/nao-existe", None, None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["erro"]["codigo"], "nao_encontrado");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");

    let (status, _, json) = app
        .requisicao(Method::GET, "/api/v1/teste/validacao", None, None)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json,
        json!({"erro": {"codigo": "validacao", "mensagem": "CNPJ inválido", "campos": {"cnpj": "CNPJ inválido"}}})
    );

    let (status, headers, json) = app
        .requisicao(Method::GET, "/api/v1/teste/ocupado", None, None)
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers[header::RETRY_AFTER], "2");
    assert_eq!(json["erro"]["codigo"], "ocupado");

    let (status, _, json) = app
        .requisicao(Method::POST, "/api/v1/teste/duplicado", None, None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["erro"]["codigo"], "conflito");

    // JSON malformado e corpo acima de 64 KiB.
    let (status, _, corpo) = app
        .requisicao_bytes(
            Method::POST,
            "/api/v1/teste/eco",
            None,
            "application/json",
            b"{x".to_vec(),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{corpo:?}");
    let grande = json!({"x": "a".repeat(70 * 1024)});
    let (status, _, json) = app
        .requisicao(Method::POST, "/api/v1/teste/eco", None, Some(grande))
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(json["erro"]["codigo"], "validacao");
    app.encerrar().await;
}

#[tokio::test]
async fn hoje_fixavel_nos_testes() {
    let app = app_teste().await;
    app.fixar_hoje(chrono::NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    let (_, _, json) = app
        .requisicao(Method::GET, "/api/v1/teste/hoje", None, None)
        .await;
    assert_eq!(json["dados"], "2026-03-15");
    app.encerrar().await;
}

// ----------------------------- SPA -----------------------------

#[tokio::test]
async fn spa_serve_arquivos_e_fallback_para_index() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("index.html"),
        "<!doctype html><title>Felix</title>",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/app-123.js"), "console.log(1)").unwrap();
    let app = TestApp::com(Opcoes {
        frontend_dir: Some(dir.path().to_path_buf()),
        ..Opcoes::default()
    })
    .await;
    for caminho in ["/", "/o/123/empenhos", "/entrar"] {
        let r = app.req(Method::GET, caminho).enviar().await;
        assert_eq!(r.status, StatusCode::OK, "{caminho}");
        assert!(r.texto().contains("<title>Felix</title>"), "{caminho}");
        assert_eq!(r.headers[header::CACHE_CONTROL], "no-cache");
    }
    let r = app.req(Method::GET, "/assets/app-123.js").enviar().await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.texto(), "console.log(1)");
    assert!(
        r.headers[header::CACHE_CONTROL]
            .to_str()
            .unwrap()
            .contains("immutable")
    );
    // API não cai no index.html.
    let r = app.req(Method::GET, "/api/v1/qualquer").enviar().await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(r.json["erro"]["codigo"], "nao_encontrado");
    app.encerrar().await;
}

#[tokio::test]
async fn sem_frontend_responde_404_e_api_segue() {
    let app = TestApp::novo().await;
    let r = app.req(Method::GET, "/qualquer").enviar().await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = app.req(Method::GET, "/health").enviar().await;
    assert_eq!(r.status, StatusCode::OK);
    app.encerrar().await;
}

// ----------------------------- upload de PDFs -----------------------------

#[tokio::test]
async fn receber_e_analisar_lote() {
    let app = app_teste().await;
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/empenho");
    let pdf_real = std::fs::read_dir(&raiz)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|x| x == "pdf"))
        .unwrap();
    let bytes = std::fs::read(&pdf_real).unwrap();
    let (status, _, json) = app
        .multipart(
            "/api/v1/teste/analisar",
            None,
            &[
                ("empenho.pdf", bytes.as_slice()),
                ("falso.pdf", b"nao e pdf".as_slice()),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let itens = json["dados"].as_array().unwrap();
    assert_eq!(itens.len(), 2);
    assert_eq!(itens[0]["arquivo"], "empenho.pdf");
    assert_eq!(itens[0]["analise"]["perfil"], "siafi");
    assert!(itens[1]["erro"].is_string());
    assert!(itens[1].get("analise").is_none());

    let (status, _, json) = app.multipart("/api/v1/teste/analisar", None, &[]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["erro"]["codigo"], "validacao");
    app.encerrar().await;
}

// ----------------------------- F1: request-id e IP do cliente -----------------------------

#[tokio::test]
async fn request_id_recebido_e_validado() {
    let app = app_teste().await;
    let r = app
        .req(Method::GET, "/health")
        .cabecalho("x-request-id", "abc-123-XYZ")
        .enviar()
        .await;
    assert_eq!(r.headers["x-request-id"], "abc-123-XYZ");
    for ruim in ["tem espaço", "a\\b", &"a".repeat(65), "x\"y"] {
        let r = app
            .req(Method::GET, "/health")
            .cabecalho("x-request-id", ruim)
            .enviar()
            .await;
        let gerado = r.headers["x-request-id"].to_str().unwrap();
        assert_ne!(gerado, ruim);
        assert!(Uuid::parse_str(gerado).is_ok(), "{gerado}");
    }
    app.encerrar().await;
}

#[tokio::test]
async fn ip_do_cliente_so_vem_de_proxy_confiavel() {
    let app = app_teste().await;
    let ip_de = |origem: &str, cabecalhos: &[(&str, &str)]| {
        let mut req = app
            .req(Method::GET, "/api/v1/teste/cliente")
            .ip(origem.parse().unwrap());
        for (nome, valor) in cabecalhos {
            req = req.cabecalho(nome, valor);
        }
        async move { req.enviar().await.json["dados"]["ip"].clone() }
    };
    let forjado = [("x-real-ip", "1.2.3.4"), ("x-forwarded-for", "5.6.7.8")];
    // Conexão direta da internet: cabeçalhos ignorados.
    assert_eq!(ip_de("203.0.113.7", &forjado).await, "203.0.113.7");
    // Proxy local (loopback, padrão de FELIX_TRUSTED_PROXIES).
    assert_eq!(ip_de("127.0.0.1", &forjado).await, "1.2.3.4");
    assert_eq!(
        ip_de("127.0.0.1", &[("x-forwarded-for", "9.9.9.9, 5.6.7.8")]).await,
        "5.6.7.8"
    );
    assert_eq!(ip_de("127.0.0.1", &[]).await, "127.0.0.1");
    app.encerrar().await;
}
