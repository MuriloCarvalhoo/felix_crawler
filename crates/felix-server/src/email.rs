//! Caixa de saída de e-mail (ADR-012). Todo e-mail vira uma linha em
//! `emails_saida`; o worker de jobs envia quando há SMTP configurado.

use crate::http::erro::ErroApi;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Tamanho máximo do assunto (caracteres); o excedente é cortado.
pub const ASSUNTO_MAX: usize = 200;

/// Registra um e-mail na caixa de saída (status `registrado`) e devolve o id.
/// Funciona dentro de uma transação de tenant (`emails_saida` é global e tem
/// GRANT para `felix_app`), então o e-mail só "existe" se a transação confirmar.
pub async fn enfileirar(
    conn: &mut PgConnection,
    para: &str,
    assunto: &str,
    corpo: &str,
) -> Result<Uuid, ErroApi> {
    let para = para.trim();
    if para.is_empty() || !para.contains('@') {
        return Err(ErroApi::interno_log(
            "email::enfileirar",
            "destinatário inválido",
        ));
    }
    let assunto: String = assunto
        .chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .take(ASSUNTO_MAX)
        .collect();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO emails_saida (para, assunto, corpo, status)
         VALUES ($1, $2, $3, 'registrado') RETURNING id",
    )
    .bind(para)
    .bind(assunto)
    .bind(corpo)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

/// Texto que substitui o corpo de um e-mail já enviado em produção.
pub const CORPO_REDIGIDO: &str = "[conteúdo removido após envio]";

/// Remove o corpo (links com token de convite/redefinição) de um e-mail já
/// `enviado`. Chamar logo após marcar o envio, **somente em produção**
/// (`config.producao()`); em dev o corpo fica para `/dev/emails`.
/// Devolve `true` se a linha foi redigida.
pub async fn redigir_apos_envio(conn: &mut PgConnection, id: Uuid) -> Result<bool, ErroApi> {
    let resultado = sqlx::query(
        "UPDATE emails_saida SET corpo = $2 WHERE id = $1 AND status = 'enviado' AND corpo <> $2",
    )
    .bind(id)
    .bind(CORPO_REDIGIDO)
    .execute(conn)
    .await?;
    Ok(resultado.rows_affected() > 0)
}

/// Apaga da caixa de saída os e-mails criados há mais de `dias` dias (qualquer
/// status). Devolve quantos foram apagados. Usado pelo job de limpeza.
pub async fn purgar_antigos(pool: &PgPool, dias: u32) -> Result<u64, ErroApi> {
    let resultado =
        sqlx::query("DELETE FROM emails_saida WHERE criado_em < now() - make_interval(days => $1)")
            .bind(i32::try_from(dias).unwrap_or(i32::MAX))
            .execute(pool)
            .await?;
    Ok(resultado.rows_affected())
}
