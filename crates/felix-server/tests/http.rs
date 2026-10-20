use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use felix_server::{
    config::Config,
    web::{Estado, router},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn config(path: &std::path::Path) -> Config {
    Config {
        bind: std::net::SocketAddr::from(([127, 0, 0, 1], 8080)),
        database_path: path.into(),
        token: None,
        empresa_nome: "Teste".into(),
        empresa_cnpj: "12345678000190".into(),
    }
}

#[tokio::test]
async fn token_protege_api_e_health_permanece_disponivel() {
    let temp = tempfile::tempdir().unwrap();
    let mut cfg = config(&temp.path().join("db"));
    cfg.token = Some("test-token-with-more-than-32-characters".into());
    let app = router(Estado::novo(cfg).unwrap());
    let response = app
        .clone()
        .oneshot(Request::get("/api/empresa").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/empresa")
                .header(
                    "authorization",
                    "Bearer test-token-with-more-than-32-characters",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn multipart_real_e_corrompido_tem_resultado_individual() {
    let temp = tempfile::tempdir().unwrap();
    let app = router(Estado::novo(config(&temp.path().join("db"))).unwrap());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/empenho");
    let file = std::fs::read_dir(path)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut body=b"--testboundary\r\nContent-Disposition: form-data; name=\"files\"; filename=\"a.pdf\"\r\nContent-Type: application/pdf\r\n\r\n".to_vec();
    body.extend(std::fs::read(file).unwrap());
    body.extend(b"\r\n--testboundary\r\nContent-Disposition: form-data; name=\"files\"; filename=\"bad.pdf\"\r\n\r\nnot a PDF\r\n--testboundary--\r\n");
    let response = app
        .oneshot(
            Request::post("/api/analisar")
                .header("content-type", "multipart/form-data; boundary=testboundary")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        json.pointer("/resultados/0/analise/perfil")
            .and_then(serde_json::Value::as_str),
        Some("siafi")
    );
    assert!(
        json.pointer("/resultados/1/erro")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|erro| erro.contains("PDF"))
    );
}

#[tokio::test]
async fn corpo_grande_origem_estranha_e_fila_cheia_sao_recusados() {
    let temp = tempfile::tempdir().unwrap();
    let estado = Estado::novo(config(&temp.path().join("db"))).unwrap();
    let app = router(estado.clone());
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/empenhos")
                .header("content-type", "application/json")
                .body(Body::from("x".repeat(20_000)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/empresa")
                .header("host", "localhost:8080")
                .header("origin", "https://example.invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let _ocupadas = estado.admissao.clone().acquire_many_owned(8).await.unwrap();
    let response = app
        .oneshot(
            Request::post("/api/analisar")
                .header("content-type", "multipart/form-data; boundary=b")
                .body(Body::from("--b--\r\n"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers().get("retry-after").unwrap(), "2");
}

#[test]
fn bind_externo_sem_token_nao_inicia() {
    let mut cfg = config(std::path::Path::new("unused.db"));
    cfg.bind = "0.0.0.0:8080".parse().unwrap();
    assert!(cfg.validar().is_err());
    cfg.token = Some("short".into());
    assert!(cfg.validar().is_err());
    cfg.token = Some("long-enough-test-token-with-32-characters".into());
    cfg.empresa_cnpj = "invalid".into();
    assert!(cfg.validar().is_err());
}

#[tokio::test]
async fn cadastro_http_nota_saldo_e_download_de_oficio() {
    let temp = tempfile::tempdir().unwrap();
    let app = router(Estado::novo(config(&temp.path().join("db"))).unwrap());
    let empenho = serde_json::json!({"ug":"160203","gestao":null,"empenho":"2026NE1","cnpj_fornecedor":"12345678000190","modalidade":"Pregão","numero_edital_ata":"1/2026","valor_total_centavos":10000});
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/empenhos")
                .header("content-type", "application/json")
                .body(Body::from(empenho.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = app
        .clone()
        .oneshot(Request::get("/api/empenhos").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/empenhos")
                .header("content-type", "application/json")
                .body(Body::from(empenho.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let mut nota = serde_json::json!({"numero_nf":"42","tipo_documento":"Nota Fiscal (Produtos)","valor_nf_centavos":6000,"data_emissao_nf":"2020-01-01","prazo_dias_pagamento":30});
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/empenhos/1/notas")
                .header("content-type", "application/json")
                .body(Body::from(nota.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    *nota.get_mut("valor_nf_centavos").unwrap() = serde_json::json!(5000);
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/empenhos/1/notas")
                .header("content-type", "application/json")
                .body(Body::from(nota.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = app
        .clone()
        .oneshot(Request::get("/api/notas").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/notas/1/oficio?multa_bps=200&juros_bps=100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Oficio_Cobranca_1.txt")
    );
    let text = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains("NF nº 42"));
    assert!(text.contains("Multa informada (2,00%): R$ 1,20"));
    assert!(text.contains("Teste (CNPJ:"));
    let response = app
        .oneshot(
            Request::get("/api/notas/999/oficio")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
