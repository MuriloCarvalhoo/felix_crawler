import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useMemo, useState } from 'react'
import { z } from 'zod'
import { aplicarErrosServidor, enviarFormulario, ErroFormulario, useAppForm } from '@/components/form/formulario'
import { Button, Dialog, FecharDialog, useToast } from '@/components/ui'
import { opcoesEmpresas } from '@/lib/consultas'
import { formatarBRL } from '@/lib/formatos'
import type { Empenho } from '@/lib/tipos'
import { invalidarEmpenhos } from './comum'
import { paraEdicaoEmpenho, valoresDoEmpenho } from './esquemas'
import { FormEmpenho } from './FormEmpenho'
import { anularEmpenho, editarEmpenho } from './servicos'

interface DialogEmpenhoProps {
  orgId: string
  empenho: Empenho
  aberto: boolean
  aoMudar: (aberto: boolean) => void
}

/** Editar empenho. O valor não pode ficar abaixo do faturado (o servidor confere). */
export function DialogEditarEmpenho({ orgId, empenho, aberto, aoMudar }: DialogEmpenhoProps) {
  const qc = useQueryClient()
  const toast = useToast()
  const empresas = useQuery(opcoesEmpresas(orgId))
  const valoresIniciais = useMemo(() => valoresDoEmpenho(empenho), [empenho])
  return (
    <Dialog
      aberto={aberto}
      aoMudar={aoMudar}
      largura="g"
      titulo={`Editar empenho ${empenho.numero}`}
      descricao={`Já faturado: ${formatarBRL(empenho.faturado_centavos)}. O valor total não pode ficar abaixo disso.`}
    >
      {aberto ? (
        <FormEmpenho
          orgId={orgId}
          valoresIniciais={valoresIniciais}
          empresas={empresas.data}
          semEmpresa
          rotuloFormulario="Editar empenho"
          rotuloEnviar="Salvar alterações"
          acoes={
            <FecharDialog asChild>
              <Button variante="secundario">Cancelar</Button>
            </FecharDialog>
          }
          aoEnviar={async (valores) => {
            await editarEmpenho(orgId, empenho.id, paraEdicaoEmpenho(valores))
            await invalidarEmpenhos(qc, orgId)
            toast.sucesso('Empenho atualizado', 'Vencimentos das notas abertas foram recalculados, se a regra mudou.')
            aoMudar(false)
          }}
        />
      ) : null}
    </Dialog>
  )
}

const esquemaAnular = z.object({
  motivo: z.string().trim().min(3, 'Informe o motivo da anulação.').max(500, 'Use no máximo 500 caracteres.'),
})

function FormAnular({ orgId, empenho, aoConcluir }: { orgId: string; empenho: Empenho; aoConcluir: () => void }) {
  const qc = useQueryClient()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const form = useAppForm({
    defaultValues: { motivo: '' },
    validators: { onChange: esquemaAnular },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      try {
        await anularEmpenho(orgId, empenho.id, { motivo: value.motivo.trim() })
        await invalidarEmpenhos(qc, orgId)
        toast.sucesso('Empenho anulado')
        aoConcluir()
      } catch (erro) {
        setErroGeral(aplicarErrosServidor(formApi, erro))
      }
    },
  })
  return (
    <form onSubmit={enviarFormulario(form)} noValidate aria-label="Anular empenho" className="flex flex-col gap-4">
      <p className="text-corpo text-tinta-2">
        O empenho deixa de aceitar notas e sai da lista de ativos. Não é possível anular com nota paga, e as notas em aberto
        precisam ser canceladas antes.
      </p>
      <form.AppField name="motivo">
        {(f) => <f.CampoAreaTexto rotulo="Motivo" obrigatorio linhas={3} maxLength={500} />}
      </form.AppField>
      <ErroFormulario mensagem={erroGeral} />
      <div className="flex flex-wrap justify-end gap-2">
        <FecharDialog asChild>
          <Button variante="secundario">Voltar</Button>
        </FecharDialog>
        <form.AppForm>
          <form.BotaoEnviar variante="perigo">Anular empenho</form.BotaoEnviar>
        </form.AppForm>
      </div>
    </form>
  )
}

/** Anular empenho com motivo obrigatório; a mensagem do 409 aparece no formulário. */
export function DialogAnularEmpenho({ orgId, empenho, aberto, aoMudar }: DialogEmpenhoProps) {
  return (
    <Dialog aberto={aberto} aoMudar={aoMudar} largura="m" titulo={`Anular empenho ${empenho.numero}?`}>
      {aberto ? <FormAnular orgId={orgId} empenho={empenho} aoConcluir={() => aoMudar(false)} /> : null}
    </Dialog>
  )
}
