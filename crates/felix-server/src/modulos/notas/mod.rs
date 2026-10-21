//! Notas fiscais: criação com trava de saldo, listagem, edição e máquina de
//! estados (02-arquitetura.md). Dono: agente G.

pub mod repo;
pub mod tipos;

use crate::{
    app::AppState,
    db,
    http::{Caminho, ClienteInfo, Consulta, Corpo, ErroApi, OrgCtx, Papel, criado, dados},
    modulos::{
        cobrancas,
        empenhos::comum::{Erros, padrao_busca, texto_obrigatorio, texto_opcional, violacao_unica},
    },
};
use axum::{
    Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::NaiveDate;
use felix_core::dinheiro::{MAX_CENTAVOS, reais};
use serde_json::json;
use sqlx::PgConnection;
use tipos::{
    EdicaoNota, FiltroNotas, Nota, NotaDetalhe, NovaNota, NovoEvento, STATUS_NOTA,
    TAMANHO_DOCUMENTO, TAMANHO_NUMERO, TAMANHO_OBSERVACAO, TIPOS_NOTA,
};
use uuid::Uuid;

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org}/notas", get(listar))
        .route("/orgs/{org}/empenhos/{id}/notas", post(criar))
        .route("/orgs/{org}/notas/{id}", get(detalhe).patch(editar))
        .route("/orgs/{org}/notas/{id}/eventos", post(registrar_evento))
}

/// Data mínima aceita em notas e eventos.
fn data_minima() -> NaiveDate {
    NaiveDate::from_ymd_opt(2000, 1, 1).unwrap_or(NaiveDate::MIN)
}

fn validar_data(erros: &mut Erros, campo: &str, data: NaiveDate, hoje: NaiveDate) {
    if data < data_minima() {
        erros.add(campo, "Data anterior a 01/01/2000.");
    } else if data > hoje {
        erros.add(campo, "A data não pode ser futura.");
    }
}

fn validar_valor(erros: &mut Erros, campo: &str, valor: i64) {
    if !(1..=MAX_CENTAVOS).contains(&valor) {
        erros.add(campo, "Informe um valor maior que zero e dentro do limite.");
    }
}

fn conflito_saldo(saldo: i64) -> ErroApi {
    ErroApi::conflito(format!(
        "Valor excede o saldo do empenho (saldo disponível: R$ {}).",
        reais(saldo.max(0))
    ))
}

fn conflito_duplicada(tipo: &str, numero: &str) -> ErroApi {
    ErroApi::conflito(format!(
        "Já existe nota de {} nº {numero} neste empenho.",
        if tipo == "servico" {
            "serviço"
        } else {
            "produto"
        }
    ))
}

async fn criar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, empenho_id)): Caminho<(Uuid, Uuid)>,
    Corpo(corpo): Corpo<NovaNota>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let hoje = estado.hoje();
    let mut erros = Erros::default();
    let numero = texto_obrigatorio(&mut erros, "numero", &corpo.numero, TAMANHO_NUMERO);
    let tipo = corpo.tipo.trim().to_lowercase();
    if !TIPOS_NOTA.contains(&tipo.as_str()) {
        erros.add("tipo", "Use produto ou servico.");
    }
    validar_valor(&mut erros, "valor_centavos", corpo.valor_centavos);
    validar_data(&mut erros, "data_emissao", corpo.data_emissao, hoje);
    if let Some(recebimento) = corpo.data_recebimento {
        if recebimento < corpo.data_emissao {
            erros.add(
                "data_recebimento",
                "O recebimento não pode ser anterior à emissão.",
            );
        } else {
            validar_data(&mut erros, "data_recebimento", recebimento, hoje);
        }
    }
    erros.resultado()?;

    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let empenho = repo::travar_empenho(&mut tx, ctx.org_id, empenho_id)
        .await?
        .ok_or_else(|| ErroApi::nao_encontrado_msg("Empenho não encontrado."))?;
    if empenho.status != "ativo" {
        return Err(ErroApi::conflito("Empenho anulado não aceita novas notas."));
    }
    let faturado = repo::faturado(&mut tx, ctx.org_id, empenho.id, None).await?;
    let saldo = empenho.valor_total_centavos - faturado;
    if corpo.valor_centavos > saldo {
        return Err(conflito_saldo(saldo));
    }
    let vencimento = empenho.vencimento(corpo.data_emissao, corpo.data_recebimento)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO notas (organizacao_id, empenho_id, numero, tipo, valor_centavos,
                            data_emissao, data_recebimento, vencimento, criado_por)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(ctx.org_id)
    .bind(empenho.id)
    .bind(&numero)
    .bind(&tipo)
    .bind(corpo.valor_centavos)
    .bind(corpo.data_emissao)
    .bind(corpo.data_recebimento)
    .bind(vencimento)
    .bind(ctx.usuario_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if violacao_unica(&e) {
            conflito_duplicada(&tipo, &numero)
        } else {
            ErroApi::from(e)
        }
    })?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "nota.criar",
        "nota",
        Some(&id.to_string()),
        json!({"empenho_id": empenho.id, "numero": numero, "tipo": tipo,
               "valor_centavos": corpo.valor_centavos, "vencimento": vencimento}),
        cliente.ip,
    )
    .await?;
    let nota = repo::buscar(&mut tx, ctx.org_id, id, hoje).await?;
    tx.commit().await?;
    Ok(criado(nota))
}

async fn listar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Consulta(filtro): Consulta<FiltroNotas>,
) -> Result<impl IntoResponse, ErroApi> {
    let mut erros = Erros::default();
    let status = filtro.status.as_deref().and_then(|texto| {
        let lista: Vec<String> = texto
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        (!lista.is_empty()).then_some(lista)
    });
    if let Some(lista) = &status
        && lista.iter().any(|s| !STATUS_NOTA.contains(&s.as_str()))
    {
        erros.add("status", "Use emitida, liquidada, paga ou cancelada.");
    }
    let empenho_id = match filtro.empenho_id.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(texto) => match Uuid::parse_str(texto) {
            Ok(id) => Some(id),
            Err(_) => {
                erros.add("empenho_id", "Identificador inválido.");
                None
            }
        },
    };
    let situacao = match filtro.situacao.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s @ ("vencida" | "a_vencer" | "vence_7d")) => Some(s.to_owned()),
        Some(_) => {
            erros.add("situacao", "Use vencida, a_vencer ou vence_7d.");
            None
        }
    };
    let ordenar_criado = match filtro.ordenar.as_deref().map(str::trim) {
        None | Some("" | "vencimento") => false,
        Some("criado_em") => true,
        Some(_) => {
            erros.add("ordenar", "Use vencimento ou criado_em.");
            false
        }
    };
    erros.resultado()?;
    let consulta = repo::Filtro {
        status,
        empenho_id,
        situacao,
        busca: padrao_busca(filtro.busca.as_deref()),
        ordenar_criado,
        limite: filtro.paginacao.limite(),
        offset: filtro.paginacao.offset(),
    };
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let (notas, total) = repo::listar(&mut tx, ctx.org_id, &consulta, estado.hoje()).await?;
    tx.commit().await?;
    Ok(filtro.paginacao.lista(notas, total))
}

async fn detalhe(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let nota = repo::buscar(&mut tx, ctx.org_id, id, estado.hoje()).await?;
    let eventos = repo::eventos(&mut tx, ctx.org_id, id).await?;
    let cobrancas = cobrancas::repo::da_nota(&mut tx, ctx.org_id, id).await?;
    tx.commit().await?;
    Ok(dados(NotaDetalhe {
        nota,
        eventos,
        cobrancas,
    }))
}

/// Trava empenho e nota (sempre nessa ordem, evitando deadlock) e devolve ambos.
async fn travar_nota(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    hoje: NaiveDate,
) -> Result<(repo::EmpenhoTravado, Nota), ErroApi> {
    let empenho_id: Option<Uuid> =
        sqlx::query_scalar("SELECT empenho_id FROM notas WHERE organizacao_id = $1 AND id = $2")
            .bind(org)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let empenho_id =
        empenho_id.ok_or_else(|| ErroApi::nao_encontrado_msg("Nota fiscal não encontrada."))?;
    let empenho = repo::travar_empenho(conn, org, empenho_id)
        .await?
        .ok_or_else(|| ErroApi::nao_encontrado_msg("Nota fiscal não encontrada."))?;
    sqlx::query("SELECT 1 FROM notas WHERE organizacao_id = $1 AND id = $2 FOR UPDATE")
        .bind(org)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let nota = repo::buscar(conn, org, id, hoje).await?;
    Ok((empenho, nota))
}

async fn editar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
    Corpo(corpo): Corpo<EdicaoNota>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let hoje = estado.hoje();
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let (empenho, atual) = travar_nota(&mut tx, ctx.org_id, id, hoje).await?;
    if atual.status != "emitida" {
        return Err(ErroApi::conflito(
            "Só notas emitidas podem ser editadas. Reabra a nota antes.",
        ));
    }
    let mut erros = Erros::default();
    let numero = match &corpo.numero {
        Some(n) => texto_obrigatorio(&mut erros, "numero", n, TAMANHO_NUMERO),
        None => atual.numero.clone(),
    };
    let valor = corpo.valor_centavos.unwrap_or(atual.valor_centavos);
    validar_valor(&mut erros, "valor_centavos", valor);
    let emissao = corpo.data_emissao.unwrap_or(atual.data_emissao);
    if corpo.data_emissao.is_some() {
        validar_data(&mut erros, "data_emissao", emissao, hoje);
    }
    let recebimento = corpo.data_recebimento.unwrap_or(atual.data_recebimento);
    if let Some(r) = recebimento {
        if r < emissao {
            erros.add(
                "data_recebimento",
                "O recebimento não pode ser anterior à emissão.",
            );
        } else if corpo.data_recebimento.is_some() {
            validar_data(&mut erros, "data_recebimento", r, hoje);
        }
    }
    erros.resultado()?;
    if valor != atual.valor_centavos {
        let outras = repo::faturado(&mut tx, ctx.org_id, empenho.id, Some(id)).await?;
        let saldo = empenho.valor_total_centavos - outras;
        if valor > saldo {
            return Err(conflito_saldo(saldo));
        }
    }
    let vencimento = empenho.vencimento(emissao, recebimento)?;
    sqlx::query(
        "UPDATE notas SET numero = $3, valor_centavos = $4, data_emissao = $5,
                data_recebimento = $6, vencimento = $7, atualizado_em = now()
          WHERE organizacao_id = $1 AND id = $2",
    )
    .bind(ctx.org_id)
    .bind(id)
    .bind(&numero)
    .bind(valor)
    .bind(emissao)
    .bind(recebimento)
    .bind(vencimento)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if violacao_unica(&e) {
            conflito_duplicada(&atual.tipo, &numero)
        } else {
            ErroApi::from(e)
        }
    })?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "nota.editar",
        "nota",
        Some(&id.to_string()),
        json!({
            "antes": {"numero": atual.numero, "valor_centavos": atual.valor_centavos,
                      "data_emissao": atual.data_emissao, "data_recebimento": atual.data_recebimento},
            "depois": {"numero": numero, "valor_centavos": valor,
                       "data_emissao": emissao, "data_recebimento": recebimento,
                       "vencimento": vencimento}
        }),
        cliente.ip,
    )
    .await?;
    let nota = repo::buscar(&mut tx, ctx.org_id, id, hoje).await?;
    tx.commit().await?;
    Ok(dados(nota))
}

/// Alteração da nota resultante de um evento válido.
struct Transicao {
    status: &'static str,
    valor_pago: i64,
    data_liquidacao: Option<NaiveDate>,
    data_pagamento: Option<NaiveDate>,
    documento_liquidacao: Option<String>,
    documento_pagamento: Option<String>,
    vencimento: NaiveDate,
    valor_evento: Option<i64>,
}

fn transicao_invalida(tipo: &str, status: &str) -> ErroApi {
    ErroApi::conflito(format!(
        "Transição inválida: nota {status} não aceita o evento \"{tipo}\"."
    ))
}

/// Evento a aplicar numa nota (manual ou vindo do Portal da Transparência).
pub(crate) struct EntradaEvento {
    /// `liquidada`, `paga`, `cancelada` ou `reaberta`.
    pub tipo: String,
    pub data: NaiveDate,
    pub valor_centavos: Option<i64>,
    pub documento: Option<String>,
    pub observacao: Option<String>,
    /// `manual` ou `transparencia`.
    pub origem: &'static str,
    /// Valor acima do valor da nota é limitado a ele (com observação) em vez
    /// de recusado. Usado para documentos do Portal (valor bruto).
    pub limitar_ao_valor_da_nota: bool,
}

/// Evento aplicado: status antes/depois, valor e observação gravados.
pub(crate) struct EventoAplicado {
    pub empenho_id: Uuid,
    pub status_anterior: String,
    pub status: &'static str,
    pub valor_evento: Option<i64>,
}

/// Aplica um evento à nota pela máquina de estados (02-arquitetura.md), com as
/// mesmas regras para a tela e para o Portal: data entre 01/01/2000 e hoje,
/// não anterior à emissão nem (pagamento) à liquidação; valor em 1..=valor da
/// nota. Trava empenho → nota (sempre nessa ordem), atualiza a nota e grava o
/// evento. 404 se a nota não existe; 409 para transição inválida.
pub(crate) async fn aplicar_evento(
    conn: &mut PgConnection,
    org: Uuid,
    usuario: Uuid,
    nota_id: Uuid,
    mut entrada: EntradaEvento,
    hoje: NaiveDate,
) -> Result<EventoAplicado, ErroApi> {
    let mut erros = Erros::default();
    validar_data(&mut erros, "data", entrada.data, hoje);
    erros.resultado()?;
    let (empenho, nota) = travar_nota(conn, org, nota_id, hoje).await?;
    if entrada.limitar_ao_valor_da_nota
        && let Some(valor) = entrada.valor_centavos
        && valor > nota.valor_centavos
    {
        let aviso = format!(
            "Valor do documento (R$ {}) acima do valor da nota; registrado o valor da nota.",
            reais(valor)
        );
        entrada.observacao = Some(match entrada.observacao.take() {
            Some(texto) => format!("{texto} {aviso}"),
            None => aviso,
        });
        entrada.valor_centavos = Some(nota.valor_centavos);
    }
    let transicao = calcular_transicao(conn, org, &entrada, &empenho, &nota).await?;
    sqlx::query(
        "UPDATE notas SET status = $3, valor_pago_centavos = $4, data_liquidacao = $5,
                data_pagamento = $6, documento_liquidacao = $7, documento_pagamento = $8,
                vencimento = $9, atualizado_em = now()
          WHERE organizacao_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(nota_id)
    .bind(transicao.status)
    .bind(transicao.valor_pago)
    .bind(transicao.data_liquidacao)
    .bind(transicao.data_pagamento)
    .bind(&transicao.documento_liquidacao)
    .bind(&transicao.documento_pagamento)
    .bind(transicao.vencimento)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO eventos_nota (organizacao_id, nota_id, tipo, data, valor_centavos,
                                   documento, observacao, origem, criado_por)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(org)
    .bind(nota_id)
    .bind(&entrada.tipo)
    .bind(entrada.data)
    .bind(transicao.valor_evento)
    .bind(&entrada.documento)
    .bind(&entrada.observacao)
    .bind(entrada.origem)
    .bind(usuario)
    .execute(&mut *conn)
    .await?;
    Ok(EventoAplicado {
        empenho_id: empenho.id,
        status_anterior: nota.status,
        status: transicao.status,
        valor_evento: transicao.valor_evento,
    })
}

async fn registrar_evento(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
    Corpo(corpo): Corpo<NovoEvento>,
) -> Result<impl IntoResponse, ErroApi> {
    let tipo = corpo.tipo.trim().to_lowercase();
    match tipo.as_str() {
        "reaberta" => ctx.exigir(Papel::Admin)?,
        "liquidada" | "paga" | "cancelada" => ctx.exigir(Papel::Operador)?,
        _ => {
            return Err(ErroApi::validacao_campo(
                "tipo",
                "Use liquidada, paga, cancelada ou reaberta.",
            ));
        }
    }
    let hoje = estado.hoje();
    let mut erros = Erros::default();
    validar_data(&mut erros, "data", corpo.data, hoje);
    let documento = texto_opcional(
        &mut erros,
        "documento",
        corpo.documento.as_deref(),
        TAMANHO_DOCUMENTO,
    );
    let observacao = texto_opcional(
        &mut erros,
        "observacao",
        corpo.observacao.as_deref(),
        TAMANHO_OBSERVACAO,
    );
    erros.resultado()?;

    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let aplicado = aplicar_evento(
        &mut tx,
        ctx.org_id,
        ctx.usuario_id,
        id,
        EntradaEvento {
            tipo: tipo.clone(),
            data: corpo.data,
            valor_centavos: corpo.valor_centavos,
            documento: documento.clone(),
            observacao,
            origem: "manual",
            limitar_ao_valor_da_nota: false,
        },
        hoje,
    )
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        &format!("nota.{tipo}"),
        "nota",
        Some(&id.to_string()),
        json!({"de": aplicado.status_anterior, "para": aplicado.status, "data": corpo.data,
               "valor_centavos": aplicado.valor_evento, "documento": documento}),
        cliente.ip,
    )
    .await?;
    let nota = repo::buscar(&mut tx, ctx.org_id, id, hoje).await?;
    tx.commit().await?;
    Ok(dados(nota))
}

/// Máquina de estados: valida o evento contra o status atual e devolve o novo estado.
async fn calcular_transicao(
    conn: &mut PgConnection,
    org: Uuid,
    entrada: &EntradaEvento,
    empenho: &repo::EmpenhoTravado,
    nota: &Nota,
) -> Result<Transicao, ErroApi> {
    let tipo = entrada.tipo.as_str();
    let data = entrada.data;
    let status = nota.status.as_str();
    let base = Transicao {
        status: "emitida",
        valor_pago: nota.valor_pago_centavos,
        data_liquidacao: nota.data_liquidacao,
        data_pagamento: nota.data_pagamento,
        documento_liquidacao: nota.documento_liquidacao.clone(),
        documento_pagamento: nota.documento_pagamento.clone(),
        vencimento: nota.vencimento,
        valor_evento: None,
    };
    let antes_da_emissao =
        || ErroApi::validacao_campo("data", "A data não pode ser anterior à emissão da nota.");
    match tipo {
        "liquidada" => {
            if status != "emitida" {
                return Err(transicao_invalida(tipo, status));
            }
            if data < nota.data_emissao {
                return Err(antes_da_emissao());
            }
            if let Some(v) = entrada.valor_centavos
                && !(1..=nota.valor_centavos).contains(&v)
            {
                return Err(ErroApi::validacao_campo(
                    "valor_centavos",
                    "O valor liquidado deve estar entre R$ 0,01 e o valor da nota.",
                ));
            }
            Ok(Transicao {
                status: "liquidada",
                data_liquidacao: Some(data),
                documento_liquidacao: entrada.documento.clone(),
                valor_evento: entrada.valor_centavos,
                ..base
            })
        }
        "paga" => {
            if !matches!(status, "emitida" | "liquidada") {
                return Err(transicao_invalida(tipo, status));
            }
            if data < nota.data_emissao {
                return Err(antes_da_emissao());
            }
            if let Some(liquidacao) = nota.data_liquidacao
                && status == "liquidada"
                && data < liquidacao
            {
                return Err(ErroApi::validacao_campo(
                    "data",
                    "A data do pagamento não pode ser anterior à liquidação.",
                ));
            }
            let valor = entrada.valor_centavos.unwrap_or(nota.valor_centavos);
            if !(1..=nota.valor_centavos).contains(&valor) {
                return Err(ErroApi::validacao_campo(
                    "valor_centavos",
                    "O valor pago deve estar entre R$ 0,01 e o valor da nota.",
                ));
            }
            let data_liquidacao = if status == "emitida" {
                Some(data)
            } else {
                nota.data_liquidacao
            };
            Ok(Transicao {
                status: "paga",
                valor_pago: valor,
                data_liquidacao,
                data_pagamento: Some(data),
                documento_pagamento: entrada.documento.clone(),
                valor_evento: Some(valor),
                ..base
            })
        }
        "cancelada" => match status {
            "emitida" | "liquidada" => Ok(Transicao {
                status: "cancelada",
                ..base
            }),
            "paga" => Err(ErroApi::conflito(
                "Nota paga não pode ser cancelada. Reabra a nota antes, se o pagamento foi estornado.",
            )),
            _ => Err(transicao_invalida(tipo, status)),
        },
        "reaberta" => {
            if !matches!(status, "liquidada" | "paga" | "cancelada") {
                return Err(transicao_invalida(tipo, status));
            }
            if status == "cancelada" {
                if empenho.status != "ativo" {
                    return Err(ErroApi::conflito(
                        "Empenho anulado não aceita notas abertas.",
                    ));
                }
                let outras = repo::faturado(conn, org, empenho.id, Some(nota.id)).await?;
                let saldo = empenho.valor_total_centavos - outras;
                if nota.valor_centavos > saldo {
                    return Err(conflito_saldo(saldo));
                }
            }
            let vencimento = empenho.vencimento(nota.data_emissao, nota.data_recebimento)?;
            Ok(Transicao {
                status: "emitida",
                valor_pago: 0,
                data_liquidacao: None,
                data_pagamento: None,
                documento_liquidacao: None,
                documento_pagamento: None,
                vencimento,
                valor_evento: None,
            })
        }
        _ => Err(ErroApi::validacao_campo("tipo", "Tipo de evento inválido.")),
    }
}
