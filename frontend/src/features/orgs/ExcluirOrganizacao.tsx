import { Trash2 } from 'lucide-react'
import { useState } from 'react'
import { Button, ConfirmDialog, Field, Input, useToast } from '@/components/ui'
import { api, ErroApi, mensagemDeErro } from '@/lib/api'

export interface ExcluirOrganizacaoProps {
  orgId: string
  nome: string
  /** Chamado depois do DELETE bem-sucedido (navegar para fora e atualizar a sessão). */
  aoExcluir: () => Promise<void>
}

/** Zona de perigo: exclusão da organização pelo dono, confirmada digitando o nome exato. */
export function ExcluirOrganizacao({ orgId, nome, aoExcluir }: ExcluirOrganizacaoProps) {
  const toast = useToast()
  const [aberto, setAberto] = useState(false)
  const [digitado, setDigitado] = useState('')
  const [erro, setErro] = useState<string | null>(null)
  const confere = digitado.trim() === nome.trim()

  async function excluir() {
    setErro(null)
    try {
      await api.del(api.rotaOrg(orgId), { confirmacao: digitado.trim() })
    } catch (e) {
      const mensagem =
        e instanceof ErroApi && e.campos.confirmacao ? e.campos.confirmacao : mensagemDeErro(e)
      setErro(mensagem)
      throw e
    }
    toast.sucesso('Organização excluída', nome)
    await aoExcluir()
  }

  return (
    <section
      aria-labelledby="titulo-zona-perigo"
      className="rounded-bloco border border-lacre/40 bg-superficie"
    >
      <header className="border-b border-lacre/25 px-4 py-3 sm:px-5">
        <h3 id="titulo-zona-perigo" className="text-corpo font-semibold text-lacre">
          Zona de perigo
        </h3>
      </header>
      <div className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between sm:p-5">
        <div className="flex flex-col gap-1">
          <p className="text-corpo font-semibold text-tinta">Excluir organização</p>
          <p className="max-w-[60ch] text-detalhe text-tinta-2">
            Remove o acesso de todos os membros e revoga os convites pendentes. Os CNPJs, empenhos e
            notas deixam de aparecer para todos.
          </p>
        </div>
        <Button
          variante="perigo"
          icone={<Trash2 />}
          onClick={() => setAberto(true)}
          className="self-start sm:self-auto"
        >
          Excluir organização
        </Button>
      </div>

      <ConfirmDialog
        aberto={aberto}
        aoMudar={(v) => {
          setAberto(v)
          if (!v) {
            setDigitado('')
            setErro(null)
          }
        }}
        titulo="Excluir organização?"
        descricao="Esta ação não pode ser desfeita por aqui."
        rotuloConfirmar="Excluir definitivamente"
        perigo
        confirmarDesabilitado={!confere}
        aoConfirmar={excluir}
      >
        <div className="flex flex-col gap-3">
          <p className="text-detalhe text-tinta-2">
            Para confirmar, digite o nome da organização:{' '}
            <strong className="break-all text-tinta">{nome}</strong>
          </p>
          <Field rotulo="Nome da organização" erro={erro}>
            {(p) => (
              <Input
                {...p}
                autoComplete="off"
                spellCheck={false}
                value={digitado}
                onChange={(e) => {
                  setDigitado(e.target.value)
                  setErro(null)
                }}
              />
            )}
          </Field>
        </div>
      </ConfirmDialog>
    </section>
  )
}
