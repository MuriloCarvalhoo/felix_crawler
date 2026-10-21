import { useQuery } from '@tanstack/react-query'
import { ReceiptText, Search } from 'lucide-react'
import { useMemo } from 'react'
import { z } from 'zod'
import {
  classesBotao,
  DataTable,
  EmptyState,
  Input,
  PageHeader,
  Pagination,
  ROTULOS_STATUS_NOTA,
  Select,
} from '@/components/ui'
import { api } from '@/lib/api'
import type { OrgAtual } from '@/lib/consultas'
import { cx } from '@/lib/cx'
import { textoDaUrl } from '@/lib/navegacao'
import { useCorrigirPagina, type AtualizacaoBusca } from '@/lib/paginacao'
import type { FiltrosNotas, StatusNota } from '@/lib/tipos'
import { BotaoBaixar } from '../empenhos/BotaoBaixar'
import { useTextoBusca } from '../empenhos/comum'
import { classeLinhaNota, colunasNotas } from './colunas'
import { opcoesNotas } from './servicos'

export const POR_PAGINA_NOTAS = 25
const STATUS: StatusNota[] = ['emitida', 'liquidada', 'paga', 'cancelada']

export const esquemaBuscaNotas = z.object({
  busca: textoDaUrl.catch('').default(''),
  status: z.array(z.enum(['emitida', 'liquidada', 'paga', 'cancelada'])).catch([]).default([]),
  situacao: z.enum(['', 'vencida', 'a_vencer', 'vence_7d']).catch('').default(''),
  ordenar: z.enum(['vencimento', 'criado_em']).catch('vencimento').default('vencimento'),
  pagina: z.coerce.number().int().min(1).catch(1).default(1),
})

export type BuscaNotas = z.output<typeof esquemaBuscaNotas>

export function filtrosDaBuscaNotas(b: BuscaNotas): FiltrosNotas {
  return {
    busca: b.busca || undefined,
    status: b.status.length > 0 ? b.status : undefined,
    situacao: b.situacao || undefined,
    ordenar: b.ordenar,
    pagina: b.pagina,
    por_pagina: POR_PAGINA_NOTAS,
  }
}

const OPCOES_SITUACAO = [
  { valor: 'vencida', rotulo: 'Vencidas' },
  { valor: 'vence_7d', rotulo: 'Vencem em 7 dias' },
  { valor: 'a_vencer', rotulo: 'A vencer' },
]

const OPCOES_ORDEM = [
  { valor: 'vencimento', rotulo: 'Vencimento' },
  { valor: 'criado_em', rotulo: 'Mais recentes' },
]

export interface PaginaNotasProps {
  org: OrgAtual
  busca: BuscaNotas
  /** Recebe a nova busca ou um atualizador sobre a busca mais recente da URL. */
  aoMudarBusca: (busca: AtualizacaoBusca<BuscaNotas>) => void
}

/** Lista de notas fiscais com filtros de status e situação, ordenação, paginação e exportação. */
export function PaginaNotas({ org, busca, aoMudarBusca }: PaginaNotasProps) {
  const consulta = useQuery(opcoesNotas(org.id, filtrosDaBuscaNotas(busca)))
  const colunas = useMemo(() => colunasNotas(org.id), [org.id])
  const [texto, setTexto, propsFoco] = useTextoBusca(busca.busca, (t) => {
    aoMudarBusca((anterior) => (t === anterior.busca ? anterior : { ...anterior, busca: t, pagina: 1 }))
  })
  useCorrigirPagina(consulta.isPlaceholderData ? undefined : consulta.data, busca.pagina, (pagina) => {
    aoMudarBusca((anterior) => ({ ...anterior, pagina }))
  })
  const comFiltro = busca.busca !== '' || busca.status.length > 0 || busca.situacao !== ''

  function alternarStatus(s: StatusNota) {
    const status = busca.status.includes(s) ? busca.status.filter((x) => x !== s) : STATUS.filter((x) => x === s || busca.status.includes(x))
    aoMudarBusca({ ...busca, status, pagina: 1 })
  }

  return (
    <>
      <PageHeader
        titulo="Notas fiscais"
        descricao="Notas emitidas contra empenhos, com vencimento e situação de pagamento."
        acoes={<BotaoBaixar caminho={api.rotaOrg(org.id, '/exportar/notas.csv')}>Exportar CSV</BotaoBaixar>}
      />
      <div role="search" aria-label="Filtrar notas" className="mb-4 flex flex-col gap-3">
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex min-w-0 flex-[1_1_16rem] flex-col gap-1.5">
            <span className="text-detalhe font-semibold text-tinta">Buscar</span>
            <Input
              type="search"
              value={texto}
              {...propsFoco}
              onChange={(e) => setTexto(e.target.value)}
              placeholder="Número da nota, empenho ou UG"
              prefixo={<Search />}
            />
          </label>
          <label className="flex min-w-0 flex-[1_1_10rem] flex-col gap-1.5">
            <span className="text-detalhe font-semibold text-tinta">Situação</span>
            <Select
              value={busca.situacao}
              vazia="Todas"
              opcoes={OPCOES_SITUACAO}
              onChange={(e) =>
                aoMudarBusca({ ...busca, situacao: esquemaBuscaNotas.shape.situacao.parse(e.target.value), pagina: 1 })
              }
            />
          </label>
          <label className="flex min-w-0 flex-[0_1_10rem] flex-col gap-1.5">
            <span className="text-detalhe font-semibold text-tinta">Ordenar por</span>
            <Select
              value={busca.ordenar}
              opcoes={OPCOES_ORDEM}
              onChange={(e) =>
                aoMudarBusca({ ...busca, ordenar: esquemaBuscaNotas.shape.ordenar.parse(e.target.value), pagina: 1 })
              }
            />
          </label>
        </div>
        <fieldset className="flex flex-wrap items-center gap-2">
          <legend className="mb-1.5 text-detalhe font-semibold text-tinta">Status</legend>
          {STATUS.map((s) => {
            const marcado = busca.status.includes(s)
            return (
              <label
                key={s}
                className={cx(
                  'inline-flex h-8 cursor-pointer items-center gap-2 rounded-full border px-3 text-detalhe transition-colors focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-foco',
                  marcado ? 'border-tesouro bg-tesouro-claro text-tinta' : 'border-linha-forte bg-superficie text-tinta-2 hover:text-tinta',
                )}
              >
                <input type="checkbox" className="size-3.5 accent-(--tesouro)" checked={marcado} onChange={() => alternarStatus(s)} />
                {ROTULOS_STATUS_NOTA[s]}
              </label>
            )
          })}
        </fieldset>
      </div>
      <div className="overflow-hidden rounded-bloco border border-linha bg-superficie">
        <DataTable
          legenda="Notas fiscais"
          colunas={colunas}
          dados={consulta.data?.dados}
          carregando={consulta.isFetching}
          erro={consulta.error}
          aoTentarDeNovo={() => void consulta.refetch()}
          classeLinha={classeLinhaNota}
          vazio={
            comFiltro ? (
              <EmptyState
                compacto
                titulo="Nenhuma nota encontrada"
                descricao="Nenhuma nota corresponde aos filtros escolhidos."
                acao={
                  <button
                    type="button"
                    className={classesBotao({ variante: 'secundario', tamanho: 'p' })}
                    onClick={() => aoMudarBusca(esquemaBuscaNotas.parse({}))}
                  >
                    Limpar filtros
                  </button>
                }
              />
            ) : (
              <EmptyState
                compacto
                icone={<ReceiptText />}
                titulo="Nenhuma nota lançada"
                descricao="As notas são lançadas no detalhe de cada empenho, pelo botão “Lançar nota”."
              />
            )
          }
        />
      </div>
      {consulta.data && consulta.data.meta.total > 0 ? (
        <Pagination
          className="mt-4"
          pagina={busca.pagina}
          porPagina={consulta.data.meta.por_pagina || POR_PAGINA_NOTAS}
          total={consulta.data.meta.total}
          aoMudarPagina={(pagina) => aoMudarBusca({ ...busca, pagina })}
        />
      ) : null}
    </>
  )
}
