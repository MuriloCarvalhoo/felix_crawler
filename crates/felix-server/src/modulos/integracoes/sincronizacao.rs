//! Job `transparencia_sync`: baixa empenhos, liquidações e pagamentos de cada
//! CNPJ da organização no Portal da Transparência e grava em
//! `documentos_transparencia` (único por organização + código).
//!
//! - Cada empresa/fase é gravada na sua própria transação: uma falha no meio
//!   preserva o que já foi obtido e deixa o status `parcial` com o erro.
//! - Orçamento de [`MAX_REQUISICOES`] requisições por sincronização; ao
//!   atingi-lo (ou se o Portal devolver mais páginas que o limite do cliente),
//!   o status fica `parcial` com aviso em `ultimo_erro`.
//! - Liquidações/pagamentos têm os empenhos impactados consultados uma única
//!   vez (`impactados_consultados`), mesmo quando nenhum empenho é identificado.

use crate::{
    app::AppState,
    config::Config,
    db,
    http::ErroApi,
    jobs,
    modulos::assinatura::{limites_do_plano, plano_efetivo},
};
use felix_transparencia::{
    Cliente, Config as ConfigTransparencia, Documento, ErroTransparencia, Fase, numero_empenho,
};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    sync::{LazyLock, Mutex, PoisonError},
    time::Duration,
};
use uuid::Uuid;

pub const TIPO_INTEGRACAO: &str = "transparencia";
pub const MSG_NAO_CONFIGURADA: &str = "Integração não configurada no servidor.";
pub const MSG_PLANO: &str = "O plano atual não inclui a integração com o Portal da Transparência.";
/// Requisições ao Portal por sincronização. Acima disso a sincronização para,
/// grava o que obteve e fica `parcial` (o restante vem na próxima).
pub const MAX_REQUISICOES: u32 = 600;

static CLIENTES: LazyLock<Mutex<HashMap<(String, String), Cliente>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Servidor local (testes, proxy de desenvolvimento): sem o limite de 90 req/min,
/// que existe para proteger a API oficial.
fn url_local(url: &str) -> bool {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|local| host == *local || host.starts_with(&format!("{local}:")))
}

/// Cliente do Portal montado a partir da configuração, criado uma vez por
/// (URL, chave) e reutilizado (o limitador de requisições é compartilhado entre
/// clones). `Ok(None)` sem chave configurada.
pub fn cliente(config: &Config) -> Result<Option<Cliente>, ErroTransparencia> {
    let Some(chave) = &config.transparencia_key else {
        return Ok(None);
    };
    let id = (config.transparencia_url.clone(), chave.clone());
    let mut mapa = CLIENTES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existente) = mapa.get(&id) {
        return Ok(Some(existente.clone()));
    }
    let mut cfg = ConfigTransparencia::nova(chave.clone());
    cfg.base_url.clone_from(&config.transparencia_url);
    if url_local(&config.transparencia_url) {
        cfg.intervalo_minimo = Duration::ZERO;
    }
    let novo = Cliente::novo(cfg)?;
    mapa.insert(id, novo.clone());
    Ok(Some(novo))
}

/// Documento baixado, pronto para gravar.
struct Baixado {
    empresa_id: Uuid,
    codigo: String,
    doc: Documento,
    empenho_codigo: Option<String>,
    /// Empenhos impactados já consultados (fases 2/3), mesmo sem resultado.
    consultado: bool,
}

/// Resultado de uma sincronização concluída (`ok` ou `parcial`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumoSync {
    pub documentos: usize,
    pub novos: u64,
    pub requisicoes: u32,
    /// Algo ficou de fora (orçamento de requisições ou limite de páginas).
    pub parcial: bool,
    pub avisos: Vec<String>,
}

/// Falha da sincronização; `permanente` = não adianta repetir (sem chave no
/// servidor, plano sem a integração, chave recusada pelo Portal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FalhaSync {
    pub mensagem: String,
    pub permanente: bool,
}

impl FalhaSync {
    fn permanente(mensagem: impl Into<String>) -> Self {
        Self {
            mensagem: mensagem.into(),
            permanente: true,
        }
    }
}

impl From<ErroTransparencia> for FalhaSync {
    fn from(erro: ErroTransparencia) -> Self {
        let permanente = matches!(
            erro,
            ErroTransparencia::ChaveInvalida | ErroTransparencia::Configuracao(_)
        );
        Self {
            mensagem: erro.to_string(),
            permanente,
        }
    }
}

/// 4xx do servidor (ex.: 402 do plano) é permanente; o resto (banco), temporário.
impl From<ErroApi> for FalhaSync {
    fn from(erro: ErroApi) -> Self {
        let status = erro.status();
        Self {
            mensagem: erro.mensagem().to_owned(),
            permanente: status.is_client_error() && status.as_u16() != 429,
        }
    }
}

/// Executa a sincronização da organização para o `ano` e registra o resultado
/// em `integracoes`: `ok`, `parcial` (aviso ou erro em `ultimo_erro`) ou `erro`.
pub async fn sincronizar(estado: &AppState, org: Uuid, ano: i32) -> Result<ResumoSync, FalhaSync> {
    sincronizar_com_orcamento(estado, org, ano, MAX_REQUISICOES).await
}

/// [`sincronizar`] com um orçamento de requisições explícito.
pub async fn sincronizar_com_orcamento(
    estado: &AppState,
    org: Uuid,
    ano: i32,
    orcamento: u32,
) -> Result<ResumoSync, FalhaSync> {
    let mut progresso = Progresso::novo(orcamento);
    let resultado = executar(estado, org, ano, &mut progresso).await;
    let (status, mensagem) = match &resultado {
        Ok(()) if progresso.parcial => ("parcial", Some(progresso.avisos.join(" "))),
        Ok(()) => ("ok", None),
        Err(falha) => {
            tracing::warn!(org = %org, ano, erro = %falha.mensagem, "Sincronização com o Portal falhou");
            let status = if progresso.etapas_gravadas > 0 {
                "parcial"
            } else {
                "erro"
            };
            (status, Some(falha.mensagem.clone()))
        }
    };
    if let Err(erro) = registrar_status(estado, org, status, mensagem.as_deref()).await {
        tracing::error!(org = %org, erro = %erro, "Falha ao registrar o status da sincronização");
    }
    resultado?;
    tracing::info!(
        org = %org,
        ano,
        documentos = progresso.documentos,
        novos = progresso.novos,
        requisicoes = progresso.requisicoes,
        status,
        "Sincronização concluída"
    );
    Ok(ResumoSync {
        documentos: progresso.documentos,
        novos: progresso.novos,
        requisicoes: progresso.requisicoes,
        parcial: progresso.parcial,
        avisos: progresso.avisos,
    })
}

/// Contagem e avisos acumulados durante a sincronização.
struct Progresso {
    orcamento: u32,
    requisicoes: u32,
    documentos: usize,
    novos: u64,
    etapas_gravadas: u32,
    parcial: bool,
    avisos: Vec<String>,
}

impl Progresso {
    fn novo(orcamento: u32) -> Self {
        Self {
            orcamento,
            requisicoes: 0,
            documentos: 0,
            novos: 0,
            etapas_gravadas: 0,
            parcial: false,
            avisos: Vec::new(),
        }
    }

    fn restante(&self) -> u32 {
        self.orcamento.saturating_sub(self.requisicoes)
    }

    fn contar(&mut self, requisicoes: u32) {
        self.requisicoes = self.requisicoes.saturating_add(requisicoes);
    }

    fn avisar(&mut self, aviso: String) {
        self.parcial = true;
        if !self.avisos.contains(&aviso) {
            self.avisos.push(aviso);
        }
    }

    fn avisar_orcamento(&mut self) {
        let aviso = format!(
            "Limite de {} requisições por sincronização atingido; o restante será buscado na próxima sincronização.",
            self.orcamento
        );
        self.avisar(aviso);
    }
}

async fn executar(
    estado: &AppState,
    org: Uuid,
    ano: i32,
    progresso: &mut Progresso,
) -> Result<(), FalhaSync> {
    let cliente =
        cliente(&estado.config)?.ok_or_else(|| FalhaSync::permanente(MSG_NAO_CONFIGURADA))?;
    let (empresas, conhecidos) = carregar_contexto(estado, org).await?;
    for (empresa_id, cnpj) in &empresas {
        for fase in Fase::TODAS {
            if progresso.restante() == 0 {
                progresso.avisar_orcamento();
                return Ok(());
            }
            let etapa = Etapa {
                estado,
                cliente: &cliente,
                org,
                ano,
                empresa_id: *empresa_id,
                cnpj,
                fase,
                conhecidos: &conhecidos,
            };
            etapa.executar(progresso).await?;
        }
    }
    Ok(())
}

/// Uma empresa numa fase: baixa, resolve os empenhos e grava numa transação.
struct Etapa<'a> {
    estado: &'a AppState,
    cliente: &'a Cliente,
    org: Uuid,
    ano: i32,
    empresa_id: Uuid,
    cnpj: &'a str,
    fase: Fase,
    conhecidos: &'a HashSet<String>,
}

impl Etapa<'_> {
    async fn executar(&self, progresso: &mut Progresso) -> Result<(), FalhaSync> {
        let docs = self
            .cliente
            .documentos_por_favorecido_ate(self.cnpj, self.fase, self.ano, progresso.restante())
            .await?;
        progresso.contar(docs.requisicoes);
        if docs.truncado {
            if progresso.restante() == 0 {
                progresso.avisar_orcamento();
            } else {
                progresso.avisar(format!(
                    "O Portal devolveu mais páginas que o limite para o CNPJ {} ({}); \
                     parte dos documentos pode ter ficado de fora.",
                    self.cnpj,
                    self.fase.nome()
                ));
            }
        }
        let mut baixados = Vec::with_capacity(docs.len());
        let mut falha = None;
        for doc in docs {
            let codigo = if doc.codigo.is_empty() {
                doc.codigo_resumido.clone()
            } else {
                doc.codigo.clone()
            };
            if codigo.is_empty() {
                continue;
            }
            let (empenho_codigo, consultado) = if doc.fase == Fase::Empenho {
                (doc.numero_empenho(), false)
            } else if self.conhecidos.contains(&codigo) {
                (None, true)
            } else if falha.is_some() {
                (None, false)
            } else if progresso.restante() == 0 {
                progresso.avisar_orcamento();
                (None, false)
            } else {
                match empenho_de_execucao(self.cliente, &doc, progresso).await {
                    Ok(numero) => (numero, true),
                    Err(erro) => {
                        falha = Some(erro);
                        (None, false)
                    }
                }
            };
            baixados.push(Baixado {
                empresa_id: self.empresa_id,
                codigo,
                doc,
                empenho_codigo,
                consultado,
            });
        }
        if !baixados.is_empty() {
            progresso.novos += gravar(self.estado, self.org, &baixados).await?;
            progresso.documentos += baixados.len();
            progresso.etapas_gravadas += 1;
        }
        match falha {
            Some(erro) => Err(erro),
            None => Ok(()),
        }
    }
}

/// Número do empenho de uma liquidação/pagamento: primeiro empenho impactado;
/// sem ele, tenta a observação do documento.
async fn empenho_de_execucao(
    cliente: &Cliente,
    doc: &Documento,
    progresso: &mut Progresso,
) -> Result<Option<String>, FalhaSync> {
    let impactados = match cliente.empenhos_impactados(&doc.codigo, doc.fase).await {
        Ok(lista) => {
            progresso.contar(lista.requisicoes);
            lista.itens
        }
        // Código vazio/estranho: não dá para consultar; segue sem o vínculo.
        Err(ErroTransparencia::Configuracao(_)) => Vec::new(),
        Err(erro) => return Err(erro.into()),
    };
    Ok(impactados
        .first()
        .and_then(|codigo| numero_empenho(codigo))
        .or_else(|| numero_empenho(&doc.observacao)))
}

/// Empresas (id, CNPJ) e códigos de liquidações/pagamentos cujos empenhos já
/// foram identificados ou consultados (não precisam de nova consulta).
async fn carregar_contexto(
    estado: &AppState,
    org: Uuid,
) -> Result<(Vec<(Uuid, String)>, HashSet<String>), ErroApi> {
    let mut tx = db::tenant(&estado.pool, org).await?;
    let plano = plano_efetivo(&mut tx, org, estado.agora()).await?;
    if !limites_do_plano(plano).transparencia {
        return Err(ErroApi::limite_plano(MSG_PLANO));
    }
    let empresas: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, cnpj::text FROM empresas WHERE organizacao_id = $1 ORDER BY criado_em, id",
    )
    .bind(org)
    .fetch_all(&mut *tx)
    .await?;
    let conhecidos: Vec<String> = sqlx::query_scalar(
        "SELECT codigo FROM documentos_transparencia
         WHERE organizacao_id = $1 AND (empenho_codigo IS NOT NULL OR impactados_consultados)",
    )
    .bind(org)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((empresas, conhecidos.into_iter().collect()))
}

/// Upsert dos documentos de uma etapa, numa transação. Situação e vínculos
/// existentes são preservados. Devolve quantos documentos são novos.
async fn gravar(estado: &AppState, org: Uuid, baixados: &[Baixado]) -> Result<u64, ErroApi> {
    let mut tx = db::tenant(&estado.pool, org).await?;
    let mut novos = 0;
    for b in baixados {
        let d = &b.doc;
        let novo: bool = sqlx::query_scalar(
            "INSERT INTO documentos_transparencia
                 (organizacao_id, empresa_id, codigo, codigo_resumido, fase, data, valor_centavos,
                  ug, ug_nome, orgao, especie, observacao, empenho_codigo, bruto,
                  impactados_consultados)
             VALUES ($1, $2, $3, NULLIF($4, ''), $5, $6, $7, NULLIF($8, ''), NULLIF($9, ''),
                     NULLIF($10, ''), NULLIF($11, ''), NULLIF($12, ''), $13, $14, $15)
             ON CONFLICT (organizacao_id, codigo) DO UPDATE SET
                 codigo_resumido = EXCLUDED.codigo_resumido,
                 data = EXCLUDED.data,
                 valor_centavos = EXCLUDED.valor_centavos,
                 ug = EXCLUDED.ug,
                 ug_nome = EXCLUDED.ug_nome,
                 orgao = EXCLUDED.orgao,
                 especie = EXCLUDED.especie,
                 observacao = EXCLUDED.observacao,
                 empenho_codigo = COALESCE(EXCLUDED.empenho_codigo,
                                           documentos_transparencia.empenho_codigo),
                 bruto = EXCLUDED.bruto,
                 impactados_consultados = documentos_transparencia.impactados_consultados
                                          OR EXCLUDED.impactados_consultados
             RETURNING (xmax = 0)",
        )
        .bind(org)
        .bind(b.empresa_id)
        .bind(&b.codigo)
        .bind(&d.codigo_resumido)
        .bind(i16::from(d.fase))
        .bind(d.data)
        .bind(d.valor_centavos)
        .bind(&d.ug_codigo)
        .bind(&d.ug_nome)
        .bind(&d.orgao)
        .bind(&d.especie)
        .bind(&d.observacao)
        .bind(&b.empenho_codigo)
        .bind(&d.bruto)
        .bind(b.consultado)
        .fetch_one(&mut *tx)
        .await?;
        if novo {
            novos += 1;
        }
    }
    tx.commit().await?;
    Ok(novos)
}

async fn registrar_status(
    estado: &AppState,
    org: Uuid,
    status: &str,
    mensagem: Option<&str>,
) -> Result<(), ErroApi> {
    let mensagem: Option<String> = mensagem.map(|e| e.chars().take(500).collect());
    let mut tx = db::tenant(&estado.pool, org).await?;
    sqlx::query(
        "INSERT INTO integracoes (organizacao_id, tipo, ultima_sincronizacao_em, ultimo_status, ultimo_erro)
         VALUES ($1, $2, now(), $3, $4)
         ON CONFLICT (organizacao_id, tipo) DO UPDATE SET
             ultima_sincronizacao_em = now(), ultimo_status = EXCLUDED.ultimo_status,
             ultimo_erro = EXCLUDED.ultimo_erro",
    )
    .bind(org)
    .bind(TIPO_INTEGRACAO)
    .bind(status)
    .bind(mensagem)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Ano padrão da sincronização: o ano de hoje (BRT).
pub fn ano_padrao(estado: &AppState) -> i32 {
    chrono::Datelike::year(&estado.hoje())
}

/// Enfileira a sincronização da organização. `Ok(None)` se já existe uma
/// pendente/executando (índice único parcial em `jobs`).
pub async fn enfileirar(estado: &AppState, org: Uuid, ano: i32) -> Result<Option<Uuid>, ErroApi> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO jobs (tipo, organizacao_id, payload) VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind(jobs::TIPO_TRANSPARENCIA)
    .bind(org)
    .bind(json!({ "org_id": org, "ano": ano }))
    .fetch_optional(&estado.pool)
    .await?;
    Ok(id)
}

/// Rodada diária: enfileira a sincronização das organizações com a integração
/// ativa (se o servidor tem chave e o plano permite). Falha de uma organização
/// é registrada no log e não impede as demais. Devolve quantas enfileirou.
pub async fn agendar_sincronizacoes_ativas(estado: &AppState) -> Result<u64, ErroApi> {
    if estado.config.transparencia_key.is_none() {
        return Ok(0);
    }
    let orgs: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM organizacoes WHERE excluida_em IS NULL ORDER BY id")
            .fetch_all(&estado.pool)
            .await?;
    let ano = ano_padrao(estado);
    let mut enfileiradas = 0;
    for org in orgs {
        match agendar_da_org(estado, org, ano).await {
            Ok(true) => enfileiradas += 1,
            Ok(false) => {}
            Err(erro) => {
                tracing::error!(org = %org, erro = %erro, "Falha ao agendar a sincronização da organização");
            }
        }
    }
    Ok(enfileiradas)
}

async fn agendar_da_org(estado: &AppState, org: Uuid, ano: i32) -> Result<bool, ErroApi> {
    let mut tx = db::tenant(&estado.pool, org).await?;
    let ativa: Option<bool> =
        sqlx::query_scalar("SELECT ativa FROM integracoes WHERE organizacao_id = $1 AND tipo = $2")
            .bind(org)
            .bind(TIPO_INTEGRACAO)
            .fetch_optional(&mut *tx)
            .await?;
    let plano = plano_efetivo(&mut tx, org, estado.agora()).await?;
    tx.commit().await?;
    Ok(ativa == Some(true)
        && limites_do_plano(plano).transparencia
        && enfileirar(estado, org, ano).await?.is_some())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn reconhece_servidor_local() {
        assert!(url_local("http://127.0.0.1:4321"));
        assert!(url_local("http://localhost"));
        assert!(!url_local("https://api.portaldatransparencia.gov.br"));
        assert!(!url_local("http://127.0.0.1.exemplo.com"));
    }

    #[test]
    fn classifica_falhas() {
        assert!(FalhaSync::from(ErroTransparencia::ChaveInvalida).permanente);
        assert!(!FalhaSync::from(ErroTransparencia::LimiteExcedido).permanente);
        assert!(!FalhaSync::from(ErroTransparencia::Indisponivel("x".into())).permanente);
        assert!(FalhaSync::from(ErroApi::limite_plano("x")).permanente);
        assert!(!FalhaSync::from(ErroApi::interno()).permanente);
    }
}
