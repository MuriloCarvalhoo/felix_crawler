import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { z } from 'zod'
import { PaginaAuditoria, type FiltrosAuditoria } from '@/features/auditoria/PaginaAuditoria'

const buscaAuditoria = z.object({
  pagina: z.coerce.number().int().min(1).optional().catch(undefined),
  entidade: z.string().trim().min(1).max(60).optional().catch(undefined),
})

export const Route = createFileRoute('/o/$orgId/configuracoes/auditoria')({
  validateSearch: buscaAuditoria,
  head: () => ({ meta: [{ title: 'Auditoria | Felix' }] }),
  component: Pagina,
})

function Pagina() {
  const busca = Route.useSearch()
  const navigate = useNavigate({ from: Route.fullPath })

  function mudar(filtros: FiltrosAuditoria, opcoes?: { substituir?: boolean }) {
    void navigate({
      search: {
        pagina: filtros.pagina > 1 ? filtros.pagina : undefined,
        entidade: filtros.entidade,
      },
      replace: opcoes?.substituir,
    })
  }

  return (
    <PaginaAuditoria pagina={busca.pagina ?? 1} entidade={busca.entidade} aoMudarFiltros={mudar} />
  )
}
