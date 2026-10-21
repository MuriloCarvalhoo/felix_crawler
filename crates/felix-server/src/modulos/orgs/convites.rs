//! Convites: listar/criar/revogar (admin) e o fluxo público de aceite por token.

use super::travar_org;
use crate::{
    app::AppState,
    db, email,
    http::{
        Caminho, ClienteInfo, Corpo, ErroApi, OrgCtx, Papel, SessaoOpcional, criado, dados,
        sem_conteudo, sessao::criar_sessao,
    },
    modulos::{
        assinatura::{Recurso, verificar_limite},
        auth::{
            Me, carregar_me, conferir, escopo_limitador,
            limitador::{self, Acao},
            normalizar_email, validar_nome,
        },
    },
    seguranca::{
        gerar_token, hash_senha_async, hash_token, token_bem_formado, validar_senha_forte,
    },
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use uuid::Uuid;

/// Validade do convite.
pub const VALIDADE_CONVITE: TimeDelta = TimeDelta::days(7);
/// Convites criados por organização a cada 24 h (inclui revogados).
pub const COTA_CONVITES_DIA: i64 = 20;
const MSG_CONVITE_INVALIDO: &str = "Convite inválido, expirado ou já utilizado.";

pub(super) fn rotas() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org}/convites", get(listar).post(criar))
        .route("/orgs/{org}/convites/{id}", delete(revogar))
        .route("/convites/{token}", get(publico))
        .route("/convites/{token}/aceitar", post(aceitar))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Convite {
    pub id: Uuid,
    pub email: String,
    pub papel: Papel,
    pub expira_em: DateTime<Utc>,
    pub criado_em: DateTime<Utc>,
}

async fn listar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
) -> Result<Json<impl Serialize>, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let convites: Vec<Convite> = sqlx::query_as(
        "SELECT id, email::text AS email, papel, expira_em, criado_em FROM convites
         WHERE organizacao_id = $1 AND aceito_em IS NULL AND revogado_em IS NULL AND expira_em > $2
         ORDER BY criado_em DESC",
    )
    .bind(ctx.org_id)
    .bind(estado.agora())
    .fetch_all(&estado.pool)
    .await?;
    Ok(dados(convites))
}

#[derive(Debug, Deserialize)]
struct NovoConvite {
    #[serde(default)]
    email: String,
    papel: Papel,
}

async fn criar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<NovoConvite>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    if corpo.papel == Papel::Dono && ctx.papel != Papel::Dono {
        return Err(ErroApi::proibido(
            "Somente um dono pode convidar outro dono.",
        ));
    }
    let para = normalizar_email(&corpo.email).map_err(|m| ErroApi::validacao_campo("email", m))?;
    let agora = estado.agora();
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    travar_org(&mut tx, ctx.org_id).await?;
    let criados_no_dia: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM convites
         WHERE organizacao_id = $1 AND criado_em > $2 - interval '24 hours'",
    )
    .bind(ctx.org_id)
    .bind(agora)
    .fetch_one(&mut *tx)
    .await?;
    if criados_no_dia >= COTA_CONVITES_DIA {
        return Err(ErroApi::ocupado_msg(format!(
            "Limite de {COTA_CONVITES_DIA} convites por dia nesta organização atingido. \
             Tente novamente amanhã."
        )));
    }
    let ja_membro: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM membros m JOIN usuarios u ON u.id = m.usuario_id
                        WHERE m.organizacao_id = $1 AND u.email = $2)",
    )
    .bind(ctx.org_id)
    .bind(&para)
    .fetch_one(&mut *tx)
    .await?;
    if ja_membro {
        return Err(ErroApi::conflito("Esta pessoa já é membro da organização."));
    }
    let pendente: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM convites
                        WHERE organizacao_id = $1 AND email = $2 AND aceito_em IS NULL
                          AND revogado_em IS NULL AND expira_em > $3)",
    )
    .bind(ctx.org_id)
    .bind(&para)
    .bind(agora)
    .fetch_one(&mut *tx)
    .await?;
    if pendente {
        return Err(ErroApi::conflito(
            "Já existe um convite pendente para este e-mail. Revogue-o para enviar outro.",
        ));
    }
    verificar_limite(&mut tx, ctx.org_id, Recurso::Usuario, agora).await?;
    let token = gerar_token();
    let convite: Convite = sqlx::query_as(
        "INSERT INTO convites (organizacao_id, email, papel, token_hash, convidado_por, criado_em, expira_em)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, email::text AS email, papel, expira_em, criado_em",
    )
    .bind(ctx.org_id)
    .bind(&para)
    .bind(corpo.papel)
    .bind(hash_token(&token))
    .bind(ctx.usuario_id)
    .bind(agora)
    .bind(agora + VALIDADE_CONVITE)
    .fetch_one(&mut *tx)
    .await?;
    let (org_nome, quem): (String, String) = sqlx::query_as(
        "SELECT o.nome, u.nome FROM organizacoes o, usuarios u WHERE o.id = $1 AND u.id = $2",
    )
    .bind(ctx.org_id)
    .bind(ctx.usuario_id)
    .fetch_one(&mut *tx)
    .await?;
    let link = format!("{}/convite/{token}", estado.config.public_url);
    let corpo_email = format!(
        "Olá.\n\n\
         {quem} convidou você para participar da organização \"{org_nome}\" no Felix \
         com o papel {papel}.\n\n\
         Para aceitar, acesse o link abaixo (válido por 7 dias):\n\n\
         {link}\n\n\
         Se você não esperava este convite, ignore este e-mail.\n",
        papel = corpo.papel
    );
    email::enfileirar(
        &mut tx,
        &para,
        &format!("Convite para {org_nome} no Felix"),
        &corpo_email,
    )
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "criar",
        "convite",
        Some(&convite.id.to_string()),
        json!({"email": para, "papel": corpo.papel}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(criado(convite))
}

async fn revogar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
) -> Result<StatusCode, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let revogado: Option<(String, Papel)> = sqlx::query_as(
        "UPDATE convites SET revogado_em = $3
         WHERE id = $1 AND organizacao_id = $2 AND aceito_em IS NULL AND revogado_em IS NULL
         RETURNING email::text, papel",
    )
    .bind(id)
    .bind(ctx.org_id)
    .bind(estado.agora())
    .fetch_optional(&mut *tx)
    .await?;
    let (email, papel) =
        revogado.ok_or_else(|| ErroApi::nao_encontrado_msg("Convite não encontrado."))?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "revogar",
        "convite",
        Some(&id.to_string()),
        json!({"email": email, "papel": papel}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(sem_conteudo())
}

/// Convite válido localizado pelo token. Só vale enquanto quem convidou continua
/// membro com papel admin ou dono (dono, para convites de dono).
#[derive(Debug, sqlx::FromRow)]
struct ConviteValido {
    id: Uuid,
    organizacao_id: Uuid,
    organizacao_nome: String,
    email: String,
    papel: Papel,
    usuario_id: Option<Uuid>,
}

async fn buscar_por_token<'e, E: sqlx::PgExecutor<'e>>(
    executor: E,
    token: &str,
    agora: DateTime<Utc>,
    travar: bool,
) -> Result<ConviteValido, ErroApi> {
    if !token_bem_formado(token) {
        return Err(ErroApi::nao_encontrado_msg(MSG_CONVITE_INVALIDO));
    }
    let sql = format!(
        "SELECT c.id, c.organizacao_id, o.nome AS organizacao_nome, c.email::text AS email,
                c.papel, u.id AS usuario_id
         FROM convites c
         JOIN organizacoes o ON o.id = c.organizacao_id
         JOIN membros quem ON quem.organizacao_id = c.organizacao_id
                          AND quem.usuario_id = c.convidado_por
                          AND quem.papel IN ('admin', 'dono')
                          AND (c.papel <> 'dono' OR quem.papel = 'dono')
         LEFT JOIN usuarios u ON u.email = c.email
         WHERE c.token_hash = $1 AND c.aceito_em IS NULL AND c.revogado_em IS NULL
           AND c.expira_em > $2 AND o.excluida_em IS NULL{}",
        if travar { " FOR UPDATE OF c" } else { "" }
    );
    sqlx::query_as(&sql)
        .bind(hash_token(token))
        .bind(agora)
        .fetch_optional(executor)
        .await?
        .ok_or_else(|| ErroApi::nao_encontrado_msg(MSG_CONVITE_INVALIDO))
}

#[derive(Debug, Serialize)]
struct ConvitePublico {
    organizacao_nome: String,
    email: String,
    papel: Papel,
    usuario_existe: bool,
}

/// `GET /convites/{token}` (público).
async fn publico(
    State(estado): State<AppState>,
    Caminho(token): Caminho<String>,
) -> Result<Json<impl Serialize>, ErroApi> {
    let c = buscar_por_token(&estado.pool, &token, estado.agora(), false).await?;
    Ok(dados(ConvitePublico {
        organizacao_nome: c.organizacao_nome,
        email: c.email,
        papel: c.papel,
        usuario_existe: c.usuario_id.is_some(),
    }))
}

#[derive(Debug, Default, Deserialize)]
struct Aceite {
    nome: Option<String>,
    senha: Option<String>,
}

/// Resposta do aceite: o `Me` e a organização em que a pessoa entrou.
#[derive(Debug, Serialize)]
struct AceiteConcluido {
    #[serde(flatten)]
    me: Me,
    organizacao_id: Uuid,
}

/// Valida `{nome, senha}` de quem cria a conta pelo convite e devolve (nome, hash).
async fn preparar_conta(corpo: Aceite) -> Result<(String, String), ErroApi> {
    let mut erros = BTreeMap::new();
    let nome = match validar_nome(corpo.nome.as_deref().unwrap_or_default(), "seu nome") {
        Ok(n) => Some(n),
        Err(m) => {
            erros.insert("nome".to_owned(), m);
            None
        }
    };
    let senha = corpo.senha.unwrap_or_default();
    if let Err(m) = validar_senha_forte(&senha) {
        erros.insert("senha".to_owned(), m);
    }
    conferir(erros)?;
    let nome = nome.unwrap_or_default();
    let hash = hash_senha_async(senha).await?;
    Ok((nome, hash))
}

/// `POST /convites/{token}/aceitar`. Usuário existente: precisa estar logado
/// com o e-mail do convite. Usuário novo: cria a conta com `{nome, senha}` e entra.
async fn aceitar(
    State(estado): State<AppState>,
    SessaoOpcional(sessao): SessaoOpcional,
    cliente: ClienteInfo,
    Caminho(token): Caminho<String>,
    corpo: Bytes,
) -> Result<Response, ErroApi> {
    limitador::tentar(
        &escopo_limitador(&estado),
        Acao::AceitarConvite,
        cliente.ip,
        None,
    )?;
    // Corpo opcional: vazio vale como `{}` (usuário existente não envia nada).
    let corpo: Aceite = if corpo.iter().all(u8::is_ascii_whitespace) {
        Aceite::default()
    } else {
        serde_json::from_slice(&corpo)
            .map_err(|e| ErroApi::validacao(format!("JSON inválido: {e}")))?
    };
    let agora = estado.agora();
    // Pré-checagem sem trava; para conta nova, valida e calcula o hash Argon2
    // antes de abrir a transação (sem segurar a trava do convite durante o hash).
    let previa = buscar_por_token(&estado.pool, &token, agora, false).await?;
    let nova_conta = match previa.usuario_id {
        Some(_) => None,
        None => Some(preparar_conta(corpo).await?),
    };
    let mut tx = estado.pool.begin().await?;
    let convite = buscar_por_token(&mut *tx, &token, agora, true).await?;
    let mut nova_sessao = None;
    let usuario_id = match (convite.usuario_id, nova_conta) {
        (Some(existente), _) => {
            let Some(sessao) = sessao else {
                return Err(ErroApi::novo(
                    StatusCode::UNAUTHORIZED,
                    "nao_autenticado",
                    format!(
                        "Entre com a conta {} para aceitar o convite.",
                        convite.email
                    ),
                ));
            };
            if sessao.usuario_id != existente {
                return Err(ErroApi::proibido(format!(
                    "Este convite foi enviado para {}. Entre com essa conta para aceitá-lo.",
                    convite.email
                )));
            }
            existente
        }
        (None, Some((nome, hash))) => {
            // O token chegou por e-mail: o endereço está verificado.
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO usuarios (email, nome, senha_hash, email_verificado_em)
                 VALUES ($1, $2, $3, $4) RETURNING id",
            )
            .bind(&convite.email)
            .bind(&nome)
            .bind(hash)
            .bind(agora)
            .fetch_one(&mut *tx)
            .await?;
            nova_sessao = Some(
                criar_sessao(
                    &mut *tx,
                    estado.config.cookie_seguro(),
                    id,
                    cliente.ip,
                    cliente.user_agent.as_deref(),
                )
                .await?,
            );
            id
        }
        (None, None) => {
            return Err(ErroApi::conflito(
                "O convite mudou durante o aceite. Recarregue a página e tente novamente.",
            ));
        }
    };
    let inserido = sqlx::query(
        "INSERT INTO membros (organizacao_id, usuario_id, papel) VALUES ($1, $2, $3)
         ON CONFLICT (organizacao_id, usuario_id) DO NOTHING",
    )
    .bind(convite.organizacao_id)
    .bind(usuario_id)
    .bind(convite.papel)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserido == 0 {
        return Err(ErroApi::conflito("Você já é membro desta organização."));
    }
    sqlx::query("UPDATE convites SET aceito_em = $2 WHERE id = $1")
        .bind(convite.id)
        .bind(agora)
        .execute(&mut *tx)
        .await?;
    db::entrar_tenant(&mut tx, convite.organizacao_id).await?;
    db::auditar(
        &mut tx,
        convite.organizacao_id,
        Some(usuario_id),
        "aceitar",
        "convite",
        Some(&convite.id.to_string()),
        json!({"email": convite.email, "papel": convite.papel, "novo_usuario": nova_sessao.is_some()}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    let me = AceiteConcluido {
        me: carregar_me(&estado.pool, usuario_id, agora).await?,
        organizacao_id: convite.organizacao_id,
    };
    Ok(match nova_sessao {
        Some(s) => ([(header::SET_COOKIE, s.cookie)], dados(me)).into_response(),
        None => dados(me).into_response(),
    })
}
