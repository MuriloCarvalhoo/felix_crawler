//! Limpeza diária (etapa da rodada `alertas_diarios`): apaga registros que só
//! ocupam espaço e, no caso de sessões, tokens e e-mails, reduzem a exposição
//! de dados antigos.
//!
//! - sessões expiradas ou revogadas há mais de [`DIAS_SESSOES`] dias;
//! - tokens de usuário expirados ou usados há mais de [`DIAS_TOKENS`] dias;
//! - jobs `concluido`/`falhou` parados há mais de [`DIAS_JOBS`] dias;
//! - notificações lidas há mais de [`DIAS_NOTIFICACOES`] dias (por organização,
//!   pois a tabela tem RLS forçado);
//! - e-mails da caixa de saída com mais de [`DIAS_EMAILS`] dias
//!   ([`crate::email::purgar_antigos`]).

use crate::{app::AppState, db, email, http::ErroApi};
use uuid::Uuid;

pub const DIAS_SESSOES: i32 = 30;
pub const DIAS_TOKENS: i32 = 30;
pub const DIAS_JOBS: i32 = 30;
pub const DIAS_NOTIFICACOES: i32 = 90;
pub const DIAS_EMAILS: u32 = 30;

/// Linhas apagadas em cada categoria.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoLimpeza {
    pub sessoes: u64,
    pub tokens: u64,
    pub jobs: u64,
    pub notificacoes: u64,
    pub emails: u64,
}

/// Executa todas as etapas. Falha de uma etapa (ou de uma organização) não
/// impede as demais; o último erro é devolvido ao final.
pub async fn executar(estado: &AppState) -> Result<ResumoLimpeza, ErroApi> {
    let pool = &estado.pool;
    let mut resumo = ResumoLimpeza::default();
    let mut ultimo_erro = None;
    let mut anotar = |etapa: &str, erro: ErroApi| {
        tracing::error!(etapa, erro = %erro, "Falha na limpeza diária");
        ultimo_erro = Some(erro);
    };

    match apagar(
        pool,
        "DELETE FROM sessoes
         WHERE expira_em < now() - make_interval(days => $1)
            OR revogada_em < now() - make_interval(days => $1)",
        DIAS_SESSOES,
    )
    .await
    {
        Ok(n) => resumo.sessoes = n,
        Err(erro) => anotar("sessoes", erro),
    }
    match apagar(
        pool,
        "DELETE FROM tokens_usuario
         WHERE expira_em < now() - make_interval(days => $1)
            OR usado_em < now() - make_interval(days => $1)",
        DIAS_TOKENS,
    )
    .await
    {
        Ok(n) => resumo.tokens = n,
        Err(erro) => anotar("tokens", erro),
    }
    match apagar(
        pool,
        "DELETE FROM jobs
         WHERE status IN ('concluido', 'falhou')
           AND atualizado_em < now() - make_interval(days => $1)",
        DIAS_JOBS,
    )
    .await
    {
        Ok(n) => resumo.jobs = n,
        Err(erro) => anotar("jobs", erro),
    }
    match apagar_notificacoes(estado).await {
        Ok(n) => resumo.notificacoes = n,
        Err(erro) => anotar("notificacoes", erro),
    }
    match email::purgar_antigos(pool, DIAS_EMAILS).await {
        Ok(n) => resumo.emails = n,
        Err(erro) => anotar("emails", erro),
    }
    tracing::info!(
        sessoes = resumo.sessoes,
        tokens = resumo.tokens,
        jobs = resumo.jobs,
        notificacoes = resumo.notificacoes,
        emails = resumo.emails,
        "Limpeza diária concluída"
    );
    match ultimo_erro {
        Some(erro) => Err(erro),
        None => Ok(resumo),
    }
}

async fn apagar(pool: &sqlx::PgPool, sql: &str, dias: i32) -> Result<u64, ErroApi> {
    Ok(sqlx::query(sql)
        .bind(dias)
        .execute(pool)
        .await?
        .rows_affected())
}

/// Notificações lidas antigas, organização por organização (RLS).
async fn apagar_notificacoes(estado: &AppState) -> Result<u64, ErroApi> {
    let orgs: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM organizacoes ORDER BY id")
        .fetch_all(&estado.pool)
        .await?;
    let mut total = 0;
    let mut ultimo_erro = None;
    for org in orgs {
        match apagar_notificacoes_da_org(estado, org).await {
            Ok(n) => total += n,
            Err(erro) => {
                tracing::error!(org = %org, erro = %erro, "Falha ao limpar notificações da organização");
                ultimo_erro = Some(erro);
            }
        }
    }
    match ultimo_erro {
        Some(erro) => Err(erro),
        None => Ok(total),
    }
}

async fn apagar_notificacoes_da_org(estado: &AppState, org: Uuid) -> Result<u64, ErroApi> {
    let mut tx = db::tenant(&estado.pool, org).await?;
    let n = sqlx::query(
        "DELETE FROM notificacoes
         WHERE organizacao_id = $1 AND lida_em < now() - make_interval(days => $2)",
    )
    .bind(org)
    .bind(DIAS_NOTIFICACOES)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(n)
}
