import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { json, mockarFetch, prepararDom, renderizar, semConteudo } from '@/features/notificacoes/testeUtil'
import { chaves } from '@/lib/consultas'
import type { Me } from '@/lib/tipos'
import { MenuUsuario } from './MenuUsuario'

const ME: Me = {
  usuario: { id: 'u1', nome: 'Ana Souza', email: 'ana@empresa.com.br' },
  organizacoes: [{ id: 'o1', nome: 'Ana Ltda', papel: 'dono', plano: 'profissional' }],
}

beforeEach(() => {
  prepararDom()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function montar(responderLogout: () => Response) {
  mockarFetch([
    { caminho: '/api/v1/auth/me', responder: () => json({ dados: ME }) },
    { metodo: 'POST', caminho: '/api/v1/auth/logout', responder: responderLogout },
  ])
  const r = renderizar(<MenuUsuario />, '/o/o1')
  r.qc.setQueryData(chaves.me, ME)
  return r
}

async function sair() {
  const usuario = userEvent.setup()
  await usuario.click(await screen.findByRole('button', { name: /Menu da conta/ }))
  await usuario.click(await screen.findByRole('menuitem', { name: 'Sair' }))
}

describe('MenuUsuario: sair', () => {
  it('erro de servidor no logout: avisa e mantém a sessão', async () => {
    const { qc, router } = montar(() => json({ erro: { codigo: 'interno', mensagem: 'Falha interna.' } }, 500))
    await sair()

    expect(await screen.findByText('Não foi possível sair')).toBeInTheDocument()
    expect(qc.getQueryData(chaves.me)).toEqual(ME)
    expect(router.state.location.pathname).toBe('/o/o1')
  })

  it('401 no logout (sessão já expirada): sai localmente', async () => {
    const { qc, router } = montar(() => json({ erro: { codigo: 'nao_autenticado', mensagem: 'x' } }, 401))
    await sair()

    await waitFor(() => expect(router.state.location.pathname).toBe('/entrar'))
    expect(qc.getQueryData(chaves.me)).toBeUndefined()
  })

  it('logout bem-sucedido limpa a sessão e vai para /entrar', async () => {
    const { qc, router } = montar(semConteudo)
    await sair()

    await waitFor(() => expect(router.state.location.pathname).toBe('/entrar'))
    expect(qc.getQueryData(chaves.me)).toBeUndefined()
  })
})
