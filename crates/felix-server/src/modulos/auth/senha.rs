//! Troca de senha (logado), "esqueci a senha" e redefinição por token.

use super::{
    conferir, escopo_limitador,
    eventos::{self, TipoEvento},
    limitador,
    limitador::Acao,
};
use crate::{
    app::AppState,
    email,
    http::{ClienteInfo, Corpo, ErroApi, Sessao, sem_conteudo, sessao::revogar_sessoes_do_usuario},
    seguranca::{
        gerar_token, hash_senha_async, hash_token, token_bem_formado, validar_senha_forte,
        verificar_senha_async,
    },
};
use axum::{extract::State, http::StatusCode};
use chrono::TimeDelta;
use serde::Deserialize;
use std::collections::BTreeMap;
use tracing::Instrument;
use uuid::Uuid;

/// Validade do link de redefinição.
pub const VALIDADE_REDEFINICAO: TimeDelta = TimeDelta::hours(1);
const MSG_TOKEN_INVALIDO: &str =
    "Link de redefinição inválido ou expirado. Solicite um novo e-mail.";

#[derive(Debug, Deserialize)]
pub(super) struct TrocaSenha {
    #[serde(default)]
    senha_atual: String,
    #[serde(default)]
    senha_nova: String,
}

/// `POST /auth/me/senha`: confere a senha atual, grava a nova e revoga as outras sessões.
pub(super) async fn trocar(
    State(estado): State<AppState>,
    sessao: Sessao,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<TrocaSenha>,
) -> Result<StatusCode, ErroApi> {
    let escopo = escopo_limitador(&estado);
    limitador::verificar_usuario(&escopo, Acao::TrocarSenha, sessao.usuario_id)?;
    let mut erros = BTreeMap::new();
    if let Err(m) = validar_senha_forte(&corpo.senha_nova) {
        erros.insert("senha_nova".to_owned(), m);
    }
    let hash_atual: String = sqlx::query_scalar("SELECT senha_hash FROM usuarios WHERE id = $1")
        .bind(sessao.usuario_id)
        .fetch_optional(&estado.pool)
        .await?
        .ok_or_else(ErroApi::nao_autenticado)?;
    if !verificar_senha_async(corpo.senha_atual, hash_atual).await? {
        limitador::falha_usuario(&escopo, Acao::TrocarSenha, sessao.usuario_id)?;
        erros.insert(
            "senha_atual".to_owned(),
            "Senha atual incorreta.".to_owned(),
        );
    }
    conferir(erros)?;
    let hash = hash_senha_async(corpo.senha_nova).await?;
    let mut tx = estado.pool.begin().await?;
    sqlx::query("UPDATE usuarios SET senha_hash = $1, atualizado_em = now() WHERE id = $2")
        .bind(hash)
        .bind(sessao.usuario_id)
        .execute(&mut *tx)
        .await?;
    revogar_sessoes_do_usuario(&mut tx, sessao.usuario_id, Some(sessao.sessao_id)).await?;
    eventos::registrar(
        &mut *tx,
        TipoEvento::SenhaTrocada,
        Some(sessao.usuario_id),
        None,
        &cliente,
    )
    .await?;
    tx.commit().await?;
    Ok(sem_conteudo())
}

#[derive(Debug, Deserialize)]
pub(super) struct Esqueci {
    #[serde(default)]
    email: String,
}

/// `POST /auth/senha/esqueci`: sempre 204 imediato (exceto limite de tentativas
/// ou e-mail longo demais). Consulta, token e e-mail rodam em segundo plano para
/// o tempo de resposta não revelar se a conta existe.
pub(super) async fn esqueci(
    State(estado): State<AppState>,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<Esqueci>,
) -> Result<StatusCode, ErroApi> {
    let email = limitador::normalizar_email(&corpo.email)?;
    limitador::tentar(
        &escopo_limitador(&estado),
        Acao::Esqueci,
        cliente.ip,
        Some(email.as_str()).filter(|e| !e.is_empty()),
    )?;
    if email.is_empty() {
        return Ok(sem_conteudo());
    }
    tokio::spawn(
        async move {
            if let Err(erro) = enviar_link_redefinicao(&estado, &email).await {
                tracing::error!(erro = %erro, "Falha ao processar \"esqueci a senha\"");
            }
        }
        .instrument(tracing::Span::current()),
    );
    Ok(sem_conteudo())
}

/// Gera o token de redefinição (invalidando os anteriores) e enfileira o e-mail,
/// se a conta existir.
async fn enviar_link_redefinicao(estado: &AppState, email: &str) -> Result<(), ErroApi> {
    let mut tx = estado.pool.begin().await?;
    let usuario: Option<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, nome, email::text FROM usuarios WHERE email = $1")
            .bind(email)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((usuario_id, nome, email)) = usuario else {
        return Ok(());
    };
    let agora = estado.agora();
    // Um link válido por vez: invalida os anteriores.
    sqlx::query(
        "UPDATE tokens_usuario SET usado_em = $2
         WHERE usuario_id = $1 AND finalidade = 'redefinir_senha' AND usado_em IS NULL",
    )
    .bind(usuario_id)
    .bind(agora)
    .execute(&mut *tx)
    .await?;
    let token = gerar_token();
    sqlx::query(
        "INSERT INTO tokens_usuario (usuario_id, finalidade, token_hash, criado_em, expira_em)
         VALUES ($1, 'redefinir_senha', $2, $3, $4)",
    )
    .bind(usuario_id)
    .bind(hash_token(&token))
    .bind(agora)
    .bind(agora + VALIDADE_REDEFINICAO)
    .execute(&mut *tx)
    .await?;
    let link = format!("{}/redefinir-senha?token={token}", estado.config.public_url);
    let corpo_email = format!(
        "Olá, {nome}.\n\n\
         Recebemos um pedido para redefinir a sua senha no Felix.\n\
         Para criar uma nova senha, acesse o link abaixo (válido por 1 hora):\n\n\
         {link}\n\n\
         Se você não fez este pedido, ignore este e-mail; sua senha continua a mesma.\n"
    );
    email::enfileirar(
        &mut tx,
        &email,
        "Redefinição de senha do Felix",
        &corpo_email,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub(super) struct Redefinir {
    #[serde(default)]
    token: String,
    #[serde(default)]
    senha: String,
}

/// `POST /auth/senha/redefinir`: troca a senha, marca o token como usado e revoga todas as sessões.
/// O hash Argon2 é calculado antes de abrir a transação (sem segurar trava durante o hash).
pub(super) async fn redefinir(
    State(estado): State<AppState>,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<Redefinir>,
) -> Result<StatusCode, ErroApi> {
    limitador::tentar(
        &escopo_limitador(&estado),
        Acao::Redefinir,
        cliente.ip,
        None,
    )?;
    validar_senha_forte(&corpo.senha).map_err(|m| ErroApi::validacao_campo("senha", m))?;
    let token = corpo.token.trim();
    if !token_bem_formado(token) {
        return Err(ErroApi::validacao_campo("token", MSG_TOKEN_INVALIDO));
    }
    let token_hash = hash_token(token);
    let agora = estado.agora();
    const SELECT_TOKEN: &str = "SELECT id, usuario_id FROM tokens_usuario
         WHERE token_hash = $1 AND finalidade = 'redefinir_senha'
           AND usado_em IS NULL AND expira_em > $2";
    // Pré-checagem sem trava: token inválido não gasta Argon2.
    let previa: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(SELECT_TOKEN)
        .bind(&token_hash)
        .bind(agora)
        .fetch_optional(&estado.pool)
        .await?;
    if !matches!(previa, Some((_, Some(_)))) {
        return Err(ErroApi::validacao_campo("token", MSG_TOKEN_INVALIDO));
    }
    let hash = hash_senha_async(corpo.senha).await?;
    let mut tx = estado.pool.begin().await?;
    let linha: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(&format!("{SELECT_TOKEN} FOR UPDATE"))
        .bind(&token_hash)
        .bind(agora)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((token_id, Some(usuario_id))) = linha else {
        return Err(ErroApi::validacao_campo("token", MSG_TOKEN_INVALIDO));
    };
    sqlx::query("UPDATE usuarios SET senha_hash = $1, atualizado_em = now() WHERE id = $2")
        .bind(hash)
        .bind(usuario_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE tokens_usuario SET usado_em = $2 WHERE id = $1")
        .bind(token_id)
        .bind(agora)
        .execute(&mut *tx)
        .await?;
    revogar_sessoes_do_usuario(&mut tx, usuario_id, None).await?;
    eventos::registrar(
        &mut *tx,
        TipoEvento::SenhaRedefinida,
        Some(usuario_id),
        None,
        &cliente,
    )
    .await?;
    tx.commit().await?;
    Ok(sem_conteudo())
}
