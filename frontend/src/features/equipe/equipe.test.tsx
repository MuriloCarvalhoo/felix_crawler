import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactElement } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ToastProvider } from '@/components/ui'
import type { Membro, Papel } from '@/lib/tipos'
import { ConvidarDialog } from './ConvidarDialog'
import { TabelaMembros } from './TabelaMembros'

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

describe('ConvidarDialog', () => {
  it('valida o e-mail antes de enviar', async () => {
    const usuario = userEvent.setup()
    renderizar(<ConvidarDialog orgId="o1" aberto aoMudar={vi.fn()} podeConvidarDono />)

    await usuario.click(screen.getByRole('button', { name: 'Enviar convite' }))
    expect(await screen.findByText('Informe o e-mail.')).toBeInTheDocument()

    await usuario.type(screen.getByLabelText(/E-mail/), 'bia@')
    expect(await screen.findByText(/Informe um e-mail válido/)).toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('envia e-mail e papel escolhido e fecha o diálogo', async () => {
    const usuario = userEvent.setup()
    const aoMudar = vi.fn()
    fetchMock.mockResolvedValueOnce(
      json(
        {
          dados: {
            id: 'c1',
            email: 'bia@empresa.com.br',
            papel: 'admin',
            expira_em: '2026-10-27T12:00:00Z',
            criado_em: '2026-10-20T12:00:00Z',
          },
        },
        201,
      ),
    )
    renderizar(<ConvidarDialog orgId="o1" aberto aoMudar={aoMudar} podeConvidarDono />)

    expect(screen.getByRole('radio', { name: /Operador/ })).toBeChecked()
    await usuario.type(screen.getByLabelText(/E-mail/), ' bia@empresa.com.br ')
    await usuario.click(screen.getByRole('radio', { name: /Administrador/ }))
    await usuario.click(screen.getByRole('button', { name: 'Enviar convite' }))

    await waitFor(() => expect(aoMudar).toHaveBeenCalledWith(false))
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(url).toBe('/api/v1/orgs/o1/convites')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({
      email: 'bia@empresa.com.br',
      papel: 'admin',
    })
  })

  it('esconde a opção dono para administrador e mostra descrições dos papéis', () => {
    renderizar(<ConvidarDialog orgId="o1" aberto aoMudar={vi.fn()} podeConvidarDono={false} />)
    expect(screen.queryByRole('radio', { name: /Dono/ })).not.toBeInTheDocument()
    expect(screen.getAllByRole('radio')).toHaveLength(3)
    expect(screen.getByText('Consulta e exporta dados, sem alterar nada.')).toBeInTheDocument()
  })

  it('mostra conflito de convite duplicado no campo de e-mail', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(
      json(
        { erro: { codigo: 'conflito', mensagem: 'Esta pessoa já é membro da organização.' } },
        409,
      ),
    )
    renderizar(<ConvidarDialog orgId="o1" aberto aoMudar={vi.fn()} podeConvidarDono />)

    await usuario.type(screen.getByLabelText(/E-mail/), 'ana@empresa.com.br')
    await usuario.click(screen.getByRole('button', { name: 'Enviar convite' }))

    expect(await screen.findByText('Esta pessoa já é membro da organização.')).toBeInTheDocument()
    expect(screen.getByLabelText(/E-mail/)).toHaveAttribute('aria-invalid', 'true')
  })
})

const MEMBROS: Membro[] = [
  {
    usuario_id: 'u1',
    nome: 'Ana Souza',
    email: 'ana@empresa.com.br',
    papel: 'dono',
    desde: '2026-01-10T12:00:00Z',
  },
  {
    usuario_id: 'u2',
    nome: 'Bia Lima',
    email: 'bia@empresa.com.br',
    papel: 'admin',
    desde: '2026-02-10T12:00:00Z',
  },
  {
    usuario_id: 'u3',
    nome: 'Caio Reis',
    email: 'caio@empresa.com.br',
    papel: 'leitor',
    desde: '2026-03-10T12:00:00Z',
  },
]

function tabela(papel: Papel, usuarioId: string) {
  return renderizar(
    <TabelaMembros
      orgId="o1"
      orgNome="Ana Ltda"
      papel={papel}
      usuarioId={usuarioId}
      membros={MEMBROS}
      aoSair={vi.fn(() => Promise.resolve())}
    />,
  )
}

describe('TabelaMembros', () => {
  it('leitor vê os membros sem controles de papel nem remoção, só "sair"', () => {
    tabela('leitor', 'u3')
    expect(screen.getByRole('table', { name: 'Membros da organização' })).toBeInTheDocument()
    expect(screen.getByText('Ana Souza')).toBeInTheDocument()
    expect(screen.queryAllByRole('combobox')).toHaveLength(0)
    expect(screen.queryByRole('button', { name: /^Remover/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Sair da organização' })).toBeInTheDocument()
    expect(screen.getByText(/Só administradores e donos alteram papéis/)).toBeInTheDocument()
  })

  it('administrador não altera nem remove o dono e não promove a dono', () => {
    tabela('admin', 'u2')
    expect(screen.queryByRole('combobox', { name: 'Papel de Ana Souza' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Remover Ana Souza' })).not.toBeInTheDocument()

    const select = screen.getByRole('combobox', { name: 'Papel de Caio Reis' })
    const opcoes = within(select)
      .getAllByRole('option')
      .map((o) => o.textContent)
    expect(opcoes).toEqual(['Administrador', 'Operador', 'Leitor'])
    expect(screen.getByRole('button', { name: 'Remover Caio Reis' })).toBeInTheDocument()
  })

  it('dono troca papel e vê o erro do servidor (último dono)', async () => {
    const usuario = userEvent.setup()
    fetchMock.mockResolvedValueOnce(
      json(
        {
          erro: {
            codigo: 'conflito',
            mensagem:
              'A organização precisa de pelo menos um dono. Promova outra pessoa a dono antes.',
          },
        },
        409,
      ),
    )
    tabela('dono', 'u1')

    const select = screen.getByRole('combobox', { name: 'Papel de Ana Souza' })
    expect(within(select).getAllByRole('option')).toHaveLength(4)
    await usuario.selectOptions(select, 'admin')

    expect(await screen.findByText(/precisa de pelo menos um dono/)).toBeInTheDocument()
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(url).toBe('/api/v1/orgs/o1/membros/u1')
    expect(init?.method).toBe('PATCH')
    expect(JSON.parse(init?.body as string)).toEqual({ papel: 'admin' })
  })
})
