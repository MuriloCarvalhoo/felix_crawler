import { describe, expect, it } from 'vitest'
import { papelAtinge, papelMinimo, pode, type Acao } from './permissoes'
import type { Papel } from './tipos'

const PAPEIS: Papel[] = ['dono', 'admin', 'operador', 'leitor']

function quemPode(acao: Acao): Papel[] {
  return PAPEIS.filter((p) => pode(p, acao))
}

describe('pode (ADR-008)', () => {
  it('todos leem e exportam', () => {
    expect(quemPode('ler')).toEqual(PAPEIS)
    expect(quemPode('exportar')).toEqual(PAPEIS)
  })

  it('operador para cima cria e edita empenhos, NFs, cobranças e eventos', () => {
    for (const acao of [
      'criar_empenho',
      'editar_empenho',
      'anular_empenho',
      'analisar_pdf',
      'criar_nota',
      'editar_nota',
      'registrar_evento',
      'registrar_cobranca',
      'vincular_documento',
    ] as const) {
      expect(quemPode(acao), acao).toEqual(['dono', 'admin', 'operador'])
    }
  })

  it('admin para cima gerencia CNPJs, convites, membros, integrações e auditoria', () => {
    for (const acao of [
      'gerenciar_empresas',
      'gerenciar_convites',
      'gerenciar_membros',
      'gerenciar_integracoes',
      'sincronizar_integracao',
      'ver_auditoria',
      'editar_organizacao',
      'reabrir_nota',
      'gerar_alertas',
    ] as const) {
      expect(quemPode(acao), acao).toEqual(['dono', 'admin'])
    }
  })

  it('só o dono mexe no plano, exclui a organização e transfere a posse', () => {
    for (const acao of ['gerenciar_plano', 'excluir_organizacao', 'transferir_dono', 'promover_a_dono'] as const) {
      expect(quemPode(acao), acao).toEqual(['dono'])
    }
  })

  it('sem papel não pode nada', () => {
    expect(pode(undefined, 'ler')).toBe(false)
    expect(pode(null, 'ler')).toBe(false)
  })

  it('expõe papel mínimo e comparação', () => {
    expect(papelMinimo('criar_nota')).toBe('operador')
    expect(papelAtinge('admin', 'operador')).toBe(true)
    expect(papelAtinge('leitor', 'operador')).toBe(false)
    expect(papelAtinge(undefined, 'leitor')).toBe(false)
  })
})
