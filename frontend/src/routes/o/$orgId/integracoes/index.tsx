import { createFileRoute } from '@tanstack/react-router'
import { z } from 'zod'
import { PaginaIntegracoes } from '@/features/integracoes/PaginaIntegracoes'
import type { BuscaDocumentos } from '@/features/integracoes/TabelaDocumentos'
import { useOrgAtual } from '@/lib/consultas'

const esquemaBusca = z.object({
  situacao: z.enum(['novo', 'vinculado', 'ignorado']).catch('novo').default('novo'),
  fase: z
    .union([z.literal(1), z.literal(2), z.literal(3)])
    .optional()
    .catch(undefined),
  pagina: z.coerce.number().int().min(1).catch(1).default(1),
})

export const Route = createFileRoute('/o/$orgId/integracoes/')({
  validateSearch: esquemaBusca,
  head: () => ({ meta: [{ title: 'Integrações | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  const busca = Route.useSearch()
  const navigate = Route.useNavigate()

  function aoMudarBusca(nova: BuscaDocumentos) {
    void navigate({ search: nova, replace: true })
  }

  return <PaginaIntegracoes org={org} busca={busca} aoMudarBusca={aoMudarBusca} />
}
