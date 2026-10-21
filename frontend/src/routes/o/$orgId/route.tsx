import { createFileRoute, Outlet, redirect } from '@tanstack/react-router'
import { AppShell } from '@/components/layout/AppShell'
import { GuardaOrg } from '@/components/layout/GuardaOrg'
import { exigirSessao } from '@/lib/guardas'

/**
 * Layout da organização. Guarda: exige sessão (senão /entrar?redirect=...) e
 * confere se `orgId` é uma organização do usuário (senão vai para a primeira).
 * Filhos recebem `org` no contexto do router e podem usar `useOrgAtual()`.
 * Se a organização sumir do `Me` durante o uso, volta para `/` (GuardaOrg e,
 * como rede de segurança, o `defaultErrorComponent` do router).
 */
export const Route = createFileRoute('/o/$orgId')({
  beforeLoad: async ({ context, params, location }) => {
    const me = await exigirSessao(context.queryClient, location.href)
    const org = me.organizacoes.find((o) => o.id === params.orgId)
    if (!org) {
      const primeira = me.organizacoes[0]
      if (primeira) throw redirect({ to: '/o/$orgId', params: { orgId: primeira.id }, replace: true })
      throw redirect({ to: '/conta', replace: true })
    }
    return { org }
  },
  component: LayoutOrg,
})

function LayoutOrg() {
  const { orgId } = Route.useParams()
  return (
    <GuardaOrg orgId={orgId}>
      <AppShell>
        <Outlet />
      </AppShell>
    </GuardaOrg>
  )
}
