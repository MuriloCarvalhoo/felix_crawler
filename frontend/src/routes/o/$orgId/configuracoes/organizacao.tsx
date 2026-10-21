import { createFileRoute } from '@tanstack/react-router'
import { PaginaOrganizacao } from '@/features/orgs/PaginaOrganizacao'

export const Route = createFileRoute('/o/$orgId/configuracoes/organizacao')({
  head: () => ({ meta: [{ title: 'Organização | Felix' }] }),
  component: PaginaOrganizacao,
})
