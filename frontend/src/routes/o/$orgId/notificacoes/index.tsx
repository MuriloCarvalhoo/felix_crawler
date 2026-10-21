import { createFileRoute } from '@tanstack/react-router'
import { z } from 'zod'
import {
  PaginaNotificacoes,
  type BuscaNotificacoes,
} from '@/features/notificacoes/PaginaNotificacoes'
import { useOrgAtual } from '@/lib/consultas'

const esquemaBusca = z.object({
  filtro: z.enum(['todas', 'nao_lidas']).catch('todas').default('todas'),
  pagina: z.coerce.number().int().min(1).catch(1).default(1),
})

export const Route = createFileRoute('/o/$orgId/notificacoes/')({
  validateSearch: esquemaBusca,
  head: () => ({ meta: [{ title: 'Notificações | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const org = useOrgAtual()
  const busca = Route.useSearch()
  const navigate = Route.useNavigate()

  function aoMudarBusca(nova: BuscaNotificacoes) {
    void navigate({ search: nova, replace: true })
  }

  return <PaginaNotificacoes org={org} busca={busca} aoMudarBusca={aoMudarBusca} />
}
