import { useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { z } from 'zod'
import {
  aplicarErrosServidor,
  enviarFormulario,
  ErroFormulario,
  erroVisivel,
  useAppForm,
} from '@/components/form/formulario'
import { Button, Dialog, FecharDialog, useToast } from '@/components/ui'
import { campoEmail } from '@/features/auth/esquemas'
import { AvisoLimitePlano, ehLimitePlano } from '@/features/plano/AvisoLimitePlano'
import { api, ErroApi } from '@/lib/api'
import { cx } from '@/lib/cx'
import { chaves } from '@/lib/consultas'
import { DESCRICOES_PAPEL, NOMES_PAPEL, PAPEIS } from '@/lib/permissoes'
import type { Convite, NovoConvite, Papel } from '@/lib/tipos'

export const esquemaConvite = z.object({
  email: campoEmail,
  papel: z.enum(['dono', 'admin', 'operador', 'leitor'], { error: 'Escolha o papel.' }),
})

export interface ConvidarDialogProps {
  orgId: string
  aberto: boolean
  aoMudar: (aberto: boolean) => void
  /** Só o dono pode convidar outro dono. */
  podeConvidarDono: boolean
}

/** Convite por e-mail com papel (POST /orgs/{org}/convites). */
export function ConvidarDialog({ orgId, aberto, aoMudar, podeConvidarDono }: ConvidarDialogProps) {
  const qc = useQueryClient()
  const toast = useToast()
  const [erroGeral, setErroGeral] = useState<string | null>(null)
  const [limite, setLimite] = useState<string | null>(null)
  const papeis = PAPEIS.filter((p) => podeConvidarDono || p !== 'dono')

  const form = useAppForm({
    defaultValues: { email: '', papel: 'operador' as Papel },
    validators: { onChange: esquemaConvite },
    onSubmit: async ({ value, formApi }) => {
      setErroGeral(null)
      setLimite(null)
      const corpo: NovoConvite = { email: value.email.trim(), papel: value.papel }
      try {
        const convite = await api.post<Convite>(api.rotaOrg(orgId, '/convites'), corpo)
        toast.sucesso(
          'Convite enviado',
          `${convite.email} receberá um e-mail com o link para entrar.`,
        )
        void qc.invalidateQueries({ queryKey: chaves.convites(orgId) })
        void qc.invalidateQueries({ queryKey: chaves.assinatura(orgId) })
        void qc.invalidateQueries({ queryKey: chaves.organizacao(orgId) })
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
          formApi.setFieldMeta('email', (m) => ({
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
      titulo="Convidar pessoa"
      descricao="A pessoa recebe um e-mail com um link para entrar na organização. O convite vale por alguns dias."
    >
      <form
        onSubmit={enviarFormulario(form)}
        noValidate
        className="flex flex-col gap-4"
        aria-label="Convidar pessoa"
      >
        <form.AppField name="email">
          {(f) => (
            <f.CampoTexto
              rotulo="E-mail"
              type="email"
              inputMode="email"
              autoComplete="off"
              obrigatorio
              autoFocus
            />
          )}
        </form.AppField>
        <form.Field name="papel">
          {(field) => {
            const erro = erroVisivel(field)
            return (
              <fieldset
                className="flex flex-col gap-2"
                aria-describedby={erro ? 'erro-papel-convite' : undefined}
              >
                <legend className="mb-1.5 text-detalhe font-semibold text-tinta">Papel</legend>
                {papeis.map((p) => {
                  const marcado = field.state.value === p
                  return (
                    <label
                      key={p}
                      className={cx(
                        'flex cursor-pointer items-start gap-3 rounded-campo border px-3 py-2.5 transition-colors',
                        marcado
                          ? 'border-tesouro bg-tesouro-claro/50'
                          : 'border-linha hover:border-linha-forte',
                      )}
                    >
                      <input
                        type="radio"
                        name="papel"
                        value={p}
                        checked={marcado}
                        onChange={() => field.handleChange(p)}
                        onBlur={field.handleBlur}
                        className="mt-1 size-4 shrink-0 accent-(--tesouro)"
                      />
                      <span className="flex flex-col">
                        <span className="text-corpo font-medium text-tinta">{NOMES_PAPEL[p]}</span>
                        <span className="text-detalhe text-tinta-2">{DESCRICOES_PAPEL[p]}</span>
                      </span>
                    </label>
                  )
                })}
                {erro ? (
                  <p id="erro-papel-convite" className="text-detalhe text-lacre">
                    {erro}
                  </p>
                ) : null}
              </fieldset>
            )
          }}
        </form.Field>
        {limite ? <AvisoLimitePlano orgId={orgId} mensagem={limite} /> : null}
        <ErroFormulario mensagem={erroGeral} />
        <div className="flex justify-end gap-2 pt-1">
          <FecharDialog asChild>
            <Button variante="secundario">Cancelar</Button>
          </FecharDialog>
          <form.AppForm>
            <form.BotaoEnviar>Enviar convite</form.BotaoEnviar>
          </form.AppForm>
        </div>
      </form>
    </Dialog>
  )
}
