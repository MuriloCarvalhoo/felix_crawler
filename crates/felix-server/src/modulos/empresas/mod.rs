//! Empresas fornecedoras (CNPJs) da organização.

use crate::{
    app::AppState,
    db,
    http::{Caminho, ClienteInfo, Corpo, ErroApi, OrgCtx, Papel, criado, dados, sem_conteudo},
    modulos::assinatura::{Recurso, verificar_limite},
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use uuid::Uuid;

pub const RAZAO_SOCIAL_MAX: usize = 200;

/// Valida um CNPJ numérico ou alfanumérico (IN RFB 2.229/2024), com ou sem
/// máscara (`12.345.678/0001-95`, `12.ABC.345/01DE-35`), e devolve as 14
/// posições normalizadas (maiúsculas, sem máscara). Regras: 12 primeiras
/// posições em `[A-Z0-9]`, 2 últimas dígitos; DV por módulo 11 com pesos
/// 5..2,9..2 e 6..2,9..2, valor de cada caractere = código ASCII − 48;
/// recusa 14 caracteres iguais. Erro = mensagem para o campo `cnpj`.
pub fn validar_cnpj(entrada: &str) -> Result<String, &'static str> {
    const FORMATO: &str = "CNPJ deve ter 14 caracteres (com ou sem máscara): 12 letras ou dígitos e 2 dígitos verificadores.";
    let mut cnpj = String::with_capacity(14);
    for c in entrada.chars() {
        match c {
            '.' | '/' | '-' => {}
            c if c.is_whitespace() => {}
            c if c.is_ascii_alphanumeric() => cnpj.push(c.to_ascii_uppercase()),
            _ => return Err(FORMATO),
        }
    }
    let bytes = cnpj.as_bytes();
    let Some(dvs) = bytes.get(12..) else {
        return Err(FORMATO);
    };
    if bytes.len() != 14 || !dvs.iter().all(u8::is_ascii_digit) {
        return Err(FORMATO);
    }
    if bytes.iter().all(|b| Some(b) == bytes.first()) {
        return Err("CNPJ inválido.");
    }
    // Valor do caractere: ASCII − 48 ('0'..'9' → 0..9, 'A' → 17).
    let valores: Vec<u32> = bytes.iter().map(|b| u32::from(*b) - 48).collect();
    let digito = |parte: &[u32]| -> u32 {
        // Pesos 2..=9 da direita para a esquerda, reiniciando em 2.
        let soma: u32 = parte
            .iter()
            .rev()
            .zip((2..=9).cycle())
            .map(|(v, p)| v * p)
            .sum();
        match soma % 11 {
            0 | 1 => 0,
            r => 11 - r,
        }
    };
    let (Some(base_v), Some(&dv1), Some(&dv2)) =
        (valores.get(..12), valores.get(12), valores.get(13))
    else {
        return Err(FORMATO);
    };
    let calc1 = digito(base_v);
    let mut com_dv1 = base_v.to_vec();
    com_dv1.push(calc1);
    let calc2 = digito(&com_dv1);
    if calc1 == dv1 && calc2 == dv2 {
        Ok(cnpj)
    } else {
        Err("CNPJ inválido: confira os dígitos verificadores.")
    }
}

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org}/empresas", get(listar).post(criar))
        .route(
            "/orgs/{org}/empresas/{id}",
            patch(atualizar).delete(excluir),
        )
}

/// Contrato `Empresa`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Empresa {
    pub id: Uuid,
    pub razao_social: String,
    pub cnpj: String,
    pub criado_em: DateTime<Utc>,
}

fn validar_razao_social(texto: &str) -> Result<String, String> {
    let razao = texto.trim();
    if razao.is_empty() {
        return Err("Informe a razão social.".into());
    }
    if razao.chars().count() > RAZAO_SOCIAL_MAX {
        return Err(format!("Use no máximo {RAZAO_SOCIAL_MAX} caracteres."));
    }
    if razao.chars().any(char::is_control) {
        return Err("Remova caracteres de controle.".into());
    }
    Ok(razao.to_owned())
}

async fn listar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
) -> Result<Json<impl Serialize>, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let empresas: Vec<Empresa> = sqlx::query_as(
        "SELECT id, razao_social, cnpj::text AS cnpj, criado_em FROM empresas
         WHERE organizacao_id = $1 ORDER BY razao_social, cnpj",
    )
    .bind(ctx.org_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(dados(empresas))
}

#[derive(Debug, Deserialize)]
struct NovaEmpresa {
    #[serde(default)]
    razao_social: String,
    #[serde(default)]
    cnpj: String,
}

async fn criar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<NovaEmpresa>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let mut erros = BTreeMap::new();
    let razao = validar_razao_social(&corpo.razao_social)
        .map_err(|m| erros.insert("razao_social".to_owned(), m))
        .ok();
    let cnpj = validar_cnpj(&corpo.cnpj)
        .map_err(|m| erros.insert("cnpj".to_owned(), m.to_owned()))
        .ok();
    if let Some(erro) = ErroApi::validacao_campos(erros) {
        return Err(erro);
    }
    let (Some(razao), Some(cnpj)) = (razao, cnpj) else {
        return Err(ErroApi::interno_log(
            "empresas::criar",
            "validação inconsistente",
        ));
    };
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let duplicada: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM empresas WHERE organizacao_id = $1 AND cnpj = $2)",
    )
    .bind(ctx.org_id)
    .bind(&cnpj)
    .fetch_one(&mut *tx)
    .await?;
    if duplicada {
        return Err(ErroApi::conflito("CNPJ já cadastrado nesta organização."));
    }
    verificar_limite(&mut tx, ctx.org_id, Recurso::Cnpj, estado.agora()).await?;
    let empresa: Empresa = sqlx::query_as(
        "INSERT INTO empresas (organizacao_id, razao_social, cnpj) VALUES ($1, $2, $3)
         RETURNING id, razao_social, cnpj::text AS cnpj, criado_em",
    )
    .bind(ctx.org_id)
    .bind(&razao)
    .bind(&cnpj)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.kind() == sqlx::error::ErrorKind::UniqueViolation => {
            ErroApi::conflito("CNPJ já cadastrado nesta organização.")
        }
        _ => ErroApi::from(e),
    })?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "criar",
        "empresa",
        Some(&empresa.id.to_string()),
        json!({"razao_social": razao, "cnpj": cnpj}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(criado(empresa))
}

#[derive(Debug, Deserialize)]
struct AtualizarEmpresa {
    #[serde(default)]
    razao_social: String,
}

async fn atualizar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
    Corpo(corpo): Corpo<AtualizarEmpresa>,
) -> Result<Json<impl Serialize>, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let razao = validar_razao_social(&corpo.razao_social)
        .map_err(|m| ErroApi::validacao_campo("razao_social", m))?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let anterior: String = sqlx::query_scalar(
        "SELECT razao_social FROM empresas WHERE id = $1 AND organizacao_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(ctx.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ErroApi::nao_encontrado_msg("Empresa não encontrada."))?;
    let empresa: Empresa = sqlx::query_as(
        "UPDATE empresas SET razao_social = $3 WHERE id = $1 AND organizacao_id = $2
         RETURNING id, razao_social, cnpj::text AS cnpj, criado_em",
    )
    .bind(id)
    .bind(ctx.org_id)
    .bind(&razao)
    .fetch_one(&mut *tx)
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "atualizar",
        "empresa",
        Some(&id.to_string()),
        json!({"razao_social": razao, "razao_social_anterior": anterior}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(dados(empresa))
}

async fn excluir(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
) -> Result<StatusCode, ErroApi> {
    ctx.exigir(Papel::Admin)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let empresa: Empresa = sqlx::query_as(
        "SELECT id, razao_social, cnpj::text AS cnpj, criado_em FROM empresas
         WHERE id = $1 AND organizacao_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(ctx.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ErroApi::nao_encontrado_msg("Empresa não encontrada."))?;
    let tem_empenhos: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM empenhos WHERE organizacao_id = $1 AND empresa_id = $2)",
    )
    .bind(ctx.org_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if tem_empenhos {
        return Err(ErroApi::conflito("Empresa possui empenhos."));
    }
    sqlx::query("DELETE FROM empresas WHERE id = $1 AND organizacao_id = $2")
        .bind(id)
        .bind(ctx.org_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.kind() == sqlx::error::ErrorKind::ForeignKeyViolation => {
                ErroApi::conflito("Empresa possui empenhos.")
            }
            _ => ErroApi::from(e),
        })?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "excluir",
        "empresa",
        Some(&id.to_string()),
        json!({"razao_social": empresa.razao_social, "cnpj": empresa.cnpj}),
        cliente.ip,
    )
    .await?;
    tx.commit().await?;
    Ok(sem_conteudo())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cnpj_valido_com_e_sem_mascara() {
        assert_eq!(
            validar_cnpj("11.222.333/0001-81").as_deref(),
            Ok("11222333000181")
        );
        assert_eq!(
            validar_cnpj(" 11222333000181 ").as_deref(),
            Ok("11222333000181")
        );
        assert_eq!(
            validar_cnpj("00.000.000/0001-91").as_deref(),
            Ok("00000000000191")
        );
    }

    #[test]
    fn cnpj_alfanumerico() {
        // Exemplo oficial da Receita Federal.
        assert_eq!(
            validar_cnpj("12.ABC.345/01DE-35").as_deref(),
            Ok("12ABC34501DE35")
        );
        assert_eq!(
            validar_cnpj("12abc34501de35").as_deref(),
            Ok("12ABC34501DE35")
        );
        assert!(validar_cnpj("12.ABC.345/01DE-36").is_err());
        assert!(validar_cnpj("12.ABC.345/01DE-3A").is_err());
        assert!(validar_cnpj("AAAAAAAAAAAAAA").is_err());
        assert!(validar_cnpj("12.ABÇ.345/01DE-35").is_err());
    }

    #[test]
    fn cnpj_invalido() {
        for ruim in [
            "",
            "11.222.333/0001-82",
            "11222333000180",
            "1122233300018",
            "112223330001811",
            "11222333000a81",
            "00000000000000",
            "11111111111111",
        ] {
            assert!(validar_cnpj(ruim).is_err(), "{ruim}");
        }
    }
}
