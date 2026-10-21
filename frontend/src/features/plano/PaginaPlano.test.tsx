import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { corpoDaChamada, instalarFetch, json, ORG_ID, renderizarComRotas } from '@/features/empenhos/testeUtil'
import { prepararDom } from '@/features/notificacoes/testeUtil'
import type { Assinatura } from '@/lib/tipos'
import { PaginaPlano } from './PaginaPlano'
import { LIMITES_PLANOS } from './planos'

const ASSINATURA: Assinatura = {
  plano: 'profissional',
  status: 'teste',
  teste_ate: '2026-11-03T15:00:00Z',
  provedor: 'manual',
  limites: LIMITES_PLANOS.profissional,
  uso: { cnpjs: 1, empenhos_ativos: 3, usuarios: 1 },
}

const CAMINHO = `/api/v1/orgs/${ORG_ID}/assinatura`

beforeEach(() => {
  prepararDom()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function cartao(nome: string): HTMLElement {
  const titulo = screen.getByRole('heading', { level: 3, name: nome })
  const item = titulo.closest('li')
  if (!item) throw new Error(`Cartão ${nome} não encontrado`)
  return item
}

describe('PaginaPlano', () => {
  it('sem troca paga disponível: planos pagos sem botão, aviso no lugar e sem "sem custo"', async () => {
    instalarFetch((url, init) =>
      (init?.method ?? 'GET') === 'GET' && url.pathname === CAMINHO
        ? json({ dados: { ...ASSINATURA, troca_paga_disponivel: false } })
        : undefined,
    )
    renderizarComRotas(<PaginaPlano />, 'dono')

    expect(await screen.findByRole('heading', { level: 3, name: 'Empresarial' })).toBeInTheDocument()
    expect(within(cartao('Empresarial')).queryByRole('button')).not.toBeInTheDocument()
    expect(within(cartao('Profissional')).queryByRole('button')).not.toBeInTheDocument()
    expect(within(cartao('Empresarial')).getByText(/Planos pagos estarão disponíveis quando o pagamento online for ativado/)).toBeInTheDocument()
    expect(within(cartao('Gratuito')).getByRole('button', { name: 'Mudar para este plano' })).toBeEnabled()
    expect(screen.queryByText(/sem custo/)).not.toBeInTheDocument()
  })

  it('campo ausente vale como disponível; 403 proibido mostra mensagem específica', async () => {
    const usuario = userEvent.setup()
    const fetch = instalarFetch((url, init) => {
      if (url.pathname !== CAMINHO) return undefined
      if (init?.method === 'POST') return json({ erro: { codigo: 'proibido', mensagem: 'Proibido.' } }, 403)
      return json({ dados: ASSINATURA })
    })
    renderizarComRotas(<PaginaPlano />, 'dono')

    expect(await screen.findByText(/sem custo/)).toBeInTheDocument()
    await usuario.click(within(cartao('Empresarial')).getByRole('button', { name: 'Mudar para este plano' }))
    await usuario.click(await screen.findByRole('button', { name: 'Confirmar mudança' }))

    expect(await screen.findByText('Não foi possível mudar de plano')).toBeInTheDocument()
    expect(screen.getAllByText('Planos pagos estarão disponíveis quando o pagamento online for ativado.').length).toBeGreaterThan(0)
    await waitFor(() => expect(corpoDaChamada(fetch, 'POST', CAMINHO)).toEqual({ plano: 'empresarial' }))
  })
})
