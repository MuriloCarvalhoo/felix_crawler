import { cx } from '@/lib/cx'
import { formatarBRL, formatarBRLCompacto, formatarNumero, pluralizar } from '@/lib/formatos'
import type { FaixaAging, ItemAging } from '@/lib/tipos'

export const FAIXAS: readonly FaixaAging[] = ['a_vencer', '1_30', '31_60', '61_90', '90_mais']

export const ROTULOS_FAIXA: Record<FaixaAging, string> = {
  a_vencer: 'A vencer',
  '1_30': '1 a 30 dias',
  '31_60': '31 a 60 dias',
  '61_90': '61 a 90 dias',
  '90_mais': 'Mais de 90 dias',
}

/**
 * Cores por faixa (skill dataviz): "a vencer" em cinza neutro (ainda não é problema);
 * as faixas vencidas formam uma rampa ordinal de um só matiz (lacre), clara → escura
 * conforme o atraso cresce. Rampa validada em claro e escuro (ΔL ≥ 0,06; extremo
 * claro ≥ 2:1 sobre a superfície) por validate_palette.js --ordinal.
 */
const COR_FAIXA: Record<FaixaAging, string> = {
  a_vencer: 'bg-tinta-3',
  '1_30': 'bg-[color-mix(in_srgb,var(--lacre)_45%,var(--superficie))]',
  '31_60': 'bg-[color-mix(in_srgb,var(--lacre)_63%,var(--superficie))]',
  '61_90': 'bg-[color-mix(in_srgb,var(--lacre)_81%,var(--superficie))]',
  '90_mais': 'bg-lacre',
}

/** Garante as 5 faixas na ordem, com zero nas ausentes. */
export function normalizarAging(itens: readonly ItemAging[]): ItemAging[] {
  return FAIXAS.map((faixa) => itens.find((i) => i.faixa === faixa) ?? { faixa, centavos: 0, quantidade: 0 })
}

function textoPct(parte: number, total: number): string {
  if (total <= 0) return '0%'
  const p = (parte / total) * 100
  if (p > 0 && p < 1) return '<1%'
  return `${String(Math.round(p))}%`
}

export interface GraficoAgingProps {
  aging: readonly ItemAging[]
  className?: string
}

/**
 * Aging do contas a receber em barras horizontais. É uma tabela de verdade
 * (faixa, valor, notas), então a "visão em tabela" já é o próprio gráfico:
 * valor escrito na ponta de cada barra; detalhes no foco/hover.
 */
export function GraficoAging({ aging, className }: GraficoAgingProps) {
  const linhas = normalizarAging(aging)
  const maximo = Math.max(0, ...linhas.map((l) => l.centavos))
  const total = linhas.reduce((s, l) => s + l.centavos, 0)

  return (
    <table className={cx('w-full border-collapse text-detalhe', className)}>
      <caption className="sr-only">Valores em aberto por faixa de atraso</caption>
      <thead>
        <tr className="text-mini text-tinta-3">
          <th scope="col" className="pb-1 text-left font-medium">
            Atraso
          </th>
          <th scope="col" className="pb-1 text-left font-medium">
            Em aberto
          </th>
          <th scope="col" className="pb-1 text-right font-medium">
            Notas
          </th>
        </tr>
      </thead>
      <tbody>
        {linhas.map((l) => {
          const fracao = maximo > 0 ? l.centavos / maximo : 0
          const resumo = `${ROTULOS_FAIXA[l.faixa]}: ${formatarBRL(l.centavos)}, ${pluralizar(l.quantidade, 'nota', 'notas')}, ${textoPct(l.centavos, total)} do total em aberto`
          return (
            <tr key={l.faixa} className="group">
              <th scope="row" className="w-28 py-1.5 pr-3 text-left font-medium whitespace-nowrap text-tinta-2 sm:w-32">
                {ROTULOS_FAIXA[l.faixa]}
              </th>
              <td className="py-1.5">
                {/* Barra é imagem com descrição completa; não recebe foco (os números já estão na tabela). */}
                <div role="img" className="relative flex h-7 items-center gap-2 rounded-[4px]" aria-label={resumo}>
                  <span
                    aria-hidden="true"
                    className={cx(
                      'block h-5 shrink-0 rounded-r-[4px] transition-[filter] group-hover:brightness-110',
                      l.centavos > 0 ? COR_FAIXA[l.faixa] : 'bg-linha',
                    )}
                    style={{ width: l.centavos > 0 ? `max(2px, calc((100% - 7.5rem) * ${String(fracao)}))` : '2px' }}
                  />
                  <span aria-hidden="true" className="cifras font-semibold whitespace-nowrap text-tinta">
                    {formatarBRLCompacto(l.centavos)}
                  </span>
                  <span
                    aria-hidden="true"
                    className="pointer-events-none absolute bottom-full left-0 z-10 mb-1 hidden min-w-44 rounded-campo border border-linha bg-superficie px-3 py-2 shadow-elevado group-hover:block"
                  >
                    <span className="cifras block text-corpo font-semibold text-tinta">{formatarBRL(l.centavos)}</span>
                    <span className="block text-mini text-tinta-2">
                      {ROTULOS_FAIXA[l.faixa]} · {pluralizar(l.quantidade, 'nota', 'notas')} · {textoPct(l.centavos, total)}
                    </span>
                  </span>
                </div>
              </td>
              <td className="cifras w-16 py-1.5 pl-2 text-right whitespace-nowrap text-tinta-2">
                {formatarNumero(l.quantidade)}
                <span className="sr-only"> {l.quantidade === 1 ? 'nota' : 'notas'}</span>
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}
