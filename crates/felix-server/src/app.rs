//! Estado compartilhado e montagem do router HTTP.

use crate::{
    config::Config,
    http::{csrf, erro::ErroApi},
    modulos,
    pdf::Pipeline,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, MatchedPath, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use serde_json::json;
use sqlx::PgPool;
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::Semaphore;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    services::{ServeDir, ServeFile},
    timeout::TimeoutLayer,
    trace::{DefaultOnResponse, TraceLayer},
};

/// Tamanho máximo de um `X-Request-Id` recebido.
pub const REQUEST_ID_MAX: usize = 64;

/// Limite de corpo JSON em `/api`.
pub const LIMITE_JSON: usize = 64 * 1024;
/// Requisições de análise de PDF admitidas ao mesmo tempo (excesso → 429).
pub const ANALISES_ADMITIDAS: usize = 8;
pub const TIMEOUT_REQUISICAO: Duration = Duration::from_secs(60);

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
font-src 'self'; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; \
base-uri 'self'; form-action 'self'";

/// Fonte do instante atual; trocável nos testes.
pub trait Relogio: Send + Sync + 'static {
    fn agora(&self) -> DateTime<Utc>;
}

/// Relógio do sistema.
pub struct RelogioSistema;

impl Relogio for RelogioSistema {
    fn agora(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Relógio fixo e ajustável (testes).
#[derive(Clone)]
pub struct RelogioFixo(Arc<RwLock<DateTime<Utc>>>);

impl RelogioFixo {
    pub fn novo(instante: DateTime<Utc>) -> Self {
        Self(Arc::new(RwLock::new(instante)))
    }

    /// Meio-dia em Brasília (15:00 UTC) da data informada.
    pub fn em_data(data: NaiveDate) -> Self {
        Self::novo(meio_dia_brt(data))
    }

    pub fn definir(&self, instante: DateTime<Utc>) {
        if let Ok(mut atual) = self.0.write() {
            *atual = instante;
        }
    }

    pub fn definir_data(&self, data: NaiveDate) {
        self.definir(meio_dia_brt(data));
    }
}

impl Relogio for RelogioFixo {
    fn agora(&self) -> DateTime<Utc> {
        self.0.read().map_or_else(|_| Utc::now(), |atual| *atual)
    }
}

fn meio_dia_brt(data: NaiveDate) -> DateTime<Utc> {
    data.and_hms_opt(15, 0, 0).unwrap_or_default().and_utc()
}

/// Data em Brasília (UTC−3, sem horário de verão desde 2019).
pub fn data_brt(instante: DateTime<Utc>) -> NaiveDate {
    (instante - TimeDelta::hours(3)).date_naive()
}

/// Estado de todas as rotas.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub pipeline: Arc<Pipeline>,
    /// Admissão de análises de PDF (8). Use `try_acquire_owned` → 429.
    pub admissao: Arc<Semaphore>,
    relogio: Arc<dyn Relogio>,
}

impl AppState {
    pub fn novo(pool: PgPool, config: Config) -> Result<Self, String> {
        Ok(Self {
            pool,
            config: Arc::new(config),
            pipeline: Arc::new(Pipeline::novo()?),
            admissao: Arc::new(Semaphore::new(ANALISES_ADMITIDAS)),
            relogio: Arc::new(RelogioSistema),
        })
    }

    /// Troca o relógio (testes).
    pub fn com_relogio(mut self, relogio: Arc<dyn Relogio>) -> Self {
        self.relogio = relogio;
        self
    }

    /// Instante atual segundo o relógio do estado.
    pub fn agora(&self) -> DateTime<Utc> {
        self.relogio.agora()
    }

    /// "Hoje" em Brasília: use para vencimentos, atrasos e cálculos de cobrança.
    pub fn hoje(&self) -> NaiveDate {
        data_brt(self.agora())
    }
}

/// Router de produção: `/health`, `/ready`, `/api/v1/*` e a SPA.
pub fn router(estado: AppState) -> Router {
    montar(estado, modulos::rotas())
}

/// Monta o router com as rotas da API informadas (testes podem acrescentar rotas).
pub fn montar(estado: AppState, rotas_v1: Router<AppState>) -> Router {
    let api = Router::new()
        .nest("/v1", rotas_v1)
        .fallback(api_nao_encontrada)
        .layer(middleware::from_fn_with_state(
            estado.clone(),
            csrf::verificar,
        ))
        .layer(DefaultBodyLimit::max(LIMITE_JSON));
    let dir = estado.config.frontend_dir.clone();
    let spa = ServeDir::new(&dir).fallback(ServeFile::new(dir.join("index.html")));
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .nest("/api", api)
        .fallback_service(spa)
        .layer(middleware::from_fn_with_state(
            estado.clone(),
            cabecalhos_seguranca,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &Request| {
                    let rota = req
                        .extensions()
                        .get::<MatchedPath>()
                        .map_or("-", MatchedPath::as_str)
                        .to_owned();
                    let request_id = req
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-")
                        .to_owned();
                    tracing::info_span!("http", metodo = %req.method(), rota, request_id)
                })
                .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(middleware::from_fn(sanear_request_id))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            TIMEOUT_REQUISICAO,
        ))
        .with_state(estado)
}

/// `X-Request-Id` aceito do cliente/proxy: 1–64 caracteres `[A-Za-z0-9-]`.
pub fn request_id_valido(valor: &[u8]) -> bool {
    !valor.is_empty()
        && valor.len() <= REQUEST_ID_MAX
        && valor
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

/// Descarta `X-Request-Id` fora do formato (evita injeção em logs); a camada
/// seguinte gera um UUID novo.
async fn sanear_request_id(mut request: Request, next: Next) -> Response {
    let invalido = request
        .headers()
        .get_all("x-request-id")
        .iter()
        .enumerate()
        .any(|(i, v)| i > 0 || !request_id_valido(v.as_bytes()));
    if invalido {
        request.headers_mut().remove("x-request-id");
    }
    next.run(request).await
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

async fn ready(State(estado): State<AppState>) -> Response {
    let consulta = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&estado.pool);
    match tokio::time::timeout(Duration::from_secs(2), consulta).await {
        Ok(Ok(_)) => Json(json!({
            "status": "pronto",
            "executores_jobs": crate::jobs::executores_vivos(),
        }))
        .into_response(),
        Ok(Err(erro)) => {
            tracing::warn!(erro = %erro, "Banco indisponível no /ready");
            ErroApi::indisponivel("Banco de dados indisponível.").into_response()
        }
        Err(_) => ErroApi::indisponivel("Banco de dados não respondeu a tempo.").into_response(),
    }
}

async fn api_nao_encontrada() -> ErroApi {
    ErroApi::nao_encontrado_msg("Rota da API não encontrada.")
}

async fn cabecalhos_seguranca(
    State(estado): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let caminho = request.uri().path().to_owned();
    let mut resposta = next.run(request).await;
    let status = resposta.status();
    let h = resposta.headers_mut();
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    h.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    h.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    if estado.config.producao() {
        h.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }
    let cache = if caminho.starts_with("/api/") || caminho == "/api" || caminho == "/ready" {
        "no-store"
    } else if caminho.starts_with("/assets/") && status.is_success() {
        // Arquivos do Vite têm hash no nome.
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    resposta
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn formato_do_request_id() {
        assert!(request_id_valido(b"abc-123-DEF"));
        assert!(request_id_valido(&[b'a'; REQUEST_ID_MAX]));
        assert!(!request_id_valido(&[b'a'; REQUEST_ID_MAX + 1]));
        assert!(!request_id_valido(b""));
        assert!(!request_id_valido(b"a b"));
        assert!(!request_id_valido(b"x\"},{\"y"));
    }

    #[test]
    fn hoje_em_brasilia() {
        let instante = "2026-10-21T02:30:00Z".parse::<DateTime<Utc>>().unwrap();
        assert_eq!(data_brt(instante).to_string(), "2026-10-20");
        let relogio = RelogioFixo::em_data(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap());
        assert_eq!(data_brt(relogio.agora()).to_string(), "2026-01-31");
        relogio.definir_data(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
        assert_eq!(data_brt(relogio.agora()).to_string(), "2026-02-01");
    }
}
