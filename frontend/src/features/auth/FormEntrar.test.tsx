import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from '@/lib/tipos'
import { FormEntrar } from './FormEntrar'

const ME: Me = {
  usuario: { id: 'u1', nome: 'Ana Souza', email: 'ana@empresa.com.br' },
  organizacoes: [{ id: 'o1', nome: 'Ana Ltda', papel: 'dono', plano: 'profissional' }],
}

const fetchMock = vi.fn<typeof fetch>()

function json(corpo: unknown, status = 200) {
  return new Response(JSON.stringify(corpo), { status, headers: { 'Content-Type': 'application/json' } })
}

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('FormEntrar', () => {
  it('valida campos antes de enviar', async () => {
    const usuario = userEvent.setup()
    const aoEntrar = vi.fn()
    render(<FormEntrar aoEntrar={aoEntrar} />)

    await usuario.click(screen.getByRole('button', { name: 'Entrar' }))

    expect(await screen.findByText('Informe o e-mail.')).toBeInTheDocument()
    expect(screen.getByText('Informe a senha.')).toBeInTheDocument()
    expect(screen.getByLabelText('E-mail')).toHaveAttribute('aria-invalid', 'true')
    expect(fetchMock).not.toHaveBeenCalled()
    expect(aoEntrar).not.toHaveBeenCalled()

    await usuario.type(screen.getByLabelText('E-mail'), 'ana@')
    expect(await screen.findByText(/Informe um e-mail válido/)).toBeInTheDocument()
  })

  it('envia credenciais e devolve o Me', async () => {
    const usuario = userEvent.setup()
    const aoEntrar = vi.fn()
    fetchMock.mockResolvedValueOnce(json({ dados: ME }))
    render(<FormEntrar aoEntrar={aoEntrar} />)

    await usuario.type(screen.getByLabelText('E-mail'), ' ana@empresa.com.br ')
    await usuario.type(screen.getByLabelText('Senha'), 'senha-muito-segura')
    await usuario.click(screen.getByRole('button', { name: 'Entrar' }))

    await waitFor(() => expect(aoEntrar).toHaveBeenCalledWith(ME))
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(url).toBe('/api/v1/auth/login')
    expect((init?.headers as Record<string, string>)['X-Felix']).toBe('1')
    expect(JSON.parse(init?.body as string)).toEqual({ email: 'ana@empresa.com.br', senha: 'senha-muito-segura' })
  })

  it('mostra erro genérico para credenciais inválidas e limpa a senha', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(json({ erro: { codigo: 'credenciais_invalidas', mensagem: 'Credenciais inválidas.' } }, 401))
    render(<FormEntrar aoEntrar={vi.fn()} />)

    await usuario.type(screen.getByLabelText('E-mail'), 'ana@empresa.com.br')
    await usuario.type(screen.getByLabelText('Senha'), 'errada-123456')
    await usuario.click(screen.getByRole('button', { name: 'Entrar' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('E-mail ou senha incorretos.')
    expect(screen.getByLabelText('Senha')).toHaveValue('')
  })

  it('distribui erros de campo vindos do servidor', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(
      json({ erro: { codigo: 'validacao', mensagem: 'Confira os campos.', campos: { email: 'E-mail bloqueado.' } } }, 422),
    )
    render(<FormEntrar aoEntrar={vi.fn()} />)

    await usuario.type(screen.getByLabelText('E-mail'), 'ana@empresa.com.br')
    await usuario.type(screen.getByLabelText('Senha'), 'qualquer-coisa')
    await usuario.click(screen.getByRole('button', { name: 'Entrar' }))

    expect(await screen.findByText('E-mail bloqueado.')).toBeInTheDocument()
    expect(screen.getByLabelText('E-mail')).toHaveAttribute('aria-invalid', 'true')

    // Ao editar o campo, o erro do servidor some.
    await usuario.type(screen.getByLabelText('E-mail'), 'x')
    await waitFor(() => expect(screen.queryByText('E-mail bloqueado.')).not.toBeInTheDocument())
  })
})
