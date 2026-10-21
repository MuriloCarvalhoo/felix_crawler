//! Importador do SQLite da versão anterior (`felix-server importar-sqlite`).
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::type_complexity
)]

mod comum;

use comum::TestApp;
use felix_server::legado::{self, Contagem, ErroLegado};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Esquema do `crates/felix-server/src/db.rs` antigo (rusqlite).
const ESQUEMA_ANTIGO: &str = "
CREATE TABLE empresas(id INTEGER PRIMARY KEY AUTOINCREMENT,razao_social TEXT,cnpj TEXT UNIQUE);
CREATE TABLE empenhos(id INTEGER PRIMARY KEY AUTOINCREMENT,empresa_id INTEGER,ug TEXT,gestao TEXT,empenho TEXT,cnpj_fornecedor TEXT,modalidade TEXT,numero_edital_ata TEXT,valor_total REAL,data_cadastro TEXT,FOREIGN KEY(empresa_id) REFERENCES empresas(id));
CREATE TABLE notas_fiscais(id INTEGER PRIMARY KEY AUTOINCREMENT,empenho_id INTEGER,tipo_documento TEXT,numero_nf TEXT,valor_nf REAL,status TEXT,data_emissao_nf TEXT,prazo_dias_pagamento INTEGER,data_emissao_ob TEXT,FOREIGN KEY(empenho_id) REFERENCES empenhos(id));
INSERT INTO empresas VALUES (1, 'Fornecedor Exemplo Ltda', '19.116.488/0001-45');
INSERT INTO empresas VALUES (2, 'Outra Empresa', '11222333000181');
INSERT INTO empresas VALUES (3, 'CNPJ ruim', '123');
INSERT INTO empenhos VALUES (1, 1, '160203', '00001', '2026NE000290', '19116488000145', 'Pregão', '90012/2025', 32450.0, '2026-01-20');
INSERT INTO empenhos VALUES (2, 1, '16020', '00001', '2026NE000291', '19116488000145', 'Pregão', '', 100.0, '2026-01-20');
INSERT INTO empenhos VALUES (3, 2, '160204', NULL, '2026NE000001', '11222333000181', NULL, NULL, 1000.1, '2026-01-21');
INSERT INTO empenhos VALUES (4, 3, '160204', NULL, '2026NE000002', '123', NULL, NULL, 50.0, '2026-01-21');
INSERT INTO notas_fiscais VALUES (1, 1, 'Nota Fiscal (Produtos)', '1234', 10000.5, 'PENDENTE', '2026-02-01', 45, NULL);
INSERT INTO notas_fiscais VALUES (2, 1, 'NFS-e (Serviços)', '77', 0.29, 'PAGO', '2026-02-02', 10, '2026-03-01');
INSERT INTO notas_fiscais VALUES (3, 1, 'Nota Fiscal (Produtos)', '1234', 5.0, 'PENDENTE', '2026-02-01', 30, NULL);
INSERT INTO notas_fiscais VALUES (4, 3, 'Nota Fiscal (Produtos)', '9', 100.0, 'ESTRANHO', '2026-02-01', 30, NULL);
INSERT INTO notas_fiscais VALUES (5, 3, 'Nota Fiscal (Produtos)', '10', 999.999, 'PENDENTE', '01/02/2026', 0, NULL);
INSERT INTO notas_fiscais VALUES (6, 3, 'Nota Fiscal (Produtos)', '11', 10.0, 'PENDENTE', 'sem data', 30, NULL);
";

async fn criar_sqlite(dir: &Path) -> PathBuf {
    criar_sqlite_com(dir, ESQUEMA_ANTIGO).await
}

async fn criar_sqlite_com(dir: &Path, sql: &str) -> PathBuf {
    let arquivo = dir.join("saas_cobrancas.db");
    let opcoes = SqliteConnectOptions::new()
        .filename(&arquivo)
        .create_if_missing(true);
    let mut conn = SqliteConnection::connect_with(&opcoes).await.unwrap();
    sqlx::raw_sql(sql).execute(&mut conn).await.unwrap();
    conn.close().await.unwrap();
    // Somente leitura no sistema de arquivos: o importador não pode escrever.
    let mut permissoes = std::fs::metadata(&arquivo).unwrap().permissions();
    permissoes.set_readonly(true);
    std::fs::set_permissions(&arquivo, permissoes).unwrap();
    arquivo
}

fn contagem(importados: u64, pulados: u64) -> Contagem {
    Contagem {
        importados,
        pulados,
    }
}

#[derive(Debug, sqlx::FromRow, PartialEq)]
struct NotaImportada {
    numero: String,
    tipo: String,
    valor_centavos: i64,
    status: String,
    data_emissao: String,
    vencimento: String,
    valor_pago_centavos: i64,
    data_pagamento: Option<String>,
}

#[tokio::test]
async fn importa_sqlite_antigo_e_pula_duplicados() {
    let app = TestApp::novo().await;
    let dir = tempfile::tempdir().unwrap();
    let arquivo = criar_sqlite(dir.path()).await;
    let antes = std::fs::read(&arquivo).unwrap();
    let (_, org, _) = app.dono_com_org().await;

    let resumo = legado::importar_sqlite(&app.pool, &arquivo, org, None)
        .await
        .unwrap();
    assert_eq!(resumo.empresas, contagem(2, 0));
    assert_eq!(resumo.empenhos, contagem(2, 0));
    assert_eq!(resumo.notas, contagem(3, 1), "{resumo:?}");
    assert_eq!(
        resumo.erros,
        [
            "Empresa 3: CNPJ inválido.",
            "Empenho 2: UG inválida.",
            "NF 4: status desconhecido.",
            "NF 6: data de emissão inválida.",
        ]
    );
    assert_eq!(std::fs::read(&arquivo).unwrap(), antes, "SQLite intacto");

    let mut tx = app.tenant(org).await;
    let empresas: Vec<(String, String)> =
        sqlx::query_as("SELECT cnpj::text, razao_social FROM empresas ORDER BY cnpj")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(
        empresas,
        [
            ("11222333000181".into(), "Outra Empresa".into()),
            ("19116488000145".into(), "Fornecedor Exemplo Ltda".into()),
        ]
    );
    let empenhos: Vec<(
        String,
        String,
        Option<String>,
        i64,
        String,
        i32,
        String,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT ug::text, numero, gestao, valor_total_centavos, regra_vencimento, prazo_dias,
                    origem, modalidade
             FROM empenhos ORDER BY numero",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        empenhos,
        [
            (
                "160204".into(),
                "2026NE000001".into(),
                None,
                100_010,
                "corridos_emissao".into(),
                30,
                "legado".into(),
                None
            ),
            (
                "160203".into(),
                "2026NE000290".into(),
                Some("00001".into()),
                3_245_000,
                "corridos_emissao".into(),
                45,
                "legado".into(),
                Some("Pregão".into())
            ),
        ]
    );
    let notas: Vec<NotaImportada> = sqlx::query_as(
        "SELECT numero, tipo, valor_centavos, status, data_emissao::text, vencimento::text,
                valor_pago_centavos, data_pagamento::text
         FROM notas ORDER BY numero",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        notas,
        [
            NotaImportada {
                numero: "10".into(),
                tipo: "produto".into(),
                valor_centavos: 100_000,
                status: "emitida".into(),
                data_emissao: "2026-02-01".into(),
                vencimento: "2026-03-03".into(),
                valor_pago_centavos: 0,
                data_pagamento: None,
            },
            NotaImportada {
                numero: "1234".into(),
                tipo: "produto".into(),
                valor_centavos: 1_000_050,
                status: "emitida".into(),
                data_emissao: "2026-02-01".into(),
                vencimento: "2026-03-18".into(),
                valor_pago_centavos: 0,
                data_pagamento: None,
            },
            NotaImportada {
                numero: "77".into(),
                tipo: "servico".into(),
                valor_centavos: 29,
                status: "paga".into(),
                data_emissao: "2026-02-02".into(),
                vencimento: "2026-03-19".into(),
                valor_pago_centavos: 29,
                data_pagamento: Some("2026-03-01".into()),
            },
        ]
    );
    let eventos: Vec<(String, Option<i64>)> =
        sqlx::query_as("SELECT tipo, valor_centavos FROM eventos_nota")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(eventos, [("paga".into(), Some(29))]);
    drop(tx);

    // Segunda execução: tudo duplicado.
    let resumo = legado::importar_sqlite(&app.pool, &arquivo, org, None)
        .await
        .unwrap();
    assert_eq!(resumo.empresas, contagem(0, 2));
    assert_eq!(resumo.empenhos, contagem(0, 2));
    assert_eq!(resumo.notas, contagem(0, 4));
    app.encerrar().await;
}

#[tokio::test]
async fn filtra_por_cnpj_e_valida_entradas() {
    let app = TestApp::novo().await;
    let dir = tempfile::tempdir().unwrap();
    let arquivo = criar_sqlite(dir.path()).await;
    let (_, org, _) = app.dono_com_org().await;

    let resumo = legado::importar_sqlite(&app.pool, &arquivo, org, Some("11.222.333/0001-81"))
        .await
        .unwrap();
    assert_eq!(resumo.empresas, contagem(1, 0));
    assert_eq!(resumo.empenhos, contagem(1, 0));
    assert_eq!(resumo.notas, contagem(1, 0));
    let mut tx = app.tenant(org).await;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM empresas")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(total, 1);
    drop(tx);

    let erro = legado::importar_sqlite(&app.pool, &arquivo, org, Some("98765432000198"))
        .await
        .unwrap_err();
    assert!(matches!(erro, ErroLegado::Entrada(_)), "{erro}");
    let erro = legado::importar_sqlite(&app.pool, &arquivo, Uuid::now_v7(), None)
        .await
        .unwrap_err();
    assert!(erro.to_string().contains("não encontrada"), "{erro}");
    let erro = legado::importar_sqlite(&app.pool, &dir.path().join("nao.db"), org, None)
        .await
        .unwrap_err();
    assert!(erro.to_string().contains("não encontrado"), "{erro}");
    app.encerrar().await;
}

#[tokio::test]
async fn comando_importar_sqlite_imprime_resumo_json() {
    let app = TestApp::novo().await;
    let dir = tempfile::tempdir().unwrap();
    let arquivo = criar_sqlite(dir.path()).await;
    let (_, org, _) = app.dono_com_org().await;
    let admin = std::env::var("FELIX_TEST_DATABASE_URL")
        .unwrap_or_else(|_| comum::URL_ADMIN_PADRAO.to_owned());
    let host = admin
        .split('@')
        .nth(1)
        .and_then(|resto| resto.split('/').next())
        .unwrap();
    let url = format!(
        "postgres://{u}:{u}@{host}/{}",
        app.nome_banco,
        u = comum::USUARIO_TESTE
    );
    let saida = tokio::process::Command::new(env!("CARGO_BIN_EXE_felix-server"))
        .arg("importar-sqlite")
        .arg(&arquivo)
        .args(["--org", &org.to_string()])
        .env("DATABASE_URL", url)
        .env("FELIX_ENV", "dev")
        .env("RUST_LOG", "off")
        .output()
        .await
        .unwrap();
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&saida.stdout).unwrap();
    assert_eq!(json["empenhos"]["importados"], 2);
    assert_eq!(json["notas"]["importados"], 3);
    assert_eq!(json["erros"].as_array().unwrap().len(), 4);

    let saida = tokio::process::Command::new(env!("CARGO_BIN_EXE_felix-server"))
        .arg("importar-sqlite")
        .arg(&arquivo)
        .output()
        .await
        .unwrap();
    assert!(!saida.status.success());
    assert!(String::from_utf8_lossy(&saida.stderr).contains("--org"));
    app.encerrar().await;
}

/// Dados fora do padrão: número em minúsculas, gestão inválida, textos longos
/// e notas que estourariam o saldo do empenho.
const LEGADO_FORA_DO_PADRAO: &str = "
CREATE TABLE empresas(id INTEGER PRIMARY KEY AUTOINCREMENT,razao_social TEXT,cnpj TEXT UNIQUE);
CREATE TABLE empenhos(id INTEGER PRIMARY KEY AUTOINCREMENT,empresa_id INTEGER,ug TEXT,gestao TEXT,empenho TEXT,cnpj_fornecedor TEXT,modalidade TEXT,numero_edital_ata TEXT,valor_total REAL,data_cadastro TEXT);
CREATE TABLE notas_fiscais(id INTEGER PRIMARY KEY AUTOINCREMENT,empenho_id INTEGER,tipo_documento TEXT,numero_nf TEXT,valor_nf REAL,status TEXT,data_emissao_nf TEXT,prazo_dias_pagamento INTEGER,data_emissao_ob TEXT);
INSERT INTO empresas VALUES (1, 'Fornecedor', '19116488000145');
INSERT INTO empenhos VALUES (1, 1, '160203', '1', '2026ne000777', '', printf('%.150c', 'm'), '90012/2025', 100.0, '2026-01-20');
INSERT INTO notas_fiscais VALUES (1, 1, 'Nota Fiscal (Produtos)', 'A', 60.0, 'PENDENTE', '2026-02-01', 30, NULL);
INSERT INTO notas_fiscais VALUES (2, 1, 'Nota Fiscal (Produtos)', 'B', 30.0, 'CANCELADO', '2026-02-01', 30, NULL);
INSERT INTO notas_fiscais VALUES (3, 1, 'Nota Fiscal (Produtos)', 'C', 50.0, 'PENDENTE', '2026-02-01', 30, NULL);
INSERT INTO notas_fiscais VALUES (4, 1, 'Nota Fiscal (Produtos)', 'D', 40.0, 'PAGO', '2026-02-01', 30, '2026-02-10');
";

#[tokio::test]
async fn normaliza_numero_alinha_limites_e_respeita_saldo() {
    let app = TestApp::novo().await;
    let dir = tempfile::tempdir().unwrap();
    let arquivo = criar_sqlite_com(dir.path(), LEGADO_FORA_DO_PADRAO).await;
    let (_, org, _) = app.dono_com_org().await;
    let resumo = legado::importar_sqlite(&app.pool, &arquivo, org, None)
        .await
        .unwrap();
    assert_eq!(resumo.empenhos, contagem(1, 0));
    // A (60) e D (40) somam o valor total; B cancelada não conta; C estouraria.
    assert_eq!(resumo.notas, contagem(3, 0), "{resumo:?}");
    assert_eq!(
        resumo.erros,
        [
            "Empenho 1: gestão fora do padrão (5 dígitos); não importada.",
            "Empenho 1: modalidade cortada em 100 caracteres.",
            "NF 3: valor excede o saldo do empenho (saldo R$ 40,00); não importada.",
        ]
    );
    let mut tx = app.tenant(org).await;
    let (numero, gestao, modalidade): (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT numero, gestao, modalidade FROM empenhos")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(numero, "2026NE000777");
    assert_eq!(gestao, None);
    assert_eq!(modalidade.unwrap().chars().count(), 100);
    let numeros: Vec<String> = sqlx::query_scalar("SELECT numero FROM notas ORDER BY numero")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(numeros, ["A", "B", "D"]);
    drop(tx);

    // Reimportação: duplicados são pulados, sem falso erro de saldo.
    let resumo = legado::importar_sqlite(&app.pool, &arquivo, org, None)
        .await
        .unwrap();
    assert_eq!(resumo.empenhos, contagem(0, 1));
    assert_eq!(resumo.notas, contagem(0, 3));
    assert_eq!(resumo.erros.len(), 3, "{:?}", resumo.erros);
    app.encerrar().await;
}
