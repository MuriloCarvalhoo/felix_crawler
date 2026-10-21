import { createFileRoute } from '@tanstack/react-router'
import { PaginaEmpresas } from '@/features/orgs/PaginaEmpresas'

export const Route = createFileRoute('/o/$orgId/configuracoes/empresas')({
  head: () => ({ meta: [{ title: 'Empresas | Felix' }] }),
  component: PaginaEmpresas,
})
