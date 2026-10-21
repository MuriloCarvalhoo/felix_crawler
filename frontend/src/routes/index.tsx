import { createFileRoute, redirect } from '@tanstack/react-router'
import { sessaoOpcional } from '@/lib/guardas'

/** `/`: logado → primeira organização; sem organização → /conta; sem sessão → /entrar. */
export const Route = createFileRoute('/')({
  beforeLoad: async ({ context }) => {
    const me = await sessaoOpcional(context.queryClient)
    if (!me) throw redirect({ to: '/entrar', replace: true })
    const primeira = me.organizacoes[0]
    if (!primeira) throw redirect({ to: '/conta', replace: true })
    throw redirect({ to: '/o/$orgId', params: { orgId: primeira.id }, replace: true })
  },
})
