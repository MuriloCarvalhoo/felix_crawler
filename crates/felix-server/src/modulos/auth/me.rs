//! Tipo `Me` e validações de entrada compartilhadas (e-mail, nome).

use crate::{
    http::{ErroApi, Papel},
    modulos::assinatura::{Plano, plano_vigente},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgExecutor;
use uuid::Uuid;

pub const NOME_MAX: usize = 120;
pub const EMAIL_MAX: usize = 254;

/// `Me` = `{usuario: {id, nome, email}, organizacoes: [{id, nome, papel, plano}]}`.
#[derive(Debug, Clone, Serialize)]
pub struct Me {
    pub usuario: UsuarioMe,
    pub organizacoes: Vec<OrgResumo>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UsuarioMe {
    pub id: Uuid,
    pub nome: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrgResumo {
    pub id: Uuid,
    pub nome: String,
    pub papel: Papel,
    pub plano: Plano,
}

/// Monta o `Me` (somente organizações não excluídas, com o plano efetivo).
pub async fn carregar_me<'c, E>(
    executor: E,
    usuario_id: Uuid,
    agora: DateTime<Utc>,
) -> Result<Me, ErroApi>
where
    E: PgExecutor<'c> + Copy,
{
    let usuario: UsuarioMe =
        sqlx::query_as("SELECT id, nome, email::text AS email FROM usuarios WHERE id = $1")
            .bind(usuario_id)
            .fetch_optional(executor)
            .await?
            .ok_or_else(ErroApi::nao_autenticado)?;
    type Linha = (
        Uuid,
        String,
        Papel,
        Option<String>,
        Option<String>,
        Option<DateTime<Utc>>,
    );
    let linhas: Vec<Linha> = sqlx::query_as(
        "SELECT o.id, o.nome, m.papel, a.plano, a.status, a.teste_ate
         FROM membros m
         JOIN organizacoes o ON o.id = m.organizacao_id
         LEFT JOIN assinaturas a ON a.organizacao_id = o.id
         WHERE m.usuario_id = $1 AND o.excluida_em IS NULL
         ORDER BY o.criado_em, o.id",
    )
    .bind(usuario_id)
    .fetch_all(executor)
    .await?;
    let organizacoes = linhas
        .into_iter()
        .map(|(id, nome, papel, plano, status, teste_ate)| OrgResumo {
            id,
            nome,
            papel,
            plano: plano_vigente(
                plano.as_deref().unwrap_or("gratuito"),
                status.as_deref().unwrap_or("ativa"),
                teste_ate,
                agora,
            ),
        })
        .collect();
    Ok(Me {
        usuario,
        organizacoes,
    })
}

/// Normaliza (apara e põe em minúsculas) e valida o formato do e-mail.
/// Erro = mensagem para o campo `email`.
pub fn normalizar_email(texto: &str) -> Result<String, String> {
    let email = texto.trim().to_lowercase();
    let invalido = || "Informe um e-mail válido.".to_owned();
    if email.is_empty() {
        return Err("Informe o e-mail.".into());
    }
    if email.chars().count() > EMAIL_MAX
        || email.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(invalido());
    }
    let Some((local, dominio)) = email.split_once('@') else {
        return Err(invalido());
    };
    if local.is_empty()
        || dominio.contains('@')
        || !dominio.contains('.')
        || dominio.starts_with('.')
        || dominio.ends_with('.')
    {
        return Err(invalido());
    }
    Ok(email)
}

/// Apara e valida um nome (1–120 caracteres). Erro = mensagem para o campo.
pub fn validar_nome(texto: &str, rotulo: &str) -> Result<String, String> {
    let nome = texto.trim();
    if nome.is_empty() {
        return Err(format!("Informe {rotulo}."));
    }
    if nome.chars().count() > NOME_MAX {
        return Err(format!("Use no máximo {NOME_MAX} caracteres."));
    }
    if nome.chars().any(char::is_control) {
        return Err("Remova caracteres de controle.".into());
    }
    Ok(nome.to_owned())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn emails() {
        assert_eq!(
            normalizar_email("  Maria@Exemplo.COM ").as_deref(),
            Ok("maria@exemplo.com")
        );
        for ruim in [
            "",
            "sem-arroba",
            "@x.com",
            "a@b",
            "a@@b.com",
            "a b@c.com",
            "a@.com",
            "a@b.com.",
        ] {
            assert!(normalizar_email(ruim).is_err(), "{ruim}");
        }
        assert!(normalizar_email(&format!("{}@x.com", "a".repeat(250))).is_err());
    }

    #[test]
    fn nomes() {
        assert_eq!(validar_nome("  Ana  ", "o nome").as_deref(), Ok("Ana"));
        assert!(validar_nome("   ", "o nome").is_err());
        assert!(validar_nome(&"x".repeat(121), "o nome").is_err());
    }
}
