import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { vi } from 'vitest'
import type { Notificacao } from '@/lib/tipos'
import { SinoNotificacoes } from './SinoNotificacoes'
import { json, mockarFetch, prepararDom, renderizar, semConteudo } from './testeUtil'

const BASE = '/api/v1/orgs/o1/notificacoes'

function notificacao(id: string, extra: Partial<Notificacao> = {}): Notificacao {
  return {
    id,
    tipo: 'vence_5d',
    titulo: `NF ${id} vence em 5 dias`,
    corpo: 'A nota 123 do empenho 2026NE000001 vence em 25/10/2026.',
    link: `/o/o1/notas/${id}`,
    lida_em: null,
    criado_em: new Date(Date.now() - 2 * 3600_000).toISOString(),
    ...extra,
  }
}

function mockarServidor(inicial: number) {
  let naoLidas = inicial
  const lista = [notificacao('n1'), notificacao('n2', { tipo: 'vencida', titulo: 'NF n2 vencida' })]
  return mockarFetch([
    {
      caminho: BASE,
      responder: (url) => {
        const soNaoLidas = url.searchParams.get('nao_lidas') === 'true'
        const dados = soNaoLidas ? lista.slice(0, Math.min(naoLidas, 1)) : lista
        return json({
          dados,
          meta: {
            total: soNaoLidas ? naoLidas : lista.length,
            pagina: 1,
            por_pagina: Number(url.searchParams.get('por_pagina') ?? 25),
            nao_lidas: naoLidas,
          },
        })
      },
    },
    {
      metodo: 'POST',
      caminho: `${BASE}/lidas`,
      responder: () => {
        naoLidas = 0
        return semConteudo()
      },
    },
    {
      metodo: 'POST',
      caminho: /\/notificacoes\/[^/]+\/lida$/,
      responder: () => {
        naoLidas = Math.max(0, naoLidas - 1)
        return semConteudo()
      },
    },
  ])
}

beforeEach(() => {
  prepararDom()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('SinoNotificacoes', () => {
  it('mostra o contador de não lidas e marca todas como lidas', async () => {
    const usuario = userEvent.setup()
    const { chamadas } = mockarServidor(3)
    renderizar(<SinoNotificacoes orgId="o1" />)

    const sino = await screen.findByRole('button', { name: 'Notificações, 3 não lidas' })
    expect(sino).toHaveTextContent('3')
    const contador = chamadas.find((c) => c.busca.get('nao_lidas') === 'true')
    expect(contador?.busca.get('por_pagina')).toBe('1')

    await usuario.click(sino)
    expect(await screen.findByText('NF n1 vence em 5 dias')).toBeInTheDocument()
    expect(screen.getByText('NF n2 vencida')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Ver todas' })).toHaveAttribute(
      'href',
      '/o/o1/notificacoes',
    )

    await usuario.click(screen.getByRole('button', { name: 'Marcar todas como lidas' }))

    await waitFor(() =>
      expect(chamadas.some((c) => c.metodo === 'POST' && c.caminho === `${BASE}/lidas`)).toBe(true),
    )
    expect(await screen.findByRole('button', { name: 'Notificações' })).toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: 'Marcar todas como lidas' }),
    ).not.toBeInTheDocument()
  })

  it('ao clicar numa notificação marca como lida e navega para o link', async () => {
    const usuario = userEvent.setup()
    const { chamadas } = mockarServidor(2)
    const { router } = renderizar(<SinoNotificacoes orgId="o1" />)

    await usuario.click(await screen.findByRole('button', { name: 'Notificações, 2 não lidas' }))
    await usuario.click(await screen.findByText('NF n1 vence em 5 dias'))

    await waitFor(() =>
      expect(chamadas.some((c) => c.metodo === 'POST' && c.caminho === `${BASE}/n1/lida`)).toBe(
        true,
      ),
    )
    await waitFor(() => expect(router.state.location.pathname).toBe('/o/o1/notas/n1'))
  })
})
