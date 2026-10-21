//! Planos, limites e assinatura da organização (ADR-013).
//!
//! Funções públicas usadas por outros módulos:
//! - [`plano_efetivo`]: plano vigente (teste expirado ou cancelado → gratuito);
//! - [`verificar_limite`]: 402 `limite_plano` se criar mais um recurso exceder o plano;
//! - [`limites_do_plano`]: tabela de limites (docs/saas/01-produto.md).

mod rotas;

pub use rotas::{Assinatura, Uso, carregar_assinatura};

use crate::{app::AppState, http::erro::ErroApi};
use axum::Router;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

/// Dias de teste do plano Profissional dados no cadastro.
pub const DIAS_TESTE: i64 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plano {
    Gratuito,
    Profissional,
    Empresarial,
}

impl Plano {
    pub const TODOS: [Plano; 3] = [Plano::Gratuito, Plano::Profissional, Plano::Empresarial];

    pub fn como_texto(self) -> &'static str {
        match self {
            Plano::Gratuito => "gratuito",
            Plano::Profissional => "profissional",
            Plano::Empresarial => "empresarial",
        }
    }

    pub fn de_texto(texto: &str) -> Option<Self> {
        Self::TODOS.into_iter().find(|p| p.como_texto() == texto)
    }

    /// Nome para mensagens ao usuário.
    pub fn rotulo(self) -> &'static str {
        match self {
            Plano::Gratuito => "Gratuito",
            Plano::Profissional => "Profissional",
            Plano::Empresarial => "Empresarial",
        }
    }
}

/// Limites de um plano; `None` = ilimitado (serializa como `null`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Limites {
    pub cnpjs: Option<i64>,
    pub empenhos_ativos: Option<i64>,
    pub usuarios: Option<i64>,
    pub transparencia: bool,
    pub alertas_email: bool,
}

pub fn limites_do_plano(p: Plano) -> Limites {
    match p {
        Plano::Gratuito => Limites {
            cnpjs: Some(1),
            empenhos_ativos: Some(30),
            usuarios: Some(2),
            transparencia: false,
            alertas_email: false,
        },
        Plano::Profissional => Limites {
            cnpjs: Some(5),
            empenhos_ativos: None,
            usuarios: Some(10),
            transparencia: true,
            alertas_email: true,
        },
        Plano::Empresarial => Limites {
            cnpjs: None,
            empenhos_ativos: None,
            usuarios: None,
            transparencia: true,
            alertas_email: true,
        },
    }
}

/// Recurso contável sujeito a limite de plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recurso {
    Cnpj,
    EmpenhoAtivo,
    Usuario,
}

/// Plano vigente segundo a linha de `assinaturas` (sem RLS):
/// status `teste` com `teste_ate` no passado → Gratuito; `cancelada` → Gratuito;
/// sem linha → Gratuito.
pub fn plano_vigente(
    plano: &str,
    status: &str,
    teste_ate: Option<DateTime<Utc>>,
    agora: DateTime<Utc>,
) -> Plano {
    let plano = Plano::de_texto(plano).unwrap_or(Plano::Gratuito);
    match status {
        "teste" if teste_ate.is_none_or(|ate| ate < agora) => Plano::Gratuito,
        "cancelada" => Plano::Gratuito,
        _ => plano,
    }
}

/// Plano efetivo da organização. Lê `assinaturas` (sem RLS), então funciona
/// dentro ou fora de uma transação de tenant.
pub async fn plano_efetivo(
    conn: &mut PgConnection,
    org_id: Uuid,
    agora: DateTime<Utc>,
) -> Result<Plano, ErroApi> {
    let linha: Option<(String, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT plano, status, teste_ate FROM assinaturas WHERE organizacao_id = $1",
    )
    .bind(org_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(linha.map_or(Plano::Gratuito, |(plano, status, teste_ate)| {
        plano_vigente(&plano, &status, teste_ate, agora)
    }))
}

/// Uso atual de um recurso. Para `Cnpj` e `EmpenhoAtivo` a conexão precisa estar
/// numa transação de tenant da própria organização (`db::tenant`), pois as
/// tabelas têm RLS forçado. `Usuario` = membros + convites pendentes válidos.
pub async fn contar_uso(
    conn: &mut PgConnection,
    org_id: Uuid,
    recurso: Recurso,
    agora: DateTime<Utc>,
) -> Result<i64, ErroApi> {
    let consulta = match recurso {
        Recurso::Cnpj => {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM empresas WHERE organizacao_id = $1")
                .bind(org_id)
        }
        Recurso::EmpenhoAtivo => sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM empenhos WHERE organizacao_id = $1 AND status = 'ativo'",
        )
        .bind(org_id),
        Recurso::Usuario => sqlx::query_scalar::<_, i64>(
            "SELECT (SELECT count(*) FROM membros WHERE organizacao_id = $1)
                  + (SELECT count(*) FROM convites
                     WHERE organizacao_id = $1 AND aceito_em IS NULL
                       AND revogado_em IS NULL AND expira_em > $2)",
        )
        .bind(org_id)
        .bind(agora),
    };
    Ok(consulta.fetch_one(&mut *conn).await?)
}

/// Retorna 402 `limite_plano` se adicionar mais um `recurso` exceder o plano
/// efetivo. Para `Cnpj`/`EmpenhoAtivo`, chame dentro de `db::tenant`.
/// Trava a linha da organização (`FOR NO KEY UPDATE`) até o fim da transação,
/// serializando criações concorrentes da mesma organização: chame na mesma
/// transação que fará o INSERT.
pub async fn verificar_limite(
    conn: &mut PgConnection,
    org_id: Uuid,
    recurso: Recurso,
    agora: DateTime<Utc>,
) -> Result<(), ErroApi> {
    crate::modulos::orgs::travar_org(conn, org_id).await?;
    let plano = plano_efetivo(conn, org_id, agora).await?;
    let limites = limites_do_plano(plano);
    let limite = match recurso {
        Recurso::Cnpj => limites.cnpjs,
        Recurso::EmpenhoAtivo => limites.empenhos_ativos,
        Recurso::Usuario => limites.usuarios,
    };
    let Some(limite) = limite else {
        return Ok(());
    };
    let uso = contar_uso(conn, org_id, recurso, agora).await?;
    if uso < limite {
        return Ok(());
    }
    Err(ErroApi::limite_plano(mensagem_limite(
        plano, recurso, limite,
    )))
}

fn mensagem_limite(plano: Plano, recurso: Recurso, limite: i64) -> String {
    let (singular, plural, acao) = match recurso {
        Recurso::Cnpj => ("CNPJ", "CNPJs", "cadastrar mais CNPJs"),
        Recurso::EmpenhoAtivo => (
            "empenho ativo",
            "empenhos ativos",
            "cadastrar mais empenhos",
        ),
        Recurso::Usuario => (
            "usuário",
            "usuários (membros e convites pendentes)",
            "convidar mais pessoas",
        ),
    };
    let item = if limite == 1 { singular } else { plural };
    format!(
        "O plano {} permite até {limite} {item}. Mude de plano para {acao}.",
        plano.rotulo()
    )
}

pub fn rotas() -> Router<AppState> {
    rotas::rotas()
}

#[cfg(test)]
mod testes {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn plano_vigente_regras() {
        let agora = Utc::now();
        let futuro = Some(agora + TimeDelta::days(1));
        let passado = Some(agora - TimeDelta::days(1));
        assert_eq!(
            plano_vigente("profissional", "teste", futuro, agora),
            Plano::Profissional
        );
        assert_eq!(
            plano_vigente("profissional", "teste", passado, agora),
            Plano::Gratuito
        );
        assert_eq!(
            plano_vigente("profissional", "teste", None, agora),
            Plano::Gratuito
        );
        assert_eq!(
            plano_vigente("empresarial", "ativa", None, agora),
            Plano::Empresarial
        );
        assert_eq!(
            plano_vigente("empresarial", "cancelada", None, agora),
            Plano::Gratuito
        );
        assert_eq!(plano_vigente("xpto", "ativa", None, agora), Plano::Gratuito);
    }

    #[test]
    fn limites_e_mensagens() {
        assert_eq!(limites_do_plano(Plano::Gratuito).cnpjs, Some(1));
        assert_eq!(limites_do_plano(Plano::Profissional).empenhos_ativos, None);
        assert_eq!(limites_do_plano(Plano::Empresarial).usuarios, None);
        let m = mensagem_limite(Plano::Gratuito, Recurso::Cnpj, 1);
        assert!(m.contains("Gratuito") && m.contains("1 CNPJ"), "{m}");
        for p in Plano::TODOS {
            assert_eq!(Plano::de_texto(p.como_texto()), Some(p));
            assert_eq!(
                serde_json::to_value(p).unwrap_or_default(),
                serde_json::json!(p.como_texto())
            );
        }
    }
}
