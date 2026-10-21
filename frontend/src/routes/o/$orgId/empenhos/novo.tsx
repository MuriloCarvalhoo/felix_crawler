import { createFileRoute } from '@tanstack/react-router'
import { z } from 'zod'
import { PaginaNovoEmpenho, type AbaNovoEmpenho } from '@/features/empenhos/PaginaNovoEmpenho'
import { useOrgAtual } from '@/lib/consultas'

const esquemaBusca = z.object({
  aba: z.enum(['pdf', 'manual']).catch('pdf').default('pdf'),
})

export const Route = createFileRoute('/o/$orgId/empenhos/novo')({
  validateSearch: esquemaBusca,
  head: () => ({ meta: [{ title: 'Novo empenho | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  const { aba } = Route.useSearch()
  const navigate = Route.useNavigate()

  function aoMudarAba(nova: AbaNovoEmpenho) {
    void navigate({ search: { aba: nova }, replace: true })
  }

  return <PaginaNovoEmpenho org={org} aba={aba} aoMudarAba={aoMudarAba} />
}
