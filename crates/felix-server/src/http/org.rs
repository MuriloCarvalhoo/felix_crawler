//! Contexto de organização (ADR-007/008): `/orgs/{org}/...` exige que o usuário
//! da sessão seja membro de uma organização não excluída. Não membro → 404.

use super::{erro::ErroApi, sessao::Sessao};
use crate::app::AppState;
use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
};
use serde::{Deserialize, Serialize};
use sqlx::{
    Decode, Encode, Postgres, Type,
    encode::IsNull,
    error::BoxDynError,
    postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef},
};
use std::collections::HashMap;
use uuid::Uuid;

/// Papel na organização, em ordem crescente de poder: `Leitor < Operador < Admin < Dono`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Papel {
    Leitor,
    Operador,
    Admin,
    Dono,
}

impl Papel {
    pub const TODOS: [Papel; 4] = [Papel::Leitor, Papel::Operador, Papel::Admin, Papel::Dono];

    pub fn como_texto(self) -> &'static str {
        match self {
            Papel::Leitor => "leitor",
            Papel::Operador => "operador",
            Papel::Admin => "admin",
            Papel::Dono => "dono",
        }
    }

    pub fn de_texto(texto: &str) -> Option<Self> {
        Self::TODOS.into_iter().find(|p| p.como_texto() == texto)
    }
}

impl std::fmt::Display for Papel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.como_texto())
    }
}

impl Type<Postgres> for Papel {
    fn type_info() -> PgTypeInfo {
        <&str as Type<Postgres>>::type_info()
    }
    fn compatible(ty: &PgTypeInfo) -> bool {
        <&str as Type<Postgres>>::compatible(ty)
    }
}

impl<'r> Decode<'r, Postgres> for Papel {
    fn decode(valor: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        let texto = <&str as Decode<Postgres>>::decode(valor)?;
        Papel::de_texto(texto).ok_or_else(|| format!("papel desconhecido: {texto}").into())
    }
}

impl<'q> Encode<'q, Postgres> for Papel {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        <&str as Encode<'q, Postgres>>::encode_by_ref(&self.como_texto(), buf)
    }
}

/// Contexto da organização do caminho. Rejeições: 401 sem sessão, 404 se a
/// organização não existe/foi excluída ou o usuário não é membro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrgCtx {
    pub org_id: Uuid,
    pub usuario_id: Uuid,
    pub sessao_id: Uuid,
    pub papel: Papel,
}

impl OrgCtx {
    /// Exige papel mínimo; abaixo dele → 403 `proibido`.
    pub fn exigir(&self, minimo: Papel) -> Result<(), ErroApi> {
        if self.papel >= minimo {
            Ok(())
        } else {
            Err(ErroApi::papel_insuficiente())
        }
    }
}

impl FromRequestParts<AppState> for OrgCtx {
    type Rejection = ErroApi;

    async fn from_request_parts(
        parts: &mut Parts,
        estado: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(ctx) = parts.extensions.get::<OrgCtx>() {
            return Ok(*ctx);
        }
        let sessao = Sessao::from_request_parts(parts, estado).await?;
        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, estado)
            .await
            .map_err(|_| ErroApi::nao_encontrado())?;
        let Some(org) = params.get("org") else {
            return Err(ErroApi::interno_log("OrgCtx", "rota sem parâmetro {org}"));
        };
        let org_id = Uuid::parse_str(org).map_err(|_| ErroApi::nao_encontrado())?;
        let papel: Option<Papel> = sqlx::query_scalar(
            "SELECT m.papel FROM membros m
             JOIN organizacoes o ON o.id = m.organizacao_id
             WHERE m.organizacao_id = $1 AND m.usuario_id = $2 AND o.excluida_em IS NULL",
        )
        .bind(org_id)
        .bind(sessao.usuario_id)
        .fetch_optional(&estado.pool)
        .await?;
        let papel =
            papel.ok_or_else(|| ErroApi::nao_encontrado_msg("Organização não encontrada."))?;
        let ctx = OrgCtx {
            org_id,
            usuario_id: sessao.usuario_id,
            sessao_id: sessao.sessao_id,
            papel,
        };
        parts.extensions.insert(ctx);
        Ok(ctx)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ordem_e_texto() {
        assert!(Papel::Leitor < Papel::Operador);
        assert!(Papel::Operador < Papel::Admin);
        assert!(Papel::Admin < Papel::Dono);
        for p in Papel::TODOS {
            assert_eq!(Papel::de_texto(p.como_texto()), Some(p));
            assert_eq!(
                serde_json::to_value(p).unwrap(),
                serde_json::json!(p.como_texto())
            );
        }
        assert_eq!(Papel::de_texto("root"), None);
    }

    #[test]
    fn exigir() {
        let ctx = OrgCtx {
            org_id: Uuid::nil(),
            usuario_id: Uuid::nil(),
            sessao_id: Uuid::nil(),
            papel: Papel::Operador,
        };
        assert!(ctx.exigir(Papel::Leitor).is_ok());
        assert!(ctx.exigir(Papel::Operador).is_ok());
        assert_eq!(ctx.exigir(Papel::Admin).unwrap_err().codigo(), "proibido");
    }
}
