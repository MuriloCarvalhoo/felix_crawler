import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ToastProvider } from '@/components/ui'
import { EmpresaDialog } from './EmpresaDialog'

const fetchMock = vi.fn<typeof fetch>()

function renderizar(aoMudar = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={qc}>
      <ToastProvider>
        <EmpresaDialog orgId="o1" aberto aoMudar={aoMudar} />
      </ToastProvider>
    </QueryClientProvider>,
  )
  return aoMudar
}

function json(corpo: unknown, status = 200) {
  return new Response(JSON.stringify(corpo), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('EmpresaDialog', () => {
  it('recusa CNPJ com dígito verificador errado', async () => {
    const usuario = userEvent.setup()
    renderizar()

    await usuario.type(screen.getByLabelText(/^CNPJ/), '12abc34501de36')
    await usuario.type(screen.getByLabelText(/Razão social/), 'Fornecedora Alfa')
    await usuario.click(screen.getByRole('button', { name: 'Adicionar CNPJ' }))

    expect(await screen.findByText(/CNPJ inválido/)).toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('aceita CNPJ alfanumérico, mascara em maiúsculas e envia sem máscara', async () => {
    const usuario = userEvent.setup()
    fetchMock
      .mockResolvedValueOnce(
        json(
          {
            dados: {
              id: 'e1',
              razao_social: 'Fornecedora Alfa',
              cnpj: '12ABC34501DE35',
              criado_em: '2026-10-20T12:00:00Z',
            },
          },
          201,
        ),
      )
      .mockResolvedValue(json({ dados: [] }))
    const aoMudar = renderizar()

    const campo = screen.getByLabelText(/^CNPJ/)
    await usuario.type(campo, '12abc34501de35')
    expect(campo).toHaveValue('12.ABC.345/01DE-35')
    await usuario.type(screen.getByLabelText(/Razão social/), 'Fornecedora Alfa')
    await usuario.click(screen.getByRole('button', { name: 'Adicionar CNPJ' }))

    await waitFor(() => expect(aoMudar).toHaveBeenCalledWith(false))
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(url).toBe('/api/v1/orgs/o1/empresas')
    expect(JSON.parse(init?.body as string)).toEqual({
      razao_social: 'Fornecedora Alfa',
      cnpj: '12ABC34501DE35',
    })
  })

  it('mostra CNPJ duplicado (409) no campo', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(
      json(
        { erro: { codigo: 'conflito', mensagem: 'CNPJ já cadastrado nesta organização.' } },
        409,
      ),
    )
    renderizar()

    await usuario.type(screen.getByLabelText(/^CNPJ/), '11.222.333/0001-81')
    await usuario.type(screen.getByLabelText(/Razão social/), 'Fornecedora Beta')
    await usuario.click(screen.getByRole('button', { name: 'Adicionar CNPJ' }))

    expect(await screen.findByText('CNPJ já cadastrado nesta organização.')).toBeInTheDocument()
    expect(screen.getByLabelText(/^CNPJ/)).toHaveAttribute('aria-invalid', 'true')
  })
})
