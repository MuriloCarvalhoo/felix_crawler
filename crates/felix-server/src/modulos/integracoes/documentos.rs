//! Documentos baixados do Portal: listagem com sugestão de vínculo
//! (`sugerir_empenho`/`sugerir_nota`, candidatos buscados só para as UGs e
//! números da página) e ações humanas (vincular, importar, ignorar). A
//! transição de estado da nota (liquidada/paga) reutiliza
//! `notas::aplicar_evento`, com as mesmas regras da transição manual.

use crate::{
    http::ErroApi,
    modulos::{
        assinatura::{Recurso, verificar_limite},
        empenhos::{
            TAMANHO_NOME, TAMANHO_OBSERVACAO, comum::texto_cortado, repo as empenhos_repo,
            tipos::Empenho,
        },
        notas,
    },
};
use chrono::{DateTime, NaiveDate, Utc};
use felix_core::dinheiro::reais;
use felix_transparencia::{
    Documento, EmpenhoRef, Fase, NotaRef, numero_empenho, sugerir_empenho, sugerir_nota,
};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

/// Item de `GET .../documentos` e resposta das ações.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DocumentoTransparencia {
    pub id: Uuid,
    pub empresa_id: Uuid,
    pub empresa_cnpj: String,
    pub codigo: String,
    pub codigo_resumido: Option<String>,
    pub fase: i16,
    pub data: Option<NaiveDate>,
    pub valor_centavos: Option<i64>,
    pub ug: Option<String>,
    pub ug_nome: Option<String>,
    pub orgao: Option<String>,
    pub especie: Option<String>,
    pub observacao: Option<String>,
    pub empenho_codigo: Option<String>,
    pub situacao: String,
    pub empenho_id: Option<Uuid>,
    pub nota_id: Option<Uuid>,
    pub importado_em: DateTime<Utc>,
    /// Vínculo sugerido (só para documentos `novo`); `null` sem sugestão.
    #[sqlx(skip)]
    pub sugestao: Option<Sugestao>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sugestao {
    /// `empenho` (fase 1) ou `nota` (fases 2 e 3).
    pub tipo: &'static str,
    pub id: Uuid,
    pub rotulo: String,
}

const SELECT_DOCUMENTO: &str =
    "SELECT d.id, d.empresa_id, emp.cnpj::text AS empresa_cnpj, d.codigo,
        d.codigo_resumido, d.fase, d.data, d.valor_centavos, d.ug, d.ug_nome, d.orgao, d.especie,
        d.observacao, d.empenho_codigo, d.situacao, d.empenho_id, d.nota_id, d.importado_em
     FROM documentos_transparencia d
     JOIN empresas emp ON emp.organizacao_id = d.organizacao_id AND emp.id = d.empresa_id";

/// Filtros da listagem (já validados).
#[derive(Debug, Clone, Copy, Default)]
pub struct FiltroDocumentos<'a> {
    pub situacao: Option<&'a str>,
    pub fase: Option<i16>,
    pub limite: i64,
    pub offset: i64,
}

pub async fn listar(
    conn: &mut PgConnection,
    org: Uuid,
    filtro: FiltroDocumentos<'_>,
) -> Result<(Vec<DocumentoTransparencia>, i64), ErroApi> {
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM documentos_transparencia d
         WHERE d.organizacao_id = $1 AND ($2::text IS NULL OR d.situacao = $2)
           AND ($3::smallint IS NULL OR d.fase = $3)",
    )
    .bind(org)
    .bind(filtro.situacao)
    .bind(filtro.fase)
    .fetch_one(&mut *conn)
    .await?;
    let mut itens: Vec<DocumentoTransparencia> = sqlx::query_as(&format!(
        "{SELECT_DOCUMENTO}
         WHERE d.organizacao_id = $1 AND ($2::text IS NULL OR d.situacao = $2)
           AND ($3::smallint IS NULL OR d.fase = $3)
         ORDER BY d.data DESC NULLS LAST, d.codigo DESC
         LIMIT $4 OFFSET $5"
    ))
    .bind(org)
    .bind(filtro.situacao)
    .bind(filtro.fase)
    .bind(filtro.limite)
    .bind(filtro.offset)
    .fetch_all(&mut *conn)
    .await?;
    preencher_sugestoes(conn, org, &mut itens).await?;
    Ok((itens, total))
}

pub async fn buscar(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<DocumentoTransparencia, ErroApi> {
    sqlx::query_as(&format!(
        "{SELECT_DOCUMENTO} WHERE d.organizacao_id = $1 AND d.id = $2"
    ))
    .bind(org)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ErroApi::nao_encontrado_msg("Documento não encontrado."))
}

/// Trava o documento para uma ação; vinculado → 409.
async fn travar(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<DocumentoTransparencia, ErroApi> {
    sqlx::query(
        "SELECT 1 FROM documentos_transparencia WHERE organizacao_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(org)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ErroApi::nao_encontrado_msg("Documento não encontrado."))?;
    let doc = buscar(conn, org, id).await?;
    if doc.situacao == "vinculado" {
        return Err(ErroApi::conflito("Documento já vinculado."));
    }
    Ok(doc)
}

fn fase_de(doc: &DocumentoTransparencia) -> Result<Fase, ErroApi> {
    Fase::try_from(doc.fase).map_err(|_| ErroApi::interno_log("documento", "fase inválida"))
}

/// Código exibido como "documento" no evento da nota.
fn codigo_curto(doc: &DocumentoTransparencia) -> String {
    doc.codigo_resumido
        .clone()
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| doc.codigo.clone())
}

/// Número do empenho a que o documento se refere.
fn alvo_empenho(doc: &DocumentoTransparencia) -> Option<String> {
    doc.empenho_codigo
        .as_deref()
        .and_then(numero_empenho)
        .or_else(|| doc.codigo_resumido.as_deref().and_then(numero_empenho))
}

/// Expressão regular (POSIX, só dígitos e letras fixas) que encontra no banco
/// os números de empenho equivalentes a `AAAANE000290` (`2026NE290`,
/// `2026NE000290`, código completo). A conferência exata fica com
/// `sugerir_empenho`/`sugerir_nota`.
fn padrao_numero(alvo: &str) -> Option<String> {
    let (ano, numero) = alvo.split_once("NE")?;
    if ano.len() != 4
        || !ano.bytes().all(|b| b.is_ascii_digit())
        || numero.is_empty()
        || !numero.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let numero = numero.trim_start_matches('0');
    Some(format!("{ano}NE0*{numero}([^0-9]|$)"))
}

#[derive(sqlx::FromRow)]
struct NotaAberta {
    id: Uuid,
    numero: String,
    valor_centavos: i64,
    status: String,
    empenho_numero: String,
    ug: String,
}

async fn preencher_sugestoes(
    conn: &mut PgConnection,
    org: Uuid,
    docs: &mut [DocumentoTransparencia],
) -> Result<(), ErroApi> {
    // Candidatos só para as UGs/números dos documentos `novo` desta página.
    let mut ugs = Vec::new();
    let mut padroes = Vec::new();
    for doc in docs.iter().filter(|d| d.situacao == "novo") {
        let (Some(ug), Some(alvo)) = (doc.ug.as_deref().map(str::trim), alvo_empenho(doc)) else {
            continue;
        };
        if let Some(padrao) = padrao_numero(&alvo)
            && !ug.is_empty()
        {
            ugs.push(ug.to_owned());
            padroes.push(padrao);
        }
    }
    if ugs.is_empty() {
        return Ok(());
    }
    let empenhos: Vec<EmpenhoRef<Uuid>> = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT DISTINCT e.id, e.ug::text, e.numero
         FROM empenhos e
         JOIN unnest($2::text[], $3::text[]) AS t(ug, padrao)
           ON e.ug::text = t.ug AND upper(regexp_replace(e.numero, '\\s', '', 'g')) ~ t.padrao
         WHERE e.organizacao_id = $1 AND e.status = 'ativo'",
    )
    .bind(org)
    .bind(&ugs)
    .bind(&padroes)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(id, ug, numero)| EmpenhoRef { id, ug, numero })
    .collect();
    let notas: Vec<NotaAberta> = sqlx::query_as(
        "SELECT DISTINCT n.id, n.numero, n.valor_centavos, n.status, e.numero AS empenho_numero,
                e.ug::text AS ug
         FROM notas n
         JOIN empenhos e ON e.organizacao_id = n.organizacao_id AND e.id = n.empenho_id
         JOIN unnest($2::text[], $3::text[]) AS t(ug, padrao)
           ON e.ug::text = t.ug AND upper(regexp_replace(e.numero, '\\s', '', 'g')) ~ t.padrao
         WHERE n.organizacao_id = $1 AND n.status IN ('emitida', 'liquidada')",
    )
    .bind(org)
    .bind(&ugs)
    .bind(&padroes)
    .fetch_all(&mut *conn)
    .await?;
    for doc in docs.iter_mut().filter(|d| d.situacao == "novo") {
        doc.sugestao = sugerir(doc, &empenhos, &notas);
    }
    Ok(())
}

fn sugerir(
    doc: &DocumentoTransparencia,
    empenhos: &[EmpenhoRef<Uuid>],
    notas: &[NotaAberta],
) -> Option<Sugestao> {
    let alvo = alvo_empenho(doc)?;
    let ug = doc.ug.clone().unwrap_or_default();
    match Fase::try_from(doc.fase).ok()? {
        Fase::Empenho => {
            let e = sugerir_empenho(&ug, &alvo, empenhos)?;
            Some(Sugestao {
                tipo: "empenho",
                id: e.id,
                rotulo: format!("Empenho {} — UG {}", e.numero, e.ug.trim()),
            })
        }
        fase => {
            let refs: Vec<NotaRef<Uuid>> = notas
                .iter()
                .map(|n| NotaRef {
                    id: n.id,
                    empenho_numero: n.empenho_numero.clone(),
                    ug: n.ug.clone(),
                    valor_centavos: n.valor_centavos,
                    // Liquidação só se aplica a nota emitida; pagamento, a emitida ou liquidada.
                    status_aberta: fase == Fase::Pagamento || n.status == "emitida",
                })
                .collect();
            let documento = Documento {
                codigo: doc.codigo.clone(),
                codigo_resumido: doc.codigo_resumido.clone().unwrap_or_default(),
                fase,
                data: doc.data,
                valor_centavos: doc.valor_centavos,
                ug_codigo: ug,
                ug_nome: doc.ug_nome.clone().unwrap_or_default(),
                orgao: doc.orgao.clone().unwrap_or_default(),
                especie: doc.especie.clone().unwrap_or_default(),
                observacao: doc.observacao.clone().unwrap_or_default(),
                favorecido_codigo: String::new(),
                numero_processo: String::new(),
                bruto: serde_json::Value::Null,
            };
            let escolhida = sugerir_nota(&documento, &alvo, &refs)?;
            let nota = notas.iter().find(|n| n.id == escolhida.id)?;
            Some(Sugestao {
                tipo: "nota",
                id: nota.id,
                rotulo: format!(
                    "NF {} — empenho {} — R$ {}",
                    nota.numero,
                    nota.empenho_numero,
                    reais(nota.valor_centavos)
                ),
            })
        }
    }
}

/// Vincula o documento a um empenho (fase 1) ou a uma nota (fases 2/3; cria o
/// evento `liquidada`/`paga` com origem `transparencia`).
pub async fn vincular(
    conn: &mut PgConnection,
    org: Uuid,
    usuario: Uuid,
    id: Uuid,
    empenho_id: Option<Uuid>,
    nota_id: Option<Uuid>,
    hoje: NaiveDate,
) -> Result<DocumentoTransparencia, ErroApi> {
    let doc = travar(conn, org, id).await?;
    match fase_de(&doc)? {
        Fase::Empenho => {
            let empenho_id = empenho_id.ok_or_else(|| {
                ErroApi::validacao_campo("empenho_id", "Informe o empenho para vincular.")
            })?;
            sqlx::query("SELECT 1 FROM empenhos WHERE organizacao_id = $1 AND id = $2")
                .bind(org)
                .bind(empenho_id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or_else(|| ErroApi::nao_encontrado_msg("Empenho não encontrado."))?;
            marcar_vinculado(conn, org, id, Some(empenho_id), None).await?;
        }
        fase => {
            let nota_id = nota_id.ok_or_else(|| {
                ErroApi::validacao_campo("nota_id", "Informe a nota fiscal para vincular.")
            })?;
            let empenho_da_nota =
                registrar_evento(conn, org, usuario, nota_id, fase, &doc, hoje).await?;
            marcar_vinculado(conn, org, id, Some(empenho_da_nota), Some(nota_id)).await?;
        }
    }
    buscar(conn, org, id).await
}

async fn marcar_vinculado(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    empenho_id: Option<Uuid>,
    nota_id: Option<Uuid>,
) -> Result<(), ErroApi> {
    sqlx::query(
        "UPDATE documentos_transparencia SET situacao = 'vinculado', empenho_id = $3, nota_id = $4
         WHERE organizacao_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(empenho_id)
    .bind(nota_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Aplica a liquidação (fase 2) ou o pagamento (fase 3) pela mesma máquina de
/// estados da transição manual ([`notas::aplicar_evento`]): data do documento
/// (ou hoje) validada contra emissão/liquidação/hoje; valor acima do valor da
/// nota (OB bruta) é limitado a ele, com observação. Devolve o empenho da nota.
async fn registrar_evento(
    conn: &mut PgConnection,
    org: Uuid,
    usuario: Uuid,
    nota_id: Uuid,
    fase: Fase,
    doc: &DocumentoTransparencia,
    hoje: NaiveDate,
) -> Result<Uuid, ErroApi> {
    let tipo = if fase == Fase::Liquidacao {
        "liquidada"
    } else {
        "paga"
    };
    let aplicado = notas::aplicar_evento(
        conn,
        org,
        usuario,
        nota_id,
        notas::EntradaEvento {
            tipo: tipo.to_owned(),
            data: doc.data.unwrap_or(hoje),
            valor_centavos: doc.valor_centavos.filter(|v| *v > 0),
            documento: Some(codigo_curto(doc)),
            observacao: Some(format!("Portal da Transparência: {}", doc.codigo)),
            origem: "transparencia",
            limitar_ao_valor_da_nota: true,
        },
        hoje,
    )
    .await?;
    Ok(aplicado.empenho_id)
}

/// Gestão (5 dígitos) do código completo `UG(6) + gestão(5) + número(12)`.
fn gestao_do_codigo(codigo: &str) -> Option<String> {
    if codigo.len() != 23 || !codigo.is_ascii() {
        return None;
    }
    codigo
        .get(6..11)
        .filter(|g| g.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_owned)
}

/// Cria um empenho (origem `transparencia`) a partir de um documento de fase 1.
/// UG + número já cadastrados → 409 (use vincular).
pub async fn importar(
    conn: &mut PgConnection,
    org: Uuid,
    usuario: Uuid,
    id: Uuid,
    agora: DateTime<Utc>,
) -> Result<(DocumentoTransparencia, Empenho), ErroApi> {
    let doc = travar(conn, org, id).await?;
    if fase_de(&doc)? != Fase::Empenho {
        return Err(ErroApi::validacao(
            "Só documentos de empenho (fase 1) podem ser importados.",
        ));
    }
    let ug = doc.ug.as_deref().unwrap_or_default().trim().to_owned();
    if ug.len() != 6 || !ug.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ErroApi::validacao("Documento sem UG válida (6 dígitos)."));
    }
    let numero = alvo_empenho(&doc)
        .ok_or_else(|| ErroApi::validacao("Documento sem número de empenho reconhecível."))?;
    let valor = doc
        .valor_centavos
        .filter(|v| *v > 0)
        .ok_or_else(|| ErroApi::validacao("Documento sem valor positivo."))?;
    let existentes: Vec<String> =
        sqlx::query_scalar("SELECT numero FROM empenhos WHERE organizacao_id = $1 AND ug = $2")
            .bind(org)
            .bind(&ug)
            .fetch_all(&mut *conn)
            .await?;
    if existentes
        .iter()
        .any(|n| n == &numero || numero_empenho(n).as_deref() == Some(numero.as_str()))
    {
        return Err(ErroApi::conflito(
            "Empenho já cadastrado com esta UG e número; use vincular.",
        ));
    }
    verificar_limite(conn, org, Recurso::EmpenhoAtivo, agora).await?;
    let empenho_id: Uuid = sqlx::query_scalar(
        "INSERT INTO empenhos
             (organizacao_id, empresa_id, ug, ug_nome, orgao, gestao, numero, valor_total_centavos,
              data_emissao, origem, observacao, criado_por)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'transparencia', $10, $11)
         RETURNING id",
    )
    .bind(org)
    .bind(doc.empresa_id)
    .bind(&ug)
    .bind(texto_cortado(doc.ug_nome.as_deref(), TAMANHO_NOME))
    .bind(texto_cortado(doc.orgao.as_deref(), TAMANHO_NOME))
    .bind(gestao_do_codigo(&doc.codigo))
    .bind(&numero)
    .bind(valor)
    .bind(doc.data)
    .bind(texto_cortado(doc.observacao.as_deref(), TAMANHO_OBSERVACAO))
    .bind(usuario)
    .fetch_one(&mut *conn)
    .await?;
    marcar_vinculado(conn, org, id, Some(empenho_id), None).await?;
    let empenho = empenhos_repo::buscar(conn, org, empenho_id).await?;
    let doc = buscar(conn, org, id).await?;
    Ok((doc, empenho))
}

/// Marca o documento como ignorado (vinculado → 409).
pub async fn ignorar(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<DocumentoTransparencia, ErroApi> {
    travar(conn, org, id).await?;
    sqlx::query(
        "UPDATE documentos_transparencia SET situacao = 'ignorado'
         WHERE organizacao_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    buscar(conn, org, id).await
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn padrao_do_numero_de_empenho() {
        assert_eq!(
            padrao_numero("2026NE000290").as_deref(),
            Some("2026NE0*290([^0-9]|$)")
        );
        assert_eq!(padrao_numero("x"), None);
        assert_eq!(padrao_numero("2026NE"), None);
    }

    #[test]
    fn gestao_do_codigo_completo() {
        assert_eq!(
            gestao_do_codigo("160203000012026NE000290").as_deref(),
            Some("00001")
        );
        assert_eq!(gestao_do_codigo("2026NE000290"), None);
    }
}
