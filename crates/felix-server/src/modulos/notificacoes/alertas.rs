//! Alertas diários de vencimento (job `alertas_diarios` e
//! `POST /orgs/{org}/alertas/gerar`).
//!
//! Para cada organização ativa, cria notificações da organização inteira
//! (`usuario_id` nulo):
//! - `vence_5d`: notas `emitida`/`liquidada` com vencimento entre hoje e hoje + 5;
//! - `vencida`: notas `emitida`/`liquidada` com vencimento antes de hoje.
//!
//! `vence_5d` uma vez por nota e vencimento (`{org}:vence_5d:{nota}:{vencimento}`);
//! `vencida` uma vez por nota (`{org}:vencida:{nota}`) e depois um lembrete
//! semanal (`{org}:vencida:{nota}:{ano}-S{semana ISO}`). Inserção em lotes. Se o
//! plano tem `alertas_email`, registra também um e-mail-resumo diário para cada
//! dono/admin (um por usuário, por organização, por dia; tabela
//! `alertas_email_enviados`), listando no máximo 50 notas. "Hoje" é a data em
//! Brasília (`estado.hoje()`).

use crate::{
    app::AppState,
    db, email,
    http::ErroApi,
    modulos::assinatura::{limites_do_plano, plano_efetivo},
};
use chrono::{DateTime, Datelike, Days, NaiveDate, Utc};
use felix_core::dinheiro::reais;
use sqlx::PgConnection;
use uuid::Uuid;

/// Janela do alerta `vence_5d`, em dias corridos a partir de hoje.
pub const DIAS_AVISO: u64 = 5;
/// Linhas listadas no corpo do e-mail-resumo (o restante vira "e mais N").
const LINHAS_EMAIL: usize = 50;
/// Notas por comando de inserção de notificações.
const LOTE_NOTIFICACOES: usize = 500;

/// Nota com vencimento próximo ou vencida.
#[derive(Debug, Clone, sqlx::FromRow)]
struct NotaAlerta {
    id: Uuid,
    numero: String,
    valor_centavos: i64,
    vencimento: NaiveDate,
    empenho_numero: String,
    ug: String,
}

impl NotaAlerta {
    fn vencida(&self, hoje: NaiveDate) -> bool {
        self.vencimento < hoje
    }
}

/// Resultado da geração para uma organização.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultadoAlertas {
    /// Notificações novas (as já existentes do dia não contam).
    pub criadas: u64,
    /// E-mails-resumo registrados.
    pub emails: u64,
}

/// Gera alertas de todas as organizações não excluídas. Falha numa
/// organização não impede as outras; o erro é devolvido ao final.
pub async fn gerar_todas(estado: &AppState) -> Result<ResultadoAlertas, ErroApi> {
    let orgs: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM organizacoes WHERE excluida_em IS NULL ORDER BY id")
            .fetch_all(&estado.pool)
            .await?;
    let mut total = ResultadoAlertas::default();
    let mut ultimo_erro = None;
    for org in orgs {
        match gerar_para_org(estado, org).await {
            Ok(r) => {
                total.criadas += r.criadas;
                total.emails += r.emails;
            }
            Err(erro) => {
                tracing::error!(org = %org, erro = %erro, "Falha ao gerar alertas da organização");
                ultimo_erro = Some(erro);
            }
        }
    }
    tracing::info!(
        criadas = total.criadas,
        emails = total.emails,
        "Alertas diários gerados"
    );
    match ultimo_erro {
        Some(erro) => Err(erro),
        None => Ok(total),
    }
}

/// Gera os alertas de hoje de uma organização (idempotente no mesmo dia).
pub async fn gerar_para_org(estado: &AppState, org: Uuid) -> Result<ResultadoAlertas, ErroApi> {
    let hoje = estado.hoje();
    let limite = hoje.checked_add_days(Days::new(DIAS_AVISO)).unwrap_or(hoje);
    let mut tx = db::tenant(&estado.pool, org).await?;
    let notas: Vec<NotaAlerta> = sqlx::query_as(
        "SELECT n.id, n.numero, n.valor_centavos, n.vencimento, e.numero AS empenho_numero,
                e.ug::text AS ug
         FROM notas n
         JOIN empenhos e ON e.organizacao_id = n.organizacao_id AND e.id = n.empenho_id
         WHERE n.organizacao_id = $1 AND n.status IN ('emitida', 'liquidada')
           AND n.vencimento <= $2
         ORDER BY n.vencimento, n.numero, n.id",
    )
    .bind(org)
    .bind(limite)
    .fetch_all(&mut *tx)
    .await?;
    let criadas = inserir_notificacoes(&mut tx, org, hoje, estado.agora(), &notas).await?;
    let mut emails = 0;
    if !notas.is_empty() {
        let plano = plano_efetivo(&mut tx, org, estado.agora()).await?;
        if limites_do_plano(plano).alertas_email {
            emails = registrar_emails(&mut tx, estado, org, hoje, &notas).await?;
        }
    }
    tx.commit().await?;
    Ok(ResultadoAlertas { criadas, emails })
}

fn data_br(data: NaiveDate) -> String {
    data.format("%d/%m/%Y").to_string()
}

fn texto_dias(dias: i64) -> String {
    if dias == 1 {
        "1 dia".to_owned()
    } else {
        format!("{dias} dias")
    }
}

/// Título e corpo do alerta de uma nota.
fn mensagem(nota: &NotaAlerta, hoje: NaiveDate) -> (&'static str, String, String) {
    let base = format!(
        "A NF {} do empenho {} (UG {}), no valor de R$ {}",
        nota.numero,
        nota.empenho_numero,
        nota.ug.trim(),
        reais(nota.valor_centavos)
    );
    if nota.vencida(hoje) {
        let atraso = (hoje - nota.vencimento).num_days();
        (
            "vencida",
            format!("NF {} vencida há {}", nota.numero, texto_dias(atraso)),
            format!(
                "{base}, venceu em {} e está com {} de atraso.",
                data_br(nota.vencimento),
                texto_dias(atraso)
            ),
        )
    } else {
        let faltam = (nota.vencimento - hoje).num_days();
        let titulo = if faltam == 0 {
            format!("NF {} vence hoje", nota.numero)
        } else {
            format!("NF {} vence em {}", nota.numero, texto_dias(faltam))
        };
        (
            "vence_5d",
            titulo,
            format!("{base}, vence em {}.", data_br(nota.vencimento)),
        )
    }
}

/// Chave do primeiro aviso de nota vencida (um por nota).
fn chave_vencida(org: Uuid, nota: Uuid) -> String {
    format!("{org}:vencida:{nota}")
}

/// Insere as notificações do dia em lotes de [`LOTE_NOTIFICACOES`]:
/// - `vence_5d`: uma por nota e vencimento (`{org}:vence_5d:{nota}:{vencimento}`);
/// - `vencida`: uma por nota (`{org}:vencida:{nota}`) e depois um lembrete por
///   semana ISO (`{org}:vencida:{nota}:{ano}-S{semana}`), só se o último aviso
///   da nota tiver cerca de 6 dias ou mais (5,5 dias, tolerando variação no
///   horário do job diário).
///
/// `criado_em` = `agora` (relógio da aplicação). Devolve quantas foram criadas.
async fn inserir_notificacoes(
    conn: &mut PgConnection,
    org: Uuid,
    hoje: NaiveDate,
    agora: DateTime<Utc>,
    notas: &[NotaAlerta],
) -> Result<u64, ErroApi> {
    let semana = hoje.iso_week();
    let sufixo_semana = format!("{:04}-S{:02}", semana.year(), semana.week());
    let mut criadas = 0;
    for lote in notas.chunks(LOTE_NOTIFICACOES) {
        let mut novas = Linhas::default();
        let mut lembretes = Linhas::default();
        let mut bases = Vec::new();
        for nota in lote {
            let (tipo, titulo, corpo) = mensagem(nota, hoje);
            let link = format!("/o/{org}/notas/{}", nota.id);
            if tipo == "vencida" {
                let base = chave_vencida(org, nota.id);
                lembretes.push(
                    tipo,
                    titulo.clone(),
                    corpo.clone(),
                    link.clone(),
                    format!("{base}:{sufixo_semana}"),
                );
                bases.push(base.clone());
                novas.push(tipo, titulo, corpo, link, base);
            } else {
                let chave = format!("{org}:{tipo}:{}:{}", nota.id, nota.vencimento);
                novas.push(tipo, titulo, corpo, link, chave);
            }
        }
        // Lembretes primeiro: só valem para avisos de dias anteriores.
        if !bases.is_empty() {
            criadas += sqlx::query(
                "INSERT INTO notificacoes
                     (organizacao_id, usuario_id, tipo, titulo, corpo, link, chave_dedup, criado_em)
                 SELECT $1, NULL, t.tipo, t.titulo, t.corpo, t.link, t.chave, $8
                 FROM unnest($2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::text[])
                      AS t(tipo, titulo, corpo, link, chave, base)
                 WHERE EXISTS (SELECT 1 FROM notificacoes b
                               WHERE b.organizacao_id = $1 AND b.chave_dedup = t.base)
                   AND NOT EXISTS (SELECT 1 FROM notificacoes r
                                   WHERE r.organizacao_id = $1
                                     AND (r.chave_dedup = t.base OR r.chave_dedup LIKE t.base || ':%')
                                     AND r.criado_em > $8 - interval '5 days 12 hours')
                 ON CONFLICT (chave_dedup) DO NOTHING",
            )
            .bind(org)
            .bind(&lembretes.tipos)
            .bind(&lembretes.titulos)
            .bind(&lembretes.corpos)
            .bind(&lembretes.links)
            .bind(&lembretes.chaves)
            .bind(&bases)
            .bind(agora)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        }
        criadas += sqlx::query(
            "INSERT INTO notificacoes
                 (organizacao_id, usuario_id, tipo, titulo, corpo, link, chave_dedup, criado_em)
             SELECT $1, NULL, t.tipo, t.titulo, t.corpo, t.link, t.chave, $7
             FROM unnest($2::text[], $3::text[], $4::text[], $5::text[], $6::text[])
                  AS t(tipo, titulo, corpo, link, chave)
             ON CONFLICT (chave_dedup) DO NOTHING",
        )
        .bind(org)
        .bind(&novas.tipos)
        .bind(&novas.titulos)
        .bind(&novas.corpos)
        .bind(&novas.links)
        .bind(&novas.chaves)
        .bind(agora)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    }
    Ok(criadas)
}

/// Colunas de um lote de notificações (para `unnest`).
#[derive(Default)]
struct Linhas {
    tipos: Vec<String>,
    titulos: Vec<String>,
    corpos: Vec<String>,
    links: Vec<String>,
    chaves: Vec<String>,
}

impl Linhas {
    fn push(&mut self, tipo: &str, titulo: String, corpo: String, link: String, chave: String) {
        self.tipos.push(tipo.to_owned());
        self.titulos.push(titulo);
        self.corpos.push(corpo);
        self.links.push(link);
        self.chaves.push(chave);
    }
}

async fn registrar_emails(
    conn: &mut PgConnection,
    estado: &AppState,
    org: Uuid,
    hoje: NaiveDate,
    notas: &[NotaAlerta],
) -> Result<u64, ErroApi> {
    let org_nome: String = sqlx::query_scalar("SELECT nome FROM organizacoes WHERE id = $1")
        .bind(org)
        .fetch_one(&mut *conn)
        .await?;
    let destinatarios: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email::text, u.nome FROM membros m
         JOIN usuarios u ON u.id = m.usuario_id
         WHERE m.organizacao_id = $1 AND m.papel IN ('dono', 'admin')
         ORDER BY u.id",
    )
    .bind(org)
    .fetch_all(&mut *conn)
    .await?;
    let vencidas = notas.iter().filter(|n| n.vencida(hoje)).count();
    let a_vencer = notas.len() - vencidas;
    let assunto =
        format!("Felix: {vencidas} NF(s) vencida(s) e {a_vencer} a vencer em 5 dias — {org_nome}");
    let mut linhas: Vec<String> = notas
        .iter()
        .take(LINHAS_EMAIL)
        .map(|n| {
            let situacao = if n.vencida(hoje) {
                "vencida"
            } else {
                "a vencer"
            };
            format!(
                "- NF {} | empenho {} | UG {} | R$ {} | vencimento {} ({situacao})",
                n.numero,
                n.empenho_numero,
                n.ug.trim(),
                reais(n.valor_centavos),
                data_br(n.vencimento)
            )
        })
        .collect();
    if notas.len() > LINHAS_EMAIL {
        linhas.push(format!("… e mais {} nota(s).", notas.len() - LINHAS_EMAIL));
    }
    let link = format!("{}/o/{org}/notas", estado.config.public_url);
    let mut emails = 0;
    for (usuario, endereco, nome) in destinatarios {
        let novo = sqlx::query(
            "INSERT INTO alertas_email_enviados (organizacao_id, usuario_id, data)
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(org)
        .bind(usuario)
        .bind(hoje)
        .execute(&mut *conn)
        .await?
        .rows_affected()
            > 0;
        if !novo {
            continue;
        }
        let corpo = format!(
            "Olá, {nome}.\n\nResumo de {} da organização {org_nome}:\n\n{}\n\nAcompanhe em: {link}\n\n\
             Você recebe este e-mail por ser dono ou administrador da organização.\n",
            data_br(hoje),
            linhas.join("\n")
        );
        let email_id = email::enfileirar(&mut *conn, &endereco, &assunto, &corpo).await?;
        sqlx::query(
            "UPDATE alertas_email_enviados SET email_id = $4
             WHERE organizacao_id = $1 AND usuario_id = $2 AND data = $3",
        )
        .bind(org)
        .bind(usuario)
        .bind(hoje)
        .bind(email_id)
        .execute(&mut *conn)
        .await?;
        emails += 1;
    }
    Ok(emails)
}
