import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/o/$orgId/configuracoes/')({
  beforeLoad: ({ params }) => {
    throw redirect({ to: '/o/$orgId/configuracoes/organizacao', params, replace: true })
  },
})
