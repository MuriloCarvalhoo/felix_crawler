import { createFileRoute } from '@tanstack/react-router'
import { PaginaPlano } from '@/features/plano/PaginaPlano'

export const Route = createFileRoute('/o/$orgId/configuracoes/plano')({
  head: () => ({ meta: [{ title: 'Plano | Felix' }] }),
  component: PaginaPlano,
})
