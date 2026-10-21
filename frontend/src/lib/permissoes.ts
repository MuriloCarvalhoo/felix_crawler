/**
 * Permissões por papel (ADR-008). Serve só para esconder/desabilitar controles:
 * o servidor é quem decide (403/404).
 *
 * | Ação                                         | dono | admin | operador | leitor |
 * |----------------------------------------------|:----:|:-----:|:--------:|:------:|
 * | Ler dados da organização                     |  ✓   |   ✓   |    ✓     |   ✓    |
 * | Criar/editar empenhos, NFs, cobranças, eventos|  ✓   |   ✓   |    ✓     |        |
 * | CNPJs, convites, membros, integrações         |  ✓   |   ✓   |          |        |
 * | Auditoria                                    |  ✓   |   ✓   |          |        |
 * | Plano, excluir organização, transferir dono  |  ✓   |       |          |        |
 */
import type { Papel } from './tipos'

export type Acao =
  // leitor
  | 'ler'
  | 'exportar'
  // operador
  | 'editar_operacao'
  | 'analisar_pdf'
  | 'criar_empenho'
  | 'editar_empenho'
  | 'anular_empenho'
  | 'criar_nota'
  | 'editar_nota'
  | 'registrar_evento'
  | 'registrar_cobranca'
  | 'vincular_documento'
  // admin
  | 'reabrir_nota'
  | 'editar_organizacao'
  | 'gerenciar_empresas'
  | 'gerenciar_membros'
  | 'gerenciar_convites'
  | 'gerenciar_integracoes'
  | 'sincronizar_integracao'
  | 'gerar_alertas'
  | 'ver_auditoria'
  // dono
  | 'gerenciar_plano'
  | 'excluir_organizacao'
  | 'transferir_dono'
  | 'promover_a_dono'

const NIVEL: Record<Papel, number> = { leitor: 0, operador: 1, admin: 2, dono: 3 }

const MINIMO: Record<Acao, Papel> = {
  ler: 'leitor',
  exportar: 'leitor',
  editar_operacao: 'operador',
  analisar_pdf: 'operador',
  criar_empenho: 'operador',
  editar_empenho: 'operador',
  anular_empenho: 'operador',
  criar_nota: 'operador',
  editar_nota: 'operador',
  registrar_evento: 'operador',
  registrar_cobranca: 'operador',
  vincular_documento: 'operador',
  reabrir_nota: 'admin',
  editar_organizacao: 'admin',
  gerenciar_empresas: 'admin',
  gerenciar_membros: 'admin',
  gerenciar_convites: 'admin',
  gerenciar_integracoes: 'admin',
  sincronizar_integracao: 'admin',
  gerar_alertas: 'admin',
  ver_auditoria: 'admin',
  gerenciar_plano: 'dono',
  excluir_organizacao: 'dono',
  transferir_dono: 'dono',
  promover_a_dono: 'dono',
}

export function papelMinimo(acao: Acao): Papel {
  return MINIMO[acao]
}

/** `true` se o papel alcança o papel mínimo da ação. Papel ausente → `false`. */
export function pode(papel: Papel | null | undefined, acao: Acao): boolean {
  if (!papel) return false
  return NIVEL[papel] >= NIVEL[MINIMO[acao]]
}

/** Compara papéis: `true` se `papel` é pelo menos `minimo`. */
export function papelAtinge(papel: Papel | null | undefined, minimo: Papel): boolean {
  if (!papel) return false
  return NIVEL[papel] >= NIVEL[minimo]
}

export const NOMES_PAPEL: Record<Papel, string> = {
  dono: 'Dono',
  admin: 'Administrador',
  operador: 'Operador',
  leitor: 'Leitor',
}

export const DESCRICOES_PAPEL: Record<Papel, string> = {
  dono: 'Controle total, inclusive plano e exclusão da organização.',
  admin: 'Gerencia CNPJs, equipe, convites, integrações e auditoria.',
  operador: 'Cadastra empenhos, notas, eventos e cobranças.',
  leitor: 'Consulta e exporta dados, sem alterar nada.',
}

export const PAPEIS: readonly Papel[] = ['dono', 'admin', 'operador', 'leitor']
