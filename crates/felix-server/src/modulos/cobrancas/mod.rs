//! Cobranças: cálculo de encargos, minuta de ofício e histórico de envios.
//! Dono: agente G.

pub mod oficio;
pub mod repo;
pub mod tipos;

use crate::{
    app::AppState,
    db,
    http::{Caminho, ClienteInfo, Consulta, Corpo, ErroApi, OrgCtx, Papel, criado, dados},
    modulos::empenhos::comum::{Erros, anexo, nome_arquivo_seguro, texto_opcional},
};
use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Days, NaiveDate};
use felix_core::cobranca::{Calculo, calcular_encargos};
use serde_json::json;
use tipos::{CANAIS, ContextoOficio, FiltroCobrancas, MAX_BPS, NovaCobranca, ParametrosCalculo};
use uuid::Uuid;

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org}/notas/{id}/calculo", get(calculo))
        .route("/orgs/{org}/notas/{id}/oficio", get(oficio_minuta))
        .route("/orgs/{org}/notas/{id}/cobrancas", post(registrar))
        .route("/orgs/{org}/cobrancas", get(listar))
        .route("/orgs/{org}/cobrancas/{id}/oficio", get(oficio_guardado))
}

const CONTENT_TYPE_TEXTO: &str = "text/plain; charset=utf-8";

fn validar_taxas(erros: &mut Erros, multa_bps: u32, juros_bps: u32) {
    if multa_bps > MAX_BPS {
        erros.add("multa_bps", "Use de 0 a 10000 pontos-base (0% a 100%).");
    }
    if juros_bps > MAX_BPS {
        erros.add("juros_bps", "Use de 0 a 10000 pontos-base (0% a 100%).");
    }
}

/// Data de referência: a informada (até hoje + 3650 dias); senão a data do
/// pagamento, se a nota foi paga; senão hoje.
fn data_referencia(
    erros: &mut Erros,
    p: &ParametrosCalculo,
    c: &ContextoOficio,
    hoje: NaiveDate,
) -> NaiveDate {
    match p.data_referencia {
        Some(data) => {
            let limite = hoje.checked_add_days(Days::new(3650)).unwrap_or(hoje);
            if data > limite {
                erros.add(
                    "data_referencia",
                    "Use uma data até 10 anos a partir de hoje.",
                );
            }
            data
        }
        None if c.nota_status == "paga" => c.data_pagamento.unwrap_or(hoje),
        None => hoje,
    }
}

fn calcular(
    c: &ContextoOficio,
    data: NaiveDate,
    multa_bps: u32,
    juros_bps: u32,
) -> Result<Calculo, ErroApi> {
    calcular_encargos(c.vencimento, c.valor_centavos, data, multa_bps, juros_bps)
        .map_err(ErroApi::validacao)
}

async fn preparar(
    estado: &AppState,
    ctx: &OrgCtx,
    nota_id: Uuid,
    p: &ParametrosCalculo,
) -> Result<(ContextoOficio, Calculo, NaiveDate), ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let contexto = repo::contexto(&mut tx, ctx.org_id, nota_id).await?;
    tx.commit().await?;
    let mut erros = Erros::default();
    validar_taxas(&mut erros, p.multa_bps, p.juros_bps);
    let data = data_referencia(&mut erros, p, &contexto, estado.hoje());
    erros.resultado()?;
    let calculo = calcular(&contexto, data, p.multa_bps, p.juros_bps)?;
    Ok((contexto, calculo, data))
}

async fn calculo(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
    Consulta(p): Consulta<ParametrosCalculo>,
) -> Result<impl IntoResponse, ErroApi> {
    let (_, calculo, _) = preparar(&estado, &ctx, id, &p).await?;
    Ok(dados(calculo))
}

fn nome_oficio(numero: &str) -> String {
    format!("Oficio_{}.txt", nome_arquivo_seguro(numero))
}

async fn oficio_minuta(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
    Consulta(p): Consulta<ParametrosCalculo>,
) -> Result<Response, ErroApi> {
    let (contexto, calculo, data) = preparar(&estado, &ctx, id, &p).await?;
    let texto = oficio::gerar(&contexto, &calculo, p.multa_bps, p.juros_bps, data);
    anexo(
        texto.into_bytes(),
        CONTENT_TYPE_TEXTO,
        &nome_oficio(&contexto.nota_numero),
    )
}

async fn registrar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, nota_id)): Caminho<(Uuid, Uuid)>,
    Corpo(corpo): Corpo<NovaCobranca>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let hoje = estado.hoje();
    let mut erros = Erros::default();
    validar_taxas(&mut erros, corpo.multa_bps, corpo.juros_bps);
    let canal = corpo.canal.trim().to_lowercase();
    if !CANAIS.contains(&canal.as_str()) {
        erros.add("canal", "Use email, protocolo, sei, correios ou outro.");
    }
    if corpo.enviada_em > hoje {
        erros.add("enviada_em", "A data de envio não pode ser futura.");
    }
    let protocolo = texto_opcional(&mut erros, "protocolo", corpo.protocolo.as_deref(), 100);
    let observacao = texto_opcional(&mut erros, "observacao", corpo.observacao.as_deref(), 1000);
    erros.resultado()?;

    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    sqlx::query("SELECT 1 FROM notas WHERE organizacao_id = $1 AND id = $2 FOR UPDATE")
        .bind(ctx.org_id)
        .bind(nota_id)
        .execute(&mut *tx)
        .await?;
    let contexto = repo::contexto(&mut tx, ctx.org_id, nota_id).await?;
    if matches!(contexto.nota_status.as_str(), "paga" | "cancelada") {
        return Err(ErroApi::conflito(format!(
            "Nota {} não pode receber cobrança.",
            contexto.nota_status
        )));
    }
    if corpo.enviada_em < contexto.data_emissao {
        return Err(ErroApi::validacao_campo(
            "enviada_em",
            "A data de envio não pode ser anterior à emissão da nota.",
        ));
    }
    let calculo = calcular(
        &contexto,
        corpo.enviada_em,
        corpo.multa_bps,
        corpo.juros_bps,
    )?;
    let texto = oficio::gerar(
        &contexto,
        &calculo,
        corpo.multa_bps,
        corpo.juros_bps,
        corpo.enviada_em,
    );
    let dias_atraso = i32::try_from(calculo.dias_atraso)
        .map_err(|_| ErroApi::validacao("Atraso fora dos limites."))?;
    let multa_bps = i32::try_from(corpo.multa_bps).unwrap_or(i32::MAX);
    let juros_bps = i32::try_from(corpo.juros_bps).unwrap_or(i32::MAX);
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO cobrancas (organizacao_id, nota_id, multa_bps, juros_bps, data_referencia,
                principal_centavos, multa_centavos, juros_centavos, total_centavos, dias_atraso,
                canal, protocolo, enviada_em, observacao, texto, criado_por)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
         RETURNING id",
    )
    .bind(ctx.org_id)
    .bind(nota_id)
    .bind(multa_bps)
    .bind(juros_bps)
    .bind(corpo.enviada_em)
    .bind(calculo.principal_centavos)
    .bind(calculo.multa_centavos)
    .bind(calculo.juros_centavos)
    .bind(calculo.total_centavos)
    .bind(dias_atraso)
    .bind(&canal)
    .bind(&protocolo)
    .bind(corpo.enviada_em)
    .bind(&observacao)
    .bind(&texto)
    .bind(ctx.usuario_id)
    .fetch_one(&mut *tx)
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "cobranca.registrar",
        "cobranca",
        Some(&id.to_string()),
        json!({"nota_id": nota_id, "canal": canal, "protocolo": protocolo,
               "enviada_em": corpo.enviada_em, "total_centavos": calculo.total_centavos}),
        cliente.ip,
    )
    .await?;
    let cobranca = repo::buscar(&mut tx, ctx.org_id, id).await?;
    tx.commit().await?;
    Ok(criado(cobranca))
}

async fn listar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Consulta(filtro): Consulta<FiltroCobrancas>,
) -> Result<impl IntoResponse, ErroApi> {
    let nota_id = match filtro.nota_id.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(texto) => Some(
            Uuid::parse_str(texto)
                .map_err(|_| ErroApi::validacao_campo("nota_id", "Identificador inválido."))?,
        ),
    };
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let (itens, total) = repo::listar(
        &mut tx,
        ctx.org_id,
        nota_id,
        filtro.paginacao.limite(),
        filtro.paginacao.offset(),
    )
    .await?;
    tx.commit().await?;
    Ok(filtro.paginacao.lista(itens, total))
}

async fn oficio_guardado(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
) -> Result<Response, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let cobranca = repo::buscar(&mut tx, ctx.org_id, id).await?;
    tx.commit().await?;
    anexo(
        cobranca.texto.into_bytes(),
        CONTENT_TYPE_TEXTO,
        &nome_oficio(&cobranca.resumo.nota_numero),
    )
}
