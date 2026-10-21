//! Worker de jobs em processo (ADR-011) e agendador.
//!
//! - Fila na tabela `jobs`, retirada com `FOR UPDATE SKIP LOCKED` (vários
//!   executores nunca pegam o mesmo job).
//! - Dois executores concorrentes; cada job tem até [`MAX_TENTATIVAS`] tentativas
//!   com espera exponencial ([`espera_retentativa`]).
//! - Jobs presos em `executando` há mais de 15 minutos (processo morto) voltam
//!   para a fila; enquanto um job roda, um "batimento" atualiza `atualizado_em`.
//! - Agendador: a cada minuto garante o job `alertas_diarios` de hoje às 06:00
//!   (BRT) e, com SMTP configurado, um `envio_email` quando há e-mails prontos
//!   para (re)envio.
//! - Pânico dentro de um job vira falha permanente ([`proteger`]); o executor
//!   continua vivo ([`executores_vivos`]).
//! - Erros 4xx (exceto 429) são permanentes; 5xx/429/rede, temporários.
//! - Jobs de organização excluída são concluídos sem executar.
//!
//! Tipos: [`TIPO_TRANSPARENCIA`] `{org_id, ano}`, [`TIPO_ALERTAS`] `{data}`
//! (rodada diária: alertas, sincronizações ativas e [`limpeza`]), [`TIPO_EMAIL`] `{}`.

pub mod envio_email;
pub mod limpeza;

use crate::{
    app::AppState,
    http::ErroApi,
    modulos::{integracoes, notificacoes},
};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use futures_util::FutureExt;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use std::{
    panic::AssertUnwindSafe,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::{sync::watch, task::JoinHandle};
use uuid::Uuid;

pub const TIPO_TRANSPARENCIA: &str = "transparencia_sync";
pub const TIPO_ALERTAS: &str = "alertas_diarios";
pub const TIPO_EMAIL: &str = "envio_email";

/// Executores simultâneos.
pub const CONCORRENCIA: usize = 2;
/// Tentativas por job (a primeira conta).
pub const MAX_TENTATIVAS: i32 = 3;
/// Job em `executando` sem batimento há mais que isso é considerado preso.
pub const PRESO_APOS_MINUTOS: i32 = 15;
/// Hora local (BRT) do job diário de alertas.
pub const HORA_ALERTAS_BRT: u32 = 6;

const ESPERA_OCIOSA: Duration = Duration::from_secs(5);
const ESPERA_ERRO: Duration = Duration::from_secs(10);
const INTERVALO_BATIMENTO: Duration = Duration::from_secs(60);
const INTERVALO_EMAIL: Duration = Duration::from_secs(15);
const INTERVALO_AGENDADOR: Duration = Duration::from_secs(60);
const PRAZO_ENCERRAMENTO: Duration = Duration::from_secs(30);
const ERRO_MAX: usize = 500;
/// Chave de `pg_advisory_xact_lock` do agendamento diário.
const TRAVA_AGENDADOR: i64 = 7_262_101;

/// Executores em execução neste processo (para `/ready`).
static EXECUTORES_VIVOS: AtomicUsize = AtomicUsize::new(0);

/// Quantos executores de jobs estão vivos neste processo (0 antes de
/// [`iniciar`] e depois de [`Trabalhador::encerrar`]).
pub fn executores_vivos() -> usize {
    EXECUTORES_VIVOS.load(Ordering::SeqCst)
}

/// Desconta o executor ao sair do laço (inclusive por pânico ou abort).
struct ExecutorVivo;

impl ExecutorVivo {
    fn registrar() -> Self {
        EXECUTORES_VIVOS.fetch_add(1, Ordering::SeqCst);
        ExecutorVivo
    }
}

impl Drop for ExecutorVivo {
    fn drop(&mut self) {
        EXECUTORES_VIVOS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Job retirado da fila.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: Uuid,
    pub tipo: String,
    pub organizacao_id: Option<Uuid>,
    pub payload: Value,
    /// Tentativas já iniciadas, incluindo a atual.
    pub tentativas: i32,
}

/// Resultado de falha de um job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Falha {
    /// Pode dar certo depois (rede, limite, banco): reagenda até o máximo de tentativas.
    Temporaria(String),
    /// Não adianta repetir (tipo desconhecido, payload inválido).
    Permanente(String),
}

impl Falha {
    fn mensagem(&self) -> &str {
        match self {
            Falha::Temporaria(m) | Falha::Permanente(m) => m,
        }
    }
}

/// 4xx (validação, conflito, plano, não encontrado…) não muda com o tempo:
/// permanente. 429, 5xx e indisponibilidade: temporária.
impl From<ErroApi> for Falha {
    fn from(erro: ErroApi) -> Self {
        let status = erro.status();
        let mensagem = erro.mensagem().to_owned();
        if status.is_client_error() && status != StatusCode::TOO_MANY_REQUESTS {
            Falha::Permanente(mensagem)
        } else {
            Falha::Temporaria(mensagem)
        }
    }
}

/// Executa o trabalho de um job capturando pânicos: um pânico vira
/// [`Falha::Permanente`] (com log), e o executor segue vivo.
pub async fn proteger<F: Future<Output = Result<(), Falha>>>(trabalho: F) -> Result<(), Falha> {
    match AssertUnwindSafe(trabalho).catch_unwind().await {
        Ok(resultado) => resultado,
        Err(panico) => {
            let detalhe = panico
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panico.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            tracing::error!(detalhe = %detalhe, "Pânico durante a execução de um job");
            Err(Falha::Permanente(
                "Erro interno inesperado durante a execução do job.".to_owned(),
            ))
        }
    }
}

/// Tarefas de fundo em execução. [`Trabalhador::encerrar`] pede parada e
/// aguarda os jobs em andamento (até 30 s).
pub struct Trabalhador {
    parar: watch::Sender<bool>,
    tarefas: Vec<JoinHandle<()>>,
}

impl Trabalhador {
    /// Encerramento gracioso: nenhum job novo é retirado; os em andamento
    /// terminam ou, após o prazo, são abortados (e recuperados depois como presos).
    pub async fn encerrar(self) {
        let _ = self.parar.send(true);
        for tarefa in self.tarefas {
            let abortar = tarefa.abort_handle();
            if tokio::time::timeout(PRAZO_ENCERRAMENTO, tarefa)
                .await
                .is_err()
            {
                tracing::warn!("Tarefa de fundo não terminou no prazo; abortando");
                abortar.abort();
            }
        }
        tracing::info!("Worker de jobs encerrado");
    }
}

/// Inicia os executores e o agendador. Chamado pelo `main` depois das migrações.
pub fn iniciar(estado: AppState) -> Trabalhador {
    let (parar, receptor) = watch::channel(false);
    let mut tarefas = Vec::with_capacity(CONCORRENCIA + 1);
    for indice in 0..CONCORRENCIA {
        tarefas.push(tokio::spawn(laco_executor(
            estado.clone(),
            receptor.clone(),
            indice,
        )));
    }
    tarefas.push(tokio::spawn(laco_agendador(estado, receptor)));
    tracing::info!(executores = CONCORRENCIA, "Worker de jobs iniciado");
    Trabalhador { parar, tarefas }
}

/// Espera até `duracao` ou até o pedido de parada. Devolve `true` se deve parar.
async fn aguardar(parar: &mut watch::Receiver<bool>, duracao: Duration) -> bool {
    if *parar.borrow() {
        return true;
    }
    tokio::select! {
        mudou = parar.changed() => mudou.is_err() || *parar.borrow(),
        () = tokio::time::sleep(duracao) => false,
    }
}

async fn laco_executor(estado: AppState, mut parar: watch::Receiver<bool>, indice: usize) {
    let _vivo = ExecutorVivo::registrar();
    loop {
        if *parar.borrow() {
            break;
        }
        let espera = match executar_um(&estado).await {
            Ok(true) => continue,
            Ok(false) => ESPERA_OCIOSA,
            Err(erro) => {
                tracing::warn!(executor = indice, erro = %erro, "Falha ao retirar job da fila");
                ESPERA_ERRO
            }
        };
        if aguardar(&mut parar, espera).await {
            break;
        }
    }
}

async fn laco_agendador(estado: AppState, mut parar: watch::Receiver<bool>) {
    let mut ultimo_diario: Option<tokio::time::Instant> = None;
    loop {
        let agora = tokio::time::Instant::now();
        if ultimo_diario.is_none_or(|t| agora.duration_since(t) >= INTERVALO_AGENDADOR) {
            ultimo_diario = Some(agora);
            if let Err(erro) = agendar_alertas(&estado).await {
                tracing::warn!(erro = %erro, "Falha ao agendar alertas diários");
            }
            match recuperar_presos(&estado.pool).await {
                Ok(0) => {}
                Ok(n) => tracing::warn!(jobs = n, "Jobs presos recolocados na fila"),
                Err(erro) => tracing::warn!(erro = %erro, "Falha ao recuperar jobs presos"),
            }
        }
        if let Err(erro) = agendar_envio_email(&estado).await {
            tracing::warn!(erro = %erro, "Falha ao agendar envio de e-mails");
        }
        if aguardar(&mut parar, INTERVALO_EMAIL).await {
            break;
        }
    }
}

/// Coloca um job na fila para execução imediata.
pub async fn enfileirar(
    pool: &PgPool,
    tipo: &str,
    org_id: Option<Uuid>,
    payload: Value,
) -> Result<Uuid, ErroApi> {
    let mut conn = pool.acquire().await?;
    enfileirar_em(&mut conn, tipo, org_id, payload, None).await
}

/// Coloca um job na fila numa conexão/transação existente; `executar_em`
/// ausente = agora.
pub async fn enfileirar_em(
    conn: &mut PgConnection,
    tipo: &str,
    org_id: Option<Uuid>,
    payload: Value,
    executar_em: Option<DateTime<Utc>>,
) -> Result<Uuid, ErroApi> {
    let id = sqlx::query_scalar(
        "INSERT INTO jobs (tipo, organizacao_id, payload, executar_em)
         VALUES ($1, $2, $3, COALESCE($4, now())) RETURNING id",
    )
    .bind(tipo)
    .bind(org_id)
    .bind(payload)
    .bind(executar_em)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

/// Retira um job pendente vencido (`SKIP LOCKED`), marca `executando` e soma
/// uma tentativa. `None` se a fila estiver vazia.
pub async fn retirar(pool: &PgPool) -> Result<Option<Job>, ErroApi> {
    let linha: Option<(Uuid, String, Option<Uuid>, Value, i32)> = sqlx::query_as(
        "UPDATE jobs SET status = 'executando', tentativas = tentativas + 1, atualizado_em = now()
         WHERE id = (
             SELECT id FROM jobs
             WHERE status = 'pendente' AND executar_em <= now()
             ORDER BY executar_em, id
             LIMIT 1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING id, tipo, organizacao_id, payload, tentativas",
    )
    .fetch_optional(pool)
    .await?;
    Ok(
        linha.map(|(id, tipo, organizacao_id, payload, tentativas)| Job {
            id,
            tipo,
            organizacao_id,
            payload,
            tentativas,
        }),
    )
}

/// Processa no máximo um job. `true` se havia um job (com sucesso ou falha).
pub async fn executar_um(estado: &AppState) -> Result<bool, ErroApi> {
    let Some(job) = retirar(&estado.pool).await? else {
        return Ok(false);
    };
    tracing::info!(job = %job.id, tipo = %job.tipo, tentativa = job.tentativas, "Executando job");
    let resultado = com_batimento(&estado.pool, job.id, proteger(processar(estado, &job))).await;
    match resultado {
        Ok(()) => {
            sqlx::query(
                "UPDATE jobs SET status = 'concluido', erro = NULL, atualizado_em = now()
                 WHERE id = $1",
            )
            .bind(job.id)
            .execute(&estado.pool)
            .await?;
            tracing::info!(job = %job.id, tipo = %job.tipo, "Job concluído");
        }
        Err(falha) => registrar_falha(&estado.pool, &job, &falha).await?,
    }
    Ok(true)
}

/// Executa `trabalho` atualizando `jobs.atualizado_em` a cada minuto, para que
/// jobs longos não sejam confundidos com presos.
async fn com_batimento<F: Future<Output = Result<(), Falha>>>(
    pool: &PgPool,
    id: Uuid,
    trabalho: F,
) -> Result<(), Falha> {
    let mut trabalho = std::pin::pin!(trabalho);
    let mut intervalo = tokio::time::interval(INTERVALO_BATIMENTO);
    intervalo.tick().await;
    loop {
        tokio::select! {
            resultado = &mut trabalho => return resultado,
            _ = intervalo.tick() => {
                let r = sqlx::query(
                    "UPDATE jobs SET atualizado_em = now() WHERE id = $1 AND status = 'executando'",
                )
                .bind(id)
                .execute(pool)
                .await;
                if let Err(erro) = r {
                    tracing::warn!(job = %id, erro = %erro, "Falha no batimento do job");
                }
            }
        }
    }
}

/// Espera antes da próxima tentativa: 1 min, 2 min, 4 min…
pub fn espera_retentativa(tentativas: i32) -> Duration {
    let expoente = u32::try_from(tentativas.saturating_sub(1).clamp(0, 10)).unwrap_or(0);
    Duration::from_secs(60 * 2_u64.pow(expoente))
}

async fn registrar_falha(pool: &PgPool, job: &Job, falha: &Falha) -> Result<(), ErroApi> {
    let definitiva = matches!(falha, Falha::Permanente(_)) || job.tentativas >= MAX_TENTATIVAS;
    let erro: String = falha.mensagem().chars().take(ERRO_MAX).collect();
    let espera = i64::try_from(espera_retentativa(job.tentativas).as_secs()).unwrap_or(3600);
    sqlx::query(
        "UPDATE jobs SET
             status = CASE WHEN $2 THEN 'falhou' ELSE 'pendente' END,
             executar_em = CASE WHEN $2 THEN executar_em
                                ELSE now() + make_interval(secs => $4::double precision) END,
             erro = $3, atualizado_em = now()
         WHERE id = $1",
    )
    .bind(job.id)
    .bind(definitiva)
    .bind(&erro)
    .bind(espera)
    .execute(pool)
    .await?;
    if definitiva {
        tracing::error!(job = %job.id, tipo = %job.tipo, tentativas = job.tentativas, erro = %erro, "Job falhou");
    } else {
        tracing::warn!(job = %job.id, tipo = %job.tipo, tentativas = job.tentativas, erro = %erro, "Job falhou; será repetido");
    }
    Ok(())
}

/// Organização do job (coluna ou `payload.org_id`).
fn org_do_job(job: &Job) -> Option<Uuid> {
    job.payload
        .get("org_id")
        .and_then(Value::as_str)
        .and_then(|t| Uuid::parse_str(t).ok())
        .or(job.organizacao_id)
}

/// `true` se a organização foi excluída (ou não existe mais).
async fn org_excluida(pool: &PgPool, org: Uuid) -> Result<bool, ErroApi> {
    let ativa: Option<bool> =
        sqlx::query_scalar("SELECT excluida_em IS NULL FROM organizacoes WHERE id = $1")
            .bind(org)
            .fetch_optional(pool)
            .await?;
    Ok(ativa != Some(true))
}

async fn processar(estado: &AppState, job: &Job) -> Result<(), Falha> {
    if let Some(org) = org_do_job(job)
        && org_excluida(&estado.pool, org).await?
    {
        tracing::info!(job = %job.id, tipo = %job.tipo, org = %org, "Organização excluída; job concluído sem executar");
        return Ok(());
    }
    match job.tipo.as_str() {
        TIPO_TRANSPARENCIA => {
            let org =
                org_do_job(job).ok_or_else(|| Falha::Permanente("payload sem org_id".into()))?;
            let ano = job
                .payload
                .get("ano")
                .and_then(Value::as_i64)
                .and_then(|a| i32::try_from(a).ok())
                .ok_or_else(|| Falha::Permanente("payload sem ano".into()))?;
            integracoes::sincronizar(estado, org, ano)
                .await
                .map(|_| ())
                .map_err(|falha| {
                    if falha.permanente {
                        Falha::Permanente(falha.mensagem)
                    } else {
                        Falha::Temporaria(falha.mensagem)
                    }
                })
        }
        TIPO_ALERTAS => rodada_diaria(estado).await,
        TIPO_EMAIL => {
            envio_email::processar_fila(estado).await?;
            Ok(())
        }
        outro => Err(Falha::Permanente(format!(
            "tipo de job desconhecido: {outro}"
        ))),
    }
}

/// Rodada diária: alertas, agendamento das sincronizações ativas e limpeza.
/// As etapas são independentes (falha numa não impede as outras; dentro de
/// cada uma, falha de uma organização não impede as demais). Se alguma etapa
/// falhou, o job é repetido; todas as etapas são idempotentes.
async fn rodada_diaria(estado: &AppState) -> Result<(), Falha> {
    let mut falhas = Vec::new();
    if let Err(erro) = notificacoes::alertas::gerar_todas(estado).await {
        tracing::error!(erro = %erro, "Rodada diária: falha nos alertas");
        falhas.push(format!("alertas: {}", erro.mensagem()));
    }
    if let Err(erro) = integracoes::agendar_sincronizacoes_ativas(estado).await {
        tracing::error!(erro = %erro, "Rodada diária: falha ao agendar sincronizações");
        falhas.push(format!("sincronizações: {}", erro.mensagem()));
    }
    if let Err(erro) = limpeza::executar(estado).await {
        tracing::error!(erro = %erro, "Rodada diária: falha na limpeza");
        falhas.push(format!("limpeza: {}", erro.mensagem()));
    }
    if falhas.is_empty() {
        Ok(())
    } else {
        Err(Falha::Temporaria(falhas.join("; ")))
    }
}

/// Recoloca na fila jobs em `executando` sem batimento há mais de 15 minutos
/// (ou marca `falhou` se já esgotaram as tentativas). Devolve quantos mudaram.
pub async fn recuperar_presos(pool: &PgPool) -> Result<u64, ErroApi> {
    let r = sqlx::query(
        "UPDATE jobs SET
             status = CASE WHEN tentativas >= $1 THEN 'falhou' ELSE 'pendente' END,
             erro = 'Execução interrompida (processo encerrado durante o job).',
             executar_em = now(), atualizado_em = now()
         WHERE status = 'executando' AND atualizado_em < now() - make_interval(mins => $2)",
    )
    .bind(MAX_TENTATIVAS)
    .bind(PRESO_APOS_MINUTOS)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// Instante UTC de `data` às 06:00 em Brasília (UTC−3).
pub fn horario_alertas(data: NaiveDate) -> DateTime<Utc> {
    data.and_hms_opt(HORA_ALERTAS_BRT + 3, 0, 0)
        .unwrap_or_default()
        .and_utc()
}

/// Garante (idempotente) o job `alertas_diarios` de hoje às 06:00 BRT.
/// Devolve `true` se criou agora.
pub async fn agendar_alertas(estado: &AppState) -> Result<bool, ErroApi> {
    let hoje = estado.hoje();
    let quando = horario_alertas(hoje);
    let mut tx = estado.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(TRAVA_AGENDADOR)
        .execute(&mut *tx)
        .await?;
    let criado = sqlx::query(
        "INSERT INTO jobs (tipo, payload, executar_em)
         SELECT $1, $2, $3
         WHERE NOT EXISTS (SELECT 1 FROM jobs WHERE tipo = $1 AND executar_em = $3)",
    )
    .bind(TIPO_ALERTAS)
    .bind(json!({ "data": hoje }))
    .bind(quando)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    tx.commit().await?;
    if criado {
        tracing::info!(data = %hoje, "Job de alertas diários agendado");
    }
    Ok(criado)
}

/// Com SMTP configurado e e-mails `registrado` prontos para (re)envio (sem
/// espera pendente), garante um job `envio_email` na fila. Sem SMTP não faz
/// nada (os e-mails ficam registrados).
pub async fn agendar_envio_email(estado: &AppState) -> Result<bool, ErroApi> {
    if estado.config.smtp_url.is_none() {
        return Ok(false);
    }
    let criado = sqlx::query(
        "INSERT INTO jobs (tipo, payload)
         SELECT $1, '{}'::jsonb
         WHERE EXISTS (SELECT 1 FROM emails_saida
                       WHERE status = 'registrado' AND tentativas < $2
                         AND (proxima_tentativa_em IS NULL OR proxima_tentativa_em <= now()))
         ON CONFLICT DO NOTHING",
    )
    .bind(TIPO_EMAIL)
    .bind(envio_email::MAX_TENTATIVAS_EMAIL)
    .execute(&estado.pool)
    .await?
    .rows_affected()
        > 0;
    Ok(criado)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn espera_exponencial() {
        assert_eq!(espera_retentativa(1), Duration::from_secs(60));
        assert_eq!(espera_retentativa(2), Duration::from_secs(120));
        assert_eq!(espera_retentativa(3), Duration::from_secs(240));
        assert_eq!(espera_retentativa(0), Duration::from_secs(60));
        assert_eq!(espera_retentativa(99), Duration::from_secs(60 * 1024));
    }

    #[test]
    fn horario_dos_alertas_em_brt() {
        let data = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap_or_default();
        assert_eq!(
            horario_alertas(data).to_rfc3339(),
            "2026-10-20T09:00:00+00:00"
        );
    }
}
