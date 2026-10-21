//! Eventos de autenticação (ADR-021): login (sucesso/falha), logout, troca e
//! redefinição de senha em `eventos_autenticacao`. Só registro, sem endpoint.
//! Nunca grava senha; o e-mail vai como SHA-256 do texto normalizado.

use crate::http::{ClienteInfo, ErroApi};
use sha2::{Digest, Sha256};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoEvento {
    LoginSucesso,
    LoginFalha,
    Logout,
    SenhaTrocada,
    SenhaRedefinida,
}

impl TipoEvento {
    pub fn como_texto(self) -> &'static str {
        match self {
            TipoEvento::LoginSucesso => "login_sucesso",
            TipoEvento::LoginFalha => "login_falha",
            TipoEvento::Logout => "logout",
            TipoEvento::SenhaTrocada => "senha_trocada",
            TipoEvento::SenhaRedefinida => "senha_redefinida",
        }
    }
}

/// SHA-256 do e-mail normalizado (apara e minúsculas).
pub fn hash_email(email: &str) -> Vec<u8> {
    Sha256::digest(email.trim().to_lowercase().as_bytes()).to_vec()
}

/// Grava o evento (pode participar de uma transação: `&mut *tx`).
pub async fn registrar<'e, E: PgExecutor<'e>>(
    executor: E,
    tipo: TipoEvento,
    usuario_id: Option<Uuid>,
    email: Option<&str>,
    cliente: &ClienteInfo,
) -> Result<(), ErroApi> {
    sqlx::query(
        "INSERT INTO eventos_autenticacao (usuario_id, email_hash, tipo, ip, user_agent)
         VALUES ($1, $2, $3, $4::text::inet, $5)",
    )
    .bind(usuario_id)
    .bind(email.map(hash_email))
    .bind(tipo.como_texto())
    .bind(cliente.ip.map(|ip| ip.to_string()))
    .bind(cliente.user_agent.as_deref())
    .execute(executor)
    .await?;
    Ok(())
}

/// Como [`registrar`], fora de transação: falha só é registrada no log (o
/// evento não pode derrubar login/logout).
pub async fn registrar_ou_logar(
    pool: &PgPool,
    tipo: TipoEvento,
    usuario_id: Option<Uuid>,
    email: Option<&str>,
    cliente: &ClienteInfo,
) {
    if let Err(erro) = registrar(pool, tipo, usuario_id, email, cliente).await {
        tracing::error!(tipo = tipo.como_texto(), erro = %erro, "Falha ao registrar evento de autenticação");
    }
}
