import type { QueryClient } from '@tanstack/react-query'
import { createRootRouteWithContext, HeadContent, Link, Outlet } from '@tanstack/react-router'
import { Compass } from 'lucide-react'
import { classesBotao } from '@/components/ui/Button'
import { EmptyState } from '@/components/ui/EmptyState'

export interface ContextoRouter {
  queryClient: QueryClient
}

export const Route = createRootRouteWithContext<ContextoRouter>()({
  head: () => ({ meta: [{ title: 'Felix' }] }),
  component: Raiz,
  notFoundComponent: NaoEncontrada,
})

function Raiz() {
  return (
    <>
      {/* React 19 eleva <title>/<meta> para o <head>: as rotas definem `head`. */}
      <HeadContent />
      <Outlet />
    </>
  )
}

function NaoEncontrada() {
  return (
    <main id="conteudo" className="mx-auto flex min-h-[60dvh] max-w-xl items-center px-4 py-16">
      <EmptyState
        className="w-full"
        icone={<Compass />}
        titulo="Página não encontrada"
        descricao="O endereço pode ter mudado ou o registro não existe mais."
        acao={
          <Link to="/" className={classesBotao({ variante: 'secundario' })}>
            Ir para o início
          </Link>
        }
      />
    </main>
  )
}
