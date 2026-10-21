import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { Landmark, Plus, Search } from 'lucide-react'
import { useId, useMemo } from 'react'
import { z } from 'zod'
import {
  Badge,
  BadgeStatusEmpenho,
  Checkbox,
  classesBotao,
  criarColunas,
  DataTable,
  EmptyState,
  Input,
  PageHeader,
  Pagination,
  Select,
  Valor,
  type ColunaTabela,
} from '@/components/ui'
import { api } from '@/lib/api'
import { opcoesEmpresas, type OrgAtual } from '@/lib/consultas'
import { formatarCNPJ } from '@/lib/formatos'
import { textoDaUrl } from '@/lib/navegacao'
import { useCorrigirPagina, type AtualizacaoBusca } from '@/lib/paginacao'
import type { EmpenhoResumo, Empresa, FiltrosEmpenhos } from '@/lib/tipos'
import { BotaoBaixar } from './BotaoBaixar'
import { NOMES_ORIGEM, useTextoBusca } from './comum'
import { opcoesEmpenhos } from './servicos'

export const POR_PAGINA = 25

export const esquemaBuscaEmpenhos = z.object({
  busca: textoDaUrl.catch('').default(''),
  empresa: textoDaUrl.catch('').default(''),
  status: z.enum(['ativo', 'anulado', 'todos']).catch('ativo').default('ativo'),
  saldo: z.boolean().catch(false).default(false),
  pagina: z.coerce.number().int().min(1).catch(1).default(1),
})

export type BuscaEmpenhos = z.output<typeof esquemaBuscaEmpenhos>

export function filtrosDaBusca(b: BuscaEmpenhos): FiltrosEmpenhos {
  return {
    busca: b.busca || undefined,
    empresa_id: b.empresa || undefined,
    status: b.status,
    com_saldo: b.saldo ? true : undefined,
    pagina: b.pagina,
    por_pagina: POR_PAGINA,
  }
}

const col = criarColunas<EmpenhoResumo>()

function criarColunasEmpenhos(orgId: string, empresas: Map<string, Empresa>): ColunaTabela<EmpenhoResumo>[] {
  return col.columns([
    col.accessor('numero', {
      header: 'Empenho',
      enableSorting: false,
      cell: (c) => (
        <Link
          to="/o/$orgId/empenhos/$empenhoId"
          params={{ orgId, empenhoId: c.row.original.id }}
          className="cifras font-semibold text-tesouro underline-offset-4 hover:underline"
        >
          {c.row.original.numero}
        </Link>
      ),
    }),
    col.accessor('ug', {
      header: 'UG',
      enableSorting: false,
      cell: (c) => (
        <span className="line-clamp-2 min-w-36">
          <span className="cifras">{c.row.original.ug}</span>
          {c.row.original.ug_nome ? <span className="text-tinta-2"> {c.row.original.ug_nome}</span> : null}
        </span>
      ),
    }),
    col.accessor('orgao', {
      header: 'Órgão',
      enableSorting: false,
      meta: { ocultarNoCelular: true },
      cell: (c) => <span className="line-clamp-2 min-w-32 text-tinta-2">{c.row.original.orgao ?? '—'}</span>,
    }),
    col.accessor('empresa_cnpj', {
      header: 'Empresa',
      enableSorting: false,
      meta: { ocultarNoCelular: true },
      cell: (c) => {
        const empresa = empresas.get(c.row.original.empresa_id)
        return (
          <span className="flex min-w-36 flex-col">
            {empresa ? <span className="truncate">{empresa.razao_social}</span> : null}
            <span className="cifras text-mini text-tinta-2">{formatarCNPJ(c.row.original.empresa_cnpj)}</span>
          </span>
        )
      },
    }),
    col.accessor('valor_total_centavos', {
      enableSorting: false,
      header: 'Valor',
      meta: { alinhar: 'fim' },
      cell: (c) => <Valor centavos={c.row.original.valor_total_centavos} />,
    }),
    col.accessor('faturado_centavos', {
      enableSorting: false,
      header: 'Faturado',
      meta: { alinhar: 'fim', ocultarNoCelular: true },
      cell: (c) => <Valor centavos={c.row.original.faturado_centavos} />,
    }),
    col.accessor('saldo_centavos', {
      enableSorting: false,
      header: 'Saldo',
      meta: { alinhar: 'fim' },
      cell: (c) => <Valor centavos={c.row.original.saldo_centavos} tom={c.row.original.saldo_centavos > 0 ? 'tinta' : 'apagado'} />,
    }),
    col.accessor('status', {
      header: 'Status',
      enableSorting: false,
      cell: (c) => <BadgeStatusEmpenho status={c.row.original.status} />,
    }),
    col.accessor('origem', {
      header: 'Origem',
      enableSorting: false,
      meta: { ocultarNoCelular: true },
      cell: (c) => (
        <Badge tom="neutro" ponto={false}>
          {NOMES_ORIGEM[c.row.original.origem]}
        </Badge>
      ),
    }),
  ])
}

export interface PaginaEmpenhosProps {
  org: OrgAtual
  busca: BuscaEmpenhos
  /** Recebe a nova busca ou um atualizador sobre a busca mais recente da URL. */
  aoMudarBusca: (busca: AtualizacaoBusca<BuscaEmpenhos>) => void
}

const OPCOES_STATUS = [
  { valor: 'ativo', rotulo: 'Ativos' },
  { valor: 'anulado', rotulo: 'Anulados' },
  { valor: 'todos', rotulo: 'Todos' },
]

/** Lista de empenhos com busca, filtros, paginação e exportação. */
export function PaginaEmpenhos({ org, busca, aoMudarBusca }: PaginaEmpenhosProps) {
  const id = useId()
  const empresas = useQuery(opcoesEmpresas(org.id))
  const consulta = useQuery(opcoesEmpenhos(org.id, filtrosDaBusca(busca)))
  const [texto, setTexto, propsFoco] = useTextoBusca(busca.busca, (t) => {
    aoMudarBusca((anterior) => (t === anterior.busca ? anterior : { ...anterior, busca: t, pagina: 1 }))
  })
  useCorrigirPagina(consulta.isPlaceholderData ? undefined : consulta.data, busca.pagina, (pagina) => {
    aoMudarBusca((anterior) => ({ ...anterior, pagina }))
  })

  const mapaEmpresas = useMemo(() => new Map((empresas.data ?? []).map((e) => [e.id, e])), [empresas.data])
  const colunas = useMemo(() => criarColunasEmpenhos(org.id, mapaEmpresas), [org.id, mapaEmpresas])

  const comFiltro = busca.busca !== '' || busca.empresa !== '' || busca.status !== 'ativo' || busca.saldo
  const podeCriar = org.pode('criar_empenho')
  const linkNovo = (
    <Link to="/o/$orgId/empenhos/novo" params={{ orgId: org.id }} className={classesBotao()}>
      <Plus aria-hidden="true" />
      Novo empenho
    </Link>
  )

  return (
    <>
      <PageHeader
        titulo="Empenhos"
        descricao="Notas de empenho cadastradas, com saldo a faturar e valores recebidos."
        acoes={
          <>
            <BotaoBaixar caminho={api.rotaOrg(org.id, '/exportar/empenhos.csv')}>Exportar CSV</BotaoBaixar>
            {podeCriar ? linkNovo : null}
          </>
        }
      />
      <div role="search" aria-label="Filtrar empenhos" className="mb-4 flex flex-wrap items-end gap-3">
        <label className="flex min-w-0 flex-[1_1_16rem] flex-col gap-1.5">
          <span className="text-detalhe font-semibold text-tinta">Buscar</span>
          <Input
            type="search"
            value={texto}
            {...propsFoco}
            onChange={(e) => setTexto(e.target.value)}
            placeholder="Número, UG, órgão ou modalidade"
            prefixo={<Search />}
          />
        </label>
        <label className="flex min-w-0 flex-[1_1_12rem] flex-col gap-1.5">
          <span className="text-detalhe font-semibold text-tinta">Empresa</span>
          <Select
            value={busca.empresa}
            vazia="Todas as empresas"
            opcoes={(empresas.data ?? []).map((e) => ({ valor: e.id, rotulo: e.razao_social }))}
            onChange={(e) => aoMudarBusca({ ...busca, empresa: e.target.value, pagina: 1 })}
          />
        </label>
        <label className="flex min-w-0 flex-[0_1_9rem] flex-col gap-1.5">
          <span className="text-detalhe font-semibold text-tinta">Status</span>
          <Select
            value={busca.status}
            opcoes={OPCOES_STATUS}
            onChange={(e) => {
              const status = esquemaBuscaEmpenhos.shape.status.parse(e.target.value)
              aoMudarBusca({ ...busca, status, pagina: 1 })
            }}
          />
        </label>
        <Checkbox
          id={`${id}-saldo`}
          rotulo="Só com saldo"
          className="h-10 items-center"
          checked={busca.saldo}
          onChange={(e) => aoMudarBusca({ ...busca, saldo: e.target.checked, pagina: 1 })}
        />
      </div>
      <div className="overflow-hidden rounded-bloco border border-linha bg-superficie">
        <DataTable
          legenda="Empenhos"
          colunas={colunas}
          dados={consulta.data?.dados}
          carregando={consulta.isFetching}
          erro={consulta.error}
          aoTentarDeNovo={() => void consulta.refetch()}
          vazio={
            comFiltro ? (
              <EmptyState
                compacto
                titulo="Nenhum empenho encontrado"
                descricao="Nenhum empenho corresponde aos filtros escolhidos."
                acao={
                  <button
                    type="button"
                    className={classesBotao({ variante: 'secundario', tamanho: 'p' })}
                    onClick={() => aoMudarBusca(esquemaBuscaEmpenhos.parse({}))}
                  >
                    Limpar filtros
                  </button>
                }
              />
            ) : (
              <EmptyState
                compacto
                icone={<Landmark />}
                titulo="Nenhum empenho cadastrado"
                descricao="Envie os PDFs do SIAFI ou cadastre o primeiro empenho manualmente."
                acao={podeCriar ? linkNovo : undefined}
              />
            )
          }
        />
      </div>
      {consulta.data && consulta.data.meta.total > 0 ? (
        <Pagination
          className="mt-4"
          pagina={busca.pagina}
          porPagina={consulta.data.meta.por_pagina || POR_PAGINA}
          total={consulta.data.meta.total}
          aoMudarPagina={(pagina) => aoMudarBusca({ ...busca, pagina })}
        />
      ) : null}
    </>
  )
}
