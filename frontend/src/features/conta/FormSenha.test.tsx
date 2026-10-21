import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactElement } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ToastProvider } from '@/components/ui'
import { FormSenha } from './FormSenha'

const fetchMock = vi.fn<typeof fetch>()

function renderizar(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={qc}>
      <ToastProvider>{ui}</ToastProvider>
    </QueryClientProvider>,
  )
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

describe('FormSenha', () => {
  it('exige confirmação igual à nova senha e não envia', async () => {
    const usuario = userEvent.setup()
    renderizar(<FormSenha />)

    await usuario.type(screen.getByLabelText(/Senha atual/), 'senha-antiga-123')
    await usuario.type(screen.getByLabelText(/^Nova senha/), 'frase-nova-e-longa')
    await usuario.type(screen.getByLabelText(/Confirme a nova senha/), 'frase-nova-diferente')
    await usuario.click(screen.getByRole('button', { name: 'Trocar senha' }))

    expect(await screen.findByText('As senhas não conferem.')).toBeInTheDocument()
    expect(screen.getByLabelText(/Confirme a nova senha/)).toHaveAttribute('aria-invalid', 'true')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('exige pelo menos 10 caracteres na nova senha', async () => {
    const usuario = userEvent.setup()
    renderizar(<FormSenha />)

    await usuario.type(screen.getByLabelText(/Senha atual/), 'senha-antiga-123')
    await usuario.type(screen.getByLabelText(/^Nova senha/), 'curta')
    await usuario.type(screen.getByLabelText(/Confirme a nova senha/), 'curta')
    await usuario.click(screen.getByRole('button', { name: 'Trocar senha' }))

    expect(await screen.findByText('Use pelo menos 10 caracteres.')).toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('envia senha atual e nova e limpa o formulário', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }))
    renderizar(<FormSenha />)

    await usuario.type(screen.getByLabelText(/Senha atual/), 'senha-antiga-123')
    await usuario.type(screen.getByLabelText(/^Nova senha/), 'frase-nova-e-longa')
    await usuario.type(screen.getByLabelText(/Confirme a nova senha/), 'frase-nova-e-longa')
    await usuario.click(screen.getByRole('button', { name: 'Trocar senha' }))

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1))
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(url).toBe('/api/v1/auth/me/senha')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({
      senha_atual: 'senha-antiga-123',
      senha_nova: 'frase-nova-e-longa',
    })
    expect(await screen.findByText('Senha alterada')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByLabelText(/Senha atual/)).toHaveValue(''))
  })

  it('mostra no campo o erro de senha atual incorreta', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(
      json(
        {
          erro: {
            codigo: 'validacao',
            mensagem: 'Confira os campos.',
            campos: { senha_atual: 'Senha atual incorreta.' },
          },
        },
        422,
      ),
    )
    renderizar(<FormSenha />)

    await usuario.type(screen.getByLabelText(/Senha atual/), 'errada-123456')
    await usuario.type(screen.getByLabelText(/^Nova senha/), 'frase-nova-e-longa')
    await usuario.type(screen.getByLabelText(/Confirme a nova senha/), 'frase-nova-e-longa')
    await usuario.click(screen.getByRole('button', { name: 'Trocar senha' }))

    expect(await screen.findByText('Senha atual incorreta.')).toBeInTheDocument()
    expect(screen.getByLabelText(/Senha atual/)).toHaveAttribute('aria-invalid', 'true')
  })
})
