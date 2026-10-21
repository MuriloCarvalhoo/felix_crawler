import { useQuery, useQueryClient } from '@tanstack/react-query'
import { MailX, Send } from 'lucide-react'
import { useMemo, useState } from 'react'
import {
  Button,
  ConfirmDialog,
  criarColunas,
  DataTable,
  EmptyState,
  useToast,
  type ColunaTabela,
} from '@/components/ui'
import { api, mensagemDeErro } from '@/lib/api'
import { chaves } from '@/lib/consultas'
import { formatarInstante } from '@/lib/formatos'
import { NOMES_PAPEL } from '@/lib/permissoes'
import type { Convite } from '@/lib/tipos'

const col = criarColunas<Convite>()

/** Convites pendentes (admin+), com revogação. */
export function ConvitesPendentes({ orgId }: { orgId: string }) {
  const qc = useQueryClient()
  const toast = useToast()
  const [revogando, setRevogando] = useState<Convite | null>(null)
  const consulta = useQuery({
    queryKey: chaves.convites(orgId),
    queryFn: ({ signal }) => api.get<Convite[]>(api.rotaOrg(orgId, '/convites'), { signal }),
  })

  const colunas = useMemo<ColunaTabela<Convite>[]>(
    () => [
      col.accessor('email', {
        header: 'E-mail',
        sortFn: 'text',
        cell: (c) => (
          <div className="flex min-w-0 flex-col">
            <span className="break-all text-tinta">{c.row.original.email}</span>
            <span className="text-mini text-tinta-2 sm:hidden">
              Expira em {formatarInstante(c.row.original.expira_em)}
            </span>
          </div>
        ),
      }),
      col.accessor('papel', {
        header: 'Papel',
        enableSorting: false,
        cell: (c) => NOMES_PAPEL[c.row.original.papel],
      }),
      col.accessor('expira_em', {
        header: 'Expira em',
        sortFn: 'basic',
        meta: { ocultarNoCelular: true, classe: 'whitespace-nowrap' },
        cell: (c) => (
          <span className="cifras text-tinta-2">{formatarInstante(c.row.original.expira_em)}</span>
        ),
      }),
      col.display({
        id: 'acoes',
        header: () => <span className="sr-only">Ações</span>,
        meta: { alinhar: 'fim', classe: 'w-28' },
        cell: (c) => (
          <Button
            variante="fantasma"
            tamanho="p"
            icone={<MailX />}
            aria-label={`Revogar convite de ${c.row.original.email}`}
            className="hover:text-lacre"
            onClick={() => setRevogando(c.row.original)}
          >
            <span className="hidden sm:inline">Revogar</span>
          </Button>
        ),
      }),
    ],
    [],
  )

  async function revogar(convite: Convite) {
    try {
      await api.del(api.rotaOrg(orgId, `/convites/${convite.id}`))
    } catch (e) {
      toast.erro('Não foi possível revogar o convite', mensagemDeErro(e))
      throw e
    }
    toast.sucesso('Convite revogado', convite.email)
    void qc.invalidateQueries({ queryKey: chaves.assinatura(orgId) })
    void qc.invalidateQueries({ queryKey: chaves.organizacao(orgId) })
    await qc.invalidateQueries({ queryKey: chaves.convites(orgId) })
  }

  return (
    <>
      <DataTable
        legenda="Convites pendentes"
        colunas={colunas}
        dados={consulta.data}
        carregando={consulta.isFetching}
        erro={consulta.isError ? consulta.error : undefined}
        aoTentarDeNovo={() => void consulta.refetch()}
        linhasEsqueleto={2}
        vazio={
          <EmptyState
            compacto
            icone={<Send />}
            titulo="Nenhum convite pendente"
            descricao="Convites aceitos, revogados ou expirados não aparecem aqui."
          />
        }
      />
      <ConfirmDialog
        aberto={revogando !== null}
        aoMudar={(v) => {
          if (!v) setRevogando(null)
        }}
        titulo="Revogar convite?"
        descricao={
          revogando ? `O link enviado para ${revogando.email} deixa de funcionar.` : undefined
        }
        rotuloConfirmar="Revogar convite"
        perigo
        aoConfirmar={() => (revogando ? revogar(revogando) : undefined)}
      />
    </>
  )
}
