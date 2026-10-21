import { Link } from '@tanstack/react-router'
import { FilePlus2, Landmark } from 'lucide-react'
import { useMemo, type ReactNode } from 'react'
import {
  BadgeSituacao,
  Card,
  classesBotao,
  criarColunas,
  DataTable,
  EmptyState,
  Stat,
  Valor,
  type ColunaTabela,
} from '@/components/ui'
import { cx } from '@/lib/cx'
import { formatarData, pluralizar } from '@/lib/formatos'
import type { ItemPorUg, Nota, Painel } from '@/lib/tipos'
import { GraficoAging } from './GraficoAging'

const col = criarColunas<ItemPorUg>()
const colunasUg: ColunaTabela<ItemPorUg>[] = col.columns([
  col.accessor('ug', {
    header: 'UG',
    sortFn: 'alphanumeric',
    cell: (c) => (
      <span className="line-clamp-2 min-w-28">
        <span className="cifras">{c.row.original.ug}</span>
        {c.row.original.ug_nome ? <span className="text-tinta-2"> {c.row.original.ug_nome}</span> : null}
      </span>
    ),
  }),
  col.accessor('aberto_centavos', {
    header: 'Em aberto',
    sortFn: 'basic',
    meta: { alinhar: 'fim' },
    cell: (c) => <Valor centavos={c.row.original.aberto_centavos} />,
  }),
  col.accessor('vencido_centavos', {
    header: 'Vencido',
    sortFn: 'basic',
    meta: { alinhar: 'fim' },
    cell: (c) => (
      <Valor centavos={c.row.original.vencido_centavos} tom={c.row.original.vencido_centavos > 0 ? 'lacre' : 'apagado'} />
    ),
  }),
  col.accessor('atraso_medio_dias', {
    header: 'Atraso médio',
    sortFn: 'basic',
    meta: { alinhar: 'fim', ocultarNoCelular: true },
    cell: (c) => {
      const d = Math.round(c.row.original.atraso_medio_dias)
      return d > 0 ? pluralizar(d, 'dia', 'dias') : '—'
    },
  }),
])

function ListaNotas({ orgId, notas, vazio }: { orgId: string; notas: Nota[]; vazio: string }) {
  if (notas.length === 0) return <p className="px-4 py-6 text-center text-detalhe text-tinta-2 sm:px-5">{vazio}</p>
  return (
    <ul className="divide-y divide-linha">
      {notas.map((n) => (
        <li key={n.id} className="flex items-center gap-3 px-4 py-2.5 sm:px-5">
          <div className="flex min-w-0 flex-1 flex-col">
            <Link
              to="/o/$orgId/notas/$notaId"
              params={{ orgId, notaId: n.id }}
              className="cifras truncate font-semibold text-tesouro underline-offset-4 hover:underline"
            >
              NF {n.numero}
            </Link>
            <span className="truncate text-mini text-tinta-2">
              {n.ug_nome ?? `UG ${n.ug}`} · vence {formatarData(n.vencimento)}
            </span>
          </div>
          <div className="flex shrink-0 flex-col items-end gap-1">
            <Valor centavos={n.valor_centavos} />
            <BadgeSituacao status={n.status} vencimento={n.vencimento} diasAtraso={n.dias_atraso} />
          </div>
        </li>
      ))}
    </ul>
  )
}

export function painelVazio(p: Painel): boolean {
  return p.empenhos_ativos === 0 && p.notas_abertas === 0 && p.a_receber_centavos === 0 && p.recebido_mes_centavos === 0
}

export interface PainelVisaoProps {
  orgId: string
  painel: Painel
  podeCriar: boolean
  /** Recarregando em segundo plano: mantém o quadro com opacidade reduzida. */
  atualizando?: boolean
}

/** Conteúdo do painel a partir do `GET /painel`. */
export function PainelVisao({ orgId, painel: p, podeCriar, atualizando }: PainelVisaoProps) {
  const totalAging = useMemo(() => p.aging.reduce((s, i) => s + i.centavos, 0), [p.aging])

  if (painelVazio(p)) {
    return (
      <EmptyState
        icone={<Landmark />}
        titulo="Cadastre o primeiro empenho"
        descricao="Envie os PDFs das notas de empenho do SIAFI ou cadastre manualmente. Depois, lance as notas fiscais para acompanhar o que há a receber e o que vence."
        acao={
          podeCriar ? (
            <>
              <Link to="/o/$orgId/empenhos/novo" params={{ orgId }} className={classesBotao()}>
                <FilePlus2 aria-hidden="true" />
                Importar PDFs de empenho
              </Link>
              <Link
                to="/o/$orgId/empenhos/novo"
                params={{ orgId }}
                search={{ aba: 'manual' }}
                className={classesBotao({ variante: 'secundario' })}
              >
                Cadastrar manualmente
              </Link>
            </>
          ) : (
            <p className="text-detalhe text-tinta-2">Peça a um operador ou administrador para cadastrar os empenhos.</p>
          )
        }
      />
    )
  }

  const notasVencidas: ReactNode =
    p.notas_vencidas > 0 ? (
      <Link
        to="/o/$orgId/notas"
        params={{ orgId }}
        search={{ situacao: 'vencida' }}
        className="underline-offset-4 hover:text-tinta hover:underline"
      >
        {pluralizar(p.notas_vencidas, 'nota vencida', 'notas vencidas')}
      </Link>
    ) : (
      'Nenhuma nota vencida'
    )

  return (
    <div className={cx('flex flex-col gap-5 transition-opacity', atualizando && 'opacity-70')}>
      <section aria-label="Indicadores" className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)_minmax(0,1fr)]">
        <Stat
          rotulo="A receber"
          centavos={p.a_receber_centavos}
          destaque="tesouro"
          detalhe={pluralizar(p.notas_abertas, 'nota aberta', 'notas abertas')}
          className="sm:col-span-2 xl:col-span-1 xl:row-span-2 xl:justify-center"
        />
        <Stat
          rotulo="Vencido"
          centavos={p.vencido_centavos}
          tom={p.vencido_centavos > 0 ? 'lacre' : 'tinta'}
          destaque={p.vencido_centavos > 0 ? 'lacre' : 'nenhum'}
          detalhe={notasVencidas}
        />
        <Stat
          rotulo="Vence em 30 dias"
          centavos={p.vence_30d_centavos}
          destaque={p.vence_30d_centavos > 0 ? 'ambar' : 'nenhum'}
          detalhe="Notas em aberto no prazo"
        />
        <Stat rotulo="Recebido no mês" centavos={p.recebido_mes_centavos} detalhe="Pagamentos registrados" />
        <Stat
          rotulo="Saldo de empenhos"
          centavos={p.saldo_empenhos_centavos}
          detalhe={`${pluralizar(p.empenhos_ativos, 'empenho ativo', 'empenhos ativos')} a faturar`}
        />
      </section>

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-[minmax(0,5fr)_minmax(0,6fr)]">
        <Card titulo="Aging" descricao="Valores em aberto por faixa de atraso">
          {totalAging > 0 ? (
            <GraficoAging aging={p.aging} />
          ) : (
            <p className="py-6 text-center text-detalhe text-tinta-2">Nenhum valor em aberto.</p>
          )}
        </Card>
        <Card titulo="Por UG" descricao="Onde está o dinheiro a receber" semPadding>
          <DataTable
            legenda="Valores em aberto por unidade gestora"
            colunas={colunasUg}
            dados={p.por_ug}
            idLinha={(l) => l.ug}
            ordenacaoInicial={[{ id: 'aberto_centavos', desc: true }]}
            vazio={<p className="px-4 py-6 text-center text-detalhe text-tinta-2">Nenhuma UG com valor em aberto.</p>}
          />
        </Card>
      </div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <Card titulo="Próximos vencimentos" semPadding>
          <ListaNotas orgId={orgId} notas={p.proximos_vencimentos} vazio="Nenhuma nota a vencer." />
        </Card>
        <Card titulo="Mais atrasadas" semPadding>
          <ListaNotas orgId={orgId} notas={p.mais_atrasadas} vazio="Nenhuma nota em atraso." />
        </Card>
      </div>
    </div>
  )
}
