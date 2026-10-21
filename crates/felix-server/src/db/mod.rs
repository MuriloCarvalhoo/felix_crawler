//! Conexão, migrações, transações de tenant (RLS) e auditoria.

use crate::{config::Config, http::erro::ErroApi};
use sqlx::{
    PgConnection, PgPool, Postgres, Transaction,
    migrate::{MigrateError, Migrator},
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{net::IpAddr, str::FromStr, time::Duration};
use uuid::Uuid;

pub const MAX_CONEXOES: u32 = 10;
pub const TIMEOUT_AQUISICAO: Duration = Duration::from_secs(5);

/// Migrações embutidas no binário (`crates/felix-server/migrations`).
pub static MIGRACOES: Migrator = sqlx::migrate!("./migrations");

/// Opções de conexão com fuso UTC e `application_name`.
pub fn opcoes(database_url: &str) -> Result<PgConnectOptions, sqlx::Error> {
    Ok(PgConnectOptions::from_str(database_url)?
        .application_name("felix-server")
        .options([("timezone", "UTC")]))
}

/// Pool de produção: até 10 conexões, espera máxima de 5 s.
pub async fn conectar(config: &Config) -> Result<PgPool, sqlx::Error> {
    conectar_com(opcoes(&config.database_url)?).await
}

pub async fn conectar_com(opcoes: PgConnectOptions) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_CONEXOES)
        .acquire_timeout(TIMEOUT_AQUISICAO)
        .connect_with(opcoes)
        .await
}

/// Aplica as migrações pendentes.
pub async fn migrar(pool: &PgPool) -> Result<(), MigrateError> {
    MIGRACOES.run(pool).await
}

/// Abre a transação de tenant: `BEGIN; SET LOCAL ROLE felix_app;
/// set_config('app.org_id', org, true)`. Toda consulta em tabela de tenant
/// passa por aqui; o RLS forçado filtra e valida `organizacao_id`.
/// Lembre de `tx.commit().await?` nas mutações (o drop faz rollback).
pub async fn tenant(
    pool: &PgPool,
    org_id: Uuid,
) -> Result<Transaction<'static, Postgres>, ErroApi> {
    let mut tx = pool.begin().await?;
    entrar_tenant(&mut tx, org_id).await?;
    Ok(tx)
}

/// Aplica o contexto de tenant numa transação já aberta.
pub async fn entrar_tenant(conn: &mut PgConnection, org_id: Uuid) -> Result<(), ErroApi> {
    sqlx::query("SET LOCAL ROLE felix_app")
        .execute(&mut *conn)
        .await?;
    sqlx::query("SELECT set_config('app.org_id', $1, true)")
        .bind(org_id.to_string())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Registra uma mutação na auditoria (ADR-018), na mesma transação de tenant.
/// Nunca inclua senha, hash ou token em `dados`.
#[allow(clippy::too_many_arguments)]
pub async fn auditar(
    conn: &mut PgConnection,
    org_id: Uuid,
    usuario_id: Option<Uuid>,
    acao: &str,
    entidade: &str,
    entidade_id: Option<&str>,
    dados: serde_json::Value,
    ip: Option<IpAddr>,
) -> Result<(), ErroApi> {
    sqlx::query(
        "INSERT INTO auditoria (organizacao_id, usuario_id, acao, entidade, entidade_id, dados, ip)
         VALUES ($1, $2, $3, $4, $5, $6, $7::text::inet)",
    )
    .bind(org_id)
    .bind(usuario_id)
    .bind(acao)
    .bind(entidade)
    .bind(entidade_id)
    .bind(dados)
    .bind(ip.map(|ip| ip.to_string()))
    .execute(conn)
    .await?;
    Ok(())
}
