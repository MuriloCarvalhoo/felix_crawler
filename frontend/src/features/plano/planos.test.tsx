import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { formatarDados, rotuloAcao, rotuloEntidade, temDados } from '@/features/auditoria/rotulos'
import { BarraUso } from './BarraUso'
import { LIMITES_PLANOS, recursosAcimaDoLimite, textoLimite, textoStatus, textoUso } from './planos'

describe('planos', () => {
  it('descreve limites e uso, inclusive ilimitado', () => {
    expect(textoLimite(null)).toBe('Ilimitado')
    expect(textoLimite(5)).toBe('5')
    expect(textoUso('cnpjs', 1, 1)).toBe('1 de 1 CNPJ')
    expect(textoUso('cnpjs', 3, 5)).toBe('3 de 5 CNPJs')
    expect(textoUso('empenhos_ativos', 42, null)).toBe('42 empenhos ativos (ilimitado)')
  })

  it('mostra status de teste com data', () => {
    expect(textoStatus({ status: 'teste', teste_ate: '2026-11-03T15:00:00Z' })).toBe(
      'Teste até 03/11/2026',
    )
    expect(textoStatus({ status: 'ativa', teste_ate: null })).toBe('Ativa')
  })

  it('aponta recursos acima do limite de outro plano', () => {
    const uso = { cnpjs: 3, empenhos_ativos: 10, usuarios: 2 }
    expect(recursosAcimaDoLimite(uso, LIMITES_PLANOS.gratuito)).toEqual(['cnpjs'])
    expect(recursosAcimaDoLimite(uso, LIMITES_PLANOS.empresarial)).toEqual([])
  })

  it('barra de uso expõe valores acessíveis', () => {
    render(<BarraUso recurso="usuarios" uso={2} limite={10} />)
    const barra = screen.getByRole('meter', { name: 'Usuários' })
    expect(barra).toHaveAttribute('aria-valuenow', '2')
    expect(barra).toHaveAttribute('aria-valuemax', '10')
    expect(barra).toHaveAttribute('aria-valuetext', '2 de 10 usuários')
  })
})

describe('rótulos da auditoria', () => {
  it('traduz ações e entidades conhecidas e cai no genérico', () => {
    expect(rotuloAcao('alterar_papel')).toBe('Alterou papel')
    expect(rotuloAcao('algo_novo')).toBe('Algo novo')
    expect(rotuloAcao('empenho.anular')).toBe('Anulou')
    expect(rotuloAcao('cobranca.registrar')).toBe('Registrou')
    expect(rotuloAcao('nota.registrar_evento')).toBe('Registrou evento')
    expect(rotuloEntidade('nota')).toBe('Nota fiscal')
    expect(rotuloEntidade('outra_coisa')).toBe('Outra coisa')
  })

  it('formata dados como JSON indentado', () => {
    expect(formatarDados({ papel: 'admin' })).toBe('{\n  "papel": "admin"\n}')
    expect(formatarDados(null)).toBe('—')
    expect(temDados({})).toBe(false)
    expect(temDados({ a: 1 })).toBe(true)
  })
})
