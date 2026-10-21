import { createFileRoute } from '@tanstack/react-router'
import { PaginaPainel } from '@/features/painel/PaginaPainel'
import { useOrgAtual } from '@/lib/consultas'

export const Route = createFileRoute('/o/$orgId/')({
  head: () => ({ meta: [{ title: 'Painel | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  return <PaginaPainel org={org} />
}
