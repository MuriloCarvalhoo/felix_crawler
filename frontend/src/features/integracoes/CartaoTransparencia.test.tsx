import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { criarOrg, json, mockarFetch, prepararDom, renderizar } from '@/features/notificacoes/testeUtil'
import type { IntegracaoStatus } from '@/lib/tipos'
import { CartaoTransparencia } from './CartaoTransparencia'

// Polling rápido no teste (o real é de 3 s).
vi.mock('./consultas', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./consultas')>()),
  INTERVALO_SINCRONIZACAO_MS: 20,
}))

const BASE = '/api/v1/orgs/o1/integracoes/transparencia'

const NORMAL: IntegracaoStatus = {
  configurada_no_servidor: true,
  permitida_no_plano: true,
  ativa: true,
  ultima_sincronizacao_em: '2026-10-19T09:00:00Z',
  ultimo_status: 'ok',
  ultimo_erro: null,
  pendentes: 0,
}

const PARCIAL: IntegracaoStatus = {
  ...NORMAL,
  ultima_sincronizacao_em: '2026-10-20T09:00:00Z',
  ultimo_status: 'parcial',
  ultimo_erro: 'CNPJ 12.345.678/0001-95: o Portal não respondeu.',
  sincronizando: false,
}

beforeEach(() => {
  prepararDom()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('CartaoTransparencia: status parcial', () => {
  it('mostra badge âmbar "Parcial" e o último erro', async () => {
    mockarFetch([{ caminho: BASE, responder: () => json({ dados: PARCIAL }) }])
    renderizar(<CartaoTransparencia org={criarOrg('admin')} />)

    expect(await screen.findByText('Parcial')).toBeInTheDocument()
    expect(screen.getByText('A última sincronização ficou incompleta')).toBeInTheDocument()
    expect(screen.getByText(PARCIAL.ultimo_erro ?? '')).toBeInTheDocument()
  })

  it('sincronização que termina parcial avisa com o último erro (polling pela própria consulta)', async () => {
    const usuario = userEvent.setup()
    let iniciada = false
    const { chamadas } = mockarFetch([
      { caminho: BASE, responder: () => json({ dados: iniciada ? PARCIAL : NORMAL }) },
      {
        metodo: 'POST',
        caminho: `${BASE}/sincronizar`,
        responder: () => {
          iniciada = true
          return json({ dados: { job_id: 'j1' } }, 202)
        },
      },
    ])
    renderizar(<CartaoTransparencia org={criarOrg('admin')} />)

    await usuario.click(await screen.findByRole('button', { name: 'Sincronizar agora' }))

    expect(await screen.findByText('Sincronização concluída com pendências')).toBeInTheDocument()
    // No aviso (toast) e no quadro do cartão.
    await waitFor(() => expect(screen.getAllByText(PARCIAL.ultimo_erro ?? '')).toHaveLength(2))
    expect(screen.getByText('Parcial')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Sincronizar agora' })).toBeEnabled()

    // Concluído: o polling para.
    const antes = chamadas.filter((c) => c.metodo === 'GET' && c.caminho === BASE).length
    await new Promise((r) => setTimeout(r, 80))
    expect(chamadas.filter((c) => c.metodo === 'GET' && c.caminho === BASE).length).toBe(antes)
  })
})
