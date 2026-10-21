//! Worker de jobs: fila com SKIP LOCKED, retentativas, jobs presos, agendador
//! diário e envio da caixa de saída de e-mail.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod comum;

use chrono::{DateTime, NaiveDate, Utc};
use comum::TestApp;
use felix_server::jobs::{
    self, MAX_TENTATIVAS, TIPO_ALERTAS, TIPO_EMAIL, TIPO_TRANSPARENCIA,
    envio_email::{self, Remetente},
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct LinhaJob {
    status: String,
    tentativas: i32,
    erro: Option<String>,
    executar_em: DateTime<Utc>,
}

async fn job(app: &TestApp, id: Uuid) -> LinhaJob {
    sqlx::query_as("SELECT status, tentativas, erro, executar_em FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

async fn liberar_agora(app: &TestApp, id: Uuid) {
    sqlx::query("UPDATE jobs SET executar_em = now() WHERE id = $1")
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn enfileira_executa_e_conclui() {
    let app = TestApp::novo().await;
    let id = jobs::enfileirar(&app.pool, TIPO_ALERTAS, None, json!({}))
        .await
        .unwrap();
    assert_eq!(job(&app, id).await.status, "pendente");
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let linha = job(&app, id).await;
    assert_eq!(linha.status, "concluido");
    assert_eq!(linha.tentativas, 1);
    assert!(linha.erro.is_none());
    assert!(!jobs::executar_um(&app.estado).await.unwrap(), "fila vazia");
    app.encerrar().await;
}

#[tokio::test]
async fn job_agendado_no_futuro_espera() {
    let app = TestApp::novo().await;
    let mut conn = app.pool.acquire().await.unwrap();
    let futuro = Utc::now() + chrono::TimeDelta::hours(1);
    let id = jobs::enfileirar_em(&mut conn, TIPO_ALERTAS, None, json!({}), Some(futuro))
        .await
        .unwrap();
    drop(conn);
    assert!(!jobs::executar_um(&app.estado).await.unwrap());
    liberar_agora(&app, id).await;
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    assert_eq!(job(&app, id).await.status, "concluido");
    app.encerrar().await;
}

/// Aponta o app para um Portal (com chave) na URL informada.
fn configurar_portal(app: &mut TestApp, url: &str) {
    let mut config = (*app.estado.config).clone();
    config.transparencia_key = Some("chave-teste".into());
    config.transparencia_url = url.into();
    app.estado = felix_server::app::AppState::novo(app.pool.clone(), config)
        .unwrap()
        .com_relogio(Arc::new(app.relogio.clone()));
}

async fn criar_empresa(app: &TestApp, org: Uuid) {
    let mut tx = app.tenant(org).await;
    sqlx::query(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj)
         VALUES ($1, 'Fornecedor', '19116488000145')",
    )
    .bind(org)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn falha_temporaria_reagenda_com_espera_e_esgota_tentativas() {
    let mut app = TestApp::novo().await;
    // Portal inacessível (porta fechada): falha de rede é temporária.
    configurar_portal(&mut app, "http://127.0.0.1:9");
    let (_, org, _) = app.dono_com_org().await;
    criar_empresa(&app, org).await;
    let id = jobs::enfileirar(
        &app.pool,
        TIPO_TRANSPARENCIA,
        Some(org),
        json!({"org_id": org, "ano": 2026}),
    )
    .await
    .unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let linha = job(&app, id).await;
    assert_eq!(linha.status, "pendente");
    assert_eq!(linha.tentativas, 1);
    assert!(
        linha.erro.as_deref().unwrap().contains("indisponível"),
        "{:?}",
        linha.erro
    );
    let espera = linha.executar_em - Utc::now();
    assert!(
        espera > chrono::TimeDelta::seconds(50) && espera <= chrono::TimeDelta::seconds(60),
        "espera de ~1 min: {espera:?}"
    );
    assert!(
        !jobs::executar_um(&app.estado).await.unwrap(),
        "ainda não venceu"
    );

    liberar_agora(&app, id).await;
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let linha = job(&app, id).await;
    assert_eq!((linha.status.as_str(), linha.tentativas), ("pendente", 2));
    let espera = linha.executar_em - Utc::now();
    assert!(
        espera > chrono::TimeDelta::seconds(110),
        "espera dobra: {espera:?}"
    );

    liberar_agora(&app, id).await;
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let linha = job(&app, id).await;
    assert_eq!(linha.status, "falhou");
    assert_eq!(linha.tentativas, MAX_TENTATIVAS);
    assert!(!jobs::executar_um(&app.estado).await.unwrap());
    // A integração registra o erro.
    let mut tx = app.tenant(org).await;
    let status: String = sqlx::query_scalar(
        "SELECT ultimo_status FROM integracoes WHERE organizacao_id = $1 AND tipo = 'transparencia'",
    )
    .bind(org)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(status, "erro");
    drop(tx);
    app.encerrar().await;
}

#[tokio::test]
async fn falhas_de_configuracao_e_plano_nao_sao_repetidas() {
    // Sem chave do Portal no servidor: não adianta repetir.
    let app = TestApp::novo().await;
    let (_, org, _) = app.dono_com_org().await;
    let id = jobs::enfileirar(
        &app.pool,
        TIPO_TRANSPARENCIA,
        Some(org),
        json!({"org_id": org, "ano": 2026}),
    )
    .await
    .unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let linha = job(&app, id).await;
    assert_eq!((linha.status.as_str(), linha.tentativas), ("falhou", 1));
    assert_eq!(
        linha.erro.as_deref(),
        Some("Integração não configurada no servidor.")
    );
    app.encerrar().await;

    // Plano sem a integração (402): também definitiva.
    let mut app = TestApp::novo().await;
    configurar_portal(&mut app, "http://127.0.0.1:9");
    let (_, org, _) = app.dono_com_org().await;
    sqlx::query(
        "UPDATE assinaturas SET plano = 'gratuito', status = 'ativa' WHERE organizacao_id = $1",
    )
    .bind(org)
    .execute(&app.pool)
    .await
    .unwrap();
    let id = jobs::enfileirar(
        &app.pool,
        TIPO_TRANSPARENCIA,
        Some(org),
        json!({"org_id": org, "ano": 2026}),
    )
    .await
    .unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let linha = job(&app, id).await;
    assert_eq!((linha.status.as_str(), linha.tentativas), ("falhou", 1));
    app.encerrar().await;
}

#[test]
fn erros_4xx_sao_permanentes_e_5xx_temporarios() {
    use felix_server::{http::ErroApi, jobs::Falha};
    assert!(matches!(
        Falha::from(ErroApi::validacao("x")),
        Falha::Permanente(_)
    ));
    assert!(matches!(
        Falha::from(ErroApi::limite_plano("x")),
        Falha::Permanente(_)
    ));
    assert!(matches!(
        Falha::from(ErroApi::nao_encontrado()),
        Falha::Permanente(_)
    ));
    assert!(matches!(
        Falha::from(ErroApi::ocupado()),
        Falha::Temporaria(_)
    ));
    assert!(matches!(
        Falha::from(ErroApi::indisponivel("x")),
        Falha::Temporaria(_)
    ));
    assert!(matches!(
        Falha::from(ErroApi::interno()),
        Falha::Temporaria(_)
    ));
}

#[tokio::test]
async fn panico_vira_falha_permanente() {
    let r = jobs::proteger(async {
        let vazio: Vec<u8> = Vec::new();
        // Indexação fora dos limites: pânico dentro do job.
        let _ = vazio[3];
        Ok(())
    })
    .await;
    assert!(matches!(r, Err(jobs::Falha::Permanente(_))), "{r:?}");
    assert_eq!(jobs::proteger(async { Ok(()) }).await, Ok(()));
}

#[tokio::test]
async fn job_de_organizacao_excluida_e_concluido_sem_executar() {
    let app = TestApp::novo().await;
    let (_, org, _) = app.dono_com_org().await;
    let id = jobs::enfileirar(
        &app.pool,
        TIPO_TRANSPARENCIA,
        Some(org),
        json!({"org_id": org, "ano": 2026}),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE organizacoes SET excluida_em = now() WHERE id = $1")
        .bind(org)
        .execute(&app.pool)
        .await
        .unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let linha = job(&app, id).await;
    assert_eq!((linha.status.as_str(), linha.tentativas), ("concluido", 1));
    // Nada foi executado: a integração não registrou status.
    let mut tx = app.tenant(org).await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM integracoes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 0);
    drop(tx);
    app.encerrar().await;
}

#[tokio::test]
async fn rodada_diaria_continua_apos_falha_de_uma_organizacao() {
    let mut app = TestApp::novo().await;
    configurar_portal(&mut app, "http://127.0.0.1:9");
    app.fixar_hoje(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap());
    let (_, org_a, _) = app.dono_com_org().await;
    let (_, org_b, _) = app.dono_com_org().await;
    for org in [org_a, org_b] {
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
             VALUES ($1, $2, '160203', '2026NE000290', 1000000) RETURNING id",
        )
        .bind(org)
        .bind(empresa)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO notas (organizacao_id, empenho_id, numero, tipo, valor_centavos,
                                data_emissao, vencimento)
             VALUES ($1, $2, '1', 'produto', 100, '2026-01-01', '2026-03-01')",
        )
        .bind(org)
        .bind(empenho)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO integracoes (organizacao_id, tipo, ativa) VALUES ($1, 'transparencia', true)",
        )
        .bind(org)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    // A organização A falha ao gravar notificações.
    sqlx::query(&format!(
        "CREATE FUNCTION falhar_org_a() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
           IF NEW.organizacao_id = '{org_a}' THEN RAISE EXCEPTION 'falha simulada'; END IF;
           RETURN NEW;
         END $$"
    ))
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER falhar_org_a BEFORE INSERT ON notificacoes
         FOR EACH ROW EXECUTE FUNCTION falhar_org_a()",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let id = jobs::enfileirar(&app.pool, TIPO_ALERTAS, None, json!({}))
        .await
        .unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    // B recebeu os alertas mesmo com A falhando.
    let mut tx = app.tenant(org_b).await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM notificacoes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 1);
    drop(tx);
    // As sincronizações das duas organizações foram enfileiradas.
    let syncs: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE tipo = $1")
        .bind(TIPO_TRANSPARENCIA)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(syncs, 2);
    // O job da rodada registra a falha (será repetido; tudo é idempotente).
    let linha = job(&app, id).await;
    assert_eq!(linha.status, "pendente");
    assert!(linha.erro.unwrap().contains("alertas"));
    app.encerrar().await;
}

#[tokio::test]
async fn limpeza_remove_registros_antigos() {
    let app = TestApp::novo().await;
    let (usuario, org, _) = app.dono_com_org().await;
    let token = |n: u8| vec![n; 32];
    // Sessões: expirada há 31 dias, revogada há 31 dias, revogada há 5 dias, ativa.
    for (n, expira, revogada) in [
        (1_u8, "now() - interval '31 days'", "NULL"),
        (2, "now() + interval '7 days'", "now() - interval '31 days'"),
        (3, "now() + interval '7 days'", "now() - interval '5 days'"),
        (4, "now() + interval '7 days'", "NULL"),
    ] {
        sqlx::query(&format!(
            "INSERT INTO sessoes (usuario_id, token_hash, expira_em, revogada_em)
             VALUES ($1, $2, {expira}, {revogada})"
        ))
        .bind(usuario)
        .bind(token(n))
        .execute(&app.pool)
        .await
        .unwrap();
    }
    // Tokens: expirado há 31 dias, usado há 31 dias, válido.
    for (n, expira, usado) in [
        (11_u8, "now() - interval '31 days'", "NULL"),
        (
            12,
            "now() + interval '1 hour'",
            "now() - interval '31 days'",
        ),
        (13, "now() + interval '1 hour'", "NULL"),
    ] {
        sqlx::query(&format!(
            "INSERT INTO tokens_usuario (usuario_id, finalidade, token_hash, expira_em, usado_em)
             VALUES ($1, 'redefinir_senha', $2, {expira}, {usado})"
        ))
        .bind(usuario)
        .bind(token(n))
        .execute(&app.pool)
        .await
        .unwrap();
    }
    // Jobs: concluído e falho antigos, concluído recente, pendente antigo.
    for (status, idade) in [
        ("concluido", "31 days"),
        ("falhou", "31 days"),
        ("concluido", "1 day"),
        ("pendente", "31 days"),
    ] {
        sqlx::query(&format!(
            "INSERT INTO jobs (tipo, status, atualizado_em)
             VALUES ('x', $1, now() - interval '{idade}')"
        ))
        .bind(status)
        .execute(&app.pool)
        .await
        .unwrap();
    }
    // Notificações: lida há 91 dias, lida há 10 dias, não lida antiga.
    let mut tx = app.tenant(org).await;
    for (chave, lida) in [
        ("a", "now() - interval '91 days'"),
        ("b", "now() - interval '10 days'"),
        ("c", "NULL"),
    ] {
        sqlx::query(&format!(
            "INSERT INTO notificacoes (organizacao_id, tipo, titulo, chave_dedup, lida_em, criado_em)
             VALUES ($1, 'x', 't', $2, {lida}, now() - interval '100 days')"
        ))
        .bind(org)
        .bind(format!("{org}:{chave}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let r = jobs::limpeza::executar(&app.estado).await.unwrap();
    assert_eq!(
        (r.sessoes, r.tokens, r.jobs, r.notificacoes),
        (2, 2, 2, 1),
        "{r:?}"
    );
    let sessoes: i64 = sqlx::query_scalar("SELECT count(*) FROM sessoes")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    // As duas restantes + a sessão criada para o dono no harness.
    assert!(sessoes >= 2);
    let mut tx = app.tenant(org).await;
    let chaves: Vec<String> =
        sqlx::query_scalar("SELECT chave_dedup FROM notificacoes ORDER BY chave_dedup")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    drop(tx);
    assert_eq!(chaves, [format!("{org}:b"), format!("{org}:c")]);
    // Idempotente.
    let r = jobs::limpeza::executar(&app.estado).await.unwrap();
    assert_eq!((r.sessoes, r.tokens, r.jobs, r.notificacoes), (0, 0, 0, 0));
    app.encerrar().await;
}

#[tokio::test]
async fn tipo_desconhecido_e_payload_invalido_falham_de_vez() {
    let app = TestApp::novo().await;
    let desconhecido = jobs::enfileirar(&app.pool, "inexistente", None, json!({}))
        .await
        .unwrap();
    let sem_org = jobs::enfileirar(&app.pool, TIPO_TRANSPARENCIA, None, json!({"ano": 2026}))
        .await
        .unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let a = job(&app, desconhecido).await;
    assert_eq!((a.status.as_str(), a.tentativas), ("falhou", 1));
    assert!(a.erro.unwrap().contains("desconhecido"));
    let b = job(&app, sem_org).await;
    assert_eq!((b.status.as_str(), b.tentativas), ("falhou", 1));
    app.encerrar().await;
}

#[tokio::test]
async fn skip_locked_nao_entrega_o_mesmo_job_duas_vezes() {
    let app = TestApp::novo().await;
    // Dois retirar simultâneos com um único job: só um recebe.
    let unico = jobs::enfileirar(&app.pool, TIPO_ALERTAS, None, json!({}))
        .await
        .unwrap();
    let (a, b) = tokio::join!(jobs::retirar(&app.pool), jobs::retirar(&app.pool));
    let pegos: Vec<_> = [a.unwrap(), b.unwrap()].into_iter().flatten().collect();
    assert_eq!(pegos.len(), 1);
    assert_eq!(pegos[0].id, unico);
    assert_eq!(pegos[0].tentativas, 1);

    // Dois executores concorrentes esvaziam a fila sem repetir jobs.
    let mut ids = HashSet::new();
    for _ in 0..8 {
        ids.insert(
            jobs::enfileirar(&app.pool, TIPO_ALERTAS, None, json!({}))
                .await
                .unwrap(),
        );
    }
    let processados = Arc::new(AtomicUsize::new(0));
    let mut tarefas = Vec::new();
    for _ in 0..2 {
        let estado = app.estado.clone();
        let processados = Arc::clone(&processados);
        tarefas.push(tokio::spawn(async move {
            while jobs::executar_um(&estado).await.unwrap() {
                processados.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for tarefa in tarefas {
        tarefa.await.unwrap();
    }
    assert_eq!(processados.load(Ordering::SeqCst), 8);
    let linhas: Vec<(Uuid, String, i32)> =
        sqlx::query_as("SELECT id, status, tentativas FROM jobs WHERE id <> $1 ORDER BY id")
            .bind(unico)
            .fetch_all(&app.pool)
            .await
            .unwrap();
    assert_eq!(linhas.len(), 8);
    for (id, status, tentativas) in linhas {
        assert!(ids.contains(&id));
        assert_eq!((status.as_str(), tentativas), ("concluido", 1));
    }
    app.encerrar().await;
}

#[tokio::test]
async fn recupera_jobs_presos() {
    let app = TestApp::novo().await;
    let preso = jobs::enfileirar(&app.pool, TIPO_ALERTAS, None, json!({}))
        .await
        .unwrap();
    let esgotado = jobs::enfileirar(&app.pool, TIPO_ALERTAS, None, json!({}))
        .await
        .unwrap();
    let recente = jobs::enfileirar(&app.pool, TIPO_ALERTAS, None, json!({}))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE jobs SET status = 'executando', tentativas = 1,
             atualizado_em = now() - interval '20 minutes' WHERE id = $1",
    )
    .bind(preso)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE jobs SET status = 'executando', tentativas = 3,
             atualizado_em = now() - interval '16 minutes' WHERE id = $1",
    )
    .bind(esgotado)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE jobs SET status = 'executando', tentativas = 1 WHERE id = $1")
        .bind(recente)
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(jobs::recuperar_presos(&app.pool).await.unwrap(), 2);
    assert_eq!(job(&app, preso).await.status, "pendente");
    assert_eq!(job(&app, esgotado).await.status, "falhou");
    assert_eq!(job(&app, recente).await.status, "executando");
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    let linha = job(&app, preso).await;
    assert_eq!((linha.status.as_str(), linha.tentativas), ("concluido", 2));
    app.encerrar().await;
}

#[tokio::test]
async fn agendador_cria_alertas_diarios_uma_vez_por_dia() {
    let app = TestApp::novo().await;
    let hoje = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
    app.fixar_hoje(hoje);
    assert!(jobs::agendar_alertas(&app.estado).await.unwrap());
    assert!(!jobs::agendar_alertas(&app.estado).await.unwrap());
    let (quando, payload): (DateTime<Utc>, Value) =
        sqlx::query_as("SELECT executar_em, payload FROM jobs WHERE tipo = $1")
            .bind(TIPO_ALERTAS)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(quando.to_rfc3339(), "2026-03-10T09:00:00+00:00");
    assert_eq!(payload, json!({"data": "2026-03-10"}));
    // Mesmo concluído, não recria no mesmo dia.
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    assert!(!jobs::agendar_alertas(&app.estado).await.unwrap());
    // Dia seguinte: novo job.
    app.fixar_hoje(hoje.succ_opt().unwrap());
    assert!(jobs::agendar_alertas(&app.estado).await.unwrap());
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE tipo = $1")
        .bind(TIPO_ALERTAS)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(total, 2);
    app.encerrar().await;
}

#[tokio::test]
async fn agendamentos_concorrentes_nao_duplicam() {
    let app = TestApp::novo().await;
    let resultados = tokio::join!(
        jobs::agendar_alertas(&app.estado),
        jobs::agendar_alertas(&app.estado),
        jobs::agendar_alertas(&app.estado),
    );
    let criados = [resultados.0, resultados.1, resultados.2]
        .into_iter()
        .filter(|r| *r.as_ref().unwrap())
        .count();
    assert_eq!(criados, 1);
    app.encerrar().await;
}

#[tokio::test]
async fn worker_em_segundo_plano_processa_e_encerra() {
    let app = TestApp::novo().await;
    let id = jobs::enfileirar(&app.pool, TIPO_ALERTAS, None, json!({}))
        .await
        .unwrap();
    let trabalhador = jobs::iniciar(app.estado.clone());
    for _ in 0..50 {
        if jobs::executores_vivos() >= jobs::CONCORRENCIA {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(jobs::executores_vivos(), jobs::CONCORRENCIA);
    let mut concluido = false;
    for _ in 0..100 {
        if job(&app, id).await.status == "concluido" {
            concluido = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(concluido, "o worker deveria processar o job");
    tokio::time::timeout(Duration::from_secs(10), trabalhador.encerrar())
        .await
        .expect("encerramento gracioso");
    assert_eq!(jobs::executores_vivos(), 0);
    app.encerrar().await;
}

// ------------------------------- e-mail -------------------------------

async fn registrar_email(app: &TestApp, para: &str) -> Uuid {
    let mut conn = app.pool.acquire().await.unwrap();
    felix_server::email::enfileirar(&mut conn, para, "Assunto", "Corpo")
        .await
        .unwrap()
}

async fn liberar_email(app: &TestApp, id: Uuid) {
    sqlx::query("UPDATE emails_saida SET proxima_tentativa_em = now() WHERE id = $1")
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();
}

async fn espera_email(app: &TestApp, id: Uuid) -> chrono::TimeDelta {
    let quando: DateTime<Utc> =
        sqlx::query_scalar("SELECT proxima_tentativa_em FROM emails_saida WHERE id = $1")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    quando - Utc::now()
}

async fn email(app: &TestApp, id: Uuid) -> (String, i32, Option<String>, bool) {
    sqlx::query_as(
        "SELECT status, tentativas, erro, enviado_em IS NOT NULL FROM emails_saida WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&app.pool)
    .await
    .unwrap()
}

/// Remetente falso: falha para endereços com "falha".
#[derive(Default)]
struct RemetenteFalso {
    enviados: Mutex<Vec<String>>,
}

impl Remetente for RemetenteFalso {
    async fn enviar(&self, para: &str, _assunto: &str, _corpo: &str) -> Result<(), String> {
        if para.contains("falha") {
            return Err("Falha no envio: recusado temporariamente pelo servidor SMTP.".into());
        }
        self.enviados.lock().unwrap().push(para.to_owned());
        Ok(())
    }
}

#[tokio::test]
async fn sem_smtp_emails_ficam_registrados() {
    let app = TestApp::novo().await;
    let id = registrar_email(&app, "a@teste.felix").await;
    assert!(!jobs::agendar_envio_email(&app.estado).await.unwrap());
    let job_id = jobs::enfileirar(&app.pool, TIPO_EMAIL, None, json!({}))
        .await
        .unwrap();
    assert!(jobs::executar_um(&app.estado).await.unwrap());
    assert_eq!(job(&app, job_id).await.status, "concluido");
    assert_eq!(email(&app, id).await, ("registrado".into(), 0, None, false));
    app.encerrar().await;
}

#[tokio::test]
async fn envio_marca_enviado_e_falhou_apos_tentativas() {
    let app = TestApp::novo().await;
    let ok = registrar_email(&app, "ok@teste.felix").await;
    let ruim = registrar_email(&app, "falha@teste.felix").await;
    let remetente = RemetenteFalso::default();
    let resumo = envio_email::enviar_registrados(&app.pool, &remetente, 50, false)
        .await
        .unwrap();
    assert_eq!((resumo.enviados, resumo.falhas), (1, 1));
    assert_eq!(email(&app, ok).await, ("enviado".into(), 1, None, true));
    let (status, tentativas, erro, _) = email(&app, ruim).await;
    assert_eq!((status.as_str(), tentativas), ("registrado", 1));
    let erro = erro.unwrap();
    assert!(!erro.contains("falha@"), "erro sem dados pessoais: {erro}");
    // Espera 1 min antes da 2ª tentativa; antes disso não reenvia.
    let espera = espera_email(&app, ruim).await;
    assert!(
        espera > chrono::TimeDelta::seconds(50) && espera <= chrono::TimeDelta::seconds(60),
        "{espera:?}"
    );
    let resumo = envio_email::enviar_registrados(&app.pool, &remetente, 50, false)
        .await
        .unwrap();
    assert_eq!((resumo.enviados, resumo.falhas), (0, 0), "ainda esperando");
    // Esperas seguintes: 5 e 30 minutos.
    for (tentativa, minutos) in [(2, 5), (3, 30)] {
        liberar_email(&app, ruim).await;
        envio_email::enviar_registrados(&app.pool, &remetente, 50, false)
            .await
            .unwrap();
        let (status, tentativas, _, _) = email(&app, ruim).await;
        assert_eq!((status.as_str(), tentativas), ("registrado", tentativa));
        let espera = espera_email(&app, ruim).await;
        assert!(
            espera > chrono::TimeDelta::minutes(minutos) - chrono::TimeDelta::seconds(10)
                && espera <= chrono::TimeDelta::minutes(minutos),
            "{espera:?}"
        );
    }
    liberar_email(&app, ruim).await;
    envio_email::enviar_registrados(&app.pool, &remetente, 50, false)
        .await
        .unwrap();
    let (status, tentativas, _, _) = email(&app, ruim).await;
    assert_eq!(
        (status.as_str(), tentativas),
        ("falhou", envio_email::MAX_TENTATIVAS_EMAIL)
    );
    // Nada mais a enviar; o enviado não é reenviado.
    let resumo = envio_email::enviar_registrados(&app.pool, &remetente, 50, false)
        .await
        .unwrap();
    assert_eq!((resumo.enviados, resumo.falhas), (0, 0));
    assert_eq!(remetente.enviados.lock().unwrap().len(), 1);
    app.encerrar().await;
}

#[tokio::test]
async fn em_producao_o_corpo_e_apagado_apos_o_envio() {
    let app = TestApp::novo().await;
    let ok = registrar_email(&app, "ok@teste.felix").await;
    let ruim = registrar_email(&app, "falha@teste.felix").await;
    let remetente = RemetenteFalso::default();
    envio_email::enviar_registrados(&app.pool, &remetente, 50, true)
        .await
        .unwrap();
    let corpo = |id: Uuid| {
        let pool = app.pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT corpo FROM emails_saida WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(corpo(ok).await, felix_server::email::CORPO_REDIGIDO);
    assert_eq!(corpo(ruim).await, "Corpo", "não enviado: corpo mantido");
    app.encerrar().await;
}

#[test]
fn esperas_do_email() {
    assert_eq!(envio_email::espera_minutos(1), 1);
    assert_eq!(envio_email::espera_minutos(2), 5);
    assert_eq!(envio_email::espera_minutos(3), 30);
    assert_eq!(envio_email::espera_minutos(9), 30);
}

#[tokio::test]
async fn com_smtp_agenda_um_unico_job_de_envio() {
    let mut app = TestApp::novo().await;
    let mut config = (*app.estado.config).clone();
    config.smtp_url = Some("smtp://127.0.0.1:1".into());
    config.email_from = Some("Felix <nao-responda@teste.felix>".into());
    app.estado = felix_server::app::AppState::novo(app.pool.clone(), config)
        .unwrap()
        .com_relogio(Arc::new(app.relogio.clone()));
    assert!(
        !jobs::agendar_envio_email(&app.estado).await.unwrap(),
        "sem e-mails registrados"
    );
    let id = registrar_email(&app, "a@teste.felix").await;
    // E-mail aguardando a próxima tentativa não agenda job.
    sqlx::query(
        "UPDATE emails_saida SET tentativas = 1, proxima_tentativa_em = now() + interval '5 minutes'
         WHERE id = $1",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .unwrap();
    assert!(!jobs::agendar_envio_email(&app.estado).await.unwrap());
    liberar_email(&app, id).await;
    assert!(jobs::agendar_envio_email(&app.estado).await.unwrap());
    assert!(
        !jobs::agendar_envio_email(&app.estado).await.unwrap(),
        "já existe job pendente"
    );
    let remetente = envio_email::remetente(&app.estado.config).unwrap();
    assert!(remetente.is_some());
    app.encerrar().await;
}
