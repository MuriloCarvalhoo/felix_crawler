import { createFileRoute } from '@tanstack/react-router'
import { esquemaBuscaNotas, PaginaNotas, type BuscaNotas } from '@/features/notas/PaginaNotas'
import { useOrgAtual } from '@/lib/consultas'
import { aplicarAtualizacao, type AtualizacaoBusca } from '@/lib/paginacao'

export const Route = createFileRoute('/o/$orgId/notas/')({
  validateSearch: esquemaBuscaNotas,
  head: () => ({ meta: [{ title: 'Notas fiscais | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  const busca = Route.useSearch()
  const navigate = Route.useNavigate()

  function aoMudarBusca(nova: AtualizacaoBusca<BuscaNotas>) {
    void navigate({ search: (anterior) => aplicarAtualizacao(anterior, nova), replace: true })
  }

  return <PaginaNotas org={org} busca={busca} aoMudarBusca={aoMudarBusca} />
}
