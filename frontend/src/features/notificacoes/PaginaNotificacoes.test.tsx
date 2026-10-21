import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { PaginaNotificacoes } from './PaginaNotificacoes'
import { criarOrg, json, mockarFetch, prepararDom, renderizar, semConteudo } from './testeUtil'

const BASE = '/api/v1/orgs/o1/notificacoes'

beforeEach(() => {
  prepararDom()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('PaginaNotificacoes', () => {
  it('página além do fim (lista vazia com total > 0) volta para a última página', async () => {
    mockarFetch([
      {
        caminho: BASE,
        responder: () => json({ dados: [], meta: { total: 30, pagina: 4, por_pagina: 25, nao_lidas: 0 } }),
      },
    ])
    const aoMudarBusca = vi.fn()
    renderizar(
      <PaginaNotificacoes org={criarOrg('leitor')} busca={{ filtro: 'todas', pagina: 4 }} aoMudarBusca={aoMudarBusca} />,
    )
    await waitFor(() => expect(aoMudarBusca).toHaveBeenCalledWith({ filtro: 'todas', pagina: 2 }))
  })

  it('estado vazio explica o horário dos alertas e admin gera alertas agora', async () => {
    const usuario = userEvent.setup()
    const { chamadas } = mockarFetch([
      {
        caminho: BASE,
        responder: () =>
          json({ dados: [], meta: { total: 0, pagina: 1, por_pagina: 25, nao_lidas: 0 } }),
      },
      {
        metodo: 'POST',
        caminho: '/api/v1/orgs/o1/alertas/gerar',
        responder: () => json({ dados: { criadas: 2 } }),
      },
    ])
    renderizar(
      <PaginaNotificacoes
        org={criarOrg('admin')}
        busca={{ filtro: 'todas', pagina: 1 }}
        aoMudarBusca={vi.fn()}
      />,
    )

    expect(await screen.findByText(/06:00 \(horário de Brasília\)/)).toBeInTheDocument()
    await usuario.click(
      screen.getAllByRole('button', { name: 'Gerar alertas agora' })[0] as HTMLElement,
    )

    expect(await screen.findByText('2 alertas criados')).toBeInTheDocument()
    expect(chamadas.some((c) => c.metodo === 'POST' && c.caminho.endsWith('/alertas/gerar'))).toBe(
      true,
    )
  })

  it('leitor não vê "Gerar alertas agora" e marca uma como lida', async () => {
    const usuario = userEvent.setup()
    const { chamadas } = mockarFetch([
      {
        caminho: BASE,
        responder: () =>
          json({
            dados: [
              {
                id: 'n1',
                tipo: 'vencida',
                titulo: 'NF 123 vencida',
                corpo: 'Venceu em 10/10/2026.',
                link: '/o/o1/notas/x',
                lida_em: null,
                criado_em: new Date().toISOString(),
              },
            ],
            meta: { total: 1, pagina: 1, por_pagina: 25, nao_lidas: 1 },
          }),
      },
      { metodo: 'POST', caminho: `${BASE}/n1/lida`, responder: () => semConteudo() },
    ])
    renderizar(
      <PaginaNotificacoes
        org={criarOrg('leitor')}
        busca={{ filtro: 'nao_lidas', pagina: 1 }}
        aoMudarBusca={vi.fn()}
      />,
    )

    expect(await screen.findByText('NF 123 vencida')).toBeInTheDocument()
    expect(screen.getByRole('img', { name: 'Nota vencida' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Gerar alertas agora' })).not.toBeInTheDocument()
    expect(chamadas[0]?.busca.get('nao_lidas')).toBe('true')

    await usuario.click(screen.getByRole('button', { name: 'Marcar como lida' }))
    await waitFor(() =>
      expect(chamadas.some((c) => c.metodo === 'POST' && c.caminho === `${BASE}/n1/lida`)).toBe(
        true,
      ),
    )
  })
})
