import { createFileRoute } from '@tanstack/react-router'
import { PaginaEquipe } from '@/features/equipe/PaginaEquipe'

export const Route = createFileRoute('/o/$orgId/configuracoes/equipe')({
  head: () => ({ meta: [{ title: 'Equipe | Felix' }] }),
  component: PaginaEquipe,
})
