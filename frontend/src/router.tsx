import { createRouter, Navigate } from '@tanstack/react-router'
import { ehOrgIndisponivel } from '@/components/layout/GuardaOrg'
import { ErrorState } from '@/components/ui/EmptyState'
import { Carregando } from '@/components/ui/Spinner'
import { criarQueryClient } from '@/lib/queryClient'
import { routeTree } from './routeTree.gen'

export const queryClient = criarQueryClient()

export const router = createRouter({
  routeTree,
  context: { queryClient },
  defaultPreload: 'intent',
  // Os dados ficam no TanStack Query; o cache do router não precisa segurar nada.
  defaultPreloadStaleTime: 0,
  scrollRestoration: true,
  defaultPendingComponent: () => <Carregando />,
  defaultPendingMs: 300,
  defaultErrorComponent: ({ error, reset }) =>
    // Organização removida do `Me` durante o uso: volta para `/` em vez de mostrar erro.
    ehOrgIndisponivel(error) ? (
      <Navigate to="/" replace />
    ) : (
      <div className="p-6">
        <ErrorState erro={error} aoTentarDeNovo={reset} />
      </div>
    ),
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
