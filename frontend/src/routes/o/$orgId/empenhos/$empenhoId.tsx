import { createFileRoute } from '@tanstack/react-router'
import { DetalheEmpenho } from '@/features/empenhos/DetalheEmpenho'
import { useOrgAtual } from '@/lib/consultas'

export const Route = createFileRoute('/o/$orgId/empenhos/$empenhoId')({
  head: () => ({ meta: [{ title: 'Empenho | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  const { empenhoId } = Route.useParams()
  return <DetalheEmpenho key={empenhoId} org={org} empenhoId={empenhoId} />
}
