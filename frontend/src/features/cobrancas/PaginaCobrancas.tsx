import { useQuery } from '@tanstack/react-query'
import { useCorrigirPagina } from '@/lib/paginacao'
import { Link } from '@tanstack/react-router'
import { FileText } from 'lucide-react'
import { useMemo } from 'react'
import { z } from 'zod'
import { criarColunas, DataTable, EmptyState, PageHeader, Pagination, Valor, type ColunaTabela } from '@/components/ui'
import type { OrgAtual } from '@/lib/consultas'
import { formatarAtraso, formatarData } from '@/lib/formatos'
import type { CobrancaResumo } from '@/lib/tipos'
import { BotaoBaixar } from '../empenhos/BotaoBaixar'
import { NOMES_CANAL } from '../empenhos/comum'
import { caminhoOficioGuardado, opcoesCobrancas } from './servicos'

export const POR_PAGINA_COBRANCAS = 25

export const esquemaBuscaCobrancas = z.object({
  pagina: z.coerce.number().int().min(1).catch(1).default(1),
})

export type BuscaCobrancas = z.output<typeof esquemaBuscaCobrancas>

const col = criarColunas<CobrancaResumo>()

function criarColunasCobrancas(orgId: string): ColunaTabela<CobrancaResumo>[] {
  return col.columns([
    col.accessor('enviada_em', {
      header: 'Envio',
      enableSorting: false,
      meta: { classe: 'cifras whitespace-nowrap' },
      cell: (c) => formatarData(c.row.original.enviada_em),
    }),
    col.accessor('nota_numero', {
      header: 'NF',
      enableSorting: false,
      cell: (c) => (
        <Link
          to="/o/$orgId/notas/$notaId"
          params={{ orgId, notaId: c.row.original.nota_id }}
          className="cifras font-semibold text-tesouro underline-offset-4 hover:underline"
        >
          {c.row.original.nota_numero}
        </Link>
      ),
    }),
    col.accessor('empenho_numero', {
      header: 'Empenho',
      enableSorting: false,
      meta: { ocultarNoCelular: true, classe: 'cifras' },
    }),
    col.accessor('ug', { header: 'UG', enableSorting: false, meta: { ocultarNoCelular: true, classe: 'cifras' } }),
    col.accessor('canal', {
      header: 'Canal',
      enableSorting: false,
      cell: (c) => (
        <span className="flex flex-col">
          <span>{NOMES_CANAL[c.row.original.canal]}</span>
          {c.row.original.protocolo ? <span className="text-mini text-tinta-2">{c.row.original.protocolo}</span> : null}
        </span>
      ),
    }),
    col.accessor('total_centavos', {
      header: 'Total',
      enableSorting: false,
      meta: { alinhar: 'fim' },
      cell: (c) => <Valor centavos={c.row.original.total_centavos} />,
    }),
    col.accessor('dias_atraso', {
      header: 'Atraso',
      enableSorting: false,
      meta: { ocultarNoCelular: true, classe: 'whitespace-nowrap' },
      cell: (c) => formatarAtraso(c.row.original.dias_atraso),
    }),
    col.accessor('criado_por_nome', {
      header: 'Autor',
      enableSorting: false,
      meta: { ocultarNoCelular: true },
      cell: (c) => c.row.original.criado_por_nome ?? '—',
    }),
    col.display({
      id: 'oficio',
      header: () => <span className="sr-only">Ofício</span>,
      meta: { alinhar: 'fim' },
      cell: (c) => (
        <BotaoBaixar
          caminho={caminhoOficioGuardado(orgId, c.row.original.id)}
          tamanho="p"
          variante="fantasma"
          icone={<FileText />}
          aria-label={`Baixar ofício da NF ${c.row.original.nota_numero}`}
        >
          <span className="hidden sm:inline">Ofício</span>
        </BotaoBaixar>
      ),
    }),
  ])
}

export interface PaginaCobrancasProps {
  org: OrgAtual
  busca: BuscaCobrancas
  aoMudarBusca: (busca: BuscaCobrancas) => void
}

/** Histórico de cobranças enviadas, com o ofício guardado de cada uma. */
export function PaginaCobrancas({ org, busca, aoMudarBusca }: PaginaCobrancasProps) {
  const consulta = useQuery(opcoesCobrancas(org.id, { pagina: busca.pagina, por_pagina: POR_PAGINA_COBRANCAS }))
  useCorrigirPagina(consulta.isPlaceholderData ? undefined : consulta.data, busca.pagina, (pagina) => aoMudarBusca({ pagina }), POR_PAGINA_COBRANCAS)
  const colunas = useMemo(() => criarColunasCobrancas(org.id), [org.id])
  return (
    <>
      <PageHeader titulo="Cobranças" descricao="Ofícios de cobrança enviados e seus valores com multa e juros." />
      <div className="overflow-hidden rounded-bloco border border-linha bg-superficie">
        <DataTable
          legenda="Cobranças enviadas"
          colunas={colunas}
          dados={consulta.data?.dados}
          carregando={consulta.isFetching}
          erro={consulta.error}
          aoTentarDeNovo={() => void consulta.refetch()}
          vazio={
            <EmptyState
              compacto
              icone={<FileText />}
              titulo="Nenhuma cobrança registrada"
              descricao={
                <>
                  Para cobrar uma nota vencida, abra a nota em{' '}
                  <Link
                    to="/o/$orgId/notas"
                    params={{ orgId: org.id }}
                    search={{ situacao: 'vencida' }}
                    className="font-semibold text-tesouro underline-offset-4 hover:underline"
                  >
                    Notas vencidas
                  </Link>
                  , calcule os encargos e registre o envio do ofício.
                </>
              }
            />
          }
        />
      </div>
      {consulta.data && consulta.data.meta.total > 0 ? (
        <Pagination
          className="mt-4"
          pagina={busca.pagina}
          porPagina={consulta.data.meta.por_pagina || POR_PAGINA_COBRANCAS}
          total={consulta.data.meta.total}
          aoMudarPagina={(pagina) => aoMudarBusca({ pagina })}
        />
      ) : null}
    </>
  )
}
