import { useState } from 'react'
import {
  aplicarErrosServidor,
  enviarFormulario,
  ErroFormulario,
  useAppForm,
} from '@/components/form/formulario'
import { useToast } from '@/components/ui'
import { AJUDA_SENHA } from '@/features/auth/esquemas'
import { api } from '@/lib/api'
import type { TrocarSenhaEntrada } from '@/lib/tipos'
import { esquemaTrocarSenha } from './esquemas'

const VAZIO = { senha_atual: '', senha_nova: '', confirmacao: '' }

/** Troca de senha (POST /auth/me/senha). O servidor encerra as outras sessões. */
export function FormSenha() {
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: VAZIO,
    validators: { onChange: esquemaTrocarSenha },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      const corpo: TrocarSenhaEntrada = {
        senha_atual: value.senha_atual,
        senha_nova: value.senha_nova,
      }
      try {
        await api.post('/auth/me/senha', corpo)
        formApi.reset(VAZIO)
        toast.sucesso(
          'Senha alterada',
          'As sessões abertas em outros dispositivos foram encerradas.',
        )
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })

  return (
    <form
      onSubmit={enviarFormulario(form)}
      noValidate
      className="flex flex-col gap-4"
      aria-label="Trocar senha"
    >
      <form.AppField name="senha_atual">
        {(f) => <f.CampoSenha rotulo="Senha atual" autoComplete="current-password" obrigatorio />}
      </form.AppField>
      <form.AppField name="senha_nova">
        {(f) => (
          <f.CampoSenha
            rotulo="Nova senha"
            autoComplete="new-password"
            ajuda={AJUDA_SENHA}
            obrigatorio
          />
        )}
      </form.AppField>
      <form.AppField name="confirmacao">
        {(f) => (
          <f.CampoSenha rotulo="Confirme a nova senha" autoComplete="new-password" obrigatorio />
        )}
      </form.AppField>
      <ErroFormulario mensagem={erroGeral} />
      <div className="flex justify-end">
        <form.AppForm>
          <form.BotaoEnviar>Trocar senha</form.BotaoEnviar>
        </form.AppForm>
      </div>
    </form>
  )
}
