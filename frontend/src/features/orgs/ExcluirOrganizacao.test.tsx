import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ToastProvider } from '@/components/ui'
import { ExcluirOrganizacao } from './ExcluirOrganizacao'

const fetchMock = vi.fn<typeof fetch>()

function renderizar(aoExcluir = vi.fn(() => Promise.resolve())) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <ExcluirOrganizacao orgId="o1" nome="Ana Ltda" aoExcluir={aoExcluir} />
      </ToastProvider>
    </QueryClientProvider>,
  )
  return aoExcluir
}

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ExcluirOrganizacao', () => {
  it('só habilita a exclusão com o nome exato e envia a confirmação', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }))
    const aoExcluir = renderizar()

    await usuario.click(screen.getByRole('button', { name: 'Excluir organização' }))
    const confirmar = await screen.findByRole('button', { name: 'Excluir definitivamente' })
    expect(confirmar).toBeDisabled()

    const campo = screen.getByLabelText('Nome da organização')
    await usuario.type(campo, 'ana ltda')
    expect(confirmar).toBeDisabled()

    await usuario.clear(campo)
    await usuario.type(campo, 'Ana Ltd')
    expect(confirmar).toBeDisabled()

    await usuario.type(campo, 'a')
    expect(confirmar).toBeEnabled()
    await usuario.click(confirmar)

    await waitFor(() => expect(aoExcluir).toHaveBeenCalledTimes(1))
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(url).toBe('/api/v1/orgs/o1')
    expect(init?.method).toBe('DELETE')
    expect((init?.headers as Record<string, string>)['X-Felix']).toBe('1')
    expect(JSON.parse(init?.body as string)).toEqual({ confirmacao: 'Ana Ltda' })
  })

  it('mantém o diálogo aberto e mostra o erro do servidor', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          erro: {
            codigo: 'validacao',
            mensagem: 'Confira os campos.',
            campos: {
              confirmacao: 'Digite exatamente o nome da organização para confirmar a exclusão.',
            },
          },
        }),
        { status: 422, headers: { 'Content-Type': 'application/json' } },
      ),
    )
    const aoExcluir = renderizar()

    await usuario.click(screen.getByRole('button', { name: 'Excluir organização' }))
    await usuario.type(await screen.findByLabelText('Nome da organização'), 'Ana Ltda')
    await usuario.click(screen.getByRole('button', { name: 'Excluir definitivamente' }))

    expect(
      await screen.findByText('Digite exatamente o nome da organização para confirmar a exclusão.'),
    ).toBeInTheDocument()
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    expect(aoExcluir).not.toHaveBeenCalled()
  })
})
