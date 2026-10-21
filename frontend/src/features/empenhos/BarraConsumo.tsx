import { Valor } from '@/components/ui'
import { formatarBRL } from '@/lib/formatos'
import type { Centavos } from '@/lib/tipos'

export interface BarraConsumoProps {
  valor: Centavos
  faturado: Centavos
  recebido: Centavos
}

/** Percentual com uma casa, sem passar de 100. */
function pct(parte: Centavos, total: Centavos): number {
  if (total <= 0) return 0
  return Math.min(100, Math.max(0, (parte / total) * 100))
}

function textoPct(p: number): string {
  return `${(Math.round(p * 10) / 10).toString().replace('.', ',')}%`
}

// Rampa ordinal de um só matiz (validada na skill dataviz): recebido escuro, faturado médio, saldo = trilho claro.
const COR_RECEBIDO = 'bg-tesouro'
const COR_FATURADO = 'bg-[color-mix(in_srgb,var(--tesouro)_50%,var(--superficie))]'
const COR_TRILHO = 'bg-tesouro-claro'

/**
 * Consumo do empenho: recebido + faturado a receber + saldo, sobre o valor total.
 * Valores sempre visíveis na legenda (a barra não é o único canal).
 */
export function BarraConsumo({ valor, faturado, recebido }: BarraConsumoProps) {
  const recebidoSeguro = Math.min(recebido, faturado)
  const aReceber = Math.max(0, faturado - recebidoSeguro)
  const saldo = Math.max(0, valor - faturado)
  const pRecebido = pct(recebidoSeguro, valor)
  const pAReceber = pct(aReceber, valor)
  const pFaturado = pct(faturado, valor)
  const descricao = `Faturado ${textoPct(pFaturado)} do empenho: ${formatarBRL(recebidoSeguro)} recebido, ${formatarBRL(aReceber)} a receber e ${formatarBRL(saldo)} de saldo.`

  const itens = [
    { rotulo: 'Recebido', centavos: recebidoSeguro, cor: COR_RECEBIDO },
    { rotulo: 'Faturado a receber', centavos: aReceber, cor: COR_FATURADO },
    { rotulo: 'Saldo a faturar', centavos: saldo, cor: COR_TRILHO },
  ]

  return (
    <figure className="flex flex-col gap-3">
      <div
        role="img"
        aria-label={descricao}
        className={`flex h-3 w-full gap-[2px] overflow-hidden rounded-[4px] ${COR_TRILHO}`}
      >
        {pRecebido > 0 ? (
          <span className={`h-full rounded-l-[4px] ${COR_RECEBIDO}`} style={{ width: `${String(pRecebido)}%` }} title={`Recebido: ${formatarBRL(recebidoSeguro)}`} />
        ) : null}
        {pAReceber > 0 ? (
          <span
            className={`h-full ${pRecebido > 0 ? '' : 'rounded-l-[4px]'} ${COR_FATURADO}`}
            style={{ width: `${String(pAReceber)}%` }}
            title={`Faturado a receber: ${formatarBRL(aReceber)}`}
          />
        ) : null}
      </div>
      <figcaption>
        <dl className="grid grid-cols-1 gap-x-6 gap-y-2 sm:grid-cols-3">
          {itens.map((i) => (
            <div key={i.rotulo} className="flex items-center justify-between gap-3 sm:flex-col sm:items-start sm:gap-0.5">
              <dt className="flex items-center gap-2 text-detalhe text-tinta-2">
                <span aria-hidden="true" className={`size-2.5 rounded-[2px] ${i.cor} ${i.cor === COR_TRILHO ? 'ring-1 ring-linha-forte' : ''}`} />
                {i.rotulo}
              </dt>
              <dd>
                <Valor centavos={i.centavos} />
              </dd>
            </div>
          ))}
        </dl>
        <p className="mt-2 text-mini text-tinta-3">{textoPct(pFaturado)} do valor do empenho já foi faturado.</p>
      </figcaption>
    </figure>
  )
}
