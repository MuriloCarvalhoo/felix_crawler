import { QueryClientProvider } from '@tanstack/react-query'
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
  useParams,
} from '@tanstack/react-router'
import { act, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { criarQueryClient, ORG_ID } from '@/features/empenhos/testeUtil'
import { chaves, useOrgAtual } from '@/lib/consultas'
import type { Me } from '@/lib/tipos'
import { GuardaOrg } from './GuardaOrg'

function Filho() {
  const org = useOrgAtual()
  return <p>Organização {org.nome}</p>
}

function LayoutOrg() {
  const { orgId } = useParams({ from: '/o/$orgId' })
  return (
    <GuardaOrg orgId={orgId}>
      <Filho />
    </GuardaOrg>
  )
}

describe('GuardaOrg', () => {
  it('organização removida do Me durante o uso: volta para / sem quebrar a tela', async () => {
    const qc = criarQueryClient()
    const raiz = createRootRoute({ component: Outlet })
    const inicio = createRoute({ getParentRoute: () => raiz, path: '/', component: () => <p>Início</p> })
    const org = createRoute({ getParentRoute: () => raiz, path: '/o/$orgId', component: LayoutOrg })
    const router = createRouter({
      routeTree: raiz.addChildren([inicio, org]),
      history: createMemoryHistory({ initialEntries: [`/o/${ORG_ID}`] }),
    })
    render(
      <QueryClientProvider client={qc}>
        <RouterProvider router={router as never} />
      </QueryClientProvider>,
    )

    expect(await screen.findByText('Organização Fornecedora Ltda')).toBeInTheDocument()

    act(() => {
      qc.setQueryData<Me>(chaves.me, (me) => (me ? { ...me, organizacoes: [] } : me))
    })

    expect(await screen.findByText('Início')).toBeInTheDocument()
    await waitFor(() => expect(router.state.location.pathname).toBe('/'))
    expect(screen.queryByText(/Algo deu errado|não faz mais parte/)).not.toBeInTheDocument()
  })
})
