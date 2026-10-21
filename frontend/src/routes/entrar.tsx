import { useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link, redirect, useNavigate } from '@tanstack/react-router'
import { AuthLayout } from '@/components/layout/AuthLayout'
import { FormEntrar } from '@/features/auth/FormEntrar'
import { destinoAposEntrar, esquemaBuscaEntrar } from '@/features/auth/destino'
import { definirMe } from '@/lib/consultas'
import { sessaoOpcional } from '@/lib/guardas'

export const Route = createFileRoute('/entrar')({
  validateSearch: esquemaBuscaEntrar,
  beforeLoad: async ({ context, search }) => {
    const me = await sessaoOpcional(context.queryClient)
    if (me) throw redirect({ href: destinoAposEntrar(me, search.redirect), replace: true })
  },
  head: () => ({ meta: [{ title: 'Entrar | Felix' }] }),
  component: PaginaEntrar,
})

function PaginaEntrar() {
  const { redirect: destino, email } = Route.useSearch()
  const qc = useQueryClient()
  const navigate = useNavigate()

  return (
    <AuthLayout
      titulo="Entrar no Felix"
      descricao="Acompanhe empenhos, notas e pagamentos das suas vendas ao governo."
      rodape={
        <>
          Ainda não tem conta?{' '}
          <Link to="/cadastrar" className="font-semibold text-tesouro underline-offset-4 hover:underline">
            Crie a sua organização
          </Link>
        </>
      }
    >
      <FormEntrar
        emailInicial={email}
        aoEntrar={async (me) => {
          definirMe(qc, me)
          await navigate({ href: destinoAposEntrar(me, destino), replace: true })
        }}
        linkEsqueci={
          <Link to="/esqueci-senha" className="text-detalhe text-tesouro underline-offset-4 hover:underline">
            Esqueci a senha
          </Link>
        }
      />
    </AuthLayout>
  )
}
