import { createFileRoute, Link } from '@tanstack/react-router'
import { CircleCheck } from 'lucide-react'
import { useState } from 'react'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { AuthLayout } from '@/components/layout/AuthLayout'
import { classesBotao } from '@/components/ui/Button'
import { esquemaBuscaRedefinir } from '@/features/auth/destino'
import { AJUDA_SENHA, esquemaNovaSenha } from '@/features/auth/esquemas'
import { api, ErroApi } from '@/lib/api'

export const Route = createFileRoute('/redefinir-senha')({
  validateSearch: esquemaBuscaRedefinir,
  head: () => ({ meta: [{ title: 'Nova senha | Felix' }] }),
  component: PaginaRedefinir,
})

function PaginaRedefinir() {
  const { token } = Route.useSearch()
  const [concluido, setConcluido] = useState(false)
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: { senha: '', confirmacao: '' },
    validators: { onChange: esquemaNovaSenha },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        await api.post('/auth/senha/redefinir', { token, senha: value.senha }, { semRedirecionar: true })
        setConcluido(true)
      } catch (erro) {
        if (erro instanceof ErroApi && (erro.status === 404 || erro.campos.token)) {
          setErroGeral('Este link não vale mais. Peça um novo link de redefinição.')
          return
        }
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })

  const linkNovo = (
    <Link to="/esqueci-senha" className="font-semibold text-tesouro underline-offset-4 hover:underline">
      Pedir um novo link
    </Link>
  )

  if (!token) {
    return (
      <AuthLayout titulo="Link incompleto" descricao="Abra o link exatamente como chegou no e-mail ou peça outro." rodape={linkNovo}>
        <Link to="/entrar" className={classesBotao({ variante: 'secundario', larguraTotal: true })}>
          Voltar para entrar
        </Link>
      </AuthLayout>
    )
  }

  if (concluido) {
    return (
      <AuthLayout titulo="Senha alterada">
        <div role="status" className="flex gap-3 rounded-bloco border border-linha bg-superficie p-4">
          <CircleCheck className="mt-0.5 size-5 shrink-0 text-tesouro" aria-hidden="true" />
          <p className="text-corpo text-tinta-2">
            Sua senha nova já vale. Por segurança, encerramos as sessões abertas em outros aparelhos.
          </p>
        </div>
        <Link to="/entrar" className={`${classesBotao({ larguraTotal: true })} mt-5`}>
          Entrar com a nova senha
        </Link>
      </AuthLayout>
    )
  }

  return (
    <AuthLayout titulo="Crie uma senha nova" descricao="Ela substitui a senha atual em todos os aparelhos." rodape={linkNovo}>
      <form onSubmit={enviarFormulario(form)} noValidate className="flex flex-col gap-5" aria-label="Nova senha">
        <form.AppField name="senha">
          {(f) => <f.CampoSenha rotulo="Senha nova" autoComplete="new-password" ajuda={AJUDA_SENHA} autoFocus />}
        </form.AppField>
        <form.AppField name="confirmacao">
          {(f) => <f.CampoSenha rotulo="Repita a senha nova" autoComplete="new-password" />}
        </form.AppField>
        <ErroFormulario mensagem={erroGeral} />
        <form.AppForm>
          <form.BotaoEnviar larguraTotal>Salvar senha nova</form.BotaoEnviar>
        </form.AppForm>
      </form>
    </AuthLayout>
  )
}
