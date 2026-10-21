//! `GET/POST /orgs/{org}/assinatura`.
//!
//! Sem provedor de pagamento (`FELIX_PLANO_MANUAL=false`, padrão em produção),
//! o dono só pode mudar para o Gratuito. Qualquer mudança exige que o uso atual
//! caiba nos limites do novo plano (409 com o que excede).

use super::{Limites, Plano, Recurso, contar_uso, limites_do_plano, plano_vigente};
use crate::{
    app::AppState,
    db,
    http::{ClienteInfo, Corpo, ErroApi, OrgCtx, Papel, dados},
    modulos::orgs::travar_org,
};
use axum::{Json, Router, extract::State, response::IntoResponse, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

/// Contrato `Assinatura` (docs/saas/03-api-v1.md). `plano` é o plano efetivo.
#[derive(Debug, Clone, Serialize)]
pub struct Assinatura {
    pub plano: Plano,
    pub status: String,
    pub teste_ate: Option<DateTime<Utc>>,
    pub provedor: String,
    pub limites: Limites,
    pub uso: Uso,
    /// `FELIX_PLANO_MANUAL`: `false` = só é possível mudar para o Gratuito.
    pub troca_paga_disponivel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Uso {
    pub cnpjs: i64,
    pub empenhos_ativos: i64,
    pub usuarios: i64,
}

/// Monta a `Assinatura` com limites e uso. `conn` deve estar numa transação de
/// tenant da organização (contagem de empresas/empenhos passa pelo RLS).
/// `troca_paga_disponivel` = `config.plano_manual`.
pub async fn carregar_assinatura(
    conn: &mut PgConnection,
    org_id: Uuid,
    agora: DateTime<Utc>,
    troca_paga_disponivel: bool,
) -> Result<Assinatura, ErroApi> {
    let linha: Option<(String, String, Option<DateTime<Utc>>, String)> = sqlx::query_as(
        "SELECT plano, status, teste_ate, provedor FROM assinaturas WHERE organizacao_id = $1",
    )
    .bind(org_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (plano_texto, status, teste_ate, provedor) = linha.unwrap_or_else(|| {
        (
            "gratuito".to_owned(),
            "ativa".to_owned(),
            None,
            "manual".to_owned(),
        )
    });
    let plano = plano_vigente(&plano_texto, &status, teste_ate, agora);
    let uso = Uso {
        cnpjs: contar_uso(conn, org_id, Recurso::Cnpj, agora).await?,
        empenhos_ativos: contar_uso(conn, org_id, Recurso::EmpenhoAtivo, agora).await?,
        usuarios: contar_uso(conn, org_id, Recurso::Usuario, agora).await?,
    };
    Ok(Assinatura {
        plano,
        status,
        teste_ate,
        provedor,
        limites: limites_do_plano(plano),
        uso,
        troca_paga_disponivel,
    })
}

/// Mensagem do 403 de upgrade sem provedor de pagamento.
pub const MSG_PLANO_PAGO: &str =
    "Mudança de plano pago depende do provedor de pagamento (em configuração).";

/// Itens de uso acima dos limites: `(campo, descrição)`.
pub fn excedentes(uso: &Uso, limites: &Limites) -> Vec<(&'static str, String)> {
    [
        ("cnpjs", "CNPJs", uso.cnpjs, limites.cnpjs),
        (
            "empenhos_ativos",
            "empenhos ativos",
            uso.empenhos_ativos,
            limites.empenhos_ativos,
        ),
        (
            "usuarios",
            "usuários (membros e convites pendentes)",
            uso.usuarios,
            limites.usuarios,
        ),
    ]
    .into_iter()
    .filter_map(|(campo, rotulo, usado, limite)| {
        limite
            .filter(|limite| usado > *limite)
            .map(|limite| (campo, format!("{usado} {rotulo} (limite {limite})")))
    })
    .collect()
}

pub fn rotas() -> Router<AppState> {
    Router::new().route("/orgs/{org}/assinatura", get(obter).post(trocar))
}

async fn obter(
    State(estado): State<AppState>,
    ctx: OrgCtx,
) -> Result<Json<impl Serialize>, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let assinatura = carregar_assinatura(
        &mut tx,
        ctx.org_id,
        estado.agora(),
        estado.config.plano_manual,
    )
    .await?;
    tx.commit().await?;
    Ok(dados(assinatura))
}

#[derive(Debug, Deserialize)]
struct TrocaPlano {
    plano: String,
}

async fn trocar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<TrocaPlano>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Dono)?;
    let plano = Plano::de_texto(corpo.plano.trim()).ok_or_else(|| {
        ErroApi::validacao_campo(
            "plano",
            "Plano inválido: use gratuito, profissional ou empresarial.",
        )
    })?;
    if !estado.config.plano_manual && plano != Plano::Gratuito {
        return Err(ErroApi::proibido(MSG_PLANO_PAGO));
    }
    let agora = estado.agora();
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    // Serializa com as criações limitadas pelo plano (verificar_limite trava a org).
    travar_org(&mut tx, ctx.org_id).await?;
    let anterior: Option<(String, String)> = sqlx::query_as(
        "SELECT plano, status FROM assinaturas WHERE organizacao_id = $1 FOR UPDATE",
    )
    .bind(ctx.org_id)
    .fetch_optional(&mut *tx)
    .await?;
    let uso = Uso {
        cnpjs: contar_uso(&mut tx, ctx.org_id, Recurso::Cnpj, agora).await?,
        empenhos_ativos: contar_uso(&mut tx, ctx.org_id, Recurso::EmpenhoAtivo, agora).await?,
        usuarios: contar_uso(&mut tx, ctx.org_id, Recurso::Usuario, agora).await?,
    };
    let excesso = excedentes(&uso, &limites_do_plano(plano));
    if !excesso.is_empty() {
        let lista: Vec<&str> = excesso.iter().map(|(_, texto)| texto.as_str()).collect();
        let mut erro = ErroApi::conflito(format!(
            "O uso atual não cabe no plano {}: {}. Reduza o uso antes de mudar de plano.",
            plano.rotulo(),
            lista.join("; ")
        ));
        for (campo, texto) in excesso {
            erro = erro.com_campo(campo, texto);
        }
        return Err(erro);
    }
    sqlx::query(
        "INSERT INTO assinaturas (organizacao_id, plano, status, teste_ate, provedor, atualizado_em)
         VALUES ($1, $2, 'ativa', NULL, 'manual', $3)
         ON CONFLICT (organizacao_id) DO UPDATE
         SET plano = EXCLUDED.plano, status = 'ativa', teste_ate = NULL,
             provedor = 'manual', atualizado_em = EXCLUDED.atualizado_em",
    )
    .bind(ctx.org_id)
    .bind(plano.como_texto())
    .bind(agora)
    .execute(&mut *tx)
    .await?;
    let (plano_anterior, status_anterior) = anterior.unzip();
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "trocar_plano",
        "assinatura",
        Some(&ctx.org_id.to_string()),
        json!({
            "plano": plano.como_texto(),
            "plano_anterior": plano_anterior,
            "status_anterior": status_anterior,
            "provedor": "manual",
        }),
        cliente.ip,
    )
    .await?;
    let assinatura =
        carregar_assinatura(&mut tx, ctx.org_id, agora, estado.config.plano_manual).await?;
    tx.commit().await?;
    Ok(dados(assinatura))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn excedentes_do_downgrade() {
        let uso = Uso {
            cnpjs: 3,
            empenhos_ativos: 30,
            usuarios: 4,
        };
        let gratuito = excedentes(&uso, &limites_do_plano(Plano::Gratuito));
        let campos: Vec<&str> = gratuito.iter().map(|(c, _)| *c).collect();
        assert_eq!(campos, ["cnpjs", "usuarios"]);
        assert_eq!(
            gratuito.first().map(|(_, t)| t.as_str()),
            Some("3 CNPJs (limite 1)")
        );
        assert!(excedentes(&uso, &limites_do_plano(Plano::Empresarial)).is_empty());
    }
}
