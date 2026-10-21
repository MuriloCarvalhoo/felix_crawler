import { queryOptions, useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { Plus } from 'lucide-react'
import { classesBotao, ErrorState, PageHeader, Skeleton, Stat } from '@/components/ui'
import { api } from '@/lib/api'
import { chaves, type OrgAtual } from '@/lib/consultas'
import type { Painel } from '@/lib/tipos'
import { PainelVisao } from './PainelVisao'

export function opcoesPainel(org: string) {
  return queryOptions({
    queryKey: chaves.painel(org),
    queryFn: ({ signal }) => api.get<Painel>(api.rotaOrg(org, '/painel'), { signal }),
  })
}

function PainelCarregando() {
  return (
    <div className="flex flex-col gap-5" aria-busy="true">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)_minmax(0,1fr)]">
        {['A receber', 'Vencido', 'Vence em 30 dias', 'Recebido no mês', 'Saldo de empenhos'].map((r) => (
          <Stat key={r} rotulo={r} carregando />
        ))}
      </div>
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <Skeleton className="h-64 rounded-bloco" />
        <Skeleton className="h-64 rounded-bloco" />
      </div>
      <span className="sr-only" role="status">
        Carregando painel…
      </span>
    </div>
  )
}

/** Painel da organização: indicadores, aging, por UG e listas de vencimento. */
export function PaginaPainel({ org }: { org: OrgAtual }) {
  const consulta = useQuery(opcoesPainel(org.id))
  const podeCriar = org.pode('criar_empenho')
  return (
    <>
      <PageHeader
        titulo="Painel"
        descricao="Quanto há a receber, o que está vencido e o que vence nos próximos dias."
        acoes={
          podeCriar && consulta.data ? (
            <Link to="/o/$orgId/empenhos/novo" params={{ orgId: org.id }} className={classesBotao({ variante: 'secundario' })}>
              <Plus aria-hidden="true" />
              Novo empenho
            </Link>
          ) : null
        }
      />
      {consulta.isPending ? (
        <PainelCarregando />
      ) : consulta.isError ? (
        <ErrorState erro={consulta.error} aoTentarDeNovo={() => void consulta.refetch()} />
      ) : (
        <PainelVisao orgId={org.id} painel={consulta.data} podeCriar={podeCriar} atualizando={consulta.isRefetching} />
      )}
    </>
  )
}
