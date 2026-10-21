import { ehErroApi, mensagemDeErro } from '@/lib/api'
import type { DocumentoTransparencia, FaseTransparencia, SituacaoDocumento } from '@/lib/tipos'

export const ROTULOS_FASE: Record<FaseTransparencia, string> = {
  1: 'Empenho',
  2: 'Liquidação',
  3: 'Pagamento',
}

export const ROTULOS_SITUACAO: Record<SituacaoDocumento, string> = {
  novo: 'Novo',
  vinculado: 'Vinculado',
  ignorado: 'Ignorado',
}

/** Código curto para exibir (o completo vai no `title`). */
export function codigoExibido(
  doc: Pick<DocumentoTransparencia, 'codigo' | 'codigo_resumido'>,
): string {
  return doc.codigo_resumido?.trim() || doc.codigo
}

/**
 * Número do empenho (ex.: "2026NE000123") mencionado no documento, para
 * pré-preencher a busca de empenhos.
 */
export function numeroEmpenhoDe(
  doc: Pick<DocumentoTransparencia, 'codigo' | 'codigo_resumido' | 'empenho_codigo'>,
): string {
  for (const texto of [doc.empenho_codigo, doc.codigo_resumido, doc.codigo]) {
    const m = /\d{4}NE\d{6}/i.exec(texto?.replace(/\s+/g, '') ?? '')
    if (m) return m[0].toUpperCase()
  }
  return ''
}

/** Mensagem legível para erros das ações (409 vem com o motivo do servidor). */
export function mensagemAcao(erro: unknown): string {
  if (ehErroApi(erro) && erro.status === 409) {
    const motivo = erro.mensagem.trim()
    return motivo && motivo !== 'Esta ação conflita com o estado atual do registro.'
      ? motivo
      : 'O documento ou a nota mudou de situação. Atualize a lista e tente de novo.'
  }
  return mensagemDeErro(erro)
}
