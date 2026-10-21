import { useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { z } from 'zod'
import {
  aplicarErrosServidor,
  enviarFormulario,
  ErroFormulario,
  useAppForm,
} from '@/components/form/formulario'
import { useToast } from '@/components/ui'
import { api } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import type { Me, Organizacao } from '@/lib/tipos'

const esquema = z.object({
  nome: z
    .string()
    .trim()
    .min(2, 'Informe o nome da organização.')
    .max(120, 'Use no máximo 120 caracteres.'),
})

/** Nome da organização (PATCH /orgs/{org}); admin para cima. */
export function FormNomeOrganizacao({ org }: { org: Organizacao }) {
  const qc = useQueryClient()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: { nome: org.nome },
    validators: { onChange: esquema },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        const atualizada = await api.patch<Organizacao>(api.rotaOrg(org.id), {
          nome: value.nome.trim(),
        })
        qc.setQueryData(chaves.organizacao(org.id), atualizada)
        qc.setQueryData<Me>(chaves.me, (me) =>
          me
            ? {
                ...me,
                organizacoes: me.organizacoes.map((o) =>
                  o.id === org.id ? { ...o, nome: atualizada.nome } : o,
                ),
              }
            : me,
        )
        void qc.invalidateQueries({ queryKey: chaves.me })
        formApi.reset({ nome: atualizada.nome })
        toast.sucesso('Nome da organização atualizado')
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
      aria-label="Dados da organização"
    >
      <form.AppField name="nome">
        {(f) => (
          <f.CampoTexto
            rotulo="Nome da organização"
            autoComplete="organization"
            obrigatorio
            ajuda="Aparece no seletor de organizações e nos convites enviados à equipe."
          />
        )}
      </form.AppField>
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
