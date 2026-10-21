import { Link } from '@tanstack/react-router'
import { BadgeSituacao, BadgeStatusNota, criarColunas, Valor, type ColunaTabela } from '@/components/ui'
import { formatarData } from '@/lib/formatos'
import type { Nota } from '@/lib/tipos'
import { NOMES_TIPO_NOTA } from '../empenhos/comum'

const col = criarColunas<Nota>()

export interface OpcoesColunasNotas {
  /** Esconde as colunas do empenho (já estamos no detalhe dele). */
  semEmpenho?: boolean
}

/** Colunas da tabela de notas (lista, detalhe do empenho). Sem ordenação local: as listas são paginadas no servidor. */
export function colunasNotas(orgId: string, { semEmpenho = false }: OpcoesColunasNotas = {}): ColunaTabela<Nota>[] {
  const colunas: ColunaTabela<Nota>[] = [
    col.accessor('numero', {
      header: 'NF',
      enableSorting: false,
      cell: (c) => (
        <Link
          to="/o/$orgId/notas/$notaId"
          params={{ orgId, notaId: c.row.original.id }}
          className="cifras font-semibold text-tesouro underline-offset-4 hover:underline"
        >
          {c.row.original.numero}
        </Link>
      ),
    }),
  ]
  if (!semEmpenho) {
    colunas.push(
      col.accessor('empenho_numero', {
        header: 'Empenho',
        enableSorting: false,
        cell: (c) => (
          <Link
            to="/o/$orgId/empenhos/$empenhoId"
            params={{ orgId, empenhoId: c.row.original.empenho_id }}
            className="cifras text-tinta underline-offset-4 hover:underline"
          >
            {c.row.original.empenho_numero}
          </Link>
        ),
      }),
      col.accessor('ug', {
        header: 'UG',
        enableSorting: false,
        meta: { ocultarNoCelular: true },
        cell: (c) => (
          <span className="line-clamp-2 min-w-32">
            <span className="cifras">{c.row.original.ug}</span>
            {c.row.original.ug_nome ? <span className="text-tinta-2"> {c.row.original.ug_nome}</span> : null}
          </span>
        ),
      }),
    )
  } else {
    colunas.push(
      col.accessor('tipo', {
        header: 'Tipo',
        enableSorting: false,
        meta: { ocultarNoCelular: true },
        cell: (c) => NOMES_TIPO_NOTA[c.row.original.tipo],
      }),
    )
  }
  colunas.push(
    col.accessor('valor_centavos', {
      header: 'Valor',
      enableSorting: false,
      meta: { alinhar: 'fim' },
      cell: (c) => <Valor centavos={c.row.original.valor_centavos} />,
    }),
    col.accessor('data_emissao', {
      header: 'Emissão',
      enableSorting: false,
      meta: { ocultarNoCelular: true, classe: 'cifras whitespace-nowrap' },
      cell: (c) => formatarData(c.row.original.data_emissao),
    }),
    col.accessor('vencimento', {
      header: 'Vencimento',
      enableSorting: false,
      meta: { classe: 'cifras whitespace-nowrap' },
      cell: (c) => formatarData(c.row.original.vencimento),
    }),
    col.display({
      id: 'situacao',
      header: 'Situação',
      cell: (c) => {
        const n = c.row.original
        return <BadgeSituacao status={n.status} vencimento={n.vencimento} diasAtraso={n.dias_atraso} />
      },
    }),
    col.accessor('status', {
      header: 'Status',
      enableSorting: false,
      cell: (c) => <BadgeStatusNota status={c.row.original.status} />,
    }),
  )
  return colunas
}

/** Destaca a linha de nota vencida. */
export function classeLinhaNota(n: Nota): string | undefined {
  return n.dias_atraso > 0 && (n.status === 'emitida' || n.status === 'liquidada') ? 'bg-lacre-claro/35' : undefined
}
