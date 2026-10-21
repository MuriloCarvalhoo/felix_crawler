import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Empenho, Empresa, ResultadoAnalise } from '@/lib/tipos'
import { CartaoConferencia } from './CartaoConferencia'
import { corpoDaChamada, instalarFetch, json, ORG_ID, renderizarComRotas } from './testeUtil'

afterEach(() => {
  vi.unstubAllGlobals()
})

const EMPRESAS: Empresa[] = [
  { id: 'emp-a', razao_social: 'Alfa Comércio Ltda', cnpj: '11222333000181', criado_em: '2026-01-01T00:00:00Z' },
  { id: 'emp-b', razao_social: 'Beta Serviços Ltda', cnpj: '12345678000195', criado_em: '2026-01-01T00:00:00Z' },
]

const RESULTADO: ResultadoAnalise = {
  arquivo: 'NE000123.pdf',
  analise: {
    perfil: 'siafi',
    dados: {
      ug: '160203',
      gestao: '00001',
      empenho: '2025NE000123',
      cnpj_fornecedor: '12345678000195',
      cnpj_emitente: null,
      fornecedor: 'BETA SERVICOS LTDA',
      valor_total_centavos: 1_234_567,
      data_emissao: '2025-03-10',
      modalidade: 'Pregão',
      origem: 'licitacao',
      numero_edital_ata: '12/2025',
    },
    avisos: ['Valor por extenso diverge do valor numérico.'],
  },
  empresa_id_sugerida: 'emp-b',
  ja_cadastrado: false,
}

const EMPENHO_SALVO = { id: 'novo-1', numero: '2025NE000123' } as Empenho

describe('CartaoConferencia', () => {
  it('preenche os campos extraídos, destaca avisos e pré-seleciona a empresa sugerida', async () => {
    instalarFetch()
    renderizarComRotas(<CartaoConferencia orgId={ORG_ID} resultado={RESULTADO} empresas={EMPRESAS} />)

    expect(await screen.findByLabelText(/Número do empenho/)).toHaveValue('2025NE000123')
    expect(screen.getByLabelText(/Empresa fornecedora/)).toHaveValue('emp-b')
    expect(screen.getByLabelText(/^UG/)).toHaveValue('160203')
    expect(screen.getByLabelText(/Gestão/)).toHaveValue('00001')
    expect(screen.getByLabelText(/Valor total/)).toHaveValue('12.345,67')
    expect(screen.getByLabelText(/Data de emissão/)).toHaveValue('2025-03-10')
    expect(screen.getByLabelText(/Prazo em dias/)).toHaveValue('30')
    expect(screen.getByRole('note')).toHaveTextContent('Valor por extenso diverge do valor numérico.')
    expect(screen.getByText('Empresa identificada pelo CNPJ: Beta Serviços Ltda.')).toBeInTheDocument()
  })

  it('envia o NovoEmpenho com centavos, empresa sugerida, edições e origem pdf', async () => {
    const usuario = userEvent.setup()
    const aoSalvar = vi.fn()
    const fetch = instalarFetch((url, init) =>
      init?.method === 'POST' && url.pathname === `/api/v1/orgs/${ORG_ID}/empenhos` ? json({ dados: EMPENHO_SALVO }, 201) : undefined,
    )
    renderizarComRotas(<CartaoConferencia orgId={ORG_ID} resultado={RESULTADO} empresas={EMPRESAS} aoSalvar={aoSalvar} />)

    await usuario.type(await screen.findByLabelText(/Nome da UG/), 'Comando da 1ª RM')
    await usuario.clear(screen.getByLabelText(/Prazo em dias/))
    await usuario.type(screen.getByLabelText(/Prazo em dias/), '45')
    await usuario.click(screen.getByRole('button', { name: 'Salvar empenho' }))

    await waitFor(() => expect(aoSalvar).toHaveBeenCalledWith(EMPENHO_SALVO))
    expect(corpoDaChamada(fetch, 'POST', `/api/v1/orgs/${ORG_ID}/empenhos`)).toEqual({
      empresa_id: 'emp-b',
      ug: '160203',
      ug_nome: 'Comando da 1ª RM',
      orgao: null,
      gestao: '00001',
      numero: '2025NE000123',
      modalidade: 'Pregão',
      numero_edital_ata: '12/2025',
      valor_total_centavos: 1_234_567,
      data_emissao: '2025-03-10',
      regra_vencimento: 'corridos_emissao',
      prazo_dias: 45,
      origem: 'pdf',
      observacao: null,
    })
    expect(await screen.findByText('Salvo')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Abrir empenho' })).toHaveAttribute('href', `/o/${ORG_ID}/empenhos/novo-1`)
  })

  it('mostra no cartão o conflito 409 e a explicação da IN 77 sem prazo', async () => {
    const usuario = userEvent.setup()
    const fetch = instalarFetch((url, init) =>
      init?.method === 'POST' && url.pathname.endsWith('/empenhos')
        ? json({ erro: { codigo: 'conflito', mensagem: 'Já existe o empenho 2025NE000123 da UG 160203 nesta organização.' } }, 409)
        : undefined,
    )
    renderizarComRotas(
      <CartaoConferencia orgId={ORG_ID} resultado={{ ...RESULTADO, ja_cadastrado: true }} empresas={EMPRESAS} />,
    )

    expect(await screen.findByText('Já cadastrado')).toBeInTheDocument()
    await usuario.selectOptions(screen.getByLabelText(/Regra de vencimento/), 'in77_uteis')
    expect(screen.queryByLabelText(/Prazo em dias/)).not.toBeInTheDocument()
    expect(screen.getByText(/IN SEGES 77\/2022/)).toBeInTheDocument()
    await usuario.click(screen.getByRole('button', { name: 'Salvar empenho' }))

    expect(await screen.findByText(/Já existe o empenho 2025NE000123/)).toHaveAttribute('role', 'alert')
    const corpo = corpoDaChamada(fetch, 'POST', `/api/v1/orgs/${ORG_ID}/empenhos`) as Record<string, unknown>
    expect(corpo.regra_vencimento).toBe('in77_uteis')
    expect(corpo).not.toHaveProperty('prazo_dias')
  })

  it('sem empresa correspondente, avisa e aponta para Configurações › Empresas', async () => {
    instalarFetch()
    renderizarComRotas(
      <CartaoConferencia orgId={ORG_ID} resultado={{ ...RESULTADO, empresa_id_sugerida: null }} empresas={EMPRESAS} />,
    )
    expect(await screen.findByLabelText(/Empresa fornecedora/)).toHaveValue('')
    expect(screen.getByRole('link', { name: /cadastre o CNPJ em Configurações › Empresas/ })).toHaveAttribute(
      'href',
      `/o/${ORG_ID}/configuracoes/empresas`,
    )
  })
})
