//! Exportação CSV: BOM, `;`, valores `1234,56`, datas dd/mm/aaaa, fórmula neutralizada.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod comum;

use axum::http::{Method, StatusCode, header};
use chrono::NaiveDate;
use comum::TestApp;
use felix_server::http::Papel;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn exporta_empenhos_e_notas_em_csv() {
    let app = TestApp::novo().await;
    app.fixar_hoje(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
    let (_, org, cookie) = app.dono_com_org().await;
    let mut tx = app.tenant(org).await;
    let empresa: Uuid = sqlx::query_scalar(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj)
         VALUES ($1, 'Fornecedora; \"Aspas\" Ltda', '19116488000145') RETURNING id",
    )
    .bind(org)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let r = app
        .req(Method::POST, &format!("/api/v1/orgs/{org}/empenhos"))
        .cookie(&cookie)
        .json(
            json!({"empresa_id": empresa, "ug": "160203", "ug_nome": "=HYPERLINK(\"http://x\")",
                     "numero": "2026NE000290", "valor_total_centavos": 123_456_789,
                     "data_emissao": "2026-06-15", "observacao": "@SUM(A1)"}),
        )
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    let empenho = r.json["dados"]["id"].as_str().unwrap().to_owned();
    let r = app
        .req(
            Method::POST,
            &format!("/api/v1/orgs/{org}/empenhos/{empenho}/notas"),
        )
        .cookie(&cookie)
        .json(
            json!({"numero": "-55", "tipo": "servico", "valor_centavos": 123_456,
                     "data_emissao": "2026-09-01", "data_recebimento": "2026-09-03"}),
        )
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);

    let (_, leitor) = app.membro(org, Papel::Leitor).await;
    let r = app
        .req(
            Method::GET,
            &format!("/api/v1/orgs/{org}/exportar/empenhos.csv"),
        )
        .cookie(&leitor)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers[header::CONTENT_TYPE], "text/csv; charset=utf-8");
    assert_eq!(
        r.headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"empenhos_2026-10-20.csv\""
    );
    assert!(r.corpo.starts_with(b"\xEF\xBB\xBF"));
    let texto = String::from_utf8(r.corpo[3..].to_vec()).unwrap();
    let linhas: Vec<&str> = texto.lines().collect();
    assert_eq!(linhas.len(), 2, "{texto}");
    assert!(
        linhas[0].starts_with("Empresa;CNPJ;UG;Nome da UG;Órgão;Gestão;Empenho;"),
        "{}",
        linhas[0]
    );
    let mut leitor_csv = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_reader(texto.as_bytes());
    let cab = leitor_csv.headers().unwrap().clone();
    let linha = leitor_csv.records().next().unwrap().unwrap();
    let campo = |nome: &str| linha[cab.iter().position(|c| c == nome).unwrap()].to_owned();
    assert_eq!(campo("Empresa"), "Fornecedora; \"Aspas\" Ltda");
    assert_eq!(campo("CNPJ"), "19116488000145");
    assert_eq!(campo("Nome da UG"), "'=HYPERLINK(\"http://x\")");
    assert_eq!(campo("Observação"), "'@SUM(A1)");
    assert_eq!(campo("Emissão"), "15/06/2026");
    assert_eq!(campo("Valor total (R$)"), "1234567,89");
    assert_eq!(campo("Faturado (R$)"), "1234,56");
    assert_eq!(campo("Saldo (R$)"), "1233333,33");
    assert_eq!(campo("Situação"), "Ativo");

    let r = app
        .req(
            Method::GET,
            &format!("/api/v1/orgs/{org}/exportar/notas.csv"),
        )
        .cookie(&leitor)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.corpo.starts_with(b"\xEF\xBB\xBF"));
    let texto = String::from_utf8(r.corpo[3..].to_vec()).unwrap();
    let mut leitor_csv = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_reader(texto.as_bytes());
    let cab = leitor_csv.headers().unwrap().clone();
    assert_eq!(&cab[0], "Empenho");
    let linhas: Vec<_> = leitor_csv.records().map(Result::unwrap).collect();
    assert_eq!(linhas.len(), 1);
    let campo = |nome: &str| linhas[0][cab.iter().position(|c| c == nome).unwrap()].to_owned();
    assert_eq!(campo("Nota fiscal"), "'-55");
    assert_eq!(campo("Tipo"), "Serviço");
    assert_eq!(campo("Valor (R$)"), "1234,56");
    assert_eq!(campo("Emissão"), "01/09/2026");
    assert_eq!(campo("Recebimento"), "03/09/2026");
    assert_eq!(campo("Vencimento"), "01/10/2026");
    assert_eq!(campo("Dias de atraso"), "19");
    assert_eq!(campo("Situação"), "Emitida");

    // Outra organização: só cabeçalho; não membro → 404.
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    for arquivo in ["empenhos.csv", "notas.csv"] {
        let r = app
            .req(
                Method::GET,
                &format!("/api/v1/orgs/{org_b}/exportar/{arquivo}"),
            )
            .cookie(&cookie_b)
            .enviar()
            .await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(
            String::from_utf8_lossy(&r.corpo).lines().count(),
            1,
            "{arquivo}"
        );
        let r = app
            .req(
                Method::GET,
                &format!("/api/v1/orgs/{org}/exportar/{arquivo}"),
            )
            .cookie(&cookie_b)
            .enviar()
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
    }
    app.encerrar().await;
}

#[tokio::test]
async fn exportacao_acima_do_limite_pede_filtro() {
    let app = TestApp::novo().await;
    app.fixar_hoje(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap());
    let (_, org, cookie) = app.dono_com_org().await;
    let mut tx = app.tenant(org).await;
    let empresa: Uuid = sqlx::query_scalar(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj)
         VALUES ($1, 'F', '19116488000145') RETURNING id",
    )
    .bind(org)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let empenho: Uuid = sqlx::query_scalar(
        "INSERT INTO empenhos (organizacao_id, empresa_id, ug, numero, valor_total_centavos)
         VALUES ($1, $2, '160203', '2026NE000290', 100000000) RETURNING id",
    )
    .bind(org)
    .bind(empresa)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO notas (organizacao_id, empenho_id, numero, tipo, valor_centavos, data_emissao,
                            vencimento)
         SELECT $1, $2, g::text, 'produto', 1, '2026-01-01', '2026-02-01'
         FROM generate_series(1, $3) AS g",
    )
    .bind(org)
    .bind(empenho)
    .bind(i32::try_from(felix_server::modulos::exportar::MAX_LINHAS + 1).unwrap())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let r = app
        .req(
            Method::GET,
            &format!("/api/v1/orgs/{org}/exportar/notas.csv"),
        )
        .cookie(&cookie)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        r.json["erro"]["mensagem"].as_str().unwrap().contains("ano"),
        "{}",
        r.json
    );
    // Com filtro por ano de emissão, cabe no limite.
    let r = app
        .req(
            Method::GET,
            &format!("/api/v1/orgs/{org}/exportar/notas.csv?ano=2025"),
        )
        .cookie(&cookie)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(String::from_utf8_lossy(&r.corpo[3..]).lines().count(), 1);
    let r = app
        .req(
            Method::GET,
            &format!("/api/v1/orgs/{org}/exportar/empenhos.csv?ano=2026"),
        )
        .cookie(&cookie)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let r = app
        .req(
            Method::GET,
            &format!("/api/v1/orgs/{org}/exportar/empenhos.csv?ano=abc"),
        )
        .cookie(&cookie)
        .enviar()
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    app.encerrar().await;
}
