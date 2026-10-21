import { useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useState } from 'react'
import { z } from 'zod'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { Button, Dialog, FecharDialog, useToast } from '@/components/ui'
import { api } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import type { Me, OrganizacaoResumo } from '@/lib/tipos'

const esquema = z.object({
  nome: z.string().trim().min(2, 'Informe o nome da organização.').max(120, 'Use no máximo 120 caracteres.'),
})

/** Cria uma organização (POST /orgs) e abre o painel dela. */
export function NovaOrgDialog({ aberto, aoMudar }: { aberto: boolean; aoMudar: (v: boolean) => void }) {
  const qc = useQueryClient()
  const navigate = useNavigate()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: { nome: '' },
    validators: { onChange: esquema },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        const org = await api.post<OrganizacaoResumo>('/orgs', { nome: value.nome.trim() })
        qc.setQueryData<Me>(chaves.me, (me) => (me ? { ...me, organizacoes: [...me.organizacoes, org] } : me))
        void qc.invalidateQueries({ queryKey: chaves.me })
        toast.sucesso('Organização criada', org.nome)
        formApi.reset()
        aoMudar(false)
        await navigate({ to: '/o/$orgId', params: { orgId: org.id } })
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })

  return (
    <Dialog
      aberto={aberto}
      aoMudar={(v) => {
        if (!v) {
          form.reset()
          setErroGeral(null)
        }
        aoMudar(v)
      }}
      titulo="Nova organização"
      descricao="Cada organização tem seus próprios CNPJs, empenhos, equipe e plano. Você será o dono."
    >
      <form id="form-nova-org" onSubmit={enviarFormulario(form)} noValidate className="flex flex-col gap-4">
        <form.AppField name="nome">
          {(f) => <f.CampoTexto rotulo="Nome da organização" autoComplete="organization" obrigatorio autoFocus />}
        </form.AppField>
        <ErroFormulario mensagem={erroGeral} />
        <div className="flex justify-end gap-2 pt-1">
          <FecharDialog asChild>
            <Button variante="secundario">Cancelar</Button>
          </FecharDialog>
          <form.AppForm>
            <form.BotaoEnviar>Criar organização</form.BotaoEnviar>
          </form.AppForm>
        </div>
      </form>
    </Dialog>
  )
}
