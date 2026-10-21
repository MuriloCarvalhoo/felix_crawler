/** Rótulos legíveis para a trilha de auditoria (ADR-018). Valores desconhecidos caem em `rotuloGenerico`. */

export const ENTIDADES_AUDITORIA: Record<string, string> = {
  organizacao: 'Organização',
  membro: 'Membro',
  convite: 'Convite',
  empresa: 'Empresa (CNPJ)',
  assinatura: 'Assinatura',
  empenho: 'Empenho',
  nota: 'Nota fiscal',
  cobranca: 'Cobrança',
  integracao: 'Integração',
  documento_transparencia: 'Documento da Transparência',
  notificacao: 'Notificação',
}

const ACOES: Record<string, string> = {
  cadastro: 'Cadastro',
  criar: 'Criou',
  atualizar: 'Atualizou',
  alterar: 'Alterou',
  editar: 'Editou',
  excluir: 'Excluiu',
  remover: 'Removeu',
  sair: 'Saiu',
  aceitar: 'Aceitou',
  revogar: 'Revogou',
  alterar_papel: 'Alterou papel',
  trocar_plano: 'Trocou de plano',
  anular: 'Anulou',
  registrar_evento: 'Registrou evento',
  registrar: 'Registrou',
  sincronizar: 'Sincronizou',
  vincular: 'Vinculou',
  importar: 'Importou',
  ignorar: 'Ignorou',
  gerar_alertas: 'Gerou alertas',
}

/** "registrar_evento" → "Registrar evento". */
export function rotuloGenerico(valor: string): string {
  const texto = valor.replace(/_/g, ' ').trim()
  return texto ? texto.charAt(0).toUpperCase() + texto.slice(1) : '—'
}

export function rotuloEntidade(entidade: string): string {
  return ENTIDADES_AUDITORIA[entidade] ?? rotuloGenerico(entidade)
}

/** Aceita também o formato "entidade.acao" (ex.: "empenho.anular" → "Anulou"). */
export function rotuloAcao(acao: string): string {
  const verbo = acao.includes('.') ? acao.slice(acao.lastIndexOf('.') + 1) : acao
  return ACOES[acao] ?? ACOES[verbo] ?? rotuloGenerico(verbo)
}

/** JSON indentado; volta texto simples se não for serializável. */
export function formatarDados(dados: unknown): string {
  if (dados === null || dados === undefined) return '—'
  try {
    return JSON.stringify(dados, null, 2)
  } catch {
    return typeof dados === 'string' ? dados : '(dados não exibíveis)'
  }
}

/** `true` se há algo para mostrar em `dados` (objeto/array não vazio ou valor simples). */
export function temDados(dados: unknown): boolean {
  if (dados === null || dados === undefined) return false
  if (Array.isArray(dados)) return dados.length > 0
  if (typeof dados === 'object') return Object.keys(dados).length > 0
  return true
}
