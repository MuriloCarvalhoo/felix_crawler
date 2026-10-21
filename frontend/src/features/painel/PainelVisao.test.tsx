import { screen, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { formatarBRL } from '@/lib/formatos'
import type { Nota, Painel } from '@/lib/tipos'
import { ORG_ID, renderizarComRotas } from '../empenhos/testeUtil'
import { normalizarAging } from './GraficoAging'

/** Texto como o Testing Library normaliza (NBSP vira espaço). */
const brl = (c: number) => formatarBRL(c).replace(/\u00a0/g, ' ')
const re = (t: string) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
import { PainelVisao } from './PainelVisao'

afterEach(() => {
  vi.unstubAllGlobals()
})

function nota(parcial: Partial<Nota>): Nota {
  return {
    id: 'n1',
    empenho_id: 'e1',
    empenho_numero: '2026NE000123',
    ug: '160203',
    ug_nome: 'Comando da 1ª RM',
    empresa_cnpj: '12345678000195',
    numero: '101',
    tipo: 'produto',
    valor_centavos: 150_000,
    data_emissao: '2026-08-01',
    data_recebimento: null,
    vencimento: '2026-08-31',
    status: 'emitida',
    dias_atraso: 0,
    valor_pago_centavos: 0,
    data_liquidacao: null,
    data_pagamento: null,
    documento_liquidacao: null,
    documento_pagamento: null,
    criado_em: '2026-08-01T12:00:00Z',
    ...parcial,
  }
}

const PAINEL: Painel = {
  a_receber_centavos: 4_550_000,
  vencido_centavos: 1_230_000,
  vence_30d_centavos: 870_050,
  recebido_mes_centavos: 312_345,
  notas_abertas: 12,
  notas_vencidas: 3,
  empenhos_ativos: 7,
  saldo_empenhos_centavos: 9_800_000,
  aging: [
    // fora de ordem e sem a faixa 61_90: o gráfico normaliza
    { faixa: '90_mais', centavos: 230_000, quantidade: 1 },
    { faixa: 'a_vencer', centavos: 3_320_000, quantidade: 9 },
    { faixa: '1_30', centavos: 800_000, quantidade: 1 },
    { faixa: '31_60', centavos: 200_000, quantidade: 1 },
  ],
  por_ug: [
    { ug: '160203', ug_nome: 'Comando da 1ª RM', aberto_centavos: 3_000_000, vencido_centavos: 1_000_000, atraso_medio_dias: 18.4 },
    { ug: '120001', ug_nome: 'Base Aérea', aberto_centavos: 1_550_000, vencido_centavos: 0, atraso_medio_dias: 0 },
  ],
  proximos_vencimentos: [nota({ id: 'n2', numero: '202', vencimento: '2026-10-25' })],
  mais_atrasadas: [nota({ id: 'n3', numero: '303', dias_atraso: 95, vencimento: '2026-07-17' })],
}

describe('PainelVisao', () => {
  it('mostra os indicadores com valores em reais e contagens', async () => {
    renderizarComRotas(<PainelVisao orgId={ORG_ID} painel={PAINEL} podeCriar />)
    const indicadores = await screen.findByRole('region', { name: 'Indicadores' })
    const em = within(indicadores)
    expect(em.getByText('A receber')).toBeInTheDocument()
    expect(em.getByText(brl(4_550_000))).toBeInTheDocument()
    expect(em.getByText(brl(1_230_000))).toBeInTheDocument()
    expect(em.getByText(brl(870_050))).toBeInTheDocument()
    expect(em.getByText(brl(312_345))).toBeInTheDocument()
    expect(em.getByText(brl(9_800_000))).toBeInTheDocument()
    expect(em.getByText('12 notas abertas')).toBeInTheDocument()
    expect(em.getByRole('link', { name: '3 notas vencidas' })).toHaveAttribute('href', '/o/o1/notas?situacao=vencida')
    expect(em.getByText('7 empenhos ativos a faturar')).toBeInTheDocument()
  })

  it('desenha o aging nas 5 faixas, em ordem, com valor e quantidade', async () => {
    renderizarComRotas(<PainelVisao orgId={ORG_ID} painel={PAINEL} podeCriar />)
    const tabela = await screen.findByRole('table', { name: 'Valores em aberto por faixa de atraso' })
    const faixas = within(tabela)
      .getAllByRole('rowheader')
      .map((c) => c.textContent)
    expect(faixas).toEqual(['A vencer', '1 a 30 dias', '31 a 60 dias', '61 a 90 dias', 'Mais de 90 dias'])
    expect(within(tabela).getByLabelText(new RegExp(`^1 a 30 dias: ${re(brl(800_000))}, 1 nota`))).toBeInTheDocument()
    expect(within(tabela).getByLabelText(new RegExp(`^61 a 90 dias: ${re(brl(0))}, 0 notas`))).toBeInTheDocument()
    expect(within(tabela).getByLabelText(new RegExp(`^A vencer: ${re(brl(3_320_000))}, 9 notas, 73%`))).toBeInTheDocument()
  })

  it('lista UGs, próximos vencimentos e mais atrasadas com link para a nota', async () => {
    renderizarComRotas(<PainelVisao orgId={ORG_ID} painel={PAINEL} podeCriar />)
    const ugs = await screen.findByRole('table', { name: 'Valores em aberto por unidade gestora' })
    const linhas = within(ugs).getAllByRole('row').slice(1)
    expect(linhas[0]).toHaveTextContent('160203')
    expect(linhas[0]).toHaveTextContent('18 dias')
    expect(screen.getByRole('link', { name: 'NF 202' })).toHaveAttribute('href', '/o/o1/notas/n2')
    expect(screen.getByRole('link', { name: 'NF 303' })).toHaveAttribute('href', '/o/o1/notas/n3')
    expect(screen.getByText('Vencida há 95 dias')).toBeInTheDocument()
  })

  it('sem dados, orienta a cadastrar o primeiro empenho', async () => {
    const vazio: Painel = {
      ...PAINEL,
      a_receber_centavos: 0,
      vencido_centavos: 0,
      vence_30d_centavos: 0,
      recebido_mes_centavos: 0,
      notas_abertas: 0,
      notas_vencidas: 0,
      empenhos_ativos: 0,
      saldo_empenhos_centavos: 0,
      aging: [],
      por_ug: [],
      proximos_vencimentos: [],
      mais_atrasadas: [],
    }
    renderizarComRotas(<PainelVisao orgId={ORG_ID} painel={vazio} podeCriar />)
    expect(await screen.findByText('Cadastre o primeiro empenho')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Importar PDFs de empenho' })).toHaveAttribute('href', '/o/o1/empenhos/novo')
    expect(screen.getByRole('link', { name: 'Cadastrar manualmente' })).toHaveAttribute('href', '/o/o1/empenhos/novo?aba=manual')
  })
})

describe('normalizarAging', () => {
  it('completa faixas ausentes com zero', () => {
    expect(normalizarAging([]).map((i) => [i.faixa, i.centavos])).toEqual([
      ['a_vencer', 0],
      ['1_30', 0],
      ['31_60', 0],
      ['61_90', 0],
      ['90_mais', 0],
    ])
  })
})
