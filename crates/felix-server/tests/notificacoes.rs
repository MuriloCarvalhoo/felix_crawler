//! Alertas diários (vencida / vence_5d), e-mail-resumo e endpoints de notificações.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod comum;

use axum::http::{Method, StatusCode};
use chrono::NaiveDate;
use comum::TestApp;
use felix_server::{http::Papel, jobs, modulos::notificacoes::alertas};
use serde_json::Value;
use uuid::Uuid;

fn data(texto: &str) -> NaiveDate {
    texto.parse().unwrap()
}

const HOJE: &str = "2026-03-10";

/// Empresa + empenho; devolve o id do empenho.
async fn empenho(app: &TestApp, org: Uuid) -> Uuid {
    let mut tx = app.tenant(org).await;
    let empresa: Uuid = sqlx::query_scalar(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj)
         VALUES ($1, 'Fornecedor', '19116488000145') RETURNING id",
    )
    .bind(org)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let id = sqlx::query_scalar(
        "INSERT INTO empenhos (organizacao_id, empresa_id, ug, numero, valor_total_centavos)
         VALUES ($1, $2, '160203', '2026NE000290', 10000000) RETURNING id",
    )
    .bind(org)
    .bind(empresa)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn nota(
    app: &TestApp,
    org: Uuid,
    empenho: Uuid,
    numero: &str,
    vencimento: &str,
    status: &str,
) -> Uuid {
    let mut tx = app.tenant(org).await;
    let id = sqlx::query_scalar(
        "INSERT INTO notas (organizacao_id, empenho_id, numero, tipo, valor_centavos, data_emissao,
                            vencimento, status)
         VALUES ($1, $2, $3, 'produto', 123456, '2026-01-01', $4, $5) RETURNING id",
    )
    .bind(org)
    .bind(empenho)
    .bind(numero)
    .bind(data(vencimento))
    .bind(status)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// Cenário: A vencida, B e C vencem em até 5 dias, D vence em 6, E paga e F cancelada.
async fn cenario(app: &TestApp, org: Uuid) -> [Uuid; 3] {
    let e = empenho(app, org).await;
    let a = nota(app, org, e, "100", "2026-03-01", "emitida").await;
    let b = nota(app, org, e, "200", "2026-03-10", "liquidada").await;
    let c = nota(app, org, e, "300", "2026-03-15", "emitida").await;
    nota(app, org, e, "400", "2026-03-16", "emitida").await;
    nota(app, org, e, "500", "2026-03-01", "paga").await;
    nota(app, org, e, "600", "2026-03-01", "cancelada").await;
    [a, b, c]
}

async fn emails_para(app: &TestApp, para: &str) -> Vec<(String, String)> {
    sqlx::query_as("SELECT assunto, corpo FROM emails_saida WHERE para = $1 ORDER BY criado_em")
        .bind(para)
        .fetch_all(&app.pool)
        .await
        .unwrap()
}

async fn email_de(app: &TestApp, usuario: Uuid) -> String {
    sqlx::query_scalar("SELECT email::text FROM usuarios WHERE id = $1")
        .bind(usuario)
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

async fn gerar(app: &TestApp, org: Uuid, cookie: &str) -> (StatusCode, Value) {
    let (status, _, json) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/orgs/{org}/alertas/gerar"),
            Some(cookie),
            None,
        )
        .await;
    (status, json)
}

#[tokio::test]
async fn gera_vencida_e_vence_5d_sem_duplicar() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(HOJE));
    let (dono, org, cookie) = app.dono_com_org().await;
    let (admin, _) = app.membro(org, Papel::Admin).await;
    let (_, cookie_operador) = app.membro(org, Papel::Operador).await;
    let [a, b, c] = cenario(&app, org).await;

    let (status, json) = gerar(&app, org, &cookie_operador).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{json}");

    let (status, json) = gerar(&app, org, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["criadas"], 3);
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 0, "mesmo dia não duplica");

    let mut tx = app.tenant(org).await;
    let linhas: Vec<(String, String, String, String, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT tipo, titulo, corpo, link, usuario_id, chave_dedup FROM notificacoes ORDER BY titulo",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    drop(tx);
    assert_eq!(linhas.len(), 3);
    let (tipo, titulo, corpo, link, usuario, chave) = &linhas[0];
    assert_eq!(tipo, "vencida");
    assert_eq!(titulo, "NF 100 vencida há 9 dias");
    assert!(corpo.contains("empenho 2026NE000290"), "{corpo}");
    assert!(corpo.contains("R$ 1.234,56"), "{corpo}");
    assert!(corpo.contains("01/03/2026"), "{corpo}");
    assert_eq!(link, &format!("/o/{org}/notas/{a}"));
    assert!(usuario.is_none());
    // "vencida": uma vez por nota; "vence_5d": uma vez por nota e vencimento.
    assert_eq!(chave, &format!("{org}:vencida:{a}"));
    assert_eq!(linhas[1].0, "vence_5d");
    assert_eq!(linhas[1].1, "NF 200 vence hoje");
    assert_eq!(linhas[1].5, format!("{org}:vence_5d:{b}:2026-03-10"));
    assert_eq!(linhas[2].1, "NF 300 vence em 5 dias");
    assert_eq!(linhas[2].5, format!("{org}:vence_5d:{c}:2026-03-15"));

    // Plano profissional: um e-mail-resumo por dono/admin por dia.
    for usuario in [dono, admin] {
        let emails = emails_para(&app, &email_de(&app, usuario).await).await;
        assert_eq!(emails.len(), 1);
        assert!(
            emails[0].0.contains("1 NF(s) vencida(s) e 2 a vencer"),
            "{}",
            emails[0].0
        );
        assert!(emails[0].1.contains("NF 100"), "{}", emails[0].1);
    }
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM emails_saida")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(total, 2, "operador não recebe");

    // Dia seguinte: só o que mudou (B venceu, D entrou na janela); A e C não
    // se repetem. O resumo por e-mail continua diário.
    app.fixar_hoje(data("2026-03-11"));
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 2);
    assert_eq!(
        emails_para(&app, &email_de(&app, dono).await).await.len(),
        2
    );
    app.encerrar().await;
}

#[tokio::test]
async fn plano_sem_alertas_email_nao_registra_email() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(HOJE));
    let (_, org, cookie) = app.dono_com_org().await;
    sqlx::query(
        "UPDATE assinaturas SET plano = 'gratuito', status = 'ativa' WHERE organizacao_id = $1",
    )
    .bind(org)
    .execute(&app.pool)
    .await
    .unwrap();
    cenario(&app, org).await;
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 3);
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM emails_saida")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(total, 0);
    app.encerrar().await;
}

#[tokio::test]
async fn job_diario_gera_para_todas_as_organizacoes() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(HOJE));
    let (_, org_a, _) = app.dono_com_org().await;
    let (_, org_b, _) = app.dono_com_org().await;
    cenario(&app, org_a).await;
    cenario(&app, org_b).await;
    assert!(jobs::agendar_alertas(&app.estado).await.unwrap());
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    for org in [org_a, org_b] {
        let mut tx = app.tenant(org).await;
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM notificacoes")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(n, 3);
    }
    let r = alertas::gerar_todas(&app.estado).await.unwrap();
    assert_eq!((r.criadas, r.emails), (0, 0));
    app.encerrar().await;
}

#[tokio::test]
async fn lista_marca_lidas_e_isola_organizacoes() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(HOJE));
    let (_, org, cookie) = app.dono_com_org().await;
    let (_, cookie_leitor) = app.membro(org, Papel::Leitor).await;
    cenario(&app, org).await;
    gerar(&app, org, &cookie).await;
    // Notificação pessoal de outro membro: invisível para o dono.
    let (outro, _) = app.membro(org, Papel::Operador).await;
    let mut tx = app.tenant(org).await;
    sqlx::query(
        "INSERT INTO notificacoes (organizacao_id, usuario_id, tipo, titulo) VALUES ($1, $2, 'aviso', 'Pessoal')",
    )
    .bind(org)
    .bind(outro)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let base = format!("/api/v1/orgs/{org}/notificacoes");
    let (status, _, json) = app
        .requisicao(Method::GET, &base, Some(&cookie_leitor), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["meta"]["total"], 3);
    assert_eq!(json["meta"]["nao_lidas"], 3);
    let item = &json["dados"][0];
    for campo in [
        "id",
        "tipo",
        "titulo",
        "corpo",
        "link",
        "lida_em",
        "criado_em",
    ] {
        assert!(item.get(campo).is_some(), "campo {campo}");
    }
    let id = item["id"].as_str().unwrap().to_owned();

    let (status, _, _) = app
        .requisicao(
            Method::POST,
            &format!("{base}/{id}/lida"),
            Some(&cookie_leitor),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, _, json) = app
        .requisicao(
            Method::GET,
            &format!("{base}?nao_lidas=true&por_pagina=1"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(json["meta"]["total"], 2);
    assert_eq!(json["meta"]["nao_lidas"], 2);
    assert_eq!(json["meta"]["por_pagina"], 1);
    assert_eq!(json["dados"].as_array().unwrap().len(), 1);
    assert_ne!(json["dados"][0]["id"], id);

    let (status, _, json) = app
        .requisicao(
            Method::GET,
            &format!("{base}?nao_lidas=talvez"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");

    // Outra organização: não enxerga nem marca.
    let (_, org_b, cookie_b) = app.dono_com_org().await;
    let (status, _, _) = app
        .requisicao(Method::GET, &base, Some(&cookie_b), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = app
        .requisicao(
            Method::POST,
            &format!("/api/v1/orgs/{org_b}/notificacoes/{id}/lida"),
            Some(&cookie_b),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = app
        .requisicao(
            Method::POST,
            &format!("{base}/{}/lida", Uuid::now_v7()),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = app
        .requisicao(Method::POST, &format!("{base}/lidas"), Some(&cookie), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, _, json) = app
        .requisicao(Method::GET, &base, Some(&cookie), None)
        .await;
    assert_eq!(json["meta"]["nao_lidas"], 0);
    assert_eq!(json["meta"]["total"], 3);
    // A notificação pessoal do outro membro continua não lida.
    let mut tx = app.tenant(org).await;
    let pessoal: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT lida_em FROM notificacoes WHERE usuario_id = $1")
            .bind(outro)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(pessoal.is_none());
    drop(tx);
    app.encerrar().await;
}

async fn chaves(app: &TestApp, org: Uuid) -> Vec<String> {
    let mut tx = app.tenant(org).await;
    let chaves =
        sqlx::query_scalar("SELECT chave_dedup FROM notificacoes ORDER BY criado_em, chave_dedup")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    drop(tx);
    chaves
}

#[tokio::test]
async fn vencida_avisa_uma_vez_e_lembra_semanalmente() {
    let app = TestApp::novo().await;
    // Terça-feira, semana ISO 11.
    app.fixar_hoje(data("2026-03-10"));
    let (_, org, cookie) = app.dono_com_org().await;
    let e = empenho(&app, org).await;
    let a = nota(&app, org, e, "100", "2026-03-01", "emitida").await;
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 1);
    for dia in ["2026-03-11", "2026-03-13", "2026-03-15"] {
        app.fixar_hoje(data(dia));
        let (_, json) = gerar(&app, org, &cookie).await;
        assert_eq!(json["dados"]["criadas"], 0, "sem repetição diária ({dia})");
    }
    // Segunda-feira seguinte (semana 12): lembrete semanal.
    app.fixar_hoje(data("2026-03-16"));
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 1);
    app.fixar_hoje(data("2026-03-18"));
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 0, "um lembrete por semana");
    app.fixar_hoje(data("2026-03-23"));
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 1);
    assert_eq!(
        chaves(&app, org).await,
        [
            format!("{org}:vencida:{a}"),
            format!("{org}:vencida:{a}:2026-S12"),
            format!("{org}:vencida:{a}:2026-S13"),
        ]
    );
    app.encerrar().await;
}

#[tokio::test]
async fn vence_5d_uma_vez_por_vencimento() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data("2026-03-10"));
    let (_, org, cookie) = app.dono_com_org().await;
    let e = empenho(&app, org).await;
    let c = nota(&app, org, e, "300", "2026-03-15", "emitida").await;
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 1);
    app.fixar_hoje(data("2026-03-12"));
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 0);
    // Novo vencimento (edição da nota/regra): novo aviso.
    let mut tx = app.tenant(org).await;
    sqlx::query("UPDATE notas SET vencimento = '2026-03-16' WHERE id = $1")
        .bind(c)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (_, json) = gerar(&app, org, &cookie).await;
    assert_eq!(json["dados"]["criadas"], 1);
    app.encerrar().await;
}

#[tokio::test]
async fn muitas_notas_em_lotes_e_resumo_limitado() {
    let app = TestApp::novo().await;
    app.fixar_hoje(data(HOJE));
    let (dono, org, cookie) = app.dono_com_org().await;
    let e = empenho(&app, org).await;
    let mut tx = app.tenant(org).await;
    sqlx::query(
        "INSERT INTO notas (organizacao_id, empenho_id, numero, tipo, valor_centavos, data_emissao,
                            vencimento, status)
         SELECT $1, $2, 'L' || g, 'produto', 100, '2026-01-01', '2026-03-01', 'emitida'
         FROM generate_series(1, 1205) AS g",
    )
    .bind(org)
    .bind(e)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (status, json) = gerar(&app, org, &cookie).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["dados"]["criadas"], 1205);
    let emails = emails_para(&app, &email_de(&app, dono).await).await;
    assert_eq!(emails.len(), 1);
    let corpo = &emails[0].1;
    assert_eq!(corpo.matches("- NF ").count(), 50, "{corpo}");
    assert!(corpo.contains("e mais 1155 nota(s)"), "{corpo}");
    assert!(emails[0].0.contains("1205 NF(s) vencida(s)"));
    app.encerrar().await;
}
