import { useState, type ReactNode } from 'react'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { api, ErroApi } from '@/lib/api'
import type { Me } from '@/lib/tipos'
import { esquemaEntrar } from './esquemas'

export interface FormEntrarProps {
  aoEntrar: (me: Me) => void | Promise<void>
  /** Link "Esqueci a senha" ao lado do rótulo da senha. */
  linkEsqueci?: ReactNode
  emailInicial?: string
}

/** Formulário de login (POST /auth/login). */
export function FormEntrar({ aoEntrar, linkEsqueci, emailInicial = '' }: FormEntrarProps) {
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: { email: emailInicial, senha: '' },
    validators: { onChange: esquemaEntrar },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        const me = await api.post<Me>(
          '/auth/login',
          { email: value.email.trim(), senha: value.senha },
          { semRedirecionar: true },
        )
        await aoEntrar(me)
      } catch (erro) {
        if (erro instanceof ErroApi && erro.status === 401) {
          setErroGeral('E-mail ou senha incorretos.')
          formApi.resetField('senha')
          return
        }
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })

  return (
    <form onSubmit={enviarFormulario(form)} noValidate className="flex flex-col gap-5" aria-label="Entrar">
      <form.AppField name="email">
        {(f) => <f.CampoTexto rotulo="E-mail" type="email" autoComplete="email" inputMode="email" autoFocus />}
      </form.AppField>
      <form.AppField name="senha">
        {(f) => <f.CampoSenha rotulo="Senha" autoComplete="current-password" acaoRotulo={linkEsqueci} />}
      </form.AppField>
      <ErroFormulario mensagem={erroGeral} />
      <form.AppForm>
        <form.BotaoEnviar larguraTotal>Entrar</form.BotaoEnviar>
      </form.AppForm>
    </form>
  )
}
