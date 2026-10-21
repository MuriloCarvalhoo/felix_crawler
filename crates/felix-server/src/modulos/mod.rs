//! Módulos de negócio. Cada submódulo expõe `rotas()` com caminhos completos
//! relativos a `/api/v1` (ex.: `/orgs/{org}/empresas`). Dois módulos não podem
//! registrar o mesmo caminho (o axum entra em pânico ao mesclar).

use crate::app::AppState;
use axum::Router;

pub mod assinatura;
pub mod auditoria;
pub mod auth;
pub mod cobrancas;
pub mod dev;
pub mod empenhos;
pub mod empresas;
pub mod exportar;
pub mod integracoes;
pub mod notas;
pub mod notificacoes;
pub mod orgs;
pub mod painel;

pub fn rotas() -> Router<AppState> {
    Router::new()
        .merge(auth::rotas())
        .merge(orgs::rotas())
        .merge(empresas::rotas())
        .merge(assinatura::rotas())
        .merge(auditoria::rotas())
        .merge(dev::rotas())
        .merge(empenhos::rotas())
        .merge(notas::rotas())
        .merge(cobrancas::rotas())
        .merge(painel::rotas())
        .merge(exportar::rotas())
        .merge(integracoes::rotas())
        .merge(notificacoes::rotas())
}
