import { useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { z } from 'zod'
import {
  aplicarErrosServidor,
  enviarFormulario,
  ErroFormulario,
  useAppForm,
} from '@/components/form/formulario'
import { Button, Dialog, FecharDialog, useToast } from '@/components/ui'
import { AvisoLimitePlano, ehLimitePlano } from '@/features/plano/AvisoLimitePlano'
import { api, ErroApi } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import { formatarCNPJ, normalizarCNPJ, validarCNPJ } from '@/lib/formatos'
import type { Empresa, NovaEmpresa } from '@/lib/tipos'

const campoRazao = z
  .string()
  .trim()
  .min(1, 'Informe a razão social.')
  .max(200, 'Use no máximo 200 caracteres.')

export const esquemaNovaEmpresa = z.object({
  razao_social: campoRazao,
  cnpj: z
    .string()
    .trim()
    .min(1, 'Informe o CNPJ.')
    .refine(validarCNPJ, 'CNPJ inválido. Confira os 14 caracteres e os dígitos verificadores.'),
})

const esquemaEdicao = z.object({ razao_social: campoRazao, cnpj: z.string() })

export interface EmpresaDialogProps {
  orgId: string
  aberto: boolean
  aoMudar: (aberto: boolean) => void
  /** Empresa a editar; ausente = nova empresa. */
  empresa?: Empresa | null
}

/** Adiciona um CNPJ (POST) ou edita a razão social (PATCH). */
export function EmpresaDialog({ orgId, aberto, aoMudar, empresa }: EmpresaDialogProps) {
  const qc = useQueryClient()
  const toast = useToast()
  const editando = Boolean(empresa)
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const [limite, setLimite] = useState<string | null>(null)

  const form = useAppForm({
    defaultValues: {
      razao_social: empresa?.razao_social ?? '',
      cnpj: empresa ? formatarCNPJ(empresa.cnpj) : '',
    },
    validators: { onChange: editando ? esquemaEdicao : esquemaNovaEmpresa },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      setLimite(null)
      try {
        if (empresa) {
          await api.patch<Empresa>(api.rotaOrg(orgId, `/empresas/${empresa.id}`), {
            razao_social: value.razao_social.trim(),
          })
          toast.sucesso('Razão social atualizada')
        } else {
          const corpo: NovaEmpresa = {
            razao_social: value.razao_social.trim(),
            cnpj: normalizarCNPJ(value.cnpj),
          }
          const criada = await api.post<Empresa>(api.rotaOrg(orgId, '/empresas'), corpo)
          toast.sucesso('CNPJ adicionado', `${criada.razao_social} · ${formatarCNPJ(criada.cnpj)}`)
          void qc.invalidateQueries({ queryKey: chaves.assinatura(orgId) })
          void qc.invalidateQueries({ queryKey: chaves.organizacao(orgId) })
        }
        await qc.invalidateQueries({ queryKey: chaves.empresas(orgId) })
        formApi.reset()
        aoMudar(false)
      } catch (erro) {
        if (ehLimitePlano(erro)) {
          setLimite(erro.message)
          return
        }
        if (
          erro instanceof ErroApi &&
          erro.status === 409 &&
          Object.keys(erro.campos).length === 0
        ) {
          formApi.setFieldMeta('cnpj', (m) => ({
            ...m,
            errorMap: { ...m.errorMap, onServer: erro.message },
          }))
          return
        }
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
          setLimite(null)
        }
        aoMudar(v)
      }}
      titulo={editando ? 'Editar empresa' : 'Adicionar CNPJ'}
      descricao={
        editando
          ? 'O CNPJ não pode ser alterado. Para trocar, exclua a empresa e adicione o novo CNPJ.'
          : 'Empresa fornecedora em nome da qual os empenhos são emitidos. Aceita CNPJ numérico ou alfanumérico.'
      }
    >
      <form
        onSubmit={enviarFormulario(form)}
        noValidate
        className="flex flex-col gap-4"
        aria-label={editando ? 'Editar empresa' : 'Adicionar CNPJ'}
      >
        <form.AppField name="cnpj">
          {(f) => (
            <f.CampoCNPJ
              rotulo="CNPJ"
              obrigatorio={!editando}
              desabilitado={editando}
              ajuda={
                editando
                  ? undefined
                  : 'Com ou sem pontuação. Letras são aceitas nas 12 primeiras posições.'
              }
            />
          )}
        </form.AppField>
        <form.AppField name="razao_social">
          {(f) => (
            <f.CampoTexto
              rotulo="Razão social"
              autoComplete="organization"
              obrigatorio
              autoFocus={editando}
              maxLength={200}
            />
          )}
        </form.AppField>
        {limite ? <AvisoLimitePlano orgId={orgId} mensagem={limite} /> : null}
        <ErroFormulario mensagem={erroGeral} />
        <div className="flex justify-end gap-2 pt-1">
          <FecharDialog asChild>
            <Button variante="secundario">Cancelar</Button>
          </FecharDialog>
          <form.AppForm>
            <form.BotaoEnviar>
              {editando ? 'Salvar razão social' : 'Adicionar CNPJ'}
            </form.BotaoEnviar>
          </form.AppForm>
        </div>
      </form>
    </Dialog>
  )
}
