import { createFileRoute } from '@tanstack/react-router'
import { esquemaBuscaEmpenhos, PaginaEmpenhos, type BuscaEmpenhos } from '@/features/empenhos/PaginaEmpenhos'
import { useOrgAtual } from '@/lib/consultas'
import { aplicarAtualizacao, type AtualizacaoBusca } from '@/lib/paginacao'

export const Route = createFileRoute('/o/$orgId/empenhos/')({
  validateSearch: esquemaBuscaEmpenhos,
  head: () => ({ meta: [{ title: 'Empenhos | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  const busca = Route.useSearch()
  const navigate = Route.useNavigate()

  function aoMudarBusca(nova: AtualizacaoBusca<BuscaEmpenhos>) {
    void navigate({ search: (anterior) => aplicarAtualizacao(anterior, nova), replace: true })
  }

  return <PaginaEmpenhos org={org} busca={busca} aoMudarBusca={aoMudarBusca} />
}
