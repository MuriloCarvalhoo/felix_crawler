import { useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link, redirect, useNavigate } from '@tanstack/react-router'
import { useState } from 'react'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { AuthLayout } from '@/components/layout/AuthLayout'
import { destinoAposEntrar } from '@/features/auth/destino'
import { AJUDA_SENHA, esquemaCadastro } from '@/features/auth/esquemas'
import { api } from '@/lib/api'
import { definirMe } from '@/lib/consultas'
import { sessaoOpcional } from '@/lib/guardas'
import type { CadastroEntrada, Me } from '@/lib/tipos'

export const Route = createFileRoute('/cadastrar')({
  beforeLoad: async ({ context }) => {
    const me = await sessaoOpcional(context.queryClient)
    if (me) throw redirect({ to: '/', replace: true })
  },
  head: () => ({ meta: [{ title: 'Criar conta | Felix' }] }),
  component: PaginaCadastrar,
})

function PaginaCadastrar() {
  const qc = useQueryClient()
  const navigate = useNavigate()
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: { nome: '', email: '', senha: '', organizacao_nome: '' },
    validators: { onChange: esquemaCadastro },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      const corpo: CadastroEntrada = {
        nome: value.nome.trim(),
        email: value.email.trim(),
        senha: value.senha,
        organizacao_nome: value.organizacao_nome.trim(),
      }
      try {
        const me = await api.post<Me>('/auth/cadastro', corpo, { semRedirecionar: true })
        definirMe(qc, me)
        await navigate({ href: destinoAposEntrar(me), replace: true })
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })

  return (
    <AuthLayout
      titulo="Crie sua conta"
      descricao="14 dias do plano Profissional para testar, sem cartão. Depois, continue no Gratuito ou escolha um plano."
      rodape={
        <>
          Já tem conta?{' '}
          <Link to="/entrar" className="font-semibold text-tesouro underline-offset-4 hover:underline">
            Entrar
          </Link>
        </>
      }
    >
      <form onSubmit={enviarFormulario(form)} noValidate className="flex flex-col gap-5" aria-label="Criar conta">
        <form.AppField name="nome">
          {(f) => <f.CampoTexto rotulo="Seu nome" autoComplete="name" autoFocus />}
        </form.AppField>
        <form.AppField name="email">
          {(f) => <f.CampoTexto rotulo="E-mail de trabalho" type="email" autoComplete="email" inputMode="email" />}
        </form.AppField>
        <form.AppField name="senha">
          {(f) => <f.CampoSenha rotulo="Senha" autoComplete="new-password" ajuda={AJUDA_SENHA} />}
        </form.AppField>
        <form.AppField name="organizacao_nome">
          {(f) => (
            <f.CampoTexto
              rotulo="Nome da organização"
              autoComplete="organization"
              ajuda="Normalmente a razão social ou o nome do grupo de empresas. Você cadastra os CNPJs depois."
            />
          )}
        </form.AppField>
        <ErroFormulario mensagem={erroGeral} />
        <form.AppForm>
          <form.BotaoEnviar larguraTotal>Criar conta</form.BotaoEnviar>
        </form.AppForm>
      </form>
    </AuthLayout>
  )
}
