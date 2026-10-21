import { createFileRoute, Link } from '@tanstack/react-router'
import { MailCheck } from 'lucide-react'
import { useState } from 'react'
import { enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { AuthLayout } from '@/components/layout/AuthLayout'
import { esquemaEsqueci } from '@/features/auth/esquemas'
import { api, ErroApi } from '@/lib/api'

export const Route = createFileRoute('/esqueci-senha')({
  head: () => ({ meta: [{ title: 'Redefinir senha | Felix' }] }),
  component: PaginaEsqueci,
})

function PaginaEsqueci() {
  const [enviadoPara, setEnviadoPara] = useState<string | null>(null)
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: { email: '' },
    validators: { onChange: esquemaEsqueci },
    onSubmit: async ({ value }) => {
      setErroGeral(null)
      const email = value.email.trim()
      try {
        await api.post('/auth/senha/esqueci', { email }, { semRedirecionar: true })
      } catch (erro) {
        // A resposta é neutra de propósito; só informamos limite de tentativas e falha de rede.
        if (erro instanceof ErroApi && (erro.status === 429 || erro.status === 0)) {
          setErroGeral(erro.message)
          return
        }
      }
      setEnviadoPara(email)
    },
  })

  const voltar = (
    <>
      Lembrou a senha?{' '}
      <Link to="/entrar" className="font-semibold text-tesouro underline-offset-4 hover:underline">
        Entrar
      </Link>
    </>
  )

  if (enviadoPara) {
    return (
      <AuthLayout titulo="Confira seu e-mail" rodape={voltar}>
        <div role="status" className="flex gap-3 rounded-bloco border border-linha bg-superficie p-4">
          <MailCheck className="mt-0.5 size-5 shrink-0 text-tesouro" aria-hidden="true" />
          <p className="text-corpo text-tinta-2">
            Se existir uma conta para <strong className="font-semibold text-tinta">{enviadoPara}</strong>, enviamos um
            link para criar uma nova senha. Ele expira em pouco tempo; se não chegar, confira o spam ou peça outro.
          </p>
        </div>
        <button
          type="button"
          className="mt-4 text-detalhe font-semibold text-tesouro underline-offset-4 hover:underline"
          onClick={() => {
            setEnviadoPara(null)
            form.reset()
          }}
        >
          Usar outro e-mail
        </button>
      </AuthLayout>
    )
  }

  return (
    <AuthLayout
      titulo="Esqueceu a senha?"
      descricao="Informe o e-mail da sua conta. Enviaremos um link para você criar uma senha nova."
      rodape={voltar}
    >
      <form onSubmit={enviarFormulario(form)} noValidate className="flex flex-col gap-5" aria-label="Pedir link de redefinição">
        <form.AppField name="email">
          {(f) => <f.CampoTexto rotulo="E-mail" type="email" autoComplete="email" inputMode="email" autoFocus />}
        </form.AppField>
        <ErroFormulario mensagem={erroGeral} />
        <form.AppForm>
          <form.BotaoEnviar larguraTotal>Enviar link</form.BotaoEnviar>
        </form.AppForm>
      </form>
    </AuthLayout>
  )
}
