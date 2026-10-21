//! Envio da caixa de saída (ADR-012). Com `FELIX_SMTP_URL`, o job
//! `envio_email` envia as linhas `registrado` via SMTP com TLS (`lettre`) e
//! marca `enviado` ou, após [`MAX_TENTATIVAS_EMAIL`], `falhou`. Entre as
//! tentativas há espera de 1, 5 e 30 minutos (`proxima_tentativa_em`). Em
//! produção o corpo é apagado após o envio ([`email::redigir_apos_envio`]).
//! Sem SMTP nada muda: os e-mails continuam `registrado` (visíveis em `/dev/emails`).

use crate::{app::AppState, config::Config, email, http::ErroApi};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};
use sqlx::PgPool;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, PoisonError},
};
use uuid::Uuid;

/// Tentativas de envio por e-mail (uma inicial + uma após cada espera de
/// [`ESPERAS_MINUTOS`]).
pub const MAX_TENTATIVAS_EMAIL: i32 = 4;
/// Espera antes da 2ª, 3ª e 4ª tentativas.
pub const ESPERAS_MINUTOS: [i64; 3] = [1, 5, 30];

/// Minutos de espera após a falha da tentativa `tentativas` (1 = primeira).
pub fn espera_minutos(tentativas: i32) -> i64 {
    let indice = usize::try_from(tentativas.saturating_sub(1)).unwrap_or(0);
    ESPERAS_MINUTOS
        .get(indice)
        .or(ESPERAS_MINUTOS.last())
        .copied()
        .unwrap_or(30)
}
/// E-mails enviados por execução do job.
pub const LOTE_EMAILS: usize = 50;

/// Quem efetivamente entrega a mensagem (SMTP em produção; falso nos testes).
pub trait Remetente: Send + Sync {
    /// Envia; o erro deve ser uma mensagem curta, sem endereços nem credenciais.
    fn enviar(
        &self,
        para: &str,
        assunto: &str,
        corpo: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

/// Contagem de uma execução do envio.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoEnvio {
    pub enviados: u32,
    pub falhas: u32,
}

/// Remetente SMTP (`lettre`, Tokio, rustls).
pub struct RemetenteSmtp {
    transporte: AsyncSmtpTransport<Tokio1Executor>,
    de: Mailbox,
}

impl RemetenteSmtp {
    /// `smtps://` usa TLS implícito; `smtp://` sem `tls=` passa a exigir STARTTLS
    /// (`tls=required`). Erros não incluem a URL (que pode ter senha).
    pub fn novo(smtp_url: &str, email_from: &str) -> Result<Self, String> {
        let url = if smtp_url.starts_with("smtp://") && !smtp_url.contains("tls=") {
            let separador = if smtp_url.contains('?') { '&' } else { '?' };
            format!("{smtp_url}{separador}tls=required")
        } else {
            smtp_url.to_owned()
        };
        let transporte = AsyncSmtpTransport::<Tokio1Executor>::from_url(&url)
            .map_err(|_| "FELIX_SMTP_URL inválida.".to_owned())?
            .build();
        let de = email_from
            .parse::<Mailbox>()
            .map_err(|_| "FELIX_EMAIL_FROM inválido.".to_owned())?;
        Ok(Self { transporte, de })
    }
}

impl Remetente for RemetenteSmtp {
    async fn enviar(&self, para: &str, assunto: &str, corpo: &str) -> Result<(), String> {
        let para = para
            .parse::<Mailbox>()
            .map_err(|_| "Endereço de destino inválido.".to_owned())?;
        let mensagem = Message::builder()
            .from(self.de.clone())
            .to(para)
            .subject(assunto)
            .header(ContentType::TEXT_PLAIN)
            .body(corpo.to_owned())
            .map_err(|_| "Mensagem inválida.".to_owned())?;
        self.transporte
            .send(mensagem)
            .await
            .map(|_| ())
            .map_err(|erro| {
                let motivo = if erro.is_permanent() {
                    "rejeitado pelo servidor SMTP (erro permanente)"
                } else if erro.is_transient() {
                    "recusado temporariamente pelo servidor SMTP"
                } else if erro.is_timeout() {
                    "tempo esgotado ao falar com o servidor SMTP"
                } else {
                    "falha de conexão ou TLS com o servidor SMTP"
                };
                tracing::warn!(motivo, "Falha no envio de e-mail");
                format!("Falha no envio: {motivo}.")
            })
    }
}

static REMETENTES: LazyLock<Mutex<HashMap<String, Arc<RemetenteSmtp>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Remetente SMTP da configuração (criado uma vez e reutilizado). `None` sem SMTP.
pub fn remetente(config: &Config) -> Result<Option<Arc<RemetenteSmtp>>, String> {
    let (Some(url), Some(de)) = (&config.smtp_url, &config.email_from) else {
        return Ok(None);
    };
    let chave = format!("{url}\u{0}{de}");
    let mut mapa = REMETENTES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existente) = mapa.get(&chave) {
        return Ok(Some(Arc::clone(existente)));
    }
    let novo = Arc::new(RemetenteSmtp::novo(url, de)?);
    mapa.insert(chave, Arc::clone(&novo));
    Ok(Some(novo))
}

/// Corpo do job `envio_email`: sem SMTP não faz nada.
pub async fn processar_fila(estado: &AppState) -> Result<ResumoEnvio, ErroApi> {
    let Some(remetente) = remetente(&estado.config).map_err(ErroApi::indisponivel)? else {
        return Ok(ResumoEnvio::default());
    };
    enviar_registrados(
        &estado.pool,
        remetente.as_ref(),
        LOTE_EMAILS,
        estado.config.producao(),
    )
    .await
}

/// Envia até `limite` e-mails `registrado` prontos para envio (sem espera
/// pendente; cada um no máximo uma vez nesta chamada), atualizando status,
/// tentativas, erro e a próxima tentativa. Com `redigir`, o corpo é apagado
/// logo após o envio (produção).
pub async fn enviar_registrados<R: Remetente>(
    pool: &PgPool,
    remetente: &R,
    limite: usize,
    redigir: bool,
) -> Result<ResumoEnvio, ErroApi> {
    let mut resumo = ResumoEnvio::default();
    let mut vistos: Vec<Uuid> = Vec::new();
    while vistos.len() < limite {
        let linha: Option<(Uuid, String, String, String, i32)> = sqlx::query_as(
            "UPDATE emails_saida SET tentativas = tentativas + 1
             WHERE id = (
                 SELECT id FROM emails_saida
                 WHERE status = 'registrado' AND tentativas < $1 AND NOT (id = ANY($2))
                   AND (proxima_tentativa_em IS NULL OR proxima_tentativa_em <= now())
                 ORDER BY criado_em, id
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, para::text, assunto, corpo, tentativas",
        )
        .bind(MAX_TENTATIVAS_EMAIL)
        .bind(&vistos)
        .fetch_optional(pool)
        .await?;
        let Some((id, para, assunto, corpo, tentativas)) = linha else {
            break;
        };
        vistos.push(id);
        match remetente.enviar(&para, &assunto, &corpo).await {
            Ok(()) => {
                let mut conn = pool.acquire().await?;
                sqlx::query(
                    "UPDATE emails_saida SET status = 'enviado', enviado_em = now(), erro = NULL,
                         proxima_tentativa_em = NULL
                     WHERE id = $1",
                )
                .bind(id)
                .execute(&mut *conn)
                .await?;
                if redigir {
                    email::redigir_apos_envio(&mut conn, id).await?;
                }
                resumo.enviados += 1;
            }
            Err(erro) => {
                let erro: String = erro.chars().take(300).collect();
                let espera = i32::try_from(espera_minutos(tentativas)).unwrap_or(30);
                sqlx::query(
                    "UPDATE emails_saida
                     SET status = CASE WHEN $2 THEN 'falhou' ELSE 'registrado' END, erro = $3,
                         proxima_tentativa_em = CASE WHEN $2 THEN NULL
                                                     ELSE now() + make_interval(mins => $4) END
                     WHERE id = $1",
                )
                .bind(id)
                .bind(tentativas >= MAX_TENTATIVAS_EMAIL)
                .bind(erro)
                .bind(espera)
                .execute(pool)
                .await?;
                resumo.falhas += 1;
            }
        }
    }
    Ok(resumo)
}
