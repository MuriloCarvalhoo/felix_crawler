//! Importador do banco SQLite da versão anterior (`felix-server importar-sqlite`).
//!
//! Lê o arquivo em modo somente leitura (sqlx/SQLite) e grava numa única
//! transação de tenant da organização de destino:
//! - `empresas` → empresas (reaproveita o CNPJ já cadastrado na organização);
//! - `empenhos` → empenhos com origem `legado`, regra `corridos_emissao` e prazo
//!   da primeira NF do empenho (ou 30 dias);
//! - `notas_fiscais` → notas (`NFS-e (Serviços)` → serviço, demais → produto;
//!   `PENDENTE` → emitida, `PAGO`/`PAGA` → paga, `CANCELADO`/`CANCELADA` → cancelada),
//!   com o vencimento recalculado pela regra do empenho.
//!
//! Valores `REAL` viram centavos com arredondamento explícito ao centavo mais
//! próximo. Duplicados (CNPJ, UG + número, empenho + tipo + número) são pulados.
//! Os campos seguem os limites da tela: número do empenho em maiúsculas,
//! gestão só com 5 dígitos, textos cortados no tamanho máximo (com aviso), e
//! notas que deixariam o saldo do empenho negativo não entram.
//! Linhas inválidas e avisos entram em `erros` e não interrompem a importação;
//! falhas de banco desfazem tudo. Limites de plano não se aplicam (operação
//! administrativa).

use crate::{
    db,
    http::ErroApi,
    modulos::empenhos::{TAMANHO_CURTO, TAMANHO_NUMERO},
};
use chrono::NaiveDate;
use felix_core::{
    dinheiro::{MAX_CENTAVOS, reais},
    documento::normalizar_cnpj,
    prazos::{RegraVencimento, calcular_vencimento},
};
use serde::Serialize;
use sqlx::{Connection, PgConnection, PgPool, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{collections::HashMap, path::Path};
use thiserror::Error;
use uuid::Uuid;

/// Prazo padrão (dias corridos) quando a NF não traz um prazo válido.
pub const PRAZO_PADRAO: i32 = 30;
const PRAZO_MAXIMO: i32 = 3650;
const OBSERVACAO_LEGADO: &str = "Importado do sistema anterior (SQLite).";

#[derive(Debug, Error)]
pub enum ErroLegado {
    #[error("{0}")]
    Entrada(String),
    #[error("Falha ao ler o SQLite: {0}")]
    Sqlite(String),
    #[error("Falha no banco de destino: {0}")]
    Destino(String),
}

impl From<ErroApi> for ErroLegado {
    fn from(erro: ErroApi) -> Self {
        ErroLegado::Destino(erro.mensagem().to_owned())
    }
}

impl From<sqlx::Error> for ErroLegado {
    fn from(erro: sqlx::Error) -> Self {
        ErroLegado::from(ErroApi::from(erro))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Contagem {
    pub importados: u64,
    pub pulados: u64,
}

/// Resumo impresso em JSON pelo comando.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Resumo {
    pub empresas: Contagem,
    pub empenhos: Contagem,
    pub notas: Contagem,
    pub erros: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct EmpresaLegada {
    id: i64,
    razao_social: Option<String>,
    cnpj: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct EmpenhoLegado {
    id: i64,
    empresa_id: Option<i64>,
    ug: Option<String>,
    gestao: Option<String>,
    empenho: Option<String>,
    modalidade: Option<String>,
    numero_edital_ata: Option<String>,
    valor_total: Option<f64>,
}

#[derive(Debug, sqlx::FromRow)]
struct NotaLegada {
    id: i64,
    empenho_id: Option<i64>,
    tipo_documento: Option<String>,
    numero_nf: Option<String>,
    valor_nf: Option<f64>,
    status: Option<String>,
    data_emissao_nf: Option<String>,
    prazo_dias_pagamento: Option<i64>,
    data_emissao_ob: Option<String>,
}

/// Conteúdo lido do SQLite.
struct Origem {
    empresas: Vec<EmpresaLegada>,
    empenhos: Vec<EmpenhoLegado>,
    notas: Vec<NotaLegada>,
}

/// Empenho de destino usado para recalcular o vencimento das notas.
#[derive(Debug, Clone, Copy)]
struct EmpenhoDestino {
    id: Uuid,
    regra: RegraVencimento,
    prazo: i32,
    valor: i64,
}

/// Reais (`REAL`) → centavos, arredondando ao centavo mais próximo
/// (meio centavo para longe de zero). `None` se não finito, ≤ 0 ou acima do máximo.
pub fn reais_para_centavos(valor: f64) -> Option<i64> {
    if !valor.is_finite() {
        return None;
    }
    let centavos = (valor * 100.0).round();
    #[allow(clippy::cast_precision_loss)]
    let maximo = MAX_CENTAVOS as f64;
    if centavos < 1.0 || centavos > maximo {
        return None;
    }
    // Dentro de 1..=MAX_CENTAVOS (< 2^53): a conversão é exata.
    #[allow(clippy::cast_possible_truncation)]
    let inteiro = centavos as i64;
    Some(inteiro)
}

/// Data `AAAA-MM-DD` (formato do sistema anterior) ou `DD/MM/AAAA`.
pub fn ler_data(texto: &str) -> Option<NaiveDate> {
    let texto = texto.trim();
    let texto = texto.get(..10).unwrap_or(texto);
    NaiveDate::parse_from_str(texto, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(texto, "%d/%m/%Y"))
        .ok()
}

/// Tipo de nota do texto antigo.
pub fn tipo_nota(tipo_documento: &str) -> &'static str {
    if tipo_documento.trim() == "NFS-e (Serviços)" {
        "servico"
    } else {
        "produto"
    }
}

/// Status de nota do texto antigo; `None` se desconhecido.
pub fn status_nota(status: Option<&str>) -> Option<&'static str> {
    match status.map(|s| s.trim().to_uppercase()).as_deref() {
        None | Some("" | "PENDENTE") => Some("emitida"),
        Some("PAGO" | "PAGA") => Some("paga"),
        Some("CANCELADO" | "CANCELADA") => Some("cancelada"),
        Some(_) => None,
    }
}

fn texto_limpo(texto: Option<&str>, maximo: usize) -> Option<String> {
    let texto = texto?.trim();
    if texto.is_empty() {
        return None;
    }
    Some(texto.chars().take(maximo).collect())
}

/// Como [`texto_limpo`], registrando aviso em `erros` se o texto foi cortado.
fn texto_cortado(
    texto: Option<&str>,
    maximo: usize,
    rotulo: &str,
    aviso: &str,
    erros: &mut Vec<String>,
) -> Option<String> {
    let cortado = texto_limpo(texto, maximo)?;
    if texto.is_some_and(|t| t.trim().chars().count() > maximo) {
        erros.push(format!("{rotulo}: {aviso} em {maximo} caracteres."));
    }
    Some(cortado)
}

/// Gestão com 5 dígitos; outro formato é descartado (com aviso).
fn gestao_valida(texto: Option<&str>, rotulo: &str, erros: &mut Vec<String>) -> Option<String> {
    let gestao = texto_limpo(texto, 64)?;
    if gestao.len() == 5 && gestao.bytes().all(|b| b.is_ascii_digit()) {
        Some(gestao)
    } else {
        erros.push(format!(
            "{rotulo}: gestão fora do padrão (5 dígitos); não importada."
        ));
        None
    }
}

async fn ler_origem(arquivo: &Path) -> Result<Origem, ErroLegado> {
    if !arquivo.is_file() {
        return Err(ErroLegado::Entrada(format!(
            "Arquivo não encontrado: {}",
            arquivo.display()
        )));
    }
    let opcoes = SqliteConnectOptions::new()
        .filename(arquivo)
        .read_only(true)
        .create_if_missing(false);
    let sqlite = |e: sqlx::Error| ErroLegado::Sqlite(e.to_string());
    let mut conn = SqliteConnection::connect_with(&opcoes)
        .await
        .map_err(sqlite)?;
    let empresas = sqlx::query_as(
        "SELECT id, CAST(razao_social AS TEXT) AS razao_social, CAST(cnpj AS TEXT) AS cnpj
         FROM empresas ORDER BY id",
    )
    .fetch_all(&mut conn)
    .await
    .map_err(sqlite)?;
    let empenhos = sqlx::query_as(
        "SELECT id, empresa_id, CAST(ug AS TEXT) AS ug, CAST(gestao AS TEXT) AS gestao,
                CAST(empenho AS TEXT) AS empenho, CAST(modalidade AS TEXT) AS modalidade,
                CAST(numero_edital_ata AS TEXT) AS numero_edital_ata,
                CAST(valor_total AS REAL) AS valor_total
         FROM empenhos ORDER BY id",
    )
    .fetch_all(&mut conn)
    .await
    .map_err(sqlite)?;
    let notas = sqlx::query_as(
        "SELECT id, empenho_id, CAST(tipo_documento AS TEXT) AS tipo_documento,
                CAST(numero_nf AS TEXT) AS numero_nf, CAST(valor_nf AS REAL) AS valor_nf,
                CAST(status AS TEXT) AS status, CAST(data_emissao_nf AS TEXT) AS data_emissao_nf,
                CAST(prazo_dias_pagamento AS INTEGER) AS prazo_dias_pagamento,
                CAST(data_emissao_ob AS TEXT) AS data_emissao_ob
         FROM notas_fiscais ORDER BY id",
    )
    .fetch_all(&mut conn)
    .await
    .map_err(sqlite)?;
    let _ = conn.close().await;
    Ok(Origem {
        empresas,
        empenhos,
        notas,
    })
}

/// Importa o SQLite `arquivo` para a organização `org`. Com `empresa_cnpj`,
/// importa só a empresa com esse CNPJ (e seus empenhos e notas).
pub async fn importar_sqlite(
    pool: &PgPool,
    arquivo: &Path,
    org: Uuid,
    empresa_cnpj: Option<&str>,
) -> Result<Resumo, ErroLegado> {
    let filtro = match empresa_cnpj {
        Some(texto) => Some(normalizar_cnpj(texto).ok_or_else(|| {
            ErroLegado::Entrada("--empresa-cnpj inválido: informe 14 dígitos.".into())
        })?),
        None => None,
    };
    let origem = ler_origem(arquivo).await?;
    let existe: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM organizacoes WHERE id = $1 AND excluida_em IS NULL)",
    )
    .bind(org)
    .fetch_one(pool)
    .await?;
    if !existe {
        return Err(ErroLegado::Entrada(format!(
            "Organização {org} não encontrada."
        )));
    }
    let mut resumo = Resumo::default();
    let mut tx = db::tenant(pool, org).await?;
    let empresas = importar_empresas(&mut tx, org, &origem, filtro.as_deref(), &mut resumo).await?;
    if filtro.is_some() && empresas.is_empty() {
        return Err(ErroLegado::Entrada(
            "Nenhuma empresa com esse CNPJ no arquivo.".into(),
        ));
    }
    let empenhos = importar_empenhos(&mut tx, org, &origem, &empresas, &mut resumo).await?;
    importar_notas(&mut tx, org, &origem, &empenhos, &mut resumo).await?;
    tx.commit().await?;
    Ok(resumo)
}

/// Mapa id antigo → id novo das empresas importadas ou já existentes.
async fn importar_empresas(
    conn: &mut PgConnection,
    org: Uuid,
    origem: &Origem,
    filtro: Option<&str>,
    resumo: &mut Resumo,
) -> Result<HashMap<i64, Uuid>, ErroLegado> {
    let mut mapa = HashMap::new();
    for antiga in &origem.empresas {
        let Some(cnpj) = antiga.cnpj.as_deref().and_then(normalizar_cnpj) else {
            if filtro.is_none() {
                resumo
                    .erros
                    .push(format!("Empresa {}: CNPJ inválido.", antiga.id));
            }
            continue;
        };
        if filtro.is_some_and(|f| f != cnpj) {
            continue;
        }
        let existente: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM empresas WHERE organizacao_id = $1 AND cnpj = $2")
                .bind(org)
                .bind(&cnpj)
                .fetch_optional(&mut *conn)
                .await?;
        let id = if let Some(id) = existente {
            resumo.empresas.pulados += 1;
            id
        } else {
            let razao = texto_limpo(antiga.razao_social.as_deref(), 200)
                .unwrap_or_else(|| format!("Empresa {cnpj}"));
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO empresas (organizacao_id, razao_social, cnpj) VALUES ($1, $2, $3)
                 RETURNING id",
            )
            .bind(org)
            .bind(razao)
            .bind(&cnpj)
            .fetch_one(&mut *conn)
            .await?;
            resumo.empresas.importados += 1;
            id
        };
        mapa.insert(antiga.id, id);
    }
    Ok(mapa)
}

/// Prazo da primeira NF (menor id) de cada empenho antigo.
fn prazos_por_empenho(notas: &[NotaLegada]) -> HashMap<i64, i32> {
    let mut prazos = HashMap::new();
    for nota in notas {
        if let Some(empenho) = nota.empenho_id {
            prazos.entry(empenho).or_insert_with(|| {
                nota.prazo_dias_pagamento
                    .and_then(|p| i32::try_from(p).ok())
                    .filter(|p| (1..=PRAZO_MAXIMO).contains(p))
                    .unwrap_or(PRAZO_PADRAO)
            });
        }
    }
    prazos
}

async fn importar_empenhos(
    conn: &mut PgConnection,
    org: Uuid,
    origem: &Origem,
    empresas: &HashMap<i64, Uuid>,
    resumo: &mut Resumo,
) -> Result<HashMap<i64, EmpenhoDestino>, ErroLegado> {
    let prazos = prazos_por_empenho(&origem.notas);
    let mut mapa = HashMap::new();
    for antigo in &origem.empenhos {
        let Some(empresa_id) = antigo.empresa_id.and_then(|e| empresas.get(&e).copied()) else {
            continue;
        };
        let rotulo = format!("Empenho {}", antigo.id);
        let ug = antigo.ug.as_deref().unwrap_or_default().trim().to_owned();
        if ug.len() != 6 || !ug.bytes().all(|b| b.is_ascii_digit()) {
            resumo.erros.push(format!("{rotulo}: UG inválida."));
            continue;
        }
        let Some(numero) =
            texto_limpo(antigo.empenho.as_deref(), TAMANHO_NUMERO).map(|n| n.to_uppercase())
        else {
            resumo.erros.push(format!("{rotulo}: número ausente."));
            continue;
        };
        let Some(valor) = antigo.valor_total.and_then(reais_para_centavos) else {
            resumo
                .erros
                .push(format!("{rotulo}: valor total inválido."));
            continue;
        };
        let prazo = prazos.get(&antigo.id).copied().unwrap_or(PRAZO_PADRAO);
        let gestao = gestao_valida(antigo.gestao.as_deref(), &rotulo, &mut resumo.erros);
        let modalidade = texto_cortado(
            antigo.modalidade.as_deref(),
            TAMANHO_CURTO,
            &rotulo,
            "modalidade cortada",
            &mut resumo.erros,
        );
        let edital = texto_cortado(
            antigo.numero_edital_ata.as_deref(),
            TAMANHO_CURTO,
            &rotulo,
            "edital/ata cortado",
            &mut resumo.erros,
        );
        let novo: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO empenhos
                 (organizacao_id, empresa_id, ug, gestao, numero, modalidade, numero_edital_ata,
                  valor_total_centavos, regra_vencimento, prazo_dias, origem, observacao)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'corridos_emissao', $9, 'legado', $10)
             ON CONFLICT (organizacao_id, ug, numero) DO NOTHING
             RETURNING id",
        )
        .bind(org)
        .bind(empresa_id)
        .bind(&ug)
        .bind(gestao)
        .bind(&numero)
        .bind(modalidade)
        .bind(edital)
        .bind(valor)
        .bind(prazo)
        .bind(OBSERVACAO_LEGADO)
        .fetch_optional(&mut *conn)
        .await?;
        let destino = if let Some(id) = novo {
            resumo.empenhos.importados += 1;
            EmpenhoDestino {
                id,
                regra: RegraVencimento::CorridosEmissao,
                prazo,
                valor,
            }
        } else {
            resumo.empenhos.pulados += 1;
            let (id, regra, prazo, valor): (Uuid, String, i32, i64) = sqlx::query_as(
                "SELECT id, regra_vencimento, prazo_dias, valor_total_centavos FROM empenhos
                 WHERE organizacao_id = $1 AND ug = $2 AND numero = $3",
            )
            .bind(org)
            .bind(&ug)
            .bind(&numero)
            .fetch_one(&mut *conn)
            .await?;
            EmpenhoDestino {
                id,
                regra: regra.parse().unwrap_or_default(),
                prazo,
                valor,
            }
        };
        mapa.insert(antigo.id, destino);
    }
    Ok(mapa)
}

async fn importar_notas(
    conn: &mut PgConnection,
    org: Uuid,
    origem: &Origem,
    empenhos: &HashMap<i64, EmpenhoDestino>,
    resumo: &mut Resumo,
) -> Result<(), ErroLegado> {
    // Faturado (notas não canceladas) por empenho de destino, carregado do banco
    // na primeira nota de cada empenho (reimportação) e somado a cada inserção.
    let mut faturados: HashMap<Uuid, i64> = HashMap::new();
    for antiga in &origem.notas {
        let Some(empenho) = antiga.empenho_id.and_then(|e| empenhos.get(&e).copied()) else {
            continue;
        };
        let rotulo = format!("NF {}", antiga.id);
        let Some(numero) = texto_limpo(antiga.numero_nf.as_deref(), 40) else {
            resumo.erros.push(format!("{rotulo}: número ausente."));
            continue;
        };
        let Some(valor) = antiga.valor_nf.and_then(reais_para_centavos) else {
            resumo.erros.push(format!("{rotulo}: valor inválido."));
            continue;
        };
        let Some(emissao) = antiga.data_emissao_nf.as_deref().and_then(ler_data) else {
            resumo
                .erros
                .push(format!("{rotulo}: data de emissão inválida."));
            continue;
        };
        let Some(status) = status_nota(antiga.status.as_deref()) else {
            resumo.erros.push(format!("{rotulo}: status desconhecido."));
            continue;
        };
        let prazo = u32::try_from(empenho.prazo).unwrap_or(30).max(1);
        let Ok(vencimento) =
            calcular_vencimento(empenho.regra, emissao, None, prazo, empenho.valor)
        else {
            resumo
                .erros
                .push(format!("{rotulo}: vencimento fora do intervalo."));
            continue;
        };
        let tipo = tipo_nota(antiga.tipo_documento.as_deref().unwrap_or_default());
        let duplicada: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM notas
                            WHERE organizacao_id = $1 AND empenho_id = $2 AND tipo = $3 AND numero = $4)",
        )
        .bind(org)
        .bind(empenho.id)
        .bind(tipo)
        .bind(&numero)
        .fetch_one(&mut *conn)
        .await?;
        if duplicada {
            resumo.notas.pulados += 1;
            continue;
        }
        let faturado = match faturados.get(&empenho.id) {
            Some(valor) => *valor,
            None => {
                let valor: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(SUM(valor_centavos), 0)::bigint FROM notas
                     WHERE organizacao_id = $1 AND empenho_id = $2 AND status <> 'cancelada'",
                )
                .bind(org)
                .bind(empenho.id)
                .fetch_one(&mut *conn)
                .await?;
                faturados.insert(empenho.id, valor);
                valor
            }
        };
        if status != "cancelada" && faturado.saturating_add(valor) > empenho.valor {
            resumo.erros.push(format!(
                "{rotulo}: valor excede o saldo do empenho (saldo R$ {}); não importada.",
                reais((empenho.valor - faturado).max(0))
            ));
            continue;
        }
        let pagamento = (status == "paga").then(|| {
            antiga
                .data_emissao_ob
                .as_deref()
                .and_then(ler_data)
                .unwrap_or(emissao)
        });
        let nota: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO notas
                 (organizacao_id, empenho_id, numero, tipo, valor_centavos, data_emissao, vencimento,
                  status, valor_pago_centavos, data_liquidacao, data_pagamento)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
             ON CONFLICT (empenho_id, tipo, numero) DO NOTHING
             RETURNING id",
        )
        .bind(org)
        .bind(empenho.id)
        .bind(&numero)
        .bind(tipo)
        .bind(valor)
        .bind(emissao)
        .bind(vencimento)
        .bind(status)
        .bind(if pagamento.is_some() { valor } else { 0 })
        .bind(pagamento)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(nota) = nota else {
            resumo.notas.pulados += 1;
            continue;
        };
        resumo.notas.importados += 1;
        if status != "cancelada" {
            faturados.insert(empenho.id, faturado.saturating_add(valor));
        }
        if status != "emitida" {
            sqlx::query(
                "INSERT INTO eventos_nota
                     (organizacao_id, nota_id, tipo, data, valor_centavos, observacao, origem)
                 VALUES ($1, $2, $3, $4, $5, $6, 'manual')",
            )
            .bind(org)
            .bind(nota)
            .bind(status)
            .bind(pagamento.unwrap_or(emissao))
            .bind(pagamento.map(|_| valor))
            .bind(OBSERVACAO_LEGADO)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn converte_reais_com_arredondamento() {
        assert_eq!(reais_para_centavos(32_450.0), Some(3_245_000));
        assert_eq!(reais_para_centavos(0.29), Some(29));
        assert_eq!(reais_para_centavos(1.005), Some(100));
        assert_eq!(reais_para_centavos(1.015), Some(101));
        assert_eq!(reais_para_centavos(36_657.299_999_999_996), Some(3_665_730));
        assert_eq!(reais_para_centavos(0.0), None);
        assert_eq!(reais_para_centavos(-5.0), None);
        assert_eq!(reais_para_centavos(f64::NAN), None);
        assert_eq!(reais_para_centavos(1e13), None);
    }

    #[test]
    fn mapeia_tipos_status_e_datas() {
        assert_eq!(tipo_nota("NFS-e (Serviços)"), "servico");
        assert_eq!(tipo_nota("Nota Fiscal (Produtos)"), "produto");
        assert_eq!(status_nota(Some("PENDENTE")), Some("emitida"));
        assert_eq!(status_nota(None), Some("emitida"));
        assert_eq!(status_nota(Some("pago")), Some("paga"));
        assert_eq!(status_nota(Some("OUTRO")), None);
        assert_eq!(ler_data("2026-01-15"), NaiveDate::from_ymd_opt(2026, 1, 15));
        assert_eq!(ler_data("15/01/2026"), NaiveDate::from_ymd_opt(2026, 1, 15));
        assert_eq!(
            ler_data("2026-01-15 10:00:00"),
            NaiveDate::from_ymd_opt(2026, 1, 15)
        );
        assert_eq!(ler_data("ontem"), None);
    }
}
