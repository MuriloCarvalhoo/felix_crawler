//! Empenhos: análise de PDF, cadastro, edição, anulação e listagem com
//! faturado/recebido/saldo. Dono: agente G.

pub mod comum;
pub mod repo;
pub mod tipos;

use crate::{
    app::AppState,
    db,
    http::{Caminho, ClienteInfo, Consulta, Corpo, ErroApi, OrgCtx, Papel, criado, dados},
    modulos::{
        assinatura::{Recurso, verificar_limite},
        notas::{self, repo::calcular_vencimento},
    },
    pdf,
};
use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::NaiveDate;
use comum::{Erros, padrao_busca, texto_obrigatorio, texto_opcional, violacao_unica};
use felix_core::{dinheiro::MAX_CENTAVOS, prazos::RegraVencimento};
use serde_json::json;
use tipos::{Anulacao, EdicaoEmpenho, Empenho, FiltroEmpenhos, ItemAnalise, NovoEmpenho};
use uuid::Uuid;

pub fn rotas() -> Router<AppState> {
    Router::new()
        .route(
            "/orgs/{org}/empenhos/analisar",
            post(analisar).layer(DefaultBodyLimit::max(pdf::LIMITE_CORPO_ANALISE)),
        )
        .route("/orgs/{org}/empenhos", get(listar).post(criar))
        .route("/orgs/{org}/empenhos/{id}", get(detalhe).patch(editar))
        .route("/orgs/{org}/empenhos/{id}/anular", post(anular))
}

/// Limites dos campos de texto do empenho (também aplicados, por corte, ao
/// importador legado e à importação do Portal).
pub(crate) const TAMANHO_NUMERO: usize = 40;
pub(crate) const TAMANHO_NOME: usize = 200;
pub(crate) const TAMANHO_CURTO: usize = 100;
pub(crate) const TAMANHO_OBSERVACAO: usize = 2000;
const PRAZO_PADRAO: i64 = 30;

/// Campos de empenho já validados e normalizados.
struct Campos {
    ug: String,
    ug_nome: Option<String>,
    orgao: Option<String>,
    gestao: Option<String>,
    numero: String,
    modalidade: Option<String>,
    numero_edital_ata: Option<String>,
    valor_total_centavos: i64,
    data_emissao: Option<NaiveDate>,
    regra_vencimento: RegraVencimento,
    prazo_dias: i32,
    observacao: Option<String>,
}

/// Campos brutos (de `NovoEmpenho` ou do empenho atual + PATCH).
struct Entrada<'a> {
    ug: &'a str,
    ug_nome: Option<&'a str>,
    orgao: Option<&'a str>,
    gestao: Option<&'a str>,
    numero: &'a str,
    modalidade: Option<&'a str>,
    numero_edital_ata: Option<&'a str>,
    valor_total_centavos: i64,
    data_emissao: Option<NaiveDate>,
    regra_vencimento: Option<&'a str>,
    prazo_dias: Option<i64>,
    observacao: Option<&'a str>,
}

fn so_digitos(texto: &str, n: usize) -> bool {
    texto.len() == n && texto.bytes().all(|b| b.is_ascii_digit())
}

fn validar(e: &Entrada<'_>, hoje: NaiveDate, erros: &mut Erros) -> Campos {
    let ug = e.ug.trim().to_owned();
    if !so_digitos(&ug, 6) {
        erros.add("ug", "A UG tem 6 dígitos.");
    }
    let gestao = texto_opcional(erros, "gestao", e.gestao, 5);
    if let Some(g) = &gestao
        && !so_digitos(g, 5)
    {
        erros.add("gestao", "A gestão tem 5 dígitos.");
    }
    let numero = texto_obrigatorio(erros, "numero", e.numero, TAMANHO_NUMERO).to_uppercase();
    if !(1..=MAX_CENTAVOS).contains(&e.valor_total_centavos) {
        erros.add(
            "valor_total_centavos",
            "Informe um valor maior que zero e dentro do limite.",
        );
    }
    if let Some(data) = e.data_emissao {
        let minima = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap_or(NaiveDate::MIN);
        if data < minima || data > hoje {
            erros.add("data_emissao", "Use uma data entre 01/01/2000 e hoje.");
        }
    }
    let regra = match e.regra_vencimento.map(str::trim) {
        None | Some("") => RegraVencimento::default(),
        Some(texto) => texto.parse().unwrap_or_else(|_| {
            erros.add("regra_vencimento", "Use corridos_emissao ou in77_uteis.");
            RegraVencimento::default()
        }),
    };
    let prazo = e.prazo_dias.unwrap_or(PRAZO_PADRAO);
    let prazo_dias = match i32::try_from(prazo) {
        Ok(p) if (1..=3650).contains(&p) => p,
        _ => {
            erros.add("prazo_dias", "Use de 1 a 3650 dias.");
            0
        }
    };
    Campos {
        ug,
        ug_nome: texto_opcional(erros, "ug_nome", e.ug_nome, TAMANHO_NOME),
        orgao: texto_opcional(erros, "orgao", e.orgao, TAMANHO_NOME),
        gestao,
        numero,
        modalidade: texto_opcional(erros, "modalidade", e.modalidade, TAMANHO_CURTO),
        numero_edital_ata: texto_opcional(
            erros,
            "numero_edital_ata",
            e.numero_edital_ata,
            TAMANHO_CURTO,
        ),
        valor_total_centavos: e.valor_total_centavos,
        data_emissao: e.data_emissao,
        regra_vencimento: regra,
        prazo_dias,
        observacao: texto_opcional(erros, "observacao", e.observacao, TAMANHO_OBSERVACAO),
    }
}

fn conflito_duplicado(ug: &str, numero: &str) -> ErroApi {
    ErroApi::conflito(format!(
        "Já existe o empenho {numero} da UG {ug} nesta organização."
    ))
}

async fn analisar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    multipart: Multipart,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let resultados = pdf::receber_e_analisar(&estado, multipart).await?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let mut itens = Vec::with_capacity(resultados.len());
    for resultado in resultados {
        let (mut empresa_id_sugerida, mut ja_cadastrado) = (None, false);
        if let Some(analise) = &resultado.analise {
            let d = &analise.dados;
            if let Some(cnpj) = &d.cnpj_fornecedor {
                empresa_id_sugerida = sqlx::query_scalar(
                    "SELECT id FROM empresas WHERE organizacao_id = $1 AND cnpj = $2",
                )
                .bind(ctx.org_id)
                .bind(cnpj)
                .fetch_optional(&mut *tx)
                .await?;
            }
            if let (Some(ug), Some(numero)) = (&d.ug, &d.empenho) {
                ja_cadastrado =
                    repo::existe_ug_numero(&mut tx, ctx.org_id, ug, numero, None).await?;
            }
        }
        itens.push(ItemAnalise {
            resultado,
            empresa_id_sugerida,
            ja_cadastrado,
        });
    }
    tx.commit().await?;
    Ok(dados(itens))
}

async fn listar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Consulta(filtro): Consulta<FiltroEmpenhos>,
) -> Result<impl IntoResponse, ErroApi> {
    let mut erros = Erros::default();
    let empresa_id = match filtro.empresa_id.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(t) => Uuid::parse_str(t).map(Some).unwrap_or_else(|_| {
            erros.add("empresa_id", "Identificador inválido.");
            None
        }),
    };
    let ug = match filtro.ug.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(t) if so_digitos(t, 6) => Some(t.to_owned()),
        Some(_) => {
            erros.add("ug", "A UG tem 6 dígitos.");
            None
        }
    };
    let status = match filtro.status.as_deref().map(str::trim) {
        None | Some("") | Some("ativo") => Some("ativo".to_owned()),
        Some("anulado") => Some("anulado".to_owned()),
        Some("todos") => None,
        Some(_) => {
            erros.add("status", "Use ativo, anulado ou todos.");
            None
        }
    };
    let com_saldo = match filtro.com_saldo.as_deref().map(str::trim) {
        None | Some("") => None,
        Some("true" | "1") => Some(true),
        Some("false" | "0") => Some(false),
        Some(_) => {
            erros.add("com_saldo", "Use true ou false.");
            None
        }
    };
    erros.resultado()?;
    let f = repo::Filtro {
        busca: padrao_busca(filtro.busca.as_deref()),
        empresa_id,
        ug,
        status,
        com_saldo,
        limite: filtro.paginacao.limite(),
        offset: filtro.paginacao.offset(),
    };
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let (itens, total) = repo::listar(&mut tx, ctx.org_id, &f).await?;
    tx.commit().await?;
    Ok(filtro.paginacao.lista(itens, total))
}

async fn criar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Corpo(corpo): Corpo<NovoEmpenho>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let mut erros = Erros::default();
    let origem = match corpo.origem.as_deref().map(str::trim) {
        None | Some("") => "manual",
        Some("manual") => "manual",
        Some("pdf") => "pdf",
        Some(_) => {
            erros.add("origem", "Use pdf ou manual.");
            "manual"
        }
    };
    let entrada = Entrada {
        ug: &corpo.ug,
        ug_nome: corpo.ug_nome.as_deref(),
        orgao: corpo.orgao.as_deref(),
        gestao: corpo.gestao.as_deref(),
        numero: &corpo.numero,
        modalidade: corpo.modalidade.as_deref(),
        numero_edital_ata: corpo.numero_edital_ata.as_deref(),
        valor_total_centavos: corpo.valor_total_centavos,
        data_emissao: corpo.data_emissao,
        regra_vencimento: corpo.regra_vencimento.as_deref(),
        prazo_dias: corpo.prazo_dias,
        observacao: corpo.observacao.as_deref(),
    };
    let c = validar(&entrada, estado.hoje(), &mut erros);
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let empresa_existe: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM empresas WHERE organizacao_id = $1 AND id = $2)",
    )
    .bind(ctx.org_id)
    .bind(corpo.empresa_id)
    .fetch_one(&mut *tx)
    .await?;
    if !empresa_existe {
        erros.add("empresa_id", "Empresa não encontrada nesta organização.");
    }
    erros.resultado()?;
    if repo::existe_ug_numero(&mut tx, ctx.org_id, &c.ug, &c.numero, None).await? {
        return Err(conflito_duplicado(&c.ug, &c.numero));
    }
    verificar_limite(&mut tx, ctx.org_id, Recurso::EmpenhoAtivo, estado.agora()).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO empenhos (organizacao_id, empresa_id, ug, ug_nome, orgao, gestao, numero,
                modalidade, numero_edital_ata, valor_total_centavos, data_emissao,
                regra_vencimento, prazo_dias, origem, observacao, criado_por)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
         RETURNING id",
    )
    .bind(ctx.org_id)
    .bind(corpo.empresa_id)
    .bind(&c.ug)
    .bind(&c.ug_nome)
    .bind(&c.orgao)
    .bind(&c.gestao)
    .bind(&c.numero)
    .bind(&c.modalidade)
    .bind(&c.numero_edital_ata)
    .bind(c.valor_total_centavos)
    .bind(c.data_emissao)
    .bind(c.regra_vencimento.as_str())
    .bind(c.prazo_dias)
    .bind(origem)
    .bind(&c.observacao)
    .bind(ctx.usuario_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if violacao_unica(&e) {
            conflito_duplicado(&c.ug, &c.numero)
        } else {
            ErroApi::from(e)
        }
    })?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "empenho.criar",
        "empenho",
        Some(&id.to_string()),
        json!({"ug": c.ug, "numero": c.numero, "empresa_id": corpo.empresa_id,
               "valor_total_centavos": c.valor_total_centavos, "origem": origem}),
        cliente.ip,
    )
    .await?;
    let empenho = repo::buscar(&mut tx, ctx.org_id, id).await?;
    tx.commit().await?;
    Ok(criado(empenho))
}

async fn detalhe(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ErroApi> {
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let empenho = repo::buscar(&mut tx, ctx.org_id, id).await?;
    let lista = notas::repo::do_empenho(&mut tx, ctx.org_id, id, estado.hoje()).await?;
    tx.commit().await?;
    Ok(dados(Empenho {
        notas: Some(lista),
        ..empenho
    }))
}

async fn editar(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
    Corpo(p): Corpo<EdicaoEmpenho>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    notas::repo::travar_empenho(&mut tx, ctx.org_id, id)
        .await?
        .ok_or_else(|| ErroApi::nao_encontrado_msg("Empenho não encontrado."))?;
    let atual = repo::buscar(&mut tx, ctx.org_id, id).await?;
    if atual.resumo.status != "ativo" {
        return Err(ErroApi::conflito("Empenho anulado não pode ser editado."));
    }
    let escolher = |novo: &Option<Option<String>>, antigo: &Option<String>| -> Option<String> {
        match novo {
            Some(v) => v.clone(),
            None => antigo.clone(),
        }
    };
    let ug_nome = escolher(&p.ug_nome, &atual.resumo.ug_nome);
    let orgao = escolher(&p.orgao, &atual.resumo.orgao);
    let gestao = escolher(&p.gestao, &atual.gestao);
    let modalidade = escolher(&p.modalidade, &atual.resumo.modalidade);
    let edital = escolher(&p.numero_edital_ata, &atual.numero_edital_ata);
    let observacao = escolher(&p.observacao, &atual.observacao);
    let entrada = Entrada {
        ug: p.ug.as_deref().unwrap_or(&atual.resumo.ug),
        ug_nome: ug_nome.as_deref(),
        orgao: orgao.as_deref(),
        gestao: gestao.as_deref(),
        numero: p.numero.as_deref().unwrap_or(&atual.resumo.numero),
        modalidade: modalidade.as_deref(),
        numero_edital_ata: edital.as_deref(),
        valor_total_centavos: p
            .valor_total_centavos
            .unwrap_or(atual.resumo.valor_total_centavos),
        data_emissao: p.data_emissao.unwrap_or(atual.resumo.data_emissao),
        regra_vencimento: Some(
            p.regra_vencimento
                .as_deref()
                .unwrap_or(&atual.regra_vencimento),
        ),
        prazo_dias: Some(p.prazo_dias.unwrap_or(i64::from(atual.prazo_dias))),
        observacao: observacao.as_deref(),
    };
    let mut erros = Erros::default();
    let mut c = validar(&entrada, estado.hoje(), &mut erros);
    // Só os campos enviados são validados: dados legados/do Portal fora do
    // padrão da tela não podem impedir a edição de outro campo.
    let enviados: Vec<&str> = [
        ("ug", p.ug.is_some()),
        ("ug_nome", p.ug_nome.is_some()),
        ("orgao", p.orgao.is_some()),
        ("gestao", p.gestao.is_some()),
        ("numero", p.numero.is_some()),
        ("modalidade", p.modalidade.is_some()),
        ("numero_edital_ata", p.numero_edital_ata.is_some()),
        ("valor_total_centavos", p.valor_total_centavos.is_some()),
        ("data_emissao", p.data_emissao.is_some()),
        ("regra_vencimento", p.regra_vencimento.is_some()),
        ("prazo_dias", p.prazo_dias.is_some()),
        ("observacao", p.observacao.is_some()),
    ]
    .into_iter()
    .filter_map(|(campo, enviado)| enviado.then_some(campo))
    .collect();
    erros.reter(&enviados);
    erros.resultado()?;
    manter_nao_enviados(&mut c, &p, &atual);
    if c.valor_total_centavos < atual.resumo.faturado_centavos {
        return Err(ErroApi::conflito(format!(
            "O valor total não pode ser menor que o já faturado (R$ {}).",
            felix_core::dinheiro::reais(atual.resumo.faturado_centavos)
        )));
    }
    if (c.ug != atual.resumo.ug || c.numero != atual.resumo.numero)
        && repo::existe_ug_numero(&mut tx, ctx.org_id, &c.ug, &c.numero, Some(id)).await?
    {
        return Err(conflito_duplicado(&c.ug, &c.numero));
    }
    sqlx::query(
        "UPDATE empenhos SET ug = $3, ug_nome = $4, orgao = $5, gestao = $6, numero = $7,
                modalidade = $8, numero_edital_ata = $9, valor_total_centavos = $10,
                data_emissao = $11, regra_vencimento = $12, prazo_dias = $13, observacao = $14,
                atualizado_em = now()
          WHERE organizacao_id = $1 AND id = $2",
    )
    .bind(ctx.org_id)
    .bind(id)
    .bind(&c.ug)
    .bind(&c.ug_nome)
    .bind(&c.orgao)
    .bind(&c.gestao)
    .bind(&c.numero)
    .bind(&c.modalidade)
    .bind(&c.numero_edital_ata)
    .bind(c.valor_total_centavos)
    .bind(c.data_emissao)
    .bind(c.regra_vencimento.as_str())
    .bind(c.prazo_dias)
    .bind(&c.observacao)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if violacao_unica(&e) {
            conflito_duplicado(&c.ug, &c.numero)
        } else {
            ErroApi::from(e)
        }
    })?;
    let regra_mudou = c.regra_vencimento.as_str() != atual.regra_vencimento
        || c.prazo_dias != atual.prazo_dias
        || c.valor_total_centavos != atual.resumo.valor_total_centavos;
    let recalculadas = if regra_mudou {
        recalcular_vencimentos(&mut tx, ctx.org_id, id, &c).await?
    } else {
        0
    };
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "empenho.editar",
        "empenho",
        Some(&id.to_string()),
        json!({
            "antes": {"ug": atual.resumo.ug, "numero": atual.resumo.numero,
                      "valor_total_centavos": atual.resumo.valor_total_centavos,
                      "regra_vencimento": atual.regra_vencimento, "prazo_dias": atual.prazo_dias},
            "depois": {"ug": c.ug, "numero": c.numero,
                       "valor_total_centavos": c.valor_total_centavos,
                       "regra_vencimento": c.regra_vencimento.as_str(), "prazo_dias": c.prazo_dias},
            "notas_recalculadas": recalculadas
        }),
        cliente.ip,
    )
    .await?;
    let empenho = repo::buscar(&mut tx, ctx.org_id, id).await?;
    tx.commit().await?;
    Ok(dados(empenho))
}

/// PATCH: campos não enviados ficam exatamente como estão gravados (sem a
/// normalização da validação).
fn manter_nao_enviados(c: &mut Campos, p: &EdicaoEmpenho, atual: &Empenho) {
    if p.ug.is_none() {
        c.ug.clone_from(&atual.resumo.ug);
    }
    if p.numero.is_none() {
        c.numero.clone_from(&atual.resumo.numero);
    }
    if p.ug_nome.is_none() {
        c.ug_nome.clone_from(&atual.resumo.ug_nome);
    }
    if p.orgao.is_none() {
        c.orgao.clone_from(&atual.resumo.orgao);
    }
    if p.gestao.is_none() {
        c.gestao.clone_from(&atual.gestao);
    }
    if p.modalidade.is_none() {
        c.modalidade.clone_from(&atual.resumo.modalidade);
    }
    if p.numero_edital_ata.is_none() {
        c.numero_edital_ata.clone_from(&atual.numero_edital_ata);
    }
    if p.observacao.is_none() {
        c.observacao.clone_from(&atual.observacao);
    }
    if p.valor_total_centavos.is_none() {
        c.valor_total_centavos = atual.resumo.valor_total_centavos;
    }
    if p.data_emissao.is_none() {
        c.data_emissao = atual.resumo.data_emissao;
    }
    if p.regra_vencimento.is_none()
        && let Ok(regra) = atual.regra_vencimento.parse()
    {
        c.regra_vencimento = regra;
    }
    if p.prazo_dias.is_none() {
        c.prazo_dias = atual.prazo_dias;
    }
}

/// Recalcula `vencimento` das notas emitidas/liquidadas pela regra nova.
/// (O valor do empenho entra na regra IN 77: prazos pela metade até o limite.)
async fn recalcular_vencimentos(
    conn: &mut sqlx::PgConnection,
    org: Uuid,
    empenho_id: Uuid,
    c: &Campos,
) -> Result<usize, ErroApi> {
    let abertas: Vec<(Uuid, NaiveDate, Option<NaiveDate>)> = sqlx::query_as(
        "SELECT id, data_emissao, data_recebimento FROM notas
          WHERE organizacao_id = $1 AND empenho_id = $2 AND status IN ('emitida', 'liquidada')",
    )
    .bind(org)
    .bind(empenho_id)
    .fetch_all(&mut *conn)
    .await?;
    for (nota_id, emissao, recebimento) in &abertas {
        let vencimento = calcular_vencimento(
            c.regra_vencimento.as_str(),
            c.prazo_dias,
            c.valor_total_centavos,
            *emissao,
            *recebimento,
        )?;
        sqlx::query(
            "UPDATE notas SET vencimento = $3, atualizado_em = now()
              WHERE organizacao_id = $1 AND id = $2",
        )
        .bind(org)
        .bind(nota_id)
        .bind(vencimento)
        .execute(&mut *conn)
        .await?;
    }
    Ok(abertas.len())
}

async fn anular(
    State(estado): State<AppState>,
    ctx: OrgCtx,
    cliente: ClienteInfo,
    Caminho((_, id)): Caminho<(Uuid, Uuid)>,
    Corpo(corpo): Corpo<Anulacao>,
) -> Result<impl IntoResponse, ErroApi> {
    ctx.exigir(Papel::Operador)?;
    let mut erros = Erros::default();
    let motivo = texto_obrigatorio(&mut erros, "motivo", &corpo.motivo, 500);
    erros.resultado()?;
    let mut tx = db::tenant(&estado.pool, ctx.org_id).await?;
    let empenho = notas::repo::travar_empenho(&mut tx, ctx.org_id, id)
        .await?
        .ok_or_else(|| ErroApi::nao_encontrado_msg("Empenho não encontrado."))?;
    if empenho.status != "ativo" {
        return Err(ErroApi::conflito("Empenho já está anulado."));
    }
    let (pagas, abertas): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE status = 'paga'),
                COUNT(*) FILTER (WHERE status IN ('emitida', 'liquidada'))
           FROM notas WHERE organizacao_id = $1 AND empenho_id = $2",
    )
    .bind(ctx.org_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if pagas > 0 {
        return Err(ErroApi::conflito(
            "Empenho com nota paga não pode ser anulado.",
        ));
    }
    if abertas > 0 {
        return Err(ErroApi::conflito(
            "Cancele as notas abertas antes de anular.",
        ));
    }
    sqlx::query(
        "UPDATE empenhos SET status = 'anulado', anulado_em = $3, motivo_anulacao = $4,
                atualizado_em = now()
          WHERE organizacao_id = $1 AND id = $2",
    )
    .bind(ctx.org_id)
    .bind(id)
    .bind(estado.agora())
    .bind(&motivo)
    .execute(&mut *tx)
    .await?;
    db::auditar(
        &mut tx,
        ctx.org_id,
        Some(ctx.usuario_id),
        "empenho.anular",
        "empenho",
        Some(&id.to_string()),
        json!({"motivo": motivo}),
        cliente.ip,
    )
    .await?;
    let empenho = repo::buscar(&mut tx, ctx.org_id, id).await?;
    tx.commit().await?;
    Ok(dados(empenho))
}
