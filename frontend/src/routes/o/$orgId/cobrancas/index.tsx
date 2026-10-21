import { createFileRoute } from '@tanstack/react-router'
import { esquemaBuscaCobrancas, PaginaCobrancas, type BuscaCobrancas } from '@/features/cobrancas/PaginaCobrancas'
import { useOrgAtual } from '@/lib/consultas'

export const Route = createFileRoute('/o/$orgId/cobrancas/')({
  validateSearch: esquemaBuscaCobrancas,
  head: () => ({ meta: [{ title: 'Cobranças | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  const busca = Route.useSearch()
  const navigate = Route.useNavigate()

  function aoMudarBusca(nova: BuscaCobrancas) {
    void navigate({ search: nova, replace: true })
  }

  return <PaginaCobrancas org={org} busca={busca} aoMudarBusca={aoMudarBusca} />
}
