import { useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import {
  aplicarErrosServidor,
  enviarFormulario,
  ErroFormulario,
  useAppForm,
} from '@/components/form/formulario'
import { useToast } from '@/components/ui'
import { api } from '@/lib/api'
import { definirMe } from '@/lib/consultas'
import type { Me, Usuario } from '@/lib/tipos'
import { esquemaPerfil } from './esquemas'

/** Nome do usuário (PATCH /auth/me). O e-mail é só leitura. */
export function FormPerfil({ usuario }: { usuario: Usuario }) {
  const qc = useQueryClient()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: { nome: usuario.nome },
    validators: { onChange: esquemaPerfil },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        const me = await api.patch<Me>('/auth/me', { nome: value.nome.trim() })
        definirMe(qc, me)
        formApi.reset({ nome: me.usuario.nome })
        toast.sucesso('Nome atualizado')
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
      aria-label="Perfil"
    >
      <form.AppField name="nome">
        {(f) => <f.CampoTexto rotulo="Nome" autoComplete="name" obrigatorio />}
      </form.AppField>
      <div className="flex flex-col gap-1.5">
        <span className="text-detalhe font-semibold text-tinta">E-mail</span>
        <p className="text-corpo break-all text-tinta">{usuario.email}</p>
        <p className="text-detalhe text-tinta-2">
          O e-mail é usado para entrar e receber convites. Não é possível alterá-lo por aqui.
        </p>
      </div>
      <ErroFormulario mensagem={erroGeral} />
      <div className="flex justify-end">
        <form.Subscribe selector={(s) => s.isDirty}>
          {(alterado) => (
            <form.AppForm>
              <form.BotaoEnviar disabled={!alterado}>Salvar nome</form.BotaoEnviar>
            </form.AppForm>
          )}
        </form.Subscribe>
      </div>
    </form>
  )
}
