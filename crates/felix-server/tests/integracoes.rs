//! Integração com o Portal da Transparência contra um servidor falso (wiremock)
//! servindo as fixtures de `felix-transparencia`.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::type_complexity
)]

mod comum;

use axum::http::{Method, StatusCode};
use comum::TestApp;
use felix_server::{
    app::{self, AppState},
    http::Papel,
    jobs, modulos,
};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

const CHAVE: &str = "chave-secreta-teste";
const CNPJ: &str = "19116488000145";
const DOCS: &str = "/api-de-dados/despesas/documentos-por-favorecido";
const IMPACTADOS: &str = "/api-de-dados/despesas/empenhos-impactados";

fn fixture(nome: &str) -> Value {
    let caminho = format!(
        "{}/../felix-transparencia/tests/fixtures/{nome}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(caminho).unwrap()).unwrap()
}

async fn responder(servidor: &MockServer, caminho: &str, filtros: &[(&str, &str)], corpo: Value) {
    let mut mock = Mock::given(method("GET"))
        .and(path(caminho))
        .and(header("chave-api-dados", CHAVE));
    for (nome, valor) in filtros {
        mock = mock.and(query_param(*nome, *valor));
    }
    mock.respond_with(ResponseTemplate::new(200).set_body_json(corpo))
        .mount(servidor)
        .await;
}

/// Portal falso com as fixtures (CNPJ 19116488000145, ano 2026).
async fn portal() -> MockServer {
    let s = MockServer::start().await;
    let docs = |fase: &'static str, pagina: &'static str| {
        [
            ("codigoPessoa", CNPJ),
            ("fase", fase),
            ("ano", "2026"),
            ("pagina", pagina),
        ]
    };
    responder(&s, DOCS, &docs("1", "1"), fixture("empenhos_pagina1.json")).await;
    responder(&s, DOCS, &docs("1", "2"), fixture("empenhos_pagina2.json")).await;
    responder(&s, DOCS, &docs("1", "3"), json!([])).await;
    responder(&s, DOCS, &docs("2", "1"), fixture("liquidacoes.json")).await;
    responder(&s, DOCS, &docs("2", "2"), json!([])).await;
    responder(&s, DOCS, &docs("3", "1"), fixture("pagamentos.json")).await;
    responder(&s, DOCS, &docs("3", "2"), json!([])).await;
    for (codigo, fase) in [
        ("160203000012026NS000123", "2"),
        ("160203000012026OB000456", "3"),
    ] {
        let filtros = [("codigoDocumento", codigo), ("fase", fase), ("pagina", "1")];
        responder(
            &s,
            IMPACTADOS,
            &filtros,
            fixture("empenhos_impactados.json"),
        )
        .await;
        let filtros = [("codigoDocumento", codigo), ("fase", fase), ("pagina", "2")];
        responder(&s, IMPACTADOS, &filtros, json!([])).await;
    }
    s
}

/// Troca a configuração do app para apontar para o Portal falso com chave.
fn configurar(app: &mut TestApp, url: &str) {
    let mut config = (*app.estado.config).clone();
    config.transparencia_key = Some(CHAVE.into());
    config.transparencia_url = url.into();
    app.estado = AppState::novo(app.pool.clone(), config)
        .unwrap()
        .com_relogio(Arc::new(app.relogio.clone()));
    app.router = app::montar(app.estado.clone(), modulos::rotas());
}

struct Cenario {
    org: Uuid,
    cookie: String,
    empresa: Uuid,
    empenho: Uuid,
    nota: Uuid,
}

/// Org com a empresa do CNPJ das fixtures, o empenho 2026NE000290 e a NF 1234 aberta.
async fn cenario(app: &TestApp) -> Cenario {
    app.fixar_hoje("2026-03-10".parse().unwrap());
    let (_, org, cookie) = app.dono_com_org().await;
    let mut tx = app.tenant(org).await;
    let empresa: Uuid = sqlx::query_scalar(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj) VALUES ($1, 'Fornecedor', $2)
         RETURNING id",
    )
    .bind(org)
    .bind(CNPJ)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let empenho: Uuid = sqlx::query_scalar(
        "INSERT INTO empenhos (organizacao_id, empresa_id, ug, numero, valor_total_centavos)
         VALUES ($1, $2, '160203', '2026NE000290', 3245000) RETURNING id",
    )
    .bind(org)
    .bind(empresa)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let nota: Uuid = sqlx::query_scalar(
        "INSERT INTO notas (organizacao_id, empenho_id, numero, tipo, valor_centavos, data_emissao,
                            vencimento, status)
         VALUES ($1, $2, '1234', 'produto', 3245000, '2026-02-01', '2026-03-03', 'emitida')
         RETURNING id",
    )
    .bind(org)
    .bind(empenho)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    Cenario {
        org,
        cookie,
        empresa,
        empenho,
        nota,
    }
}

fn base(org: Uuid) -> String {
    format!("/api/v1/orgs/{org}/integracoes/transparencia")
}

async fn post(
    app: &TestApp,
    caminho: &str,
    cookie: &str,
    corpo: Option<Value>,
) -> (StatusCode, Value) {
    let (status, _, json) = app
        .requisicao(Method::POST, caminho, Some(cookie), corpo)
        .await;
    (status, json)
}

async fn get(app: &TestApp, caminho: &str, cookie: &str) -> (StatusCode, Value) {
    let (status, _, json) = app
        .requisicao(Method::GET, caminho, Some(cookie), None)
        .await;
    (status, json)
}

/// Avança o relógio além do intervalo mínimo entre sincronizações manuais.
fn passar_uma_hora(app: &TestApp) {
    app.relogio
        .definir(app.estado.agora() + chrono::TimeDelta::minutes(61));
}

/// Sincroniza e executa o job; devolve os documentos (todos) por código resumido.
async fn sincronizar(app: &TestApp, c: &Cenario) -> Vec<Value> {
    passar_uma_hora(app);
    let (status, json) = post(
        app,
        &format!("{}/sincronizar", base(c.org)),
        &c.cookie,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{json}");
    assert!(json["dados"]["job_id"].is_string());
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let (status, json) = get(
        app,
        &format!("{}/documentos?situacao=todos&por_pagina=100", base(c.org)),
        &c.cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    json["dados"].as_array().unwrap().clone()
}

fn doc<'a>(docs: &'a [Value], resumido: &str) -> &'a Value {
    docs.iter()
        .find(|d| d["codigo_resumido"] == resumido)
        .unwrap_or_else(|| panic!("documento {resumido} ausente"))
}

#[tokio::test]
async fn sincroniza_sugere_e_vincula() {
    let mut app = TestApp::novo().await;
    let servidor = portal().await;
    configurar(&mut app, &servidor.uri());
    let c = cenario(&app).await;

    let (status, json) = get(&app, &base(c.org), &c.cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["dados"],
        json!({
            "configurada_no_servidor": true, "permitida_no_plano": true, "ativa": false,
            "ultima_sincronizacao_em": null, "ultimo_status": null, "ultimo_erro": null,
            "pendentes": 0, "sincronizando": false
        })
    );

    let (status, json) = post(
        &app,
        &format!("{}/sincronizar", base(c.org)),
        &c.cookie,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{json}");
    let job_id: Uuid = json["dados"]["job_id"].as_str().unwrap().parse().unwrap();
    let (status, json) = post(
        &app,
        &format!("{}/sincronizar", base(c.org)),
        &c.cookie,
        Some(json!({"ano": 2026})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "job duplicado: {json}");
    let (_, json) = get(&app, &base(c.org), &c.cookie).await;
    assert_eq!(json["dados"]["sincronizando"], true);

    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(status, "concluido");
    let (_, json) = get(&app, &base(c.org), &c.cookie).await;
    assert_eq!(json["dados"]["ultimo_status"], "ok");
    assert_eq!(json["dados"]["pendentes"], 5);
    assert!(json["dados"]["ultima_sincronizacao_em"].is_string());

    let (status, json) = get(&app, &format!("{}/documentos", base(c.org)), &c.cookie).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["meta"]["total"], 5);
    let docs = json["dados"].as_array().unwrap().clone();
    let ne290 = doc(&docs, "2026NE000290");
    assert_eq!(ne290["fase"], 1);
    assert_eq!(ne290["empresa_cnpj"], CNPJ);
    assert_eq!(ne290["valor_centavos"], 3_245_000);
    assert_eq!(ne290["ug"], "160203");
    assert_eq!(ne290["data"], "2026-01-15");
    assert_eq!(ne290["empenho_codigo"], "2026NE000290");
    assert_eq!(ne290["situacao"], "novo");
    assert_eq!(ne290["sugestao"]["tipo"], "empenho");
    assert_eq!(ne290["sugestao"]["id"], c.empenho.to_string());
    assert_eq!(
        ne290["sugestao"]["rotulo"],
        "Empenho 2026NE000290 — UG 160203"
    );
    assert!(doc(&docs, "2026NE000291")["sugestao"].is_null());
    let ns = doc(&docs, "2026NS000123");
    assert_eq!(ns["fase"], 2);
    assert_eq!(ns["empenho_codigo"], "2026NE000290");
    assert_eq!(ns["sugestao"]["tipo"], "nota");
    assert_eq!(ns["sugestao"]["id"], c.nota.to_string());
    assert!(
        ns["sugestao"]["rotulo"]
            .as_str()
            .unwrap()
            .contains("NF 1234")
    );
    let ob = doc(&docs, "2026OB000456");
    assert_eq!(
        ob["sugestao"]["id"],
        c.nota.to_string(),
        "única nota aberta"
    );

    let (_, json) = get(
        &app,
        &format!("{}/documentos?fase=2", base(c.org)),
        &c.cookie,
    )
    .await;
    assert_eq!(json["meta"]["total"], 1);
    let (status, _) = get(
        &app,
        &format!("{}/documentos?fase=9", base(c.org)),
        &c.cookie,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Fase 1 → empenho.
    let docs_url = format!("{}/documentos", base(c.org));
    let id = ne290["id"].as_str().unwrap();
    let (status, json) = post(
        &app,
        &format!("{docs_url}/{id}/vincular"),
        &c.cookie,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    let (status, json) = post(
        &app,
        &format!("{docs_url}/{id}/vincular"),
        &c.cookie,
        Some(json!({"empenho_id": c.empenho})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["situacao"], "vinculado");
    assert_eq!(json["dados"]["empenho_id"], c.empenho.to_string());
    let (status, _) = post(
        &app,
        &format!("{docs_url}/{id}/vincular"),
        &c.cookie,
        Some(json!({"empenho_id": c.empenho})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "já vinculado");

    // Liquidação → evento liquidada.
    let id_ns = ns["id"].as_str().unwrap();
    let (status, json) = post(
        &app,
        &format!("{docs_url}/{id_ns}/vincular"),
        &c.cookie,
        Some(json!({"nota_id": c.nota})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["nota_id"], c.nota.to_string());
    // Pagamento (líquido de retenções) → evento paga.
    let id_ob = ob["id"].as_str().unwrap();
    let (status, json) = post(
        &app,
        &format!("{docs_url}/{id_ob}/vincular"),
        &c.cookie,
        Some(json!({"nota_id": c.nota})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");

    let mut tx = app.tenant(c.org).await;
    let nota: (
        String,
        i64,
        Option<chrono::NaiveDate>,
        Option<chrono::NaiveDate>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT status, valor_pago_centavos, data_liquidacao, data_pagamento,
                    documento_liquidacao, documento_pagamento FROM notas WHERE id = $1",
    )
    .bind(c.nota)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(nota.0, "paga");
    assert_eq!(nota.1, 2_938_348);
    assert_eq!(nota.2.unwrap().to_string(), "2026-02-10");
    assert_eq!(nota.3.unwrap().to_string(), "2026-02-25");
    assert_eq!(nota.4.as_deref(), Some("2026NS000123"));
    assert_eq!(nota.5.as_deref(), Some("2026OB000456"));
    let eventos: Vec<(String, String, Option<i64>, Option<String>, String)> = sqlx::query_as(
        "SELECT tipo, data::text, valor_centavos, documento, origem FROM eventos_nota
         WHERE nota_id = $1 ORDER BY criado_em, id",
    )
    .bind(c.nota)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    drop(tx);
    assert_eq!(
        eventos,
        [
            (
                "liquidada".into(),
                "2026-02-10".into(),
                Some(3_245_000),
                Some("2026NS000123".into()),
                "transparencia".into()
            ),
            (
                "paga".into(),
                "2026-02-25".into(),
                Some(2_938_348),
                Some("2026OB000456".into()),
                "transparencia".into()
            ),
        ]
    );

    // Nova sincronização preserva vínculos e situação.
    let docs = sincronizar(&app, &c).await;
    assert_eq!(docs.len(), 5);
    assert_eq!(doc(&docs, "2026NE000290")["situacao"], "vinculado");
    assert_eq!(doc(&docs, "2026OB000456")["nota_id"], c.nota.to_string());
    assert_eq!(doc(&docs, "2026NS000123")["empenho_codigo"], "2026NE000290");
    let (_, json) = get(&app, &base(c.org), &c.cookie).await;
    assert_eq!(json["dados"]["pendentes"], 2);
    // A chave foi enviada no cabeçalho e nunca aparece na resposta.
    assert!(!json.to_string().contains(CHAVE));
    let _ = c.empresa;
    app.encerrar().await;
}

#[tokio::test]
async fn importa_ignora_e_respeita_estados() {
    let mut app = TestApp::novo().await;
    let servidor = portal().await;
    configurar(&mut app, &servidor.uri());
    let c = cenario(&app).await;
    // NE000292 já cadastrado com outro formato de número → importar dá 409.
    let mut tx = app.tenant(c.org).await;
    sqlx::query(
        "INSERT INTO empenhos (organizacao_id, empresa_id, ug, numero, valor_total_centavos)
         VALUES ($1, $2, '160203', '2026NE292', 1335100)",
    )
    .bind(c.org)
    .bind(c.empresa)
    .execute(&mut *tx)
    .await
    .unwrap();
    // Nota cancelada do mesmo empenho para testar transição inválida.
    let cancelada: Uuid = sqlx::query_scalar(
        "INSERT INTO notas (organizacao_id, empenho_id, numero, tipo, valor_centavos, data_emissao,
                            vencimento, status)
         VALUES ($1, $2, '999', 'produto', 1000, '2026-02-01', '2026-03-03', 'cancelada') RETURNING id",
    )
    .bind(c.org)
    .bind(c.empenho)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let docs = sincronizar(&app, &c).await;
    let docs_url = format!("{}/documentos", base(c.org));
    let (_, cookie_leitor) = app.membro(c.org, Papel::Leitor).await;

    let ne291 = doc(&docs, "2026NE000291")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, _) = post(
        &app,
        &format!("{docs_url}/{ne291}/importar"),
        &cookie_leitor,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, json) = post(
        &app,
        &format!("{docs_url}/{ne291}/importar"),
        &c.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    let empenho = &json["dados"];
    assert_eq!(empenho["origem"], "transparencia");
    assert_eq!(empenho["ug"], "160203");
    assert_eq!(empenho["numero"], "2026NE000291");
    assert_eq!(empenho["valor_total_centavos"], 3_665_730);
    assert_eq!(empenho["data_emissao"], "2026-01-15");
    assert_eq!(empenho["gestao"], "00001");
    assert_eq!(empenho["empresa_id"], c.empresa.to_string());
    assert_eq!(empenho["ug_nome"], "UNIDADE GESTORA 160203");
    let (status, _) = post(
        &app,
        &format!("{docs_url}/{ne291}/importar"),
        &c.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "documento já vinculado");

    let ne292 = doc(&docs, "2026NE000292")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, json) = post(
        &app,
        &format!("{docs_url}/{ne292}/importar"),
        &c.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert!(
        json["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("vincular")
    );

    let ns = doc(&docs, "2026NS000123")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, _) = post(&app, &format!("{docs_url}/{ns}/importar"), &c.cookie, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "só fase 1");
    let (status, json) = post(
        &app,
        &format!("{docs_url}/{ns}/vincular"),
        &c.cookie,
        Some(json!({"nota_id": cancelada})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "nota cancelada: {json}");
    let (status, _) = post(
        &app,
        &format!("{docs_url}/{ns}/vincular"),
        &c.cookie,
        Some(json!({"nota_id": Uuid::now_v7()})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, json) = post(
        &app,
        &format!("{docs_url}/{ne292}/ignorar"),
        &c.cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["situacao"], "ignorado");
    let (_, json) = get(&app, &docs_url, &c.cookie).await;
    let restantes: Vec<&str> = json["dados"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["codigo_resumido"].as_str().unwrap())
        .collect();
    assert!(!restantes.contains(&"2026NE000292"));
    assert!(!restantes.contains(&"2026NE000291"));
    let (_, json) = get(&app, &format!("{docs_url}?situacao=ignorado"), &c.cookie).await;
    assert_eq!(json["meta"]["total"], 1);

    // Outra organização não enxerga nem age sobre os documentos.
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let (status, _) = get(&app, &docs_url, &cookie_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(
        &app,
        &format!("{}/documentos/{ns}/ignorar", base(org_b)),
        &cookie_b,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, json) = get(
        &app,
        &format!("{}/documentos?situacao=todos", base(org_b)),
        &cookie_b,
    )
    .await;
    assert_eq!(json["meta"]["total"], 0);
    app.encerrar().await;
}

#[tokio::test]
async fn sem_chave_responde_503_e_plano_gratuito_402() {
    let app = TestApp::novo().await;
    let (_, org, cookie) = app.dono_com_org().await;
    let (status, json) = post(
        &app,
        &format!("{}/sincronizar", base(org)),
        &cookie,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["erro"]["codigo"], "indisponivel");
    assert_eq!(
        json["erro"]["mensagem"],
        "Integração não configurada no servidor."
    );
    let (_, json) = get(&app, &base(org), &cookie).await;
    assert_eq!(json["dados"]["configurada_no_servidor"], false);
    app.encerrar().await;

    let mut app = TestApp::novo().await;
    let servidor = portal().await;
    configurar(&mut app, &servidor.uri());
    let (_, org, cookie) = app.dono_com_org().await;
    let (_, cookie_operador) = app.membro(org, Papel::Operador).await;
    let (status, _) = post(
        &app,
        &format!("{}/sincronizar", base(org)),
        &cookie_operador,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(
        &app,
        &format!("{}/sincronizar", base(org)),
        &cookie,
        Some(json!({"ano": 1999})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    sqlx::query(
        "UPDATE assinaturas SET plano = 'gratuito', status = 'ativa' WHERE organizacao_id = $1",
    )
    .bind(org)
    .execute(&app.pool)
    .await
    .unwrap();
    let (status, json) = post(
        &app,
        &format!("{}/sincronizar", base(org)),
        &cookie,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED, "{json}");
    let (_, json) = get(&app, &base(org), &cookie).await;
    assert_eq!(json["dados"]["permitida_no_plano"], false);
    let (status, _, _) = app
        .requisicao(
            Method::PATCH,
            &base(org),
            Some(&cookie),
            Some(json!({"ativa": true})),
        )
        .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
    app.encerrar().await;
}

#[tokio::test]
async fn ativa_integracao_e_registra_erro_do_portal_sem_a_chave() {
    let mut app = TestApp::novo().await;
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&servidor)
        .await;
    configurar(&mut app, &servidor.uri());
    let c = cenario(&app).await;
    let (_, cookie_leitor) = app.membro(c.org, Papel::Leitor).await;
    let (status, _, _) = app
        .requisicao(
            Method::PATCH,
            &base(c.org),
            Some(&cookie_leitor),
            Some(json!({"ativa": true})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, json) = app
        .requisicao(
            Method::PATCH,
            &base(c.org),
            Some(&c.cookie),
            Some(json!({"ativa": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["ativa"], true);

    // Rodada diária enfileira a sincronização das integrações ativas.
    assert_eq!(
        modulos::integracoes::agendar_sincronizacoes_ativas(&app.estado)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        modulos::integracoes::agendar_sincronizacoes_ativas(&app.estado)
            .await
            .unwrap(),
        0,
        "já há uma pendente"
    );
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let (_, json) = get(&app, &base(c.org), &c.cookie).await;
    assert_eq!(json["dados"]["ultimo_status"], "erro");
    let erro = json["dados"]["ultimo_erro"].as_str().unwrap();
    assert!(erro.contains("chave"), "{erro}");
    assert!(!erro.contains(CHAVE));
    let (status, erro_job): (String, Option<String>) =
        sqlx::query_as("SELECT status, erro FROM jobs WHERE organizacao_id = $1")
            .bind(c.org)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(status, "falhou", "chave recusada não adianta repetir");
    assert!(!erro_job.unwrap().contains(CHAVE));
    app.encerrar().await;
}

async fn status_integracao(app: &TestApp, c: &Cenario) -> (String, Option<String>) {
    let (_, json) = get(app, &base(c.org), &c.cookie).await;
    (
        json["dados"]["ultimo_status"].as_str().unwrap().to_owned(),
        json["dados"]["ultimo_erro"].as_str().map(str::to_owned),
    )
}

async fn total_documentos(app: &TestApp, org: Uuid) -> i64 {
    let mut tx = app.tenant(org).await;
    let n = sqlx::query_scalar("SELECT count(*) FROM documentos_transparencia")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    drop(tx);
    n
}

#[tokio::test]
async fn sincronizacao_manual_no_maximo_uma_por_hora() {
    let mut app = TestApp::novo().await;
    let servidor = portal().await;
    configurar(&mut app, &servidor.uri());
    let c = cenario(&app).await;
    let url = format!("{}/sincronizar", base(c.org));
    let (status, json) = post(&app, &url, &c.cookie, Some(json!({}))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{json}");
    // O job e a auditoria foram gravados juntos.
    let job_id: Uuid = json["dados"]["job_id"].as_str().unwrap().parse().unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let status_job: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(status_job, "concluido");

    // Mesmo concluída, outra manual só depois de 1 hora.
    app.relogio
        .definir(app.estado.agora() + chrono::TimeDelta::minutes(59));
    let (status, json) = post(&app, &url, &c.cookie, Some(json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert!(
        json["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("uma vez por hora"),
        "{json}"
    );
    // Recusada: nenhum job novo.
    let jobs_org: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE organizacao_id = $1")
        .bind(c.org)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(jobs_org, 1);
    // A automática diária não é afetada pelo limite.
    let mut tx = app.tenant(c.org).await;
    sqlx::query("UPDATE integracoes SET ativa = true WHERE organizacao_id = $1")
        .bind(c.org)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        modulos::integracoes::agendar_sincronizacoes_ativas(&app.estado)
            .await
            .unwrap(),
        1
    );
    assert!(jobs::executar_um(&app.estado).await.unwrap());

    app.relogio
        .definir(app.estado.agora() + chrono::TimeDelta::minutes(2));
    let (status, json) = post(&app, &url, &c.cookie, Some(json!({}))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{json}");
    app.encerrar().await;
}

#[tokio::test]
async fn falha_no_meio_preserva_o_obtido_e_fica_parcial() {
    let mut app = TestApp::novo().await;
    let s = MockServer::start().await;
    let docs = |fase: &'static str, pagina: &'static str| {
        [
            ("codigoPessoa", CNPJ),
            ("fase", fase),
            ("ano", "2026"),
            ("pagina", pagina),
        ]
    };
    responder(&s, DOCS, &docs("1", "1"), fixture("empenhos_pagina1.json")).await;
    responder(&s, DOCS, &docs("1", "2"), json!([])).await;
    Mock::given(method("GET"))
        .and(path(DOCS))
        .and(query_param("fase", "2"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&s)
        .await;
    configurar(&mut app, &s.uri());
    let c = cenario(&app).await;
    let job = jobs::enfileirar(
        &app.pool,
        jobs::TIPO_TRANSPARENCIA,
        Some(c.org),
        json!({"org_id": c.org, "ano": 2026}),
    )
    .await
    .unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    // Os empenhos (fase 1) ficaram gravados.
    assert_eq!(total_documentos(&app, c.org).await, 2);
    let (status, erro) = status_integracao(&app, &c).await;
    assert_eq!(status, "parcial");
    assert!(erro.unwrap().contains("indisponível"));
    // Portal fora do ar é temporário: o job será repetido.
    let status_job: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
        .bind(job)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(status_job, "pendente");
    app.encerrar().await;
}

#[tokio::test]
async fn truncamento_e_orcamento_deixam_status_parcial() {
    // Portal que nunca devolve página vazia: o cliente para no máximo de páginas.
    let mut app = TestApp::novo().await;
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DOCS))
        .and(query_param("fase", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("empenhos_pagina2.json")))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path(DOCS))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&s)
        .await;
    configurar(&mut app, &s.uri());
    let c = cenario(&app).await;
    let resumo = modulos::integracoes::sincronizar(&app.estado, c.org, 2026)
        .await
        .unwrap();
    assert!(resumo.parcial);
    assert_eq!(
        resumo.requisicoes,
        felix_transparencia::MAX_PAGINAS_PADRAO + 2
    );
    let (status, aviso) = status_integracao(&app, &c).await;
    assert_eq!(status, "parcial");
    assert!(aviso.unwrap().contains("limite"), "aviso de truncamento");

    // Orçamento de requisições: para no meio e avisa.
    let resumo =
        modulos::integracoes::sincronizacao::sincronizar_com_orcamento(&app.estado, c.org, 2026, 3)
            .await
            .unwrap();
    assert!(resumo.parcial);
    assert_eq!(resumo.requisicoes, 3);
    let (status, aviso) = status_integracao(&app, &c).await;
    assert_eq!(status, "parcial");
    assert!(aviso.unwrap().contains("Limite de 3 requisições"));
    assert_eq!(total_documentos(&app, c.org).await, 1);
    app.encerrar().await;
}

#[tokio::test]
async fn empenhos_impactados_sao_consultados_uma_vez() {
    let mut app = TestApp::novo().await;
    let s = MockServer::start().await;
    let docs = |fase: &'static str, pagina: &'static str| {
        [
            ("codigoPessoa", CNPJ),
            ("fase", fase),
            ("ano", "2026"),
            ("pagina", pagina),
        ]
    };
    // Liquidação sem número de empenho na observação.
    let mut liquidacoes = fixture("liquidacoes.json");
    liquidacoes[0]["observacao"] = json!("LIQUIDACAO SEM REFERENCIA");
    responder(&s, DOCS, &docs("1", "1"), json!([])).await;
    responder(&s, DOCS, &docs("2", "1"), liquidacoes).await;
    responder(&s, DOCS, &docs("2", "2"), json!([])).await;
    responder(&s, DOCS, &docs("3", "1"), json!([])).await;
    Mock::given(method("GET"))
        .and(path(IMPACTADOS))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&s)
        .await;
    configurar(&mut app, &s.uri());
    let c = cenario(&app).await;
    for _ in 0..2 {
        let resumo = modulos::integracoes::sincronizar(&app.estado, c.org, 2026)
            .await
            .unwrap();
        assert_eq!(resumo.documentos, 1);
        assert!(!resumo.parcial);
    }
    let mut tx = app.tenant(c.org).await;
    let (empenho, consultado): (Option<String>, bool) = sqlx::query_as(
        "SELECT empenho_codigo, impactados_consultados FROM documentos_transparencia",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    drop(tx);
    assert_eq!((empenho, consultado), (None, true));
    s.verify().await;
    app.encerrar().await;
}

/// Sincroniza só a liquidação e o pagamento das fixtures e devolve os ids.
async fn documentos_de_execucao(app: &TestApp, c: &Cenario) -> (String, String) {
    let docs = sincronizar(app, c).await;
    (
        doc(&docs, "2026NS000123")["id"]
            .as_str()
            .unwrap()
            .to_owned(),
        doc(&docs, "2026OB000456")["id"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
}

async fn mudar_documento(app: &TestApp, org: Uuid, id: &str, sql: &str) {
    let mut tx = app.tenant(org).await;
    sqlx::query(&format!(
        "UPDATE documentos_transparencia SET {sql} WHERE id = $1::uuid"
    ))
    .bind(id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn vincular_aplica_as_regras_da_transicao_manual() {
    let mut app = TestApp::novo().await;
    let servidor = portal().await;
    configurar(&mut app, &servidor.uri());
    let c = cenario(&app).await;
    let (ns, ob) = documentos_de_execucao(&app, &c).await;
    let docs_url = format!("{}/documentos", base(c.org));
    let vincular = |id: String| {
        let url = format!("{docs_url}/{id}/vincular");
        let cookie = c.cookie.clone();
        let nota = c.nota;
        let app = &app;
        async move { post(app, &url, &cookie, Some(json!({"nota_id": nota}))).await }
    };

    // Data anterior à emissão da nota (01/02/2026) → 422 com campo data.
    mudar_documento(&app, c.org, &ns, "data = '2026-01-20'").await;
    let (status, json) = vincular(ns.clone()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    assert!(json["erro"]["campos"]["data"].is_string(), "{json}");
    // Data futura → 422.
    mudar_documento(&app, c.org, &ns, "data = '2026-04-01'").await;
    let (status, json) = vincular(ns.clone()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    // Liquidação válida.
    mudar_documento(&app, c.org, &ns, "data = '2026-02-10'").await;
    let (status, json) = vincular(ns.clone()).await;
    assert_eq!(status, StatusCode::OK, "{json}");

    // Pagamento antes da liquidação → 422.
    mudar_documento(&app, c.org, &ob, "data = '2026-02-05'").await;
    let (status, json) = vincular(ob.clone()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    assert!(
        json["erro"]["mensagem"]
            .as_str()
            .unwrap()
            .contains("liquidação")
            || json["erro"]["campos"]["data"]
                .as_str()
                .unwrap_or_default()
                .contains("liquidação"),
        "{json}"
    );
    // Pagamento maior que a nota: limitado ao valor da nota, com observação.
    mudar_documento(
        &app,
        c.org,
        &ob,
        "data = '2026-02-25', valor_centavos = 9999999",
    )
    .await;
    let (status, json) = vincular(ob.clone()).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let mut tx = app.tenant(c.org).await;
    let (status_nota, pago): (String, i64) =
        sqlx::query_as("SELECT status, valor_pago_centavos FROM notas WHERE id = $1")
            .bind(c.nota)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    let (valor_evento, observacao): (Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT valor_centavos, observacao FROM eventos_nota
         WHERE nota_id = $1 AND tipo = 'paga'",
    )
    .bind(c.nota)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    drop(tx);
    assert_eq!((status_nota.as_str(), pago), ("paga", 3_245_000));
    assert_eq!(valor_evento, Some(3_245_000));
    let observacao = observacao.unwrap();
    assert!(observacao.contains("2026OB000456") || observacao.contains("Portal"));
    assert!(observacao.contains("99.999,99"), "{observacao}");
    app.encerrar().await;
}

#[tokio::test]
async fn sugestoes_consideram_apenas_a_pagina() {
    let mut app = TestApp::novo().await;
    let servidor = portal().await;
    configurar(&mut app, &servidor.uri());
    let c = cenario(&app).await;
    let docs = sincronizar(&app, &c).await;
    assert_eq!(docs.len(), 5);
    // Página com um documento só: a sugestão continua correta.
    let (_, json) = get(
        &app,
        &format!("{}/documentos?fase=1&por_pagina=1&pagina=3", base(c.org)),
        &c.cookie,
    )
    .await;
    let itens = json["dados"].as_array().unwrap();
    assert_eq!(itens.len(), 1);
    assert_eq!(itens[0]["codigo_resumido"], "2026NE000290");
    assert_eq!(itens[0]["sugestao"]["id"], c.empenho.to_string());
    let (_, json) = get(
        &app,
        &format!("{}/documentos?fase=3&por_pagina=1", base(c.org)),
        &c.cookie,
    )
    .await;
    assert_eq!(json["dados"][0]["sugestao"]["id"], c.nota.to_string());
    app.encerrar().await;
}
