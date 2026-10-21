import { createFileRoute } from '@tanstack/react-router'
import { DetalheNota } from '@/features/notas/DetalheNota'
import { useOrgAtual } from '@/lib/consultas'

export const Route = createFileRoute('/o/$orgId/notas/$notaId')({
  head: () => ({ meta: [{ title: 'Nota fiscal | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  const { notaId } = Route.useParams()
  return <DetalheNota key={notaId} org={org} notaId={notaId} />
}
