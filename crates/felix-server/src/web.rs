//! HTTP leve; extração e SQLite não bloqueiam os workers de rede.

use crate::{
    config::Config,
    db::{Empresa, ErroDb, NovaNota, NovoEmpenho, Repositorio},
    pdf::{MAX_PDF_BYTES, Pipeline},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use chrono::{NaiveDate, Utc};
use felix_core::{cobranca::calcular, dinheiro::reais, documento::Analise};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use tokio::{io::AsyncWriteExt, sync::Semaphore};
use tower_http::timeout::TimeoutLayer;

pub const MAX_BATCH_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_FILES: usize = 10;

#[derive(Clone)]
pub struct Estado {
    pub pipeline: Arc<Pipeline>,
    pub admissao: Arc<Semaphore>,
    banco_vagas: Arc<Semaphore>,
    repo: Repositorio,
    empresa: Empresa,
    token: Option<Arc<str>>,
}

impl Estado {
    pub fn novo(config: Config) -> Result<Self, String> {
        config.validar()?;
        let repo = Repositorio::abrir(&config.database_path).map_err(|e| e.to_string())?;
        let empresa = repo
            .empresa(&config.empresa_nome, &config.empresa_cnpj)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            pipeline: Arc::new(Pipeline::novo()?),
            admissao: Arc::new(Semaphore::new(8)),
            banco_vagas: Arc::new(Semaphore::new(2)),
            repo,
            empresa,
            token: config.token.map(Arc::from),
        })
    }
}

pub struct ErroApi(StatusCode, String);

impl IntoResponse for ErroApi {
    fn into_response(self) -> Response {
        let mut resposta = (self.0, Json(json!({"erro":self.1}))).into_response();
        if self.0 == StatusCode::TOO_MANY_REQUESTS {
            resposta
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
        }
        resposta
    }
}

impl From<ErroDb> for ErroApi {
    fn from(e: ErroDb) -> Self {
        let status = match &e {
            ErroDb::Entrada(_) => StatusCode::BAD_REQUEST,
            ErroDb::Acesso => StatusCode::FORBIDDEN,
            ErroDb::Conflito(_) => StatusCode::CONFLICT,
            ErroDb::Banco(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if matches!(e, ErroDb::Banco(_)) {
            tracing::error!("Falha de operação SQLite");
        }
        Self(status, e.to_string())
    }
}

fn ocupado() -> ErroApi {
    ErroApi(
        StatusCode::TOO_MANY_REQUESTS,
        "Servidor ocupado. Tente novamente em 2 segundos.".into(),
    )
}

async fn banco<T: Send + 'static>(
    estado: &Estado,
    operation: impl FnOnce(Repositorio) -> Result<T, ErroDb> + Send + 'static,
) -> Result<T, ErroApi> {
    let permit = Arc::clone(&estado.banco_vagas)
        .try_acquire_owned()
        .map_err(|_| ocupado())?;
    let repo = estado.repo.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation(repo)
    })
    .await
    .map_err(|_| {
        ErroApi(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Operação interrompida.".into(),
        )
    })?
    .map_err(Into::into)
}

async fn autorizar(
    State(estado): State<Estado>,
    request: Request,
    next: Next,
) -> Result<Response, ErroApi> {
    if let Some(expected) = &estado.token {
        let provided = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or("");
        if !bool::from(provided.as_bytes().ct_eq(expected.as_bytes())) {
            return Err(ErroApi(
                StatusCode::UNAUTHORIZED,
                "Token de acesso inválido ou ausente.".into(),
            ));
        }
    }
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if origin != format!("http://{host}") && origin != format!("https://{host}") {
            return Err(ErroApi(
                StatusCode::FORBIDDEN,
                "Origem da requisição não autorizada.".into(),
            ));
        }
    }
    if estado.token.is_none()
        && !host.is_empty()
        && !host.starts_with("127.0.0.1:")
        && !host.starts_with("localhost:")
        && !host.starts_with("[::1]:")
    {
        return Err(ErroApi(
            StatusCode::FORBIDDEN,
            "Modo demonstração disponível somente em loopback.".into(),
        ));
    }
    Ok(next.run(request).await)
}

async fn cabecalhos(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"));
    response
}

pub fn router(estado: Estado) -> Router {
    let api = Router::new()
        .route("/empresa", get(empresa))
        .route("/empenhos", get(listar_empenhos).post(salvar_empenho))
        .route("/empenhos/{id}/notas", post(salvar_nota))
        .route("/notas", get(listar_notas))
        .route("/notas/{id}/oficio", get(oficio))
        .route(
            "/analisar",
            post(analisar).layer(DefaultBodyLimit::max(MAX_BATCH_BYTES + 65536)),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(estado.clone(), autorizar))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(60),
        ));
    Router::new()
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .route(
            "/",
            get(|| async { Html(include_str!("../../../web/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/app.js"),
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../../../web/app.css"),
                )
            }),
        )
        .route("/health", get(health))
        .nest("/api", api)
        .with_state(estado)
        .layer(middleware::from_fn(cabecalhos))
}

async fn health(State(s): State<Estado>) -> Json<serde_json::Value> {
    Json(
        json!({"status":"ok","pdfs_ativos":s.pipeline.ativos(),"pico_pdfs_ativos":s.pipeline.pico()}),
    )
}

async fn empresa(State(s): State<Estado>) -> Json<serde_json::Value> {
    Json(
        json!({"empresa":s.empresa,"modo":if s.token.is_some(){"autenticado"}else{"demonstracao"},"limites":{"arquivo_bytes":MAX_PDF_BYTES,"lote_bytes":MAX_BATCH_BYTES,"arquivos":MAX_FILES,"processadores_pdf":2,"requisicoes_admitidas":8}}),
    )
}

#[derive(Default, Deserialize)]
struct Paginacao {
    #[serde(default)]
    offset: u32,
}

async fn listar_empenhos(
    State(s): State<Estado>,
    Query(page): Query<Paginacao>,
) -> Result<impl IntoResponse, ErroApi> {
    let id = s.empresa.id;
    Ok(Json(
        banco(&s, move |r| r.listar_empenhos(id, page.offset)).await?,
    ))
}

async fn salvar_empenho(
    State(s): State<Estado>,
    Json(data): Json<NovoEmpenho>,
) -> Result<impl IntoResponse, ErroApi> {
    let empresa = s.empresa.clone();
    let id = banco(&s, move |r| r.salvar_empenho(&empresa, &data)).await?;
    Ok((StatusCode::CREATED, Json(json!({"id":id}))))
}

async fn salvar_nota(
    State(s): State<Estado>,
    Path(id): Path<i64>,
    Json(data): Json<NovaNota>,
) -> Result<impl IntoResponse, ErroApi> {
    let empresa = s.empresa.id;
    let id = banco(&s, move |r| r.salvar_nota(empresa, id, &data)).await?;
    Ok((StatusCode::CREATED, Json(json!({"id":id}))))
}

async fn listar_notas(
    State(s): State<Estado>,
    Query(page): Query<Paginacao>,
) -> Result<impl IntoResponse, ErroApi> {
    let id = s.empresa.id;
    let notas = banco(&s, move |r| r.listar_notas(id, page.offset)).await?;
    let hoje = (Utc::now() - chrono::Duration::hours(3)).date_naive();
    let itens = notas
        .itens
        .into_iter()
        .map(|nota| {
            let calculo = NaiveDate::parse_from_str(&nota.data_emissao_nf, "%Y-%m-%d")
                .ok()
                .and_then(|emissao| {
                    calcular(
                        emissao,
                        nota.prazo_dias_pagamento,
                        nota.valor_nf_centavos,
                        hoje,
                        0,
                        0,
                    )
                    .ok()
                });
            json!({"nota":nota,"calculo":calculo})
        })
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"itens":itens,"total":notas.total,"offset":notas.offset,"limite":notas.limite}),
    ))
}

#[derive(Default, Deserialize)]
struct Taxas {
    #[serde(default)]
    multa_bps: u32,
    #[serde(default)]
    juros_bps: u32,
}

async fn oficio(
    State(s): State<Estado>,
    Path(id): Path<i64>,
    Query(taxas): Query<Taxas>,
) -> Result<Response, ErroApi> {
    let empresa = s.empresa.id;
    let nota = banco(&s, move |r| r.obter_nota(empresa, id)).await?;
    let emissao = NaiveDate::parse_from_str(&nota.data_emissao_nf, "%Y-%m-%d")
        .map_err(|_| ErroApi(StatusCode::BAD_REQUEST, "Data de emissão inválida.".into()))?;
    let hoje = (Utc::now() - chrono::Duration::hours(3)).date_naive();
    let c = calcular(
        emissao,
        nota.prazo_dias_pagamento,
        nota.valor_nf_centavos,
        hoje,
        taxas.multa_bps,
        taxas.juros_bps,
    )
    .map_err(|e| ErroApi(StatusCode::BAD_REQUEST, e))?;
    let texto = format!(
        "MINUTA — OFÍCIO DE COBRANÇA Nº ____/{}\n\nÀ Unidade Gestora: {}\nAssunto: Solicitação de pagamento — Lei nº 14.133/2021\nReferência: Empenho nº {} | {} ({}) | NF nº {}\n\nPrezado(a) Ordenador(a) de Despesas,\n\nSolicitamos a verificação e regularização do pagamento da {} nº {}, emitida em {}, no valor principal de R$ {}.\nO vencimento calculado é {}, com {} dia(s) de atraso.\n\nO art. 92, V, da Lei nº 14.133/2021 trata das condições de pagamento e dos critérios de atualização monetária. Solicitamos informações sobre a observância da ordem cronológica de pagamentos prevista no art. 141, quando aplicável.\nOs encargos dependem de previsão contratual e da conferência do termo inicial.\n\nPrincipal: R$ {}\nMulta informada ({}%): R$ {}\nJuros informados ({}% a.m., pro-rata die / 30): R$ {}\nTotal calculado: R$ {}\n\nSolicitamos esclarecimentos e providências para a regularização.\n\nAtenciosamente,\n{} (CNPJ: {})\n",
        hoje.format("%Y"),
        nota.ug,
        nota.empenho,
        nota.modalidade,
        nota.numero_edital_ata,
        nota.numero_nf,
        nota.tipo_documento,
        nota.numero_nf,
        emissao.format("%d/%m/%Y"),
        reais(c.principal_centavos),
        c.vencimento.format("%d/%m/%Y"),
        c.dias_atraso,
        reais(c.principal_centavos),
        reais(i64::from(taxas.multa_bps)),
        reais(c.multa_centavos),
        reais(i64::from(taxas.juros_bps)),
        reais(c.juros_centavos),
        reais(c.total_centavos),
        s.empresa.razao_social,
        s.empresa.cnpj
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=Oficio_Cobranca_{id}.txt")).map_err(
            |_| {
                ErroApi(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Falha no download.".into(),
                )
            },
        )?,
    );
    Ok((headers, Body::from(texto)).into_response())
}

#[derive(Serialize)]
struct Resultado {
    arquivo: String,
    analise: Option<Analise>,
    erro: Option<String>,
}

async fn analisar(
    State(s): State<Estado>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ErroApi> {
    let _permit = Arc::clone(&s.admissao)
        .try_acquire_owned()
        .map_err(|_| ocupado())?;
    let mut resultados = Vec::with_capacity(MAX_FILES);
    let mut total = 0_usize;
    while let Some(mut field) = multipart.next_field().await.map_err(|_| {
        ErroApi(
            StatusCode::BAD_REQUEST,
            "Upload multipart inválido ou acima do limite.".into(),
        )
    })? {
        let Some(nome) = field.file_name() else {
            return Err(ErroApi(
                StatusCode::BAD_REQUEST,
                "Envie arquivos no formulário.".into(),
            ));
        };
        if resultados.len() >= MAX_FILES {
            return Err(ErroApi(
                StatusCode::PAYLOAD_TOO_LARGE,
                "O lote admite até 10 arquivos.".into(),
            ));
        }
        let nome: String = nome.chars().filter(|c| !c.is_control()).take(160).collect();
        let temp = tempfile::NamedTempFile::new()
            .map_err(|_| {
                ErroApi(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Sem espaço para o upload temporário.".into(),
                )
            })?
            .into_temp_path();
        let mut output = tokio::fs::File::create(&temp).await.map_err(|_| {
            ErroApi(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha no upload temporário.".into(),
            )
        })?;
        let mut size = 0_usize;
        while let Some(chunk) = field.chunk().await.map_err(|_| {
            ErroApi(
                StatusCode::BAD_REQUEST,
                "Upload interrompido ou acima do limite.".into(),
            )
        })? {
            size = size.saturating_add(chunk.len());
            total = total.saturating_add(chunk.len());
            if size > MAX_PDF_BYTES || total > MAX_BATCH_BYTES {
                return Err(ErroApi(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Limite: 5 MiB por PDF e 20 MiB por lote.".into(),
                ));
            }
            output.write_all(&chunk).await.map_err(|_| {
                ErroApi(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Falha ao gravar upload temporário.".into(),
                )
            })?;
        }
        output.flush().await.map_err(|_| {
            ErroApi(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha ao concluir upload.".into(),
            )
        })?;
        drop(output);
        let (analise, erro) = match s.pipeline.analisar(&temp).await {
            Ok(a) => (Some(a), None),
            Err(e) => (None, Some(e.to_string())),
        };
        resultados.push(Resultado {
            arquivo: nome,
            analise,
            erro,
        });
    }
    if resultados.is_empty() {
        return Err(ErroApi(
            StatusCode::BAD_REQUEST,
            "Selecione pelo menos um PDF.".into(),
        ));
    }
    Ok(Json(json!({"resultados":resultados})))
}
