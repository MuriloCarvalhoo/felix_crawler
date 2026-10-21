import {
  createColumnHelper,
  createSortedRowModel,
  metaHelper,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_basic,
  sortFn_datetime,
  sortFn_text,
  tableFeatures,
  useTable,
  type ColumnDef,
  type RowData,
  type SortingState,
} from '@tanstack/react-table'
import { ArrowDown, ArrowUp, ChevronsUpDown } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { cx } from '@/lib/cx'
import { ErrorState } from './EmptyState'

/** Metadados de coluna aceitos pelo `DataTable` (via `meta` na definição da coluna). */
export interface MetaColuna {
  /** `fim` para valores e números (alinha à direita, algarismos tabulares). */
  alinhar?: 'inicio' | 'fim'
  /** Classe extra para `<th>` e `<td>` (ex.: largura `w-32`). */
  classe?: string
  /** Esconde a coluna abaixo de 640px. */
  ocultarNoCelular?: boolean
}

/** Recursos registrados no TanStack Table v9 para todas as tabelas do app. */
export const recursosTabela = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: {
    alphanumeric: sortFn_alphanumeric,
    text: sortFn_text,
    datetime: sortFn_datetime,
    basic: sortFn_basic,
  },
  columnMeta: metaHelper<MetaColuna>(),
})

export type RecursosTabela = typeof recursosTabela
// `any` no valor: colunas heterogêneas (cada accessor tem seu tipo) convivem no mesmo array.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type ColunaTabela<T extends RowData> = ColumnDef<RecursosTabela, T, any>
export type { SortingState as Ordenacao }

/**
 * Ajudante tipado para definir colunas:
 * ```ts
 * const col = criarColunas<EmpenhoResumo>()
 * const colunas = col.columns([
 *   col.accessor('numero', { header: 'Empenho' }),
 *   col.accessor('saldo_centavos', { header: 'Saldo', meta: { alinhar: 'fim' }, cell: (c) => <Valor centavos={c.getValue()} /> }),
 * ])
 * ```
 * Defina as colunas fora do componente (ou em `useMemo`) para não recriar a tabela a cada render.
 */
export function criarColunas<T extends RowData>() {
  return createColumnHelper<RecursosTabela, T>()
}

export interface DataTableProps<T extends RowData> {
  colunas: ColunaTabela<T>[]
  /** Linhas; `undefined` enquanto carrega pela primeira vez. */
  dados: T[] | undefined
  /** Descrição da tabela para leitores de tela (caption). */
  legenda: string
  carregando?: boolean
  /** Erro de carregamento: substitui o corpo da tabela. */
  erro?: unknown
  aoTentarDeNovo?: () => void
  /** Conteúdo quando não há linhas (use `<EmptyState compacto />`). */
  vazio?: ReactNode
  /** Id estável da linha (padrão: campo `id`, se existir). */
  idLinha?: (linha: T, indice: number) => string
  /**
   * `false` desliga a ordenação de todas as colunas (ex.: lista paginada no
   * servidor sem ordenação remota: ordenar só a página atual engana).
   */
  ordenavel?: boolean
  /** Ordenação inicial no cliente. */
  ordenacaoInicial?: SortingState
  /**
   * Ordenação controlada pelo servidor: informe `ordenacao` e `aoOrdenar`;
   * a tabela não reordena localmente (manualSorting).
   */
  ordenacao?: SortingState
  aoOrdenar?: (ordenacao: SortingState) => void
  /** Linhas fantasmas exibidas ao carregar. */
  linhasEsqueleto?: number
  /** Destaca linhas (ex.: vencidas). */
  classeLinha?: (linha: T) => string | undefined
  className?: string
}

const SEM_DADOS: never[] = []

function idPadrao(linha: unknown, indice: number): string {
  if (linha && typeof linha === 'object' && 'id' in linha) {
    const id = linha.id
    if (typeof id === 'string' || typeof id === 'number') return String(id)
  }
  return String(indice)
}

/** Tabela de dados (TanStack Table v9): ordenação, carregando, vazio e erro. */
export function DataTable<T extends RowData>({
  colunas,
  dados,
  legenda,
  carregando = false,
  erro,
  aoTentarDeNovo,
  vazio,
  idLinha = idPadrao,
  ordenavel = true,
  ordenacaoInicial = SEM_DADOS,
  ordenacao,
  aoOrdenar,
  linhasEsqueleto = 5,
  classeLinha,
  className,
}: DataTableProps<T>) {
  const [ordenacaoLocal, setOrdenacaoLocal] = useState<SortingState>(ordenacaoInicial)
  const manual = ordenacao !== undefined && aoOrdenar !== undefined
  const estadoOrdenacao = manual ? ordenacao : ordenacaoLocal

  const tabela = useTable({
    features: recursosTabela,
    columns: colunas,
    data: dados ?? (SEM_DADOS as T[]),
    getRowId: idLinha,
    manualSorting: manual,
    enableSorting: ordenavel,
    enableSortingRemoval: true,
    state: { sorting: estadoOrdenacao },
    onSortingChange: (atualizador) => {
      const proximo = typeof atualizador === 'function' ? atualizador(estadoOrdenacao) : atualizador
      if (manual) aoOrdenar(proximo)
      else setOrdenacaoLocal(proximo)
    },
  })

  const grupos = tabela.getHeaderGroups()
  const linhas = tabela.getRowModel().rows
  const totalColunas = Math.max(1, colunas.length)
  const primeiraCarga = dados === undefined && carregando

  return (
    <div className={cx('relative w-full overflow-x-auto', className)}>
      <table className="w-full border-collapse text-left text-detalhe" aria-busy={carregando || undefined}>
        <caption className="sr-only">{legenda}</caption>
        <thead>
          {grupos.map((grupo) => (
            <tr key={grupo.id} className="border-b border-linha bg-superficie-2">
              {grupo.headers.map((cabecalho) => {
                const meta = cabecalho.column.columnDef.meta
                const podeOrdenar = cabecalho.column.getCanSort()
                const sentido = cabecalho.column.getIsSorted()
                const fim = meta?.alinhar === 'fim'
                return (
                  <th
                    key={cabecalho.id}
                    scope="col"
                    colSpan={cabecalho.colSpan}
                    aria-sort={sentido === 'asc' ? 'ascending' : sentido === 'desc' ? 'descending' : undefined}
                    className={cx(
                      'h-10 px-3 font-semibold whitespace-nowrap text-tinta-2 first:pl-4 last:pr-4',
                      fim ? 'text-right' : 'text-left',
                      meta?.ocultarNoCelular && 'hidden sm:table-cell',
                      meta?.classe,
                    )}
                  >
                    {cabecalho.isPlaceholder ? null : podeOrdenar ? (
                      <button
                        type="button"
                        onClick={cabecalho.column.getToggleSortingHandler()}
                        className={cx(
                          'group inline-flex items-center gap-1 rounded-[3px] hover:text-tinta',
                          fim && 'flex-row-reverse',
                          sentido && 'text-tinta',
                        )}
                      >
                        <tabela.FlexRender header={cabecalho} />
                        {sentido === 'asc' ? (
                          <ArrowUp className="size-3.5" aria-hidden="true" />
                        ) : sentido === 'desc' ? (
                          <ArrowDown className="size-3.5" aria-hidden="true" />
                        ) : (
                          <ChevronsUpDown className="size-3.5 opacity-40 group-hover:opacity-80" aria-hidden="true" />
                        )}
                      </button>
                    ) : (
                      <tabela.FlexRender header={cabecalho} />
                    )}
                  </th>
                )
              })}
            </tr>
          ))}
        </thead>
        <tbody>
          {erro ? (
            <tr>
              <td colSpan={totalColunas}>
                <ErrorState erro={erro} aoTentarDeNovo={aoTentarDeNovo} compacto />
              </td>
            </tr>
          ) : primeiraCarga ? (
            Array.from({ length: linhasEsqueleto }, (_, i) => (
              <tr key={`esqueleto-${String(i)}`} className="border-b border-linha last:border-b-0">
                {Array.from({ length: totalColunas }, (_, j) => (
                  <td key={j} className="h-11 px-3 first:pl-4 last:pr-4">
                    <span aria-hidden="true" className="esqueleto block h-3.5 w-full max-w-32 rounded-[3px] bg-superficie-3" />
                  </td>
                ))}
              </tr>
            ))
          ) : linhas.length === 0 ? (
            <tr>
              <td colSpan={totalColunas}>{vazio ?? <p className="px-4 py-8 text-center text-tinta-2">Nenhum registro.</p>}</td>
            </tr>
          ) : (
            linhas.map((linha) => (
              <tr
                key={linha.id}
                className={cx(
                  'border-b border-linha transition-colors last:border-b-0 hover:bg-superficie-2/60',
                  carregando && 'opacity-60',
                  classeLinha?.(linha.original),
                )}
              >
                {linha.getAllCells().map((celula) => {
                  const meta = celula.column.columnDef.meta
                  return (
                    <td
                      key={celula.id}
                      className={cx(
                        'h-11 px-3 py-2 align-middle text-tinta first:pl-4 last:pr-4',
                        meta?.alinhar === 'fim' && 'cifras text-right whitespace-nowrap',
                        meta?.ocultarNoCelular && 'hidden sm:table-cell',
                        meta?.classe,
                      )}
                    >
                      <tabela.FlexRender cell={celula} />
                    </td>
                  )
                })}
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  )
}
