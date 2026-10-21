//! Identidade (ADR-006): cadastro, login, logout, `Me`, troca e redefinição de senha.

pub mod eventos;
pub mod limitador;
mod me;
mod senha;

pub use me::{Me, OrgResumo, UsuarioMe, carregar_me, normalizar_email, validar_nome};

use crate::{
    app::AppState,
    db,
    http::{
        ClienteInfo, Corpo, ErroApi, Sessao, SessaoOpcional, dados,
        sessao::{cookie_remocao, criar_sessao, revogar_sessao},
    },
    modulos::assinatura::DIAS_TESTE,
    seguranca::{hash_senha_async, validar_senha_forte, verificar_senha_async},
};
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::TimeDelta;
use eventos::TipoEvento;
use limitador::Acao;
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, sync::OnceLock};
use uuid::Uuid;

/// Mensagem do 409 de cadastro com e-mail já usado (não confirma a conta além do necessário).
pub const MSG_EMAIL_EM_USO: &str = "Não foi possível concluir o cadastro com este e-mail. Se você já tem conta, entre ou recupere a senha.";

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route("/auth/cadastro", post(cadastro))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(obter_me).patch(atualizar_me))
        .route("/auth/me/senha", post(senha::trocar))
        .route("/auth/senha/esqueci", post(senha::esqueci))
        .route("/auth/senha/redefinir", post(senha::redefinir))
}

/// Escopo do limitador de tentativas: o banco do pool (um por app).
pub(crate) fn escopo_limitador(estado: &AppState) -> String {
    estado
        .pool
        .connect_options()
        .get_database()
        .unwrap_or_default()
        .to_owned()
}

/// Insere `campo` em `erros` se `resultado` for erro; devolve o valor se ok.
pub(crate) fn coletar<T>(
    erros: &mut BTreeMap<String, String>,
    campo: &str,
    resultado: Result<T, String>,
) -> Option<T> {
    match resultado {
        Ok(v) => Some(v),
        Err(m) => {
            erros.insert(campo.to_owned(), m);
            None
        }
    }
}

/// Falha com 422 se houver erros acumulados.
pub(crate) fn conferir(erros: BTreeMap<String, String>) -> Result<(), ErroApi> {
    match ErroApi::validacao_campos(erros) {
        Some(erro) => Err(erro),
        None => Ok(()),
    }
}

fn eh_violacao_unica(erro: &sqlx::Error) -> bool {
    matches!(erro, sqlx::Error::Database(db) if db.kind() == sqlx::error::ErrorKind::UniqueViolation)
}

#[derive(Debug, Deserialize)]
struct Cadastro {
    #[serde(default)]
    nome: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    senha: String,
    #[serde(default)]
    organizacao_nome: String,
}

async fn cadastro(
    State(estado): State<AppState>,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<Cadastro>,
) -> Result<impl IntoResponse, ErroApi> {
    let mut erros = BTreeMap::new();
    let nome = coletar(&mut erros, "nome", validar_nome(&corpo.nome, "seu nome"));
    let email = coletar(&mut erros, "email", normalizar_email(&corpo.email));
    let org_nome = coletar(
        &mut erros,
        "organizacao_nome",
        validar_nome(&corpo.organizacao_nome, "o nome da organização"),
    );
    coletar(&mut erros, "senha", validar_senha_forte(&corpo.senha));
    conferir(erros)?;
    let (Some(nome), Some(email), Some(org_nome)) = (nome, email, org_nome) else {
        return Err(ErroApi::interno_log("cadastro", "validação inconsistente"));
    };
    limitador::tentar(&escopo_limitador(&estado), Acao::Cadastro, cliente.ip, None)?;
    let hash = hash_senha_async(corpo.senha).await?;
    let agora = estado.agora();

    let mut tx = estado.pool.begin().await?;
    let usuario_id: Uuid = sqlx::query_scalar(
        "INSERT INTO usuarios (email, nome, senha_hash) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&email)
    .bind(&nome)
    .bind(&hash)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if eh_violacao_unica(&e) {
            ErroApi::conflito(MSG_EMAIL_EM_USO)
        } else {
            ErroApi::from(e)
        }
    })?;
    let org_id: Uuid = sqlx::query_scalar(
        "INSERT INTO organizacoes (nome, criado_por) VALUES ($1, $2) RETURNING id",
    )
    .bind(&org_nome)
    .bind(usuario_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO membros (organizacao_id, usuario_id, papel) VALUES ($1, $2, 'dono')")
        .bind(org_id)
        .bind(usuario_id)
        .execute(&mut *tx)
        .await?;
    let teste_ate = agora + TimeDelta::days(DIAS_TESTE);
    sqlx::query(
        "INSERT INTO assinaturas (organizacao_id, plano, status, teste_ate, provedor)
         VALUES ($1, 'profissional', 'teste', $2, 'manual')",
    )
    .bind(org_id)
    .bind(teste_ate)
    .execute(&mut *tx)
    .await?;
    db::entrar_tenant(&mut tx, org_id).await?;
    db::auditar(
        &mut tx,
        org_id,
        Some(usuario_id),
        "cadastro",
        "organizacao",
        Some(&org_id.to_string()),
        json!({"nome": org_nome, "plano": "profissional", "status": "teste", "teste_ate": teste_ate}),
        cliente.ip,
    )
    .await?;
    let sessao = criar_sessao(
        &mut *tx,
        estado.config.cookie_seguro(),
        usuario_id,
        cliente.ip,
        cliente.user_agent.as_deref(),
    )
    .await?;
    tx.commit().await?;
    let me = carregar_me(&estado.pool, usuario_id, agora).await?;
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, sessao.cookie)],
        dados(me),
    ))
}

#[derive(Debug, Deserialize)]
struct Login {
    #[serde(default)]
    email: String,
    #[serde(default)]
    senha: String,
}

/// Hash fictício usado quando o e-mail não existe, para o tempo de resposta
/// ser semelhante ao de senha errada.
async fn hash_ficticio() -> Result<String, ErroApi> {
    static HASH: OnceLock<String> = OnceLock::new();
    if let Some(hash) = HASH.get() {
        return Ok(hash.clone());
    }
    let hash = hash_senha_async("felix-senha-ficticia-para-tempo-constante".into()).await?;
    Ok(HASH.get_or_init(|| hash).clone())
}

async fn login(
    State(estado): State<AppState>,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<Login>,
) -> Result<impl IntoResponse, ErroApi> {
    let email = limitador::normalizar_email(&corpo.email)?;
    let escopo = escopo_limitador(&estado);
    limitador::verificar_login(&escopo, cliente.ip, &email)?;
    let linha: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, senha_hash FROM usuarios WHERE email = $1")
            .bind(&email)
            .fetch_optional(&estado.pool)
            .await?;
    let (existente, hash) = match linha {
        Some((id, hash)) => (Some(id), hash),
        None => (None, hash_ficticio().await?),
    };
    let senha_ok = verificar_senha_async(corpo.senha, hash).await?;
    let Some(usuario_id) = existente.filter(|_| senha_ok) else {
        limitador::falha_login(&escopo, cliente.ip, &email)?;
        eventos::registrar_ou_logar(
            &estado.pool,
            TipoEvento::LoginFalha,
            existente,
            Some(&email),
            &cliente,
        )
        .await;
        return Err(ErroApi::credenciais_invalidas());
    };
    limitador::sucesso_login(&escopo, &email)?;
    let sessao = criar_sessao(
        &estado.pool,
        estado.config.cookie_seguro(),
        usuario_id,
        cliente.ip,
        cliente.user_agent.as_deref(),
    )
    .await?;
    eventos::registrar_ou_logar(
        &estado.pool,
        TipoEvento::LoginSucesso,
        Some(usuario_id),
        Some(&email),
        &cliente,
    )
    .await;
    let me = carregar_me(&estado.pool, usuario_id, estado.agora()).await?;
    Ok(([(header::SET_COOKIE, sessao.cookie)], dados(me)))
}

async fn logout(
    State(estado): State<AppState>,
    SessaoOpcional(sessao): SessaoOpcional,
    cliente: ClienteInfo,
) -> Result<impl IntoResponse, ErroApi> {
    if let Some(sessao) = sessao {
        revogar_sessao(&estado.pool, sessao.sessao_id).await?;
        eventos::registrar_ou_logar(
            &estado.pool,
            TipoEvento::Logout,
            Some(sessao.usuario_id),
            None,
            &cliente,
        )
        .await;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            cookie_remocao(estado.config.cookie_seguro()),
        )],
    ))
}

async fn obter_me(
    State(estado): State<AppState>,
    sessao: Sessao,
) -> Result<Json<impl serde::Serialize>, ErroApi> {
    let me = carregar_me(&estado.pool, sessao.usuario_id, estado.agora()).await?;
    Ok(dados(me))
}

#[derive(Debug, Deserialize)]
struct AtualizarMe {
    #[serde(default)]
    nome: String,
}

async fn atualizar_me(
    State(estado): State<AppState>,
    sessao: Sessao,
    Corpo(corpo): Corpo<AtualizarMe>,
) -> Result<Json<impl serde::Serialize>, ErroApi> {
    let nome =
        validar_nome(&corpo.nome, "seu nome").map_err(|m| ErroApi::validacao_campo("nome", m))?;
    sqlx::query("UPDATE usuarios SET nome = $1, atualizado_em = now() WHERE id = $2")
        .bind(&nome)
        .bind(sessao.usuario_id)
        .execute(&estado.pool)
        .await?;
    let me = carregar_me(&estado.pool, sessao.usuario_id, estado.agora()).await?;
    Ok(dados(me))
}
